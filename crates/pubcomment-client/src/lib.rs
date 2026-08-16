//! e-Gov パブリックコメントスクレイパー。
//!
//! 公式 API がないため `public-comment.e-gov.go.jp` の HTML を `scraper` でパースする。
//!
//! ## スクレイプ対象（現行 e-Gov UI = `egovui-*`, 2024 以降）
//!
//! - 案件一覧: `GET /pcm/list?CLASSNAME=PCMMSTLIST&Mode=1&Page={n}` (Mode=1 = 結果公示済み)
//!   → `ul.egovui-list-comment-list > li` の各カードを抽出。詳細遷移はカードの
//!   `.egovui-link-area-cursor` の onClick に埋まる `id={案件番号}` から URL を組む。
//! - 案件詳細: `GET /pcm/1040?CLASSNAME=PCM1040&id={案件番号}&Mode=1`
//!   → `table.egovui-normal-horizontal` の th/td から各属性を読む。
//!
//! ## スコープ
//!
//! 1 リクエストごとに 1 秒以上待機する。提出された意見と府省の考え方の本文は
//! HTML にインラインでは無く PDF 等の添付 (`/pcm/download?seqNo=...`) で公開される。
//! このクレートは添付の取得までを担当し、テキスト抽出・キャッシュは CLI 側で行う。

use anyhow::{Context, Result};
use reqwest::header::{ACCEPT, ACCEPT_LANGUAGE, CONTENT_DISPOSITION, CONTENT_TYPE};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub const BASE_URL: &str = "https://public-comment.e-gov.go.jp";
const DEFAULT_READER_BASE_URL: &str = "https://r.jina.ai";
const MAX_ATTACHMENT_BYTES: u64 = 50 * 1024 * 1024;

// ── 公開型 ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseMeta {
    pub case_id: String,
    pub title: String,
    pub ministry: Option<String>,
    pub reception_start: Option<String>,
    pub reception_end: Option<String>,
    pub result_published: Option<String>,
    /// 一覧/RSSに含まれる分野カテゴリー。
    #[serde(default)]
    pub category: Option<String>,
    /// 一覧/RSSに含まれる所管省庁・部局名等。
    #[serde(default)]
    pub responsible_office: Option<String>,
    /// 結果公示RSSに含まれる提出意見数。
    #[serde(default)]
    pub opinion_count: Option<u32>,
    /// "open" (意見募集中, Mode=0) / "closed" (結果公示済み, Mode=1)。
    #[serde(default)]
    pub status: String,
    pub detail_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpinionSummary {
    pub item: String,
    pub opinion: String,
    pub ministry_response: String,
}

/// 結果公示等の添付ファイル (意見と府省の考え方の本文 PDF など)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub name: String,
    pub url: String,
    /// HTTP Content-Type（パラメータを除いた MIME type）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    /// Content-Disposition から得た原ファイル名（得られた場合）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    /// 取得した原ファイルの SHA-256。URL が同じでも内容差分を検出できる。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    /// pdftotext 等で抽出した全文。構造化された意見/府省回答は下流で生成する。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extracted_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extraction_method: Option<String>,
    /// 未対応形式・スキャン PDF 等でもメタと原本は保存し、理由を残す。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extraction_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fetched_at: Option<String>,
}

/// HTTP から取得した添付原本。永続化とテキスト抽出は利用側が行う。
#[derive(Debug, Clone)]
pub struct FetchedAttachment {
    pub bytes: Vec<u8>,
    pub media_type: Option<String>,
    pub filename: Option<String>,
    pub fetched_at: String,
    /// 原本を取得できず Reader が抽出したテキストの場合に設定する。
    /// この場合、利用側は原本 SHA や原本サイズとして保存してはならない。
    pub extraction_method: Option<String>,
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
    /// 根拠法令から抽出した関連法令名 (例: 「更生保護法」)。法令リンクの主シグナル。
    pub related_law_name: Option<String>,
    /// 分野カテゴリー (例: 「刑事」)。
    #[serde(default)]
    pub category: Option<String>,
    /// 定めようとする命令などの題名 (改正政令・省令名など)。
    #[serde(default)]
    pub command_title: Option<String>,
    /// 根拠法令条項の原文 (例: 「更生保護法第12条第3項…」)。
    #[serde(default)]
    pub legal_basis: Option<String>,
    /// 所管省庁・部局名等 (例: 「法務省保護局総務課」)。
    #[serde(default)]
    pub responsible_office: Option<String>,
    /// 提出意見数。
    #[serde(default)]
    pub opinion_count: Option<u32>,
    /// HTML にインラインで意見概要がある場合のみ。通常は空 (PDF 添付)。
    #[serde(default)]
    pub opinions: Vec<OpinionSummary>,
    /// 結果公示等の添付ファイル。
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    /// "open" (意見募集中) / "closed" (結果公示済み)。
    #[serde(default)]
    pub status: String,
    pub source: CaseSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseSource {
    pub provider: String,
    pub fetched_at: String,
    pub detail_url: String,
}

impl CaseDetail {
    /// 詳細ページを取らずに一覧メタから組む（意見募集中は結果が未公開のため）。
    pub fn from_meta(meta: &CaseMeta, fetched_at: &str) -> Self {
        CaseDetail {
            schema_version: 1,
            case_id: meta.case_id.clone(),
            title: meta.title.clone(),
            ministry: meta.ministry.clone(),
            reception_start: meta.reception_start.clone(),
            reception_end: meta.reception_end.clone(),
            result_published: meta.result_published.clone(),
            related_law_name: None,
            category: meta.category.clone(),
            command_title: None,
            legal_basis: None,
            responsible_office: meta.responsible_office.clone(),
            opinion_count: meta.opinion_count,
            opinions: Vec::new(),
            attachments: Vec::new(),
            status: meta.status.clone(),
            source: CaseSource {
                provider: "egov_pubcomment".to_string(),
                fetched_at: fetched_at.to_string(),
                detail_url: meta.detail_url.clone(),
            },
        }
    }
}

// ── Provider trait ────────────────────────────────────────────────

pub trait PubcommentProvider: Send + Sync {
    /// `mode`: 0=意見募集中, 1=結果公示済み。
    fn fetch_case_list(&self, mode: u8, page: u32) -> Result<Vec<CaseMeta>>;
    fn fetch_case_detail(&self, case_id: &str, mode: u8) -> Result<CaseDetail>;
    fn fetch_attachment(&self, url: &str) -> Result<FetchedAttachment>;
}

/// mode → status 文字列。
pub fn mode_status(mode: u8) -> &'static str {
    if mode == 0 {
        "open"
    } else {
        "closed"
    }
}

