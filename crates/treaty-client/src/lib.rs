//! 外務省の条約データのクライアント。
//!
//! ## 出典
//!
//! - 「条約データ検索（日本が締結した条約）」(`https://www3.mofa.go.jp/mofaj/gaiko/treaty/index.php`)。
//!   現行の国会承認条約・行政取極を官報と外務省の条約集から収録したもの (二国間 8,950 件・
//!   多数国間 1,013 件, 2026-10 確認)。一覧 (`search2.php`) は検索条件をセッションに持つ
//!   POST フォームで、2 ページ目以降は同じセッションで `search2.php?pID={n}` (0 始まり) を GET する。
//!   各行は 正式名称・略称・効力発生年月日・国・地域名 と、本文の PDF (`pdfs/{id}.pdf`、
//!   分冊は `{id}_{n}.pdf`) と HTML (`htmls/{id}.html`、和文と英文の対訳) へのリンク。
//!   条約 ID はこのファイル名 (例: `A-R4-101` = 二国間・令和4年、`B-H30-008` = 多数国間) を使う。
//! - 「国会提出条約」(`https://www.mofa.go.jp/mofaj/gaiko/treaty/index.html` → 回次ページ →
//!   条約ごとのページ)。第154回国会 (2002 年) 以降。署名・国会承認・通告/受諾書寄託・
//!   公布（条約第○号）・効力発生 の経過と、和文/英文/説明書/概要 PDF。条約データ検索には
//!   署名日・締結日・条約番号が無いので、題名で突き合わせてここから補う。
//!
//! ## 利用条件 (2026-10 確認)
//!
//! - 条約は著作権法13条1号「憲法その他の法令」に当たり (条約を含むと解されている)、著作権の
//!   目的とならない。外国語が正文の条約の和文 (外務省の公定訳) は同条4号 (国が作成する
//!   1号の翻訳物) に当たる。説明書・概要 PDF は同条の対象外なので、下記の PDL1.0 で扱う。
//! - 外務省ウェブサイトの「法的事項」(`https://www.mofa.go.jp/mofaj/annai/legalmatters/index.html`)
//!   は、特記の無いコンテンツに「公共データ利用規約（第1.0版）」(PDL1.0、CC BY 4.0 互換) を適用。
//!   出典の記載 (例: 「出典：外務省ウェブサイト（当該ページのURL）」) と、編集・加工したときは
//!   その旨の記載を求める。よって本文は原文のまま保持し、`source_url` に出典 URL を必ず添える。
//! - robots.txt は www / www3 とも存在しない (404)。条約本文の HTML (`htmls/*.html`) には
//!   `<meta name="ROBOTS" content="none">` があるため、そのページからはリンクを辿らない
//!   (取得する URL はすべて一覧と国会提出条約ページに載っているもの)。
//! - 注意: 2026-10 時点で www / www3 とも Akamai が curl 等の非ブラウザのクライアントに
//!   403 を返す (UA を明示しても同じ)。ブロックを回避する細工はしない。[`HttpProvider`] は
//!   403 をそのままエラーにし、ベース URL は env (`LAWPUB_MOFA_BASE_URL` /
//!   `LAWPUB_MOFA_TREATY_SEARCH_URL`) で差し替えられる。
//!
//! リクエスト間隔は [`HttpProvider`] が空ける (既定 2 秒)。

use anyhow::{Context, Result};
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

pub const SITE_BASE: &str = "https://www.mofa.go.jp";
pub const SEARCH_BASE: &str = "https://www3.mofa.go.jp/mofaj/gaiko/treaty";
/// 国会提出条約の目次 (回次ごとのページへのリンク)。
pub const DIET_INDEX_URL: &str = "https://www.mofa.go.jp/mofaj/gaiko/treaty/index.html";
/// 条約データ検索の入口 (出典表示用)。
pub const SEARCH_TOP_URL: &str = "https://www3.mofa.go.jp/mofaj/gaiko/treaty/index.php";

pub const SCHEMA_VERSION: u32 = 1;

/// 二国間 / 多数国間。条約データ検索の「地域・国名」の種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Bilateral,
    Multilateral,
}

impl Kind {
    pub const ALL: [Kind; 2] = [Kind::Bilateral, Kind::Multilateral];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Bilateral => "bilateral",
            Kind::Multilateral => "multilateral",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Bilateral => "二国間",
            Kind::Multilateral => "多数国間",
        }
    }

    /// 検索フォームの `conclude_country` の値。
    fn form_value(self) -> &'static str {
        match self {
            Kind::Bilateral => "1",
            Kind::Multilateral => "2",
        }
    }
}

// ── 型 ───────────────────────────────────────────────────────────

/// 条約データ検索の一覧 1 行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchItem {
    pub id: String,
    pub kind: Kind,
    pub title: String,
    pub short_title: Option<String>,
    /// 効力発生年月日 (ISO 8601)。古い条約は「-」で空。
    pub effective_on: Option<String>,
    /// 相手国・地域 (二国間のみ。「国際機関」もある)。
    pub parties: Vec<String>,
    pub pdf_urls: Vec<String>,
    pub html_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchPage {
    pub items: Vec<SearchItem>,
    /// 「N件の条約が該当しました」。
    pub total: Option<u32>,
    /// 表示中のページ (1 始まり) と総ページ数 (「2/299」)。
    pub page: Option<u32>,
    pub pages: Option<u32>,
}

/// 経過の 1 行 (「令和7年4月23日　国会承認」)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreatyEvent {
    pub date: Option<String>,
    pub text: String,
}

/// 原本ファイル (PDF)。取得したものは SHA-256 とサイズを持つ (原本は `{sha256}.pdf` で保存)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    /// 「和文」「英文」「説明書」「概要」「本文」「本文 2」など。
    pub label: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extraction_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fetched_at: Option<String>,
}

impl Attachment {
    pub fn new(label: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            url: url.into(),
            sha256: None,
            bytes: None,
            media_type: None,
            extraction_error: None,
            fetched_at: None,
        }
    }
}

/// 経過から読み取った日付と番号。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreatyDates {
    /// 署名 (多数国間条約は作成・採択) の日。
    pub signed_on: Option<String>,
    pub diet_approved_on: Option<String>,
    /// 日本が締結した日 (受諾書・批准書・加入書の寄託、相手国への通告、公文の交換)。
    pub concluded_on: Option<String>,
    pub promulgated_on: Option<String>,
    /// 日本について効力を生じた日。
    pub effective_on: Option<String>,
    /// 公布時の条約番号 (「条約第5号」の 5)。番号は暦年ごとに振られる。
    pub treaty_number: Option<u32>,
    /// 「令和7年条約第5号」。
    pub treaty_number_label: Option<String>,
    /// 「外務省告示第265号」。
    pub notice_number: Option<String>,
}

/// 国会提出条約ページの 1 件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DietTreaty {
    /// ページのファイル名 (例: `pagew_000001_00003`)。
    pub key: String,
    pub session: Option<u32>,
    pub title: String,
    pub short_title: Option<String>,
    pub events: Vec<TreatyEvent>,
    pub dates: TreatyDates,
    pub attachments: Vec<Attachment>,
    pub url: String,
    pub fetched_at: String,
}

