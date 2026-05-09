//! 官報サイトからの取得を抽象化する Phase 3 クライアント。
//!
//! Phase 3 初期実装ではモック provider のみ。`HttpProvider` は将来、
//! [国立印刷局・官報](https://kanpou.npb.go.jp/) 等の日付ページをスクレイプして
//! PDF URL を集める想定。

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanpoIssue {
    pub issue_type: String,           // "regular" | "extra" | "special_extra"
    pub issue_no: String,             // "第101号" 等
    pub pdf_url: String,
    pub sha256: Option<String>,
    /// このPDFのpresumed公布日(=発行日)。
    pub promulgation_date: String,
    /// PDF本文から抜き出せた候補法令名・法令番号 (現状は素朴な抽出)。
    pub law_nums: Vec<String>,
    pub titles: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanpoDate {
    pub date: String,
    pub issues: Vec<KanpoIssue>,
}

pub trait KanpoProvider: Send + Sync {
    fn fetch_date(&self, date: &str) -> Result<KanpoDate>;
}

pub struct MockKanpoProvider;

impl KanpoProvider for MockKanpoProvider {
    fn fetch_date(&self, date: &str) -> Result<KanpoDate> {
        // 開発時の固定スタブ。実HTTP実装まではこの形だけ書き出す。
        Ok(KanpoDate {
            date: date.to_string(),
            issues: vec![KanpoIssue {
                issue_type: "regular".to_string(),
                issue_no: "第1号".to_string(),
                pdf_url: format!("mock://kanpo/{}/regular-1.pdf", date),
                sha256: None,
                promulgation_date: date.to_string(),
                law_nums: vec![],
                titles: vec![],
            }],
        })
    }
}