// ── URL 生成 ──────────────────────────────────────────────────────

fn list_url(base: &str, mode: u8, page: u32) -> String {
    format!("{base}/pcm/list?CLASSNAME=PCMMSTLIST&Mode={mode}&Page={page}")
}

fn legacy_list_url(base: &str, mode: u8, page: u32) -> String {
    format!("{base}/servlet/Public?CLASSNAME=PCMMSTLIST&Mode={mode}&Page={page}")
}

fn rss_url(base: &str, mode: u8) -> String {
    let feed = if mode == 0 {
        "pcm_list.xml"
    } else {
        "pcm_result.xml"
    };
    format!("{base}/rss/{feed}")
}

fn detail_url_mode(base: &str, case_id: &str, mode: u8) -> String {
    format!("{base}/pcm/1040?CLASSNAME=PCM1040&id={case_id}&Mode={mode}")
}

/// 公式 RSS が掲載している互換 URL。e-Gov の CDN は送信元によって `/pcm/1040`
/// を 403 にすることがある一方、この入口は同じ詳細ページへ遷移できる。
fn rss_detail_url_mode(base: &str, case_id: &str, mode: u8) -> String {
    format!("{base}/servlet/Public?CLASSNAME=PCM1040&id={case_id}&Mode={mode}")
}

/// GitHub-hosted runner が e-Gov CDN に拒否された場合だけ使う公開ページ Reader。
/// 空の `LAWPUB_PUBCOMMENT_READER_BASE_URL` で無効化でき、セルフホスト先にも差替可能。
fn reader_url(source_url: &str) -> Option<String> {
    let base = std::env::var("LAWPUB_PUBCOMMENT_READER_BASE_URL")
        .unwrap_or_else(|_| DEFAULT_READER_BASE_URL.to_string());
    reader_url_with_base(&base, source_url)
}

fn reader_url_with_base(base: &str, source_url: &str) -> Option<String> {
    let base = base.trim().trim_end_matches('/');
    if base.is_empty() {
        return None;
    }
    // Reader URL 自体の query と混同されないよう、取得元 query の区切りを encode する。
    Some(format!("{base}/{}", source_url.replace('&', "%26")))
}

fn detail_url(base: &str, case_id: &str) -> String {
    format!("{base}/pcm/1040?CLASSNAME=PCM1040&id={case_id}&Mode=1")
}

// ── Mock ─────────────────────────────────────────────────────────

pub struct MockProvider;

impl PubcommentProvider for MockProvider {
    fn fetch_case_list(&self, mode: u8, _page: u32) -> Result<Vec<CaseMeta>> {
        Ok(vec![CaseMeta {
            case_id: "300110052".to_string(),
            title: "民法の一部を改正する法律案に関するパブリックコメント".to_string(),
            ministry: Some("法務省".to_string()),
            reception_start: Some("2023-06-01".to_string()),
            reception_end: Some("2023-06-30".to_string()),
            result_published: Some("2023-09-01".to_string()),
            category: Some("民事".to_string()),
            responsible_office: Some("法務省民事局".to_string()),
            opinion_count: Some(1),
            status: mode_status(mode).to_string(),
            detail_url: detail_url_mode(BASE_URL, "300110052", mode),
        }])
    }

