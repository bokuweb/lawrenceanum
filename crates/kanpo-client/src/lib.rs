//! 官報サイトからの取得を抽象化する Phase 3 クライアント。
//!
//! Phase 3 初期実装ではモック provider のみ。`HttpProvider` は将来、
//! [国立印刷局・官報](https://kanpou.npb.go.jp/) 等の日付ページをスクレイプして
//! PDF URL を集める想定。

use anyhow::Result;
use serde::{Deserialize, Serialize};

pub mod http;

/// 改め文抽出は [`kanpo_amend`] crate に切り出した。後方互換のため `kanpo_client::pdf`
/// として再エクスポートする（`pdf::extract` / `pdf::segment_articles` / `pdf::detect_format_of` 等）。
pub use kanpo_amend as pdf;

pub use http::{page_pdf_url, HttpProvider};

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
    /// 目次から抽出した項目（記事）単位の一覧。各項目が改正法令1件に相当しうる。
    /// 既存のモック / 旧キャッシュとの後方互換のため `default`。
    #[serde(default)]
    pub items: Vec<KanpoItem>,
    /// この号の総ページ数（号インデックスから取得）。項目の終端ページ算定に使う。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_count: Option<u32>,
}

/// 官報1号のなかの「目次1項目」= 1つの法令/告示/通知に相当する単位。
///
/// e-Gov 改正イベントとの突合はこの粒度で行うと、改め文本文をピンポイントに
/// 取り出せる。`pdf_url` は項目別 PDF（ページ単位）を指す。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KanpoItem {
    /// 目次に表示される標題（例: 「○○の一部を改正する政令」）。
    pub title: String,
    /// この項目が始まるページ番号（項目別 PDF のキーにもなる）。
    pub page: u32,
    /// 項目別 PDF の URL。
    pub pdf_url: String,
    pub sha256: Option<String>,
    /// 標題末尾の括弧から推定した制定機関略号（例: 「総務七七」）。突合の補助。
    pub agency_hint: Option<String>,
    /// PDF から抽出・整形した改め文テキスト（PoC 段階では best-effort）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amend_text: Option<String>,
    /// 抽出結果の形式判定: "prose"（散文改め文）/ "shinkyu"（新旧対照表）/ "unknown"。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amend_format: Option<String>,
    /// 構造化した改め文（kanpo-amend の Document）。別表の 2D 表は取得時にしか復元できない
    /// （罫線座標が要る）ため、ここに保存して kanpo-link で timeline へ転記する。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amend_document: Option<serde_json::Value>,
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
                items: vec![],
                page_count: None,
            }],
        })
    }
}
