//! 法務省「日本法令外国語訳データベース」(JLT, Japanese Law Translation) の英訳クライアント。
//!
//! # 利用条件 (2026-10-20 確認)
//!
//! - サイト利用規約 <https://www.japaneselawtranslation.go.jp/ja/index/terms> は
//!   「権利表記の記載がない限り公共データ利用規約（第1.0版）(PDL1.0) を適用、著作権は法務省」。
//!   適用除外はロゴマーク・キャラクターのみ。各ページの注意書きも「掲載データは利用規約に従い
//!   引用・複製・転載して差し支えありません」。
//! - PDL1.0 <https://www.digital.go.jp/resources/open_data/public_data_license_v1.0> は
//!   複製・公衆送信・翻案・商用利用を認め、CC BY 4.0 と互換。条件は **出典の記載**、
//!   **編集・加工した場合はその旨と主体の記載**、加工物を国が作成したかのように見せないこと。
//! - 注意書き: 翻訳は正文ではなく最終改正版でないものも含む参考資料。法的効力は日本語の法令のみ。
//!   「（暫定版）」はネイティブ・専門家のチェック前。→ 出力 JSON と SPA に出典・加工の旨・
//!   参考訳である旨を必ず載せる ([`ATTRIBUTION_JA`] / [`DISCLAIMER_JA`] / [`DISCLAIMER_EN`])。
//! - robots.txt は不在 (404)。公式 API・一括ダウンロードは無い (法令ごとの txt/docx/pdf/tmx は
//!   あるが、日英の条単位の対応が取れるのは「日英併記」HTML)。
//!
//! # 取得経路
//!
//! - 一覧: `POST /ja/laws/result/?page=N` に `_csrfToken` (+ 同名 cookie) と分野 `bu`
//!   (2,4,…,4096 のビット) を送る。全分野同時は `error/too_many` になるので分野ごとに引く。
//!   検索条件はサーバに保存されないので、ページ送りでも条件を毎回送る。`ia=03` で暫定版も含める。
//! - 本文: `GET /ja/laws/view/{id}/je` (日英併記)。`{id}` は翻訳版ごとの ID で、翻訳が更新されると
//!   新しい ID になる (旧 ID は最新版へ redirect)。`div.Article` に日英の
//!   `ArticleCaption` / `ParagraphSentence` / `ItemSentence` が交互に並ぶ。
//!
//! base URL は `LAWPUB_JLT_BASE_URL` で上書きできる。リクエスト間隔は [`HttpProvider`] が空ける。

use anyhow::{bail, Context, Result};
use scraper::{ElementRef, Html, Node, Selector};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

pub const BASE_URL: &str = "https://www.japaneselawtranslation.go.jp";
pub const TERMS_URL: &str = "https://www.japaneselawtranslation.go.jp/ja/index/terms";
pub const PDL_URL: &str = "https://www.digital.go.jp/resources/open_data/public_data_license_v1.0";
pub const SOURCE_NAME_JA: &str = "日本法令外国語訳データベース（法務省）";
pub const SOURCE_NAME_EN: &str =
    "Japanese Law Translation Database System (Ministry of Justice, Japan)";
/// PDL1.0 1.1 の出典記載 + 編集・加工の旨。
pub const ATTRIBUTION_JA: &str = "出典：日本法令外国語訳データベース（法務省）（https://www.japaneselawtranslation.go.jp/）、PDL1.0（https://www.digital.go.jp/resources/open_data/public_data_license_v1.0）。条単位への分割と e-Gov 法令 ID への対応付けは lawrenceanum が加工。";
pub const DISCLAIMER_JA: &str = "翻訳は正文ではなく、最終改正版でない法令も含まれる参考資料です。法的効力を有するのは日本語の法令のみです。";
pub const DISCLAIMER_EN: &str = "These translations are not official texts and may not reflect the latest amendments. Only the original Japanese texts of the laws and regulations have legal effect; the translations are reference materials only.";

/// 一覧検索の分野 (`bu`)。憲法・行政一般 / 司法 / 民事・商事 / 刑事・警察 / … の 12 区分。
pub const CATEGORIES: [u32; 12] = [2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096];

pub const SCHEMA_VERSION: u32 = 1;

/// 法令番号で e-Gov に引けない翻訳の手動対応表 `{ "<jlt_id>": { "law_id": "...", "note": "..." } }`。
pub const LAW_ID_OVERRIDES: &str = include_str!("../data/law_id_overrides.json");

// ── 公開型 ────────────────────────────────────────────────────────