    fn fetch_case_detail(&self, case_id: &str, mode: u8) -> Result<CaseDetail> {
        Ok(CaseDetail {
            schema_version: 1,
            case_id: case_id.to_string(),
            title: "民法の一部を改正する法律案に関するパブリックコメント".to_string(),
            ministry: Some("法務省".to_string()),
            reception_start: Some("2023-06-01".to_string()),
            reception_end: Some("2023-06-30".to_string()),
            result_published: Some("2023-09-01".to_string()),
            status: mode_status(mode).to_string(),
            related_law_name: Some("民法".to_string()),
            category: Some("民事".to_string()),
            command_title: Some("民法の一部を改正する法律".to_string()),
            legal_basis: Some("民法第1条".to_string()),
            responsible_office: Some("法務省民事局".to_string()),
            opinion_count: Some(1),
            opinions: vec![OpinionSummary {
                item: "第1条関係".to_string(),
                opinion: "基本原則をより明確にすべきである。".to_string(),
                ministry_response: "ご意見を踏まえ、条文の表現を検討します。".to_string(),
            }],
            attachments: vec![Attachment {
                name: "意見募集結果".to_string(),
                url: "mock://pubcomment/result.txt".to_string(),
                media_type: None,
                filename: None,
                sha256: None,
                bytes: None,
                extracted_text: None,
                extraction_method: None,
                extraction_error: None,
                fetched_at: None,
            }],
            source: CaseSource {
                provider: "egov_pubcomment".to_string(),
                fetched_at: "2024-01-01T00:00:00Z".to_string(),
                detail_url: detail_url(BASE_URL, case_id),
            },
        })
    }

    fn fetch_attachment(&self, _url: &str) -> Result<FetchedAttachment> {
        Ok(FetchedAttachment {
            bytes: "提出意見\n府省の考え方".as_bytes().to_vec(),
            media_type: Some("text/plain".to_string()),
            filename: Some("mock-result.txt".to_string()),
            fetched_at: "2024-01-01T00:00:00Z".to_string(),
            extraction_method: None,
        })
    }
}

// ── Http ─────────────────────────────────────────────────────────

pub struct HttpProvider {
    base_url: String,
    reader_required: AtomicBool,
}

impl HttpProvider {
    pub fn new() -> Self {
        let base_url = std::env::var("LAWPUB_PUBCOMMENT_BASE_URL")
            .unwrap_or_else(|_| BASE_URL.to_string())
            .trim_end_matches('/')
            .to_string();
        Self {
            base_url,
            reader_required: AtomicBool::new(false),
        }
    }

    fn client() -> Result<reqwest::blocking::Client> {
        reqwest::blocking::Client::builder()
            // Akamai が bot を明示した UA を GitHub-hosted runner から拒否するため、
            // 通常ブラウザ相当の UA を使う。取得間隔は get_html 側で 1 秒空ける。
            .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
            .default_headers({
                let mut headers = reqwest::header::HeaderMap::new();
                headers.insert(
                    ACCEPT,
                    "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"
                        .parse()
                        .expect("static Accept header"),
                );
                headers.insert(
                    ACCEPT_LANGUAGE,
                    "ja,en-US;q=0.8,en;q=0.6"
                        .parse()
                        .expect("static Accept-Language header"),
                );
                headers
            })
            .timeout(Duration::from_secs(30))
            .build()
            .context("build reqwest client")
    }

    fn get_html(client: &reqwest::blocking::Client, url: &str) -> Result<String> {
        // 1 秒待機して連続アクセスを避ける。
        std::thread::sleep(Duration::from_secs(1));
        let resp = client
            .get(url)
            .send()
            .and_then(|r| r.error_for_status())
            .with_context(|| format!("GET {url}"))?;
        resp.text().context("read response text")
    }

    fn get_reader_html(client: &reqwest::blocking::Client, source_url: &str) -> Result<String> {
        let url = reader_url(source_url).context("pubcomment reader fallback is disabled")?;
        std::thread::sleep(Duration::from_secs(1));
        let resp = client
            .get(&url)
            .header("X-Return-Format", "html")
            .send()
            .and_then(|r| r.error_for_status())
            .with_context(|| format!("GET reader fallback {url}"))?;
        resp.text().context("read reader HTML")
    }

    fn fetch_reader_attachment(
        client: &reqwest::blocking::Client,
        source_url: &str,
        direct_error: &str,
    ) -> Result<FetchedAttachment> {
        let url = reader_url(source_url).context("pubcomment reader fallback is disabled")?;
        tracing::warn!(
            "pubcomment attachment direct fetch failed; using reader text fallback: {direct_error}"
        );
        std::thread::sleep(Duration::from_secs(1));
        let mut resp = client
            .get(&url)
            .send()
            .and_then(|r| r.error_for_status())
            .with_context(|| format!("GET attachment reader fallback {url}"))?;
        let mut bytes = Vec::new();
        resp.by_ref()
            .take(MAX_ATTACHMENT_BYTES + 1)
            .read_to_end(&mut bytes)
            .context("read attachment reader text")?;
        if bytes.len() as u64 > MAX_ATTACHMENT_BYTES {
            anyhow::bail!("attachment reader text exceeded {MAX_ATTACHMENT_BYTES} bytes");
        }
        if bytes.iter().all(u8::is_ascii_whitespace) {
            anyhow::bail!("attachment reader returned empty text for {source_url}");
        }
        Ok(FetchedAttachment {
            bytes,
            media_type: Some("text/markdown".to_string()),
            filename: None,
            fetched_at: chrono::Utc::now().to_rfc3339(),
            extraction_method: Some("jina-reader".to_string()),
        })
    }
}

