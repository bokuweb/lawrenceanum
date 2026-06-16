//! デジタル官報サイト (`https://www.kanpo.go.jp/`) からの取得実装。
//!
//! 2025 年のデジタル官報移行後、本文の実体は項目別 PDF として配信され、
//! 日付ごとの目次 `{YYYYMMDD}/{YYYYMMDD}.fullcontents.html` から
//!   - 号 (本紙 / 号外 / 特別号外)
//!   - 各号に含まれる項目 (= 法令1件) の標題・開始ページ
//! を辿れる。項目別 PDF の URL は目次リンクから機械的に導出できるため、
//! e-Gov 改正イベントとの突合は「号」ではなく「項目」粒度で行える。
//!
//! 例: 目次リンク `20260615h01726/20260615h017260002f.html`
//!   issue dir = `20260615h01726`
//!   item file = `20260615h017260002`  (末尾 `f` を除去)
//!   page      = `0002`               (issue dir を取り除いた残り)
//!   PDF       = `{base}/20260615/20260615h01726/pdf/20260615h017260002.pdf`

use crate::{KanpoDate, KanpoIssue, KanpoItem, KanpoProvider};
use anyhow::{anyhow, Context, Result};
use scraper::{Html, Selector};
use std::time::Duration;

const DEFAULT_BASE: &str = "https://www.kanpo.go.jp";
const USER_AGENT: &str =
    "lawrenceanum-kanpo/0.1 (+https://github.com/bokuweb/lawrenceanum; research/non-commercial)";

pub struct HttpProvider {
    base_url: String,
    client: reqwest::blocking::Client,
}

impl HttpProvider {
    pub fn new() -> Result<Self> {
        let base_url = std::env::var("LAWPUB_KANPO_BASE_URL")
            .unwrap_or_else(|_| DEFAULT_BASE.to_string())
            .trim_end_matches('/')
            .to_string();
        let client = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(60))
            .build()
            .context("build kanpo http client")?;
        Ok(Self { base_url, client })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// 任意 URL をバイト列で取得（PDF ダウンロード等にも使う）。簡易リトライ付き。
    pub fn get_bytes(&self, url: &str) -> Result<Vec<u8>> {
        let mut last: Option<anyhow::Error> = None;
        for attempt in 0..4 {
            match self
                .client
                .get(url)
                .send()
                .and_then(|r| r.error_for_status())
                .and_then(|r| r.bytes())
            {
                Ok(b) => return Ok(b.to_vec()),
                Err(e) => last = Some(anyhow!(e)),
            }
            std::thread::sleep(Duration::from_secs(1 << attempt));
        }
        Err(last.unwrap_or_else(|| anyhow!("fetch failed: {url}")))
    }