impl DietTreaty {
    /// 公布と効力発生まで済んでいれば、これ以上経過は増えない。
    pub fn is_settled(&self) -> bool {
        self.dates.promulgated_on.is_some() && self.dates.effective_on.is_some()
    }
}

/// 回次ページへのリンク。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DietSession {
    pub session: u32,
    pub url: String,
}

/// 回次ページに並ぶ条約 (略称) へのリンク。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DietLink {
    pub label: String,
    pub url: String,
}

/// 条約本文 (HTML 版から取り出したもの)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreatyText {
    /// `ja` (和文) / `en` (和文が無い交換公文など、英文だけのもの)。
    pub lang: String,
    pub text: String,
}

/// 配信する条約 1 件 (`public/treaties/{id}.json`)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Treaty {
    pub schema_version: u32,
    /// 条約データ検索のファイル名 (`A-R4-101`)。条約データ検索に未掲載で国会提出条約ページだけに
    /// あるものは `diet-{ページ名}`。
    pub id: String,
    pub title: String,
    pub short_title: Option<String>,
    pub kind: Option<Kind>,
    pub parties: Vec<String>,
    pub signed_on: Option<String>,
    pub diet_approved_on: Option<String>,
    pub concluded_on: Option<String>,
    pub promulgated_on: Option<String>,
    pub effective_on: Option<String>,
    pub treaty_number: Option<u32>,
    pub treaty_number_label: Option<String>,
    pub notice_number: Option<String>,
    pub diet_session: Option<u32>,
    pub events: Vec<TreatyEvent>,
    /// 条約データ検索 (現行の締結済み条約) に載っているか。
    pub in_database: bool,
    /// 本文 (原文のまま)。HTML 版があればそこから、無ければ PDF から抽出したもの。
    pub text: Option<String>,
    pub text_lang: Option<String>,
    /// `html` / `pdftotext`。
    pub text_source: Option<String>,
    pub text_url: Option<String>,
    pub attachments: Vec<Attachment>,
    /// 出典 URL (条約データ検索の本文 HTML/PDF、または国会提出条約ページ)。
    pub source_url: String,
    pub diet_url: Option<String>,
    pub fetched_at: Option<String>,
}

/// `.cache/treaty/text/{id}.json` — 本文の取得結果 (HTML か PDF)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextRecord {
    pub id: String,
    pub url: String,
    pub lang: Option<String>,
    pub text: Option<String>,
    /// `html` / `pdftotext`。
    pub source: String,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    pub error: Option<String>,
    pub fetched_at: String,
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

fn abs_url(page_url: &str, href: &str) -> String {
    reqwest::Url::parse(page_url)
        .and_then(|u| u.join(href.trim()))
        .map(|u| u.to_string())
        .unwrap_or_else(|_| href.to_string())
}

fn dash_none(s: String) -> Option<String> {
    let t = s.trim();
    if t.is_empty() || t == "-" || t == "－" || t == "―" {
        None
    } else {
        Some(t.to_string())
    }
}

/// URL の最後のパス要素から拡張子を除いたもの。
fn basename(url: &str) -> &str {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.').map(|(n, _)| n).unwrap_or(name)
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
        (!label.is_empty()).then_some(label)
    };
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).into_owned();
    let enc = content_type
        .and_then(label_of)
        .or_else(|| label_of(&head))
        .and_then(|l| encoding_rs::Encoding::for_label(l.as_bytes()))
        .unwrap_or(encoding_rs::UTF_8);
    enc.decode(bytes).0.into_owned()
}

/// 全角英数字・記号を半角に。
fn to_halfwidth(c: char) -> char {
    match c {
        '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
        '\u{3000}' => ' ',
        c => c,
    }
}

/// 題名の突き合わせ用の正規化: 空白を除き、全角英数字を半角にする。
pub fn normalize_title(s: &str) -> String {
    s.chars()
        .map(to_halfwidth)
        .filter(|c| !c.is_whitespace())
        .collect()
}

/// 末尾の「（第百六十二号）」「（第162号）」(ILO 条約の番号) を除いた題名。
fn strip_convention_number(s: &str) -> &str {
    let t = s.trim_end();
    for (open, close) in [('（', '）'), ('(', ')')] {
        if let Some(body) = t.strip_suffix(close) {
            if let Some(at) = body.rfind(open) {
                let inner = &body[at + open.len_utf8()..];
                if inner.starts_with('第') && inner.ends_with('号') {
                    return t[..at].trim_end();
                }
            }
        }
    }
    t
}

// ── 日付 ─────────────────────────────────────────────────────────

