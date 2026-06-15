//! e-Gov パブリックコメントスクレイパー。
//!
//! 公式 API がないため `public-comment.e-gov.go.jp` の HTML を `scraper` でパースする。
//!
//! ## スクレイプ対象
//!
//! - 案件一覧: `GET /servlet/Public?CLASSNAME=PCMMSTLIST&Mode=1` (結果公示済み)
//! - 案件詳細: `GET /servlet/Public?CLASSNAME=PCMMSTDETAIL&id={case_id}`
//!
//! ## スコープ
//!
//! robots.txt を尊重し、1 リクエストごとに 1 秒以上待機する。
//! 個人情報（記名意見中の氏名等）はこのクレートでは取得せず、府省が公開した
//! 「意見に対する考え方」テキストのみを対象にする。

use anyhow::{Context, Result};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const BASE_URL: &str = "https://public-comment.e-gov.go.jp";

// ── 公開型 ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseMeta {
    pub case_id: String,
    pub title: String,
    pub ministry: Option<String>,
    pub reception_start: Option<String>,
    pub reception_end: Option<String>,
    pub result_published: Option<String>,
    pub detail_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpinionSummary {
    pub item: String,
    pub opinion: String,
    pub ministry_response: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseDetail {
    pub schema_version: u32,
    pub case_id: String,
    pub title: String,
    pub ministry: Option<String>,
    pub reception_start: Option<String>,
    pub reception_end: Option<String>,
    pub result_published: Option<String>,
    pub related_law_name: Option<String>,
    pub opinions: Vec<OpinionSummary>,
    pub source: CaseSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseSource {
    pub provider: String,
    pub fetched_at: String,
    pub detail_url: String,
}

// ── Provider trait ────────────────────────────────────────────────

pub trait PubcommentProvider: Send + Sync {
    fn fetch_case_list(&self, page: u32) -> Result<Vec<CaseMeta>>;
    fn fetch_case_detail(&self, case_id: &str) -> Result<CaseDetail>;
}

// ── Mock ─────────────────────────────────────────────────────────

pub struct MockProvider;

impl PubcommentProvider for MockProvider {
    fn fetch_case_list(&self, _page: u32) -> Result<Vec<CaseMeta>> {
        Ok(vec![CaseMeta {
            case_id: "2023-00001".to_string(),
            title: "民法の一部を改正する法律案に関するパブリックコメント".to_string(),
            ministry: Some("法務省".to_string()),
            reception_start: Some("2023-06-01".to_string()),
            reception_end: Some("2023-06-30".to_string()),
            result_published: Some("2023-09-01".to_string()),
            detail_url: format!("{BASE_URL}/servlet/Public?CLASSNAME=PCMMSTDETAIL&id=2023-00001"),
        }])
    }

    fn fetch_case_detail(&self, case_id: &str) -> Result<CaseDetail> {
        Ok(CaseDetail {
            schema_version: 1,
            case_id: case_id.to_string(),
            title: "民法の一部を改正する法律案に関するパブリックコメント".to_string(),
            ministry: Some("法務省".to_string()),
            reception_start: Some("2023-06-01".to_string()),
            reception_end: Some("2023-06-30".to_string()),
            result_published: Some("2023-09-01".to_string()),
            related_law_name: Some("民法".to_string()),
            opinions: vec![OpinionSummary {
                item: "第1条関係".to_string(),
                opinion: "基本原則をより明確にすべきである。".to_string(),
                ministry_response: "ご意見を踏まえ、条文の表現を検討します。".to_string(),
            }],
            source: CaseSource {
                provider: "egov_pubcomment".to_string(),
                fetched_at: "2024-01-01T00:00:00Z".to_string(),
                detail_url: format!("{BASE_URL}/servlet/Public?CLASSNAME=PCMMSTDETAIL&id={case_id}"),
            },
        })
    }
}

// ── Http ─────────────────────────────────────────────────────────

pub struct HttpProvider {
    base_url: String,
}

impl HttpProvider {
    pub fn new() -> Self {
        let base_url = std::env::var("LAWPUB_PUBCOMMENT_BASE_URL")
            .unwrap_or_else(|_| BASE_URL.to_string())
            .trim_end_matches('/')
            .to_string();
        Self { base_url }
    }

    fn client() -> Result<reqwest::blocking::Client> {
        reqwest::blocking::Client::builder()
            .user_agent("lawpub/0.1 (+https://github.com/bokuweb/lawrenceanum)")
            .timeout(Duration::from_secs(30))
            .build()
            .context("build reqwest client")
    }

    fn get_html(client: &reqwest::blocking::Client, url: &str) -> Result<String> {
        // robots.txt 準拠: 1 秒待機
        std::thread::sleep(Duration::from_secs(1));
        let resp = client
            .get(url)
            .send()
            .and_then(|r| r.error_for_status())
            .with_context(|| format!("GET {url}"))?;
        resp.text().context("read response text")
    }
}

impl Default for HttpProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl PubcommentProvider for HttpProvider {
    fn fetch_case_list(&self, page: u32) -> Result<Vec<CaseMeta>> {
        let client = Self::client()?;
        // 結果公示済み (Mode=1) の一覧
        let url = format!(
            "{}/servlet/Public?CLASSNAME=PCMMSTLIST&Mode=1&page={}",
            self.base_url, page
        );
        let html = Self::get_html(&client, &url)?;
        parse_case_list(&html, &self.base_url)
    }

    fn fetch_case_detail(&self, case_id: &str) -> Result<CaseDetail> {
        let client = Self::client()?;
        let url = format!(
            "{}/servlet/Public?CLASSNAME=PCMMSTDETAIL&id={}",
            self.base_url, case_id
        );
        let html = Self::get_html(&client, &url)?;
        let fetched_at = chrono::Utc::now().to_rfc3339();
        parse_case_detail(&html, case_id, &url, &fetched_at)
    }
}

// ── HTML パース ───────────────────────────────────────────────────

fn sel(css: &str) -> Selector {
    Selector::parse(css).unwrap_or_else(|_| Selector::parse("*").unwrap())
}

fn text_of(el: &scraper::ElementRef) -> String {
    el.text().collect::<Vec<_>>().join("").trim().to_string()
}

/// 案件一覧 HTML から `CaseMeta` を抽出する。
/// e-Gov の HTML 構造が変わったら selector を更新する。
pub fn parse_case_list(html: &str, base_url: &str) -> Result<Vec<CaseMeta>> {
    let document = Html::parse_document(html);
    let row_sel = sel("table.list tr");
    let td_sel = sel("td");
    let a_sel = sel("a");

    let mut cases = Vec::new();
    for row in document.select(&row_sel).skip(1) {
        let tds: Vec<_> = row.select(&td_sel).collect();
        if tds.len() < 4 {
            continue;
        }
        let link = tds[0].select(&a_sel).next();
        let (case_id, title, detail_url) = if let Some(a) = link {
            let href = a.value().attr("href").unwrap_or("");
            let case_id = href
                .split("id=")
                .nth(1)
                .unwrap_or("")
                .split('&')
                .next()
                .unwrap_or("")
                .to_string();
            let url = if href.starts_with("http") {
                href.to_string()
            } else {
                format!("{base_url}{href}")
            };
            (case_id, text_of(&a), url)
        } else {
            continue;
        };

        if case_id.is_empty() {
            continue;
        }

        cases.push(CaseMeta {
            case_id,
            title,
            ministry: tds.get(1).map(text_of),
            reception_start: tds.get(2).map(text_of),
            reception_end: tds.get(3).map(text_of),
            result_published: tds.get(4).map(text_of),
            detail_url,
        });
    }
    Ok(cases)
}

/// 案件詳細 HTML から `CaseDetail` を抽出する。
pub fn parse_case_detail(
    html: &str,
    case_id: &str,
    url: &str,
    fetched_at: &str,
) -> Result<CaseDetail> {
    let document = Html::parse_document(html);

    let get_meta = |label: &str| -> Option<String> {
        let th_sel = sel("th");
        for th in document.select(&th_sel) {
            if text_of(&th).contains(label) {
                // 同じ tr の td を返す
                if let Some(tr) = th.parent().and_then(|n| scraper::ElementRef::wrap(n)) {
                    let td_sel = sel("td");
                    if let Some(td) = tr.select(&td_sel).next() {
                        let t = text_of(&td);
                        if !t.is_empty() {
                            return Some(t);
                        }
                    }
                }
            }
        }
        None
    };

    let title = get_meta("案件名").or_else(|| get_meta("件名")).unwrap_or_default();
    let ministry = get_meta("所管府省");
    let reception_start = get_meta("意見募集開始日");
    let reception_end = get_meta("意見募集終了日");
    let result_published = get_meta("結果公示日");
    let related_law_name = get_meta("関連法令名").or_else(|| get_meta("法令名"));

    // 意見と府省の考え方テーブル
    let opinion_row_sel = sel("table.opinion tr, table.result tr");
    let td_sel = sel("td");
    let mut opinions = Vec::new();

    for row in document.select(&opinion_row_sel).skip(1) {
        let tds: Vec<_> = row.select(&td_sel).collect();
        if tds.len() < 3 {
            continue;
        }
        opinions.push(OpinionSummary {
            item: text_of(&tds[0]),
            opinion: text_of(&tds[1]),
            ministry_response: text_of(&tds[2]),
        });
    }

    Ok(CaseDetail {
        schema_version: 1,
        case_id: case_id.to_string(),
        title,
        ministry,
        reception_start,
        reception_end,
        result_published,
        related_law_name,
        opinions,
        source: CaseSource {
            provider: "egov_pubcomment".to_string(),
            fetched_at: fetched_at.to_string(),
            detail_url: url.to_string(),
        },
    })
}

// ── テスト ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_provider_returns_list() {
        let p = MockProvider;
        let cases = p.fetch_case_list(1).unwrap();
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].ministry.as_deref(), Some("法務省"));
    }

    #[test]
    fn mock_provider_returns_detail() {
        let p = MockProvider;
        let d = p.fetch_case_detail("2023-00001").unwrap();
        assert_eq!(d.schema_version, 1);
        assert!(!d.opinions.is_empty());
        assert_eq!(d.source.provider, "egov_pubcomment");
    }

    #[test]
    fn parse_case_list_sample_html() {
        let html = r#"<html><body>
<table class="list">
  <tr><th>案件名</th><th>所管府省</th><th>開始日</th><th>終了日</th><th>結果公示日</th></tr>
  <tr>
    <td><a href="/servlet/Public?CLASSNAME=PCMMSTDETAIL&id=2023-00001">民法改正案パブコメ</a></td>
    <td>法務省</td><td>2023-06-01</td><td>2023-06-30</td><td>2023-09-01</td>
  </tr>
</table>
</body></html>"#;
        let cases = parse_case_list(html, BASE_URL).unwrap();
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].case_id, "2023-00001");
        assert_eq!(cases[0].title, "民法改正案パブコメ");
    }

    #[test]
    #[ignore]
    fn http_provider_real_fetch() {
        let p = HttpProvider::new();
        let cases = p.fetch_case_list(1).unwrap();
        println!("{} cases on page 1", cases.len());
    }
}
