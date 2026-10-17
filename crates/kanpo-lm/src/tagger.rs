//! 読み順の崩れの切れ目を当てる分類器（ModernBERT-Ja-30m を `train/` で fine-tune したもの）。
//!
//! トークンごとに「直前から本文の流れが途切れているか」の確率を出す。正しい本文（e-Gov の法令の本文と官報の本文）に
//! パーサの失敗に似た崩し（縦組みの列の逆順の連結・列の入れ替え・断片の混入・語の移動・行の前後の入れ替え・縦中横の
//! 単位のずれ・上付きの字の分離）を入れて学習した。擬似対数尤度（[`crate::mlm`]）と違って 1 回流すだけで済む。

use candle_core::{DType, Device, Tensor, D};
use candle_nn::{layer_norm_no_bias, linear, linear_no_bias, LayerNorm, Linear, Module, VarBuilder};
use candle_transformers::models::modernbert::{Config, ModernBert};
use std::path::Path;

/// トークン 1 つの切れ目の確率。`start..end` は文の中のバイトの範囲。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Break {
    pub start: usize,
    pub end: usize,
    pub prob: f32,
}

/// 切れ目の分類器（`config.json`・`model.safetensors`・`tokenizer.json` のあるディレクトリから読む）。
#[derive(Clone)]
pub struct BreakTagger {
    model: ModernBert,
    dense: Linear,
    norm: LayerNorm,
    classifier: Linear,
    tokenizer: tokenizers::Tokenizer,
    /// 1 回に流す最長のトークン数（学習は 512）。
    pub max_tokens: usize,
}

impl BreakTagger {
    pub fn open(dir: &Path) -> anyhow::Result<Self> {
        let device = Device::Cpu;
        let config: Config = serde_json::from_str(&std::fs::read_to_string(dir.join("config.json"))?)?;
        let vb = load_weights(&dir.join("model.safetensors"), &device)?;
        let model = ModernBert::load(vb.clone(), &config)?;
        let dense = linear_no_bias(config.hidden_size, config.hidden_size, vb.pp("head.dense"))?;
        let norm = layer_norm_no_bias(config.hidden_size, config.layer_norm_eps, vb.pp("head.norm"))?;
        let classifier = linear(config.hidden_size, 2, vb.pp("classifier"))?;
        let tokenizer = tokenizers::Tokenizer::from_file(dir.join("tokenizer.json")).map_err(anyhow::Error::msg)?;
        Ok(BreakTagger { model, dense, norm, classifier, tokenizer, max_tokens: 512 })
    }

    /// 文ごとに、トークン（特別なトークンと空白だけのトークンを除く）の切れ目の確率を返す。
    pub fn tag(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<Break>>> {
        let device = Device::Cpu;
        let mut out = Vec::with_capacity(texts.len());
        for text in texts {
            let enc = self.tokenizer.encode(*text, true).map_err(anyhow::Error::msg)?;
            let n = enc.get_ids().len().min(self.max_tokens);
            let ids = &enc.get_ids()[..n];
            let xs = Tensor::new(ids, &device)?.unsqueeze(0)?;
            let mask = Tensor::ones((1, n), DType::U32, &device)?;
            let hidden = self.model.forward(&xs, &mask)?.squeeze(0)?;
            let logits = self.classifier.forward(&self.norm.forward(&self.dense.forward(&hidden)?.gelu_erf()?)?)?;
            let probs: Vec<f32> = candle_nn::ops::softmax(&logits, D::Minus1)?.narrow(1, 1, 1)?.squeeze(1)?.to_vec1()?;
            let offsets = enc.get_offsets();
            let specials = enc.get_special_tokens_mask();
            let mut breaks = Vec::new();
            for i in 0..n {
                let (s, e) = offsets[i];
                if specials[i] == 1 || s == e || text[s..e].chars().all(|c| c.is_whitespace()) {
                    continue;
                }
                breaks.push(Break { start: s, end: e, prob: probs[i] });
            }
            out.push(breaks);
        }
        Ok(out)
    }
}

/// 重みを読む。行ごとの int8 にした重み（`train/quantize.py`。`X` が int8 に 128 を足した U8、`X.__scale` が行ごとの倍率）は
/// fp32 に戻す（計算は fp32 のまま）。fp32 のままの重みもそのまま読める。
fn load_weights(path: &Path, device: &Device) -> anyhow::Result<VarBuilder<'static>> {
    let mut tensors = candle_core::safetensors::load(path, device)?;
    let scaled: Vec<String> = tensors.keys().filter_map(|k| k.strip_suffix(".__scale").map(str::to_string)).collect();
    for name in scaled {
        let scale = tensors.remove(&format!("{name}.__scale")).ok_or_else(|| anyhow::anyhow!("{name}.__scale"))?;
        let q = tensors.get(&name).ok_or_else(|| anyhow::anyhow!("{name}"))?;
        let w = (q.to_dtype(DType::F32)? - 128.0)?.broadcast_mul(&scale.unsqueeze(1)?)?;
        tensors.insert(name, w);
    }
    Ok(VarBuilder::from_tensors(tensors, DType::F32, device))
}

/// 行の切れ目の見立て。
#[derive(Debug, Clone, PartialEq)]
pub struct LineBreaks {
    pub line: usize,
    /// 行の中でいちばん高い切れ目の確率。
    pub max_prob: f32,
    /// 切れ目と見たトークン（確率が `threshold` 以上）の、行の中のバイトの位置と確率。
    pub breaks: Vec<(usize, f32)>,
}

/// 行をまとまり（`chunk_chars` 字ほど。前後の行が文脈になる）にして切れ目を当て、行ごとにまとめる。行頭の切れ目は、
/// 前の行の終わりから続かないこと（列の入れ替え・断片の行）を表す。
pub fn find_breaks(tagger: &BreakTagger, lines: &[&str], chunk_chars: usize, threshold: f32) -> anyhow::Result<Vec<LineBreaks>> {
    let mut chunks: Vec<(std::ops::Range<usize>, String, Vec<usize>)> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let start = i;
        let (mut text, mut offsets, mut chars) = (String::new(), Vec::new(), 0);
        while i < lines.len() && (i == start || chars + lines[i].chars().count() <= chunk_chars) {
            if i > start {
                text.push('\n');
            }
            offsets.push(text.len());
            text.push_str(lines[i]);
            chars += lines[i].chars().count();
            i += 1;
        }
        chunks.push((start..i, text, offsets));
    }
    let texts: Vec<&str> = chunks.iter().map(|c| c.1.as_str()).collect();
    let tagged = tagger.tag(&texts)?;
    let mut out = Vec::with_capacity(lines.len());
    for ((range, _, offsets), breaks) in chunks.iter().zip(tagged) {
        for (k, line) in range.clone().enumerate() {
            let (from, to) = (offsets[k], offsets[k] + lines[line].len());
            let mine: Vec<&Break> = breaks.iter().filter(|b| b.start >= from && b.start < to).collect();
            out.push(LineBreaks {
                line,
                max_prob: mine.iter().map(|b| b.prob).fold(0.0, f32::max),
                breaks: mine.iter().filter(|b| b.prob >= threshold).map(|b| (b.start - from, b.prob)).collect(),
            });
        }
    }
    Ok(out)
}
