//! 国会 議案情報（法案審議トラッキング）スクレイパー。
//!
//! 衆議院「議案情報」(`itdb_gian.nsf`) から法案の審議経過を取得する。
//! 公式 API は無いため HTML をパースする。robots.txt は不在 (404)。
//!
//! ## 収集方針
//! - 審議経過の事実データに加え、衆議院が公開する提出時法律案・要綱・修正案を
//!   出典 URL / SHA-256 付きで取得する。法律案本文には提出理由も含まれる。
//! - 原 HTML は呼び出し側が内容アドレスで保存できるよう一時的に返すが、配信用 JSON には
//!   抽出本文と provenance のみを収録する。
//! - 衆議院サイトは Shift-JIS。`text_with_charset` で復号する。

use anyhow::{Context, Result};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;

pub const BASE_URL: &str = "https://www.shugiin.go.jp/internet/itdb_gian.nsf/html/gian";
pub const RESOLUTION_BASE_URL: &str = "https://www.sangiin.go.jp/japanese/gianjoho/ketsugi";

// ── 公開型 ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillMeta {
    /// 審議経過ページのファイル名 stem (例: "1DE153E")。
    pub bill_id: String,
    pub session: u32,
    pub title: String,
    /// 一覧上の状態（例: 「衆議院で審議中」「成立」）。
    pub status: Option<String>,
    pub keika_url: String,
    /// 提出時法律案・要綱・修正案を列挙する本文情報ページ。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub honbun_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyValue {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bill {
    pub schema_version: u32,
    pub bill_id: String,
    pub session: u32,
    /// 衆法 / 参法 / 閣法。
    pub bill_type: Option<String>,
    pub number: Option<String>,
    pub title: String,
    pub submitter: Option<String>,
    pub parties: Option<String>,
    /// 付託委員会（衆優先、無ければ参）。
    pub committee: Option<String>,
    /// 審議結果（衆/参のいずれか、無ければ公布で成立判断）。
    pub result: Option<String>,
    pub promulgation_date: Option<String>,
    pub law_num: Option<String>,
    /// 最新の動きの日付 (ISO)。受理/付託/審議結果/公布の最大。新着フィード用。
    pub latest_date: Option<String>,
    /// その最新の動きのラベル（例: 「委員会付託(衆)」「公布」）。
    pub latest_event: Option<String>,
    /// 一覧由来の状態。
    pub status: Option<String>,
    /// 審議経過ページの全項目（KOMOKU/NAIYO）。
    pub fields: Vec<KeyValue>,
    /// 提出時法律案・要綱・修正案。提出理由は通常 `bill_text` の末尾に含まれる。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documents: Vec<BillDocument>,
    pub source: BillSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillDocument {
    /// `bill_text` / `outline` / `amendment`。
    pub kind: String,
    pub label: String,
    pub url: String,
    pub text: String,
    /// 取得した原 HTML の SHA-256。改訂版の同定と重複排除に使う。
    pub sha256: String,
    pub fetched_at: String,
    /// `.cache` からの相対パス。CLI が原本保存後に設定する。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_path: Option<String>,
    /// CLI が原本を保存するための一時データ。配信用 JSON には出さない。
    #[serde(skip)]
    pub raw_html: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillSource {
    pub provider: String,
    pub fetched_at: String,
    pub detail_url: String,
}

/// 参議院の委員会別「附帯決議」一覧から得られる原本メタデータ。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolutionMeta {
    pub resolution_id: String,
    pub session: u32,
    pub chamber: String,
    pub committee: String,
    /// 「○○法律案に対する附帯決議」。
    pub title: String,
    /// 「に対する附帯決議」を除いた議案名部分。複数議案を含む場合も原文のまま。
    pub subject: String,
    pub resolution_date: Option<String>,
    pub source_url: String,
}

/// PDF 原本と取得時 provenance。CLI が内容アドレス保存・全文抽出する。
#[derive(Debug, Clone)]
pub struct FetchedResolution {
    pub bytes: Vec<u8>,
    pub media_type: Option<String>,
    pub fetched_at: String,
}

/// `.cache/gian-resolutions/` と配信用 JSON の安定スキーマ。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplementaryResolution {
    pub schema_version: u32,
    pub resolution_id: String,
    pub session: u32,
    pub chamber: String,
    pub committee: String,
    pub title: String,
    pub subject: String,
    pub resolution_date: Option<String>,
    pub source_url: String,
    pub media_type: Option<String>,
    pub sha256: String,
    pub bytes: u64,
    pub fetched_at: String,
    pub raw_path: String,
    pub extracted_text: Option<String>,
    pub extraction_method: Option<String>,
    pub extraction_error: Option<String>,
}

// ── Provider trait ────────────────────────────────────────────────

