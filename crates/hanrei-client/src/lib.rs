//! 裁判所 裁判例検索 (https://www.courts.go.jp/hanrei/) のクライアント。
//!
//! 取得するのはメタデータ (事件番号・事件名・裁判年月日・法廷名・結果・参照法条など) と
//! 全文 PDF への URL と、新規取得時に PDF から抽出した本文。サイト利用条件は「著作権法上認められている利用」「無断改変禁止」
//! のため、判示事項・裁判要旨・参照法条は原文のまま保持し、出典 URL を必ず添える。
//! robots.txt は `/hanrei/` を禁止していない (2026-10 確認)。リクエスト間隔は
//! [`HttpProvider`] が空ける。

use anyhow::{Context, Result};
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const BASE: &str = "https://www.courts.go.jp";

/// 一覧の 1 行。`case_id` は courts.go.jp 内部の連番 (`/hanrei/{case_id}/detail2/`)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseListItem {
    pub case_id: u64,
    pub case_number: String,
    pub case_name: String,
    /// 和暦のまま (例: 令和8年8月28日)。
    pub judged_on: String,
    pub court: String,
    pub detail_url: String,
    pub pdf_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseDetail {
    pub case_id: u64,
    pub case_number: String,
    pub case_name: String,
    pub judged_on: String,
    pub court: String,
    pub judgment_type: String,
    pub result: String,
    pub reporter: String,
    pub lower_court: String,
    pub lower_case_number: String,
    pub lower_judged_on: String,
    pub holding: String,
    pub summary: String,
    pub referenced_laws: String,
    /// 全文 PDF から機械的に抽出した本文。旧キャッシュ・抽出失敗では空。
    #[serde(default)]
    pub body_text: String,
    /// 全文 PDF 本文から抜いた「…第N条」形式の引用文字列。
    /// 法令との突き合わせは linking が行う。
    #[serde(default)]
    pub body_citations: Vec<String>,
    pub detail_url: String,
    pub pdf_url: Option<String>,
}

fn sel(s: &str) -> Selector {
    Selector::parse(s).expect("static selector")
}

fn text_of(e: ElementRef) -> String {
    e.text().collect::<Vec<_>>().join(" ")
}

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `./../../assets/x.pdf` のような相対 URL を `page_url` 基準で絶対 URL にする。
fn abs_url(page_url: &str, href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        return href.to_string();
    }
    let after_scheme = page_url.find("://").map(|i| i + 3).unwrap_or(0);
    let path_start = page_url[after_scheme..]
        .find('/')
        .map(|j| after_scheme + j)
        .unwrap_or(page_url.len());
    let (origin, path) = page_url.split_at(path_start);
    if let Some(rest) = href.strip_prefix('/') {
        return format!("{origin}/{rest}");
    }
    let path = path.split(['?', '#']).next().unwrap_or("");
    let mut segs: Vec<&str> = path.split('/').collect();
    segs.pop(); // ファイル名 (index.html) を落とす
    for part in href.split('/') {
        match part {
            "." | "" => {}
            ".." => {
                if segs.len() > 1 {
                    segs.pop();
                }
            }
            p => segs.push(p),
        }
    }
    format!("{origin}{}", segs.join("/"))
}

fn case_id_from_url(url: &str) -> Option<u64> {
    url.split('/').find_map(|p| p.parse::<u64>().ok())
}

