//! 自治体例規の取得と構造化。
//!
//! ## 方針
//!
//! - 収集対象は RILG「全国自治体例規集リンク集」に載る**自治体が公開している例規集 HTML**
//!   （ベンダの検索 DB・ログインが要る検索アプリは対象外）。[`registry`] 参照。
//! - robots.txt を尊重し、ホストごとに 1 req/sec 以下（Crawl-delay があればそれ以上）。
//! - 取得元 URL を `source` に残す。
//! - 著作権法 13 条により条例・規則の本文に著作権は生じないが、例規集の目次体系や
//!   付加情報はベンダの編集著作物になり得るため、**本文と制定情報だけ**を構造化して持つ。
//!
//! ## 対応形式
//!
//! - [`gyosei`]: ぎょうせい Reiki-Base（`reiki_menu.html`）
//! - [`d1w`]: 第一法規 例規類集 HTML 版（`d1w_reiki/`）

pub mod d1w;
pub mod gyosei;
pub mod http;
pub mod registry;
pub mod structure;
pub mod text;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub const SCHEMA_VERSION: u32 = 2;

/// 総務省全国地方公共団体コード (6 桁、検査数字込み)。
pub type MunicipalityCode = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Vendor {
    /// ぎょうせい Reiki-Base
    Gyosei,
    /// 第一法規 例規類集 (静的 HTML 版)
    D1Static,
}

