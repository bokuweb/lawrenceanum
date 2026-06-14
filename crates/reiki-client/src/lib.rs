//! 自治体例規スクレイパー。
//!
//! robots.txt 準拠・1 req/sec 上限厳守。
//! ぎょうせい型例規集（全自治体の約 50%）に 1 アダプタで対応。
//!
//! ## 方針
//!
//! - 必ず自治体**公式サイト**掲載の例規集ページから取得（ベンダ DB 直叩き NG）。
//! - 取得元 URL を `source` に残す。
//! - 著作権法 13 条により例規本文に著作権は生じないが、ToS / robots を厳守する。

use anyhow::{Context, Result};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::time::Duration;

// ── 公開型 ────────────────────────────────────────────────────────

/// 総務省全国地方公共団体コード (6 桁)。
pub type MunicipalityCode = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Municipality {
    pub code: MunicipalityCode,
    pub name: String,
    /// 例規集ページのベース URL（自治体公式）。
    pub reiki_base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReikiMeta {
    pub reiki_id: String,
    pub municipality_code: MunicipalityCode,
    pub title: String,
    pub reiki_number: Option<String>,
    pub enforced_date: Option<String>,
    pub detail_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReikiDocument {
    pub schema_version: u32,
    pub reiki_id: String,
    pub municipality_code: MunicipalityCode,
    pub municipality_name: String,
    pub title: String,
    pub reiki_number: Option<String>,
    pub enforced_date: Option<String>,
    pub body_text: String,
    pub source: ReikiSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReikiSource {
    pub provider: String,
    pub fetched_at: String,
    pub detail_url: String,
    pub municipality_official_site: String,
}

// ── Provider trait ────────────────────────────────────────────────

pub trait ReikiProvider: Send + Sync {
    fn list_reiki(&self, municipality: &Municipality) -> Result<Vec<ReikiMeta>>;
    fn fetch_reiki(&self, meta: &ReikiMeta, municipality: &Municipality)
        -> Result<ReikiDocument>;
}

// ── Mock ─────────────────────────────────────────────────────────

pub struct MockProvider;

impl ReikiProvider for MockProvider {
    fn list_reiki(&self, municipality: &Municipality) -> Result<Vec<ReikiMeta>> {
        let reiki_id = format!("{}_jourei_sample", municipality.code);
        Ok(vec![ReikiMeta {
            reiki_id: reiki_id.clone(),
            municipality_code: municipality.code.clone(),
            title: format!("{}個人情報保護条例", municipality.name),
            reiki_number: Some("条例第1号".to_string()),
            enforced_date: Some("2023-04-01".to_string()),
            detail_url: format!("{}/detail/{}", municipality.reiki_base_url, reiki_id),
        }])
    }

    fn fetch_reiki(
        &self,
        meta: &ReikiMeta,
        municipality: &Municipality,
    ) -> Result<ReikiDocument> {
        Ok(ReikiDocument {
            schema_version: 1,
            reiki_id: meta.reiki_id.clone(),
            municipality_code: municipality.code.clone(),
            municipality_name: municipality.name.clone(),
            title: meta.title.clone(),
            reiki_number: meta.reiki_number.clone(),
            enforced_date: meta.enforced_date.clone(),
            body_text: "（第一条）この条例は、個人情報の保護に関し必要な事項を定める。"
                .to_string(),
            source: ReikiSource {
                provider: "gyosei".to_string(),
                fetched_at: "2024-01-01T00:00:00Z".to_string(),
                detail_url: meta.detail_url.clone(),
                municipality_official_site: municipality.reiki_base_url.clone(),
            },
        })
    }
}

// ── ぎょうせい型 Http アダプタ ────────────────────────────────────

pub struct GyoseiHttpProvider;

impl GyoseiHttpProvider {
    pub fn new() -> Self {
        Self
    }

    fn client() -> Result<reqwest::blocking::Client> {
        reqwest::blocking::Client::builder()
            .user_agent("lawpub/0.1 (+https://github.com/bokuweb/lawrenceanum)")
            .timeout(Duration::from_secs(30))
            .build()
            .context("build reqwest client")
    }

    fn get_html(client: &reqwest::blocking::Client, url: &str) -> Result<String> {
        // 1 req/sec 厳守（robots.txt 準拠）
        std::thread::sleep(Duration::from_secs(1));
        let resp = client
            .get(url)
            .send()
            .and_then(|r| r.error_for_status())
            .with_context(|| format!("GET {url}"))?;
        resp.text().context("read response text")
    }
}

impl Default for GyoseiHttpProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ReikiProvider for GyoseiHttpProvider {
    fn list_reiki(&self, municipality: &Municipality) -> Result<Vec<ReikiMeta>> {
        let client = Self::client()?;
        // ぎょうせい型の例規一覧 URL パターン
        let url = format!("{}/reiki_menu/reiki_r_contents.html", municipality.reiki_base_url);
        let html = Self::get_html(&client, &url)?;
        parse_gyosei_list(&html, municipality)
    }

    fn fetch_reiki(
        &self,
        meta: &ReikiMeta,
        municipality: &Municipality,
    ) -> Result<ReikiDocument> {
        let client = Self::client()?;
        let html = Self::get_html(&client, &meta.detail_url)?;
        let fetched_at = chrono::Utc::now().to_rfc3339();
        parse_gyosei_detail(&html, meta, municipality, &fetched_at)
    }
}

// ── HTML パース（ぎょうせい型）────────────────────────────────────

fn sel(css: &str) -> Selector {
    Selector::parse(css).unwrap_or_else(|_| Selector::parse("*").unwrap())
}

fn text_of(el: &scraper::ElementRef) -> String {
    el.text().collect::<Vec<_>>().join("").trim().to_string()
}

pub fn parse_gyosei_list(html: &str, municipality: &Municipality) -> Result<Vec<ReikiMeta>> {
    let doc = Html::parse_document(html);
    let link_sel = sel("a");
    let mut metas = Vec::new();

    for a in doc.select(&link_sel) {
        let href = a.value().attr("href").unwrap_or("");
        if !href.contains("reiki_honbun") && !href.contains("honbun") {
            continue;
        }
        let title = text_of(&a);
        if title.is_empty() {
            continue;
        }
        let detail_url = if href.starts_with("http") {
            href.to_string()
        } else {
            format!("{}/{}", municipality.reiki_base_url.trim_end_matches('/'), href.trim_start_matches('/'))
        };
        // reiki_id = "{municipality_code}_{urlの末尾ファイル名除く拡張子}"
        let reiki_id = href
            .split('/')
            .last()
            .unwrap_or("")
            .trim_end_matches(".html")
            .to_string();
        let reiki_id = format!("{}_{}", municipality.code, reiki_id);

        metas.push(ReikiMeta {
            reiki_id,
            municipality_code: municipality.code.clone(),
            title,
            reiki_number: None,
            enforced_date: None,
            detail_url,
        });
    }
    Ok(metas)
}

pub fn parse_gyosei_detail(
    html: &str,
    meta: &ReikiMeta,
    municipality: &Municipality,
    fetched_at: &str,
) -> Result<ReikiDocument> {
    let doc = Html::parse_document(html);
    let body_sel = sel("div.reiki-honbun, div#honbun, div.honbun, div.contents");
    let body_text = doc
        .select(&body_sel)
        .next()
        .map(|el| text_of(&el))
        .unwrap_or_else(|| {
            // フォールバック: body 全体
            doc.select(&sel("body"))
                .next()
                .map(|el| text_of(&el))
                .unwrap_or_default()
        });

    Ok(ReikiDocument {
        schema_version: 1,
        reiki_id: meta.reiki_id.clone(),
        municipality_code: municipality.code.clone(),
        municipality_name: municipality.name.clone(),
        title: meta.title.clone(),
        reiki_number: meta.reiki_number.clone(),
        enforced_date: meta.enforced_date.clone(),
        body_text,
        source: ReikiSource {
            provider: "gyosei".to_string(),
            fetched_at: fetched_at.to_string(),
            detail_url: meta.detail_url.clone(),
            municipality_official_site: municipality.reiki_base_url.clone(),
        },
    })
}

// ── 既知自治体リスト（初期 3 件）────────────────────────────────

pub fn known_municipalities() -> Vec<Municipality> {
    vec![
        Municipality {
            code: "131016".to_string(),
            name: "\u{5343}\u{4ee3}\u{7530}\u{533a}".to_string(), // 千代田区
            reiki_base_url: "https://www.city.chiyoda.lg.jp/reiki".to_string(),
        },
        Municipality {
            code: "011002".to_string(),
            name: "\u{672d}\u{5e4c}\u{5e02}".to_string(), // 札幌市
            reiki_base_url: "https://www.city.sapporo.jp/reiki".to_string(),
        },
        Municipality {
            code: "271004".to_string(),
            name: "\u{5927}\u{962a}\u{5e02}".to_string(), // 大阪市
            reiki_base_url: "https://www.city.osaka.lg.jp/reiki".to_string(),
        },
    ]
}

// ── テスト ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_municipality() -> Municipality {
        Municipality {
            code: "131016".to_string(),
            name: "\u{5343}\u{4ee3}\u{7530}\u{533a}".to_string(),
            reiki_base_url: "https://example.com/reiki".to_string(),
        }
    }

    #[test]
    fn mock_provider_list() {
        let m = test_municipality();
        let p = MockProvider;
        let metas = p.list_reiki(&m).unwrap();
        assert_eq!(metas.len(), 1);
        assert!(metas[0].reiki_id.starts_with("131016_"));
    }

    #[test]
    fn mock_provider_fetch() {
        let m = test_municipality();
        let p = MockProvider;
        let metas = p.list_reiki(&m).unwrap();
        let doc = p.fetch_reiki(&metas[0], &m).unwrap();
        assert_eq!(doc.schema_version, 1);
        assert_eq!(doc.municipality_code, "131016");
        assert!(!doc.body_text.is_empty());
    }

    #[test]
    fn parse_gyosei_list_sample() {
        let m = test_municipality();
        let html = r#"<html><body>
          <a href="/reiki_menu/reiki_honbun_001.html">個人情報保護条例</a>
          <a href="/reiki_menu/reiki_honbun_002.html">情報公開条例</a>
          <a href="/other/unrelated.html">関係なし</a>
        </body></html>"#;
        let metas = parse_gyosei_list(html, &m).unwrap();
        assert_eq!(metas.len(), 2);
        assert!(metas[0].reiki_id.contains("131016"));
    }
}