/// 一覧 1 行 (= 1 翻訳)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListEntry {
    /// JLT の翻訳版 ID (`/ja/laws/view/{jlt_id}`)。
    pub jlt_id: u32,
    /// 一覧上の法令名 (「（抄）」「（第四章）」等の範囲書きを含む)。
    pub title: String,
    /// 法律 / 政令 / 府省令 / 勅令 …
    pub law_type: Option<String>,
    /// 法令番号 (例: `平成十五年法律第五十七号`)。
    pub law_num: Option<String>,
    /// 翻訳日 (ISO)。
    pub translated_on: Option<String>,
    /// 法令用語日英標準対訳辞書のバージョン。
    pub dictionary_version: Option<String>,
    /// この翻訳が載っていた分野 (`bu`)。
    #[serde(default)]
    pub categories: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListPage {
    pub entries: Vec<ListEntry>,
    /// 「98件中1〜50件を表示」の 98。
    pub total: Option<usize>,
    pub has_next: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JltParagraph {
    /// 英訳の項番号 (例: `(2)`)。第 1 項は原文に番号が無ければ None。
    pub num: Option<String>,
    /// 英訳本文。号・号の細分は `(i) …` / `(a) …` の行として改行で続ける。
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JltArticle {
    /// e-Gov の `art_{Num}` と同じ形 (例: `art_3_2`, 削除範囲は `art_38:84`)。
    /// 日本語の条名から導けないときは None。
    pub article_id: Option<String>,
    /// 日本語の条名 (例: `第三条の二`)。
    pub article_no: Option<String>,
    /// 英訳の条名 (例: `Article 3-2`)。
    pub title: Option<String>,
    /// 英訳の見出し (例: `(Fundamental Principles)`)。
    pub caption: Option<String>,
    pub paragraphs: Vec<JltParagraph>,
    /// JLT ページ内アンカー (例: `je_pt1ch1at1`)。
    pub anchor: Option<String>,
}

/// 1 翻訳の本文。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JltLaw {
    pub schema_version: u32,
    pub jlt_id: u32,
    pub title_ja: String,
    pub title_en: Option<String>,
    /// 本文冒頭の法令番号 (公布日入り, 例: `明治二十九年四月二十七日法律第八十九号`)。
    pub law_num_ja: Option<String>,
    pub law_num_en: Option<String>,
    /// 翻訳が反映している最終改正 (例: `令和三年法律第三十七号`)。
    pub last_version_ja: Option<String>,
    pub last_version_en: Option<String>,
    /// 暫定版 (ネイティブ・専門家チェック前)。
    pub tentative: bool,
    pub articles: Vec<JltArticle>,
    pub source_url: String,
    pub fetched_at: String,
    /// 取得時の一覧行 (法令番号・翻訳日・分野)。
    pub entry: ListEntry,
}

// ── Provider ──────────────────────────────────────────────────────

pub trait JltProvider: Send + Sync {
    /// 分野 `category` の翻訳一覧 (全ページ)。暫定版を含む。
    fn list(&self, category: u32) -> Result<Vec<ListEntry>>;
    /// 1 翻訳の日英併記ページを取得して解析する。
    fn fetch(&self, entry: &ListEntry) -> Result<JltLaw>;
}

/// オフライン用。実ページ (民法の抜粋) を組み込みで返す。
pub struct MockProvider;

const MOCK_VIEW: &str = include_str!("../tests/fixtures/view_4848_civil_code_trimmed.html");

impl JltProvider for MockProvider {
    fn list(&self, category: u32) -> Result<Vec<ListEntry>> {
        if category != 8 {
            return Ok(vec![]);
        }
        Ok(vec![ListEntry {
            jlt_id: 4848,
            title: "民法 (第一編第二編第三編)".to_string(),
            law_type: Some("法律".to_string()),
            law_num: Some("明治二十九年法律第八十九号".to_string()),
            translated_on: Some("2023-03-31".to_string()),
            dictionary_version: Some("15.0".to_string()),
            categories: vec![8],
        }])
    }

    fn fetch(&self, entry: &ListEntry) -> Result<JltLaw> {
        parse_view(MOCK_VIEW, entry, &view_url(BASE_URL, entry.jlt_id))
    }
}

pub fn view_url(base: &str, jlt_id: u32) -> String {
    format!("{base}/ja/laws/view/{jlt_id}/je")
}

pub struct HttpProvider {
    client: reqwest::blocking::Client,
    base_url: String,
    interval: Duration,
    /// reqwest の cookie store は feature 追加が要るので、CSRF 用の cookie だけ手で持つ。
    cookies: Mutex<BTreeMap<String, String>>,
}

impl HttpProvider {
    pub fn new(interval: Duration) -> Result<Self> {
        let base_url = std::env::var("LAWPUB_JLT_BASE_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| BASE_URL.to_string());
        let client = reqwest::blocking::Client::builder()
            .user_agent("lawrenceanum/0.1 (+https://github.com/bokuweb/lawrenceanum)")
            .timeout(Duration::from_secs(120))
            .build()?;
        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            interval,
            cookies: Mutex::new(BTreeMap::new()),
        })
    }

    fn cookie_header(&self) -> String {
        let jar = self.cookies.lock().unwrap();
        jar.iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("; ")
    }

    fn keep_cookies(&self, resp: &reqwest::blocking::Response) {
        let mut jar = self.cookies.lock().unwrap();
        for v in resp.headers().get_all(reqwest::header::SET_COOKIE) {
            let Ok(s) = v.to_str() else { continue };
            let pair = s.split(';').next().unwrap_or("");
            if let Some((k, v)) = pair.split_once('=') {
                jar.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
    }

    /// 送信して (最終 URL, 本文) を返す。
    fn send(&self, req: reqwest::blocking::RequestBuilder, what: &str) -> Result<(String, String)> {
        std::thread::sleep(self.interval);
        let cookie = self.cookie_header();
        let req = if cookie.is_empty() {
            req
        } else {
            req.header(reqwest::header::COOKIE, cookie)
        };
        let resp = req.send().with_context(|| what.to_string())?;
        self.keep_cookies(&resp);
        let resp = resp.error_for_status().with_context(|| what.to_string())?;
        let url = resp.url().to_string();
        Ok((url, resp.text()?))
    }

    fn list_page(
        &self,
        token: &str,
        category: u32,
        page: usize,
    ) -> Result<(ListPage, Option<String>)> {
        let url = format!("{}/ja/laws/result/?page={page}", self.base_url);
        let bu = category.to_string();
        let form = [("_csrfToken", token), ("bu", bu.as_str()), ("ia", "03")];
        let (final_url, html) = self.send(
            self.client.post(&url).form(&form),
            &format!("POST {url} bu={category}"),
        )?;
        if final_url.contains("/laws/error/") {
            bail!("JLT search error for bu={category} page={page}: {final_url}");
        }
        Ok((parse_list(&html), parse_csrf_token(&html)))
    }
}

impl JltProvider for HttpProvider {
    fn list(&self, category: u32) -> Result<Vec<ListEntry>> {
        let url = format!("{}/ja/laws/search-cat", self.base_url);
        let (_, html) = self.send(self.client.get(&url), &format!("GET {url}"))?;
        let mut token = parse_csrf_token(&html).context("no _csrfToken on search-cat")?;
        let mut out: Vec<ListEntry> = Vec::new();
        // 1 分野は高々数百件 (50 件/ページ)。無限ループ避けに上限を置く。
        for page in 1..=40 {
            let (lp, next_token) = self.list_page(&token, category, page)?;
            let n = lp.entries.len();
            out.extend(lp.entries);
            if let Some(t) = next_token {
                token = t;
            }
            if !lp.has_next || n == 0 {
                break;
            }
        }
        for e in &mut out {
            e.categories = vec![category];
        }
        Ok(out)
    }

    fn fetch(&self, entry: &ListEntry) -> Result<JltLaw> {
        let url = view_url(&self.base_url, entry.jlt_id);
        let (_, html) = self.send(self.client.get(&url), &format!("GET {url}"))?;
        // 出典 URL は常に本番ドメインで記録する (base URL 上書き時もリンク切れにしない)。
        parse_view(&html, entry, &view_url(BASE_URL, entry.jlt_id))
    }
}

// ── 一覧パーサ ────────────────────────────────────────────────────

fn sel(s: &str) -> Selector {
    Selector::parse(s).expect("valid selector")
}

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn text_of(el: ElementRef) -> String {
    squash(&el.text().collect::<String>())
}

pub fn parse_csrf_token(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let s = sel(r#"input[name="_csrfToken"]"#);
    doc.select(&s)
        .find_map(|e| e.value().attr("value").map(str::to_string))
        .filter(|s| !s.is_empty())
}

pub fn parse_list(html: &str) -> ListPage {
    let doc = Html::parse_document(html);
    let li_s = sel("ul.search-result > li");
    let type_s = sel(".result-law-type");
    let a_s = sel(".result-title-text a");
    let info_s = sel(".result-info-item");
    let mut entries = Vec::new();
    for li in doc.select(&li_s) {
        let Some(a) = li.select(&a_s).next() else {
            continue;
        };
        let Some(jlt_id) = a
            .value()
            .attr("href")
            .and_then(|h| h.rsplit("/view/").next())
            .and_then(|tail| tail.split(['/', '?', '#']).next())
            .and_then(|id| id.parse::<u32>().ok())
        else {
            continue;
        };
        let title = text_of(a);
        if title.is_empty() {
            continue;
        }
        let mut e = ListEntry {
            jlt_id,
            title,
            law_type: li
                .select(&type_s)
                .next()
                .map(text_of)
                .filter(|s| !s.is_empty()),
            law_num: None,
            translated_on: None,
            dictionary_version: None,
            categories: vec![],
        };
        for info in li.select(&info_s) {
            let t = text_of(info);
            let Some((k, v)) = t.split_once('：') else {
                continue;
            };
            let v = v.trim();
            if v.is_empty() {
                continue;
            }
            match k.trim() {
                "法令番号" => e.law_num = Some(v.to_string()),
                "翻訳日" => e.translated_on = wareki_date_to_iso(v),
                "辞書バージョン" => e.dictionary_version = Some(v.to_string()),
                _ => {}
            }
        }
        entries.push(e);
    }
    let total = html.split("検索結果：").nth(1).and_then(|rest| {
        let rest = rest.trim_start_matches(|c: char| c != '>' && !c.is_ascii_digit());
        let rest = rest.trim_start_matches('>').trim_start();
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        digits.parse().ok()
    });
    let has_next = doc.select(&sel(".pagination a")).any(|a| {
        a.value()
            .attr("onclick")
            .is_some_and(|o| o.contains("'next'"))
    });
    ListPage {
        entries,
        total,
        has_next,
    }
}

/// `令和3年11月5日` / `平成元年4月1日` → `2021-11-05`。
pub fn wareki_date_to_iso(s: &str) -> Option<String> {
    let s = s.trim();
    let (base, rest) = [
        ("明治", 1867),
        ("大正", 1911),
        ("昭和", 1925),
        ("平成", 1988),
        ("令和", 2018),
    ]
    .iter()
    .find_map(|(era, base)| s.strip_prefix(era).map(|r| (*base, r)))?;
    let (y, rest) = rest.split_once('年')?;
    let y: i32 = if y == "元" {
        1
    } else {
        y.trim().parse().ok()?
    };
    let (m, rest) = rest.split_once('月')?;
    let (d, _) = rest.split_once('日')?;
    let (m, d): (u32, u32) = (m.trim().parse().ok()?, d.trim().parse().ok()?);
    chrono::NaiveDate::from_ymd_opt(base + y, m, d).map(|d| d.format("%Y-%m-%d").to_string())
}

// ── 本文パーサ ────────────────────────────────────────────────────

/// 日本語か (かな・漢字が英字に比べて十分多い)。日英併記の各行の言語判定に使う。
pub fn is_japanese(s: &str) -> bool {
    let mut cjk = 0usize;
    let mut alpha = 0usize;
    for c in s.chars() {
        match c {
            '\u{3040}'..='\u{30ff}' | '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}' => cjk += 1,
            c if c.is_ascii_alphabetic() => alpha += 1,
            _ => {}
        }
    }
    cjk > 0 && cjk * 4 >= alpha
}

fn has_class(el: ElementRef, class: &str) -> bool {
    el.value().classes().any(|c| c == class)
}

fn is_title_span(el: ElementRef) -> bool {
    el.value().classes().any(|c| {
        c == "ArticleTitle"
            || c == "ParagraphNum"
            || c == "ItemTitle"
            || (c.starts_with("Subitem") && c.ends_with("Title"))
    })
}

/// 日英併記の 1 行 (`*Sentence`) を (条名, 番号, 本文) に分ける。
/// 用語の吹き出し (`span.balloon`) は本文に含めない。
fn split_sentence(el: ElementRef) -> (Option<String>, Option<String>, String) {
    let mut article_title = None;
    let mut num = None;
    let mut body = String::new();
    fn walk(el: ElementRef, body: &mut String) {
        for child in el.children() {
            match child.value() {
                Node::Text(t) => body.push_str(t),
                Node::Element(_) => {
                    if let Some(ce) = ElementRef::wrap(child) {
                        if !has_class(ce, "balloon") {
                            walk(ce, body);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    for child in el.children() {
        match child.value() {
            Node::Text(t) => body.push_str(t),
            Node::Element(_) => {
                let Some(ce) = ElementRef::wrap(child) else {
                    continue;
                };
                if is_title_span(ce) {
                    let t = text_of(ce);
                    if has_class(ce, "ArticleTitle") {
                        article_title = Some(t);
                    } else {
                        num = Some(t);
                    }
                } else if !has_class(ce, "balloon") {
                    walk(ce, &mut body);
                }
            }
            _ => {}
        }
    }
    (
        article_title.filter(|s| !s.is_empty()),
        num.filter(|s| !s.is_empty()),
        squash(&body),
    )
}

fn is_sentence(el: ElementRef) -> bool {
    el.value().classes().any(|c| c.ends_with("Sentence"))
}

fn parse_article(art: ElementRef) -> Option<JltArticle> {
    let mut a = JltArticle {
        article_id: None,
        article_no: None,
        title: None,
        caption: None,
        paragraphs: vec![],
        anchor: art.value().attr("id").map(str::to_string),
    };
    for child in art.children().filter_map(ElementRef::wrap) {
        if has_class(child, "ArticleCaption") {
            let t = text_of(child);
            if !t.is_empty() && !is_japanese(&t) {
                a.caption = Some(t);
            }
        } else if has_class(child, "Paragraph") {
            let mut para: Option<JltParagraph> = None;
            for s in child
                .descendants()
                .filter_map(ElementRef::wrap)
                .filter(|e| is_sentence(*e))
            {
                let (title, num, body) = split_sentence(s);
                let line_text = format!("{} {}", title.as_deref().unwrap_or(""), body);
                if is_japanese(&line_text) || (title.is_none() && is_japanese(&body)) {
                    if let Some(t) = title {
                        a.article_no.get_or_insert(t);
                    }
                    continue;
                }
                if body.is_empty() && num.is_none() {
                    continue;
                }
                if has_class(s, "ParagraphSentence") {
                    if let Some(t) = title {
                        a.title.get_or_insert(t);
                    }
                    match para.as_mut() {
                        // 同じ項の 2 文目 (ただし書き等)。
                        Some(p) if num.is_none() => {
                            if !p.text.is_empty() {
                                p.text.push(' ');
                            }
                            p.text.push_str(&body);
                        }
                        _ => {
                            if let Some(p) = para.take() {
                                a.paragraphs.push(p);
                            }
                            para = Some(JltParagraph { num, text: body });
                        }
                    }
                } else {
                    // 号・号の細分: 「(i) …」の行として項に続ける。
                    let line = match num {
                        Some(n) if body.is_empty() => n,
                        Some(n) => format!("{n} {body}"),
                        None => body,
                    };
                    let p = para.get_or_insert(JltParagraph {
                        num: None,
                        text: String::new(),
                    });
                    if !p.text.is_empty() {
                        p.text.push('\n');
                    }
                    p.text.push_str(&line);
                }
            }
            if let Some(p) = para.take() {
                a.paragraphs.push(p);
            }
        }
    }
    a.article_id = a
        .article_no
        .as_deref()
        .and_then(article_num_from_title)
        .map(|n| format!("art_{n}"));
    if a.paragraphs.is_empty() && a.title.is_none() {
        return None;
    }
    Some(a)
}

/// 日英併記ページを解析する。本則 (`#mp`) の条だけを返す (JLT は附則を載せない)。
pub fn parse_view(html: &str, entry: &ListEntry, source_url: &str) -> Result<JltLaw> {
    let doc = Html::parse_document(html);
    let titles: Vec<String> = doc
        .select(&sel(".LawTitle .LawTitle_text"))
        .map(text_of)
        .collect();
    let info_titles: Vec<String> = doc.select(&sel("#lawInfo .title")).map(text_of).collect();
    let title_ja = titles
        .first()
        .cloned()
        .or_else(|| info_titles.first().cloned())
        .filter(|s| !s.is_empty())
        .context("no law title in JLT page")?;
    let title_en = titles
        .get(1)
        .cloned()
        .or_else(|| info_titles.get(1).cloned());
    let tentative = doc
        .select(&sel(".LawTitle"))
        .next()
        .map(|t| {
            let s = t.text().collect::<String>();
            s.contains("暫定版") || s.contains("Tentative translation")
        })
        .unwrap_or(false);
    let (law_num_ja, law_num_en) = match doc.select(&sel(".LawNum")).next() {
        Some(n) => {
            let en = n
                .select(&sel("span"))
                .next()
                .map(text_of)
                .filter(|s| !s.is_empty());
            let all = text_of(n);
            let ja = match &en {
                Some(en) => squash(all.replace(en.as_str(), "").as_str()),
                None => all,
            };
            (Some(ja).filter(|s| !s.is_empty()), en)
        }
        None => (None, None),
    };
    let versions: Vec<String> = doc
        .select(&sel("#lawInfo .last-version"))
        .map(|v| {
            let all = text_of(v);
            let label = v
                .select(&sel(".label"))
                .next()
                .map(text_of)
                .unwrap_or_default();
            squash(all.trim_start_matches(label.as_str()))
        })
        .collect();
    let art_s = sel("div.Article");
    let root = doc.select(&sel("#mp")).next();
    let articles: Vec<JltArticle> = match root {
        Some(mp) => mp.select(&art_s).filter_map(parse_article).collect(),
        None => doc.select(&art_s).filter_map(parse_article).collect(),
    };
    Ok(JltLaw {
        schema_version: SCHEMA_VERSION,
        jlt_id: entry.jlt_id,
        title_ja,
        title_en,
        law_num_ja,
        law_num_en,
        last_version_ja: versions.first().cloned().filter(|s| !s.is_empty()),
        last_version_en: versions.get(1).cloned().filter(|s| !s.is_empty()),
        tentative,
        articles,
        source_url: source_url.to_string(),
        fetched_at: chrono::Utc::now().to_rfc3339(),
        entry: entry.clone(),
    })
}

// ── 条番号 ────────────────────────────────────────────────────────

/// 漢数字 (`三百二十五`, `千二` 等) → 数。
pub fn kanji_to_u32(s: &str) -> Option<u32> {
    if s.is_empty() {
        return None;
    }
    if let Ok(n) = s.parse::<u32>() {
        return Some(n);
    }
    let digit = |c: char| {
        "〇一二三四五六七八九"
            .chars()
            .position(|d| d == c)
            .map(|p| p as u32)
    };
    let (mut total, mut section, mut cur) = (0u32, 0u32, None::<u32>);
    for c in s.chars() {
        if let Some(d) = digit(c) {
            // 位取り表記 (例: 二〇) にも対応する。
            cur = Some(cur.map_or(d, |v| v * 10 + d));
            continue;
        }
        let unit = match c {
            '十' => 10,
            '百' => 100,
            '千' => 1000,
            '万' => {
                section += cur.take().unwrap_or(0);
                total += section.max(1) * 10_000;
                section = 0;
                continue;
            }
            _ => return None,
        };
        section += cur.take().unwrap_or(1) * unit;
    }
    Some(total + section + cur.unwrap_or(0))
}

/// `第三条の二` → `3_2`、`第三十八条から第八十四条まで` / `第五百三十四条及び第五百三十五条` → `38:84`。
/// e-Gov 法令 XML の `Article/@Num` と同じ形にする。
pub fn article_num_from_title(title: &str) -> Option<String> {
    let t: String = title.chars().filter(|c| !c.is_whitespace()).collect();
    for sep in ["から", "及び"] {
        if let Some((a, b)) = t.split_once(sep) {
            let a = single_article_num(a)?;
            let b = single_article_num(b.trim_end_matches("まで"))?;
            return Some(format!("{a}:{b}"));
        }
    }
    single_article_num(&t)
}

fn single_article_num(t: &str) -> Option<String> {
    let rest = t.strip_prefix('第')?;
    let (main, branches) = rest.split_once('条')?;
    let mut out = kanji_to_u32(main)?.to_string();
    if branches.is_empty() {
        return Some(out);
    }
    for b in branches.strip_prefix('の')?.split('の') {
        out.push('_');
        out.push_str(&kanji_to_u32(b)?.to_string());
    }
    Some(out)
}

/// 法令番号の表記ゆれを吸収する (空白除去、共同省令の区切り `、`/`，` → `・`)。
pub fn normalize_law_num(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            '、' | '，' | ',' => '・',
            c => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!(
            "{}/tests/fixtures/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    fn entry(id: u32) -> ListEntry {
        ListEntry {
            jlt_id: id,
            title: String::new(),
            law_type: None,
            law_num: None,
            translated_on: None,
            dictionary_version: None,
            categories: vec![],
        }
    }

    #[test]
    fn kanji_numbers() {
        assert_eq!(kanji_to_u32("一"), Some(1));
        assert_eq!(kanji_to_u32("十"), Some(10));
        assert_eq!(kanji_to_u32("二十三"), Some(23));
        assert_eq!(kanji_to_u32("百"), Some(100));
        assert_eq!(kanji_to_u32("四百六十五"), Some(465));
        assert_eq!(kanji_to_u32("千四十四"), Some(1044));
        assert_eq!(kanji_to_u32("二千"), Some(2000));
        assert_eq!(kanji_to_u32("abc"), None);
    }

    #[test]
    fn article_nums() {
        assert_eq!(article_num_from_title("第一条").as_deref(), Some("1"));
        assert_eq!(article_num_from_title("第三条の二").as_deref(), Some("3_2"));
        assert_eq!(
            article_num_from_title("第四百六十五条の六").as_deref(),
            Some("465_6")
        );
        assert_eq!(
            article_num_from_title("第二条の二の三").as_deref(),
            Some("2_2_3")
        );
        assert_eq!(
            article_num_from_title("第三十八条から第八十四条まで").as_deref(),
            Some("38:84")
        );
        assert_eq!(
            article_num_from_title("第五百三十四条及び第五百三十五条").as_deref(),
            Some("534:535")
        );
        assert_eq!(article_num_from_title("附則"), None);
    }

    #[test]
    fn wareki() {
        assert_eq!(
            wareki_date_to_iso("令和3年11月5日").as_deref(),
            Some("2021-11-05")
        );
        assert_eq!(
            wareki_date_to_iso("平成26年6月10日").as_deref(),
            Some("2014-06-10")
        );
        assert_eq!(
            wareki_date_to_iso("令和元年5月1日").as_deref(),
            Some("2019-05-01")
        );
        assert_eq!(wareki_date_to_iso("2021-01-01"), None);
    }

    #[test]
    fn law_num_normalization() {
        assert_eq!(
            normalize_law_num("令和二年内閣府、総務省告示第七号"),
            normalize_law_num("令和二年内閣府・総務省告示第七号")
        );
        assert_eq!(
            normalize_law_num(" 平成十五年法律第五十七号 "),
            "平成十五年法律第五十七号"
        );
    }

    #[test]
    fn japanese_detection() {
        assert!(is_japanese(
            "第一条 私権は、公共の福祉に適合しなければならない。"
        ));
        assert!(!is_japanese(
            "Article 1 (1) Private rights must be congruent with the public welfare."
        ));
        assert!(!is_japanese("(i) receiving or using principal;"));
        assert!(is_japanese("一 元本を領収し、又は利用すること。"));
    }

    #[test]
    fn parses_list_page_with_pagination() {
        let lp = parse_list(&fixture("list_bu2_page1.html"));
        assert_eq!(lp.total, Some(98));
        assert!(lp.has_next);
        assert_eq!(lp.entries.len(), 50);
        let e = &lp.entries[0];
        assert_eq!(e.jlt_id, 4241);
        assert!(
            e.title.starts_with("個人情報の保護に関する法律"),
            "{}",
            e.title
        );
        assert_eq!(e.law_type.as_deref(), Some("法律"));
        assert_eq!(e.law_num.as_deref(), Some("平成十五年法律第五十七号"));
        assert_eq!(e.translated_on.as_deref(), Some("2021-11-05"));
        assert_eq!(e.dictionary_version.as_deref(), Some("14.0"));
    }

    #[test]
    fn parses_last_list_page() {
        let lp = parse_list(&fixture("list_bu64_tentative.html"));
        assert_eq!(lp.total, Some(24));
        assert!(!lp.has_next);
        assert_eq!(lp.entries.len(), 24);
        let joint = lp.entries.iter().find(|e| e.jlt_id == 5201).unwrap();
        assert!(joint.law_num.as_deref().unwrap().contains("内閣府、総務省"));
        assert_eq!(joint.translated_on.as_deref(), Some("2025-11-28"));
    }

    #[test]
    fn parses_civil_code_view() {
        let law = parse_view(
            &fixture("view_4848_civil_code_trimmed.html"),
            &entry(4848),
            "u",
        )
        .unwrap();
        assert_eq!(law.title_ja, "民法");
        assert_eq!(law.title_en.as_deref(), Some("Civil Code"));
        assert_eq!(
            law.law_num_ja.as_deref(),
            Some("明治二十九年四月二十七日法律第八十九号")
        );
        assert_eq!(
            law.law_num_en.as_deref(),
            Some("Act No. 89 of April 27, 1896")
        );
        assert_eq!(
            law.last_version_ja.as_deref(),
            Some("令和三年法律第三十七号")
        );
        assert_eq!(law.last_version_en.as_deref(), Some("Act No. 37 of 2021"));
        assert!(!law.tentative);

        let a1 = &law.articles[0];
        assert_eq!(a1.article_id.as_deref(), Some("art_1"));
        assert_eq!(a1.article_no.as_deref(), Some("第一条"));
        assert_eq!(a1.title.as_deref(), Some("Article 1"));
        assert_eq!(a1.caption.as_deref(), Some("(Fundamental Principles)"));
        assert_eq!(a1.anchor.as_deref(), Some("je_pt1ch1at1"));
        assert_eq!(a1.paragraphs.len(), 3);
        assert_eq!(a1.paragraphs[0].num.as_deref(), Some("(1)"));
        assert_eq!(
            a1.paragraphs[0].text,
            "Private rights must be congruent with the public welfare."
        );
        assert_eq!(a1.paragraphs[2].text, "Abuse of rights is not permitted.");

        let a3_2 = law
            .articles
            .iter()
            .find(|a| a.article_id.as_deref() == Some("art_3_2"))
            .unwrap();
        assert_eq!(a3_2.title.as_deref(), Some("Article 3-2"));
        assert!(a3_2.paragraphs[0]
            .text
            .starts_with("If the person performing a juridical act"));

        // 号は「(i) …」の行として項に続く。
        let a13 = law
            .articles
            .iter()
            .find(|a| a.article_id.as_deref() == Some("art_13"))
            .unwrap();
        let lines: Vec<&str> = a13.paragraphs[0].text.lines().collect();
        assert!(lines[0].starts_with("A person under curatorship must obtain"));
        assert_eq!(lines[1], "(i) receiving or using principal;");

        // 削除範囲。
        let del = law
            .articles
            .iter()
            .find(|a| a.article_id.as_deref() == Some("art_38:84"))
            .unwrap();
        assert_eq!(del.title.as_deref(), Some("Articles 38 through 84"));
        assert_eq!(del.paragraphs[0].text, "Deleted");

        // 号の細分 (イ・ロ → (a)/(b))。
        let a465_6 = law
            .articles
            .iter()
            .find(|a| a.article_id.as_deref() == Some("art_465_6"))
            .unwrap();
        let p2 = &a465_6.paragraphs[1];
        assert_eq!(p2.num.as_deref(), Some("(2)"));
        assert!(
            p2.text
                .lines()
                .any(|l| l.starts_with("(a) a guarantee contract")),
            "{}",
            p2.text
        );

        // 日本語本文は混ざらない。
        for a in &law.articles {
            for p in &a.paragraphs {
                assert!(!is_japanese(&p.text), "{:?}: {}", a.article_id, p.text);
            }
        }
    }

    #[test]
    fn parses_tentative_joint_notice() {
        let law = parse_view(
            &fixture("view_5201_tentative_notice.html"),
            &entry(5201),
            "u",
        )
        .unwrap();
        assert!(law.tentative);
        assert!(law
            .title_en
            .as_deref()
            .unwrap()
            .starts_with("Public Notice Specifying Conditions"));
        assert!(
            !law.title_en.as_deref().unwrap().contains("proofread"),
            "balloon text leaked"
        );
        assert!(law.last_version_ja.is_none());
        let a1 = &law.articles[0];
        assert_eq!(a1.article_id.as_deref(), Some("art_1"));
        assert!(a1.paragraphs[0]
            .text
            .contains("\n(i) specified acquisition:"));
    }

    #[test]
    fn parses_ministerial_order() {
        let law = parse_view(&fixture("view_3616.html"), &entry(3616), "u").unwrap();
        assert_eq!(law.title_ja, "外国人技能実習機構の財務及び会計に関する省令");
        assert_eq!(
            law.law_num_ja.as_deref(),
            Some("平成二十八年十一月二十八日法務省・厚生労働省令第四号")
        );
        assert!(law.articles.len() > 10);
        assert!(law.articles.iter().all(|a| a.article_id.is_some()));
        assert_eq!(
            law.articles[1].caption.as_deref(),
            Some("(Establishment of Accounts)")
        );
    }

    #[test]
    fn mock_provider_round_trip() {
        let p = MockProvider;
        let list = p.list(8).unwrap();
        assert_eq!(list.len(), 1);
        let law = p.fetch(&list[0]).unwrap();
        assert_eq!(law.jlt_id, 4848);
        assert_eq!(
            law.source_url,
            "https://www.japaneselawtranslation.go.jp/ja/laws/view/4848/je"
        );
        assert!(!law.articles.is_empty());
    }

    #[test]
    #[ignore = "real network"]
    fn http_lists_and_fetches() {
        let p = HttpProvider::new(Duration::from_millis(1500)).unwrap();
        let list = p.list(64).unwrap();
        assert!(list.len() > 10, "{}", list.len());
        let small = list.iter().find(|e| e.jlt_id == 3616).unwrap_or(&list[0]);
        let law = p.fetch(small).unwrap();
        assert!(!law.articles.is_empty());
    }
}
