//! 国税庁 法令解釈通達 (soft law) スクレイパー。
//!
//! 法令本文 (e-Gov) に載らない「通達」を収集する。著作権法13条で通達に著作権は無く、
//! 国税庁サイトはデジタル庁「公共データ利用規約(PDL1.0 = CC BY 4.0 互換)」採用で
//! 商用・再配布可（出典明記）。robots.txt は `/law/` を許容。サイトは Shift-JIS。
//!
//! 構造: 目次 `…/kihon/{税目}/{章}.htm` → 本文 `…/kihon/{税目}/{章}/{節}.htm`。
//! 本文は `<h2>（見出し）</h2>` ＋ `<p class="indent1"><strong>{条}</strong>
//! <strong>−{項}　</strong>{本文}</p>` の繰り返し。

use anyhow::{Context, Result};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const BASE_URL: &str = "https://www.nta.go.jp";

// ── 公開型 ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsutatsuItem {
    /// 税目スラッグ (例: "shotoku")。
    pub tax: String,
    /// 通達番号 (例: "2-5")。
    pub number: String,
    /// 見出し（直前の <h2>、括弧除去）。
    pub caption: Option<String>,
    /// 通達本文。
    pub text: String,
    /// 出典 URL (本文ページ)。
    pub source_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsutatsuSet {
    pub schema_version: u32,
    /// 通達集名 (例: "所得税基本通達")。
    pub name: String,
    pub tax: String,
    /// 親法令の e-Gov 法令ID (通達本文中の「法」が指す法令)。クロスリンク用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_law_id: Option<String>,
    /// 親法令の題名 (例: "所得税法")。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_law_title: Option<String>,
    pub items: Vec<TsutatsuItem>,
    pub source: TsutatsuSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsutatsuSource {
    pub provider: String,
    pub fetched_at: String,
    pub index_url: String,
}

// ── Provider ──────────────────────────────────────────────────────

pub trait TsutatsuProvider: Send + Sync {
    /// 目次ページから本文ページの URL 群を返す。
    fn list_pages(&self, index_url: &str) -> Result<Vec<String>>;
    /// 本文ページから通達項目を抽出する。
    fn fetch_page(&self, page_url: &str, tax: &str) -> Result<Vec<TsutatsuItem>>;
}

// ── Mock ──────────────────────────────────────────────────────────

pub struct MockProvider;

impl TsutatsuProvider for MockProvider {
    fn list_pages(&self, _index_url: &str) -> Result<Vec<String>> {
        Ok(vec![format!("{BASE_URL}/law/tsutatsu/kihon/shotoku/01/02.htm")])
    }
    fn fetch_page(&self, page_url: &str, tax: &str) -> Result<Vec<TsutatsuItem>> {
        Ok(vec![TsutatsuItem {
            tax: tax.to_string(),
            number: "2-5".to_string(),
            caption: Some("法人でない社団の範囲".to_string()),
            text: "法第2条第1項第8号に規定する法人でない社団とは…".to_string(),
            source_url: page_url.to_string(),
        }])
    }
}

// ── Http (Shift-JIS) ──────────────────────────────────────────────

pub struct HttpProvider {
    base_url: String,
}

impl HttpProvider {
    pub fn new() -> Self {
        let base_url = std::env::var("LAWPUB_NTA_BASE_URL")
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

    fn get_sjis(client: &reqwest::blocking::Client, url: &str) -> Result<String> {
        std::thread::sleep(Duration::from_secs(1));
        let resp = client
            .get(url)
            .send()
            .and_then(|r| r.error_for_status())
            .with_context(|| format!("GET {url}"))?;
        resp.text_with_charset("Shift_JIS").context("decode shift_jis")
    }

    fn abs(&self, href: &str) -> String {
        if href.starts_with("http") {
            href.to_string()
        } else {
            format!("{}{}", self.base_url, href)
        }
    }
}

impl Default for HttpProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl TsutatsuProvider for HttpProvider {
    fn list_pages(&self, index_url: &str) -> Result<Vec<String>> {
        let client = Self::client()?;
        let html = Self::get_sjis(&client, index_url)?;
        Ok(parse_index(&html)
            .into_iter()
            .map(|h| self.abs(&h))
            .collect())
    }

    fn fetch_page(&self, page_url: &str, tax: &str) -> Result<Vec<TsutatsuItem>> {
        let client = Self::client()?;
        let html = Self::get_sjis(&client, page_url)?;
        Ok(parse_body(&html, page_url, tax))
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

/// 目次から本文ページ (`{税目}/{章}/{節}.htm` = 2 階層) のリンクを集める。
pub fn parse_index(html: &str) -> Vec<String> {
    let doc = Html::parse_document(html);
    let a = sel("a");
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for el in doc.select(&a) {
        let href = el.value().attr("href").unwrap_or("");
        // フラグメント(#a-...)とクエリを除いたページ URL で判定・重複排除する。
        // 目次は同一本文ページへ条文アンカー付きリンクを多数張るため、これを
        // 剥がさないと .htm 末尾判定に失敗し本文ページをほとんど拾えない。
        let page = href.split('#').next().unwrap_or(href);
        // 本文ページは kihon/{税目}/NN/NN.htm の形 (章/節の 2 階層)。
        if page.contains("/kihon/")
            && regex_like_two_level(page)
            && seen.insert(page.to_string())
        {
            out.push(page.to_string());
        }
    }
    out
}

/// 本文ページ判定: `…/kihon/{tax}/…/{章ディレクトリ}/{ファイル}.htm` で、
/// 直前の章ディレクトリが数字なら本文ページ。ファイル名の形は税目で異なる
/// (所得税 `02.htm`、法人税 `02_03_07a.htm`、相続税 `00.htm`) ため、ファイル名は
/// 問わず「親ディレクトリが数字」を本文ページのシグナルとする。章目次 (`{tax}/01.htm`)
/// や `{tax}/zenbun/01.htm` 等は親が数字でないため除外される。
fn regex_like_two_level(href: &str) -> bool {
    let path = href.split('?').next().unwrap_or(href);
    let path = path.split('#').next().unwrap_or(path);
    if !path.ends_with(".htm") {
        return false;
    }
    let comps: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if comps.len() < 2 {
        return false;
    }
    let file = comps[comps.len() - 1];
    let parent = comps[comps.len() - 2];
    !file.is_empty()
        && !parent.is_empty()
        && parent.chars().all(|c| c.is_ascii_digit())
}

/// 「番号らしい」<strong>断片か (数字・全角数字・ハイフン・空白・読点のみで、
/// 数字を 1 つ以上含む)。先頭の番号 strong と本文中の強調 strong (例 "第3条") を
/// 区別するために使う。
fn is_number_like(s: &str) -> bool {
    let mut has_digit = false;
    for c in s.chars() {
        match c {
            '0'..='9' | '０'..='９' => has_digit = true,
            '−' | '－' | '-' | 'ー' => {}
            c if c.is_whitespace() => {}
            '\u{3000}' => {} // 全角スペース
            _ => return false,
        }
    }
    has_digit
}

/// 通達番号らしき文字列を正規化 ("2", "−5　" → "2-5")。
fn norm_number(s: &str) -> String {
    s.chars()
        .filter_map(|c| match c {
            '0'..='9' => Some(c),
            '０'..='９' => char::from_u32(c as u32 - '０' as u32 + '0' as u32),
            '−' | '－' | '-' | 'ー' => Some('-'),
            _ => None,
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

/// 本文ページから通達項目を抽出する。h2(見出し)→p.indent1(番号+本文) を文書順に走査。
pub fn parse_body(html: &str, page_url: &str, tax: &str) -> Vec<TsutatsuItem> {
    let doc = Html::parse_document(html);
    let nodes = sel("h2, p.indent1");
    let strong = sel("strong");
    let mut items = Vec::new();
    let mut caption: Option<String> = None;
    for el in doc.select(&nodes) {
        let tag = el.value().name();
        if tag == "h2" {
            let c = text_of(&el);
            let c = c.trim_matches(|ch| ch == '（' || ch == '）' || ch == '〔' || ch == '〕' || ch == '(' || ch == ')').trim().to_string();
            caption = if c.is_empty() { None } else { Some(c) };
            continue;
        }
        // p.indent1 = 通達項目。先頭から「番号らしい」<strong> が続く範囲を番号とみなす
        // (例: "2","−5　" → "2-5"; "1　" → "1")。本文中の強調 (例 "第3条") で止める。
        let mut raw = String::new();
        for s in el.select(&strong) {
            let st = text_of(&s);
            if is_number_like(&st) {
                raw.push_str(&st);
            } else {
                break;
            }
        }
        let number = norm_number(&raw);
        if number.is_empty() {
            continue; // 番号が取れないものは項目でない。
        }
        let full = text_of(&el);
        items.push(TsutatsuItem {
            tax: tax.to_string(),
            number,
            caption: caption.clone(),
            text: full,
            source_url: page_url.to_string(),
        });
    }
    items
}

// ── 既知の通達集 ──────────────────────────────────────────────────

/// 収集対象の通達集 1 件分のメタ。`parent_law_*` は通達本文中の「法」が指す
/// 親法令で、法令↔通達クロスリンクに使う。
#[derive(Debug, Clone)]
pub struct KnownSet {
    pub tax: &'static str,
    pub name: &'static str,
    pub index_url: String,
    pub parent_law_id: &'static str,
    pub parent_law_title: &'static str,
}

/// 既知の基本通達集 (国税庁)。所得税・法人税・消費税の各基本通達。
pub fn known_sets() -> Vec<KnownSet> {
    vec![
        KnownSet {
            tax: "shotoku",
            name: "所得税基本通達",
            index_url: format!("{BASE_URL}/law/tsutatsu/kihon/shotoku/01.htm"),
            parent_law_id: "340AC0000000033",
            parent_law_title: "所得税法",
        },
        KnownSet {
            tax: "hojin",
            name: "法人税基本通達",
            index_url: format!("{BASE_URL}/law/tsutatsu/kihon/hojin/01.htm"),
            parent_law_id: "340AC0000000034",
            parent_law_title: "法人税法",
        },
        KnownSet {
            tax: "shohi",
            name: "消費税法基本通達",
            index_url: format!("{BASE_URL}/law/tsutatsu/kihon/shohi/01.htm"),
            parent_law_id: "363AC0000000108",
            parent_law_title: "消費税法",
        },
        KnownSet {
            tax: "sozoku",
            name: "相続税法基本通達",
            index_url: format!("{BASE_URL}/law/tsutatsu/kihon/sisan/sozoku2/01.htm"),
            parent_law_id: "325AC0000000073",
            parent_law_title: "相続税法",
        },
        KnownSet {
            // 財産評価基本通達も「法」は相続税法を指すため親法令は相続税法。
            tax: "hyoka",
            name: "財産評価基本通達",
            index_url: format!("{BASE_URL}/law/tsutatsu/kihon/sisan/hyoka_new/01.htm"),
            parent_law_id: "325AC0000000073",
            parent_law_title: "相続税法",
        },
    ]
}

// ── テスト ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_works() {
        let p = MockProvider;
        let pages = p.list_pages("x").unwrap();
        assert_eq!(pages.len(), 1);
        let items = p.fetch_page(&pages[0], "shotoku").unwrap();
        assert_eq!(items[0].number, "2-5");
    }

    #[test]
    fn two_level_filter() {
        assert!(regex_like_two_level("/law/tsutatsu/kihon/shotoku/01/02.htm"));
        assert!(regex_like_two_level("/law/tsutatsu/kihon/hojin/02/02_03_07a.htm")); // 法人税
        assert!(regex_like_two_level("/law/tsutatsu/kihon/sisan/sozoku2/01/00.htm")); // 相続税
        assert!(!regex_like_two_level("/law/tsutatsu/kihon/shotoku/01.htm")); // 目次(1階層)
        assert!(!regex_like_two_level("/law/tsutatsu/kihon/hojin/zenbun/01.htm")); // 全文(章でない)
        assert!(!regex_like_two_level("/law/tsutatsu/menu.htm"));
    }

    #[test]
    fn parse_index_strips_anchor_and_dedups() {
        // 目次は同一本文ページに条文アンカー付きで多数リンクする。フラグメントを
        // 剥がして本文ページ単位で重複排除する。
        let html = r#"<html><body>
          <p><a href="/law/tsutatsu/kihon/sisan/sozoku2/01/00.htm">a</a></p>
          <p><a href="/law/tsutatsu/kihon/sisan/sozoku2/01/01.htm#a-1_1">b</a></p>
          <p><a href="/law/tsutatsu/kihon/sisan/sozoku2/01/01.htm#a-1_2">c</a></p>
          <p><a href="/law/tsutatsu/kihon/sisan/sozoku2/02.htm">章目次(除外)</a></p>
        </body></html>"#;
        let pages = parse_index(html);
        assert_eq!(pages.len(), 2);
        assert!(pages.contains(&"/law/tsutatsu/kihon/sisan/sozoku2/01/00.htm".to_string()));
        assert!(pages.contains(&"/law/tsutatsu/kihon/sisan/sozoku2/01/01.htm".to_string()));
    }

    #[test]
    fn number_normalization() {
        assert_eq!(norm_number("2−5　"), "2-5");
        assert_eq!(norm_number("２－１０"), "2-10");
        assert_eq!(norm_number("見出しのみ"), "");
    }

    #[test]
    fn number_like_detection() {
        assert!(is_number_like("1　"));
        assert!(is_number_like("4−2"));
        assert!(is_number_like("２－１０"));
        assert!(!is_number_like("第3条")); // 本文中の強調は番号でない
        assert!(!is_number_like("　")); // 数字なし
        assert!(!is_number_like("注"));
    }

    #[test]
    fn parse_body_accepts_bare_number_items() {
        // 財産評価基本通達のような単独番号 ("1") の項目も拾う。
        let html = r#"<html><body>
          <h2>(評価の原則)</h2>
          <p class="indent1"><strong>1　</strong>財産の価額は、時価によるものとし…</p>
          <h2>(時価の意義)</h2>
          <p class="indent1"><strong>4</strong><strong>−2　</strong>「時価」とは…</p>
          <p class="indent1">法第3条の規定により…（番号 strong なし＝項目でない）</p>
        </body></html>"#;
        let items = parse_body(html, "https://x/01/01.htm", "hyoka");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].number, "1");
        assert_eq!(items[0].caption.as_deref(), Some("評価の原則"));
        assert_eq!(items[1].number, "4-2");
    }

    #[test]
    fn parse_body_extracts_items() {
        let html = r#"<html><body>
          <h2>（法人でない社団の範囲）</h2>
          <p class="indent1"><strong>2</strong><strong>−5　</strong>法第2条第1項第8号に規定する法人でない社団とは、多数の者が…</p>
          <p class="indent2">（1） 民法第667条の規定による組合</p>
          <h2>（法人でない財団の範囲）</h2>
          <p class="indent1"><strong>2</strong><strong>−6　</strong>法第2条第1項第8号に規定する法人でない財団とは…</p>
        </body></html>"#;
        let items = parse_body(html, "https://x/01/02.htm", "shotoku");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].number, "2-5");
        assert_eq!(items[0].caption.as_deref(), Some("法人でない社団の範囲"));
        assert!(items[0].text.contains("法人でない社団とは"));
        assert_eq!(items[1].number, "2-6");
    }

    #[test]
    #[ignore]
    fn http_real_fetch() {
        let ks = known_sets().into_iter().next().unwrap();
        let (tax, name) = (ks.tax, ks.name);
        let p = HttpProvider::new();
        let pages = p.list_pages(&ks.index_url).unwrap();
        println!("{name}: {} body pages", pages.len());
        assert!(!pages.is_empty());
        // 前文ページは項目を持たないので、項目が出るまで数ページ試す。
        let mut total = 0usize;
        for page in pages.iter().take(5) {
            let items = p.fetch_page(page, tax).unwrap();
            if !items.is_empty() && total == 0 {
                println!("e.g. {:?}", items.first().map(|i| (&i.number, &i.caption, i.text.chars().take(20).collect::<String>())));
            }
            total += items.len();
        }
        println!("items in first 5 pages: {total}");
        assert!(total > 0);
    }
}