impl Default for HttpProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl PubcommentProvider for HttpProvider {
    fn fetch_case_list(&self, mode: u8, page: u32) -> Result<Vec<CaseMeta>> {
        let client = Self::client()?;
        let url = list_url(&self.base_url, mode, page);
        let primary =
            Self::get_html(&client, &url).and_then(|html| parse_case_list(&html, &self.base_url));
        let (mut metas, try_legacy) = match primary {
            Ok(cases) if !cases.is_empty() => (cases, false),
            Ok(cases) if page > 1 => (cases, false),
            Ok(_) => {
                tracing::warn!("pubcomment primary list returned no cases; trying legacy entry");
                (Vec::new(), true)
            }
            Err(error) => {
                tracing::warn!("pubcomment primary list failed ({error:#}); trying legacy entry");
                (Vec::new(), true)
            }
        };

        if try_legacy {
            let legacy_url = legacy_list_url(&self.base_url, mode, page);
            metas = match Self::get_html(&client, &legacy_url)
                .and_then(|html| parse_case_list(&html, &self.base_url))
            {
                Ok(cases) if !cases.is_empty() => cases,
                Ok(_) => {
                    tracing::warn!("pubcomment legacy list returned no cases; trying official RSS");
                    Vec::new()
                }
                Err(error) => {
                    tracing::warn!(
                        "pubcomment legacy list failed ({error:#}); trying official RSS"
                    );
                    Vec::new()
                }
            };
        }

        // e-Gov の HTML 一覧は CDN/WAF が GitHub-hosted runner を 403 にすることがある。
        // 公式 RSS は静的配信で、募集中・結果公示の直近案件を毎日取得する用途に適する。
        // RSS はページングしないため page=1 だけで使い、日次 durable cache へ追記する。
        if metas.is_empty() && page == 1 {
            let feed_url = rss_url(&self.base_url, mode);
            let xml = Self::get_html(&client, &feed_url)
                .with_context(|| format!("GET pubcomment RSS fallback {feed_url}"))?;
            metas = parse_case_rss(&xml, mode)?;
            tracing::info!(
                "pubcomment RSS fallback: mode={mode}, {} cases",
                metas.len()
            );
        }
        // 詳細 URL の Mode と status を揃える。
        for m in metas.iter_mut() {
            m.status = mode_status(mode).to_string();
            if m.detail_url.is_empty() {
                m.detail_url = detail_url_mode(&self.base_url, &m.case_id, mode);
            }
        }
        Ok(metas)
    }

    fn fetch_case_detail(&self, case_id: &str, mode: u8) -> Result<CaseDetail> {
        let client = Self::client()?;
        let mut errors = Vec::new();
        let direct_url = detail_url_mode(&self.base_url, case_id, mode);
        let rss_url = rss_detail_url_mode(&self.base_url, case_id, mode);
        for (source_url, via_reader) in [(direct_url, false), (rss_url, true)] {
            let response = if via_reader {
                Self::get_reader_html(&client, &source_url)
            } else {
                Self::get_html(&client, &source_url)
            };
            let html = match response {
                Ok(html) => html,
                Err(error) => {
                    if !via_reader {
                        self.reader_required.store(true, Ordering::Relaxed);
                    }
                    errors.push(format!("{source_url}: {error:#}"));
                    continue;
                }
            };
            let fetched_at = chrono::Utc::now().to_rfc3339();
            let mut detail =
                match parse_case_detail(&html, case_id, &source_url, &fetched_at, &self.base_url) {
                    Ok(detail) => detail,
                    Err(error) => {
                        errors.push(format!("{source_url}: {error:#}"));
                        continue;
                    }
                };
            // CDN のブロックページが 200 を返しても空の案件として保存しない。
            if detail.title.is_empty()
                && detail.attachments.is_empty()
                && detail.opinion_count.is_none()
            {
                if !via_reader {
                    self.reader_required.store(true, Ordering::Relaxed);
                }
                errors.push(format!(
                    "{source_url}: response did not contain case detail"
                ));
                continue;
            }
            detail.status = mode_status(mode).to_string();
            return Ok(detail);
        }
        anyhow::bail!(
            "all pubcomment detail routes failed for {case_id}: {}",
            errors.join(" | ")
        )
    }

    fn fetch_attachment(&self, url: &str) -> Result<FetchedAttachment> {
        let client = Self::client()?;
        if self.reader_required.load(Ordering::Relaxed) {
            return Self::fetch_reader_attachment(
                &client,
                url,
                "e-Gov detail access was blocked earlier in this run",
            );
        }
        std::thread::sleep(Duration::from_secs(1));
        let direct = client.get(url).send().and_then(|r| r.error_for_status());
        let mut resp = match direct {
            Ok(resp) => resp,
            Err(error) => {
                self.reader_required.store(true, Ordering::Relaxed);
                return Self::fetch_reader_attachment(&client, url, &error.to_string());
            }
        };
        if let Some(n) = resp.content_length() {
            if n > MAX_ATTACHMENT_BYTES {
                anyhow::bail!("attachment too large: {n} bytes (limit={MAX_ATTACHMENT_BYTES})");
            }
        }

        let media_type = resp
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(';').next())
            .map(|v| v.trim().to_ascii_lowercase())
            .filter(|v| !v.is_empty());
        let filename = resp
            .headers()
            .get(CONTENT_DISPOSITION)
            .and_then(|v| v.to_str().ok())
            .and_then(filename_from_content_disposition);