/// 検索結果一覧 (`search2`〜`search7`) の HTML をパースする。
pub fn parse_list(html: &str, page_url: &str) -> Vec<CaseListItem> {
    let doc = Html::parse_document(html);
    let (tr, th_a, td, p, pdf_a) = (
        sel("tr"),
        sel("th a[href]"),
        sel("td"),
        sel("td p"),
        sel("td.file-col a[href]"),
    );
    let mut out = Vec::new();
    for row in doc.select(&tr) {
        let Some(a) = row.select(&th_a).next() else { continue };
        let href = a.value().attr("href").unwrap_or_default();
        let Some(case_id) = case_id_from_url(href) else { continue };
        let Some(info) = row.select(&td).next() else { continue };
        let paras: Vec<String> = info.select(&p).map(|e| squash(&text_of(e))).collect();
        let first = paras.first().cloned().unwrap_or_default();
        // 1 段落目は「事件番号 事件名」。事件番号は最初の空白まで。
        let (case_number, case_name) = match first.split_once(' ') {
            Some((a, b)) => (a.to_string(), b.to_string()),
            None => (first, String::new()),
        };
        // 2 段落目は「裁判年月日 法廷名 種別 結果 …」。
        let second = paras.get(1).cloned().unwrap_or_default();
        let mut it = second.split(' ');
        let judged_on = it.next().unwrap_or_default().to_string();
        let court = it.next().unwrap_or_default().to_string();
        out.push(CaseListItem {
            case_id,
            case_number,
            case_name,
            judged_on,
            court,
            detail_url: abs_url(page_url, href),
            pdf_url: row
                .select(&pdf_a)
                .next()
                .and_then(|a| a.value().attr("href"))
                .map(|h| abs_url(page_url, h)),
        });
    }
    out
}

/// 詳細ページ (`/hanrei/{id}/detail2/`) をパースする。見出し (`dt`) と値 (`dd`) が
/// 項目ごとの `dl` に分かれているため、`dt`/`dd` を文書順に読んで対応づける。
pub fn parse_detail(html: &str, page_url: &str) -> Result<CaseDetail> {
    let case_id = case_id_from_url(page_url)
        .with_context(|| format!("case id not found in {page_url}"))?;
    let doc = Html::parse_document(html);
    let mut kv: Vec<(String, String)> = Vec::new();
    let mut key: Option<String> = None;
    for e in doc.select(&sel("dl dt, dl dd")) {
        let t = squash(&text_of(e));
        if e.value().name() == "dt" {
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
    anyhow::ensure!(!kv.is_empty(), "no definition list found in {page_url}");
    let get = |name: &str| {
        kv.iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    Ok(CaseDetail {
        case_id,
        case_number: get("事件番号"),
        case_name: get("事件名"),
        judged_on: get("裁判年月日"),
        court: get("法廷名"),
        judgment_type: get("裁判種別"),
        result: get("結果"),
        reporter: get("判例集等巻・号・頁"),
        lower_court: get("原審裁判所名"),
        lower_case_number: get("原審事件番号"),
        lower_judged_on: get("原審裁判年月日"),
        holding: get("判示事項"),
        summary: get("裁判要旨"),
        referenced_laws: get("参照法条"),
        body_text: String::new(),
        body_citations: Vec::new(),
        detail_url: page_url.to_string(),
        pdf_url: doc
            .select(&sel("a[href$='.pdf']"))
            .next()
            .and_then(|a| a.value().attr("href"))
            .map(|h| abs_url(page_url, h)),
    })
}

/// 一覧ページの「N件中 a～b件を表示」から総件数 N を読む。
pub fn parse_total(html: &str) -> Option<usize> {
    let head = &html[..html.find("件中")?];
    let rev: String = head
        .chars()
        .rev()
        .skip_while(|c| c.is_whitespace())
        .take_while(|c| c.is_ascii_digit() || *c == ',')
        .filter(|c| *c != ',')
        .collect();
    rev.chars().rev().collect::<String>().parse().ok()
}

/// 全文 PDF からテキストを取り出す。語を読み順につなぎ、空白は入れない (和文のため)。
pub fn pdf_text(pdf: &[u8]) -> Result<String> {
    let pages = kanpo_pdf::extract_pages(pdf).map_err(|e| anyhow::anyhow!("pdf: {e}"))?;
    let mut out = String::new();
    for page in &pages {
        for w in kanpo_pdf::bbox::words(page) {
            out.push_str(&w.text);
        }
    }
    Ok(out)
}

/// 引用 1 件の前方に遡る最大文字数 (法令名の長さの上限)。
const CITATION_LOOKBACK: usize = 40;
/// 1 判決あたりに残す引用の上限。
const MAX_CITATIONS: usize = 200;

fn is_citation_stop(c: char) -> bool {
    c.is_whitespace() || "、。，．,.「」『』〔〕[]：:；;！？".contains(c)
}

/// 本文から `法令名 + 第N条` 形式の引用 (例: `民法７０９条`) を抜き出す。法令名は条番号の
/// 直前から句読点・括弧類・空白までを最大 [`CITATION_LOOKBACK`] 文字だけ遡って取る。
/// 厳密な法令名の特定はしない (後段が法令名辞書で最長一致する)。重複は除く。
pub fn extract_citations(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
    let is_num = |c: char| {
        c.is_ascii_digit() || ('０'..='９').contains(&c) || "〇一二三四五六七八九十百千".contains(c)
    };
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        // 「第?N条」の「条」を探す。
        if chars[i] != '条' || i == 0 || !is_num(chars[i - 1]) {
            i += 1;
            continue;
        }
        let mut num_start = i;
        while num_start > 0 && is_num(chars[num_start - 1]) {
            num_start -= 1;
        }
        let art_start = if num_start > 0 && chars[num_start - 1] == '第' {
            num_start - 1
        } else {
            num_start
        };
        let mut name_start = art_start;
        while name_start > 0
            && art_start - name_start < CITATION_LOOKBACK
            && !is_citation_stop(chars[name_start - 1])
            // 直前の条番号や「条」に食い込まない (「民法７０９条、７１０条」の 2 件目など)
            && chars[name_start - 1] != '条'
        {
            name_start -= 1;
        }
        let cite: String = chars[name_start..=i].iter().collect();
        if name_start < art_start && !out.contains(&cite) {
            out.push(cite);
            if out.len() >= MAX_CITATIONS {
                break;
            }
        }
        i += 1;
    }
    out
}

/// 西暦の日付 (検索条件用)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ymd {
    pub y: i32,
    pub m: u32,
    pub d: u32,
}

impl Ymd {
    /// 検索フォームの「元号・年」。令和・平成・昭和のみ (それ以前の判例は対象外)。
    pub fn era(&self) -> Option<(&'static str, i32)> {
        let k = self.y * 10000 + self.m as i32 * 100 + self.d as i32;
        if k >= 2019_05_01 {
            Some(("令和", self.y - 2018))
        } else if k >= 1989_01_08 {
            Some(("平成", self.y - 1988))
        } else if k >= 1926_12_25 {
            Some(("昭和", self.y - 1925))
        } else {
            None
        }
    }
}

/// 日付範囲検索の 1 ページ分。`total` はヒット総件数 (サイト側の上限は 2000 件)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListPage {
    pub items: Vec<CaseListItem>,
    pub total: usize,
}

