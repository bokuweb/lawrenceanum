//! 国会の質問主意書・答弁書のクライアント。
//!
//! - 衆議院「質問答弁情報」(`/internet/itdb_shitsumon.nsf/html/shitsumon/`)。
//!   回次ごとの一覧 `kaiji{N}_l.htm`、経過 `{N}{NNN}.htm`、質問本文 `a{N}{NNN}.htm`、
//!   答弁本文 `b{N}{NNN}.htm`。Shift_JIS。第147回以前は `itdb_shitsumona.nsf`。
//! - 参議院「質問主意書」(`/japanese/joho1/kousei/syuisyo/{N}/`)。一覧 `syuisyo.htm`、
//!   明細 `meisai/m{N}{NNN}.htm`、質問本文 `syuh/s{N}{NNN}.htm`、答弁本文 `touh/t{N}{NNN}.htm`。UTF-8。
//!
//! 公式 API は無いため HTML をパースする。robots.txt は両院とも不在 (404, 2026-10 確認)。
//! 両院のサイト規約は「著作権法上認められた行為として出所を明示して引用・転載・複製できる」
//! 「無断改変は不可」なので、本文は原文のまま保持し、出典 URL を必ず添える。
//! リクエスト間隔は [`HttpProvider`] が空ける (既定 1 秒)。

use anyhow::{Context, Result};
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const SHUGIIN_BASE: &str = "https://www.shugiin.go.jp/internet";
pub const SANGIIN_BASE: &str = "https://www.sangiin.go.jp/japanese/joho1/kousei/syuisyo";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum House {
    Shugiin,
    Sangiin,
}

impl House {
    pub fn as_str(self) -> &'static str {
        match self {
            House::Shugiin => "shugiin",
            House::Sangiin => "sangiin",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            House::Shugiin => "衆議院",
            House::Sangiin => "参議院",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "shugiin" | "shu" | "衆" | "衆議院" => Some(House::Shugiin),
            "sangiin" | "san" | "参" | "参議院" => Some(House::Sangiin),
            _ => None,
        }
    }

    /// 回次 `session` の一覧ページ URL。
    pub fn list_url(self, session: u32) -> String {
        match self {
            House::Shugiin => {
                // 第147回以前は別 DB (itdb_shitsumona.nsf)。一覧ページの JS と同じ分岐。
                let db = if session > 147 {
                    "itdb_shitsumon"
                } else {
                    "itdb_shitsumona"
                };
                format!("{SHUGIIN_BASE}/{db}.nsf/html/shitsumon/kaiji{session}_l.htm")
            }
            House::Sangiin => format!("{SANGIIN_BASE}/{session}/syuisyo.htm"),
        }
    }
}

/// 質問主意書の ID (`{house}-{session}-{number}`, 例: `shugiin-219-1`)。
pub fn shitsumon_id(house: House, session: u32, number: u32) -> String {
    format!("{}-{session}-{number}", house.as_str())
}

/// 一覧の 1 行。本文 URL は公開済みのときだけ入る (未答弁なら `answer_url` は None)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListItem {
    pub house: House,
    pub session: u32,
    pub number: u32,
    pub title: String,
    pub submitter: String,
    /// 衆議院の一覧にある経過状況 (例: 「答弁受理」「内閣転送」「転送に至らず」)。参議院は None。
    pub status: Option<String>,
    /// 衆議院は経過ページ、参議院は明細ページ。
    pub detail_url: String,
    pub question_url: Option<String>,
    pub answer_url: Option<String>,
    pub question_pdf_url: Option<String>,
    pub answer_pdf_url: Option<String>,
}