/// 「令和7年4月23日」「平成元年6月16日」「2022年06月01日」→ ISO 8601。先頭から読む。
/// 戻り値の 2 つ目は日付の後ろの残り。
pub fn parse_date_prefix(s: &str) -> Option<(String, &str)> {
    let t = s.trim_start();
    let eras: [(&str, i32); 5] = [
        ("令和", 2018),
        ("平成", 1988),
        ("昭和", 1925),
        ("大正", 1911),
        ("明治", 1867),
    ];
    let (base, rest) = match eras
        .iter()
        .find_map(|(era, base)| t.strip_prefix(era).map(|r| (*base, r)))
    {
        Some(v) => v,
        None => (0, t),
    };
    let (y, rest) = rest.split_once('年')?;
    let (m, rest) = rest.split_once('月')?;
    let (d, rest) = rest.split_once('日')?;
    let num = |s: &str| -> Option<i32> {
        let s: String = s
            .chars()
            .map(to_halfwidth)
            .filter(|c| !c.is_whitespace())
            .collect();
        if s == "元" {
            return Some(1);
        }
        if s.is_empty() || s.len() > 4 || !s.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        s.parse().ok()
    };
    let (y, m, d) = (num(y)?, num(m)?, num(d)?);
    let year = if base == 0 {
        if y < 1000 {
            return None;
        }
        y
    } else {
        base + y
    };
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let dim = match m {
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if d > dim {
        return None;
    }
    Some((format!("{year:04}-{m:02}-{d:02}"), rest))
}

pub fn parse_date(s: &str) -> Option<String> {
    parse_date_prefix(s).map(|(d, _)| d)
}

/// ISO 日付の和暦年 (「令和7年」)。改元の年は日付で分ける。
pub fn wareki_year(iso: &str) -> Option<String> {
    let y: i32 = iso.get(0..4)?.parse().ok()?;
    let md = iso.get(5..10)?;
    let (era, base) = if (y, md) >= (2019, "05-01") {
        ("令和", 2018)
    } else if (y, md) >= (1989, "01-08") {
        ("平成", 1988)
    } else if (y, md) >= (1926, "12-25") {
        ("昭和", 1925)
    } else if (y, md) >= (1912, "07-30") {
        ("大正", 1911)
    } else {
        ("明治", 1867)
    };
    Some(format!("{era}{}年", y - base))
}

fn parse_number_after(s: &str, marker: &str) -> Option<u32> {
    let at = s.find(marker)? + marker.len();
    let digits: String = s[at..]
        .chars()
        .map(to_halfwidth)
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let n = digits.parse().ok()?;
    s[at..]
        .chars()
        .map(to_halfwidth)
        .nth(digits.len())
        .filter(|c| *c == '号')
        .map(|_| n)
}

/// 経過から署名・国会承認・締結・公布・効力発生の日と条約番号を読む。
pub fn derive_dates(events: &[TreatyEvent]) -> TreatyDates {
    let mut d = TreatyDates::default();
    let has = |t: &str, ws: &[&str]| ws.iter().any(|w| t.contains(w));
    let mut approval_idx = None;
    for (i, e) in events.iter().enumerate() {
        if has(&e.text, &["国会承認"]) {
            d.diet_approved_on = d.diet_approved_on.take().or(e.date.clone());
            approval_idx.get_or_insert(i);
        }
    }
    for e in events {
        if d.signed_on.is_none()
            && has(&e.text, &["署名", "作成", "採択"])
            && !has(&e.text, &["国会承認", "公布"])
        {
            d.signed_on = e.date.clone();
        }
        if has(&e.text, &["公布"]) && d.promulgated_on.is_none() {
            d.promulgated_on = e.date.clone();
            d.treaty_number = parse_number_after(&e.text, "条約第");
            if let Some(n) = parse_number_after(&e.text, "外務省告示第") {
                d.notice_number = Some(format!("外務省告示第{n}号"));
            }
        }
    }
    // 締結: 国会承認の後 (承認が無ければ最初から) の、日本側の締結行為。
    const CONCLUDE: &[&str] = &[
        "寄託",
        "日本側から",
        "我が国から",
        "日本国から",
        "公文の交換",
        "公文交換",
        "批准書の交換",
        "受諾",
        "加入",
        "締結",
    ];
    let from = approval_idx.map(|i| i + 1).unwrap_or(0);
    d.concluded_on = events[from.min(events.len())..]
        .iter()
        .find(|e| has(&e.text, CONCLUDE) && !has(&e.text, &["公布", "効力発生", "国会承認"]))
        .and_then(|e| e.date.clone());
    // 効力発生: 「我が国について」「日本について」を優先し、無ければ最後の「効力発生」。
    let effective: Vec<&TreatyEvent> = events
        .iter()
        .filter(|e| e.text.contains("効力発生") && !e.text.contains("公布"))
        .collect();
    d.effective_on = effective
        .iter()
        .rev()
        .find(|e| {
            has(
                &e.text,
                &["我が国について", "日本について", "日本国について"],
            )
        })
        .or(effective.last())
        .and_then(|e| e.date.clone());
    if let (Some(n), Some(p)) = (d.treaty_number, d.promulgated_on.as_deref()) {
        d.treaty_number_label = wareki_year(p).map(|y| format!("{y}条約第{n}号"));
    }
    d
}

// ── 条約データ検索 ───────────────────────────────────────────────

/// 一覧の行の ID: HTML 版のファイル名、無ければ最初の PDF のファイル名 (分冊の `_n` を除く)。
fn item_id(html_url: Option<&str>, pdf_urls: &[String], title: &str) -> String {
    if let Some(u) = html_url {
        return basename(u).to_string();
    }
    if let Some(u) = pdf_urls.first() {
        let b = basename(u);
        return match b.rsplit_once('_') {
            Some((head, tail)) if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) => {
                // `B-H31_R1-004` のように `_` を含む ID もあるので、分冊番号は短い数字だけ。
                if tail.len() <= 2 && pdf_urls.len() > 1 {
                    head.to_string()
                } else {
                    b.to_string()
                }
            }
            _ => b.to_string(),
        };
    }
    // リンクが 1 つも無い行は題名から決める (取り直しても同じ ID になるように)。
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in normalize_title(title).bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("t-{h:016x}")
}

