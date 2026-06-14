//! 審議会・委員会議事録スクレイパー。
//!
//! 各府省ウェブサイトに分散しており統一 API がないため、
//! 府省ごとのアダプタ方式で HTML をパースする。
//!
//! ## 現在対応府省
//!
//! - 法務省 (`moj`) — `https://www.moj.go.jp/`
//! - 内閣府 (`cao`) — `https://www.cao.go.jp/`
//!
//! ## 追加方針
//!
//! `MinistryAdapter` トレイトを実装するだけで府省を追加できる。

use anyhow::{Context, Result};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::time::Duration;

// ── 公開型 ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinutesMeta {
    pub minutes_id: String,
    pub ministry: String,
    pub committee: String,
    pub date: Option<String>,
    pub title: String,
    pub detail_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinutesDocument {
    pub schema_version: u32,
    pub minutes_id: String,
    pub ministry: String,
    pub committee: String,
    pub date: Option<String>,
    pub title: String,
    pub body_text: String,
    pub attachments: Vec<String>,
    pub source: MinutesSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinutesSource {
    pub provider: String,
    pub fetched_at: String,
    pub detail_url: String,
}

// ── アダプタ trait ────────────────────────────────────────────────

pub trait MinistryAdapter: Send + Sync {
    fn ministry_id(&self) -> &str;
    fn list_committees(&self) -> Result<Vec<String>>;
    fn list_minutes(&self, committee: &str) -> Result<Vec<MinutesMeta>>;
    fn fetch_minutes(&self, meta: &MinutesMeta) -> Result<MinutesDocument>;
}

// ── Mock ─────────────────────────────────────────────────────────

pub struct MockAdapter;

impl MinistryAdapter for MockAdapter {
    fn ministry_id(&self) -> &str {
        "mock"
    }

    fn list_committees(&self) -> Result<Vec<String>> {
        Ok(vec!["\u{6cd5}\u{52d9}\u{59d4}\u{54e1}\u{4f1a}".to_string()]) // 法務委員会
    }

    fn list_minutes(&self, committee: &str) -> Result<Vec<MinutesMeta>> {
        Ok(vec![MinutesMeta {
            minutes_id: "mock_moj_0001".to_string(),
            ministry: "mock".to_string(),
            committee: committee.to_string(),
            date: Some("2024-10-01".to_string()),
            title: format!("{} \u{7b2c}1\u{56de}", committee), // ○○ 第1回
            detail_url: "https://example.com/shingikai/0001".to_string(),
        }])
    }

    fn fetch_minutes(&self, meta: &MinutesMeta) -> Result<MinutesDocument> {
        Ok(MinutesDocument {
            schema_version: 1,
            minutes_id: meta.minutes_id.clone(),
            ministry: meta.ministry.clone(),
            committee: meta.committee.clone(),
            date: meta.date.clone(),
            title: meta.title.clone(),
            body_text: "\u{8b70}\u{4e8b}\u{8981}\u{65e8}\u{30c6}\u{30b9}\u{30c8}".to_string(), // 議事要旨テスト
            attachments: vec![],
            source: MinutesSource {
                provider: "mock".to_string(),
                fetched_at: "2024-01-01T00:00:00Z".to_string(),
                detail_url: meta.detail_url.clone(),
            },
        })
    }
}

// ── 法務省アダプタ ────────────────────────────────────────────────

pub struct MojAdapter {
    base_url: String,
}

impl MojAdapter {
    pub fn new() -> Self {
        let base_url = std::env::var("LAWPUB_MOJ_BASE_URL")
            .unwrap_or_else(|_| "https://www.moj.go.jp".to_string())
            .trim_end_matches('/')
            .to_string();
        Self { base_url }
    }

    fn client() -> Result<reqwest::blocking::Client> {
        reqwest::blocking::Client::builder()
            .user_agent("lawpub/0.1 (+https://github.com/bokuweb/lawrenceanum)")
            .timeout(Duration::from_secs(30))
            .build()
            .context("build client")
    }

    fn get_html(client: &reqwest::blocking::Client, url: &str) -> Result<String> {
        std::thread::sleep(Duration::from_secs(1));
        let resp = client
            .get(url)
            .send()
            .and_then(|r| r.error_for_status())
            .with_context(|| format!("GET {url}"))?;
        resp.text().context("read text")
    }
}

impl Default for MojAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl MinistryAdapter for MojAdapter {
    fn ministry_id(&self) -> &str {
        "moj"
    }

    fn list_committees(&self) -> Result<Vec<String>> {
        // 法務省の審議会一覧ページから委員会名を抽出する
        let client = Self::client()?;
        let url = format!("{}/shingi/index.html", self.base_url);
        let html = Self::get_html(&client, &url)?;
        let doc = Html::parse_document(&html);
        let sel = Selector::parse("a").unwrap();
        let committees: Vec<String> = doc
            .select(&sel)
            .filter_map(|a| {
                let href = a.value().attr("href")?;
                // 審議会ページへのリンクのみ抽出
                if href.contains("shingi") || href.contains("kenkyukai") {
                    let text = a.text().collect::<String>().trim().to_string();
                    if !text.is_empty() { Some(text) } else { None }
                } else {
                    None
                }
            })
            .collect();
        Ok(committees)
    }

    fn list_minutes(&self, committee: &str) -> Result<Vec<MinutesMeta>> {
        let client = Self::client()?;
        // 委員会名をURLエンコードして検索
        let encoded = committee.chars().map(|c| {
            if c.is_ascii_alphanumeric() { c.to_string() }
            else { format!("%{:02X}", c as u32) }
        }).collect::<String>();
        let url = format!("{}/shingi/{}index.html", self.base_url, encoded);
        let html = match Self::get_html(&client, &url) {
            Ok(h) => h,
            Err(e) => {
                tracing::warn!("fetch minutes list for {committee}: {e:#}");
                return Ok(vec![]);
            }
        };
        parse_minutes_list(&html, "moj", committee, &self.base_url)
    }

    fn fetch_minutes(&self, meta: &MinutesMeta) -> Result<MinutesDocument> {
        let client = Self::client()?;
        let html = Self::get_html(&client, &meta.detail_url)?;
        let fetched_at = chrono::Utc::now().to_rfc3339();
        parse_minutes_detail(&html, meta, &fetched_at)
    }
}

// ── HTML パース ───────────────────────────────────────────────────

fn text_of(el: &scraper::ElementRef) -> String {
    el.text().collect::<Vec<_>>().join("").trim().to_string()
}

pub fn parse_minutes_list(
    html: &str,
    ministry: &str,
    committee: &str,
    base_url: &str,
) -> Result<Vec<MinutesMeta>> {
    let doc = Html::parse_document(html);
    let link_sel = Selector::parse("a").unwrap();
    let mut metas = Vec::new();

    for a in doc.select(&link_sel) {
        let href = a.value().attr("href").unwrap_or("");
        // 議事録・議事要旨へのリンクを抽出
        let lower = href.to_lowercase();
        if !lower.contains("gijiroku")
            && !lower.contains("gijiyoshi")
            && !lower.contains("kaigiroku")
            && !lower.contains("minutes")
        {
            continue;
        }
        let title = text_of(&a);
        if title.is_empty() {
            continue;
        }
        let detail_url = if href.starts_with("http") {
            href.to_string()
        } else {
            format!("{}/{}", base_url.trim_end_matches('/'), href.trim_start_matches('/'))
        };
        let minutes_id = format!(
            "{}_{}_{}",
            ministry,
            committee.chars().take(10).collect::<String>(),
            href.split('/').last().unwrap_or("").trim_end_matches(".html")
        );
        metas.push(MinutesMeta {
            minutes_id,
            ministry: ministry.to_string(),
            committee: committee.to_string(),
            date: None,
            title,
            detail_url,
        });
    }
    Ok(metas)
}

pub fn parse_minutes_detail(
    html: &str,
    meta: &MinutesMeta,
    fetched_at: &str,
) -> Result<MinutesDocument> {
    let doc = Html::parse_document(html);
    let body_sel = Selector::parse("div.contents, div#main, div.main, article, main").unwrap();
    let body_text = doc
        .select(&body_sel)
        .next()
        .map(|el| text_of(&el))
        .unwrap_or_else(|| {
            doc.select(&Selector::parse("body").unwrap())
                .next()
                .map(|el| text_of(&el))
                .unwrap_or_default()
        });

    // 添付資料 URL を収集
    let pdf_sel = Selector::parse("a[href$='.pdf'], a[href$='.PDF']").unwrap();
    let attachments: Vec<String> = doc
        .select(&pdf_sel)
        .filter_map(|a| a.value().attr("href").map(String::from))
        .collect();

    Ok(MinutesDocument {
        schema_version: 1,
        minutes_id: meta.minutes_id.clone(),
        ministry: meta.ministry.clone(),
        committee: meta.committee.clone(),
        date: meta.date.clone(),
        title: meta.title.clone(),
        body_text,
        attachments,
        source: MinutesSource {
            provider: format!("shingikai_{}", meta.ministry),
            fetched_at: fetched_at.to_string(),
            detail_url: meta.detail_url.clone(),
        },
    })
}

// ── テスト ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_adapter_list_and_fetch() {
        let a = MockAdapter;
        let committees = a.list_committees().unwrap();
        assert!(!committees.is_empty());
        let metas = a.list_minutes(&committees[0]).unwrap();
        assert!(!metas.is_empty());
        let doc = a.fetch_minutes(&metas[0]).unwrap();
        assert_eq!(doc.schema_version, 1);
        assert!(!doc.body_text.is_empty());
    }

    #[test]
    fn parse_minutes_list_sample() {
        let html = r#"<html><body>
          <a href="/shingi/gijiroku_001.html">第1回議事録</a>
          <a href="/shingi/gijiyoshi_002.html">第2回議事要旨</a>
          <a href="/other/unrelated.html">関係なし</a>
        </body></html>"#;
        let metas = parse_minutes_list(html, "moj", "test_committee", "https://www.moj.go.jp").unwrap();
        assert_eq!(metas.len(), 2);
    }

    #[test]
    fn parse_minutes_detail_extracts_attachments() {
        let html = r#"<html><body>
          <div class="contents">
            <p>議事要旨テスト</p>
            <a href="/doc/shiryo.pdf">資料1</a>
            <a href="/doc/another.pdf">資料2</a>
          </div>
        </body></html>"#;
        let meta = MinutesMeta {
            minutes_id: "test".to_string(),
            ministry: "moj".to_string(),
            committee: "test".to_string(),
            date: None,
            title: "テスト".to_string(),
            detail_url: "https://example.com".to_string(),
        };
        let doc = parse_minutes_detail(html, &meta, "2024-01-01T00:00:00Z").unwrap();
        assert_eq!(doc.attachments.len(), 2);
        assert!(!doc.body_text.is_empty());
    }

    #[test]
    #[ignore]
    fn moj_real_list_committees() {
        let a = MojAdapter::new();
        let committees = a.list_committees().unwrap();
        println!("{} committees", committees.len());
    }
}