impl ListItem {
    pub fn id(&self) -> String {
        shitsumon_id(self.house, self.session, self.number)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shitsumon {
    pub schema_version: u32,
    pub id: String,
    pub house: House,
    pub session: u32,
    /// 常会 / 臨時会 / 特別会。
    pub session_type: Option<String>,
    pub number: u32,
    pub title: String,
    /// 提出者 (敬称「君」は除く。「田村智子外三名」のような共同提出もそのまま)。
    pub submitter: String,
    /// 会派名 (衆議院のみ)。
    pub faction: Option<String>,
    /// 日付はすべて ISO 8601 (YYYY-MM-DD)。
    pub submitted_on: Option<String>,
    pub forwarded_on: Option<String>,
    pub answer_delay_notified_on: Option<String>,
    pub answer_delay_until: Option<String>,
    pub answered_on: Option<String>,
    pub withdrawn_on: Option<String>,
    /// 経過状況。衆議院は経過ページの値そのまま。参議院は経過欄が無いので
    /// 日付から「質問受理 / 内閣転送 / 答弁受理」を、備考から「撤回」を導く。
    pub status: Option<String>,
    /// 参議院明細の備考。
    pub note: Option<String>,
    /// 取得時の一覧上の経過状況 (衆議院のみ)。一覧は経過ページより更新が遅れることがあるため、
    /// 取り直しの判定は `status` ではなくこれと比べる。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_status: Option<String>,
    /// 質問主意書の本文 (表題以降、原文のまま)。
    pub question_text: Option<String>,
    /// 答弁書の本文 (表題以降、原文のまま)。
    pub answer_text: Option<String>,
    pub detail_url: String,
    pub question_url: Option<String>,
    pub answer_url: Option<String>,
    pub question_pdf_url: Option<String>,
    pub answer_pdf_url: Option<String>,
    pub fetched_at: String,
}

pub const SCHEMA_VERSION: u32 = 1;

impl Shitsumon {
    /// これ以上内容が変わらない (答弁済み・撤回・転送されず) なら true。
    pub fn is_settled(&self) -> bool {
        self.answer_text.is_some()
            || self.withdrawn_on.is_some()
            || matches!(self.status.as_deref(), Some(s) if s.contains("撤回") || s.contains("至らず"))
    }

    /// 一覧の行と比べて取り直す必要があるか (新たに本文が公開された・経過が進んだ)。
    pub fn is_stale(&self, item: &ListItem) -> bool {
        if self.is_settled() {
            return false;
        }
        (self.question_text.is_none() && item.question_url.is_some())
            || (self.answer_text.is_none() && item.answer_url.is_some())
            || (item.status.is_some() && item.status != self.list_status)
    }
}

// ── HTML ユーティリティ ───────────────────────────────────────────

fn sel(s: &str) -> Selector {
    Selector::parse(s).expect("static selector")
}

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn text_of(e: ElementRef) -> String {
    squash(&e.text().collect::<String>())
}

fn non_empty(s: String) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn abs_url(page_url: &str, href: &str) -> String {
    reqwest::Url::parse(page_url)
        .and_then(|u| u.join(href))
        .map(|u| u.to_string())
        .unwrap_or_else(|_| href.to_string())
}

/// 提出者名の正規化: 空白 (全角含む) を詰め、敬称「君」を落とす。
fn normalize_name(s: &str) -> String {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    let s = s.replace("君外", "外");
    s.strip_suffix('君').map(str::to_string).unwrap_or(s)
}

/// Content-Type の charset → `<meta charset>` → UTF-8 の順で文字コードを決めて復号する。
pub fn decode_html(bytes: &[u8], content_type: Option<&str>) -> String {
    let label_of = |s: &str| {
        let lower = s.to_ascii_lowercase();
        let rest = &lower[lower.find("charset=")? + "charset=".len()..];
        let label: String = rest
            .trim_start_matches(['"', '\''])
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        non_empty(label)
    };
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).into_owned();
    let enc = content_type
        .and_then(label_of)
        .or_else(|| label_of(&head))
        .and_then(|l| encoding_rs::Encoding::for_label(l.as_bytes()))
        .unwrap_or(encoding_rs::UTF_8);
    enc.decode(bytes).0.into_owned()
}

/// 本文 HTML 断片をテキストにする。`<br>` を改行に、連続する空行は 1 行にまとめる。
/// 行頭の全角空白 (段落の字下げ) は残す。
fn html_to_text(fragment: &str) -> String {
    let flat: String = fragment
        .chars()
        .filter(|c| *c != '\r' && *c != '\n')
        .collect();
    let mut s = String::with_capacity(flat.len());
    let lower = flat.to_ascii_lowercase();
    let mut i = 0;
    while let Some(off) = lower[i..].find('<') {
        let at = i + off;
        s.push_str(&flat[i..at]);
        let end = lower[at..]
            .find('>')
            .map(|e| at + e + 1)
            .unwrap_or(flat.len());
        let tag = &lower[at..end];
        let is = |name: &str| {
            tag.strip_prefix(name)
                .is_some_and(|r| r.starts_with(['>', ' ', '/', '\t']))
        };
        if is("<br") || is("</p") || is("<p") || is("</div") || is("<div") || is("<hr") {
            s.push('\n');
        } else {
            s.push_str(&flat[at..end]);
        }
        i = end;
    }
    s.push_str(&flat[i..]);
    let text: String = Html::parse_fragment(&s).root_element().text().collect();
    let mut out: Vec<&str> = Vec::new();
    for line in text.lines() {
        let line = line.trim_end();
        let line = line.trim_start_matches([' ', '\t', '\u{a0}']);
        if line.chars().all(char::is_whitespace) {
            if out.last().is_some_and(|l| !l.is_empty()) {
                out.push("");
            }
        } else {
            out.push(line);
        }
    }
    while out.last() == Some(&"") {
        out.pop();
    }
    out.join("\n")
}

/// 質問・答弁本文ページから、区切り線 (`<hr>`) 以降の本文を取り出す。
/// 区切り線の前は提出番号・日付・宛名などの鑑で、構造化項目は経過ページから取る。
pub fn parse_body(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let head = ["id=\"topcontents\"", "class=\"title_text\""]
        .iter()
        .filter_map(|m| lower.find(m))
        .min()?;
    let hr = head + lower[head..].find("<hr")?;
    let start = hr + lower[hr..].find('>')? + 1;
    let end = ["<div class=\"gh2", "</td>", "id=\"footerblock\""]
        .iter()
        .filter_map(|m| lower[start..].find(m).map(|e| start + e))
        .min()
        .unwrap_or(html.len());
    non_empty(html_to_text(&html[start..end]))
}

/// 和暦 (例: `令和 7年10月21日`, `平成元年5月1日`) → `YYYY-MM-DD`。
pub fn wareki_to_iso(s: &str) -> Option<String> {
    let s: String = s
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            '０'..='９' => char::from_u32(c as u32 - '０' as u32 + '0' as u32).unwrap(),
            c => c,
        })
        .collect();
    let (base, rest) = [
        ("令和", 2018),
        ("平成", 1988),
        ("昭和", 1925),
        ("大正", 1911),
    ]
    .iter()
    .find_map(|(era, base)| s.strip_prefix(era).map(|r| (*base, r)))?;
    let (y, rest) = rest.split_once('年')?;
    let y: i32 = if y == "元" { 1 } else { y.parse().ok()? };
    let (m, rest) = rest.split_once('月')?;
    let (d, _) = rest.split_once('日')?;
    let (m, d): (u32, u32) = (m.parse().ok()?, d.parse().ok()?);
    chrono::NaiveDate::from_ymd_opt(base + y, m, d).map(|d| d.format("%Y-%m-%d").to_string())
}

