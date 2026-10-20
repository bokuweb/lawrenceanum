//! 金融庁 (FSA) アダプタ。
//!
//! - ノーアクションレター等: `/common/noact/index.html` の「照会に対する回答」→
//!   法令別ページ `/common/noact/kaitou_2/{法令}.html` → 一覧 `/common/noact/kaitou_2/{dir}/index.html`。
//!   一覧は `<h2>` (手続の種類) ごとに `table.kaitou` (整理番号 / 該当条文 / 照会日と照会書 /
//!   回答日と回答書) が並び、相談事例は Excel へのリンク。法令名は `<h1>`。
//!   照会書・回答書は PDF (古いものは Shift_JIS の HTML)。照会書はスキャン PDF のこともある。
//! - Q&A・ガイドライン: `/common/law/kokuji.html` の `<h3>` (分野) ごとの `<li>`。
//!   PDF 直リンクを文書として取り、HTML は Q&A ページだけ本文を取る (分野の索引ページは辿らない)。
//!   英語版は落とす。
//!
//! ライセンス (PDL1.0) と robots.txt はクレート先頭の doc comment を参照。

use crate::{
    abs_url, ascii_digits, sel, short_hash, squash, text_of, wareki_to_iso, DocLink, DocRole,
    Fetched, GuidanceKind, GuidanceProvider, LawRef, Listing, Ministry,
};
use anyhow::{Context, Result};
use scraper::{ElementRef, Html, Node};
use std::collections::HashSet;
use std::time::Duration;

pub const BASE_URL: &str = "https://www.fsa.go.jp";
pub const NOACT_INDEX_PATH: &str = "/common/noact/index.html";
pub const KOKUJI_INDEX_PATH: &str = "/common/law/kokuji.html";

pub static MINISTRY: Ministry = Ministry {
    slug: "fsa",
    name: "金融庁",
    license: "PDL1.0",
    attribution: "出典：金融庁ウェブサイト（{url}）。本文は公表 PDF/HTML から機械的に抽出したもの（lawrenceanum が加工）。",
};

fn listing(kind: GuidanceKind, key: &str, title: String, source_url: &str) -> Listing {
    Listing {
        id: format!("{}-{}-{key}", MINISTRY.slug, kind.id_prefix()),
        ministry: MINISTRY.slug.to_string(),
        ministry_name: MINISTRY.name.to_string(),
        kind,
        number: None,
        title,
        issued_on: None,
        inquired_on: None,
        related_laws: Vec::new(),
        category: None,
        note: None,
        links: Vec::new(),
        source_url: source_url.to_string(),
    }
}

/// `#main` 枠 (無ければ文書全体)。サイト共通のヘッダ・サイトマップを拾わないため。
fn main_of(doc: &Html) -> ElementRef<'_> {
    doc.select(&sel("#main"))
        .next()
        .unwrap_or_else(|| doc.root_element())
}

/// 「金融商品取引法（旧 証券取引法）」→「金融商品取引法」。
pub fn strip_former_name(s: &str) -> String {
    let s = squash(s);
    let cut = ["（旧", "(旧", "（ 旧", "( 旧"]
        .iter()
        .filter_map(|p| s.find(p))
        .min()
        .unwrap_or(s.len());
    s[..cut].trim().to_string()
}

// ── ノーアクションレター等 ────────────────────────────────────────

/// `/common/noact/index.html` から法令別ページ (`kaitou_2/{法令}.html`) の URL を集める。
pub fn parse_noact_index(html: &str, page_url: &str) -> Vec<String> {
    let doc = Html::parse_document(html);
    let mut seen = HashSet::new();
    main_of(&doc)
        .select(&sel("a[href]"))
        .filter_map(|a| a.value().attr("href"))
        .filter(|h| {
            let path = h.split('#').next().unwrap_or(h);
            path.contains("/noact/kaitou_2/")
                && path.ends_with(".html")
                && !path.ends_with("/index.html")
        })
        .map(|h| abs_url(page_url, h))
        .filter(|u| seen.insert(u.clone()))
        .collect()
}