pub trait GianProvider: Send + Sync {
    /// `session` 回次の議案一覧。0 を渡すと最新回 (menu.htm)。
    fn list_bills(&self, session: u32) -> Result<Vec<BillMeta>>;
    fn fetch_bill(&self, meta: &BillMeta) -> Result<Bill>;
    fn list_resolutions(&self, session: u32) -> Result<Vec<ResolutionMeta>>;
    fn fetch_resolution(&self, meta: &ResolutionMeta) -> Result<FetchedResolution>;
}

// ── URL ───────────────────────────────────────────────────────────

fn list_url(base: &str, session: u32) -> String {
    if session == 0 {
        format!("{base}/menu.htm")
    } else {
        format!("{base}/kaiji{session}.htm")
    }
}

// ── Mock ──────────────────────────────────────────────────────────

pub struct MockProvider;

impl GianProvider for MockProvider {
    fn list_bills(&self, session: u32) -> Result<Vec<BillMeta>> {
        let s = if session == 0 { 221 } else { session };
        Ok(vec![BillMeta {
            bill_id: "1DE153E".to_string(),
            session: s,
            title: "政治資金規正法の一部を改正する法律案".to_string(),
            status: Some("衆議院で審議中".to_string()),
            keika_url: format!("{BASE_URL}/keika/1DE153E.htm"),
            honbun_url: Some(format!("{BASE_URL}/honbun/g22105001.htm")),
        }])
    }

    fn fetch_bill(&self, meta: &BillMeta) -> Result<Bill> {
        Ok(Bill {
            schema_version: 1,
            bill_id: meta.bill_id.clone(),
            session: meta.session,
            bill_type: Some("衆法".to_string()),
            number: Some("1".to_string()),
            title: meta.title.clone(),
            submitter: Some("落合 貴之君外四名".to_string()),
            parties: Some("国民民主党・無所属クラブ".to_string()),
            committee: Some("政治改革に関する特別".to_string()),
            result: None,
            promulgation_date: None,
            law_num: None,
            latest_date: Some("2026-06-12".to_string()),
            latest_event: Some("委員会付託(衆)".to_string()),
            status: meta.status.clone(),
            fields: vec![
                KeyValue {
                    key: "議案件名".into(),
                    value: meta.title.clone(),
                },
                KeyValue {
                    key: "衆議院付託年月日／衆議院付託委員会".into(),
                    value: "令和 8年 6月12日 ／ 政治改革に関する特別".into(),
                },
            ],
            documents: vec![BillDocument {
                kind: "bill_text".to_string(),
                label: "提出時法律案".to_string(),
                url: format!("{BASE_URL}/honbun/houan/g22105001.htm"),
                text: "政治資金規正法の一部を改正する法律案 理由".to_string(),
                sha256: "mock-sha256".to_string(),
                fetched_at: "2024-01-01T00:00:00Z".to_string(),
                raw_path: None,
                raw_html: Some("<p>政治資金規正法の一部を改正する法律案 理由</p>".to_string()),
            }],
            source: BillSource {
                provider: "shugiin".to_string(),
                fetched_at: "2024-01-01T00:00:00Z".to_string(),
                detail_url: meta.keika_url.clone(),
            },
        })
    }

    fn list_resolutions(&self, session: u32) -> Result<Vec<ResolutionMeta>> {
        let session = if session == 0 { 221 } else { session };
        Ok(vec![ResolutionMeta {
            resolution_id: format!("sangiin-{session}-f065_061601"),
            session,
            chamber: "参議院".to_string(),
            committee: "法務委員会".to_string(),
            title: "政治資金規正法の一部を改正する法律案に対する附帯決議".to_string(),
            subject: "政治資金規正法の一部を改正する法律案".to_string(),
            resolution_date: Some("2026-06-16".to_string()),
            source_url: format!("{RESOLUTION_BASE_URL}/{session}/f065_061601.pdf"),
        }])
    }

    fn fetch_resolution(&self, _meta: &ResolutionMeta) -> Result<FetchedResolution> {
        Ok(FetchedResolution {
            bytes: "政府は、この法律の施行に当たり必要な措置を講ずること。"
                .as_bytes()
                .to_vec(),
            media_type: Some("text/plain; charset=utf-8".to_string()),
            fetched_at: "2026-06-16T00:00:00Z".to_string(),
        })
    }
}

// ── Http (衆議院, Shift-JIS) ───────────────────────────────────────

pub struct HttpProvider {
    base_url: String,
    resolution_base_url: String,
}

