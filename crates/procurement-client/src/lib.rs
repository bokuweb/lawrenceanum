//! 官公需情報ポータル (kkj.go.jp) 調達情報クライアント。
//!
//! 認証不要の XML API を利用する。
//! API ガイド: <https://www.kkj.go.jp/doc/ja/api_guide.pdf>
//!
//! ## エンドポイント
//!
//! - 案件検索: `GET http://www.kkj.go.jp/api/` (XML)
//!
//! ## 主パラメータ
//!
//! - `Query`: 検索式（必須）
//! - `CFT_Issue_Date`: 公示日または公示日範囲 (YYYY-MM-DD[/YYYY-MM-DD])
//! - `Count`: 件数 (最大 1,000)

use anyhow::{anyhow, ensure, Context, Result};
use chrono::NaiveDate;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Duration;

pub const BASE_URL: &str = "http://www.kkj.go.jp/api/";
const MATCH_ALL_QUERY: &str = "NOT __lawpub_no_match__";
const MAX_RESULTS_PER_REQUEST: usize = 1_000;

// ── 公開型 ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcurementItem {
    pub schema_version: u32,
    pub item_id: String,
    pub notice_type: String,
    pub title: String,
    pub organization: Option<String>,
    pub publish_date: Option<String>,
    pub deadline: Option<String>,
    pub contract_amount: Option<String>,
    pub contractor: Option<String>,
    pub contract_date: Option<String>,
    pub detail_url: Option<String>,
    pub source: ProcurementSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcurementSource {
    pub provider: String,
    pub fetched_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchBatch {
    pub from: String,
    pub to: String,
    pub items: Vec<ProcurementItem>,
}

// ── Provider trait ────────────────────────────────────────────────

pub trait ProcurementProvider: Send + Sync {
    /// 公告日範囲で案件を取得する。
    fn fetch_range(&self, from: &str, to: &str) -> Result<FetchBatch>;
}

// ── Mock ─────────────────────────────────────────────────────────

pub struct MockProvider;

impl ProcurementProvider for MockProvider {
    fn fetch_range(&self, from: &str, to: &str) -> Result<FetchBatch> {
        Ok(FetchBatch {
            from: from.to_string(),
            to: to.to_string(),
            items: vec![ProcurementItem {
                schema_version: 1,
                item_id: "mock-2024-00001".to_string(),
                notice_type: "入札公告".to_string(),
                title: "○○省庁舎清掃業務委託".to_string(),
                organization: Some("○○省".to_string()),
                publish_date: Some(from.to_string()),
                deadline: Some(to.to_string()),
                contract_amount: Some("1,000,000".to_string()),
                contractor: Some("株式会社テスト".to_string()),
                contract_date: None,
                detail_url: Some("https://www.kkj.go.jp/s/".to_string()),
                source: ProcurementSource {
                    provider: "kkj_go_jp".to_string(),
                    fetched_at: "2024-01-01T00:00:00Z".to_string(),
                },
            }],
        })
    }
}

// ── Http ─────────────────────────────────────────────────────────

pub struct HttpProvider {
    base_url: String,
}

impl HttpProvider {
    pub fn new() -> Self {
        let base_url = std::env::var("LAWPUB_KKJ_BASE_URL")
            .unwrap_or_else(|_| BASE_URL.to_string())
            .trim_end_matches('/')
            .to_string()
            + "/";
        Self { base_url }
    }

    fn client() -> Result<reqwest::blocking::Client> {
        reqwest::blocking::Client::builder()
            .user_agent("lawpub/0.1 (+https://github.com/bokuweb/lawrenceanum)")
            .timeout(Duration::from_secs(30))
            .build()
            .context("build reqwest client")
    }

    fn fetch_day(
        client: &reqwest::blocking::Client,
        base_url: &str,
        date: &str,
        fetched_at: &str,
    ) -> Result<Vec<ProcurementItem>> {
        std::thread::sleep(Duration::from_millis(500));
        let resp = client
            .get(base_url)
            .query(&[
                ("Query", MATCH_ALL_QUERY),
                ("Count", "1000"),
                ("CFT_Issue_Date", date),
            ])
            .send()
            .and_then(|r| r.error_for_status())
            .with_context(|| format!("GET {base_url} (CFT_Issue_Date={date})"))?;
        let bytes = resp.bytes().context("read bytes")?;
        let (items, _) = parse_xml(&bytes, fetched_at)?;
        Ok(items)
    }
}

impl Default for HttpProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcurementProvider for HttpProvider {
    fn fetch_range(&self, from: &str, to: &str) -> Result<FetchBatch> {
        let from_date = NaiveDate::parse_from_str(from, "%Y-%m-%d")
            .with_context(|| format!("invalid from date: {from}"))?;
        let to_date = NaiveDate::parse_from_str(to, "%Y-%m-%d")
            .with_context(|| format!("invalid to date: {to}"))?;
        ensure!(
            from_date <= to_date,
            "from date must be on or before to date"
        );

        let client = Self::client()?;
        let fetched_at = chrono::Utc::now().to_rfc3339();
        let mut items_by_id = BTreeMap::new();
        let mut date = from_date;

        loop {
            let day = date.format("%Y-%m-%d").to_string();
            let items = Self::fetch_day(&client, &self.base_url, &day, &fetched_at)
                .with_context(|| format!("fetch procurement for {day}"))?;
            if items.len() == MAX_RESULTS_PER_REQUEST {
                tracing::warn!(
                    "procurement: {day} reached the API limit of {MAX_RESULTS_PER_REQUEST} items"
                );
            }
            tracing::info!("procurement: date={day} → {} items", items.len());
            for item in items {
                items_by_id.insert(item.item_id.clone(), item);
            }

            if date == to_date {
                break;
            }
            date = date.succ_opt().context("date range overflow")?;
        }

        Ok(FetchBatch {
            from: from.to_string(),
            to: to.to_string(),
            items: items_by_id.into_values().collect(),
        })
    }
}

// ── XML パース ────────────────────────────────────────────────────

/// XML レスポンスをパースして `(items, has_next_page)` を返す。
/// 現行 API の `SearchResult` と、旧 API 用の要素名の両方を受け付ける。
pub fn parse_xml(xml: &[u8], fetched_at: &str) -> Result<(Vec<ProcurementItem>, bool)> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();

    let mut items: Vec<ProcurementItem> = Vec::new();
    let mut current: Option<ProcurementItemBuilder> = None;
    let mut path: Vec<String> = Vec::new();
    let mut text = String::new();
    let mut has_next = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                path.push(name.clone());
                text.clear();
                if matches!(
                    name.as_str(),
                    "SearchResult" | "item" | "notice" | "procurement"
                ) {
                    current = Some(ProcurementItemBuilder::default());
                }
            }
            Ok(Event::Text(t)) => {
                text.push_str(&t.unescape().unwrap_or_default());
            }
            Ok(Event::CData(t)) => {
                text.push_str(&String::from_utf8_lossy(t.as_ref()));
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let val = std::mem::take(&mut text).trim().to_string();

                if let Some(ref mut b) = current {
                    match name.as_str() {
                        "id" | "noticeId" => b.item_id = Some(val.clone()),
                        "ResultId" if b.item_id.is_none() => b.item_id = Some(val.clone()),
                        "Key" => b.item_id = Some(safe_item_id(&val)),
                        "noticeType" | "type" => b.notice_type = Some(val.clone()),
                        "Category" if b.notice_type.is_none() => b.notice_type = Some(val.clone()),
                        "ProcedureType" => b.notice_type = Some(val.clone()),
                        "title" | "subject" => b.title = Some(val.clone()),
                        "ProjectName" => b.title = Some(val.clone()),
                        "organization" | "org" | "institution" => {
                            b.organization = Some(val.clone())
                        }
                        "OrganizationName" => b.organization = Some(val.clone()),
                        "publishDate" | "date" => b.publish_date = Some(val.clone()),
                        "Date" if b.publish_date.is_none() => {
                            b.publish_date = normalized_date(&val)
                        }
                        "CftIssueDate" => b.publish_date = normalized_date(&val),
                        "deadline" | "dueDate" => b.deadline = Some(val.clone()),
                        "PeriodEndTime" => b.deadline = Some(val.clone()),
                        "contractAmount" | "amount" => b.contract_amount = Some(val.clone()),
                        "contractor" | "awardee" => b.contractor = Some(val.clone()),
                        "contractDate" => b.contract_date = Some(val.clone()),
                        "url" | "detailUrl" => b.detail_url = Some(val.clone()),
                        "ExternalDocumentURI" => b.detail_url = Some(val.clone()),
                        _ => {}
                    }
                }

                if matches!(
                    name.as_str(),
                    "SearchResult" | "item" | "notice" | "procurement"
                ) {
                    if let Some(b) = current.take() {
                        if let Some(item) = b.build(fetched_at) {
                            items.push(item);
                        }
                    }
                }
                if name == "hasNextPage" && val == "true" {
                    has_next = true;
                }
                path.pop();
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(anyhow!("XML parse error: {e}"));
            }
            _ => {}
        }
        buf.clear();
    }

    Ok((items, has_next))
}