        let mut bytes = Vec::new();
        resp.by_ref()
            .take(MAX_ATTACHMENT_BYTES + 1)
            .read_to_end(&mut bytes)
            .context("read attachment body")?;
        if bytes.len() as u64 > MAX_ATTACHMENT_BYTES {
            anyhow::bail!(
                "attachment exceeded limit while streaming: {} bytes (limit={MAX_ATTACHMENT_BYTES})",
                bytes.len()
            );
        }
        Ok(FetchedAttachment {
            bytes,
            media_type,
            filename,
            fetched_at: chrono::Utc::now().to_rfc3339(),
            extraction_method: None,
        })
    }
}

/// Content-Disposition の filename / RFC 5987 filename* を最小限解釈する。
/// 日本語 filename* は percent-encoding のままでも識別には使えるため、ここでは
/// 外部依存を増やさず値だけを保存する。
fn filename_from_content_disposition(value: &str) -> Option<String> {
    for key in ["filename*=", "filename="] {
        if let Some(raw) = value.split(';').map(str::trim).find_map(|part| {
            part.strip_prefix(key).or_else(|| {
                let lower = part.to_ascii_lowercase();
                lower.strip_prefix(key).map(|_| &part[key.len()..])
            })
        }) {
            let raw = raw.trim_matches('"');
            let raw = raw.split("''").nth(1).unwrap_or(raw);
            if !raw.is_empty() {
                return Some(raw.to_string());
            }
        }
    }
    None
}

// ── HTML パース ───────────────────────────────────────────────────

fn sel(css: &str) -> Selector {
    Selector::parse(css).unwrap_or_else(|_| Selector::parse("*").unwrap())
}

/// 連続する空白 (改行・全角含む) を 1 つに畳んで前後をトリムする。
fn norm_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn text_of(el: &scraper::ElementRef) -> String {
    norm_ws(&el.text().collect::<Vec<_>>().join(""))
}

/// 根拠法令条項の原文から関連法令名を取り出す。
/// 例: 「更生保護法第12条第3項（…）」→「更生保護法」。
/// 「第」が無ければ全体を、句読点・括弧以降は落として返す。
fn law_name_from_legal_basis(raw: &str) -> Option<String> {
    let s = norm_ws(raw);
    if s.is_empty() {
        return None;
    }
    // 「第…条」より前を法令名とみなす。全角/半角どちらの「第」でも切る。
    let head = s.split('第').next().unwrap_or(&s);
    // 括弧・読点以降を除去。
    let head = head
        .split(['（', '(', '、', '，', '　'])
        .next()
        .unwrap_or(head)
        .trim();
    if head.is_empty() {
        None
    } else {
        Some(head.to_string())
    }
}

/// 「法務省保護局総務課」→「法務省」のように先頭の府省名だけを取り出す。
fn ministry_short(office: &str) -> Option<String> {
    let s = norm_ws(office);
    if s.is_empty() {
        return None;
    }
    // 「省」「庁」で終わらない機関は、部局名まで ministry に混ざらないよう先に切る。
    const AUTHORITIES: &[&str] = &[
        "個人情報保護委員会",
        "公正取引委員会",
        "カジノ管理委員会",
        "国家公安委員会",
        "原子力規制委員会",
        "内閣官房",
        "内閣府",
        "会計検査院",
        "人事院",
    ];
    if let Some(authority) = AUTHORITIES
        .iter()
        .find(|authority| s.starts_with(**authority))
    {
        return Some((*authority).to_string());
    }
    // それ以外は最初の「省」または「庁」までを府省名とする。
    for (i, c) in s.char_indices() {
        if c == '省' || c == '庁' {
            return Some(s[..i + c.len_utf8()].to_string());
        }
    }
    Some(s)
}

/// onClick 属性等から `id={数字}` を取り出す。
fn extract_case_id(s: &str) -> Option<String> {
    let after = s.split("id=").nth(1)?;
    let id: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}

#[derive(Debug, Deserialize)]
struct RssDocument {
    #[serde(rename = "item", default)]
    items: Vec<RssItem>,
}

#[derive(Debug, Deserialize)]
struct RssItem {
    #[serde(default)]
    title: String,
    #[serde(default)]
    link: String,
    #[serde(default)]
    description: String,
}

fn rss_description_field(description: &str, label: &str) -> Option<String> {
    description
        .replace("<br />", "<br/>")
        .split("<br/>")
        .find_map(|part| part.trim().strip_prefix(label))
        .map(|value| value.trim_start_matches(['：', ':']).trim())
        .filter(|value| !value.is_empty())
        .map(String::from)
}

fn normalize_rss_date(value: Option<String>) -> Option<String> {
    value.map(|date| date.replace('/', "-"))
}