    fn get_text(&self, url: &str) -> Result<String> {
        let bytes = self.get_bytes(url)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    fn toc_url(&self, compact: &str) -> String {
        format!("{}/{c}/{c}.fullcontents.html", self.base_url, c = compact)
    }
}

impl KanpoProvider for HttpProvider {
    /// `date` は "YYYY-MM-DD"。当日の目次をパースして号・項目を返す。
    fn fetch_date(&self, date: &str) -> Result<KanpoDate> {
        let compact = compact_date(date)?;
        let url = self.toc_url(&compact);
        let html = self
            .get_text(&url)
            .with_context(|| format!("fetch kanpo TOC {url}"))?;
        let mut issues = parse_toc(&self.base_url, &compact, date, &html)?;
        // 各号の総ページ数を号インデックスから取得（項目の終端ページ算定に使う）。
        for issue in &mut issues {
            if let Some(dir) = issue_dir(&issue.pdf_url) {
                let idx_url = format!("{}{}0000f.html", issue.pdf_url, dir);
                if let Ok(idx) = self.get_text(&idx_url) {
                    issue.page_count = max_page_in_index(&idx, &dir);
                }
            }
        }
        Ok(KanpoDate {
            date: date.to_string(),
            issues,
        })
    }
}

/// "YYYY-MM-DD" / "YYYYMMDD" を "YYYYMMDD" に正規化。
fn compact_date(date: &str) -> Result<String> {
    let digits: String = date.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() != 8 {
        return Err(anyhow!("invalid kanpo date: {date}"));
    }
    Ok(digits)
}

/// 目次 HTML を号・項目に分解する。
///
/// 目次は文書順に「号見出し (`javascript:void(0)` な `<a>`、テキストに『第N号』)」
/// → 「その号の項目リンク (`<issuedir>/...f.html`)」が並ぶ。文書順を保ったまま
/// `<a>` を走査し、号見出しで issue を切り替えながら項目を積む。
pub fn parse_toc(base_url: &str, compact: &str, date: &str, html: &str) -> Result<Vec<KanpoIssue>> {
    let doc = Html::parse_document(html);
    let a_sel = Selector::parse("a").unwrap();

    let mut issues: Vec<KanpoIssue> = Vec::new();
    let mut cur: Option<KanpoIssue> = None;

    for a in doc.select(&a_sel) {
        let href = a.value().attr("href").unwrap_or("");
        let text = normalize_ws(&a.text().collect::<String>());

        // 号見出し: 「本紙 第1726号」「号外 第131号」「特別号外 …」など。
        if let Some((issue_type, issue_no)) = parse_issue_heading(&text) {
            if let Some(done) = cur.take() {
                issues.push(done);
            }
            cur = Some(KanpoIssue {
                issue_type,
                issue_no,
                pdf_url: String::new(),
                sha256: None,
                promulgation_date: date.to_string(),
                law_nums: vec![],
                titles: vec![],
                items: vec![],
                page_count: None,
            });
            continue;
        }

        // 項目リンク: `20260615h01726/20260615h017260002f.html`
        if let Some(item) = parse_item_link(base_url, compact, href, &text) {
            if let Some(issue) = cur.as_mut() {
                // 号フル PDF をまだ持っていなければ、項目の issue dir から導出。
                if issue.pdf_url.is_empty() {
                    if let Some(dir) = href.split('/').next() {
                        issue.pdf_url = format!("{base_url}/{compact}/{dir}/");
                    }
                }
                // (title, page) で重複排除しつつ追加。
                if !issue
                    .items
                    .iter()
                    .any(|x| x.page == item.page && x.title == item.title)
                {
                    issue.titles.push(item.title.clone());
                    issue.items.push(item);
                }
            }
        }
    }
    if let Some(done) = cur.take() {
        issues.push(done);
    }
    Ok(issues)
}

/// 「本紙　第1726号」→ ("regular", "第1726号")。号見出しでなければ None。
fn parse_issue_heading(text: &str) -> Option<(String, String)> {
    let t = text.replace('\u{3000}', " ");
    let t = t.trim();
    if !(t.contains("第") && t.contains("号")) {
        return None;
    }
    let issue_type = if t.starts_with("特別号外") {
        "special_extra"
    } else if t.starts_with("号外") {
        "extra"
    } else if t.starts_with("本紙") {
        "regular"
    } else {
        return None;
    };
    // 「第…号」部分を抜き出す。
    let start = t.find('第')?;
    let rest = &t[start..];
    let end = rest.find('号')? + '号'.len_utf8();
    Some((issue_type.to_string(), rest[..end].to_string()))
}

/// 項目リンクを KanpoItem に変換。対象外リンクなら None。
fn parse_item_link(base_url: &str, compact: &str, href: &str, text: &str) -> Option<KanpoItem> {
    // 期待形: `<issuedir>/<stem>f.html` で issuedir/stem とも compact 日付で始まる。
    if !href.ends_with("f.html") {
        return None;
    }
    let (dir, file) = href.split_once('/')?;
    if !dir.starts_with(compact) || !file.starts_with(compact) {
        return None;
    }
    let stem = file.strip_suffix("f.html")?; // 例: 20260615h017260002
    let page_str = stem.strip_prefix(dir)?; // 例: 0002
    let page: u32 = page_str.parse().ok()?;
    let title = clean_title(text);
    if title.is_empty() {
        return None;
    }
    let pdf_url = format!("{base_url}/{compact}/{dir}/pdf/{stem}.pdf");
    let agency_hint = extract_agency_hint(&title);
    Some(KanpoItem {
        title,
        page,
        pdf_url,
        sha256: None,
        agency_hint,
        amend_text: None,
        amend_format: None,
    })
}

/// 目次テキストから末尾のページ番号や余分な空白を除いた標題を得る。
fn clean_title(text: &str) -> String {
    let t = normalize_ws(text);
    // 目次は標題の後ろにページ番号 (半角数字) が付くことがある。末尾の数字を落とす。
    let trimmed = t.trim_end_matches(|c: char| c.is_ascii_digit() || c.is_whitespace());
    trimmed.trim().to_string()
}

/// 標題末尾の `（総務七七）` のような括弧内（制定機関略号）を取り出す。
fn extract_agency_hint(title: &str) -> Option<String> {
    let open = title.rfind('（')?;
    let close = title.rfind('）')?;
    if close > open {
        let inner = &title[open + '（'.len_utf8()..close];
        if !inner.is_empty() {
            return Some(inner.to_string());
        }
    }
    None
}

fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// issue.pdf_url (`{base}/{compact}/{dir}/`) から号ディレクトリ名 `{dir}` を取り出す。
fn issue_dir(pdf_url: &str) -> Option<String> {
    pdf_url
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// 号インデックス HTML から `{dir}{NNNN}f.html` の最大ページ番号を返す。
fn max_page_in_index(html: &str, dir: &str) -> Option<u32> {
    let needle = format!("{dir}");
    let mut max = 0u32;
    for part in html.split(&needle).skip(1) {
        // part 先頭が `NNNNf.html` の形か（full 等は弾く）。
        let bytes = part.as_bytes();
        if bytes.len() >= 4 && bytes[..4].iter().all(|b| b.is_ascii_digit()) {
            if let Ok(n) = part[..4].parse::<u32>() {
                max = max.max(n);
            }
        }
    }
    if max > 0 {
        Some(max)
    } else {
        None
    }
}

/// 項目別 PDF URL (`.../pdf/{dir}{PPPP}.pdf`) の PPPP を別ページ番号に差し替える。
pub fn page_pdf_url(item_pdf_url: &str, current_page: u32, target_page: u32) -> Option<String> {
    let suffix = format!("{:04}.pdf", current_page);
    let prefix = item_pdf_url.strip_suffix(&suffix)?;
    Some(format!("{prefix}{:04}.pdf", target_page))
}