/// 条約データ検索の一覧ページ。`kind` は検索条件 (二国間/多数国間) で、ページからは読まない。
pub fn parse_search_page(html: &str, kind: Kind, page_url: &str) -> SearchPage {
    let doc = Html::parse_document(html);
    let (row_sel, dl_sel, dt_sel, dd_sel, a_sel) = (
        sel(".check_table_result > div"),
        sel("dl"),
        sel("dt"),
        sel("dd"),
        sel("a[href]"),
    );
    let mut items = Vec::new();
    for row in doc.select(&row_sel) {
        let mut fields: HashMap<String, ElementRef> = HashMap::new();
        for dl in row.select(&dl_sel) {
            if let (Some(dt), Some(dd)) = (dl.select(&dt_sel).next(), dl.select(&dd_sel).next()) {
                fields.insert(text_of(dt), dd);
            }
        }
        let Some(name_dd) = fields.get("条約名称") else {
            continue;
        };
        // 題名は最初のリンクより前の文字列 (「… PDF(」の飾りを除く)。
        let inner = name_dd.inner_html();
        let cut = inner.find("<a").unwrap_or(inner.len());
        let pre: String = Html::parse_fragment(&inner[..cut])
            .root_element()
            .text()
            .collect();
        let title = squash(&pre)
            .trim_end_matches("PDF(")
            .trim_end_matches("PDF（")
            .trim()
            .to_string();
        if title.is_empty() {
            continue;
        }
        let mut pdf_urls = Vec::new();
        let mut html_url = None;
        for a in name_dd.select(&a_sel) {
            let href = a.value().attr("href").unwrap_or_default();
            let url = abs_url(page_url, href);
            let lower = url.to_ascii_lowercase();
            if lower.ends_with(".pdf") {
                if !pdf_urls.contains(&url) {
                    pdf_urls.push(url);
                }
            } else if lower.contains("/htmls/") && html_url.is_none() {
                html_url = Some(url);
            }
        }
        let field = |k: &str| fields.get(k).map(|e| text_of(*e)).and_then(dash_none);
        let parties = field("国・地域名")
            .map(|s| {
                s.split(['、', ','])
                    .map(str::trim)
                    .filter(|p| !p.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let id = item_id(html_url.as_deref(), &pdf_urls, &title);
        items.push(SearchItem {
            id,
            kind,
            short_title: field("略称"),
            effective_on: field("効力発生年月日").as_deref().and_then(parse_date),
            parties,
            title,
            pdf_urls,
            html_url,
        });
    }
    let body = doc.root_element().text().collect::<String>();
    let total = body.find("件の条約が該当").and_then(|at| {
        let head = &body[..at];
        let digits: String = head
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        digits.parse().ok()
    });
    let (mut page, mut pages) = (None, None);
    for div in doc.select(&sel(".paging_flex > div")) {
        let t = text_of(div);
        if let Some((a, b)) = t.split_once('/') {
            if let (Ok(a), Ok(b)) = (a.trim().parse(), b.trim().parse()) {
                page = Some(a);
                pages = Some(b);
            }
        }
    }
    SearchPage {
        items,
        total,
        page,
        pages,
    }
}

// ── 国会提出条約 ─────────────────────────────────────────────────

fn parse_session_number(s: &str) -> Option<u32> {
    let at = s.find("回国会")?;
    let head = &s[..at];
    let digits: String = head
        .chars()
        .rev()
        .map(to_halfwidth)
        .take_while(|c| c.is_ascii_digit())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    digits.parse().ok()
}

/// 国会提出条約の目次から回次ページへのリンク (新しい回次から)。
pub fn parse_diet_index(html: &str, page_url: &str) -> Vec<DietSession> {
    let doc = Html::parse_document(html);
    let mut out: Vec<DietSession> = Vec::new();
    for a in doc.select(&sel("a[href]")) {
        let t = text_of(a);
        let Some(session) = parse_session_number(&t) else {
            continue;
        };
        let url = abs_url(page_url, a.value().attr("href").unwrap_or_default());
        if !out.iter().any(|s| s.url == url || s.session == session) {
            out.push(DietSession { session, url });
        }
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.session));
    out
}

/// 回次ページに並ぶ条約ページへのリンク。
pub fn parse_diet_session(html: &str, page_url: &str) -> Vec<DietLink> {
    let doc = Html::parse_document(html);
    let root = doc
        .select(&sel("#maincontents"))
        .next()
        .or_else(|| doc.select(&sel("#contents")).next())
        .unwrap_or_else(|| doc.root_element());
    let mut out: Vec<DietLink> = Vec::new();
    for a in root.select(&sel("li a[href]")) {
        let label = text_of(a);
        let href = a.value().attr("href").unwrap_or_default();
        if label.is_empty()
            || href.starts_with("mailto:")
            || href.starts_with('#')
            || href.to_ascii_lowercase().ends_with(".pdf")
        {
            continue;
        }
        let url = abs_url(page_url, href);
        if !url.contains("mofa.go.jp") || out.iter().any(|l| l.url == url) {
            continue;
        }
        out.push(DietLink { label, url });
    }
    out
}

fn strip_short_title(s: &str) -> Option<String> {
    let t = s.trim();
    let t = t.strip_prefix('（').or_else(|| t.strip_prefix('('))?;
    let t = t.strip_prefix("略称")?;
    let t = t.trim_start_matches(['：', ':', '　', ' ']);
    let t = t
        .strip_suffix('）')
        .or_else(|| t.strip_suffix(')'))
        .unwrap_or(t);
    let t = t.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// 国会提出条約の条約ページ。題名と経過が読めなければ None。
pub fn parse_diet_detail(
    html: &str,
    url: &str,
    session: Option<u32>,
    fetched_at: &str,
) -> Option<DietTreaty> {
    let doc = Html::parse_document(html);
    let h2 = doc
        .select(&sel("#contents-header h2"))
        .next()
        .or_else(|| doc.select(&sel("#contents h2")).next())
        .or_else(|| doc.select(&sel("h2")).next())?;
    // 題名は <br> で折り返されることがある。「（略称…）」の行は略称。
    let mut title = String::new();
    let mut short_title = None;
    for part in h2.inner_html().split("<br>") {
        let t: String = Html::parse_fragment(part).root_element().text().collect();
        let t = squash(&t);
        if let Some(s) = strip_short_title(&t) {
            short_title = Some(s);
        } else {
            title.push_str(&t.replace(' ', ""));
        }
    }
    if short_title.is_none() {
        short_title = doc
            .select(&sel("#contents-header h3"))
            .next()
            .and_then(|h3| strip_short_title(&text_of(h3)));
    }
    if title.is_empty() {
        return None;
    }
    let mut events = Vec::new();
    let mut attachments: Vec<Attachment> = Vec::new();
    for li in doc.select(&sel("li")) {
        let t = text_of(li);
        if let Some((date, rest)) = parse_date_prefix(&t) {
            events.push(TreatyEvent {
                date: Some(date),
                text: rest.trim().to_string(),
            });
        }
        for a in li.select(&sel("a[href]")) {
            let href = a.value().attr("href").unwrap_or_default();
            if !href.to_ascii_lowercase().ends_with(".pdf") {
                continue;
            }
            let label = text_of(a)
                .replace("（PDF）", "")
                .replace("(PDF)", "")
                .trim()
                .to_string();
            let u = abs_url(url, href);
            if !attachments.iter().any(|x| x.url == u) {
                attachments.push(Attachment::new(label, u));
            }
        }
    }
    if events.is_empty() {
        return None;
    }
    let dates = derive_dates(&events);
    Some(DietTreaty {
        key: basename(url).to_string(),
        session,
        title,
        short_title,
        events,
        dates,
        attachments,
        url: url.to_string(),
        fetched_at: fetched_at.to_string(),
    })
}

// ── 本文 HTML ─────────────────────────────────────────────────────

/// 条約本文の HTML 版 (`htmls/{id}.html`) から和文 (無ければ英文) を取り出す。
///
/// 本文は `.doublecolumn-container` (左が和文・右が英文の対訳) か `.singlecolumn-container`
/// (`lang="en"` なら英文) の並び。表示用の「null」の段は捨て、冒頭で 2 回繰り返される題名は 1 回にする。
pub fn parse_treaty_text(html: &str) -> Option<TreatyText> {
    let doc = Html::parse_document(html);
    let (block_sel, left, right, pre) = (
        sel(".doublecolumn-container, .singlecolumn-container"),
        sel(".doublecolumn-left"),
        sel(".doublecolumn-right"),
        sel("pre"),
    );
    let read = |e: ElementRef| -> Option<String> {
        let t: String = e.select(&pre).flat_map(|p| p.text()).collect();
        let t = t.trim_matches(['\n', '\r']).trim_end().to_string();
        let trimmed = t.trim();
        (!trimmed.is_empty() && trimmed != "null").then_some(t)
    };
    let (mut ja, mut en): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    for b in doc.select(&block_sel) {
        let class = b.value().attr("class").unwrap_or_default();
        if class.contains("doublecolumn-container") {
            if let Some(t) = b.select(&left).next().and_then(read) {
                ja.push(t);
            }
            if let Some(t) = b.select(&right).next().and_then(read) {
                en.push(t);
            }
        } else if b.value().attr("lang") == Some("en") {
            en.extend(read(b));
        } else {
            ja.extend(read(b));
        }
    }
    let join = |mut v: Vec<String>| {
        if v.len() >= 2 && v[0].trim() == v[1].trim() {
            v.remove(0);
        }
        v.join("\n")
    };
    if !ja.is_empty() {
        Some(TreatyText {
            lang: "ja".into(),
            text: join(ja),
        })
    } else if !en.is_empty() {
        Some(TreatyText {
            lang: "en".into(),
            text: join(en),
        })
    } else {
        None
    }
}

// ── 突き合わせ・統合 ─────────────────────────────────────────────

/// 国会提出条約 (key) → 条約データ検索の行 (id) の対応を題名で決める。
///
/// 正規化した正式名称の完全一致、無ければ末尾の「（第…号）」を除いた題名で比べる。
/// 同名が複数あるとき (改定版の万国郵便条約など) は効力発生日の一致、次いで新しい方を採る。
pub fn match_diet_to_items(items: &[SearchItem], diets: &[DietTreaty]) -> HashMap<String, String> {
    let mut exact: HashMap<String, Vec<&SearchItem>> = HashMap::new();
    let mut loose: HashMap<String, Vec<&SearchItem>> = HashMap::new();
    for it in items {
        exact
            .entry(normalize_title(&it.title))
            .or_default()
            .push(it);
        loose
            .entry(normalize_title(strip_convention_number(&it.title)))
            .or_default()
            .push(it);
    }
    let mut out = HashMap::new();
    for d in diets {
        let cands = exact
            .get(&normalize_title(&d.title))
            .or_else(|| loose.get(&normalize_title(strip_convention_number(&d.title))));
        let Some(cands) = cands else { continue };
        let pick = cands
            .iter()
            .find(|c| c.effective_on.is_some() && c.effective_on == d.dates.effective_on)
            .or_else(|| {
                cands
                    .iter()
                    .max_by(|a, b| a.effective_on.cmp(&b.effective_on))
            });
        if let Some(it) = pick {
            out.insert(d.key.clone(), it.id.clone());
        }
    }
    out
}

/// 一覧の行・国会提出条約ページ・本文を 1 件にまとめる。どちらか一方は必須。
pub fn merge(
    item: Option<&SearchItem>,
    diet: Option<&DietTreaty>,
    text: Option<&TextRecord>,
) -> Option<Treaty> {
    let (id, title, source_url) = match (item, diet) {
        (Some(it), _) => (
            it.id.clone(),
            it.title.clone(),
            it.html_url
                .clone()
                .or_else(|| it.pdf_urls.first().cloned())
                .unwrap_or_else(|| SEARCH_TOP_URL.to_string()),
        ),
        (None, Some(d)) => (format!("diet-{}", d.key), d.title.clone(), d.url.clone()),
        (None, None) => return None,
    };
    let dates = diet.map(|d| d.dates.clone()).unwrap_or_default();
    let mut attachments: Vec<Attachment> = Vec::new();
    if let Some(it) = item {
        let n = it.pdf_urls.len();
        for (i, u) in it.pdf_urls.iter().enumerate() {
            let label = if n > 1 {
                format!("本文 {}", i + 1)
            } else {
                "本文".to_string()
            };
            attachments.push(Attachment::new(label, u.clone()));
        }
    }
    if let Some(d) = diet {
        for a in &d.attachments {
            if !attachments.iter().any(|x| x.url == a.url) {
                attachments.push(a.clone());
            }
        }
    }
    // 取得済みの原本 (SHA-256 等) を反映する。
    if let Some(t) = text {
        for fetched in &t.attachments {
            if let Some(a) = attachments.iter_mut().find(|a| a.url == fetched.url) {
                let label = a.label.clone();
                *a = fetched.clone();
                a.label = label;
            }
        }
    }
    let text_ok = text.filter(|t| t.text.as_deref().is_some_and(|s| !s.trim().is_empty()));
    Some(Treaty {
        schema_version: SCHEMA_VERSION,
        id,
        title,
        short_title: item
            .and_then(|i| i.short_title.clone())
            .or_else(|| diet.and_then(|d| d.short_title.clone())),
        kind: item.map(|i| i.kind),
        parties: item.map(|i| i.parties.clone()).unwrap_or_default(),
        signed_on: dates.signed_on,
        diet_approved_on: dates.diet_approved_on,
        concluded_on: dates.concluded_on,
        promulgated_on: dates.promulgated_on,
        effective_on: dates
            .effective_on
            .or_else(|| item.and_then(|i| i.effective_on.clone())),
        treaty_number: dates.treaty_number,
        treaty_number_label: dates.treaty_number_label,
        notice_number: dates.notice_number,
        diet_session: diet.and_then(|d| d.session),
        events: diet.map(|d| d.events.clone()).unwrap_or_default(),
        in_database: item.is_some(),
        text: text_ok.and_then(|t| t.text.clone()),
        text_lang: text_ok.and_then(|t| t.lang.clone()),
        text_source: text_ok.map(|t| t.source.clone()),
        text_url: text_ok.map(|t| t.url.clone()),
        attachments,
        source_url,
        diet_url: diet.map(|d| d.url.clone()),
        fetched_at: text
            .map(|t| t.fetched_at.clone())
            .or_else(|| diet.map(|d| d.fetched_at.clone())),
    })
}

// ── Provider ─────────────────────────────────────────────────────

/// 取得したバイト列と Content-Type。
#[derive(Debug, Clone)]
pub struct Fetched {
    pub bytes: Vec<u8>,
    pub media_type: Option<String>,
}

pub trait TreatyProvider: Send + Sync {
    /// 条約データ検索の一覧ページ (0 始まり) の HTML。
    fn search_page(&self, kind: Kind, page: u32) -> Result<String>;
    /// HTML ページ。404 は None。
    fn get_html(&self, url: &str) -> Result<Option<String>>;
    /// 原本ファイル。404 は None。
    fn get_bytes(&self, url: &str) -> Result<Option<Fetched>>;
}

/// 実ページの fixture を返すオフライン用 provider。
pub struct MockProvider;

const FX_SEARCH_BILATERAL: &str = include_str!("../tests/fixtures/search_bilateral_p2.html");
const FX_SEARCH_MULTI: &str = include_str!("../tests/fixtures/search_multilateral_p1.html");
const FX_SEARCH_MULTI_LAST: &str = include_str!("../tests/fixtures/search_multilateral_p34.html");
const FX_DIET_INDEX: &str = include_str!("../tests/fixtures/diet_index.html");
const FX_DIET_217: &str = include_str!("../tests/fixtures/diet_session_217.html");
const FX_DIET_162: &str = include_str!("../tests/fixtures/diet_session_162.html");
const FX_DIET_UKRAINE: &str = include_str!("../tests/fixtures/diet_pagew_000001_00003.html");
const FX_DIET_ASBESTOS: &str = include_str!("../tests/fixtures/diet_treaty162_4.html");
const FX_DIET_HAGUE: &str = include_str!("../tests/fixtures/diet_page24_000001.html");
const FX_TEXT_SWISS: &str = include_str!("../tests/fixtures/text_A-R4-101.html");
const FX_TEXT_BANGLADESH: &str = include_str!("../tests/fixtures/text_A-H05-0073.html");

impl TreatyProvider for MockProvider {
    fn search_page(&self, kind: Kind, page: u32) -> Result<String> {
        Ok(match (kind, page) {
            (Kind::Bilateral, 0) => FX_SEARCH_BILATERAL.to_string(),
            (Kind::Multilateral, 0) => FX_SEARCH_MULTI.to_string(),
            (Kind::Multilateral, 33) => FX_SEARCH_MULTI_LAST.to_string(),
            _ => String::from("<html><body><main></main></body></html>"),
        })
    }

    fn get_html(&self, url: &str) -> Result<Option<String>> {
        let page = |p: &str| url.ends_with(p);
        Ok(if url == DIET_INDEX_URL {
            Some(FX_DIET_INDEX)
        } else if page("/mofaj/ila/st/pagew_000001_00001.html") {
            Some(FX_DIET_217)
        } else if page("/mofaj/gaiko/treaty/treaty162.html") {
            Some(FX_DIET_162)
        } else if page("/mofaj/ila/st/pagew_000001_00003.html") {
            Some(FX_DIET_UKRAINE)
        } else if page("/mofaj/gaiko/treaty/treaty162_4.html") {
            Some(FX_DIET_ASBESTOS)
        } else if page("/mofaj/gaiko/page24_000001.html") {
            Some(FX_DIET_HAGUE)
        } else if page("/htmls/A-R4-101.html") {
            Some(FX_TEXT_SWISS)
        } else if page("/htmls/A-H05-0073.html") {
            Some(FX_TEXT_BANGLADESH)
        } else {
            None
        }
        .map(str::to_string))
    }

    fn get_bytes(&self, _url: &str) -> Result<Option<Fetched>> {
        Ok(None)
    }
}

/// 実サイトの provider。リクエストごとに `interval` 空け、検索のセッション Cookie を持ち回る。
pub struct HttpProvider {
    client: reqwest::blocking::Client,
    interval: Duration,
    site_base: String,
    search_base: String,
    cookie: Mutex<Option<String>>,
    current_kind: Mutex<Option<Kind>>,
}

impl HttpProvider {
    pub fn new(interval: Duration) -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .user_agent("lawrenceanum/0.1 (+https://github.com/bokuweb/lawrenceanum)")
            .timeout(Duration::from_secs(120))
            .build()?;
        let env = |k: &str, d: &str| {
            std::env::var(k)
                .ok()
                .filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| d.to_string())
                .trim_end_matches('/')
                .to_string()
        };
        Ok(Self {
            client,
            interval,
            site_base: env("LAWPUB_MOFA_BASE_URL", SITE_BASE),
            search_base: env("LAWPUB_MOFA_TREATY_SEARCH_URL", SEARCH_BASE),
            cookie: Mutex::new(None),
            current_kind: Mutex::new(None),
        })
    }

    /// 正規の URL (データに残す方) を、env で差し替えたベース URL に置き換える。
    fn rebase(&self, url: &str) -> String {
        let u = url.replacen("http://", "https://", 1);
        if let Some(rest) = u.strip_prefix(SEARCH_BASE) {
            format!("{}{rest}", self.search_base)
        } else if let Some(rest) = u.strip_prefix(SITE_BASE) {
            format!("{}{rest}", self.site_base)
        } else {
            u
        }
    }

    fn remember_cookies(&self, resp: &reqwest::blocking::Response) {
        let mut jar: Vec<(String, String)> = self
            .cookie
            .lock()
            .unwrap()
            .as_deref()
            .unwrap_or_default()
            .split("; ")
            .filter_map(|kv| kv.split_once('='))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        for v in resp.headers().get_all(reqwest::header::SET_COOKIE) {
            let Some((k, val)) = v
                .to_str()
                .ok()
                .and_then(|s| s.split(';').next())
                .and_then(|kv| kv.split_once('='))
            else {
                continue;
            };
            jar.retain(|(n, _)| n != k.trim());
            jar.push((k.trim().to_string(), val.trim().to_string()));
        }
        if !jar.is_empty() {
            *self.cookie.lock().unwrap() = Some(
                jar.iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join("; "),
            );
        }
    }

    fn send(
        &self,
        req: reqwest::blocking::RequestBuilder,
        url: &str,
    ) -> Result<Option<reqwest::blocking::Response>> {
        std::thread::sleep(self.interval);
        let req = match self.cookie.lock().unwrap().as_deref() {
            Some(c) => req.header(reqwest::header::COOKIE, c),
            None => req,
        };
        let resp = req.send().with_context(|| format!("GET {url}"))?;
        self.remember_cookies(&resp);
        match resp.status() {
            reqwest::StatusCode::NOT_FOUND => Ok(None),
            reqwest::StatusCode::FORBIDDEN => anyhow::bail!(
                "{url}: 403 Forbidden (外務省の配信網が非ブラウザのクライアントを拒否している可能性)"
            ),
            _ => Ok(Some(
                resp.error_for_status()
                    .with_context(|| format!("GET {url}"))?,
            )),
        }
    }

    fn read_html(resp: reqwest::blocking::Response) -> Result<String> {
        let ct = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let bytes = resp.bytes()?;
        Ok(decode_html(&bytes, ct.as_deref()))
    }
}

impl TreatyProvider for HttpProvider {
    fn search_page(&self, kind: Kind, page: u32) -> Result<String> {
        let search_url = format!("{}/search2.php", self.search_base);
        let need_post = page == 0 || *self.current_kind.lock().unwrap() != Some(kind);
        if need_post {
            let form = [
                ("first", "1"),
                ("treaty_name", ""),
                ("conclude_country", kind.form_value()),
            ];
            let resp = self
                .send(self.client.post(&search_url).form(&form), &search_url)?
                .with_context(|| format!("{search_url} not found"))?;
            *self.current_kind.lock().unwrap() = Some(kind);
            let html = Self::read_html(resp)?;
            if page == 0 {
                return Ok(html);
            }
        }
        let url = format!("{search_url}?pID={page}");
        let resp = self
            .send(self.client.get(&url), &url)?
            .with_context(|| format!("{url} not found"))?;
        Self::read_html(resp)
    }

    fn get_html(&self, url: &str) -> Result<Option<String>> {
        let target = self.rebase(url);
        match self.send(self.client.get(&target), &target)? {
            Some(resp) => Ok(Some(Self::read_html(resp)?)),
            None => Ok(None),
        }
    }

    fn get_bytes(&self, url: &str) -> Result<Option<Fetched>> {
        let target = self.rebase(url);
        let Some(resp) = self.send(self.client.get(&target), &target)? else {
            return Ok(None);
        };
        let media_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.split(';').next().unwrap_or(s).trim().to_string());
        Ok(Some(Fetched {
            bytes: resp.bytes()?.to_vec(),
            media_type,
        }))
    }
}