impl HttpProvider {
    pub fn new() -> Self {
        let base_url = std::env::var("LAWPUB_GIAN_BASE_URL")
            .unwrap_or_else(|_| BASE_URL.to_string())
            .trim_end_matches('/')
            .to_string();
        let resolution_base_url = std::env::var("LAWPUB_GIAN_RESOLUTION_BASE_URL")
            .unwrap_or_else(|_| RESOLUTION_BASE_URL.to_string())
            .trim_end_matches('/')
            .to_string();
        Self {
            base_url,
            resolution_base_url,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    fn client() -> Result<reqwest::blocking::Client> {
        reqwest::blocking::Client::builder()
            .user_agent("lawpub/0.1 (+https://github.com/bokuweb/lawrenceanum)")
            .timeout(Duration::from_secs(30))
            .build()
            .context("build reqwest client")
    }

    /// Shift-JIS ページを復号して取得する。1 req/sec。
    fn get_sjis(client: &reqwest::blocking::Client, url: &str) -> Result<String> {
        std::thread::sleep(Duration::from_secs(1));
        let resp = client
            .get(url)
            .send()
            .and_then(|r| r.error_for_status())
            .with_context(|| format!("GET {url}"))?;
        resp.text_with_charset("Shift_JIS")
            .context("decode shift_jis")
    }

    fn get_utf8(client: &reqwest::blocking::Client, url: &str) -> Result<String> {
        std::thread::sleep(Duration::from_secs(1));
        client
            .get(url)
            .send()
            .and_then(|r| r.error_for_status())
            .with_context(|| format!("GET {url}"))?
            .text()
            .context("decode utf-8")
    }
}

impl Default for HttpProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl GianProvider for HttpProvider {
    fn list_bills(&self, session: u32) -> Result<Vec<BillMeta>> {
        let client = Self::client()?;
        let url = list_url(&self.base_url, session);
        let html = Self::get_sjis(&client, &url)?;
        parse_bill_list(&html, session, &self.base_url)
    }

    fn fetch_bill(&self, meta: &BillMeta) -> Result<Bill> {
        let client = Self::client()?;
        let html = Self::get_sjis(&client, &meta.keika_url)?;
        let fetched_at = chrono::Utc::now().to_rfc3339();
        let mut bill = parse_keika(&html, meta, &fetched_at)?;

        if let Some(index_url) = &meta.honbun_url {
            match Self::get_sjis(&client, index_url) {
                Ok(index_html) => {
                    for link in parse_document_index(&index_html, index_url)? {
                        match Self::get_sjis(&client, &link.url) {
                            Ok(raw_html) => {
                                let sha256 = format!("{:x}", Sha256::digest(raw_html.as_bytes()));
                                let text = extract_document_text(&raw_html);
                                if text.is_empty() {
                                    tracing::warn!("empty bill document: {}", link.url);
                                    continue;
                                }
                                bill.documents.push(BillDocument {
                                    kind: link.kind,
                                    label: link.label,
                                    url: link.url,
                                    text,
                                    sha256,
                                    fetched_at: fetched_at.clone(),
                                    raw_path: None,
                                    raw_html: Some(raw_html),
                                });
                            }
                            Err(e) => tracing::warn!("skip bill document {}: {e:#}", link.url),
                        }
                    }
                }
                Err(e) => tracing::warn!("skip bill document index {index_url}: {e:#}"),
            }
        }
        Ok(bill)
    }

    fn list_resolutions(&self, session: u32) -> Result<Vec<ResolutionMeta>> {
        let client = Self::client()?;
        let segment = if session == 0 {
            "current".to_string()
        } else {
            session.to_string()
        };
        let url = format!("{}/{segment}/futai_ind.html", self.resolution_base_url);
        let html = Self::get_utf8(&client, &url)?;
        parse_resolution_list(&html, session, &url)
    }

    fn fetch_resolution(&self, meta: &ResolutionMeta) -> Result<FetchedResolution> {
        let client = Self::client()?;
        std::thread::sleep(Duration::from_secs(1));
        let response = client
            .get(&meta.source_url)
            .send()
            .and_then(|r| r.error_for_status())
            .with_context(|| format!("GET {}", meta.source_url))?;
        let media_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let bytes = response.bytes()?.to_vec();
        Ok(FetchedResolution {
            bytes,
            media_type,
            fetched_at: chrono::Utc::now().to_rfc3339(),
        })
    }
}

// ── パース ────────────────────────────────────────────────────────

fn sel(css: &str) -> Selector {
    Selector::parse(css).unwrap_or_else(|_| Selector::parse("*").unwrap())
}

fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn text_of(el: &scraper::ElementRef) -> String {
    norm(&el.text().collect::<Vec<_>>().join(""))
}

/// 議案一覧 (`menu.htm` / `kaiji{N}.htm`) から経過リンク行を抽出する。
pub fn parse_bill_list(html: &str, session: u32, base: &str) -> Result<Vec<BillMeta>> {
    let doc = Html::parse_document(html);
    let row_sel = sel("tr");
    let a_sel = sel("a");
    let td_sel = sel("td");

    let mut bills = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for tr in doc.select(&row_sel) {
        // 行内の「経過」リンク (keika/{ID}.htm) を探す。
        let keika = tr.select(&a_sel).find_map(|a| {
            let href = a.value().attr("href").unwrap_or("");
            href.contains("keika/").then(|| href.to_string())
        });
        let Some(href) = keika else { continue };
        let file = href.rsplit('/').next().unwrap_or("");
        let bill_id = file.trim_end_matches(".htm").to_string();
        if bill_id.is_empty() || !seen.insert(bill_id.clone()) {
            continue;
        }
        let keika_url = format!("{base}/keika/{file}");
        let honbun_url = tr.select(&a_sel).find_map(|a| {
            let href = a.value().attr("href").unwrap_or("");
            if !href.contains("honbun/") {
                return None;
            }
            reqwest::Url::parse(&format!("{base}/menu.htm"))
                .ok()?
                .join(href)
                .ok()
                .map(|u| u.to_string())
        });

        // 件名・状態: 経過/本文リンクのみのセルを除いた td テキスト。
        let cells: Vec<String> = tr
            .select(&td_sel)
            .map(|td| text_of(&td))
            .filter(|t| !t.is_empty() && t != "経過" && t != "本文")
            .collect();
        let title = cells.first().cloned().unwrap_or_default();
        if title.is_empty() {
            continue;
        }
        let status = cells.get(1).cloned();

        bills.push(BillMeta {
            bill_id,
            session,
            title,
            status,
            keika_url,
            honbun_url,
        });
    }
    Ok(bills)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BillDocumentLink {
    pub kind: String,
    pub label: String,
    pub url: String,
}

/// 参議院「附帯決議一覧」の委員会見出しと PDF リンクを安定メタへ変換する。
pub fn parse_resolution_list(
    html: &str,
    requested_session: u32,
    index_url: &str,
) -> Result<Vec<ResolutionMeta>> {
    let doc = Html::parse_document(html);
    let base = reqwest::Url::parse(index_url).context("parse resolution index URL")?;
    let session = if requested_session == 0 {
        doc.select(&sel("h2.title_text, title"))
            .find_map(|el| extract_session_number(&text_of(&el)))
            .context("current resolution page does not contain a Diet session number")?
    } else {
        requested_session
    };
    let mut committee = String::new();
    let mut resolutions = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for element in doc.select(&sel("h3, ul.exp_list_icn02")) {
        if element.value().name() == "h3" {
            committee = text_of(&element);
            continue;
        }
        if committee.is_empty() {
            continue;
        }
        for anchor in element.select(&sel("a")) {
            let Some(href) = anchor.value().attr("href") else {
                continue;
            };
            if !href.to_ascii_lowercase().contains(".pdf") {
                continue;
            }
            let url = base
                .join(href)
                .with_context(|| format!("resolve resolution URL {href}"))?
                .to_string();
            if !seen.insert(url.clone()) {
                continue;
            }
            let label = text_of(&anchor);
            let (title, subject, resolution_date) = parse_resolution_label(&label);
            if title.is_empty() {
                continue;
            }
            let stem = href
                .rsplit('/')
                .next()
                .unwrap_or(href)
                .split('.')
                .next()
                .unwrap_or("");
            if stem.is_empty() {
                continue;
            }
            resolutions.push(ResolutionMeta {
                resolution_id: format!("sangiin-{session}-{stem}"),
                session,
                chamber: "参議院".to_string(),
                committee: committee.clone(),
                title,
                subject,
                resolution_date,
                source_url: url,
            });
        }
    }
    Ok(resolutions)
}

/// Word 等が生成した縦書き PDF の `pdftotext -bbox-layout` XHTML を、
/// 同じ x 座標の文字を上から下へ、列を右から左へ並べて本文に戻す。
/// 横書き PDF を誤変換しないよう、十分な長さの縦列がないページは空として扱う。
pub fn reconstruct_vertical_glyph_text(xhtml: &str) -> String {
    #[derive(Debug)]
    struct Glyph {
        x: f32,
        y: f32,
        text: String,
    }

    let doc = Html::parse_document(xhtml);
    let page_selector = sel("page");
    let word_selector = sel("word");
    let mut pages = Vec::new();
    for page in doc.select(&page_selector) {
        let height = page
            .value()
            .attr("height")
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(842.0);
        let mut glyphs: Vec<Glyph> = page
            .select(&word_selector)
            .filter_map(|word| {
                let x = word.value().attr("xmin")?.parse::<f32>().ok()?;
                let y = word.value().attr("ymin")?.parse::<f32>().ok()?;
                if !(50.0..=(height - 25.0)).contains(&y) {
                    return None;
                }
                let text = text_of(&word);
                (!text.is_empty()).then_some(Glyph { x, y, text })
            })
            .collect();
        glyphs.sort_by(|a, b| {
            b.x.partial_cmp(&a.x)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal))
        });

        let mut columns: Vec<Vec<Glyph>> = Vec::new();
        for glyph in glyphs {
            if let Some(column) = columns
                .last_mut()
                .filter(|column| (column[0].x - glyph.x).abs() <= 1.0)
            {
                column.push(glyph);
            } else {
                columns.push(vec![glyph]);
            }
        }
        if columns.iter().map(Vec::len).max().unwrap_or(0) < 8 {
            continue;
        }
        let lines: Vec<String> = columns
            .into_iter()
            .filter_map(|mut column| {
                column.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));
                let line = column
                    .into_iter()
                    .map(|glyph| glyph.text)
                    .collect::<String>();
                (!line.trim().is_empty()).then_some(line)
            })
            .collect();
        if !lines.is_empty() {
            pages.push(lines.join("\n"));
        }
    }
    pages.join("\n\n").trim().to_string()
}