// ── 一覧 ─────────────────────────────────────────────────────────

pub fn parse_list(house: House, html: &str, session: u32, page_url: &str) -> Vec<ListItem> {
    match house {
        House::Shugiin => parse_shugiin_list(html, session, page_url),
        House::Sangiin => parse_sangiin_list(html, session, page_url),
    }
}

/// 衆議院の一覧 (`kaiji{N}_l.htm`)。各セルは `headers="SHITSUMON.*"` で識別できる。
pub fn parse_shugiin_list(html: &str, session: u32, page_url: &str) -> Vec<ListItem> {
    let doc = Html::parse_document(html);
    let (tr, td, a) = (sel("tr"), sel("td[headers]"), sel("a[href]"));
    let mut out = Vec::new();
    for row in doc.select(&tr) {
        let cell = |name: &str| {
            row.select(&td)
                .find(|c| c.value().attr("headers") == Some(name))
        };
        let link = |name: &str| {
            cell(name)
                .and_then(|c| c.select(&a).next())
                .and_then(|a| a.value().attr("href"))
                .map(|h| abs_url(page_url, h))
        };
        let Some(number) = cell("SHITSUMON.NUMBER").and_then(|c| text_of(c).parse().ok()) else {
            continue;
        };
        let Some(detail_url) = link("SHITSUMON.KLINK") else {
            continue;
        };
        out.push(ListItem {
            house: House::Shugiin,
            session,
            number,
            title: cell("SHITSUMON.KENMEI").map(text_of).unwrap_or_default(),
            submitter: normalize_name(
                &cell("SHITSUMON.TEISHUTSUSHA")
                    .map(text_of)
                    .unwrap_or_default(),
            ),
            status: cell("SHITSUMON.STATUS").map(text_of).and_then(non_empty),
            detail_url,
            question_url: link("SHITSUMON.SLINK"),
            answer_url: link("SHITSUMON.TLINK"),
            question_pdf_url: link("SHITSUMON.SLINKPDF"),
            answer_pdf_url: link("SHITSUMON.TLINKPDF"),
        });
    }
    out
}

