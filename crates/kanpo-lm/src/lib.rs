//! 官報の本文（kanpo-parser の出力）を、言語モデルで後から見直す後工程。
//!
//! パーサの中には組み込まず、パーサが組んだ本文の行だけを受け取る。
//!
//! - [`scan`] は、前後の行を文脈にして各行を採点し（1 字あたりの負の対数尤度）、読み順の崩れの候補を上から並べる
//! - [`correct`] は、怪しい行について、字を入れ替えた書き直しの候補（欄の空白で区切った片の並べ替え、縦中横の
//!   単位の置き直し）を作り、元の行より十分に自然なものだけを補正として返す
//!
//! 採点するモデルは [`Scorer`] で差し替える（`llama` 機能: llama.cpp で Gemma などの GGUF）。
//! 言語モデルは「どちらの並びが自然か」を比べるのは得意だが（字を入れ替えた行と元の行の比較で 96〜99%）、
//! 1 行だけを見て誤りかを決めるのは苦手（表・一覧・行の途中で切れた縦書きの列も読みにくい）なので、
//! [`scan`] の結果は人が見る順番を決めるためのもの、[`correct`] は書き直しの候補どうしの比較だけに使う。

mod candidates;
#[cfg(feature = "llama")]
pub mod llama;

pub use candidates::{candidates, Candidate};

/// トークン 1 つの採点。`start..end` は採点した文の中のバイトの範囲。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Token {
    pub start: usize,
    pub end: usize,
    /// 前のトークンまでを見たときの、このトークンの負の対数尤度（nat）。
    pub nll: f32,
}

/// 文を採点するモデル。`texts` の各文について、先頭から順にトークンの採点を返す。
pub trait Scorer {
    fn score(&mut self, texts: &[&str]) -> anyhow::Result<Vec<Vec<Token>>>;
}

/// 採点の設定。
#[derive(Debug, Clone)]
pub struct Options {
    /// [`scan`] で 1 回に採点する行のまとまりの字数（前の行を文脈にする）。
    pub chunk_chars: usize,
    /// [`correct`] で候補の前後に置く文脈の行数。
    pub context_lines: usize,
    /// [`correct`] で見直す行（[`scan`] の 1 字あたりの負の対数尤度がこれ以上）。
    pub suspect_nll: f32,
    /// [`correct`] で見直す行の最短の字数。
    pub min_chars: usize,
    /// 書き直しを採る、文脈を含めた負の対数尤度の改善の下限（nat）。
    pub min_gain: f32,
}

impl Default for Options {
    fn default() -> Self {
        Options { chunk_chars: 600, context_lines: 2, suspect_nll: 2.3, min_chars: 8, min_gain: 8.0 }
    }
}

/// 行の採点（前の行を文脈にしたもの）。
#[derive(Debug, Clone, PartialEq)]
pub struct LineScore {
    pub line: usize,
    pub chars: usize,
    /// 行の字に掛かるトークンの負の対数尤度の和。
    pub nll: f32,
    /// いちばん確率の低いトークンの字（行の中）。
    pub worst: String,
}

impl LineScore {
    /// 1 字あたりの負の対数尤度。
    pub fn per_char(&self) -> f32 {
        self.nll / self.chars.max(1) as f32
    }
}