fn extract_session_number(text: &str) -> Option<u32> {
    let start = text.find('第')? + '第'.len_utf8();
    let rest = &text[start..];
    let end = rest.find('回')?;
    jp_num(&rest[..end])
}

fn parse_resolution_label(label: &str) -> (String, String, Option<String>) {
    let label = label.trim().strip_suffix("（PDF）").unwrap_or(label.trim());
    let date_start = ["（令和", "（平成", "（昭和", "（大正", "（明治"]
        .iter()
        .filter_map(|marker| label.rfind(marker))
        .max();
    let (title, date) = if let Some(start) = date_start {
        let date_text = label[start + '（'.len_utf8()..]
            .split('）')
            .next()
            .unwrap_or("");
        (label[..start].trim(), wareki_to_iso(date_text))
    } else {
        (label, None)
    };
    let subject = title
        .strip_suffix("に対する附帯決議")
        .unwrap_or(title)
        .trim()
        .to_string();
    (title.to_string(), subject, date)
}

/// 本文情報一覧から、LLM の根拠資料になる法律案・要綱・修正案だけを列挙する。
pub fn parse_document_index(html: &str, index_url: &str) -> Result<Vec<BillDocumentLink>> {
    let doc = Html::parse_document(html);
    let base = reqwest::Url::parse(index_url).context("parse bill document index URL")?;
    let mut links = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for a in doc.select(&sel("a")) {
        let Some(href) = a.value().attr("href") else {
            continue;
        };
        let lower = href.to_ascii_lowercase();
        let kind = if lower.contains("/houan/") {
            "bill_text"
        } else if lower.contains("/youkou/") {
            "outline"
        } else if lower.contains("/syuuseian/") || lower.contains("/shuseian/") {
            "amendment"
        } else {
            continue;
        };
        let url = base
            .join(href)
            .with_context(|| format!("resolve bill document URL {href}"))?
            .to_string();
        if !seen.insert(url.clone()) {
            continue;
        }
        let label = text_of(&a);
        links.push(BillDocumentLink {
            kind: kind.to_string(),
            label: if label.is_empty() {
                kind.to_string()
            } else {
                label
            },
            url,
        });
    }
    Ok(links)
}