/// 参議院の一覧 (`{N}/syuisyo.htm`)。1 件が 3 行 (件名 / 番号・提出者・HTML 本文 / PDF 本文)。
pub fn parse_sangiin_list(html: &str, session: u32, page_url: &str) -> Vec<ListItem> {
    let doc = Html::parse_document(html);
    let (tr, a, num_td, name_td) = (
        sel("table.list_c tr"),
        sel("a[href]"),
        sel("td[headers]"),
        sel("td.ta_l"),
    );
    let mut out: Vec<ListItem> = Vec::new();
    let mut pending_number = false;
    for row in doc.select(&tr) {
        let links: Vec<(String, String)> = row
            .select(&a)
            .filter_map(|e| Some((e.value().attr("href")?.to_string(), text_of(e))))
            .collect();
        if let Some((href, title)) = links.iter().find(|(h, _)| h.contains("meisai/")) {
            out.push(ListItem {
                house: House::Sangiin,
                session,
                number: 0,
                title: title.clone(),
                submitter: String::new(),
                status: None,
                detail_url: abs_url(page_url, href),
                question_url: None,
                answer_url: None,
                question_pdf_url: None,
                answer_pdf_url: None,
            });
            pending_number = true;
            continue;
        }
        let Some(cur) = out.last_mut() else { continue };
        if pending_number {
            if let Some(n) = row
                .select(&num_td)
                .next()
                .and_then(|c| text_of(c).parse().ok())
            {
                cur.number = n;
                cur.submitter =
                    normalize_name(&row.select(&name_td).next().map(text_of).unwrap_or_default());
                pending_number = false;
            }
        }
        for (href, _) in &links {
            let slot = if href.contains("syuh/") {
                &mut cur.question_url
            } else if href.contains("touh/") {
                &mut cur.answer_url
            } else if href.contains("syup/") {
                &mut cur.question_pdf_url
            } else if href.contains("toup/") {
                &mut cur.answer_pdf_url
            } else {
                continue;
            };
            *slot = Some(abs_url(page_url, href));
        }
    }
    // 番号が取れなかった行は明細 URL の末尾 (m{session}{NNN}.htm) から補う。
    for it in &mut out {
        if it.number == 0 {
            it.number = number_from_url(&it.detail_url, session).unwrap_or(0);
        }
    }
    out.retain(|it| it.number > 0);
    out
}

/// `.../m219001.htm` / `.../219001.htm` → 1。
fn number_from_url(url: &str, session: u32) -> Option<u32> {
    let file = url.rsplit('/').next()?.split('.').next()?;
    let digits: String = file.chars().skip_while(|c| !c.is_ascii_digit()).collect();
    digits.strip_prefix(&session.to_string())?.parse().ok()
}

// ── 経過 / 明細 ──────────────────────────────────────────────────

/// 経過 (衆) / 明細 (参) ページの「項目 → 内容」。
fn parse_kv(html: &str) -> Vec<(String, String)> {
    let doc = Html::parse_document(html);
    let mut kv: Vec<(String, String)> = Vec::new();
    // 衆議院: <td headers="KOMOKU">項目</td><td headers="NAIYO">内容</td>
    let (komoku, naiyo) = (sel("td[headers=KOMOKU]"), sel("td[headers=NAIYO]"));
    for row in doc.select(&sel("tr")) {
        if let (Some(k), Some(v)) = (row.select(&komoku).next(), row.select(&naiyo).next()) {
            kv.push((text_of(k), text_of(v)));
        }
    }
    if !kv.is_empty() {
        return kv;
    }
    // 参議院: <th>項目</th><td>内容</td>。「備考」は見出し行と値の行が分かれている。
    let mut key: Option<String> = None;
    for cell in doc.select(&sel("table.list_c th, table.list_c td")) {
        let t = text_of(cell);
        if cell.value().name() == "th" {
            if let Some(k) = key.replace(t) {
                kv.push((k, String::new()));
            }
        } else if let Some(k) = key.take() {
            kv.push((k, t));
        }
    }
    if let Some(k) = key {
        kv.push((k, String::new()));
    }
    kv
}