/// 法令別ページから一覧ページ (`kaitou_2/{dir}/index.html`) の URL を集める (アンカー違いは 1 つに)。
pub fn parse_noact_law_page(html: &str, page_url: &str) -> Vec<String> {
    let doc = Html::parse_document(html);
    let mut seen = HashSet::new();
    main_of(&doc)
        .select(&sel("a[href]"))
        .filter_map(|a| a.value().attr("href"))
        .filter(|h| {
            h.contains("/noact/kaitou_2/")
                && h.split('#').next().unwrap_or(h).ends_with("/index.html")
        })
        .map(|h| abs_url(page_url, h))
        .filter(|u| seen.insert(u.clone()))
        .collect()
}

#[derive(Clone, Copy, PartialEq)]
enum Section {
    None,
    NoAction,
    Inquiry,
    Cases,
}

/// 一覧ページ URL の法令ディレクトリ名 (`…/kaitou_2/ginkou/index.html` → `ginkou`)。
fn law_dir(page_url: &str) -> String {
    page_url
        .split('/')
        .rev()
        .nth(1)
        .filter(|s| !s.is_empty())
        .unwrap_or("unknown")
        .to_string()
}

fn parse_number(s: &str) -> Option<u32> {
    let digits: String = ascii_digits(s)
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

/// 「第10条第２項、第12条<br>※…と同内容」→ (該当条文, 備考)。
fn split_note(s: &str) -> (String, Option<String>) {
    let s = squash(s);
    match s.find('※') {
        Some(i) => {
            let note = s[i..].trim().to_string();
            (
                s[..i].trim().to_string(),
                (!note.is_empty()).then_some(note),
            )
        }
        None => (s, None),
    }
}

/// 法令別の一覧ページ (`kaitou_2/{dir}/index.html`) をパースする。
pub fn parse_noact_listing(html: &str, page_url: &str) -> Vec<Listing> {
    let doc = Html::parse_document(html);
    let main = main_of(&doc);
    let law_name = main
        .select(&sel("h1"))
        .next()
        .map(|h| strip_former_name(&text_of(h)))
        .unwrap_or_default();
    let dir = law_dir(page_url);
    let (td, a) = (sel("td"), sel("a[href]"));
    let mut out: Vec<Listing> = Vec::new();
    let mut section = Section::None;
    for el in main.select(&sel("h2, table.kaitou tr, li")) {
        match el.value().name() {
            "h2" => {
                let t = text_of(el);
                section = if t.contains("ノーアクション") || t.contains("法令適用事前確認")
                {
                    Section::NoAction
                } else if t.contains("書面照会") {
                    Section::Inquiry
                } else if t.contains("相談事例") {
                    Section::Cases
                } else {
                    Section::None
                };
            }
            "tr" if matches!(section, Section::NoAction | Section::Inquiry) => {
                let cells: Vec<ElementRef> = el.select(&td).collect();
                if cells.len() < 4 {
                    continue; // 見出し行
                }
                let Some(number) = parse_number(&text_of(cells[0])) else {
                    continue;
                };
                let (provisions, note) = split_note(&text_of(cells[1]));
                let link_of = |cell: ElementRef, role: DocRole| {
                    cell.select(&a).next().map(|anchor| DocLink {
                        role,
                        label: text_of(anchor),
                        url: abs_url(page_url, anchor.value().attr("href").unwrap_or("")),
                    })
                };
                let inquiry = link_of(cells[2], DocRole::Inquiry);
                let answer = link_of(cells[3], DocRole::Answer);
                if answer.is_none() && inquiry.is_none() {
                    continue;
                }
                let (kind, what) = if section == Section::NoAction {
                    (GuidanceKind::NoActionLetter, "ノーアクションレター")
                } else {
                    (
                        GuidanceKind::WrittenInquiry,
                        "一般的な法令解釈に係る書面照会",
                    )
                };
                let title = if provisions.is_empty() {
                    format!("{law_name} に関する{what}（整理番号{number}）")
                } else {
                    format!("{law_name} {provisions} に関する{what}（整理番号{number}）")
                };
                let mut l = listing(kind, &format!("{dir}-{number}"), title, page_url);
                l.number = Some(number);
                l.inquired_on = wareki_to_iso(&text_of(cells[2]));
                l.issued_on = wareki_to_iso(&text_of(cells[3]));
                l.related_laws = vec![LawRef {
                    provisions: (!provisions.is_empty()).then_some(provisions),
                    ..LawRef::named(law_name.clone())
                }];
                l.note = note;
                l.links = inquiry.into_iter().chain(answer).collect();
                out.push(l);
            }
            "li" if section == Section::Cases => {
                let Some(anchor) = el.select(&a).next() else {
                    continue;
                };
                let label = text_of(anchor);
                let mut l = listing(
                    GuidanceKind::ConsultationCases,
                    &dir,
                    format!("{law_name} 広く共有することが有効な相談事例"),
                    page_url,
                );
                l.issued_on = wareki_to_iso(&label);
                l.related_laws = vec![LawRef::named(law_name.clone())];
                l.links = vec![DocLink {
                    role: DocRole::Document,
                    label,
                    url: abs_url(page_url, anchor.value().attr("href").unwrap_or("")),
                }];
                out.push(l);
            }
            _ => {}
        }
    }
    dedup_by_id(out)
}

fn dedup_by_id(v: Vec<Listing>) -> Vec<Listing> {
    let mut seen = HashSet::new();
    v.into_iter()
        .filter(|l| seen.insert(l.id.clone()))
        .collect()
}

// ── Q&A・ガイドライン ─────────────────────────────────────────────

/// 分野見出しから法令名を推定する (「銀行法等関連」→「銀行法」)。法令名で終わらなければ None。
pub fn category_law(category: &str) -> Option<String> {
    let mut s = squash(category);
    if let Some(i) = s.find(['（', '(']) {
        s.truncate(i);
    }
    let s = s.trim_end_matches("関連").trim_end_matches('等');
    s.ends_with('法').then(|| s.to_string())
}

/// URL 中の `YYYYMMDD` (報道発表の日付ディレクトリ・ファイル名) を公表日とみなす。
pub fn date_from_url(url: &str) -> Option<String> {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    for seg in path.split(['/', '-', '_', '.']) {
        if seg.len() == 8 && seg.chars().all(|c| c.is_ascii_digit()) {
            let (y, m, d) = (&seg[..4], &seg[4..6], &seg[6..]);
            if let (Ok(y), Ok(m), Ok(d)) = (y.parse(), m.parse(), d.parse()) {
                if (1990..=2100).contains(&y) {
                    if let Some(date) = chrono::NaiveDate::from_ymd_opt(y, m, d) {
                        return Some(date.format("%Y-%m-%d").to_string());
                    }
                }
            }
        }
    }
    None
}

fn is_doc_href(href: &str) -> bool {
    let h = href
        .split(['?', '#'])
        .next()
        .unwrap_or(href)
        .to_ascii_lowercase();
    h.ends_with(".pdf")
}

fn is_html_href(href: &str) -> bool {
    let h = href
        .split(['?', '#'])
        .next()
        .unwrap_or(href)
        .to_ascii_lowercase();
    h.ends_with(".html") || h.ends_with(".htm")
}

/// li 直下 (入れ子の ul を除く) の a 要素とテキスト。
fn direct_parts(li: ElementRef) -> (Vec<ElementRef>, String) {
    let mut anchors = Vec::new();
    let mut text = String::new();
    fn walk<'a>(el: ElementRef<'a>, anchors: &mut Vec<ElementRef<'a>>, text: &mut String) {
        for child in el.children() {
            match child.value() {
                Node::Text(t) => text.push_str(t),
                Node::Element(e) if e.name() == "ul" || e.name() == "ol" => {}
                Node::Element(e) => {
                    if let Some(c) = ElementRef::wrap(child) {
                        if e.name() == "a" {
                            anchors.push(c);
                            text.push_str(&c.text().collect::<String>());
                        } else {
                            walk(c, anchors, text);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    walk(li, &mut anchors, &mut text);
    (anchors, squash(&text))
}

const FORMAT_LABELS: &[&str] = &["HTML版", "Word版", "PDF版", "EXCEL版", "Excel版"];

/// 「告示・ガイドライン・Q&A・法令解釈事例集一覧」をパースする。
pub fn parse_kokuji(html: &str, page_url: &str) -> Vec<Listing> {
    let doc = Html::parse_document(html);
    let main = main_of(&doc);
    let mut out = Vec::new();
    let mut category: Option<String> = None;
    for el in main.select(&sel("h3, li")) {
        if el.value().name() == "h3" {
            category = Some(text_of(el)).filter(|c| !c.is_empty());
            continue;
        }
        let (anchors, li_text) = direct_parts(el);
        let anchors: Vec<(String, String)> = anchors
            .into_iter()
            .map(|a| (text_of(a), a.value().attr("href").unwrap_or("").to_string()))
            .filter(|(t, h)| !h.is_empty() && !t.contains("英語版") && !t.contains("English"))
            .collect();
        let Some((label, href)) = anchors
            .iter()
            .find(|(_, h)| is_doc_href(h))
            .or_else(|| anchors.iter().find(|(_, h)| is_html_href(h)))
            .cloned()
        else {
            continue;
        };
        let title = if FORMAT_LABELS.iter().any(|f| label.contains(f)) {
            // 「貸出条件緩和債権関係Q&A（HTML版・Word版・PDF版）」は li 先頭の文言が文書名。
            let t = li_text.split(['（', '(']).next().unwrap_or(&li_text).trim();
            t.to_string()
        } else {
            label.replace('\u{a0}', " ")
        };
        let title = squash(&title);
        if title.is_empty() {
            continue;
        }
        let kind = GuidanceKind::classify(&title);
        // HTML は Q&A ページだけ文書とみなす (それ以外は分野の索引ページ)。
        if !is_doc_href(&href) && kind != GuidanceKind::Qa {
            continue;
        }
        let url = abs_url(page_url, &href);
        let mut l = listing(kind, &short_hash(&title), title.clone(), page_url);
        l.issued_on = date_from_url(&url);
        l.category = category.clone();
        l.related_laws = category
            .as_deref()
            .and_then(category_law)
            .map(LawRef::named)
            .into_iter()
            .collect();
        l.links = vec![DocLink {
            role: DocRole::Document,
            label: title,
            url,
        }];
        out.push(l);
    }
    dedup_by_id(out)
}

/// HTML の Q&A ページ (報道発表ページを含む) から、本文として取るべき Q&A の PDF 版を返す。
pub fn qa_pdf_links(html: &str, page_url: &str) -> Vec<DocLink> {
    let doc = Html::parse_document(html);
    let mut seen = HashSet::new();
    main_of(&doc)
        .select(&sel("a[href]"))
        .filter_map(|a| {
            let href = a.value().attr("href")?;
            let label = text_of(a);
            (is_doc_href(href)
                && GuidanceKind::classify(&label) == GuidanceKind::Qa
                && !label.contains("英語版"))
            .then(|| DocLink {
                role: DocRole::Document,
                label,
                url: abs_url(page_url, href),
            })
        })
        .filter(|l| seen.insert(l.url.clone()))
        .take(5)
        .collect()
}

// ── HTTP ─────────────────────────────────────────────────────────

pub struct HttpProvider {
    client: reqwest::blocking::Client,
    base_url: String,
    interval: Duration,
}

impl HttpProvider {
    /// `LAWPUB_FSA_BASE_URL` で base URL を上書きできる (既定 `https://www.fsa.go.jp`)。
    pub fn new(interval: Duration) -> Result<Self> {
        let base_url = std::env::var("LAWPUB_FSA_BASE_URL")
            .unwrap_or_else(|_| BASE_URL.to_string())
            .trim_end_matches('/')
            .to_string();
        let client = reqwest::blocking::Client::builder()
            .user_agent("lawrenceanum/0.1 (+https://github.com/bokuweb/lawrenceanum)")
            .timeout(Duration::from_secs(60))
            .build()?;
        Ok(Self {
            client,
            base_url,
            interval,
        })
    }

    fn get(&self, url: &str) -> Result<Fetched> {
        std::thread::sleep(self.interval);
        let resp = self
            .client
            .get(url)
            .send()
            .and_then(|r| r.error_for_status())
            .with_context(|| format!("GET {url}"))?;
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        // 添付は大きくても数 MB。異常に大きいものは取らない。
        if resp.content_length().is_some_and(|n| n > 50 * 1024 * 1024) {
            anyhow::bail!("too large: {url}");
        }
        let bytes = resp
            .bytes()
            .with_context(|| format!("read {url}"))?
            .to_vec();
        Ok(Fetched {
            bytes,
            content_type,
            fetched_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    fn get_html(&self, url: &str) -> Result<String> {
        let f = self.get(url)?;
        Ok(crate::decode_html(&f.bytes, f.content_type.as_deref()))
    }
}

impl GuidanceProvider for HttpProvider {
    fn ministry(&self) -> &'static Ministry {
        &MINISTRY
    }

    fn list(&self) -> Result<Vec<Listing>> {
        let mut out = Vec::new();
        let index_url = format!("{}{NOACT_INDEX_PATH}", self.base_url);
        let index = self.get_html(&index_url)?;
        let mut listing_urls = Vec::new();
        for law_page in parse_noact_index(&index, &index_url) {
            match self.get_html(&law_page) {
                Ok(html) => listing_urls.extend(parse_noact_law_page(&html, &law_page)),
                Err(e) => tracing::warn!("guidance fsa: skip {law_page}: {e:#}"),
            }
        }
        listing_urls.dedup();
        for url in listing_urls {
            match self.get_html(&url) {
                Ok(html) => {
                    let items = parse_noact_listing(&html, &url);
                    tracing::info!("guidance fsa: {url}: {} documents", items.len());
                    out.extend(items);
                }
                Err(e) => tracing::warn!("guidance fsa: skip {url}: {e:#}"),
            }
        }
        let kokuji_url = format!("{}{KOKUJI_INDEX_PATH}", self.base_url);
        match self.get_html(&kokuji_url) {
            Ok(html) => {
                let items = parse_kokuji(&html, &kokuji_url);
                tracing::info!("guidance fsa: {kokuji_url}: {} documents", items.len());
                out.extend(items);
            }
            Err(e) => tracing::warn!("guidance fsa: skip {kokuji_url}: {e:#}"),
        }
        Ok(dedup_by_id(out))
    }

    fn fetch(&self, url: &str) -> Result<Fetched> {
        self.get(url)
    }

    fn follow_links(&self, page_url: &str, html: &str) -> Vec<DocLink> {
        qa_pdf_links(html, page_url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = format!("{}/tests/fixtures/fsa/{name}", env!("CARGO_MANIFEST_DIR"));
        crate::decode_html(&std::fs::read(&path).unwrap(), None)
    }

    const NOACT: &str = "https://www.fsa.go.jp/common/noact";

    #[test]
    fn noact_index_lists_law_pages() {
        let urls = parse_noact_index(&fixture("noact_index.html"), &format!("{NOACT}/index.html"));
        assert_eq!(urls.len(), 10, "{urls:?}");
        assert!(urls.contains(&format!("{NOACT}/kaitou_2/ginkou.html")));
        assert!(urls.contains(&format!("{NOACT}/kaitou_2/kinsyou_2.html")));
    }

    #[test]
    fn law_page_lists_listing_once() {
        let urls = parse_noact_law_page(
            &fixture("kaitou_2_ginkou.html"),
            &format!("{NOACT}/kaitou_2/ginkou.html"),
        );
        assert_eq!(urls, vec![format!("{NOACT}/kaitou_2/ginkou/index.html")]);
    }

    #[test]
    fn ginkou_listing() {
        let url = format!("{NOACT}/kaitou_2/ginkou/index.html");
        let items = parse_noact_listing(&fixture("kaitou_2_ginkou_index.html"), &url);
        let nal: Vec<_> = items
            .iter()
            .filter(|l| l.kind == GuidanceKind::NoActionLetter)
            .collect();
        assert_eq!(nal.len(), 10);
        let n10 = nal.iter().find(|l| l.number == Some(10)).unwrap();
        assert_eq!(n10.id, "fsa-nal-ginkou-10");
        assert_eq!(n10.issued_on.as_deref(), Some("2010-03-25"));
        assert_eq!(n10.inquired_on.as_deref(), Some("2010-03-04"));
        assert_eq!(n10.related_laws[0].name, "銀行法");
        assert_eq!(
            n10.related_laws[0].provisions.as_deref(),
            Some("第10条第２項、第12条")
        );
        assert_eq!(n10.links.len(), 2);
        assert_eq!(n10.links[0].role, DocRole::Inquiry);
        assert_eq!(
            n10.links[1].url,
            "https://www.fsa.go.jp/common/noact/kaitou/001/001_10b.pdf"
        );
        assert_eq!(
            n10.title,
            "銀行法 第10条第２項、第12条 に関するノーアクションレター（整理番号10）"
        );
        // 全角の整理番号・日付 (「平成16年6月１日」)。
        let n7 = nal.iter().find(|l| l.number == Some(7)).unwrap();
        assert_eq!(n7.issued_on.as_deref(), Some("2004-06-01"));

        let inquiry: Vec<_> = items
            .iter()
            .filter(|l| l.kind == GuidanceKind::WrittenInquiry)
            .collect();
        assert_eq!(inquiry.len(), 1);
        assert_eq!(inquiry[0].id, "fsa-inquiry-ginkou-1");
        assert_eq!(
            inquiry[0].related_laws[0].provisions.as_deref(),
            Some("第13条の３第４号")
        );
        assert_eq!(
            inquiry[0].note.as_deref(),
            Some("※金融商品取引法に係る同手続整理番号４と同内容")
        );
        assert_eq!(inquiry[0].issued_on.as_deref(), Some("2021-03-26"));

        let cases: Vec<_> = items
            .iter()
            .filter(|l| l.kind == GuidanceKind::ConsultationCases)
            .collect();
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].id, "fsa-cases-ginkou");
        assert_eq!(cases[0].issued_on.as_deref(), Some("2022-07-15"));
        assert!(cases[0].links[0]
            .url
            .ends_with("/common/noact/soudan/ginkou.xlsx"));
    }

    #[test]
    fn kinsyou_listing_strips_former_name() {
        let url = format!("{NOACT}/kaitou_2/kinsyou/index.html");
        let items = parse_noact_listing(&fixture("kaitou_2_kinsyou_index.html"), &url);
        assert_eq!(
            items
                .iter()
                .filter(|l| l.kind == GuidanceKind::NoActionLetter)
                .count(),
            8
        );
        assert_eq!(
            items
                .iter()
                .filter(|l| l.kind == GuidanceKind::WrittenInquiry)
                .count(),
            4
        );
        let n8 = items.iter().find(|l| l.id == "fsa-nal-kinsyou-8").unwrap();
        assert_eq!(n8.related_laws[0].name, "金融商品取引法");
        assert_eq!(n8.issued_on.as_deref(), Some("2025-11-28"));
        assert!(n8.links[1]
            .url
            .ends_with("/kaitou_2/kinsyou/kaitousyo8.pdf"));
    }

    #[test]
    fn hoken_listing_with_scope_headers() {
        let url = format!("{NOACT}/kaitou_2/hoken/index.html");
        let items = parse_noact_listing(&fixture("kaitou_2_hoken_index.html"), &url);
        assert_eq!(
            items
                .iter()
                .filter(|l| l.kind == GuidanceKind::NoActionLetter)
                .count(),
            24
        );
        let cases = items.iter().find(|l| l.id == "fsa-cases-hoken").unwrap();
        assert_eq!(cases.issued_on.as_deref(), Some("2026-06-26"));
    }

    #[test]
    fn kokuji_qa_and_guidelines() {
        let url = format!("{BASE_URL}{KOKUJI_INDEX_PATH}");
        let items = parse_kokuji(&fixture("kokuji.html"), &url);
        let titles: Vec<&str> = items.iter().map(|l| l.title.as_str()).collect();
        // 英語版・Excel の告示一覧・分野の索引ページは文書にしない。
        assert!(!titles.iter().any(|t| t.contains("英語版")));
        assert!(!titles.contains(&"所管告示一覧"));
        assert!(!titles.contains(&"金融分野におけるサイバーセキュリティ対策"));
        assert!(!titles.contains(&"証券法令解釈事例集"));

        let insider = items
            .iter()
            .find(|l| l.title == "インサイダー取引規制に関するQ&A")
            .unwrap();
        assert_eq!(insider.kind, GuidanceKind::Qa);
        assert_eq!(
            insider.category.as_deref(),
            Some("金融商品取引法等関連（企業開示関連を除く）")
        );
        assert_eq!(insider.related_laws, vec![LawRef::named("金融商品取引法")]);
        assert_eq!(
            insider.links[0].url,
            "https://www.fsa.go.jp/common/law/insider_qa_.pdf"
        );
        assert!(insider.id.starts_with("fsa-qa-"));

        let gyou = items
            .iter()
            .find(|l| l.title == "金融商品取引業等に関するQ&A")
            .unwrap();
        assert_eq!(gyou.issued_on.as_deref(), Some("2026-08-27"));

        let aml = items
            .iter()
            .find(|l| l.title == "マネー・ローンダリング及びテロ資金供与対策に関するガイドライン")
            .unwrap();
        assert_eq!(aml.kind, GuidanceKind::Guideline);
        assert_eq!(aml.issued_on.as_deref(), Some("2026-03-31"));
        assert!(aml.related_laws.is_empty());

        // HTML の Q&A は本文ページとして取る。
        let kashikin = items
            .iter()
            .find(|l| l.title == "貸金業法に関するQ&A")
            .unwrap();
        assert!(kashikin.links[0].url.ends_with("/policy/kashikin/qa.html"));
        assert_eq!(kashikin.related_laws, vec![LawRef::named("貸金業法")]);

        // HTML版・Word版・PDF版が並ぶ項目は PDF 版を取り、文書名は li 先頭の文言。
        let kanwa = items
            .iter()
            .find(|l| l.title == "貸出条件緩和債権関係Q&A")
            .unwrap();
        assert!(kanwa.links[0]
            .url
            .ends_with("/common/law/guide/q_and_a/01.pdf"));

        let hoken = items
            .iter()
            .find(|l| l.title.starts_with("保険業該当性に関するQ"))
            .unwrap();
        assert_eq!(hoken.kind, GuidanceKind::Qa);
        assert_eq!(hoken.related_laws, vec![LawRef::named("保険業法")]);

        let ids: HashSet<&str> = items.iter().map(|l| l.id.as_str()).collect();
        assert_eq!(ids.len(), items.len());
        assert!(items.len() >= 25, "{}", items.len());
    }

    #[test]
    fn qa_pdf_from_press_release() {
        let url = "https://www.fsa.go.jp/news/r6/shouken/20250627/20250627.html";
        let links = qa_pdf_links(&fixture("news_r6_shouken_20250627.html"), url);
        assert_eq!(links.len(), 1);
        assert_eq!(
            links[0].url,
            "https://www.fsa.go.jp/news/r6/shouken/20250627/01.pdf"
        );
    }

    #[test]
    fn shift_jis_answer_html() {
        let html = fixture("kaitou_004_01b.html");
        let text = crate::html_main_text(&html);
        assert!(
            text.contains("金融先物取引法に関する法令適用事前確認手続"),
            "{text}"
        );
        assert_eq!(
            crate::find_issuer(&text, "金融庁").as_deref(),
            Some("金融庁総務企画局市場課長")
        );
        let summary = crate::answer_summary(&text).unwrap();
        assert!(summary.starts_with("金融先物取引法は"), "{summary}");
    }

    #[test]
    fn helpers() {
        assert_eq!(
            strip_former_name("貸金業法 （旧 貸金業の規制等に関する法律）"),
            "貸金業法"
        );
        assert_eq!(category_law("銀行法等関連").as_deref(), Some("銀行法"));
        assert_eq!(category_law("保険業法関連").as_deref(), Some("保険業法"));
        assert_eq!(category_law("個人情報保護関連"), None);
        assert_eq!(
            date_from_url("https://www.fsa.go.jp/news/r5/sonota/20240329-7/03.pdf").as_deref(),
            Some("2024-03-29")
        );
        assert_eq!(
            date_from_url("https://www.fsa.go.jp/news/r2/shouken/2020722-1/01.pdf"),
            None
        );
        assert_eq!(
            date_from_url("https://www.fsa.go.jp/common/law/insider_qa_.pdf"),
            None
        );
    }

    #[test]
    #[ignore]
    fn http_real_list() {
        let p = HttpProvider::new(Duration::from_millis(1000)).unwrap();
        let items = p.list().unwrap();
        let count = |k| items.iter().filter(|l| l.kind == k).count();
        println!(
            "total={} nal={} inquiry={} cases={} qa={} guideline={}",
            items.len(),
            count(GuidanceKind::NoActionLetter),
            count(GuidanceKind::WrittenInquiry),
            count(GuidanceKind::ConsultationCases),
            count(GuidanceKind::Qa),
            count(GuidanceKind::Guideline),
        );
        assert!(count(GuidanceKind::NoActionLetter) > 50);
    }
}