/// ヘッダー・フッターを除き、文書本体の段落を改行区切りで抽出する。
pub fn extract_document_text(html: &str) -> String {
    let doc = Html::parse_document(html);
    let mut lines = Vec::new();
    for el in doc.select(&sel(
        "#mainlayout p, #mainlayout h1, #mainlayout h2, #mainlayout h3",
    )) {
        let text = text_of(&el);
        if !text.is_empty() && lines.last().is_none_or(|last| last != &text) {
            lines.push(text);
        }
    }
    lines.join("\n")
}

/// 「日付／委員会」「日付／結果」形式の値から ／ 以降を返す。
fn after_slash(v: &str) -> Option<String> {
    let parts: Vec<&str> = v.split('／').collect();
    if parts.len() >= 2 {
        let t = parts[1].trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    None
}

/// 「8」「元」「６」等 (全角/半角/元年) を数値に。
fn jp_num(s: &str) -> Option<u32> {
    let t = s.trim();
    if t == "元" {
        return Some(1);
    }
    let digits: String = t
        .chars()
        .filter_map(|c| {
            if c.is_ascii_digit() {
                Some(c)
            } else if ('０'..='９').contains(&c) {
                char::from_u32(c as u32 - '０' as u32 + '0' as u32)
            } else {
                None
            }
        })
        .collect();
    digits.parse().ok()
}

/// 「令和 8年 6月12日」→「2026-06-12」。変換不能は None。
fn wareki_to_iso(s: &str) -> Option<String> {
    let s = s.trim();
    let eras = [
        ("令和", 2018),
        ("平成", 1988),
        ("昭和", 1925),
        ("大正", 1911),
        ("明治", 1867),
    ];
    let (rest, base) = eras
        .iter()
        .find_map(|(e, b)| s.strip_prefix(e).map(|r| (r, *b)))?;
    let yi = rest.find('年')?;
    let year = jp_num(&rest[..yi])? as i32 + base;
    let after_y = &rest[yi + '年'.len_utf8()..];
    let mi = after_y.find('月')?;
    let month = jp_num(&after_y[..mi])?;
    let after_m = &after_y[mi + '月'.len_utf8()..];
    let di = after_m.find('日')?;
    let day = jp_num(&after_m[..di])?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

/// `／` 区切りの先頭（日付部分）を返す。区切りが無ければ全体。
fn before_slash(v: &str) -> &str {
    v.split('／').next().unwrap_or(v).trim()
}

/// 受理/付託/審議結果/公布のうち最も新しい日付(ISO)とそのイベント名を返す。
fn latest_event(fields: &[KeyValue]) -> (Option<String>, Option<String>) {
    let get = |k: &str| fields.iter().find(|f| f.key == k).map(|f| f.value.as_str());
    // (フィールド名, イベントラベル)
    let candidates = [
        ("公布年月日／法律番号", "公布"),
        ("衆議院審議終了年月日／衆議院審議結果", "審議終了(衆)"),
        ("参議院審議終了年月日／参議院審議結果", "審議終了(参)"),
        ("衆議院付託年月日／衆議院付託委員会", "委員会付託(衆)"),
        ("参議院付託年月日／参議院付託委員会", "委員会付託(参)"),
        ("衆議院議案受理年月日", "受理(衆)"),
        ("参議院議案受理年月日", "受理(参)"),
    ];
    let mut best: Option<(String, &str)> = None;
    for (key, label) in candidates {
        let Some(raw) = get(key) else { continue };
        let Some(iso) = wareki_to_iso(before_slash(raw)) else {
            continue;
        };
        if best.as_ref().map(|(d, _)| iso > *d).unwrap_or(true) {
            best = Some((iso, label));
        }
    }
    match best {
        Some((iso, label)) => (Some(iso), Some(label.to_string())),
        None => (None, None),
    }
}

/// 審議経過ページ (`keika/{ID}.htm`) を事実フィールドに構造化する。
pub fn parse_keika(html: &str, meta: &BillMeta, fetched_at: &str) -> Result<Bill> {
    let doc = Html::parse_document(html);
    let komoku_sel = sel(r#"td[headers="KOMOKU"]"#);
    let naiyo_sel = sel(r#"td[headers="NAIYO"]"#);

    // KOMOKU と NAIYO は行ごとに 1 対 1。文書順で zip する。
    let keys: Vec<String> = doc.select(&komoku_sel).map(|e| text_of(&e)).collect();
    let vals: Vec<String> = doc.select(&naiyo_sel).map(|e| text_of(&e)).collect();
    let fields: Vec<KeyValue> = keys
        .into_iter()
        .zip(vals)
        .filter(|(k, _)| !k.is_empty())
        .map(|(key, value)| KeyValue { key, value })
        .collect();

    let get = |k: &str| {
        fields
            .iter()
            .find(|f| f.key == k)
            .map(|f| f.value.clone())
            .filter(|v| !v.is_empty())
    };

    let bill_type = get("議案種類");
    let number = get("議案番号");
    // 実際の会期は審議経過の「議案提出回次」。一覧 (最新=0) の引数より優先する。
    let session = get("議案提出回次")
        .and_then(|v| {
            v.chars()
                .filter(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse::<u32>()
                .ok()
        })
        .unwrap_or(meta.session);
    let title = get("議案件名").unwrap_or_else(|| meta.title.clone());
    let submitter = get("議案提出者");
    let parties = get("議案提出会派");
    // 付託委員会: 衆優先、無ければ参。「日付／委員会」の ／ 以降。
    let committee = get("衆議院付託年月日／衆議院付託委員会")
        .and_then(|v| after_slash(&v))
        .or_else(|| get("参議院付託年月日／参議院付託委員会").and_then(|v| after_slash(&v)));
    // 審議結果: 衆審議結果→参審議結果。
    let result = get("衆議院審議終了年月日／衆議院審議結果")
        .and_then(|v| after_slash(&v))
        .or_else(|| get("参議院審議終了年月日／参議院審議結果").and_then(|v| after_slash(&v)));
    // 公布年月日／法律番号。
    let kofu = get("公布年月日／法律番号");
    let promulgation_date = kofu
        .as_deref()
        .map(|v| v.split('／').next().unwrap_or("").trim().to_string())
        .filter(|s| !s.is_empty());
    let law_num = kofu.as_deref().and_then(after_slash);

    let (latest_date, latest_ev) = latest_event(&fields);

    Ok(Bill {
        schema_version: 1,
        bill_id: meta.bill_id.clone(),
        session,
        bill_type,
        number,
        title,
        submitter,
        parties,
        committee,
        result,
        promulgation_date,
        law_num,
        latest_date,
        latest_event: latest_ev,
        status: meta.status.clone(),
        fields,
        documents: Vec::new(),
        source: BillSource {
            provider: "shugiin".to_string(),
            fetched_at: fetched_at.to_string(),
            detail_url: meta.keika_url.clone(),
        },
    })
}

// ── テスト ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_list_and_fetch() {
        let p = MockProvider;
        let bills = p.list_bills(0).unwrap();
        assert_eq!(bills.len(), 1);
        let b = p.fetch_bill(&bills[0]).unwrap();
        assert_eq!(b.bill_type.as_deref(), Some("衆法"));
        assert!(b.source.detail_url.contains("keika/"));
        assert_eq!(b.documents[0].kind, "bill_text");
    }

    #[test]
    fn parse_bill_list_sample() {
        let html = r#"<html><body><table>
          <tr>
            <td class="td"><span class="txt03">政治資金規正法の一部を改正する法律案</span></td>
            <td class="td"><span class="txt03">衆議院で審議中</span></td>
            <td class="td"><a href="./keika/1DE153E.htm" title="経過">経過</a></td>
            <td class="td"><a href="./honbun/g22105001.htm" title="本文">本文</a></td>
          </tr>
        </table></body></html>"#;
        let bills = parse_bill_list(html, 221, BASE_URL).unwrap();
        assert_eq!(bills.len(), 1);
        assert_eq!(bills[0].bill_id, "1DE153E");
        assert!(bills[0].title.contains("政治資金規正法"));
        assert_eq!(bills[0].status.as_deref(), Some("衆議院で審議中"));
        assert!(bills[0].keika_url.ends_with("keika/1DE153E.htm"));
        assert!(bills[0]
            .honbun_url
            .as_deref()
            .unwrap()
            .ends_with("honbun/g22105001.htm"));
    }

    #[test]
    fn parse_document_index_and_body() {
        let index = r#"<html><body><div id="mainlayout"><ul>
          <li><a href="./houan/g22105001.htm">提出時法律案</a></li>
          <li><a href="./youkou/g22105001.htm">[要綱]</a></li>
          <li><a href="./syuuseian/13_8A62.htm">修正案（可決）</a></li>
        </ul></div></body></html>"#;
        let links = parse_document_index(
            index,
            "https://www.shugiin.go.jp/internet/itdb_gian.nsf/html/gian/honbun/g22105001.htm",
        )
        .unwrap();
        assert_eq!(links.len(), 3);
        assert_eq!(links[0].kind, "bill_text");
        assert_eq!(links[1].kind, "outline");
        assert_eq!(links[2].kind, "amendment");
        assert!(links[0].url.contains("/honbun/houan/"));

        let body = r#"<html><body><div id="mainlayout">
          <p>法律案本文</p><p>理 由</p><p>これが提出理由である。</p>
        </div><div id="FooterBlock"><p>footer</p></div></body></html>"#;
        let text = extract_document_text(body);
        assert!(text.contains("法律案本文\n理 由\nこれが提出理由である。"));
        assert!(!text.contains("footer"));
    }

    #[test]
    fn parse_sangiin_resolution_list() {
        let html = r#"<html><head><title>第221回国会 附帯決議一覧</title></head><body>
          <h2 class="title_text">第221回国会　附帯決議一覧</h2>
          <h3 class="title03 mt20">内閣委員会</h3>
          <ul class="exp_list_icn02">
            <li><a href="f063_060902.pdf">国家情報会議設置法案に対する附帯決議（令和8年6月9日）（PDF）</a></li>
          </ul>
          <h3 class="title03 mt20">法務委員会</h3>
          <ul class="exp_list_icn02">
            <li><a href="f065_061601.pdf">民法等の一部を改正する法律案及び関係法律整備法案に対する附帯決議（令和８年６月１６日）（PDF）</a></li>
          </ul>
        </body></html>"#;
        let resolutions = parse_resolution_list(
            html,
            0,
            "https://www.sangiin.go.jp/japanese/gianjoho/ketsugi/current/futai_ind.html",
        )
        .unwrap();
        assert_eq!(resolutions.len(), 2);
        assert_eq!(resolutions[0].session, 221);
        assert_eq!(resolutions[0].committee, "内閣委員会");
        assert_eq!(resolutions[0].subject, "国家情報会議設置法案");
        assert_eq!(
            resolutions[0].resolution_date.as_deref(),
            Some("2026-06-09")
        );
        assert_eq!(resolutions[1].committee, "法務委員会");
        assert_eq!(
            resolutions[1].resolution_date.as_deref(),
            Some("2026-06-16")
        );
        assert!(resolutions[1].source_url.ends_with("f065_061601.pdf"));
    }

    #[test]
    fn reconstructs_single_glyph_vertical_pdf_text() {
        let xhtml = r#"<html><body><doc><page width="595" height="842">
          <word xMin="350" yMin="84" xMax="362" yMax="96">一</word>
          <word xMin="350" yMin="113" xMax="362" yMax="125">政</word>
          <word xMin="350" yMin="127" xMax="362" yMax="139">府</word>
          <word xMin="350" yMin="141" xMax="362" yMax="153">は</word>
          <word xMin="350" yMin="155" xMax="362" yMax="167">、</word>
          <word xMin="350" yMin="169" xMax="362" yMax="181">次</word>
          <word xMin="350" yMin="183" xMax="362" yMax="195">の</word>
          <word xMin="350" yMin="197" xMax="362" yMax="209">措</word>
          <word xMin="350" yMin="211" xMax="362" yMax="223">置</word>
          <word xMin="330" yMin="84" xMax="342" yMax="96">を</word>
          <word xMin="330" yMin="98" xMax="342" yMax="110">講</word>
          <word xMin="330" yMin="112" xMax="342" yMax="124">ず</word>
          <word xMin="330" yMin="126" xMax="342" yMax="138">る</word>
          <word xMin="330" yMin="140" xMax="342" yMax="152">こ</word>
          <word xMin="330" yMin="154" xMax="342" yMax="166">と</word>
          <word xMin="330" yMin="168" xMax="342" yMax="180">。</word>
          <word xMin="330" yMin="182" xMax="342" yMax="194">　</word>
        </page></doc></body></html>"#;
        assert_eq!(
            reconstruct_vertical_glyph_text(xhtml),
            "一政府は、次の措置\nを講ずること。"
        );
    }

    #[test]
    fn parse_keika_fields() {
        let html = r#"<html><body><table>
          <tr><td headers="KOMOKU"><span>議案種類</span></td><td headers="NAIYO"><span>衆法</span></td></tr>
          <tr><td headers="KOMOKU"><span>議案提出回次</span></td><td headers="NAIYO"><span>221</span></td></tr>
          <tr><td headers="KOMOKU"><span>議案番号</span></td><td headers="NAIYO"><span>1</span></td></tr>
          <tr><td headers="KOMOKU"><span>議案件名</span></td><td headers="NAIYO"><span>政治資金規正法の一部を改正する法律案</span></td></tr>
          <tr><td headers="KOMOKU"><span>議案提出者</span></td><td headers="NAIYO"><span>落合 貴之君外四名</span></td></tr>
          <tr><td headers="KOMOKU"><span>衆議院付託年月日／衆議院付託委員会</span></td><td headers="NAIYO"><span>令和 8年 6月12日 ／ 政治改革に関する特別</span></td></tr>
          <tr><td headers="KOMOKU"><span>公布年月日／法律番号</span></td><td headers="NAIYO"><span>令和 8年 6月20日 ／ 法律第50号</span></td></tr>
        </table></body></html>"#;
        let meta = BillMeta {
            bill_id: "1DE153E".into(),
            session: 0,
            title: "一覧由来".into(),
            status: Some("成立".into()),
            keika_url: "https://x/keika/1DE153E.htm".into(),
            honbun_url: Some("https://x/honbun/g22105001.htm".into()),
        };
        let b = parse_keika(html, &meta, "2026-01-01T00:00:00Z").unwrap();
        assert_eq!(b.bill_type.as_deref(), Some("衆法"));
        assert_eq!(b.number.as_deref(), Some("1"));
        assert!(b.title.contains("政治資金規正法"));
        assert_eq!(b.submitter.as_deref(), Some("落合 貴之君外四名"));
        assert_eq!(b.committee.as_deref(), Some("政治改革に関する特別"));
        assert_eq!(b.law_num.as_deref(), Some("法律第50号"));
        assert_eq!(b.promulgation_date.as_deref(), Some("令和 8年 6月20日"));
        assert_eq!(b.fields.len(), 7);
        assert_eq!(b.session, 221); // 議案提出回次で上書き
                                    // 最新の動き = 公布 (令和8年6月20日 = 2026-06-20)。
        assert_eq!(b.latest_date.as_deref(), Some("2026-06-20"));
        assert_eq!(b.latest_event.as_deref(), Some("公布"));
    }

    #[test]
    fn wareki_conversion() {
        assert_eq!(
            wareki_to_iso("令和 8年 6月12日").as_deref(),
            Some("2026-06-12")
        );
        assert_eq!(
            wareki_to_iso("令和元年 5月 1日").as_deref(),
            Some("2019-05-01")
        );
        assert_eq!(
            wareki_to_iso("平成31年 4月30日").as_deref(),
            Some("2019-04-30")
        );
        assert_eq!(wareki_to_iso("／").as_deref(), None);
        assert_eq!(wareki_to_iso("").as_deref(), None);
    }

    #[test]
    #[ignore]
    fn http_real_fetch() {
        let p = HttpProvider::new();
        let bills = p.list_bills(0).unwrap();
        println!("{} bills (latest session)", bills.len());
        assert!(!bills.is_empty());
        let b = p.fetch_bill(&bills[0]).unwrap();
        println!(
            "first: [{}] {} / 委員会={:?} / 結果={:?}",
            b.bill_type.as_deref().unwrap_or("?"),
            b.title,
            b.committee,
            b.result
        );
        assert!(!b.title.is_empty());
        assert!(!b.fields.is_empty());
        assert!(b.documents.iter().any(|d| d.kind == "bill_text"));
        assert!(b.documents.iter().any(|d| d.text.contains("理 由")));

        let resolutions = p.list_resolutions(0).unwrap();
        println!("{} supplementary resolutions", resolutions.len());
        assert!(!resolutions.is_empty());
        let fetched = p.fetch_resolution(&resolutions[0]).unwrap();
        assert!(fetched.bytes.starts_with(b"%PDF-"));
    }
}