/// 条約データ検索の一覧の正規 URL (出典表示用、`pID` は 0 始まり)。
pub fn search_page_url(page: u32) -> String {
    format!("{SEARCH_BASE}/search2.php?pID={page}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        decode_html(&std::fs::read(&path).unwrap(), None)
    }

    #[test]
    fn dates() {
        assert_eq!(parse_date("令和7年4月23日").as_deref(), Some("2025-04-23"));
        assert_eq!(parse_date("平成元年6月16日").as_deref(), Some("1989-06-16"));
        assert_eq!(
            parse_date("昭和55年10月25日　ハーグで作成").as_deref(),
            Some("1980-10-25")
        );
        assert_eq!(parse_date("2022年06月01日").as_deref(), Some("2022-06-01"));
        assert_eq!(parse_date("令和７年２月３０日"), None);
        assert_eq!(parse_date("-"), None);
        assert_eq!(parse_date("テキスト　（和文）"), None);
        assert_eq!(wareki_year("2025-07-04").as_deref(), Some("令和7年"));
        assert_eq!(wareki_year("2019-04-30").as_deref(), Some("平成31年"));
        assert_eq!(wareki_year("2019-05-01").as_deref(), Some("令和1年"));
        assert_eq!(wareki_year("1989-01-07").as_deref(), Some("昭和64年"));
    }

    #[test]
    fn bilateral_search_page() {
        let url = search_page_url(1);
        let p = parse_search_page(&fixture("search_bilateral_p2.html"), Kind::Bilateral, &url);
        assert_eq!(p.total, Some(8950));
        assert_eq!((p.page, p.pages), (Some(2), Some(299)));
        assert_eq!(p.items.len(), 3);
        let angola = &p.items[0];
        assert_eq!(angola.id, "A-R4-002");
        assert_eq!(
            angola.title,
            "債務救済措置（債務支払猶予方式）に関する日本国政府とアンゴラ共和国との間の二の交換公文"
        );
        assert_eq!(angola.html_url, None);
        assert_eq!(
            angola.pdf_urls,
            vec!["https://www.mofa.go.jp/mofaj/gaiko/treaty/pdfs/A-R4-002.pdf"]
        );
        let sweden = &p.items[1];
        assert_eq!(sweden.id, "A-R4-099");
        assert_eq!(
            sweden.title,
            "社会保障に関する日本国とスウェーデン王国との間の協定"
        );
        assert_eq!(
            sweden.short_title.as_deref(),
            Some("スウェーデンとの社会保障協定")
        );
        assert_eq!(sweden.effective_on.as_deref(), Some("2022-06-01"));
        assert_eq!(sweden.parties, vec!["スウェーデン"]);
        assert_eq!(
            sweden.html_url.as_deref(),
            Some("https://www.mofa.go.jp/mofaj/gaiko/treaty/htmls/A-R4-099.html")
        );
        assert_eq!(sweden.kind, Kind::Bilateral);
    }

    #[test]
    fn multilateral_search_pages() {
        let p = parse_search_page(
            &fixture("search_multilateral_p1.html"),
            Kind::Multilateral,
            &search_page_url(0),
        );
        assert_eq!(p.total, Some(1013));
        assert_eq!((p.page, p.pages), (Some(1), Some(34)));
        let rcep = p.items.iter().find(|i| i.id == "B-R4-001").unwrap();
        assert_eq!(rcep.title, "地域的な包括的経済連携協定");
        assert_eq!(rcep.short_title.as_deref(), Some("RCEP協定"));
        assert_eq!(rcep.pdf_urls.len(), 7);
        assert!(rcep.parties.is_empty());
        let fund = p
            .items
            .iter()
            .find(|i| i.title.starts_with("第三次"))
            .unwrap();
        assert_eq!(fund.id, "B-H31_R1-004");
        assert_eq!(fund.short_title, None);

        let last = parse_search_page(
            &fixture("search_multilateral_p34.html"),
            Kind::Multilateral,
            &search_page_url(33),
        );
        assert_eq!((last.page, last.pages), (Some(34), Some(34)));
        let ids: Vec<&str> = last.items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "B-S38-C1-787",
                "B-S38-C1-503-2",
                "B-S38-C1-503",
                "B-S38-C2-1442"
            ]
        );
        assert_eq!(last.items[1].title, "国際衛生条約(第百八条〜署名議定書)");
        assert!(last.items.iter().all(|i| i.effective_on.is_none()));
    }

    #[test]
    fn diet_index_and_sessions() {
        let s = parse_diet_index(&fixture("diet_index.html"), DIET_INDEX_URL);
        assert_eq!(s.first().map(|s| s.session), Some(221));
        assert_eq!(s.last().map(|s| s.session), Some(154));
        let s213: Vec<_> = s.iter().filter(|x| x.session == 213).collect();
        assert_eq!(s213.len(), 1);
        assert_eq!(
            s.iter().find(|x| x.session == 162).unwrap().url,
            "https://www.mofa.go.jp/mofaj/gaiko/treaty/treaty162.html"
        );
        // 「（提出した条約なし）」の回次はリンクが無いので出ない。
        assert!(s.iter().all(|x| x.session != 220));

        let url = "https://www.mofa.go.jp/mofaj/ila/st/pagew_000001_00001.html";
        let links = parse_diet_session(&fixture("diet_session_217.html"), url);
        assert_eq!(links.len(), 13);
        assert_eq!(links[0].label, "日・ウクライナ租税条約");
        assert_eq!(
            links[0].url,
            "https://www.mofa.go.jp/mofaj/ila/st/pagew_000001_00003.html"
        );
        let old = parse_diet_session(
            &fixture("diet_session_162.html"),
            "https://www.mofa.go.jp/mofaj/gaiko/treaty/treaty162.html",
        );
        assert_eq!(old.len(), 9);
        assert_eq!(
            old[3].url,
            "https://www.mofa.go.jp/mofaj/gaiko/treaty/treaty162_4.html"
        );
    }

    #[test]
    fn diet_detail_new_layout() {
        let url = "https://www.mofa.go.jp/mofaj/ila/st/pagew_000001_00003.html";
        let d = parse_diet_detail(
            &fixture("diet_pagew_000001_00003.html"),
            url,
            Some(217),
            "t",
        )
        .unwrap();
        assert_eq!(d.key, "pagew_000001_00003");
        assert_eq!(
            d.title,
            "所得に対する租税に関する二重課税の除去並びに脱税及び租税回避の防止のための日本国政府とウクライナ政府との間の条約"
        );
        assert_eq!(d.short_title.as_deref(), Some("日・ウクライナ租税条約"));
        assert_eq!(d.events.len(), 6);
        assert_eq!(d.events[2].text, "国会承認");
        assert_eq!(d.dates.signed_on.as_deref(), Some("2024-02-19"));
        assert_eq!(d.dates.diet_approved_on.as_deref(), Some("2025-04-23"));
        // 承認前の相手国からの通告ではなく、承認後の日本側からの通告。
        assert_eq!(d.dates.concluded_on.as_deref(), Some("2025-07-02"));
        assert_eq!(d.dates.promulgated_on.as_deref(), Some("2025-07-04"));
        assert_eq!(d.dates.effective_on.as_deref(), Some("2025-08-01"));
        assert_eq!(d.dates.treaty_number, Some(5));
        assert_eq!(
            d.dates.treaty_number_label.as_deref(),
            Some("令和7年条約第5号")
        );
        assert_eq!(d.dates.notice_number.as_deref(), Some("外務省告示第265号"));
        let labels: Vec<&str> = d.attachments.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels, vec!["和文", "英文", "説明書", "概要"]);
        assert_eq!(
            d.attachments[0].url,
            "https://www.mofa.go.jp/mofaj/files/100803932.pdf"
        );
        assert!(d.is_settled());
    }

    #[test]
    fn diet_detail_hague_and_old_layout() {
        let d = parse_diet_detail(
            &fixture("diet_page24_000001.html"),
            "https://www.mofa.go.jp/mofaj/gaiko/page24_000001.html",
            Some(183),
            "t",
        )
        .unwrap();
        assert_eq!(d.title, "国際的な子の奪取の民事上の側面に関する条約");
        assert_eq!(d.short_title.as_deref(), Some("ハーグ条約"));
        assert_eq!(d.dates.signed_on.as_deref(), Some("1980-10-25"));
        assert_eq!(d.dates.concluded_on.as_deref(), Some("2014-01-24"));
        assert_eq!(
            d.dates.treaty_number_label.as_deref(),
            Some("平成26年条約第2号")
        );
        assert_eq!(d.dates.effective_on.as_deref(), Some("2014-04-01"));

        let a = parse_diet_detail(
            &fixture("diet_treaty162_4.html"),
            "https://www.mofa.go.jp/mofaj/gaiko/treaty/treaty162_4.html",
            Some(162),
            "t",
        )
        .unwrap();
        assert_eq!(a.title, "石綿の使用における安全に関する条約（第162号）");
        assert_eq!(
            a.short_title.as_deref(),
            Some("石綿の使用における安全に関する条約")
        );
        assert_eq!(a.dates.signed_on.as_deref(), Some("1986-06-24"));
        // 国際的な効力発生 (平成元年) ではなく日本について効力を生じた日。
        assert_eq!(a.dates.effective_on.as_deref(), Some("2006-08-11"));
        assert_eq!(
            a.dates.treaty_number_label.as_deref(),
            Some("平成17年条約第11号")
        );
        assert_eq!(a.dates.notice_number.as_deref(), Some("外務省告示第775号"));
        assert_eq!(a.dates.concluded_on, None);
        assert_eq!(
            a.attachments[1].url,
            "https://www.mofa.go.jp/policy/treaty/submit/session162/agree-4.pdf"
        );
        assert_eq!(
            a.attachments[0].url,
            "https://www.mofa.go.jp/mofaj/gaiko/treaty/pdfs/treaty162_4a.pdf"
        );
    }

    #[test]
    fn treaty_text_bilingual_and_english_only() {
        let t = parse_treaty_text(&fixture("text_A-R4-101.html")).unwrap();
        assert_eq!(t.lang, "ja");
        assert!(
            t.text.starts_with(
                "所得に対する租税に関する二重課税の回避のための日本国とスイスとの間の条約を改正する議定書\n日本国政府及びスイス連邦政府は、"
            ),
            "{}",
            t.text
        );
        assert!(t.text.contains("第一条"));
        assert!(t
            .text
            .contains("スイス連邦政府のために\n　　シュテファン・フリュッキガー"));
        assert!(!t.text.contains("null"));
        assert!(!t.text.contains("PROTOCOL"));

        let e = parse_treaty_text(&fixture("text_A-H05-0073.html")).unwrap();
        assert_eq!(e.lang, "en");
        assert!(e
            .text
            .starts_with("(Japanese Note)\n　Dhaka, July 22, 1993"));
    }

    #[test]
    fn matches_diet_pages_to_database_rows() {
        let mk = |id: &str, title: &str, eff: Option<&str>| SearchItem {
            id: id.into(),
            kind: Kind::Multilateral,
            title: title.into(),
            short_title: None,
            effective_on: eff.map(str::to_string),
            parties: vec![],
            pdf_urls: vec![],
            html_url: None,
        };
        let items = vec![
            mk(
                "B-H26-001",
                "国際的な子の奪取の民事上の側面に関する条約",
                Some("2014-04-01"),
            ),
            mk(
                "B-H18-002",
                "石綿の使用における安全に関する条約（第百六十二号）",
                Some("2006-08-11"),
            ),
            mk("B-H30-003", "万国郵便条約", Some("2018-01-01")),
            mk("B-R4-006", "万国郵便条約", Some("2022-07-01")),
        ];
        let diet = |key: &str, title: &str, eff: Option<&str>| DietTreaty {
            key: key.into(),
            session: None,
            title: title.into(),
            short_title: None,
            events: vec![],
            dates: TreatyDates {
                effective_on: eff.map(str::to_string),
                ..Default::default()
            },
            attachments: vec![],
            url: format!("https://www.mofa.go.jp/{key}.html"),
            fetched_at: "t".into(),
        };
        let diets = vec![
            diet(
                "page24_000001",
                "国際的な子の奪取の民事上の側面に関する条約",
                Some("2014-04-01"),
            ),
            diet(
                "treaty162_4",
                "石綿の使用における安全に関する条約（第162号）",
                Some("2006-08-11"),
            ),
            diet("upu2018", "万国郵便条約", Some("2018-01-01")),
            diet("pagew_x", "日本国政府とウクライナ政府との間の条約", None),
        ];
        let m = match_diet_to_items(&items, &diets);
        assert_eq!(
            m.get("page24_000001").map(String::as_str),
            Some("B-H26-001")
        );
        assert_eq!(m.get("treaty162_4").map(String::as_str), Some("B-H18-002"));
        assert_eq!(m.get("upu2018").map(String::as_str), Some("B-H30-003"));
        assert_eq!(m.get("pagew_x"), None);

        // 条約データ検索に未掲載の国会提出条約は diet-{key} として出す。
        let t = merge(None, Some(&diets[3]), None).unwrap();
        assert_eq!(t.id, "diet-pagew_x");
        assert!(!t.in_database);
        let t = merge(Some(&items[0]), Some(&diets[0]), None).unwrap();
        assert_eq!(t.id, "B-H26-001");
        assert!(t.in_database);
        assert_eq!(
            t.diet_url.as_deref(),
            Some("https://www.mofa.go.jp/page24_000001.html")
        );
    }

    #[test]
    fn mock_provider_serves_fixtures() {
        let p = MockProvider;
        let html = p.search_page(Kind::Multilateral, 0).unwrap();
        assert_eq!(
            parse_search_page(&html, Kind::Multilateral, &search_page_url(0))
                .items
                .len(),
            4
        );
        assert!(p.get_html(DIET_INDEX_URL).unwrap().is_some());
        assert!(p
            .get_html("https://www.mofa.go.jp/mofaj/unknown.html")
            .unwrap()
            .is_none());
    }

    #[test]
    #[ignore = "外務省サイトへの実アクセス"]
    fn live_search_first_page() {
        let p = HttpProvider::new(Duration::from_millis(1000)).unwrap();
        let html = p.search_page(Kind::Multilateral, 0).unwrap();
        let page = parse_search_page(&html, Kind::Multilateral, &search_page_url(0));
        assert!(!page.items.is_empty());
        assert!(page.pages.unwrap_or(0) > 1);
        let html2 = p.search_page(Kind::Multilateral, 1).unwrap();
        let page2 = parse_search_page(&html2, Kind::Multilateral, &search_page_url(1));
        assert_eq!(page2.page, Some(2));
    }
}
