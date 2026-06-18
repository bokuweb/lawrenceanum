//! 改め文の構造化表現。`reconstruct_vertical` が出すマーカー付きテキストを、
//! HTML や表など任意の形式に変換しやすい構造化データへ整える。
//!
//! - `Document` — 1 件の改め文。形式（prose/shinkyu）とブロック列。
//! - `Block` — 段落（散文）か、新旧対照表（改正後/改正前）。
//! - `Run` — 連続するテキスト片。傍線（下線）の有無を持つ。
//!
//! 傍線は現状の `pdftotext`/`pdftocairo` 出力からは復元できないため、いまは常に
//! `false`（型としてのみ用意し、将来の傍線抽出で充填できるようにする）。

use serde::Serialize;

use crate::format::detect_format;

/// 改め文 1 件の構造化表現。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Document {
    /// "prose" | "shinkyu" | "unknown"。
    pub format: String,
    /// 文書を構成するブロック列（出現順）。
    pub blocks: Vec<Block>,
}

/// 文書ブロック。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Block {
    /// 散文（前文・散文改め文など）。
    Paragraph { runs: Vec<Run> },
    /// 新旧対照表の 1 段（改正後 / 改正前）。
    Shinkyu { after: Vec<Run>, before: Vec<Run> },
}

/// 同一スタイルが続くテキスト片。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Run {
    pub text: String,
    /// 傍線（下線）が付いているか。現状は常に false（将来の傍線抽出用の予約フィールド）。
    #[serde(default, skip_serializing_if = "is_false")]
    pub underline: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl Run {
    /// 傍線無しのプレーンな Run を作る。
    pub fn plain(text: impl Into<String>) -> Self {
        Run {
            text: text.into(),
            underline: false,
        }
    }
}

/// テキストの 1 セル（複数行）を 1 つの Run に畳む。空なら空 Vec。
/// 改行は列折り返し由来なので維持する（呼び出し側で整形可能）。
fn cell_runs(lines: &[&str]) -> Vec<Run> {
    let text = lines.join("\n").trim().to_string();
    if text.is_empty() {
        Vec::new()
    } else {
        vec![Run::plain(text)]
    }
}

impl Document {
    /// `reconstruct_vertical` 由来のマーカー付きテキストを構造化する。
    ///
    /// 「改正後」「改正前」の独立見出し行を境に新旧対照表ブロックを作り、
    /// それ以外の連続行は段落ブロックにする。形式は [`detect_format`] で判定する。
    pub fn from_text(text: &str) -> Document {
        let format = detect_format(text);
        let lines: Vec<&str> = text.lines().collect();
        let mut blocks: Vec<Block> = Vec::new();
        let mut para: Vec<&str> = Vec::new();
        let flush_para = |para: &mut Vec<&str>, blocks: &mut Vec<Block>| {
            let runs = cell_runs(para);
            if !runs.is_empty() {
                blocks.push(Block::Paragraph { runs });
            }
            para.clear();
        };
        let mut i = 0;
        while i < lines.len() {
            if lines[i].trim() == "改正後" {
                flush_para(&mut para, &mut blocks);
                // 改正後セル: 次の「改正前」まで。
                let after_start = i + 1;
                let mut j = after_start;
                while j < lines.len() && lines[j].trim() != "改正前" {
                    j += 1;
                }
                let after = cell_runs(&lines[after_start..j]);
                // 改正前セル: 次の「改正後」まで（次段の開始）。
                let before_start = (j + 1).min(lines.len());
                let mut k = before_start;
                while k < lines.len() && lines[k].trim() != "改正後" {
                    k += 1;
                }
                let before = cell_runs(&lines[before_start..k]);
                blocks.push(Block::Shinkyu { after, before });
                i = k;
            } else {
                para.push(lines[i]);
                i += 1;
            }
        }
        flush_para(&mut para, &mut blocks);
        Document { format, blocks }
    }

    /// 構造化表現を `reconstruct_vertical` 互換のフラットテキストへ戻す。
    /// 記事分割・標題突合など既存のテキスト前提処理との後方互換に使う。
    pub fn to_text(&self) -> String {
        let mut out: Vec<String> = Vec::new();
        let runs_text = |runs: &[Run]| -> String { runs.iter().map(|r| r.text.as_str()).collect::<Vec<_>>().join("") };
        for block in &self.blocks {
            match block {
                Block::Paragraph { runs } => {
                    let t = runs_text(runs);
                    if !t.is_empty() {
                        out.push(t);
                    }
                }
                Block::Shinkyu { after, before } => {
                    out.push("改正後".to_string());
                    out.push(runs_text(after));
                    out.push("改正前".to_string());
                    out.push(runs_text(before));
                }
            }
        }
        out.join("\n").trim().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structures_prose_as_single_paragraph() {
        let doc = Document::from_text("第一条中「甲」を「乙」に改める。");
        assert_eq!(doc.format, "prose");
        assert_eq!(
            doc.blocks,
            vec![Block::Paragraph {
                runs: vec![Run::plain("第一条中「甲」を「乙」に改める。")]
            }]
        );
    }

    #[test]
    fn structures_shinkyu_into_after_before() {
        let text = "前文\n改正後\n甲種\n改正前\n乙類";
        let doc = Document::from_text(text);
        assert_eq!(doc.format, "shinkyu");
        assert_eq!(
            doc.blocks,
            vec![
                Block::Paragraph {
                    runs: vec![Run::plain("前文")]
                },
                Block::Shinkyu {
                    after: vec![Run::plain("甲種")],
                    before: vec![Run::plain("乙類")],
                },
            ]
        );
    }

    #[test]
    fn handles_multiple_shinkyu_segments() {
        // 多ページ表: 改正後/改正前 が複数段。
        let text = "改正後\nA1\n改正前\nB1\n改正後\nA2\n改正前\nB2";
        let doc = Document::from_text(text);
        let shinkyu: Vec<_> = doc
            .blocks
            .iter()
            .filter(|b| matches!(b, Block::Shinkyu { .. }))
            .collect();
        assert_eq!(shinkyu.len(), 2);
        assert_eq!(
            doc.blocks[1],
            Block::Shinkyu {
                after: vec![Run::plain("A2")],
                before: vec![Run::plain("B2")]
            }
        );
    }

    #[test]
    fn to_text_roundtrips_markers() {
        let text = "前文\n改正後\n甲種\n改正前\n乙類";
        let doc = Document::from_text(text);
        assert_eq!(doc.to_text(), text);
    }

    #[test]
    fn serializes_to_json() {
        let doc = Document::from_text("改正後\n甲\n改正前\n乙");
        let json = serde_json::to_string(&doc).unwrap();
        assert!(json.contains("\"format\":\"shinkyu\""));
        assert!(json.contains("\"kind\":\"shinkyu\""));
        // underline は false のとき省略される。
        assert!(!json.contains("underline"));
    }
}
