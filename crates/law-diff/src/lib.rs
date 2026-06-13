//! 法令の任意 2 リビジョン間の構造化 diff。
//!
//! 入力: 2 つの `LawDocument`
//! 出力: 条文・項単位の差分 (`LawDiff`)。
//!
//! マッチング戦略 (MVP):
//!   1. `article_id` の完全一致で対応付け
//!   2. paragraph は `paragraph_no` または順序で対応付け
//!   3. テキスト差分は `similar::TextDiff::from_chars`
//!
//! renumbered / moved の検出は将来。

use law_normalizer::{Article, LawDocument, Paragraph};
use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};
use std::collections::BTreeMap;

pub const DIFF_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevisionRef {
    pub revision_id: Option<String>,
    pub promulgation_date: Option<String>,
    pub effective_date: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiffSummary {
    pub articles_added: usize,
    pub articles_removed: usize,
    pub articles_modified: usize,
    pub articles_unchanged: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "op")]
pub enum TextOp {
    Equal { text: String },
    Insert { text: String },
    Delete { text: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "change_type")]
pub enum ParagraphDiff {
    Unchanged {
        paragraph_no: Option<String>,
    },
    Added {
        paragraph_no: Option<String>,
        text: String,
    },
    Removed {
        paragraph_no: Option<String>,
        text: String,
    },
    Modified {
        paragraph_no: Option<String>,
        text_diff: Vec<TextOp>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArticleSide {
    pub article_no: String,
    pub caption: Option<String>,
    pub paragraphs: Vec<Paragraph>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "change_type")]
pub enum ArticleDiff {
    Unchanged {
        article_id: String,
    },
    Added {
        article_id: String,
        to: ArticleSide,
    },
    Removed {
        article_id: String,
        from: ArticleSide,
    },
    Modified {
        article_id: String,
        from: ArticleHeader,
        to: ArticleHeader,
        paragraphs: Vec<ParagraphDiff>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArticleHeader {
    pub article_no: String,
    pub caption: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LawDiff {
    pub schema_version: u32,
    pub law_id: String,
    pub from: RevisionRef,
    pub to: RevisionRef,
    pub summary: DiffSummary,
    pub articles: Vec<ArticleDiff>,
}

/// 2 つの LawDocument を比較して LawDiff を返す。
///
/// `include_unchanged = false` の場合、変化のない条文は `articles` から省略する
/// (summary には計上する)。
pub fn diff_documents(
    from: &LawDocument,
    to: &LawDocument,
    include_unchanged: bool,
) -> LawDiff {
    assert_eq!(
        from.law_id, to.law_id,
        "diff_documents: law_id mismatch ({} vs {})",
        from.law_id, to.law_id
    );

    let from_map: BTreeMap<&str, &Article> =
        from.articles.iter().map(|a| (a.article_id.as_str(), a)).collect();
    let to_map: BTreeMap<&str, &Article> =
        to.articles.iter().map(|a| (a.article_id.as_str(), a)).collect();

    // 出力順序は to の出現順 → from のみに存在するものを末尾に追加
    let mut articles: Vec<ArticleDiff> = Vec::new();
    let mut summary = DiffSummary::default();

    for ta in &to.articles {
        match from_map.get(ta.article_id.as_str()) {
            Some(fa) => {
                if articles_equal(fa, ta) {
                    summary.articles_unchanged += 1;
                    if include_unchanged {
                        articles.push(ArticleDiff::Unchanged {
                            article_id: ta.article_id.clone(),
                        });
                    }
                } else {
                    summary.articles_modified += 1;
                    articles.push(diff_article(fa, ta));
                }
            }
            None => {
                summary.articles_added += 1;
                articles.push(ArticleDiff::Added {
                    article_id: ta.article_id.clone(),
                    to: article_side(ta),
                });
            }
        }
    }

    for fa in &from.articles {
        if !to_map.contains_key(fa.article_id.as_str()) {
            summary.articles_removed += 1;
            articles.push(ArticleDiff::Removed {
                article_id: fa.article_id.clone(),
                from: article_side(fa),
            });
        }
    }

    LawDiff {
        schema_version: DIFF_SCHEMA_VERSION,
        law_id: from.law_id.clone(),
        from: RevisionRef {
            revision_id: from.revision_id.clone(),
            promulgation_date: from.promulgation_date.clone(),
            effective_date: from.effective_date.clone(),
        },
        to: RevisionRef {
            revision_id: to.revision_id.clone(),
            promulgation_date: to.promulgation_date.clone(),
            effective_date: to.effective_date.clone(),
        },
        summary,
        articles,
    }
}

fn article_side(a: &Article) -> ArticleSide {
    ArticleSide {
        article_no: a.article_no.clone(),
        caption: a.caption.clone(),
        paragraphs: a.paragraphs.clone(),
    }
}

fn articles_equal(a: &Article, b: &Article) -> bool {
    a.article_no == b.article_no
        && a.caption == b.caption
        && a.paragraphs.len() == b.paragraphs.len()
        && a.paragraphs
            .iter()
            .zip(&b.paragraphs)
            .all(|(p, q)| p.paragraph_no == q.paragraph_no && p.text == q.text)
}

fn diff_article(from: &Article, to: &Article) -> ArticleDiff {
    // 段落マッチング: paragraph_no があれば優先、なければ順序
    let paragraphs = diff_paragraphs(&from.paragraphs, &to.paragraphs);
    ArticleDiff::Modified {
        article_id: to.article_id.clone(),
        from: ArticleHeader {
            article_no: from.article_no.clone(),
            caption: from.caption.clone(),
        },
        to: ArticleHeader {
            article_no: to.article_no.clone(),
            caption: to.caption.clone(),
        },
        paragraphs,
    }
}

fn diff_paragraphs(from: &[Paragraph], to: &[Paragraph]) -> Vec<ParagraphDiff> {
    // すべてに paragraph_no があれば map で対応付け、無ければ index で対応付け
    let all_numbered = from.iter().all(|p| p.paragraph_no.is_some())
        && to.iter().all(|p| p.paragraph_no.is_some());

    let mut out: Vec<ParagraphDiff> = Vec::new();

    if all_numbered && !from.is_empty() && !to.is_empty() {
        let from_map: BTreeMap<&str, &Paragraph> = from
            .iter()
            .map(|p| (p.paragraph_no.as_deref().unwrap_or(""), p))
            .collect();
        let to_map: BTreeMap<&str, &Paragraph> = to
            .iter()
            .map(|p| (p.paragraph_no.as_deref().unwrap_or(""), p))
            .collect();
        for tp in to {
            let key = tp.paragraph_no.as_deref().unwrap_or("");
            match from_map.get(key) {
                Some(fp) => {
                    if fp.text == tp.text {
                        out.push(ParagraphDiff::Unchanged {
                            paragraph_no: tp.paragraph_no.clone(),
                        });
                    } else {
                        out.push(ParagraphDiff::Modified {
                            paragraph_no: tp.paragraph_no.clone(),
                            text_diff: diff_text(&fp.text, &tp.text),
                        });
                    }
                }
                None => out.push(ParagraphDiff::Added {
                    paragraph_no: tp.paragraph_no.clone(),
                    text: tp.text.clone(),
                }),
            }
        }
        for fp in from {
            let key = fp.paragraph_no.as_deref().unwrap_or("");
            if !to_map.contains_key(key) {
                out.push(ParagraphDiff::Removed {
                    paragraph_no: fp.paragraph_no.clone(),
                    text: fp.text.clone(),
                });
            }
        }
    } else {
        // index 対応
        let n = from.len().max(to.len());
        for i in 0..n {
            match (from.get(i), to.get(i)) {
                (Some(fp), Some(tp)) => {
                    if fp.text == tp.text && fp.paragraph_no == tp.paragraph_no {
                        out.push(ParagraphDiff::Unchanged {
                            paragraph_no: tp.paragraph_no.clone(),
                        });
                    } else {
                        out.push(ParagraphDiff::Modified {
                            paragraph_no: tp.paragraph_no.clone(),
                            text_diff: diff_text(&fp.text, &tp.text),
                        });
                    }
                }
                (None, Some(tp)) => out.push(ParagraphDiff::Added {
                    paragraph_no: tp.paragraph_no.clone(),
                    text: tp.text.clone(),
                }),
                (Some(fp), None) => out.push(ParagraphDiff::Removed {
                    paragraph_no: fp.paragraph_no.clone(),
                    text: fp.text.clone(),
                }),
                (None, None) => unreachable!(),
            }
        }
    }

    out
}

/// 文字単位 diff。極端に長い段落 (>50KB) は単語フォールバック。
fn diff_text(from: &str, to: &str) -> Vec<TextOp> {
    let use_words = from.len() > 50_000 || to.len() > 50_000;
    let diff = if use_words {
        TextDiff::from_words(from, to)
    } else {
        TextDiff::from_chars(from, to)
    };

    let mut ops: Vec<TextOp> = Vec::new();
    for change in diff.iter_all_changes() {
        let text = change.value().to_string();
        let op = match change.tag() {
            ChangeTag::Equal => TextOp::Equal { text },
            ChangeTag::Insert => TextOp::Insert { text },
            ChangeTag::Delete => TextOp::Delete { text },
        };
        merge_push(&mut ops, op);
    }
    ops
}

/// 連続する同種 op を結合 (from_chars は 1 文字ずつ返るため)。
fn merge_push(ops: &mut Vec<TextOp>, op: TextOp) {
    if let Some(last) = ops.last_mut() {
        match (last, &op) {
            (TextOp::Equal { text: a }, TextOp::Equal { text: b }) => {
                a.push_str(b);
                return;
            }
            (TextOp::Insert { text: a }, TextOp::Insert { text: b }) => {
                a.push_str(b);
                return;
            }
            (TextOp::Delete { text: a }, TextOp::Delete { text: b }) => {
                a.push_str(b);
                return;
            }
            _ => {}
        }
    }
    ops.push(op);
}

#[cfg(test)]
mod tests {
    use super::*;
    use law_normalizer::{Article, LawDocument, Paragraph, SourceMeta, SCHEMA_VERSION};

    fn doc(law_id: &str, rev: &str, articles: Vec<Article>) -> LawDocument {
        LawDocument {
            schema_version: SCHEMA_VERSION,
            law_id: law_id.to_string(),
            law_num: None,
            title: "テスト法".to_string(),
            revision_id: Some(rev.to_string()),
            promulgation_date: None,
            effective_date: None,
            status: "current".to_string(),
            articles,
            suppl_provisions: Vec::new(),
            source: SourceMeta {
                provider: "test".to_string(),
                raw_xml_sha256: None,
                fetched_at: "2026-01-01T00:00:00Z".to_string(),
            },
        }
    }

    fn art(id: &str, no: &str, texts: &[(&str, &str)]) -> Article {
        Article {
            article_id: id.to_string(),
            article_no: no.to_string(),
            caption: None,
            paragraphs: texts
                .iter()
                .map(|(n, t)| Paragraph {
                    paragraph_no: if n.is_empty() { None } else { Some(n.to_string()) },
                    text: t.to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn detects_added_article() {
        let from = doc("L1", "r1", vec![art("art_1", "第一条", &[("1", "あ")])]);
        let to = doc(
            "L1",
            "r2",
            vec![
                art("art_1", "第一条", &[("1", "あ")]),
                art("art_2", "第二条", &[("1", "い")]),
            ],
        );
        let d = diff_documents(&from, &to, false);
        assert_eq!(d.summary.articles_added, 1);
        assert_eq!(d.summary.articles_unchanged, 1);
        assert_eq!(d.summary.articles_modified, 0);
        assert!(matches!(d.articles[0], ArticleDiff::Added { .. }));
    }

    #[test]
    fn detects_removed_article() {
        let from = doc(
            "L1",
            "r1",
            vec![
                art("art_1", "第一条", &[("1", "あ")]),
                art("art_2", "第二条", &[("1", "い")]),
            ],
        );
        let to = doc("L1", "r2", vec![art("art_1", "第一条", &[("1", "あ")])]);
        let d = diff_documents(&from, &to, false);
        assert_eq!(d.summary.articles_removed, 1);
        assert!(matches!(d.articles[0], ArticleDiff::Removed { .. }));
    }

    #[test]
    fn detects_modified_paragraph_text() {
        let from = doc(
            "L1",
            "r1",
            vec![art("art_1", "第一条", &[("1", "他人の権利を侵害した者は")])],
        );
        let to = doc(
            "L1",
            "r2",
            vec![art(
                "art_1",
                "第一条",
                &[("1", "他人の権利又は法律上保護される利益を侵害した者は")],
            )],
        );
        let d = diff_documents(&from, &to, false);
        assert_eq!(d.summary.articles_modified, 1);
        let ArticleDiff::Modified { paragraphs, .. } = &d.articles[0] else {
            panic!("expected modified");
        };
        let ParagraphDiff::Modified { text_diff, .. } = &paragraphs[0] else {
            panic!("expected paragraph modified");
        };
        // 何らかの insert が混じっている
        assert!(text_diff.iter().any(|op| matches!(op, TextOp::Insert { .. })));
        // equal の text を結合したら元テキストの先頭が含まれる
        let eq_text: String = text_diff
            .iter()
            .filter_map(|op| match op {
                TextOp::Equal { text } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(eq_text.contains("他人の権利"));
    }

    #[test]
    fn unchanged_articles_omitted_by_default() {
        let from = doc("L1", "r1", vec![art("art_1", "第一条", &[("1", "あ")])]);
        let to = doc("L1", "r2", vec![art("art_1", "第一条", &[("1", "あ")])]);
        let d = diff_documents(&from, &to, false);
        assert_eq!(d.summary.articles_unchanged, 1);
        assert!(d.articles.is_empty());
    }
}