/// e-Gov 公式 RDF/RSS 1.0 の募集中・結果公示フィードを一覧メタへ正規化する。
pub fn parse_case_rss(xml: &str, mode: u8) -> Result<Vec<CaseMeta>> {
    let feed: RssDocument = quick_xml::de::from_str(xml).context("parse pubcomment RSS")?;
    let mut cases = Vec::new();
    for item in feed.items {
        let Some(case_id) = extract_case_id(&item.link) else {
            continue;
        };
        let responsible_office =
            rss_description_field(&item.description, "問合せ先（所管省庁・部局名等）");
        let ministry = responsible_office.as_deref().and_then(ministry_short);
        let opinion_count = rss_description_field(&item.description, "提出意見数").and_then(|v| {
            v.chars()
                .filter(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse::<u32>()
                .ok()
        });
        cases.push(CaseMeta {
            case_id,
            title: norm_ws(&item.title),
            ministry,
            reception_start: normalize_rss_date(rss_description_field(
                &item.description,
                "案の公示日",
            )),
            reception_end: normalize_rss_date(rss_description_field(
                &item.description,
                "受付締切日時",
            )),
            result_published: normalize_rss_date(rss_description_field(
                &item.description,
                "結果の公示日",
            )),
            category: rss_description_field(&item.description, "カテゴリー"),
            responsible_office,
            opinion_count,
            status: mode_status(mode).to_string(),
            detail_url: item.link,
        });
    }
    Ok(cases)
}

/// 案件一覧 HTML から `CaseMeta` を抽出する (現行 egovui カード構造)。
pub fn parse_case_list(html: &str, base_url: &str) -> Result<Vec<CaseMeta>> {
    let document = Html::parse_document(html);
    let li_sel = sel("ul.egovui-list-comment-list > li");
    let title_sel = sel("h2 a.egovui-link, h2 .egovui-link");
    let status_cursor_sel = sel(".egovui-link-area-cursor");
    let detail_sel = sel(".egovui-comment-detail");
    let span_sel = sel("span");

    let mut cases = Vec::new();
    for li in document.select(&li_sel) {
        // 案件番号 (id) はカード内の遷移要素の onClick に埋まる。
        let case_id = li
            .select(&status_cursor_sel)
            .find_map(|c| c.value().attr("onclick").and_then(extract_case_id));

        // 属性を label→value で集める (募集中カードは 案の公示日/受付締切日時 を持つ)。
        let mut ministry = None;
        let mut result_published = None;
        let mut reception_start = None;
        let mut reception_end = None;
        let mut category = None;
        let mut responsible_office = None;
        let mut opinion_count = None;
        let mut case_id_attr = None;
        for d in li.select(&detail_sel) {
            let full = text_of(&d);
            let label = d
                .select(&span_sel)
                .next()
                .map(|s| text_of(&s))
                .unwrap_or_default();
            let value = norm_ws(full.strip_prefix(&label).unwrap_or(&full));
            match label.as_str() {
                "案件番号" => case_id_attr = Some(value),
                "結果の公示日" | "結果公示日" => result_published = Some(value),
                "案の公示日" => reception_start = Some(value),
                "受付締切日時" => reception_end = Some(value),
                "所管省庁" => ministry = Some(value),
                "カテゴリー" => category = Some(value),
                "所管省庁・部局名等" | "問合せ先（所管省庁・部局名等）" => {
                    responsible_office = Some(value)
                }
                "提出意見数" => {
                    opinion_count = value
                        .chars()
                        .filter(|c| c.is_ascii_digit())
                        .collect::<String>()
                        .parse::<u32>()
                        .ok()
                }
                _ => {}
            }
        }

        let case_id = case_id.or(case_id_attr);
        let case_id = match case_id {
            Some(id) if !id.is_empty() => id,
            _ => continue,
        };

        let title = li
            .select(&title_sel)
            .next()
            .map(|a| text_of(&a))
            .unwrap_or_default();

        cases.push(CaseMeta {
            case_id: case_id.clone(),
            title,
            ministry,
            reception_start,
            reception_end,
            result_published,
            category,
            responsible_office,
            opinion_count,
            status: String::new(),
            detail_url: detail_url(base_url, &case_id),
        });
    }
    Ok(cases)
}

/// 案件詳細 HTML から `CaseDetail` を抽出する (現行 egovui テーブル構造)。
pub fn parse_case_detail(
    html: &str,
    case_id: &str,
    url: &str,
    fetched_at: &str,
    base_url: &str,
) -> Result<CaseDetail> {
    let document = Html::parse_document(html);

    let title = document
        .select(&sel("h1.egovui-article-title"))
        .next()
        .map(|el| text_of(&el))
        .unwrap_or_default();

    // すべての横並びテーブルの行を label(空白除去)→value で集める。
    let mut fields: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let tr_sel = sel("table.egovui-normal-horizontal tr");
    let th_sel = sel("th");
    let td_sel = sel("td");
    for tr in document.select(&tr_sel) {
        let (Some(th), Some(td)) = (tr.select(&th_sel).next(), tr.select(&td_sel).next()) else {
            continue;
        };
        // ラベルは空白を完全に除去して正規化 (「  案件番号  」→「案件番号」)。
        let label: String = text_of(&th)
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let value = text_of(&td);
        fields.entry(label).or_insert(value);
    }
    let get = |k: &str| fields.get(k).filter(|v| !v.is_empty()).cloned();

    let category = get("カテゴリー");
    let command_title = get("定めようとする命令などの題名").or_else(|| get("命令等の題名"));
    let legal_basis = get("根拠法令条項");
    let related_law_name = legal_basis.as_deref().and_then(law_name_from_legal_basis);
    let reception_start = get("案の公示日").or_else(|| get("意見募集開始日"));
    let reception_end = get("受付締切日時").or_else(|| get("意見募集終了日"));
    let result_published = get("結果の公示日");
    let responsible_office = get("（所管省庁・部局名等）")
        .or_else(|| get("所管省庁・部局名等"))
        .or_else(|| get("所管省庁"));
    let ministry = responsible_office.as_deref().and_then(ministry_short);
    let opinion_count = get("提出意見数").and_then(|v| {
        v.chars()
            .filter(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse::<u32>()
            .ok()
    });

    // 添付ファイル (結果公示 PDF 等)。
    let mut attachments = Vec::new();
    for a in document.select(&sel("a.file[href], a[href*=\"/pcm/download\"]")) {
        let href = a.value().attr("href").unwrap_or("");
        if href.is_empty() {
            continue;
        }
        let full = if href.starts_with("http") {
            href.to_string()
        } else {
            format!("{base_url}{href}")
        };
        let name = text_of(&a);
        attachments.push(Attachment {
            name: if name.is_empty() {
                "添付".to_string()
            } else {
                name
            },
            url: full,
            media_type: None,
            filename: None,
            sha256: None,
            bytes: None,
            extracted_text: None,
            extraction_method: None,
            extraction_error: None,
            fetched_at: None,
        });
    }

    let title = if title.is_empty() {
        command_title.clone().unwrap_or_default()
    } else {
        title
    };

    Ok(CaseDetail {
        schema_version: 1,
        case_id: case_id.to_string(),
        title,
        ministry,
        reception_start,
        reception_end,
        result_published,
        related_law_name,
        category,
        command_title,
        legal_basis,
        responsible_office,
        opinion_count,
        opinions: Vec::new(),
        attachments,
        status: String::new(),
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
        let cases = p.fetch_case_list(1, 1).unwrap();
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].ministry.as_deref(), Some("法務省"));
        let detail = CaseDetail::from_meta(&cases[0], "2026-08-11T00:00:00Z");
        assert_eq!(detail.category.as_deref(), Some("民事"));
        assert_eq!(detail.responsible_office.as_deref(), Some("法務省民事局"));
        assert_eq!(detail.opinion_count, Some(1));
    }

    #[test]
    fn rss_detail_route_matches_official_feed_links() {
        assert_eq!(
            rss_detail_url_mode(BASE_URL, "550004317", 1),
            "https://public-comment.e-gov.go.jp/servlet/Public?CLASSNAME=PCM1040&id=550004317&Mode=1"
        );
        assert_eq!(
            reader_url_with_base(
                "https://r.jina.ai",
                "https://public-comment.e-gov.go.jp/servlet/Public?CLASSNAME=PCM1040&id=550004317&Mode=1"
            )
            .as_deref(),
            Some("https://r.jina.ai/https://public-comment.e-gov.go.jp/servlet/Public?CLASSNAME=PCM1040%26id=550004317%26Mode=1")
        );
    }

    #[test]
    fn mock_provider_returns_detail() {
        let p = MockProvider;
        let d = p.fetch_case_detail("300110052", 1).unwrap();
        assert_eq!(d.schema_version, 1);
        assert_eq!(d.source.provider, "egov_pubcomment");
        assert!(d.source.detail_url.contains("/pcm/1040"));
        assert_eq!(d.attachments.len(), 1);
        let fetched = p.fetch_attachment(&d.attachments[0].url).unwrap();
        assert_eq!(fetched.media_type.as_deref(), Some("text/plain"));
        assert!(!fetched.bytes.is_empty());
    }

    #[test]
    fn content_disposition_filename_is_preserved() {
        assert_eq!(
            filename_from_content_disposition("inline; filename*=UTF-8''%E7%B5%90%E6%9E%9C.pdf")
                .as_deref(),
            Some("%E7%B5%90%E6%9E%9C.pdf")
        );
        assert_eq!(
            filename_from_content_disposition("attachment; filename=answer.pdf").as_deref(),
            Some("answer.pdf")
        );
    }

    #[test]
    fn law_name_extraction() {
        assert_eq!(
            law_name_from_legal_basis("更生保護法第12条第3項（…）").as_deref(),
            Some("更生保護法")
        );
        assert_eq!(
            law_name_from_legal_basis("民法第90条").as_deref(),
            Some("民法")
        );
        assert_eq!(
            law_name_from_legal_basis("労働基準法施行規則").as_deref(),
            Some("労働基準法施行規則")
        );
        assert_eq!(law_name_from_legal_basis("  ").as_deref(), None);
    }

    #[test]
    fn ministry_short_works() {
        assert_eq!(
            ministry_short("法務省保護局総務課").as_deref(),
            Some("法務省")
        );
        assert_eq!(
            ministry_short("国土交通省道路局").as_deref(),
            Some("国土交通省")
        );
    }

    #[test]
    fn parse_case_list_egovui_card() {
        // 現行 egovui カード構造の最小再現。
        let html = r#"<html><body>
<ul class="egovui-list-comment-list">
  <li class="egovui-flex-column">
    <h2><a href="javascript:void(0)" class="egovui-link">「更生保護法施行令の一部を改正する政令案」に関する意見募集の結果について</a></h2>
    <span class="egovui-comment-status egovui-badge">結果公示</span>
    <div class="egovui-list-comment-attributes">
      <div class="egovui-link-area-cursor" onClick="document.forms['formDetail'].action='/pcm/1040?CLASSNAME=PCM1040&id=300110052&Mode=1';document.forms['formDetail'].submit(); return false;"></div>
      <div class="egovui-comment-detail"><span>案件番号</span><span>300110052</span></div>
      <div class="egovui-comment-detail"><span>結果の公示日</span>2026年6月19日</div>
      <div class="egovui-comment-detail"><span>所管省庁</span><span>法務省</span></div>
    </div>
  </li>
</ul>
</body></html>"#;
        let cases = parse_case_list(html, BASE_URL).unwrap();
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].case_id, "300110052");
        assert!(cases[0].title.contains("更生保護法施行令"));
        assert_eq!(cases[0].ministry.as_deref(), Some("法務省"));
        assert_eq!(cases[0].result_published.as_deref(), Some("2026年6月19日"));
        assert!(cases[0].detail_url.contains("id=300110052"));
    }

    #[test]
    fn parse_case_detail_egovui_table() {
        let html = r#"<html><body>
<h1 class="egovui-article-title">「更生保護法施行令の一部を改正する政令案」に関する意見募集の結果について</h1>
<table class="egovui-normal-horizontal"><tbody>
  <tr><th> カテゴリー </th><td>刑事</td></tr>
  <tr><th> 案件番号 </th><td>300110052</td></tr>
  <tr><th> 定めようとする命令などの題名 </th><td>更生保護法施行令の一部を改正する政令</td></tr>
  <tr><th> 根拠法令条項 </th><td>更生保護法第１２条第３項（同法第２５条において準用する場合を含む。）</td></tr>
</tbody></table>
<table class="egovui-normal-horizontal"><tbody>
  <tr><th> 案の公示日 </th><td>2026年2月26日</td></tr>
  <tr><th> 受付締切日時 </th><td>2026年3月27日18時0分</td></tr>
  <tr><th> 結果の公示日 </th><td>2026年6月19日</td></tr>
  <tr><th> 提出意見数 </th><td>2</td></tr>
  <tr><th> （所管省庁・部局名等） </th><td>法務省保護局総務課</td></tr>
</tbody></table>
<a class="file" href="/pcm/download?seqNo=0000316383" target="_blank">結果公示</a>
</body></html>"#;
        let d = parse_case_detail(
            html,
            "300110052",
            "http://x/pcm/1040",
            "2026-01-01T00:00:00Z",
            BASE_URL,
        )
        .unwrap();
        assert!(d.title.contains("更生保護法施行令"));
        assert_eq!(d.category.as_deref(), Some("刑事"));
        assert_eq!(d.related_law_name.as_deref(), Some("更生保護法"));
        assert_eq!(d.reception_start.as_deref(), Some("2026年2月26日"));
        assert_eq!(d.reception_end.as_deref(), Some("2026年3月27日18時0分"));
        assert_eq!(d.result_published.as_deref(), Some("2026年6月19日"));
        assert_eq!(d.opinion_count, Some(2));
        assert_eq!(d.ministry.as_deref(), Some("法務省"));
        assert_eq!(d.attachments.len(), 1);
        assert!(d.attachments[0]
            .url
            .contains("/pcm/download?seqNo=0000316383"));
    }

    #[test]
    fn parse_official_rss_fallback() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<rdf:RDF xmlns="http://purl.org/rss/1.0/"
 xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
 <item rdf:about="https://public-comment.e-gov.go.jp/servlet/Public?CLASSNAME=PCM1040&amp;id=145210700&amp;Mode=1">
  <title>電波法関係告示案に係る意見募集の結果について</title>
  <link>https://public-comment.e-gov.go.jp/servlet/Public?CLASSNAME=PCM1040&amp;id=145210700&amp;Mode=1</link>
  <description>結果の公示日：2026/08/10&lt;br/&gt;案の公示日：2026/04/25&lt;br/&gt;受付締切日時：2026/05/29 23:59&lt;br/&gt;提出意見数：3&lt;br/&gt;カテゴリー：電気通信&lt;br/&gt;問合せ先（所管省庁・部局名等）：総務省総合通信基盤局&lt;br/&gt;</description>
 </item>