/// 行ごとの採点。行を [`Options::chunk_chars`] 字ほどのまとまりにして、まとまりの中の前の行を文脈に採点する
/// （1 行ずつ単独で採点すると、縦書きの列の途中で切れた行の頭が軒並み読みにくく見える）。
pub fn scan(scorer: &mut dyn Scorer, lines: &[&str], opts: &Options) -> anyhow::Result<Vec<LineScore>> {
    // まとまり: (行の範囲, つないだ文, 各行の開始バイト)
    let mut chunks: Vec<(std::ops::Range<usize>, String, Vec<usize>)> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let start = i;
        let mut text = String::new();
        let mut offsets = Vec::new();
        let mut chars = 0;
        while i < lines.len() && (i == start || chars + lines[i].chars().count() <= opts.chunk_chars) {
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
    let scored = scorer.score(&texts)?;
    let mut out = Vec::with_capacity(lines.len());
    for ((range, text, offsets), tokens) in chunks.iter().zip(scored) {
        for (k, line) in range.clone().enumerate() {
            let (from, to) = (offsets[k], offsets[k] + lines[line].len());
            // 行の字に掛かるトークン（行頭の前の改行と行の字にまたがるトークンも、この行のもの）。
            let mine: Vec<&Token> = tokens.iter().filter(|t| t.end > from && t.start < to).collect();
            let nll = mine.iter().map(|t| t.nll).sum();
            let worst = mine
                .iter()
                .max_by(|a, b| a.nll.total_cmp(&b.nll))
                .map(|t| text.get(t.start.max(from)..t.end.min(to)).unwrap_or("").to_string())
                .unwrap_or_default();
            out.push(LineScore { line, chars: lines[line].chars().count(), nll, worst });
        }
    }
    Ok(out)
}

/// 採った書き直し。
#[derive(Debug, Clone, PartialEq)]
pub struct Correction {
    pub line: usize,
    pub before: String,
    pub after: String,
    /// 書き直しの種類（[`Candidate::rule`]）。
    pub rule: &'static str,
    /// 文脈を含めた負の対数尤度の改善（nat）。
    pub gain: f32,
}

/// 怪しい行（[`scan`] で [`Options::suspect_nll`] 以上）の書き直しの候補を前後の行を文脈にして採点し、元の行より
/// [`Options::min_gain`] 以上自然な候補があれば、いちばん自然なものに書き直す。書き直した行と補正の一覧を返す。
pub fn correct(scorer: &mut dyn Scorer, lines: &[&str], opts: &Options) -> anyhow::Result<(Vec<String>, Vec<Correction>)> {
    let scores = scan(scorer, lines, opts)?;
    let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
    let mut corrections = Vec::new();
    for s in scores.iter().filter(|s| s.chars >= opts.min_chars && s.per_char() >= opts.suspect_nll) {
        let line = s.line;
        let cands = candidates(lines[line]);
        if cands.is_empty() {
            continue;
        }
        let before = lines[line.saturating_sub(opts.context_lines)..line].join("\n");
        let after = lines[(line + 1).min(lines.len())..(line + 1 + opts.context_lines).min(lines.len())].join("\n");
        let wrap = |t: &str| {
            let mut s = before.clone();
            if !s.is_empty() {
                s.push('\n');
            }
            s.push_str(t);
            if !after.is_empty() {
                s.push('\n');
                s.push_str(&after);
            }
            s
        };
        let mut texts = vec![wrap(lines[line])];
        texts.extend(cands.iter().map(|c| wrap(&c.text)));
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        let totals: Vec<f32> = scorer.score(&refs)?.iter().map(|t| t.iter().map(|t| t.nll).sum()).collect();
        let Some((best, total)) = totals[1..].iter().enumerate().min_by(|a, b| a.1.total_cmp(b.1)) else { continue };
        let gain = totals[0] - total;
        if gain >= opts.min_gain {
            out[line] = cands[best].text.clone();
            corrections.push(Correction {
                line,
                before: lines[line].to_string(),
                after: cands[best].text.clone(),
                rule: cands[best].rule,
                gain,
            });
        }
    }
    Ok((out, corrections))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 字の並びを数える採点（「正しい」文と、そこからずれた字の数で負の対数尤度を決める）。
    struct Toy {
        good: Vec<String>,
    }

    impl Scorer for Toy {
        fn score(&mut self, texts: &[&str]) -> anyhow::Result<Vec<Vec<Token>>> {
            Ok(texts
                .iter()
                .map(|t| {
                    // 正しい文の中に、前の字との 2 字の並びが無い字は 10 nat、ある字は 1 nat。
                    let mut prev: Option<char> = None;
                    t.char_indices()
                        .map(|(i, c)| {
                            let pair: String = prev.into_iter().chain([c]).collect();
                            prev = Some(c);
                            let known = self.good.iter().any(|g| g.contains(&pair));
                            Token { start: i, end: i + c.len_utf8(), nll: if known { 1.0 } else { 10.0 } }
                        })
                        .collect()
                })
                .collect())
        }
    }

    #[test]
    fn scan_attributes_tokens_to_lines() {
        let mut toy = Toy { good: vec!["ああああ".into()] };
        let lines = ["ああああ", "いいい"];
        let s = scan(&mut toy, &lines, &Options::default()).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].chars, 4);
        // 2 行目は改行と「い」の並びがどれも知らないもの。
        assert!(s[1].per_char() > s[0].per_char());
    }

    #[test]
    fn correct_moves_unit_back() {
        let good = "ロ　四三三・六七MHzを超え四三四・一七MHz以下の周波数の電波";
        let mut toy = Toy { good: vec![good.into(), "前の行\n".into()] };
        let bad = "ロ　四三三・六七　を超え四三四・一七MHz　以下の周波数の電波MHz";
        let lines = ["前の行", bad];
        let opts = Options { suspect_nll: 0.0, min_gain: 1.0, ..Options::default() };
        let (out, fixes) = correct(&mut toy, &lines, &opts).unwrap();
        assert_eq!(fixes.len(), 1, "{fixes:?}");
        assert_eq!(out[1], good);
    }

    #[test]
    fn correct_keeps_good_line() {
        let good = "代表取締役　田口　貴之";
        let mut toy = Toy { good: vec![good.into()] };
        let opts = Options { suspect_nll: 0.0, min_gain: 1.0, ..Options::default() };
        let (out, fixes) = correct(&mut toy, &[good], &opts).unwrap();
        assert!(fixes.is_empty());
        assert_eq!(out[0], good);
    }
}
