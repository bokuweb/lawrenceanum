//! 審議会・委員会議事録スクレイパー。
//!
//! 各府省ウェブサイトに分散しており統一 API がないため、府省ごとのアダプタで
//! 「審議会一覧 → 会議一覧 → 会議詳細 → 添付原本」を辿る。
//!
//! 現在の実サイト対応は法務省 (`moj`)、内閣府 (`cao`)、国土交通省 (`mlit`)、
//! 厚生労働省 (`mhlw`)。URL を委員会名から推測せず、公式一覧から辿った URL を
//! そのまま provenance として保持する。

use anyhow::{Context, Result};
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Duration;

pub const MOJ_BASE_URL: &str = "https://www.moj.go.jp";
pub const CAO_BASE_URL: &str = "https://www.cao.go.jp";
pub const MLIT_BASE_URL: &str = "https://www.mlit.go.jp";
pub const MHLW_BASE_URL: &str = "https://www.mhlw.go.jp";

// ── 公開型 ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommitteeMeta {
    pub committee_id: String,
    pub ministry: String,
    pub title: String,
    pub index_url: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MeetingStatus {
    Scheduled,
    #[default]
    Held,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MinutesMeta {
    pub minutes_id: String,
    pub ministry: String,
    pub committee_id: String,
    pub committee: String,
    pub date: Option<String>,
    #[serde(default)]
    pub status: MeetingStatus,
    pub title: String,
    pub detail_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agenda: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minutes_url: Option<String>,
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
    /// 将来の開催予定は `scheduled`、開催日を迎えた会議は `held`。
    /// 旧キャッシュにはフィールドが無いため、後方互換で `held` を既定値とする。
    #[serde(default)]
    pub status: MeetingStatus,
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
            status: MeetingStatus::Held,
            title: format!("{} 第1回会議", committee.title),
            detail_url: "https://example.com/shingikai/0001.html".to_string(),
            agenda: None,
            minutes_url: None,
        }])
    }

    fn fetch_minutes(&self, meta: &MinutesMeta) -> Result<MinutesDocument> {
        Ok(MinutesDocument {
            schema_version: 3,
            minutes_id: meta.minutes_id.clone(),
            ministry: meta.ministry.clone(),
            committee_id: meta.committee_id.clone(),
            committee: meta.committee.clone(),
            date: meta.date.clone(),
            status: meta.status,
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

// ── 内閣府アダプタ ────────────────────────────────────────────────

/// 内閣府のうち、まず法令改正との接点が多く、会議資料・議事録のHTML構造が
/// 安定している規制改革推進会議（本会議と各WG）を収集する。
pub struct CaoAdapter {
    base_url: String,
}

impl CaoAdapter {
    pub fn new() -> Self {
        let base_url = std::env::var("LAWPUB_CAO_BASE_URL")
            .unwrap_or_else(|_| CAO_BASE_URL.to_string())
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

impl Default for CaoAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl MinistryAdapter for CaoAdapter {
    fn ministry_id(&self) -> &str {
        "cao"
    }

    fn list_committees(&self) -> Result<Vec<CommitteeMeta>> {
        let client = Self::client()?;
        let council_url = format!("{}/council.html", self.base_url);
        let council_html = Self::get_html(&client, &council_url)?;
        let landing = parse_cao_regulatory_council_url(&council_html, &council_url)?;
        let landing_html = Self::get_html(&client, &landing)?;
        let meeting_url = parse_cao_regulatory_meeting_url(&landing_html, &landing)?;
        Ok(vec![CommitteeMeta {
            committee_id: "regulatory_reform".to_string(),
            ministry: "cao".to_string(),
            title: "規制改革推進会議".to_string(),
            index_url: meeting_url,
        }])
    }

    fn list_minutes(&self, committee: &CommitteeMeta) -> Result<Vec<MinutesMeta>> {
        let client = Self::client()?;
        let html = Self::get_html(&client, &committee.index_url)?;
        parse_cao_regulatory_minutes_list(&html, committee)
    }

    fn fetch_minutes(&self, meta: &MinutesMeta) -> Result<MinutesDocument> {
        let client = Self::client()?;
        let html = Self::get_html(&client, &meta.detail_url)?;
        parse_cao_minutes_detail(&html, meta, &chrono::Utc::now().to_rfc3339())
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

// ── 国土交通省アダプタ ────────────────────────────────────────────

/// 国交省の「終了から3か月間」一覧から、現在動いている審議会・分科会・部会を
/// 自動発見する。ローリング一覧から消えた会議も永続キャッシュには残る。
pub struct MlitAdapter {
    base_url: String,
}

impl MlitAdapter {
    pub fn new() -> Self {
        let base_url = std::env::var("LAWPUB_MLIT_BASE_URL")
            .unwrap_or_else(|_| MLIT_BASE_URL.to_string())
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

impl Default for MlitAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl MinistryAdapter for MlitAdapter {
    fn ministry_id(&self) -> &str {
        "mlit"
    }

    fn list_committees(&self) -> Result<Vec<CommitteeMeta>> {
        let client = Self::client()?;
        let url = format!("{}/policy/shingikai/shingikaiList.html", self.base_url);
        let html = Self::get_html(&client, &url)?;
        parse_mlit_committee_list(&html, &url)
    }

    fn list_minutes(&self, committee: &CommitteeMeta) -> Result<Vec<MinutesMeta>> {
        let client = Self::client()?;
        let html = Self::get_html(&client, &committee.index_url)?;
        parse_mlit_minutes_list(&html, committee)
    }

    fn fetch_minutes(&self, meta: &MinutesMeta) -> Result<MinutesDocument> {
        let client = Self::client()?;
        let html = Self::get_html(&client, &meta.detail_url)?;
        parse_mlit_minutes_detail(&html, meta, &chrono::Utc::now().to_rfc3339())
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

// ── 厚生労働省アダプタ ────────────────────────────────────────────

/// 厚労省の審議会一覧で直近に更新された（`new` 表示のある）委員会を自動発見する。
/// 更新対象は日々入れ替わるが、取得済み会議は永続キャッシュに残るため、活動中の
/// 審議会・分科会・部会を低負荷で継続的に積み上げられる。
pub struct MhlwAdapter {
    base_url: String,
}

impl MhlwAdapter {
    pub fn new() -> Self {
        let base_url = std::env::var("LAWPUB_MHLW_BASE_URL")
            .unwrap_or_else(|_| MHLW_BASE_URL.to_string())
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

impl Default for MhlwAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl MinistryAdapter for MhlwAdapter {
    fn ministry_id(&self) -> &str {
        "mhlw"
    }

    fn list_committees(&self) -> Result<Vec<CommitteeMeta>> {
        let client = Self::client()?;
        let url = format!("{}/stf/shingi/indexshingi.html", self.base_url);
        let html = Self::get_html(&client, &url)?;
        parse_mhlw_committee_list(&html, &url)
    }

    fn list_minutes(&self, committee: &CommitteeMeta) -> Result<Vec<MinutesMeta>> {
        let client = Self::client()?;
        let html = Self::get_html(&client, &committee.index_url)?;
        parse_mhlw_minutes_list(&html, committee)
    }

    fn fetch_minutes(&self, meta: &MinutesMeta) -> Result<MinutesDocument> {
        let client = Self::client()?;
        let html = Self::get_html(&client, &meta.detail_url)?;
        parse_mhlw_minutes_detail(&html, meta, &chrono::Utc::now().to_rfc3339())
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

pub fn parse_mlit_committee_list(html: &str, index_url: &str) -> Result<Vec<CommitteeMeta>> {
    let doc = Html::parse_document(html);
    let mut committees = Vec::new();
    let mut seen = HashSet::new();
    for anchor in doc.select(&selector("#contents .topicsList01 a[href]")) {
        let title = text_of(&anchor);
        let href = anchor.value().attr("href").unwrap_or("");
        if title.is_empty() || !href.contains("/policy/shingikai/") {
            continue;
        }
        let index_url = resolve_url(index_url, href)?;
        if !seen.insert(index_url.clone()) {
            continue;
        }
        let Some(committee_id) = url_stem(&index_url) else {
            continue;
        };
        committees.push(CommitteeMeta {
            committee_id,
            ministry: "mlit".to_string(),
            title,
            index_url,
        });
    }
    anyhow::ensure!(
        !committees.is_empty(),
        "MLIT recent council list has no meeting links"
    );
    Ok(committees)
}

pub fn parse_mhlw_committee_list(html: &str, index_url: &str) -> Result<Vec<CommitteeMeta>> {
    let doc = Html::parse_document(html);
    let mut committees = Vec::new();
    let mut seen = HashSet::new();
    for item in doc.select(&selector("#content .m-listLink__link")) {
        if item.select(&selector(".m-icnNew")).next().is_none() {
            continue;
        }
        let Some(anchor) = item.select(&selector("a[href]")).next() else {
            continue;
        };
        let href = anchor.value().attr("href").unwrap_or("");
        if !href.contains("/stf/shingi") {
            continue;
        }
        let index_url = resolve_url(index_url, href)?;
        if !seen.insert(index_url.clone()) {
            continue;
        }
        let Some(committee_id) = url_stem(&index_url) else {
            continue;
        };
        let title = text_of(&anchor);
        if title.is_empty() {
            continue;
        }
        committees.push(CommitteeMeta {
            committee_id,
            ministry: "mhlw".to_string(),
            title,
            index_url,
        });
    }
    anyhow::ensure!(
        !committees.is_empty(),
        "MHLW council list has no recently updated committee links"
    );
    Ok(committees)
}

pub fn parse_mhlw_minutes_list(html: &str, committee: &CommitteeMeta) -> Result<Vec<MinutesMeta>> {
    let doc = Html::parse_document(html);
    let mut meetings = Vec::new();
    let mut seen = HashSet::new();
    for row in doc.select(&selector("#content table.m-tableFlex tbody tr")) {
        let cells: Vec<_> = row.select(&selector(":scope > td")).collect();
        if cells.len() < 5 {
            continue;
        }
        let meeting_number = text_of(&cells[0]);
        let Some(date) = japanese_date_in_text(&text_of(&cells[1])) else {
            continue;
        };
        let agenda = text_of(&cells[2]);
        let minutes_url = cells
            .get(3)
            .and_then(|cell| cell.select(&selector("a[href]")).next())
            .and_then(|anchor| anchor.value().attr("href"))
            .map(|href| resolve_url(&committee.index_url, href))
            .transpose()?;
        let materials_url = cells
            .get(4)
            .and_then(|cell| cell.select(&selector("a[href]")).next())
            .and_then(|anchor| anchor.value().attr("href"))
            .map(|href| resolve_url(&committee.index_url, href))
            .transpose()?;
        let notice_url = cells
            .get(5)
            .and_then(|cell| cell.select(&selector("a[href]")).next())
            .and_then(|anchor| anchor.value().attr("href"))
            .map(|href| resolve_url(&committee.index_url, href))
            .transpose()?;
        let Some(detail_url) = materials_url.or_else(|| minutes_url.clone()).or(notice_url) else {
            continue;
        };
        let number_key = ascii_digits(&meeting_number);
        let suffix = (!number_key.is_empty()).then(|| format!("_{number_key}"));
        let minutes_id = format!(
            "mhlw_{}_{}{}",
            committee.committee_id,
            date.replace('-', ""),
            suffix.unwrap_or_default()
        );
        if !seen.insert(minutes_id.clone()) {
            continue;
        }
        let status = meeting_status_for_date(&date);
        meetings.push(MinutesMeta {
            minutes_id,
            ministry: "mhlw".to_string(),
            committee_id: committee.committee_id.clone(),
            committee: committee.title.clone(),
            date: Some(date),
            status,
            title: format!("{} {}", committee.title, meeting_number),
            detail_url,
            agenda: (!agenda.is_empty() && agenda != "－").then_some(agenda),
            minutes_url,
        });
    }
    meetings.sort_by(|a, b| {
        b.date
            .cmp(&a.date)
            .then_with(|| b.minutes_id.cmp(&a.minutes_id))
    });
    Ok(meetings)
}

pub fn parse_mhlw_minutes_detail(
    html: &str,
    meta: &MinutesMeta,
    fetched_at: &str,
) -> Result<MinutesDocument> {
    let doc = Html::parse_document(html);
    let content = doc
        .select(&selector("main#content"))
        .next()
        .context("MHLW meeting page has no main#content")?;
    let mut attachments = Vec::new();
    let mut seen = HashSet::new();

    if let Some(minutes_url) = &meta.minutes_url {
        seen.insert(minutes_url.clone());
        attachments.push(MinutesAttachment {
            attachment_id: url_stem(minutes_url).unwrap_or_else(|| "minutes".to_string()),
            kind: "minutes_text".to_string(),
            label: "議事録／議事要旨".to_string(),
            source_url: minutes_url.clone(),
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

    for anchor in content.select(&selector("a[href]")) {
        let href = anchor.value().attr("href").unwrap_or("");
        let path = href
            .to_ascii_lowercase()
            .split(['?', '#'])
            .next()
            .unwrap_or("")
            .to_string();
        if ![
            ".pdf", ".doc", ".docx", ".xls", ".xlsx", ".ppt", ".pptx", ".csv",
        ]
        .iter()
        .any(|extension| path.ends_with(extension))
        {
            continue;
        }
        let source_url = resolve_url(&meta.detail_url, href)?;
        if !seen.insert(source_url.clone()) {
            continue;
        }
        attachments.push(MinutesAttachment {
            attachment_id: url_stem(&source_url).unwrap_or_else(|| "material".to_string()),
            kind: "material".to_string(),
            label: text_of(&anchor),
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
        schema_version: 3,
        minutes_id: meta.minutes_id.clone(),
        ministry: meta.ministry.clone(),
        committee_id: meta.committee_id.clone(),
        committee: meta.committee.clone(),
        date: meta.date.clone(),
        status: meta.status,
        title: meta.title.clone(),
        agenda: meta.agenda.clone(),
        summary: None,
        body_text: text_of(&content),
        minutes_text: None,
        attachments,
        source: MinutesSource {
            provider: "shingikai_mhlw".to_string(),
            fetched_at: fetched_at.to_string(),
            detail_url: meta.detail_url.clone(),
            raw_html_sha256: None,
            raw_html_path: None,
        },
        raw_html: Some(html.to_string()),
    })
}

pub fn parse_mlit_minutes_list(html: &str, committee: &CommitteeMeta) -> Result<Vec<MinutesMeta>> {
    let doc = Html::parse_document(html);
    let mut meetings = Vec::new();
    let mut seen = HashSet::new();
    for item in doc.select(&selector("#contents li")) {
        let Some(title_element) = item.select(&selector(":scope > p")).next() else {
            continue;
        };
        let title = text_of(&title_element);
        let Some(date) = japanese_date_in_text(&title) else {
            continue;
        };
        let links: Vec<_> = item.select(&selector(":scope > ul a[href]")).collect();
        let detail = links
            .iter()
            .find(|anchor| text_of(anchor).contains("配布資料"))
            .or_else(|| {
                links
                    .iter()
                    .find(|anchor| text_of(anchor).contains("議事要旨"))
            })
            .or_else(|| {
                links
                    .iter()
                    .find(|anchor| text_of(anchor).contains("開催案内"))
            });
        let Some(detail_href) = detail.and_then(|anchor| anchor.value().attr("href")) else {
            continue;
        };
        let detail_url = resolve_url(&committee.index_url, detail_href)?;
        if !seen.insert(detail_url.clone()) {
            continue;
        }
        let minutes_url = links
            .iter()
            .find(|anchor| text_of(anchor).contains("議事録"))
            .and_then(|anchor| anchor.value().attr("href"))
            .map(|href| resolve_url(&committee.index_url, href))
            .transpose()?;
        meetings.push(MinutesMeta {
            // 配布資料の公開前後で detail URL が「開催案内→資料ページ」と変わっても
            // 同一会議として履歴化できるよう、委員会IDと開催日を安定キーにする。
            minutes_id: format!("mlit_{}_{}", committee.committee_id, date.replace('-', "")),
            ministry: "mlit".to_string(),
            committee_id: committee.committee_id.clone(),
            committee: committee.title.clone(),
            date: Some(date),
            status: MeetingStatus::Held,
            title,
            detail_url,
            agenda: None,
            minutes_url,
        });
    }
    meetings.sort_by(|a, b| {
        b.date
            .cmp(&a.date)
            .then_with(|| b.minutes_id.cmp(&a.minutes_id))
    });
    Ok(meetings)
}

pub fn parse_mlit_minutes_detail(
    html: &str,
    meta: &MinutesMeta,
    fetched_at: &str,
) -> Result<MinutesDocument> {
    let doc = Html::parse_document(html);
    let content = doc
        .select(&selector("#contents"))
        .next()
        .context("MLIT meeting page has no #contents")?;
    let mut attachments = Vec::new();
    let mut seen = HashSet::new();

    if let Some(minutes_url) = &meta.minutes_url {
        seen.insert(minutes_url.clone());
        attachments.push(MinutesAttachment {
            attachment_id: url_stem(minutes_url).unwrap_or_else(|| "minutes".to_string()),
            kind: "minutes_pdf".to_string(),
            label: "議事録".to_string(),
            source_url: minutes_url.clone(),
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

    for anchor in content.select(&selector("a[href]")) {
        let href = anchor.value().attr("href").unwrap_or("");
        let path = href
            .to_ascii_lowercase()
            .split(['?', '#'])
            .next()
            .unwrap_or("")
            .to_string();
        if ![
            ".pdf", ".doc", ".docx", ".xls", ".xlsx", ".ppt", ".pptx", ".csv",
        ]
        .iter()
        .any(|extension| path.ends_with(extension))
        {
            continue;
        }
        let source_url = resolve_url(&meta.detail_url, href)?;
        if !seen.insert(source_url.clone()) {
            continue;
        }
        attachments.push(MinutesAttachment {
            attachment_id: url_stem(&source_url).unwrap_or_else(|| "material".to_string()),
            kind: "material".to_string(),
            label: text_of(&anchor),
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
        schema_version: 3,
        minutes_id: meta.minutes_id.clone(),
        ministry: meta.ministry.clone(),
        committee_id: meta.committee_id.clone(),
        committee: meta.committee.clone(),
        date: meta.date.clone(),
        status: meta.status,
        title: meta.title.clone(),
        agenda: meta.agenda.clone(),
        summary: None,
        body_text: text_of(&content),
        minutes_text: None,
        attachments,
        source: MinutesSource {
            provider: "shingikai_mlit".to_string(),
            fetched_at: fetched_at.to_string(),
            detail_url: meta.detail_url.clone(),
            raw_html_sha256: None,
            raw_html_path: None,
        },
        raw_html: Some(html.to_string()),
    })
}

pub fn parse_cao_regulatory_council_url(html: &str, index_url: &str) -> Result<String> {
    let doc = Html::parse_document(html);
    doc.select(&selector("main a[href]"))
        .find(|anchor| text_of(anchor).trim() == "規制改革推進会議")
        .and_then(|anchor| anchor.value().attr("href"))
        .map(|href| resolve_url(index_url, href))
        .transpose()?
        .context("CAO council list has no regulatory reform council")
}

pub fn parse_cao_regulatory_meeting_url(html: &str, landing_url: &str) -> Result<String> {
    let doc = Html::parse_document(html);
    doc.select(&selector("main a[href]"))
        .find(|anchor| {
            text_of(anchor).contains("規制改革推進会議")
                && anchor.value().attr("href").is_some_and(|href| {
                    href.split(['?', '#'])
                        .next()
                        .unwrap_or("")
                        .ends_with("meeting.html")
                })
        })
        .and_then(|anchor| anchor.value().attr("href"))
        .map(|href| resolve_url(landing_url, href))
        .transpose()?
        .context("CAO regulatory reform landing page has no meeting list")
}

pub fn parse_cao_regulatory_minutes_list(
    html: &str,
    committee: &CommitteeMeta,
) -> Result<Vec<MinutesMeta>> {
    let doc = Html::parse_document(html);
    let mut meetings = Vec::new();
    let mut seen = HashSet::new();

    for heading in doc.select(&selector("#mainContents h3[id], #mainContents h4[id]")) {
        let mut sibling = heading.next_sibling();
        let table = loop {
            let Some(node) = sibling else { break None };
            sibling = node.next_sibling();
            let Some(element) = ElementRef::wrap(node) else {
                continue;
            };
            break (element.value().name() == "table").then_some(element);
        };
        let Some(table) = table else { continue };
        let heading_id = heading.value().attr("id").unwrap_or("general");
        let section_id = heading_id.split('_').next().unwrap_or("general");
        let committee_id = format!("regulatory_reform_{section_id}");
        let committee_title = text_of(&heading).replace(['　', '\u{00a0}'], " ");

        for row in table.select(&selector("tbody tr")) {
            let cells: Vec<_> = row.select(&selector(":scope > td")).collect();
            if cells.len() < 4 {
                continue;
            }
            let meeting_number = text_of(&cells[0]);
            let date = wareki_date_in_text(&text_of(&cells[1]));
            let Some(detail_href) = cells[2]
                .select(&selector("a[href]"))
                .next()
                .and_then(|anchor| anchor.value().attr("href"))
            else {
                continue;
            };
            let detail_url = resolve_url(&committee.index_url, detail_href)?;
            if !seen.insert(detail_url.clone()) {
                continue;
            }
            let minutes_url = cells[3]
                .select(&selector("a[href]"))
                .next()
                .and_then(|anchor| anchor.value().attr("href"))
                .map(|href| resolve_url(&committee.index_url, href))
                .transpose()?;
            let agenda = cells[2]
                .select(&selector(".agenda li"))
                .map(|item| text_of(&item))
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            let meeting_key = reqwest::Url::parse(&detail_url)
                .ok()
                .and_then(|url| {
                    let mut segments = url.path_segments()?.rev();
                    segments.next()?;
                    segments.next().map(str::to_string)
                })
                .unwrap_or_else(|| url_stem(&detail_url).unwrap_or_else(|| meeting_number.clone()));
            meetings.push(MinutesMeta {
                minutes_id: format!("cao_{committee_id}_{meeting_key}"),
                ministry: "cao".to_string(),
                committee_id: committee_id.clone(),
                committee: committee_title.clone(),
                date,
                status: MeetingStatus::Held,
                title: format!("{committee_title} {meeting_number}"),
                detail_url,
                agenda: (!agenda.is_empty()).then_some(agenda),
                minutes_url,
            });
        }
    }

    meetings.sort_by(|a, b| {
        b.date
            .cmp(&a.date)
            .then_with(|| b.minutes_id.cmp(&a.minutes_id))
    });
    Ok(meetings)
}

pub fn parse_cao_minutes_detail(
    html: &str,
    meta: &MinutesMeta,
    fetched_at: &str,
) -> Result<MinutesDocument> {
    let doc = Html::parse_document(html);
    let content = doc
        .select(&selector("#mainContents"))
        .next()
        .or_else(|| doc.select(&selector("main#contents")).next())
        .context("CAO meeting page has no main content")?;
    let mut attachments = Vec::new();
    let mut seen = HashSet::new();

    if let Some(minutes_url) = &meta.minutes_url {
        seen.insert(minutes_url.clone());
        attachments.push(MinutesAttachment {
            attachment_id: url_stem(minutes_url).unwrap_or_else(|| "minutes".to_string()),
            kind: "minutes_pdf".to_string(),
            label: "議事録".to_string(),
            source_url: minutes_url.clone(),
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
        attachments.push(MinutesAttachment {
            attachment_id: url_stem(&source_url).unwrap_or_else(|| "material".to_string()),
            kind: "material".to_string(),
            label: text_of(&anchor),
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
        schema_version: 3,
        minutes_id: meta.minutes_id.clone(),
        ministry: meta.ministry.clone(),
        committee_id: meta.committee_id.clone(),
        committee: meta.committee.clone(),
        date: meta.date.clone(),
        status: meta.status,
        title: meta.title.clone(),
        agenda: meta
            .agenda
            .clone()
            .or_else(|| section_text(&content, "議事")),
        summary: None,
        body_text: text_of(&content),
        minutes_text: None,
        attachments,
        source: MinutesSource {
            provider: "shingikai_cao".to_string(),
            fetched_at: fetched_at.to_string(),
            detail_url: meta.detail_url.clone(),
            raw_html_sha256: None,
            raw_html_path: None,
        },
        raw_html: Some(html.to_string()),
    })
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
            status: MeetingStatus::Held,
            title,
            detail_url,
            agenda: None,
            minutes_url: None,
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
        schema_version: 3,
        minutes_id: meta.minutes_id.clone(),
        ministry: meta.ministry.clone(),
        committee_id: meta.committee_id.clone(),
        committee: meta.committee.clone(),
        date: meta
            .date
            .clone()
            .or_else(|| wareki_date_in_text(&page_title)),
        status: meta.status,
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

fn japanese_date_in_text(text: &str) -> Option<String> {
    if let Some(date) = wareki_date_in_text(text) {
        return Some(date);
    }
    let normalized: String = text
        .chars()
        .map(|character| match character {
            '０'..='９' => char::from_u32(character as u32 - '０' as u32 + '0' as u32).unwrap(),
            _ => character,
        })
        .collect();
    for (start, character) in normalized.char_indices() {
        if !character.is_ascii_digit() {
            continue;
        }
        let rest = &normalized[start..];
        let Some((year_text, rest)) = rest.split_once('年') else {
            continue;
        };
        if year_text.len() != 4 || !year_text.chars().all(|value| value.is_ascii_digit()) {
            continue;
        }
        let Some((month_text, rest)) = rest.split_once('月') else {
            continue;
        };
        let Some((day_text, _)) = rest.split_once('日') else {
            continue;
        };
        let year = year_text.parse::<i32>().ok()?;
        let month = month_text.trim().parse::<u32>().ok()?;
        let day = day_text.trim().parse::<u32>().ok()?;
        let date = chrono::NaiveDate::from_ymd_opt(year, month, day)?;
        return Some(date.format("%Y-%m-%d").to_string());
    }
    None
}

fn ascii_digits(text: &str) -> String {
    text.chars()
        .filter_map(|character| match character {
            '0'..='9' => Some(character),
            '０'..='９' => char::from_u32(character as u32 - '０' as u32 + '0' as u32),
            _ => None,
        })
        .collect()
}

fn meeting_status_for_date(date: &str) -> MeetingStatus {
    let scheduled = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .ok()
        .is_some_and(|value| value > chrono::Utc::now().date_naive());
    if scheduled {
        MeetingStatus::Scheduled
    } else {
        MeetingStatus::Held
    }
}

// ── テスト ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const COUNCILS: &str = include_str!("../tests/fixtures/moj_councils.html");
    const COMMITTEE: &str = include_str!("../tests/fixtures/moj_committee.html");
    const MEETING: &str = include_str!("../tests/fixtures/moj_meeting.html");
    const CAO_COUNCILS: &str = include_str!("../tests/fixtures/cao_councils.html");
    const CAO_LANDING: &str = include_str!("../tests/fixtures/cao_regulatory_landing.html");
    const CAO_MEETINGS: &str = include_str!("../tests/fixtures/cao_regulatory_meetings.html");
    const CAO_AGENDA: &str = include_str!("../tests/fixtures/cao_agenda.html");
    const MLIT_COUNCILS: &str = include_str!("../tests/fixtures/mlit_councils.html");
    const MLIT_COMMITTEE: &str = include_str!("../tests/fixtures/mlit_committee.html");
    const MLIT_MATERIALS: &str = include_str!("../tests/fixtures/mlit_materials.html");
    const MHLW_COUNCILS: &str = include_str!("../tests/fixtures/mhlw_councils.html");
    const MHLW_COMMITTEE: &str = include_str!("../tests/fixtures/mhlw_committee.html");
    const MHLW_MATERIALS: &str = include_str!("../tests/fixtures/mhlw_materials.html");

    #[test]
    fn mock_adapter_list_and_fetch() {
        let adapter = MockAdapter;
        let committees = adapter.list_committees().unwrap();
        let metas = adapter.list_minutes(&committees[0]).unwrap();
        let doc = adapter.fetch_minutes(&metas[0]).unwrap();
        let attachment = adapter.fetch_attachment(&doc.attachments[0]).unwrap();
        assert_eq!(doc.schema_version, 3);
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
    fn classifies_future_meetings_as_scheduled() {
        assert_eq!(
            meeting_status_for_date("2099-09-15"),
            MeetingStatus::Scheduled
        );
        assert_eq!(meeting_status_for_date("2000-01-01"), MeetingStatus::Held);
    }

    #[test]
    fn parses_cao_regulatory_reform_fixtures() {
        let landing =
            parse_cao_regulatory_council_url(CAO_COUNCILS, "https://www.cao.go.jp/council.html")
                .unwrap();
        assert_eq!(landing, "https://www8.cao.go.jp/kisei-kaikaku/index.html");
        let meeting_url = parse_cao_regulatory_meeting_url(CAO_LANDING, &landing).unwrap();
        assert_eq!(
            meeting_url,
            "https://www8.cao.go.jp/kisei-kaikaku/kisei/meeting/meeting.html"
        );
        let committee = CommitteeMeta {
            committee_id: "regulatory_reform".to_string(),
            ministry: "cao".to_string(),
            title: "規制改革推進会議".to_string(),
            index_url: meeting_url,
        };
        let meetings = parse_cao_regulatory_minutes_list(CAO_MEETINGS, &committee).unwrap();
        assert_eq!(meetings.len(), 2);
        assert_eq!(
            meetings[0].minutes_id,
            "cao_regulatory_reform_general_260629"
        );
        assert_eq!(meetings[0].date.as_deref(), Some("2026-06-29"));
        assert_eq!(
            meetings[0].agenda.as_deref(),
            Some("規制改革推進に関する答申（案）について")
        );
        assert_eq!(meetings[1].committee_id, "regulatory_reform_medical");
        assert_eq!(
            meetings[1].minutes_url.as_deref(),
            Some("https://www8.cao.go.jp/kisei-kaikaku/kisei/meeting/wg/2501_02medical/260515/medical12_minutes.pdf")
        );

        let document =
            parse_cao_minutes_detail(CAO_AGENDA, &meetings[0], "2026-08-12T00:00:00Z").unwrap();
        assert_eq!(document.source.provider, "shingikai_cao");
        assert_eq!(document.attachments.len(), 3);
        assert_eq!(document.attachments[0].kind, "minutes_pdf");
        assert_eq!(document.attachments[1].label, "議事次第");
        assert!(document.body_text.contains("規制改革推進に関する答申"));
    }

    #[test]
    fn parses_mlit_official_site_fixtures() {
        let committees =
            parse_mlit_committee_list(MLIT_COUNCILS, "https://www.mlit.go.jp/policy/shingikai/")
                .unwrap();
        assert_eq!(committees.len(), 2);
        assert_eq!(committees[0].committee_id, "s101_kokudo01");
        assert_eq!(
            committees[1].title,
            "第４回インフラマネジメント戦略小委員会"
        );

        let meetings = parse_mlit_minutes_list(MLIT_COMMITTEE, &committees[0]).unwrap();
        assert_eq!(meetings.len(), 2);
        assert_eq!(meetings[0].date.as_deref(), Some("2026-05-19"));
        assert_eq!(meetings[0].minutes_id, "mlit_s101_kokudo01_20260519");
        assert_eq!(
            meetings[1].minutes_url.as_deref(),
            Some("https://www.mlit.go.jp/policy/shingikai/content/002001447.pdf")
        );

        let document =
            parse_mlit_minutes_detail(MLIT_MATERIALS, &meetings[0], "2026-08-12T00:00:00Z")
                .unwrap();
        assert_eq!(document.source.provider, "shingikai_mlit");
        assert_eq!(document.attachments.len(), 2);
        assert_eq!(
            document.attachments[0].label,
            "第28回国土審議会議事次第(PDF形式:42KB)"
        );
        assert!(document.body_text.contains("広域地方計画"));
    }

    #[test]
    fn parses_mhlw_official_site_fixtures() {
        let committees = parse_mhlw_committee_list(
            MHLW_COUNCILS,
            "https://www.mhlw.go.jp/stf/shingi/indexshingi.html",
        )
        .unwrap();
        assert_eq!(committees.len(), 2);
        assert_eq!(committees[0].committee_id, "shingi-hosho_126702");
        assert_eq!(committees[1].title, "労働条件分科会");

        let meetings = parse_mhlw_minutes_list(MHLW_COMMITTEE, &committees[0]).unwrap();
        assert_eq!(meetings.len(), 3);
        assert_eq!(meetings[0].date.as_deref(), Some("2099-09-15"));
        assert_eq!(meetings[0].status, MeetingStatus::Scheduled);
        assert_eq!(meetings[1].date.as_deref(), Some("2026-07-15"));
        assert_eq!(meetings[1].status, MeetingStatus::Held);
        assert_eq!(
            meetings[1].minutes_id,
            "mhlw_shingi-hosho_126702_20260715_123"
        );
        assert_eq!(
            meetings[1].agenda.as_deref(),
            Some("制度改正について その他")
        );
        assert_eq!(
            meetings[2].minutes_url.as_deref(),
            Some("https://www.mhlw.go.jp/stf/newpage_72796.html")
        );

        let document =
            parse_mhlw_minutes_detail(MHLW_MATERIALS, &meetings[2], "2026-08-13T00:00:00Z")
                .unwrap();
        assert_eq!(document.source.provider, "shingikai_mhlw");
        assert_eq!(document.attachments.len(), 3);
        assert_eq!(document.attachments[0].kind, "minutes_text");
        assert_eq!(document.attachments[1].label, "議事次第［81KB］");
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

    #[test]
    #[ignore]
    fn cao_real_site_contract() {
        let adapter = CaoAdapter::new();
        let committees = adapter.list_committees().unwrap();
        assert_eq!(committees.len(), 1);
        let meetings = adapter.list_minutes(&committees[0]).unwrap();
        println!("{} regulatory reform meetings", meetings.len());
        assert!(!meetings.is_empty());
        let document = adapter.fetch_minutes(&meetings[0]).unwrap();
        println!(
            "latest: {} / {} attachments",
            document.title,
            document.attachments.len()
        );
        assert!(document
            .attachments
            .iter()
            .any(|attachment| attachment.kind == "minutes_pdf"));
    }

    #[test]
    #[ignore]
    fn mlit_real_site_contract() {
        let adapter = MlitAdapter::new();
        let committees = adapter.list_committees().unwrap();
        assert!(!committees.is_empty());
        let meetings = adapter.list_minutes(&committees[0]).unwrap();
        println!("{} meetings for {}", meetings.len(), committees[0].title);
        assert!(!meetings.is_empty());
        let document = adapter.fetch_minutes(&meetings[0]).unwrap();
        println!(
            "latest: {} / {} attachments",
            document.title,
            document.attachments.len()
        );
        assert!(!document.attachments.is_empty());
    }

    #[test]
    #[ignore]
    fn mhlw_real_site_contract() {
        let adapter = MhlwAdapter::new();
        let committees = adapter.list_committees().unwrap();
        println!("{} recently updated committees", committees.len());
        assert!(!committees.is_empty());
        let mut verified = 0usize;
        let mut attachment_total = 0usize;
        for committee in &committees {
            let meetings = adapter.list_minutes(committee).unwrap_or_default();
            let Some(meeting) = meetings.first() else {
                println!("no parseable meeting: {}", committee.title);
                continue;
            };
            let document = adapter.fetch_minutes(meeting).unwrap();
            println!(
                "latest: {} / {} attachments",
                document.title,
                document.attachments.len()
            );
            assert!(!document.body_text.is_empty());
            verified += 1;
            attachment_total += document.attachments.len();
        }
        println!("verified {verified} committees / {attachment_total} attachments");
        assert!(
            verified > 0,
            "no recently updated MHLW committee had a meeting"
        );
    }
}
