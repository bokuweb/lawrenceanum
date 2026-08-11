//! 審議会・委員会議事録スクレイパー。
//!
//! 各府省ウェブサイトに分散しており統一 API がないため、府省ごとのアダプタで
//! 「審議会一覧 → 会議一覧 → 会議詳細 → 添付原本」を辿る。
//!
//! 現在の実サイト対応は法務省 (`moj`)。URL を委員会名から推測せず、公式一覧に
//! 掲載された不透明な URL をそのまま provenance として保持する。

use anyhow::{Context, Result};
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Duration;

pub const MOJ_BASE_URL: &str = "https://www.moj.go.jp";

// ── 公開型 ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommitteeMeta {
    pub committee_id: String,
    pub ministry: String,
    pub title: String,
    pub index_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MinutesMeta {
    pub minutes_id: String,
    pub ministry: String,
    pub committee_id: String,
    pub committee: String,
    pub date: Option<String>,
    pub title: String,
    pub detail_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinutesAttachment {
    pub attachment_id: String,
    /// `minutes_text` / `minutes_pdf` / `material`。
    pub kind: String,
    pub label: String,
    pub source_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fetched_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extracted_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extraction_method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extraction_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinutesDocument {
    pub schema_version: u32,
    pub minutes_id: String,
    pub ministry: String,
    pub committee_id: String,
    pub committee: String,
    pub date: Option<String>,
    pub title: String,
    pub agenda: Option<String>,
    pub summary: Option<String>,
    /// 会議詳細 HTML の可視本文。議事録公開前の議事概要も残す。
    pub body_text: String,
    /// 議事録 TXT、無ければ PDF から抽出した全文。CLI が添付取得後に設定する。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minutes_text: Option<String>,
    pub attachments: Vec<MinutesAttachment>,
    pub source: MinutesSource,
    /// CLI が raw HTML を内容アドレス保存するまでの一時データ。
    #[serde(skip)]
    pub raw_html: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinutesSource {
    pub provider: String,
    pub fetched_at: String,
    pub detail_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_html_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_html_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FetchedAttachment {
    pub bytes: Vec<u8>,
    pub media_type: Option<String>,
    pub fetched_at: String,
}

// ── アダプタ trait ────────────────────────────────────────────────

pub trait MinistryAdapter: Send + Sync {
    fn ministry_id(&self) -> &str;
    fn list_committees(&self) -> Result<Vec<CommitteeMeta>>;
    fn list_minutes(&self, committee: &CommitteeMeta) -> Result<Vec<MinutesMeta>>;
    fn fetch_minutes(&self, meta: &MinutesMeta) -> Result<MinutesDocument>;
    fn fetch_attachment(&self, attachment: &MinutesAttachment) -> Result<FetchedAttachment>;
}

// ── Mock ─────────────────────────────────────────────────────────

pub struct MockAdapter;

impl MinistryAdapter for MockAdapter {
    fn ministry_id(&self) -> &str {
        "mock"
    }

    fn list_committees(&self) -> Result<Vec<CommitteeMeta>> {
        Ok(vec![CommitteeMeta {
            committee_id: "company_law".to_string(),
            ministry: "mock".to_string(),
            title: "法制審議会会社法制部会".to_string(),
            index_url: "https://example.com/shingikai/company_law.html".to_string(),
        }])
    }

    fn list_minutes(&self, committee: &CommitteeMeta) -> Result<Vec<MinutesMeta>> {
        Ok(vec![MinutesMeta {
            minutes_id: "mock_moj_0001".to_string(),
            ministry: "mock".to_string(),
            committee_id: committee.committee_id.clone(),
            committee: committee.title.clone(),
            date: Some("2026-05-27".to_string()),
            title: format!("{} 第1回会議", committee.title),
            detail_url: "https://example.com/shingikai/0001.html".to_string(),
        }])
    }

    fn fetch_minutes(&self, meta: &MinutesMeta) -> Result<MinutesDocument> {
        Ok(MinutesDocument {
            schema_version: 2,
            minutes_id: meta.minutes_id.clone(),
            ministry: meta.ministry.clone(),
            committee_id: meta.committee_id.clone(),
            committee: meta.committee.clone(),
            date: meta.date.clone(),
            title: meta.title.clone(),
            agenda: Some("会社法制の見直し".to_string()),
            summary: Some("株主総会のデジタル化について審議された。".to_string()),
            body_text: "議事概要テスト".to_string(),
            minutes_text: None,
            attachments: vec![MinutesAttachment {
                attachment_id: "minutes".to_string(),
                kind: "minutes_text".to_string(),
                label: "議事録 TXT版".to_string(),
                source_url: "https://example.com/content/minutes.txt".to_string(),
                media_type: None,
                bytes: None,
                sha256: None,
                fetched_at: None,
                raw_path: None,
                extracted_text: None,
                extraction_method: None,
                extraction_error: None,
            }],
            source: MinutesSource {
                provider: "mock".to_string(),
                fetched_at: "2026-05-27T00:00:00Z".to_string(),
                detail_url: meta.detail_url.clone(),
                raw_html_sha256: None,
                raw_html_path: None,
            },
            raw_html: Some("<html><body>mock minutes</body></html>".to_string()),
        })
    }

    fn fetch_attachment(&self, _attachment: &MinutesAttachment) -> Result<FetchedAttachment> {
        Ok(FetchedAttachment {
            bytes: "\u{feff}法制審議会議事録\n株主総会のデジタル化を審議した。"
                .as_bytes()
                .to_vec(),
            media_type: Some("text/plain".to_string()),
            fetched_at: "2026-05-28T00:00:00Z".to_string(),
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
            .unwrap_or_else(|_| MOJ_BASE_URL.to_string())
            .trim_end_matches('/')
            .to_string();
        Self { base_url }
    }

    fn client() -> Result<reqwest::blocking::Client> {
        reqwest::blocking::Client::builder()
            .user_agent("lawpub/0.1 (+https://github.com/bokuweb/lawrenceanum)")
            .timeout(Duration::from_secs(45))
            .build()
            .context("build client")
    }

    fn get_html(client: &reqwest::blocking::Client, url: &str) -> Result<String> {
        std::thread::sleep(Duration::from_millis(500));
        client
            .get(url)
            .send()
            .and_then(|response| response.error_for_status())
            .with_context(|| format!("GET {url}"))?
            .text()
            .context("read HTML")
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

    fn list_committees(&self) -> Result<Vec<CommitteeMeta>> {
        let client = Self::client()?;
        let url = format!("{}/shingikai_index.html", self.base_url);
        let html = Self::get_html(&client, &url)?;
        parse_moj_committee_list(&html, &url)
    }

    fn list_minutes(&self, committee: &CommitteeMeta) -> Result<Vec<MinutesMeta>> {
        let client = Self::client()?;
        let html = Self::get_html(&client, &committee.index_url)?;
        parse_moj_minutes_list(&html, committee)
    }

    fn fetch_minutes(&self, meta: &MinutesMeta) -> Result<MinutesDocument> {
        let client = Self::client()?;
        let html = Self::get_html(&client, &meta.detail_url)?;
        let fetched_at = chrono::Utc::now().to_rfc3339();
        parse_moj_minutes_detail(&html, meta, &fetched_at)
    }

    fn fetch_attachment(&self, attachment: &MinutesAttachment) -> Result<FetchedAttachment> {
        let client = Self::client()?;
        std::thread::sleep(Duration::from_millis(500));
        let response = client
            .get(&attachment.source_url)
            .send()
            .and_then(|response| response.error_for_status())
            .with_context(|| format!("GET {}", attachment.source_url))?;
        let media_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        Ok(FetchedAttachment {
            bytes: response.bytes()?.to_vec(),
            media_type,
            fetched_at: chrono::Utc::now().to_rfc3339(),
        })
    }
}

// ── HTML パース ───────────────────────────────────────────────────

fn selector(value: &str) -> Selector {
    Selector::parse(value).unwrap()
}

fn text_of(element: &ElementRef<'_>) -> String {
    element
        .text()
        .flat_map(str::split_whitespace)
        .collect::<Vec<_>>()
        .join(" ")
}

fn resolve_url(base_url: &str, href: &str) -> Result<String> {
    Ok(reqwest::Url::parse(base_url)
        .with_context(|| format!("parse base URL {base_url}"))?
        .join(href)
        .with_context(|| format!("resolve URL {href}"))?
        .to_string())
}

fn url_stem(url: &str) -> Option<String> {
    reqwest::Url::parse(url)
        .ok()?
        .path_segments()?
        .next_back()?
        .rsplit_once('.')
        .map(|(stem, _)| stem.to_string())
}

pub fn parse_moj_committee_list(html: &str, index_url: &str) -> Result<Vec<CommitteeMeta>> {
    let doc = Html::parse_document(html);
    let mut committees = Vec::new();
    let mut seen = HashSet::new();
    for anchor in doc.select(&selector("#content h3.cnt_ttl03 a[href]")) {
        let href = anchor.value().attr("href").unwrap_or("");
        if !href.contains("/shingi1/") {
            continue;
        }
        let url = resolve_url(index_url, href)?;
        let Some(committee_id) = url_stem(&url) else {
            continue;
        };
        let title = text_of(&anchor);
        if title.is_empty() || !seen.insert(url.clone()) {
            continue;
        }
        committees.push(CommitteeMeta {
            committee_id,
            ministry: "moj".to_string(),
            title,
            index_url: url,
        });
    }
    Ok(committees)
}

pub fn parse_moj_minutes_list(html: &str, committee: &CommitteeMeta) -> Result<Vec<MinutesMeta>> {
    let doc = Html::parse_document(html);
    let mut meetings = Vec::new();
    let mut seen = HashSet::new();
    for anchor in doc.select(&selector("#content a[href]")) {
        let title = text_of(&anchor).replace('\u{200b}', "");
        if !title.contains("会議") || !title.contains("開催") {
            continue;
        }
        let href = anchor.value().attr("href").unwrap_or("");
        if !href.contains("/shingi1/") || !href.to_ascii_lowercase().contains(".html") {
            continue;
        }
        let detail_url = resolve_url(&committee.index_url, href)?;
        if !seen.insert(detail_url.clone()) {
            continue;
        }
        let Some(minutes_id) = url_stem(&detail_url) else {
            continue;
        };
        meetings.push(MinutesMeta {
            minutes_id,
            ministry: committee.ministry.clone(),
            committee_id: committee.committee_id.clone(),
            committee: committee.title.clone(),
            date: wareki_date_in_text(&title),
            title,
            detail_url,
        });
    }
    meetings.sort_by(|a, b| {
        b.date
            .cmp(&a.date)
            .then_with(|| b.minutes_id.cmp(&a.minutes_id))
    });
    Ok(meetings)
}

pub fn parse_moj_minutes_detail(
    html: &str,
    meta: &MinutesMeta,
    fetched_at: &str,
) -> Result<MinutesDocument> {
    let doc = Html::parse_document(html);
    let content = doc
        .select(&selector("#content"))
        .next()
        .context("meeting page has no #content")?;
    let page_heading = content
        .select(&selector("h1.cnt_ttl01"))
        .next()
        .map(|element| text_of(&element))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| meta.title.clone());
    // 一部ページの h1 は「第1回会議」だけなので、一覧側の正式名称の方が情報量が
    // 多ければそちらを安定 title とする。
    let page_title = if meta.title.chars().count() > page_heading.chars().count() {
        meta.title.clone()
    } else {
        page_heading
    };
    let agenda = section_text(&content, "議題");
    let summary = section_text(&content, "議事概要");
    let body_text = text_of(&content);

    let mut attachments = Vec::new();
    let mut seen = HashSet::new();
    for anchor in content.select(&selector("a[href]")) {
        let href = anchor.value().attr("href").unwrap_or("");
        let lower = href.to_ascii_lowercase();
        let supported = [
            ".pdf", ".txt", ".doc", ".docx", ".xls", ".xlsx", ".ppt", ".pptx", ".csv",
        ]
        .iter()
        .any(|extension| {
            lower
                .split(['?', '#'])
                .next()
                .unwrap_or("")
                .ends_with(extension)
        });
        if !supported {
            continue;
        }
        let source_url = resolve_url(&meta.detail_url, href)?;
        if !seen.insert(source_url.clone()) {
            continue;
        }
        let Some(attachment_id) = url_stem(&source_url) else {
            continue;
        };
        let label = text_of(&anchor);
        let kind = if lower.ends_with(".txt") {
            "minutes_text"
        } else if label.contains("ＰＤＦ版") || label.to_ascii_lowercase().contains("pdf版") {
            "minutes_pdf"
        } else {
            "material"
        };
        attachments.push(MinutesAttachment {
            attachment_id,
            kind: kind.to_string(),
            label,
            source_url,
            media_type: None,
            bytes: None,
            sha256: None,
            fetched_at: None,
            raw_path: None,
            extracted_text: None,
            extraction_method: None,
            extraction_error: None,
        });
    }

    Ok(MinutesDocument {
        schema_version: 2,
        minutes_id: meta.minutes_id.clone(),
        ministry: meta.ministry.clone(),
        committee_id: meta.committee_id.clone(),
        committee: meta.committee.clone(),
        date: meta
            .date
            .clone()
            .or_else(|| wareki_date_in_text(&page_title)),
        title: page_title,
        agenda,
        summary,
        body_text,
        minutes_text: None,
        attachments,
        source: MinutesSource {
            provider: format!("shingikai_{}", meta.ministry),
            fetched_at: fetched_at.to_string(),
            detail_url: meta.detail_url.clone(),
            raw_html_sha256: None,
            raw_html_path: None,
        },
        raw_html: Some(html.to_string()),
    })
}

fn section_text(content: &ElementRef<'_>, heading_prefix: &str) -> Option<String> {
    for heading in content.select(&selector("h2")) {
        if !text_of(&heading).contains(heading_prefix) {
            continue;
        }
        let mut parts = Vec::new();
        let mut sibling = heading.next_sibling();
        while let Some(node) = sibling {
            sibling = node.next_sibling();
            let Some(element) = ElementRef::wrap(node) else {
                continue;
            };
            if element.value().name() == "h2" {
                break;
            }
            let text = text_of(&element);
            if !text.is_empty() {
                parts.push(text);
            }
        }
        let value = parts.join("\n");
        return (!value.is_empty()).then_some(value);
    }
    None
}

fn wareki_date_in_text(text: &str) -> Option<String> {
    let normalized: String = text
        .chars()
        .map(|character| match character {
            '０'..='９' => char::from_u32(character as u32 - '０' as u32 + '0' as u32).unwrap(),
            _ => character,
        })
        .collect();
    for (era, base_year) in [("令和", 2018), ("平成", 1988), ("昭和", 1925)] {
        let Some(start) = normalized.find(era) else {
            continue;
        };
        let rest = &normalized[start + era.len()..];
        let (year_text, rest) = rest.split_once('年')?;
        let (month_text, rest) = rest.split_once('月')?;
        let (day_text, _) = rest.split_once('日')?;
        let year = if year_text.trim() == "元" {
            1
        } else {
            year_text.trim().parse::<i32>().ok()?
        };
        let month = month_text.trim().parse::<u32>().ok()?;
        let day = day_text.trim().parse::<u32>().ok()?;
        let date = chrono::NaiveDate::from_ymd_opt(base_year + year, month, day)?;
        return Some(date.format("%Y-%m-%d").to_string());
    }
    None
}

// ── テスト ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const COUNCILS: &str = include_str!("../tests/fixtures/moj_councils.html");
    const COMMITTEE: &str = include_str!("../tests/fixtures/moj_committee.html");
    const MEETING: &str = include_str!("../tests/fixtures/moj_meeting.html");

    #[test]
    fn mock_adapter_list_and_fetch() {
        let adapter = MockAdapter;
        let committees = adapter.list_committees().unwrap();
        let metas = adapter.list_minutes(&committees[0]).unwrap();
        let doc = adapter.fetch_minutes(&metas[0]).unwrap();
        let attachment = adapter.fetch_attachment(&doc.attachments[0]).unwrap();
        assert_eq!(doc.schema_version, 2);
        assert!(!doc.body_text.is_empty());
        assert!(attachment.bytes.starts_with(b"\xef\xbb\xbf"));
    }

    #[test]
    fn parses_moj_official_site_fixtures() {
        let committees =
            parse_moj_committee_list(COUNCILS, "https://www.moj.go.jp/shingikai_index.html")
                .unwrap();
        assert_eq!(committees.len(), 2);
        assert_eq!(committees[1].committee_id, "housei02_003007_00014");
        assert_eq!(
            committees[1].index_url,
            "https://www.moj.go.jp/shingi1/housei02_003007_00014.html"
        );

        let meetings = parse_moj_minutes_list(COMMITTEE, &committees[1]).unwrap();
        assert_eq!(meetings.len(), 2);
        assert_eq!(meetings[0].date.as_deref(), Some("2026-07-22"));
        assert_eq!(meetings[1].minutes_id, "shingi04900001_00338");

        let doc = parse_moj_minutes_detail(MEETING, &meetings[1], "2026-08-11T00:00:00Z").unwrap();
        assert_eq!(doc.date.as_deref(), Some("2026-05-27"));
        assert_eq!(doc.agenda.as_deref(), Some("会社法制の見直しに関する検討"));
        assert!(doc.summary.as_deref().unwrap().contains("デジタル化"));
        assert_eq!(doc.attachments.len(), 3);
        assert_eq!(doc.attachments[0].kind, "minutes_text");
        assert_eq!(doc.attachments[1].kind, "minutes_pdf");
        assert_eq!(doc.attachments[2].kind, "material");
        assert!(doc.attachments[0]
            .source_url
            .starts_with("https://www.moj.go.jp/content/"));
    }

    #[test]
    fn parses_fullwidth_wareki_date() {
        assert_eq!(
            wareki_date_in_text("第16回（令和８年７月２２日開催）").as_deref(),
            Some("2026-07-22")
        );
        assert_eq!(
            wareki_date_in_text("平成１３年２月１６日開催").as_deref(),
            Some("2001-02-16")
        );
    }

    #[test]
    #[ignore]
    fn moj_real_site_contract() {
        let adapter = MojAdapter::new();
        let committees = adapter.list_committees().unwrap();
        println!("{} active councils", committees.len());
        let committee = committees
            .iter()
            .find(|value| value.title.contains("会社法制"))
            .unwrap();
        let meetings = adapter.list_minutes(committee).unwrap();
        println!("{} meetings for {}", meetings.len(), committee.title);
        let doc = adapter.fetch_minutes(&meetings[0]).unwrap();
        println!(
            "latest: {} / {} attachments",
            doc.title,
            doc.attachments.len()
        );
        assert!(!meetings.is_empty());
        assert!(!doc.body_text.is_empty());
        assert!(doc.attachments.iter().any(|value| value.kind == "material"));
    }
}