/// 経過 (衆) / 明細 (参) と本文から [`Shitsumon`] を組み立てる。
pub fn build(
    item: &ListItem,
    detail_html: &str,
    question_html: Option<&str>,
    answer_html: Option<&str>,
    fetched_at: &str,
) -> Shitsumon {
    let kv = parse_kv(detail_html);
    let get = |names: &[&str]| -> Option<String> {
        names
            .iter()
            .find_map(|n| kv.iter().find(|(k, _)| k == n))
            .map(|(_, v)| v.clone())
            .and_then(non_empty)
    };
    let date = |names: &[&str]| get(names).and_then(|v| wareki_to_iso(&v));
    let session_type = get(&["国会区別"]).or_else(|| {
        // 参議院: <p class="exp">第219回国会（臨時会）</p>
        let i = detail_html.find("回国会（")? + "回国会（".len();
        let rest = &detail_html[i..];
        non_empty(rest[..rest.find('）')?].to_string())
    });
    let note = get(&["備考"]);
    let forwarded_on = date(&["内閣転送年月日", "転送日"]);
    let answered_on = date(&["答弁書受領年月日", "答弁書受領日"]);
    let withdrawn_on = date(&["撤回年月日"]);
    let status = match item.house {
        House::Shugiin => get(&["経過状況"]).or_else(|| item.status.clone()),
        House::Sangiin => Some(
            if note.as_deref().is_some_and(|n| n.contains("撤回")) {
                "撤回"
            } else if answered_on.is_some() {
                "答弁受理"
            } else if forwarded_on.is_some() {
                "内閣転送"
            } else {
                "質問受理"
            }
            .to_string(),
        ),
    };
    let title = get(&["質問件名", "件名"]).unwrap_or_else(|| item.title.clone());
    let submitter = get(&["提出者名", "提出者"])
        .map(|s| normalize_name(&s))
        .unwrap_or_else(|| item.submitter.clone());
    Shitsumon {
        schema_version: SCHEMA_VERSION,
        id: item.id(),
        house: item.house,
        session: item.session,
        session_type,
        number: item.number,
        title,
        submitter,
        faction: get(&["会派名"]),
        submitted_on: date(&["質問主意書提出年月日", "提出日"]),
        forwarded_on,
        answer_delay_notified_on: date(&["答弁延期通知受領年月日"]),
        answer_delay_until: date(&["答弁延期期限年月日"]),
        answered_on,
        withdrawn_on,
        status,
        note,
        list_status: item.status.clone(),
        question_text: question_html.and_then(parse_body),
        answer_text: answer_html.and_then(parse_body),
        detail_url: item.detail_url.clone(),
        question_url: item.question_url.clone(),
        answer_url: item.answer_url.clone(),
        question_pdf_url: item.question_pdf_url.clone(),
        answer_pdf_url: item.answer_pdf_url.clone(),
        fetched_at: fetched_at.to_string(),
    }
}