impl Vendor {
    pub fn as_str(self) -> &'static str {
        match self {
            Vendor::Gyosei => "gyosei",
            Vendor::D1Static => "d1_static",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Municipality {
    pub code: MunicipalityCode,
    pub name: String,
    #[serde(default)]
    pub prefecture: String,
    pub vendor: Vendor,
    /// 例規集のベース URL（`reiki_menu.html` / `d1w_reiki/` の親）。
    pub reiki_base_url: String,
    /// RILG に掲載された入口 URL。
    #[serde(default)]
    pub source_url: String,
}

impl Municipality {
    pub fn entry_url(&self) -> String {
        let base = self.reiki_base_url.trim_end_matches('/');
        match self.vendor {
            Vendor::Gyosei => format!("{base}/reiki_menu.html"),
            Vendor::D1Static => format!("{base}/reiki.html"),
        }
    }

    /// テスト用の簡易コンストラクタ。
    pub fn test(code: &str, name: &str, base: &str) -> Self {
        let vendor = if base.contains("d1w_reiki") {
            Vendor::D1Static
        } else {
            Vendor::Gyosei
        };
        Self {
            code: code.into(),
            name: name.into(),
            prefecture: String::new(),
            vendor,
            reiki_base_url: base.into(),
            source_url: String::new(),
        }
    }
}

/// 一覧（目次）から得られる例規のメタ情報。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReikiMeta {
    pub reiki_id: String,
    pub municipality_code: MunicipalityCode,
    pub title: String,
    pub reiki_number: Option<String>,
    /// 制定（公布）日 ISO。
    pub promulgated_date: Option<String>,
    pub detail_url: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReikiItem {
    /// 「(1)」「ア」「(ア)」「一」等、原文表記のまま。
    pub num: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subitems: Vec<ReikiItem>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReikiParagraph {
    /// 項番号（半角）。第 1 項・無番号は None。
    pub num: Option<String>,
    /// 附則の「(施行期日)」のような項見出し。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<ReikiItem>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReikiArticle {
    /// 「第1条」「第8条の2」。条を持たない本文（附則の項など）は空文字。
    pub article_no: String,
    /// 文書内で安定な ID: "art_8_2" / 附則 "s1_art_1" / 無番号 "p1"。
    #[serde(default)]
    pub article_id: String,
    pub caption: Option<String>,
    /// 直前の章・節見出し。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    #[serde(default)]
    pub paragraphs: Vec<ReikiParagraph>,
}

impl ReikiArticle {
    /// 見出し・番号込みの平坦テキスト（検索索引・抜粋用）。保存はせず都度組み立てる。
    pub fn text(&self) -> String {
        structure::render_article(self)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReikiSupplementary {
    /// 「附則(平成22年3月23日条例第7号)抄」等の見出し全体。
    pub title: String,
    pub articles: Vec<ReikiArticle>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReikiDocument {
    pub schema_version: u32,
    pub reiki_id: String,
    pub municipality_code: MunicipalityCode,
    pub municipality_name: String,
    #[serde(default)]
    pub prefecture: String,
    pub title: String,
    pub reiki_number: Option<String>,
    /// 例規番号の種別（条例・規則・訓令・告示…）。
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub promulgated_date: Option<String>,
    /// 例規集の内容現在日 ISO（取得時点）。
    #[serde(default)]
    pub current_as_of: Option<String>,
    /// 第1条より前の本文（制定文・前文・目次）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preamble: Vec<String>,
    #[serde(default)]
    pub articles: Vec<ReikiArticle>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supplementary: Vec<ReikiSupplementary>,
    /// 本文内容（題名・番号・本則・附則）のハッシュ。変更検知に使う。
    #[serde(default)]
    pub content_sha256: String,
    pub source: ReikiSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReikiSource {
    pub provider: String,
    /// この内容を最初に取得した時刻（内容が変わるたびに更新）。
    pub fetched_at: String,
    /// 最後に取得して内容を確認した時刻。
    #[serde(default)]
    pub checked_at: String,
    pub detail_url: String,
    pub municipality_official_site: String,
}

impl ReikiDocument {
    pub fn compute_sha256(&self) -> String {
        let payload = serde_json::json!({
            "title": self.title,
            "reiki_number": self.reiki_number,
            "promulgated_date": self.promulgated_date,
            "preamble": self.preamble,
            "articles": self.articles,
            "supplementary": self.supplementary,
        });
        hex::encode(Sha256::digest(payload.to_string().as_bytes()))
    }

    /// 構造化結果とメタ情報から文書を組み立てる。
    pub fn assemble(
        meta: &ReikiMeta,
        m: &Municipality,
        s: structure::Structured,
        page_title: Option<String>,
        current_as_of: Option<String>,
        fetched_at: &str,
    ) -> Self {
        let title = s
            .title
            .or(page_title)
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| meta.title.clone());
        let reiki_number = s
            .number
            .as_deref()
            .and_then(text::extract_reiki_number)
            .or_else(|| s.number.clone().filter(|n| n.contains('第')))
            .or_else(|| meta.reiki_number.clone())
            .or_else(|| {
                s.preamble
                    .iter()
                    .find_map(|p| text::extract_reiki_number(p))
            });
        let promulgated_date = s
            .date
            .as_deref()
            .and_then(text::parse_japanese_date)
            .or_else(|| meta.promulgated_date.clone());
        let mut doc = ReikiDocument {
            schema_version: SCHEMA_VERSION,
            reiki_id: meta.reiki_id.clone(),
            municipality_code: m.code.clone(),
            municipality_name: m.name.clone(),
            prefecture: m.prefecture.clone(),
            kind: reiki_number.as_deref().and_then(text::reiki_kind),
            title,
            reiki_number,
            promulgated_date,
            current_as_of,
            preamble: s.preamble,
            articles: s.articles,
            supplementary: s.supplementary,
            content_sha256: String::new(),
            source: ReikiSource {
                provider: m.vendor.as_str().to_string(),
                fetched_at: fetched_at.to_string(),
                checked_at: fetched_at.to_string(),
                detail_url: meta.detail_url.clone(),
                municipality_official_site: m.source_url.clone(),
            },
        };
        doc.content_sha256 = doc.compute_sha256();
        doc
    }
}

// ── Provider ─────────────────────────────────────────────────────

pub trait ReikiProvider: Send + Sync {
    /// 例規集の内容現在日（取れなければ None）。変更が無い自治体の巡回を省くのに使う。
    fn current_as_of(&self, m: &Municipality) -> Result<Option<String>>;
    fn list_reiki(&self, m: &Municipality) -> Result<Vec<ReikiMeta>>;
    fn fetch_reiki(
        &self,
        meta: &ReikiMeta,
        m: &Municipality,
        current_as_of: Option<String>,
    ) -> Result<ReikiDocument>;
}

pub struct MockProvider;

impl ReikiProvider for MockProvider {
    fn current_as_of(&self, _m: &Municipality) -> Result<Option<String>> {
        Ok(Some("2024-04-01".into()))
    }

    fn list_reiki(&self, m: &Municipality) -> Result<Vec<ReikiMeta>> {
        Ok(vec![ReikiMeta {
            reiki_id: format!("{}_jourei_sample", m.code),
            municipality_code: m.code.clone(),
            title: format!("{}個人情報保護条例", m.name),
            reiki_number: Some("条例第1号".into()),
            promulgated_date: Some("2023-04-01".into()),
            detail_url: format!("{}/detail/{}_jourei_sample", m.reiki_base_url, m.code),
        }])
    }

    fn fetch_reiki(
        &self,
        meta: &ReikiMeta,
        m: &Municipality,
        current_as_of: Option<String>,
    ) -> Result<ReikiDocument> {
        use structure::Block;
        let s = structure::build(vec![
            Block::Caption("趣旨".into()),
            Block::Article {
                num: "第1条".into(),
                text: "この条例は、個人情報の保護に関し必要な事項を定める。".into(),
            },
            Block::Supplementary("附則".into()),
            Block::Text("この条例は、令和5年4月1日から施行する。".into()),
        ]);
        Ok(ReikiDocument::assemble(
            meta,
            m,
            s,
            None,
            current_as_of,
            "2024-01-01T00:00:00Z",
        ))
    }
}

/// 実サイトから取得するプロバイダ。ベンダ形式は [`Municipality::vendor`] で切り替える。
pub struct HttpProvider {
    client: Arc<http::PoliteClient>,
}

impl HttpProvider {
    pub fn new(client: Arc<http::PoliteClient>) -> Self {
        Self { client }
    }

    pub fn client(&self) -> &http::PoliteClient {
        &self.client
    }
}

impl ReikiProvider for HttpProvider {
    fn current_as_of(&self, m: &Municipality) -> Result<Option<String>> {
        let html = self.client.get_html(&m.entry_url())?;
        Ok(gyosei::parse_current_as_of(&html))
    }

    fn list_reiki(&self, m: &Municipality) -> Result<Vec<ReikiMeta>> {
        let base = m.reiki_base_url.trim_end_matches('/');
        let (index_url, pages, page_dir): (String, Vec<String>, String) = match m.vendor {
            Vendor::Gyosei => {
                let url = format!("{base}/reiki_kana/kana_default.html");
                let html = self.client.get_html(&url)?;
                (
                    url,
                    gyosei::parse_kana_index(&html),
                    format!("{base}/reiki_kana"),
                )
            }
            Vendor::D1Static => {
                let url = format!("{base}/mokuji_index_index.html");
                let html = self.client.get_html(&url)?;
                (url, d1w::parse_index_pages(&html), base.to_string())
            }
        };
        if pages.is_empty() {
            anyhow::bail!("no index pages found at {index_url}");
        }
        let mut metas = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut failures = 0usize;
        for page in &pages {
            let url = format!("{page_dir}/{page}");
            let html = match self.client.get_html(&url) {
                Ok(h) => h,
                Err(e) => {
                    // 一覧の欠けは削除と誤判定させないよう、呼び出し側で「一覧不完全」として扱う。
                    tracing::warn!("index page {url}: {e:#}");
                    failures += 1;
                    continue;
                }
            };
            let parsed = match m.vendor {
                Vendor::Gyosei => gyosei::parse_list(&html, m),
                Vendor::D1Static => d1w::parse_list(&html, m),
            };
            for meta in parsed {
                if seen.insert(meta.reiki_id.clone()) {
                    metas.push(meta);
                }
            }
        }
        if failures > 0 {
            return Err(IncompleteListing {
                found: metas.len(),
                failed_pages: failures,
            }
            .into());
        }
        Ok(metas)
    }

    fn fetch_reiki(
        &self,
        meta: &ReikiMeta,
        m: &Municipality,
        current_as_of: Option<String>,
    ) -> Result<ReikiDocument> {
        let html = self.client.get_html(&meta.detail_url)?;
        let fetched_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let blocks = match m.vendor {
            Vendor::Gyosei => gyosei::parse_detail_blocks(&html),
            Vendor::D1Static => d1w::parse_detail_blocks(&html),
        };
        let s = structure::build(blocks);
        if s.articles.is_empty() && s.preamble.is_empty() && s.supplementary.is_empty() {
            anyhow::bail!("empty body at {}", meta.detail_url);
        }
        Ok(ReikiDocument::assemble(
            meta,
            m,
            s,
            gyosei::page_title(&html),
            current_as_of,
            &fetched_at,
        ))
    }
}

/// 目次ページの一部が取れなかった（この一覧で削除判定をしてはいけない）。
#[derive(Debug)]
pub struct IncompleteListing {
    pub found: usize,
    pub failed_pages: usize,
}
impl std::fmt::Display for IncompleteListing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "incomplete listing: {} index pages failed ({} reiki found)",
            self.failed_pages, self.found
        )
    }
}
impl std::error::Error for IncompleteListing {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_roundtrip_and_hash_ignores_source() {
        let m = Municipality::test("131016", "千代田区", "https://example.com/reiki");
        let p = MockProvider;
        let metas = p.list_reiki(&m).unwrap();
        let mut d = p
            .fetch_reiki(&metas[0], &m, Some("2024-04-01".into()))
            .unwrap();
        assert_eq!(d.schema_version, SCHEMA_VERSION);
        assert_eq!(d.kind.as_deref(), Some("条例"));
        assert_eq!(d.promulgated_date.as_deref(), Some("2023-04-01"));
        assert_eq!(d.articles[0].article_id, "art_1");
        assert_eq!(d.supplementary.len(), 1);
        let h = d.content_sha256.clone();
        d.source.checked_at = "later".into();
        assert_eq!(d.compute_sha256(), h);
        d.articles[0].paragraphs[0].text.push('。');
        assert_ne!(d.compute_sha256(), h);
    }

    /// 実サイト疎通（手動）: `cargo test -p reiki-client -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn http_real_fetch() {
        let client = Arc::new(http::PoliteClient::new(std::time::Duration::from_secs(1)).unwrap());
        let p = HttpProvider::new(client);
        for m in [
            Municipality::test("121002", "千葉市", "https://www1.g-reiki.net/chiba"),
            Municipality::test(
                "012122",
                "留萌市",
                "https://en3-jg.d1-law.com/rumoi/d1w_reiki",
            ),
        ] {
            let asof = p.current_as_of(&m).unwrap();
            let metas = p.list_reiki(&m).unwrap();
            println!("{}: as_of={asof:?} {} reiki", m.name, metas.len());
            let d = p.fetch_reiki(&metas[0], &m, asof).unwrap();
            println!(
                "  {} / {:?} / {} articles",
                d.title,
                d.reiki_number,
                d.articles.len()
            );
            assert!(!d.title.is_empty());
        }
    }
}