fn safe_item_id(value: &str) -> String {
    value
        .trim()
        .trim_end_matches('=')
        .replace('/', "_")
        .replace('+', "-")
}

fn normalized_date(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.chars().take(10).collect())
    }
}

#[derive(Default)]
struct ProcurementItemBuilder {
    item_id: Option<String>,
    notice_type: Option<String>,
    title: Option<String>,
    organization: Option<String>,
    publish_date: Option<String>,
    deadline: Option<String>,
    contract_amount: Option<String>,
    contractor: Option<String>,
    contract_date: Option<String>,
    detail_url: Option<String>,
}

impl ProcurementItemBuilder {
    fn build(self, fetched_at: &str) -> Option<ProcurementItem> {
        let item_id = self.item_id.filter(|s| !s.is_empty())?;
        let title = self.title.filter(|s| !s.is_empty())?;
        Some(ProcurementItem {
            schema_version: 1,
            item_id,
            notice_type: self.notice_type.unwrap_or_else(|| "不明".to_string()),
            title,
            organization: self.organization,
            publish_date: self.publish_date,
            deadline: self.deadline,
            contract_amount: self.contract_amount,
            contractor: self.contractor,
            contract_date: self.contract_date,
            detail_url: self.detail_url,
            source: ProcurementSource {
                provider: "kkj_go_jp".to_string(),
                fetched_at: fetched_at.to_string(),
            },
        })
    }
}