/// 1 ページあたりの表示件数 (`offset` の刻み)。
pub const PAGE_SIZE: usize = 30;

/// 日付範囲検索のクエリ。`offset` は結果の先頭位置 (`PAGE_SIZE` 刻み)。
pub fn range_query(
    court_case_type: u8,
    from: Ymd,
    to: Ymd,
    offset: usize,
) -> Result<Vec<(String, String)>> {
    let (fg, fy) = from.era().context("from date is before 昭和")?;
    let (tg, ty) = to.era().context("to date is before 昭和")?;
    let kv = |k: &str, v: String| (k.to_string(), v);
    let mut q = vec![
        kv("courtCaseType", court_case_type.to_string()),
        kv("filter[judgeDateMode]", "2".into()),
        kv("filter[judgeGengoFrom]", fg.into()),
        kv("filter[judgeYearFrom]", fy.to_string()),
        kv("filter[judgeMonthFrom]", from.m.to_string()),
        kv("filter[judgeDayFrom]", from.d.to_string()),
        kv("filter[judgeGengoTo]", tg.into()),
        kv("filter[judgeYearTo]", ty.to_string()),
        kv("filter[judgeMonthTo]", to.m.to_string()),
        kv("filter[judgeDayTo]", to.d.to_string()),
    ];
    if offset > 0 {
        q.push(kv("offset", offset.to_string()));
    }
    Ok(q)
}

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

    fn get_text(&self, url: &str) -> Result<String> {
        std::thread::sleep(self.interval);
        let resp = self.client.get(url).send()?.error_for_status()?;
        Ok(resp.text()?)
    }

    /// 分野別ページ (`search{search_page}`: 2=最高裁 3=高裁 4=下級裁 5=行政 6=労働 7=知財)
    /// の「最近の裁判例」一覧。`court_case_type` はそのページの `courtCaseType` 値。
    pub fn fetch_recent(&self, search_page: u8, court_case_type: u8) -> Result<Vec<CaseListItem>> {
        let url = format!(
            "{BASE}/hanrei/search{search_page}/index.html?courtCaseType={court_case_type}&filter[recent]=1"
        );
        let html = self.get_text(&url)?;
        Ok(parse_list(&html, &url))
    }

    /// 裁判年月日の範囲で検索した 1 ページ分 (`offset` は `PAGE_SIZE` 刻み)。
    pub fn fetch_range_page(
        &self,
        search_page: u8,
        court_case_type: u8,
        from: Ymd,
        to: Ymd,
        offset: usize,
    ) -> Result<ListPage> {
        let url = format!("{BASE}/hanrei/search{search_page}/index.html");
        let query = range_query(court_case_type, from, to, offset)?;
        std::thread::sleep(self.interval);
        let resp = self.client.get(&url).query(&query).send()?.error_for_status()?;
        let page_url = resp.url().to_string();
        let html = resp.text()?;
        Ok(ListPage {
            items: parse_list(&html, &page_url),
            total: parse_total(&html).unwrap_or(0),
        })
    }

    /// 全文 PDF を取得して引用文字列を抜く。
    pub fn fetch_body_citations(&self, pdf_url: &str) -> Result<Vec<String>> {
        Ok(extract_citations(&self.fetch_body_text(pdf_url)?))
    }

    /// 新規判例の Wiki で引用できるよう、抽出本文をそのまま返す。
    pub fn fetch_body_text(&self, pdf_url: &str) -> Result<String> {
        std::thread::sleep(self.interval);
        let bytes = self.client.get(pdf_url).send()?.error_for_status()?.bytes()?;
        pdf_text(&bytes)
    }

    pub fn fetch_detail(&self, case_id: u64) -> Result<CaseDetail> {
        let url = format!("{BASE}/hanrei/{case_id}/detail2/index.html");
        let html = self.get_text(&url)?;
        parse_detail(&html, &url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_metadata_without_body_remains_readable_and_new_body_round_trips() {
        let detail = parse_detail(
            "<dl><dt>事件名</dt><dd>事件</dd></dl>",
            "https://www.courts.go.jp/hanrei/123/detail2/index.html",
        ).unwrap();
        let mut old = serde_json::to_value(&detail).unwrap();
        old.as_object_mut().unwrap().remove("body_text");
        let cached: CaseDetail = serde_json::from_value(old.clone()).unwrap();
        assert_eq!(serde_json::to_value(cached).unwrap()["body_text"], "");
        old["body_text"] = serde_json::json!("民法第七百九条による損害賠償について判断する。");
        let new: CaseDetail = serde_json::from_value(old).unwrap();
        assert_eq!(serde_json::to_value(new).unwrap()["body_text"], "民法第七百九条による損害賠償について判断する。");
    }

    const LIST: &str = r#"
<p>11件中 1～11件を表示</p>
<table><tbody>
<tr>
  <th><a href="./../96970/detail2/index.html">最高裁判例</a></th>
  <td>
    <p>
      令和6(受)2169
      建物買取代金請求事件
    </p>
    <p>
      令和8年8月28日
      最高裁判所第二小法廷
      判決
      棄却
    </p>
  </td>
  <td class="file-col"><p><a href="./../../assets/hanrei/hanrei-pdf-96970.pdf">全文</a></p></td>
</tr>
</tbody></table>"#;

    #[test]
    fn parses_list_row() {
        let url = "https://www.courts.go.jp/hanrei/search2/index.html?courtCaseType=1";
        let items = parse_list(LIST, url);
        assert_eq!(items.len(), 1);
        let it = &items[0];
        assert_eq!(it.case_id, 96970);
        assert_eq!(it.case_number, "令和6(受)2169");
        assert_eq!(it.case_name, "建物買取代金請求事件");
        assert_eq!(it.judged_on, "令和8年8月28日");
        assert_eq!(it.court, "最高裁判所第二小法廷");
        assert_eq!(
            it.detail_url,
            "https://www.courts.go.jp/hanrei/96970/detail2/index.html"
        );
        assert_eq!(
            it.pdf_url.as_deref(),
            Some("https://www.courts.go.jp/assets/hanrei/hanrei-pdf-96970.pdf")
        );
        assert_eq!(parse_total(LIST), Some(11));
    }

    #[test]
    fn extracts_citations_from_body() {
        let text = "原判決は、民法７０９条に違反し、刑事訴訟法第３８２条、\
                    会社法の趣旨に反するとした。借地借家法１４条の２の適用も問題となる。";
        let got = extract_citations(text);
        assert!(got.contains(&"民法７０９条".to_string()), "{got:?}");
        assert!(got.contains(&"刑事訴訟法第３８２条".to_string()), "{got:?}");
        assert!(got.iter().any(|c| c.ends_with("借地借家法１４条")), "{got:?}");
        // 条番号を伴わない法令名は拾わない。
        assert!(!got.iter().any(|c| c.contains("会社法")), "{got:?}");
    }

    #[test]
    fn era_boundaries() {
        let e = |y, m, d| Ymd { y, m, d }.era();
        assert_eq!(e(2025, 3, 1), Some(("令和", 7)));
        assert_eq!(e(2019, 5, 1), Some(("令和", 1)));
        assert_eq!(e(2019, 4, 30), Some(("平成", 31)));
        assert_eq!(e(1989, 1, 8), Some(("平成", 1)));
        assert_eq!(e(1989, 1, 7), Some(("昭和", 64)));
        assert_eq!(e(1926, 12, 24), None);
    }

    #[test]
    fn range_query_spans_eras() {
        let q = range_query(
            1,
            Ymd { y: 2019, m: 4, d: 1 },
            Ymd { y: 2019, m: 5, d: 31 },
            60,
        )
        .unwrap();
        let get = |k: &str| q.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str());
        assert_eq!(get("filter[judgeGengoFrom]"), Some("平成"));
        assert_eq!(get("filter[judgeYearFrom]"), Some("31"));
        assert_eq!(get("filter[judgeGengoTo]"), Some("令和"));
        assert_eq!(get("filter[judgeYearTo]"), Some("1"));
        assert_eq!(get("offset"), Some("60"));
    }

    #[test]
    fn parses_detail() {
        let html = r#"
<dl><dt>事件番号</dt><dd><p> 令和6(受)2169 </p></dd></dl>
<dl><dt>事件名</dt><dd>建物買取代金請求事件</dd></dl>
<dl><dt>裁判年月日</dt><dd>令和8年8月28日</dd></dl>
<dl><dt>法廷名</dt><dd>最高裁判所第二小法廷 </dd></dl>
<dl><dt>裁判種別</dt><dd>判決</dd></dl>
<dl><dt>結果</dt><dd>棄却</dd></dl>
<dl><dt>判例集等巻・号・頁</dt><dd></dd></dl>
<dl><dt>判示事項</dt><dd>借地借家法１４条所定の建物買取請求権は成立しない</dd></dl>
<dl><dt>裁判要旨</dt><dd></dd></dl>
<dl><dt>参照法条</dt><dd>借地借家法14条</dd></dl>
<a href="/assets/hanrei/hanrei-pdf-96970.pdf">全文</a>"#;
        let d = parse_detail(html, "https://www.courts.go.jp/hanrei/96970/detail2/index.html")
            .unwrap();
        assert_eq!(d.case_id, 96970);
        assert_eq!(d.case_number, "令和6(受)2169");
        assert_eq!(d.court, "最高裁判所第二小法廷");
        assert_eq!(d.result, "棄却");
        assert_eq!(d.reporter, "");
        assert_eq!(d.summary, "");
        assert_eq!(d.referenced_laws, "借地借家法14条");
        assert_eq!(
            d.pdf_url.as_deref(),
            Some("https://www.courts.go.jp/assets/hanrei/hanrei-pdf-96970.pdf")
        );
    }
}