</rdf:RDF>"#;
        let cases = parse_case_rss(xml, 1).unwrap();
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].case_id, "145210700");
        assert_eq!(cases[0].ministry.as_deref(), Some("総務省"));
        assert_eq!(cases[0].reception_start.as_deref(), Some("2026-04-25"));
        assert_eq!(cases[0].result_published.as_deref(), Some("2026-08-10"));
        assert_eq!(cases[0].category.as_deref(), Some("電気通信"));
        assert_eq!(
            cases[0].responsible_office.as_deref(),
            Some("総務省総合通信基盤局")
        );
        assert_eq!(cases[0].opinion_count, Some(3));
        assert_eq!(cases[0].status, "closed");
    }

    #[test]
    fn ministry_short_handles_authorities_without_sho_or_cho_suffix() {
        assert_eq!(
            ministry_short("内閣府政策統括官（経済安全保障担当）").as_deref(),
            Some("内閣府")
        );
        assert_eq!(
            ministry_short("個人情報保護委員会事務局").as_deref(),
            Some("個人情報保護委員会")
        );
    }

    #[test]
    #[ignore]
    fn http_provider_real_fetch() {
        let p = HttpProvider::new();
        let cases = p.fetch_case_list(1, 1).unwrap();
        println!("{} cases on page 1", cases.len());
        assert!(!cases.is_empty());
        let d = p.fetch_case_detail(&cases[0].case_id, 1).unwrap();
        println!("detail: {} / law={:?}", d.title, d.related_law_name);
        assert!(!d.title.is_empty());
    }
}
