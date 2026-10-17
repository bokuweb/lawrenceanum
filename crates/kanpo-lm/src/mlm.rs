//! ModernBERT（穴埋めのモデル、[sbintuitions/modernbert-ja-30m] など）の擬似対数尤度で採点する [`Scorer`]。
//!
//! 各トークンを `<mask>` に置き換えて、元のトークンの確率を前後の両側の文脈から見る。1 つずつ隠すと文の
//! トークン数だけモデルを流すので、[`MlmScorer::stride`] トークンおきの位置をまとめて隠す（`stride` 回で文の全部の
//! トークンを採点する）。字を入れ替えた行と元の行の比較では、16 おきで 1 つずつ隠すのとほぼ同じ精度（語の移動 99.0%・
//! 断片の混入 98.0%・行の入れ替え 94.5%）、4 おきで少し落ち（97.5%・98.0%・93.5%）、1（全部を一度に隠す）では大きく
//! 落ちる。candle で CPU で動く（WASM にも載る）。
//!
//! [sbintuitions/modernbert-ja-30m]: https://huggingface.co/sbintuitions/modernbert-ja-30m

use crate::{Scorer, Token};
use candle_core::{DType, Device, Tensor, D};
use candle_nn::{layer_norm_no_bias, linear_no_bias, LayerNorm, Linear, Module, VarBuilder};
use candle_transformers::models::modernbert::{Config, ModernBert};
use std::path::Path;

/// ModernBERT のモデル（`config.json`・`model.safetensors`・`tokenizer.json` のあるディレクトリから読む）。
#[derive(Clone)]
pub struct MlmScorer {
    model: ModernBert,
    dense: Linear,
    norm: LayerNorm,
    decoder: Linear,
    tokenizer: tokenizers::Tokenizer,
    hidden: usize,
    mask_id: u32,
    /// まとめて隠す位置の間隔（文のトークン数以上なら 1 つずつ隠す。広いほど正確だが遅い）。
    pub stride: usize,
    /// 並べて流すスレッドの数。
    pub threads: usize,
}

impl MlmScorer {
    pub fn open(dir: &Path) -> anyhow::Result<Self> {
        let device = Device::Cpu;
        let config: Config = serde_json::from_str(&std::fs::read_to_string(dir.join("config.json"))?)?;
        let vb = unsafe { VarBuilder::from_mmaped_safetensors(&[dir.join("model.safetensors")], DType::F32, &device)? };
        let model = ModernBert::load(vb.clone(), &config)?;
        // 穴埋めの頭と出力層（重みは入力の埋め込みと共有）は、隠した位置にだけ掛ける（全位置に語彙 10 万語の
        // 出力を出すと遅い）。
        let dense = linear_no_bias(config.hidden_size, config.hidden_size, vb.pp("head.dense"))?;
        let norm = layer_norm_no_bias(config.hidden_size, config.layer_norm_eps, vb.pp("head.norm"))?;
        let decoder = Linear::new(
            vb.get((config.vocab_size, config.hidden_size), "model.embeddings.tok_embeddings.weight")?,
            Some(vb.get(config.vocab_size, "decoder.bias")?),
        );
        let tokenizer = tokenizers::Tokenizer::from_file(dir.join("tokenizer.json")).map_err(anyhow::Error::msg)?;
        let mask_id = tokenizer.token_to_id("<mask>").ok_or_else(|| anyhow::anyhow!("<mask> が語彙に無い"))?;
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
        Ok(MlmScorer { model, dense, norm, decoder, tokenizer, hidden: config.hidden_size, mask_id, stride: 16, threads })
    }

    fn score_one(&self, text: &str) -> anyhow::Result<Vec<Token>> {
        let enc = self.tokenizer.encode(text, true).map_err(anyhow::Error::msg)?;
        let ids = enc.get_ids();
        let offsets = enc.get_offsets();
        let specials = enc.get_special_tokens_mask();
        let n = ids.len();
        let targets: Vec<usize> = (0..n).filter(|&i| specials[i] == 0).collect();
        let mut nll = vec![0f32; n];
        let stride = self.stride.max(1);
        // 間隔 stride で隠す組。1 つの組を 1 本の列にして、組をまとめて流す（1 回に 16 本まで）。
        let groups: Vec<Vec<usize>> =
            (0..stride).map(|r| targets.iter().enumerate().filter(|(k, _)| k % stride == r).map(|(_, &i)| i).collect()).filter(|g: &Vec<usize>| !g.is_empty()).collect();
        let device = Device::Cpu;
        for batch in groups.chunks(16) {
            let mut input = Vec::with_capacity(batch.len() * n);
            for g in batch {
                input.extend(ids.iter().enumerate().map(|(i, &t)| if g.contains(&i) { self.mask_id } else { t }));
            }
            let xs = Tensor::from_vec(input, (batch.len(), n), &device)?;
            let mask = Tensor::ones((batch.len(), n), DType::U32, &device)?;
            let hidden = self.model.forward(&xs, &mask)?.reshape((batch.len() * n, self.hidden))?;
            let rows: Vec<u32> = batch.iter().enumerate().flat_map(|(b, g)| g.iter().map(move |&i| (b * n + i) as u32)).collect();
            let picked = hidden.index_select(&Tensor::new(rows.as_slice(), &device)?, 0)?;
            let logits = self.decoder.forward(&self.norm.forward(&self.dense.forward(&picked)?.gelu_erf()?)?)?;
            let logp = candle_nn::ops::log_softmax(&logits, D::Minus1)?;
            let gather = Tensor::new(batch.iter().flatten().map(|&i| ids[i]).collect::<Vec<u32>>().as_slice(), &device)?;
            let picked: Vec<f32> = logp.gather(&gather.unsqueeze(1)?, 1)?.squeeze(1)?.to_vec1()?;
            for (&i, v) in batch.iter().flatten().zip(picked) {
                nll[i] = -v;
            }
        }
        Ok(targets.iter().map(|&i| Token { start: offsets[i].0, end: offsets[i].1, nll: nll[i] }).collect())
    }
}

impl Scorer for MlmScorer {
    fn score(&mut self, texts: &[&str]) -> anyhow::Result<Vec<Vec<Token>>> {
        let threads = self.threads.clamp(1, texts.len().max(1));
        let mut out: Vec<Option<anyhow::Result<Vec<Token>>>> = (0..texts.len()).map(|_| None).collect();
        let this = &*self;
        std::thread::scope(|s| {
            let slots: Vec<_> = out.chunks_mut(texts.len().div_ceil(threads).max(1)).collect();
            let mut start = 0;
            for slot in slots {
                let (from, len) = (start, slot.len());
                start += len;
                s.spawn(move || {
                    for (k, o) in slot.iter_mut().enumerate() {
                        *o = Some(this.score_one(texts[from + k]));
                    }
                });
            }
        });
        out.into_iter().map(|o| o.unwrap_or_else(|| Ok(Vec::new()))).collect()
    }
}