// ── テスト ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_provider_returns_batch() {
        let p = MockProvider;
        let b = p.fetch_range("2024-01-01", "2024-01-31").unwrap();
        assert_eq!(b.items.len(), 1);
        assert_eq!(b.items[0].notice_type, "入札公告");
    }

    #[test]
    fn parse_xml_sample() {
        let xml = concat!(
            r#"<?xml version="1.0" encoding="UTF-8"?><results><hasNextPage>false</hasNextPage><item>"#,
            r#"<id>2024-KKJ-00001</id><noticeType>"#,
            "\u{5165}\u{672d}\u{516c}\u{544a}", // 入札公告
            r#"</noticeType><title>"#,
            "\u{5e81}\u{820e}\u{6e05}\u{6383}\u{696d}\u{52d9}\u{59d4}\u{8a17}", // 庁舎清掃業務委託
            r#"</title><organization>"#,
            "\u{30c6}\u{30b9}\u{30c8}\u{7701}", // テスト省
            r#"</organization><publishDate>2024-01-15</publishDate>"#,
            r#"<deadline>2024-02-15</deadline>"#,
            r#"<url>https://www.kkj.go.jp/s/detail/2024-KKJ-00001</url>"#,
            r#"</item></results>"#,
        );
        let (items, has_next) = parse_xml(xml.as_bytes(), "2024-01-01T00:00:00Z").unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].item_id, "2024-KKJ-00001");
        assert!(!has_next);
    }

    #[test]
    fn parse_xml_has_next_page() {
        let xml = r#"<?xml version="1.0"?><results><hasNextPage>true</hasNextPage><item><id>X</id><title>T</title></item></results>"#;
        let (_items, has_next) = parse_xml(xml.as_bytes(), "t").unwrap();
        assert!(has_next);
    }

    #[test]
    fn parse_current_api_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Results><SearchResults><SearchHits>1</SearchHits><SearchResult>
<ResultId>1</ResultId>
<Key><![CDATA[abc/def+=]]></Key>
<ExternalDocumentURI><![CDATA[https://example.go.jp/notice/1]]></ExternalDocumentURI>
<ProjectName>庁舎清掃業務</ProjectName>
<Date>2026-08-11</Date>
<OrganizationName>テスト省</OrganizationName>
<CftIssueDate>2026-08-12T00:00:00</CftIssueDate>
<PeriodEndTime>2026-09-01T17:00:00</PeriodEndTime>
<Category>役務</Category><ProcedureType>一般競争入札</ProcedureType>
</SearchResult></SearchResults></Results>"#;
        let (items, _) = parse_xml(xml.as_bytes(), "2026-08-12T00:00:00Z").unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].item_id, "abc_def-");
        assert_eq!(items[0].title, "庁舎清掃業務");
        assert_eq!(items[0].notice_type, "一般競争入札");
        assert_eq!(items[0].organization.as_deref(), Some("テスト省"));
        assert_eq!(items[0].publish_date.as_deref(), Some("2026-08-12"));
        assert_eq!(items[0].deadline.as_deref(), Some("2026-09-01T17:00:00"));
        assert_eq!(
            items[0].detail_url.as_deref(),
            Some("https://example.go.jp/notice/1")
        );
    }

    #[test]
    fn http_provider_rejects_reversed_range_before_request() {
        let p = HttpProvider::new();
        let error = p.fetch_range("2026-08-12", "2026-08-11").unwrap_err();
        assert!(error
            .to_string()
            .contains("from date must be on or before to date"));
    }

    #[test]
    #[ignore]
    fn http_provider_real_fetch() {
        let p = HttpProvider::new();
        let b = p.fetch_range("2024-01-01", "2024-01-07").unwrap();
        println!("{} items", b.items.len());
    }
}