/// 衆議院の回次プルダウン (`pulldown.htm`) から最新の回次を読む。
pub fn parse_latest_session(pulldown_html: &str) -> Option<u32> {
    pulldown_html
        .match_indices("kaiji")
        .filter_map(|(i, _)| {
            let digits: String = pulldown_html[i + 5..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            digits.parse().ok()
        })
        .max()
}

// ── HTTP ─────────────────────────────────────────────────────────

pub struct HttpProvider {
    client: reqwest::blocking::Client,
    interval: Duration,
}

impl HttpProvider {
    pub fn new(interval: Duration) -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .user_agent("lawrenceanum/0.1 (+https://github.com/bokuweb/lawrenceanum)")
            .timeout(Duration::from_secs(60))
            .build()?;
        Ok(Self { client, interval })
    }

    /// 取得して復号する。404 は None。
    fn get(&self, url: &str) -> Result<Option<String>> {
        std::thread::sleep(self.interval);
        let resp = self
            .client
            .get(url)
            .send()
            .with_context(|| format!("GET {url}"))?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let resp = resp
            .error_for_status()
            .with_context(|| format!("GET {url}"))?;
        let ct = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let bytes = resp.bytes()?;
        Ok(Some(decode_html(&bytes, ct.as_deref())))
    }

    /// 衆議院の回次プルダウンから最新の回次を返す。
    pub fn latest_session(&self) -> Result<u32> {
        let url = format!("{SHUGIIN_BASE}/itdb_shitsumon.nsf/html/shitsumon/pulldown.htm");
        let html = self.get(&url)?.context("pulldown.htm not found")?;
        parse_latest_session(&html).context("no session in pulldown.htm")
    }

    /// 回次の一覧。その回次のページが無ければ None。
    pub fn list(&self, house: House, session: u32) -> Result<Option<Vec<ListItem>>> {
        let url = house.list_url(session);
        Ok(self
            .get(&url)?
            .map(|html| parse_list(house, &html, session, &url)))
    }

    pub fn fetch(&self, item: &ListItem) -> Result<Shitsumon> {
        let detail = self
            .get(&item.detail_url)?
            .with_context(|| format!("detail not found: {}", item.detail_url))?;
        let question = match &item.question_url {
            Some(u) => self.get(u)?,
            None => None,
        };
        let answer = match &item.answer_url {
            Some(u) => self.get(u)?,
            None => None,
        };
        let fetched_at = chrono::Utc::now().to_rfc3339();
        Ok(build(
            item,
            &detail,
            question.as_deref(),
            answer.as_deref(),
            &fetched_at,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        decode_html(&std::fs::read(&path).unwrap(), None)
    }

    const SHU: &str = "https://www.shugiin.go.jp/internet/itdb_shitsumon.nsf/html/shitsumon";
    const SAN: &str = "https://www.sangiin.go.jp/japanese/joho1/kousei/syuisyo";

    #[test]
    fn wareki() {
        assert_eq!(
            wareki_to_iso("令和 7年10月21日").as_deref(),
            Some("2025-10-21")
        );
        assert_eq!(
            wareki_to_iso("令和7年10月21日").as_deref(),
            Some("2025-10-21")
        );
        assert_eq!(
            wareki_to_iso("平成元年5月1日").as_deref(),
            Some("1989-05-01")
        );
        assert_eq!(
            wareki_to_iso("昭和６４年１月７日").as_deref(),
            Some("1989-01-07")
        );
        assert_eq!(wareki_to_iso(""), None);
        assert_eq!(wareki_to_iso("令和7年2月30日"), None);
    }

    #[test]
    fn names() {
        assert_eq!(normalize_name("日野　紗里亜君"), "日野紗里亜");
        assert_eq!(normalize_name("田村　智子君外三名"), "田村智子外三名");
    }

    #[test]
    fn decodes_shift_jis_fixture() {
        let html = fixture("shugiin_kaiji222_l.htm");
        assert!(
            html.contains("第222回国会　質問の一覧"),
            "Shift_JIS が復号できていない"
        );
    }

    #[test]
    fn shugiin_list() {
        let url = format!("{SHU}/kaiji222_l.htm");
        let items = parse_shugiin_list(&fixture("shugiin_kaiji222_l.htm"), 222, &url);
        assert_eq!(items.len(), 12);
        let last = items.iter().find(|i| i.number == 12).unwrap();
        assert_eq!(
            last.title,
            "政府提出の「消費税・支援金法案」に関する質問主意書"
        );
        assert_eq!(last.submitter, "田村智子外三名");
        assert_eq!(last.status.as_deref(), Some("質問受理"));
        assert_eq!(last.detail_url, format!("{SHU}/222012.htm"));
        assert_eq!(last.question_url, None);
        assert_eq!(last.answer_url, None);
        // 内閣転送済みの行は質問本文がある。
        let fwd = items
            .iter()
            .find(|i| i.status.as_deref() == Some("内閣転送"))
            .unwrap();
        assert_eq!(
            fwd.question_url,
            Some(format!("{SHU}/a222{:03}.htm", fwd.number))
        );
        assert!(fwd.question_pdf_url.as_deref().unwrap().ends_with(".pdf"));
        assert_eq!(fwd.id(), format!("shugiin-222-{}", fwd.number));
    }

    #[test]
    fn shugiin_answered() {
        let item = ListItem {
            house: House::Shugiin,
            session: 219,
            number: 1,
            title: String::new(),
            submitter: String::new(),
            status: Some("答弁受理".into()),
            detail_url: format!("{SHU}/219001.htm"),
            question_url: Some(format!("{SHU}/a219001.htm")),
            answer_url: Some(format!("{SHU}/b219001.htm")),
            question_pdf_url: None,
            answer_pdf_url: None,
        };
        let q = fixture("shugiin_a219001.htm");
        let a = fixture("shugiin_b219001.htm");
        let s = build(
            &item,
            &fixture("shugiin_219001.htm"),
            Some(&q),
            Some(&a),
            "t",
        );
        assert_eq!(s.id, "shugiin-219-1");
        assert_eq!(
            s.title,
            "就労系障害福祉サービスにおける在宅支援に関する質問主意書"
        );
        assert_eq!(s.submitter, "日野紗里亜");
        assert_eq!(s.faction.as_deref(), Some("国民民主党・無所属クラブ"));
        assert_eq!(s.session_type.as_deref(), Some("臨時会"));
        assert_eq!(s.submitted_on.as_deref(), Some("2025-10-21"));
        assert_eq!(s.forwarded_on.as_deref(), Some("2025-10-27"));
        assert_eq!(s.answered_on.as_deref(), Some("2025-10-31"));
        assert_eq!(s.answer_delay_notified_on, None);
        assert_eq!(s.withdrawn_on, None);
        assert_eq!(s.status.as_deref(), Some("答弁受理"));
        let qt = s.question_text.as_deref().unwrap();
        assert!(
            qt.starts_with("就労系障害福祉サービスにおける在宅支援に関する質問主意書\n"),
            "{qt}"
        );
        assert!(qt.contains("障害者差別解消法の趣旨に反するおそれ"));
        assert!(qt.trim_end().ends_with("右質問する。"), "{qt}");
        assert!(!qt.contains("経過へ"), "ナビゲーションが混入: {qt}");
        let at = s.answer_text.as_deref().unwrap();
        assert!(at.starts_with("衆議院議員日野紗里亜君提出"), "{at}");
        assert!(at.contains("一について"));
        assert!(!at.contains("経過へ"));
        assert!(s.is_settled());
    }

    #[test]
    fn shugiin_pending() {
        let item = ListItem {
            house: House::Shugiin,
            session: 222,
            number: 12,
            title: String::new(),
            submitter: String::new(),
            status: Some("質問受理".into()),
            detail_url: format!("{SHU}/222012.htm"),
            question_url: None,
            answer_url: None,
            question_pdf_url: None,
            answer_pdf_url: None,
        };
        let s = build(&item, &fixture("shugiin_222012.htm"), None, None, "t");
        assert_eq!(s.submitter, "田村智子外三名");
        assert_eq!(s.faction.as_deref(), Some("日本共産党"));
        assert_eq!(s.submitted_on.as_deref(), Some("2026-10-15"));
        assert_eq!(s.forwarded_on, None);
        assert!(!s.is_settled());
        // 一覧で本文が公開されたら取り直す。
        let mut newer = item.clone();
        newer.question_url = Some(format!("{SHU}/a222012.htm"));
        newer.status = Some("内閣転送".into());
        assert!(s.is_stale(&newer));
        assert!(!s.is_stale(&item));
        // 一覧の状況が経過ページより遅れていても、一覧が変わらない限り取り直さない。
        let mut lagging = item.clone();
        lagging.status = Some("内閣転送".into());
        let s = build(&lagging, &fixture("shugiin_222012.htm"), None, None, "t");
        assert_eq!(s.status.as_deref(), Some("質問受理"));
        assert!(!s.is_stale(&lagging));
    }

    #[test]
    fn sangiin_list() {
        let url = format!("{SAN}/222/syuisyo.htm");
        let items = parse_sangiin_list(&fixture("sangiin_222_syuisyo.htm"), 222, &url);
        assert_eq!(items.len(), 9);
        assert_eq!(items[0].number, 1);
        assert_eq!(items[0].detail_url, format!("{SAN}/222/meisai/m222001.htm"));
        let last = &items[8];
        assert_eq!(last.number, 9);
        assert_eq!(
            last.title,
            "出入国管理及び難民認定法に基づく入国拒否理由に関する質問主意書"
        );
        assert_eq!(last.submitter, "奥田ふみよ");
        assert_eq!(last.question_url, None);
        assert_eq!(last.answer_url, None);
        assert_eq!(last.status, None);
    }

    #[test]
    fn sangiin_answered() {
        let item = ListItem {
            house: House::Sangiin,
            session: 219,
            number: 1,
            title: String::new(),
            submitter: String::new(),
            status: None,
            detail_url: format!("{SAN}/219/meisai/m219001.htm"),
            question_url: Some(format!("{SAN}/219/syuh/s219001.htm")),
            answer_url: Some(format!("{SAN}/219/touh/t219001.htm")),
            question_pdf_url: Some(format!("{SAN}/219/syup/s219001.pdf")),
            answer_pdf_url: Some(format!("{SAN}/219/toup/t219001.pdf")),
        };
        let q = fixture("sangiin_s219001.htm");
        let a = fixture("sangiin_t219001.htm");
        let s = build(
            &item,
            &fixture("sangiin_m219001.htm"),
            Some(&q),
            Some(&a),
            "t",
        );
        assert_eq!(s.id, "sangiin-219-1");
        assert_eq!(s.title, "奨学金返還に係る負担軽減策に関する質問主意書");
        assert_eq!(s.submitter, "塩村あやか");
        assert_eq!(s.faction, None);
        assert_eq!(s.session_type.as_deref(), Some("臨時会"));
        assert_eq!(s.submitted_on.as_deref(), Some("2025-10-21"));
        assert_eq!(s.forwarded_on.as_deref(), Some("2025-10-27"));
        assert_eq!(s.answered_on.as_deref(), Some("2025-10-31"));
        assert_eq!(s.status.as_deref(), Some("答弁受理"));
        assert_eq!(s.note, None);
        let qt = s.question_text.as_deref().unwrap();
        assert!(
            qt.contains("奨学金返還に係る負担軽減策に関する質問主意書"),
            "{qt}"
        );
        assert!(qt.contains("以下質問する。"));
        // 鑑 (国会法第七十四条…) は本文に含めない。
        assert!(!qt.contains("国会法第七十四条"), "{qt}");
        let at = s.answer_text.as_deref().unwrap();
        assert!(at.contains(
            "参議院議員塩村あやか君提出奨学金返還に係る負担軽減策に関する質問に対する答弁書"
        ));
        assert!(at.contains("一について"));
        assert!(!at.contains("内閣参質"), "{at}");
    }

    #[test]
    fn sangiin_pending() {
        let item = ListItem {
            house: House::Sangiin,
            session: 222,
            number: 9,
            title: String::new(),
            submitter: String::new(),
            status: None,
            detail_url: format!("{SAN}/222/meisai/m222009.htm"),
            question_url: None,
            answer_url: None,
            question_pdf_url: None,
            answer_pdf_url: None,
        };
        let s = build(&item, &fixture("sangiin_m222009.htm"), None, None, "t");
        assert_eq!(s.submitted_on.as_deref(), Some("2026-10-14"));
        assert_eq!(s.forwarded_on, None);
        assert_eq!(s.status.as_deref(), Some("質問受理"));
        assert_eq!(s.note, None);
        assert!(!s.is_settled());
    }

    #[test]
    fn latest_session_from_pulldown() {
        let html = r#"<OPTION value="../../../itdb_shitsumon.nsf/html/shitsumon/kaiji222_l.htm">第222回国会（臨時会）</OPTION>
<OPTION value="../../../itdb_shitsumon.nsf/html/shitsumon/kaiji221_l.htm">第221回国会（特別会）</OPTION>"#;
        assert_eq!(parse_latest_session(html), Some(222));
    }

    #[test]
    fn list_urls() {
        assert_eq!(
            House::Shugiin.list_url(219),
            format!("{SHU}/kaiji219_l.htm")
        );
        assert!(House::Shugiin.list_url(140).contains("itdb_shitsumona.nsf"));
        assert_eq!(
            House::Sangiin.list_url(219),
            format!("{SAN}/219/syuisyo.htm")
        );
    }
}
