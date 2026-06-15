//! 官報項目別 PDF からの改め文テキスト抽出（PoC）。
//!
//! デジタル官報 PDF はテキスト層を持つ（OCR 不要）が、**縦書き・多段組**のため
//! `pdftotext -layout` の素の出力には
//!   1. 縦書き presentation form の約物（︑ ︒ ﹇ ﹈ ︵ ︶ …）
//!   2. ページ柱（「官」「報」「令和 年 月 日」等の余白テキスト）の混入
//!   3. 新旧対照表での 改正後／改正前 2 カラムの並走
//! という癖がある。本モジュールは (1)(2) を best-effort で整形し、(3) は形式判定して
//! 呼び出し側に委ねる。`pdftotext`(poppler) への依存を前提とする。

use anyhow::{anyhow, Context, Result};
use scraper::{Html, Selector};
use std::io::Write;
use std::process::{Command, Stdio};

/// 抽出結果。`text` は整形済み本文、`format` は "prose"/"shinkyu"/"unknown"。
#[derive(Debug, Clone)]
pub struct Extracted {
    pub text: String,
    pub format: String,
}

/// PDF バイト列（官報1ページ分）から、縦書きの読み順を復元したページ本文を返す。
///
/// `pdftotext -bbox-layout` は各縦カラムを 1 つの `<word>`（幅≒1文字・高さ=文字数分、
/// テキストは上→下に整列済み）として出力する。よって **カラムを右→左に並べ替える**
/// だけで正しい読み順が復元でき、`-layout` で起きる「別記事の横方向への混線」を解消できる。
pub fn extract(pdf: &[u8]) -> Result<Extracted> {
    let xhtml = run_pdftotext_bbox(pdf)?;
    let text = reconstruct_vertical(&xhtml);
    let format = detect_format(&text);
    Ok(Extracted { text, format })
}

/// 縦書き1カラム = 1 word とみなした語。`y`/`y1` は上端/下端。
struct Col {
    x: f32,
    y: f32,
    y1: f32,
    text: String,
}

/// ページ上下の余白帯にある柱（「官」「報」「令和 年 月 日」等）を落とすための閾値(pt)。
const TOP_MARGIN: f32 = 56.0;
const BOTTOM_MARGIN: f32 = 30.0;

/// `-bbox-layout` の XHTML から縦書き読み順（右→左・各カラム上→下）を復元する。
/// 複数ページの PDF はページごとに復元して連結する。
pub fn reconstruct_vertical(xhtml: &str) -> String {
    let doc = Html::parse_document(xhtml);
    let page_sel = Selector::parse("page").unwrap();
    let word_sel = Selector::parse("word").unwrap();

    let mut pages: Vec<String> = Vec::new();
    for page in doc.select(&page_sel) {
        let height = page
            .value()
            .attr("height")
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(842.0);
        let mut cols: Vec<Col> = Vec::new();
        for w in page.select(&word_sel) {
            let v = w.value();
            // scraper は HTML パーサのため属性名を小文字化する（xMin -> xmin）。
            let x = v.attr("xmin").and_then(|s| s.parse::<f32>().ok());
            let y = v.attr("ymin").and_then(|s| s.parse::<f32>().ok());
            let y1 = v.attr("ymax").and_then(|s| s.parse::<f32>().ok());
            let (x, y, y1) = match (x, y, y1) {
                (Some(x), Some(y), Some(y1)) => (x, y, y1),
                _ => continue,
            };
            // 上下余白帯の柱（発行日・「官報」等の running header）を除外。
            if y < TOP_MARGIN || y > height - BOTTOM_MARGIN {
                continue;
            }
            // scraper が実体参照は復号済み。約物を正規化する。
            let text: String =
                w.text().collect::<String>().chars().map(normalize_char).collect();
            if text.trim().is_empty() {
                continue;
            }
            cols.push(Col { x, y, y1, text });
        }
        // 段組み(tier)ごとに分けてから各段を右→左に再構成する。官報本紙は2段組が多く、
        // 段を分けないと「同 x にある上段の列と下段の列」が1列に連結され記事が混線する。
        for tier in split_tiers(cols) {
            let page_text = reconstruct_page(tier);
            if !page_text.is_empty() {
                pages.push(page_text);
            }
        }
    }
    pages.join("\n").trim().to_string()
}

/// カラム語を縦方向の段(tier)に分割する。
///
/// 各段は独立した右→左の縦フロー。段の境目は「どのカラムも跨がない y のすき間」
/// として現れる（段内ではカラムが段の高さいっぱいに走るため y は密に覆われる）。
/// y被覆の空白(>= GAP_MIN)で区切り、各カラムを中心 y が属する段へ割り当てる。
fn split_tiers(cols: Vec<Col>) -> Vec<Vec<Col>> {
    const GAP_MIN: f32 = 6.0;
    if cols.len() < 2 {
        return vec![cols];
    }
    // y 区間を結合して被覆と空白を求める。
    let mut spans: Vec<(f32, f32)> = cols.iter().map(|c| (c.y, c.y1)).collect();
    spans.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut bands: Vec<(f32, f32)> = Vec::new();
    let (mut lo, mut hi) = spans[0];
    for &(s, e) in &spans[1..] {
        if s > hi + GAP_MIN {
            bands.push((lo, hi));
            lo = s;
            hi = e;
        } else if e > hi {
            hi = e;
        }
    }
    bands.push((lo, hi));
    if bands.len() < 2 {
        return vec![cols];
    }
    // 各カラムを中心 y が入る段へ割り当て。
    let mut tiers: Vec<Vec<Col>> = bands.iter().map(|_| Vec::new()).collect();
    for c in cols {
        let center = (c.y + c.y1) / 2.0;
        let idx = bands
            .iter()
            .position(|&(lo, hi)| center >= lo && center <= hi)
            .unwrap_or(0);
        tiers[idx].push(c);
    }
    tiers
}

/// 1ページ分のカラム語を右→左・上→下に並べて本文に組む。
fn reconstruct_page(mut cols: Vec<Col>) -> String {
    if cols.is_empty() {
        return String::new();
    }
    // x 降順（右→左）にカラムをクラスタリング。列間隔(≒13px)に対し閾値 10px。
    cols.sort_by(|a, b| b.x.partial_cmp(&a.x).unwrap_or(std::cmp::Ordering::Equal));
    const COL_THRESHOLD: f32 = 10.0;
    let mut groups: Vec<Vec<Col>> = Vec::new();
    let mut rep_x = f32::INFINITY;
    for c in cols {
        if rep_x - c.x > COL_THRESHOLD {
            groups.push(Vec::new());
            rep_x = c.x;
        }
        groups.last_mut().unwrap().push(c);
    }

    // 各カラム内は y 昇順（上→下）に連結。柱（発行日・号数）カラムは捨てる。
    let mut lines: Vec<String> = Vec::new();
    for mut g in groups {
        g.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));
        let line: String = g.into_iter().map(|c| c.text).collect();
        if is_margin_noise(&line) {
            continue;
        }
        lines.push(line);
    }
    lines.join("\n").trim().to_string()
}

/// `pdftotext -bbox-layout -enc UTF-8 - -` を stdin/stdout で実行。
fn run_pdftotext_bbox(pdf: &[u8]) -> Result<String> {
    run_pdftotext(pdf, &["-bbox-layout", "-enc", "UTF-8", "-", "-"])
}

/// `pdftotext` を任意オプションで stdin/stdout 実行する共通ヘルパ。
fn run_pdftotext(pdf: &[u8], args: &[&str]) -> Result<String> {
    let mut child = Command::new("pdftotext")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("spawn pdftotext (poppler が必要: brew install poppler / apt install poppler-utils)")?;
    child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("pdftotext stdin unavailable"))?
        .write_all(pdf)
        .context("write pdf to pdftotext stdin")?;
    let out = child.wait_with_output().context("wait pdftotext")?;
    if !out.status.success() {
        return Err(anyhow!("pdftotext exited with {}", out.status));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// 官報1ページ分の本文を「記事」単位に分割する。
///
/// 官報は各法令/告示の制定文を `〇` 見出し（例: 「〇総務省令第七十七号」「〇農林水産省
/// 告示第七百六十四号」）で始める。`〇` 始まりのカラム（行）を区切りに分割すると、
/// 1ページに複数記事が詰まっていても記事ごとに切り出せる。
pub fn segment_articles(text: &str) -> Vec<String> {
    let mut articles: Vec<String> = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    for line in text.lines() {
        let t = line.trim_start();
        if t.starts_with('〇') && !cur.is_empty() {
            articles.push(cur.join("\n").trim().to_string());
            cur = Vec::new();
        }
        cur.push(line);
    }
    if !cur.is_empty() {
        let s = cur.join("\n").trim().to_string();
        if !s.is_empty() {
            articles.push(s);
        }
    }
    articles
}

/// 縦書き約物の正規化 + ページ柱ノイズ除去。
pub fn normalize_text(raw: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for line in raw.lines() {
        let line: String = line.chars().map(normalize_char).collect();
        let stripped = strip_margin_lead(&line);
        if is_margin_noise(&stripped) {
            continue;
        }
        lines.push(stripped.trim_end().to_string());
    }
    // 連続する空行を 1 行に畳む。
    let mut out: Vec<String> = Vec::new();
    let mut prev_blank = false;
    for l in lines {
        let blank = l.trim().is_empty();
        if blank && prev_blank {
            continue;
        }
        prev_blank = blank;
        out.push(l);
    }
    out.join("\n").trim().to_string()
}

/// 縦書き presentation form を通常の全角約物に写像する。
fn normalize_char(c: char) -> char {
    match c {
        '\u{FE10}' => '，',
        '\u{FE11}' => '、',
        '\u{FE12}' => '。',
        '\u{FE13}' => '：',
        '\u{FE14}' => '；',
        '\u{FE15}' => '！',
        '\u{FE16}' => '？',
        '\u{FE17}' => '〖',
        '\u{FE18}' => '〗',
        '\u{FE19}' => '…',
        '\u{FE31}' => '—',
        '\u{FE32}' => '–',
        '\u{FE33}' | '\u{FE34}' => '｜',
        '\u{FE35}' => '（',
        '\u{FE36}' => '）',
        '\u{FE37}' => '｛',
        '\u{FE38}' => '｝',
        '\u{FE39}' => '〔',
        '\u{FE3A}' => '〕',
        '\u{FE3B}' => '【',
        '\u{FE3C}' => '】',
        '\u{FE3D}' => '《',
        '\u{FE3E}' => '》',
        '\u{FE3F}' => '〈',
        '\u{FE40}' => '〉',
        '\u{FE41}' => '「',
        '\u{FE42}' => '」',
        '\u{FE43}' => '『',
        '\u{FE44}' => '』',
        '\u{FE47}' => '〔',
        '\u{FE48}' => '〕',
        other => other,
    }
}

/// 行頭にぽつんと現れるページ柱の 1 文字（「官」「報」）を、後続が大きな空白で
/// 区切られている場合に限り除去する。
fn strip_margin_lead(line: &str) -> String {
    let trimmed = line.trim_start();
    let mut chars = trimmed.chars();
    if let Some(first) = chars.next() {
        if matches!(first, '官' | '報') {
            let rest = chars.as_str();
            // 1 文字 + 連続空白(2 個以上) + 本文、という柱パターンのみ剥がす。
            if rest.starts_with("  ") {
                return rest.trim_start().to_string();
            }
            if rest.trim().is_empty() {
                return String::new();
            }
        }
    }
    line.to_string()
}

/// ページ柱（発行日・曜日・号数の余白テキスト）と思われる行か。
fn is_margin_noise(line: &str) -> bool {
    let compact: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.is_empty() {
        return false; // 空行は畳み処理に任せる。
    }
    // 数字（半角・全角）を除いた骨格で判定（「令和  ８  年…」等の数字ゆらぎを吸収）。
    let skeleton: String = compact
        .chars()
        .filter(|c| !c.is_ascii_digit() && !('０'..='９').contains(c))
        .collect();
    const NOISE: &[&str] = &[
        "官",
        "報",
        "令和年月日",
        "平成年月日",
        "号外第号",
        "（号外第号）",
        "(号外第号)",
        "月曜日",
        "火曜日",
        "水曜日",
        "木曜日",
        "金曜日",
        "土曜日",
        "日曜日",
    ];
    NOISE.contains(&skeleton.as_str())
}

/// 記事本文の形式を判定する。判定できない場合は `None`（呼び出し側でページ全体の
/// 判定にフォールバックできるよう Option を返す）。
pub fn detect_format_of(text: &str) -> Option<String> {
    match detect_format(text).as_str() {
        "unknown" => None,
        other => Some(other.to_string()),
    }
}

/// 改め文の形式を判定する。
fn detect_format(text: &str) -> String {
    let has_after = text.contains("改正後");
    let has_before = text.contains("改正前");
    if has_after && has_before {
        return "shinkyu".to_string();
    }
    if text.contains("改める") || text.contains("次のように改正する") || text.contains("加える")
    {
        return "prose".to_string();
    }
    "unknown".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_vertical_punctuation() {
        let raw = "\u{FE35}施行\u{FE36}\u{FE12}";
        assert_eq!(normalize_text(raw), "（施行）。");
    }

    #[test]
    fn drops_margin_noise_lines() {
        let raw = "本則を次のように改める\n官\n令和 ８ 年 ６ 月 15 日\n第六条";
        let out = normalize_text(raw);
        assert!(out.contains("本則を次のように改める"));
        assert!(out.contains("第六条"));
        assert!(!out.contains("令和"));
    }

    #[test]
    fn detects_shinkyu() {
        assert_eq!(detect_format("改正後 … 改正前 …"), "shinkyu");
        assert_eq!(detect_format("第一条中「甲」を「乙」に改める。"), "prose");
    }

    #[test]
    fn reconstructs_right_to_left_columns() {
        // x が大きい(右)カラムを先に、各カラム内は y 昇順(上→下)で連結する。
        let xhtml = r#"<html><body><doc><page width="595" height="842">
            <word xMin="100" yMin="100" xMax="108" yMax="160">右上</word>
            <word xMin="100" yMin="160" xMax="108" yMax="190">右下</word>
            <word xMin="50" yMin="100" xMax="58" yMax="160">左</word>
        </page></doc></body></html>"#;
        assert_eq!(reconstruct_vertical(xhtml), "右上右下\n左");
    }

    #[test]
    fn splits_two_tiers_then_reads_each_right_to_left() {
        // 上段(y 100-360) と下段(y 420-700) が y のすき間で分かれ、各段を右→左に読む。
        // 同 x(=100) に上段列と下段列があっても連結されないことを確認する。
        let xhtml = r#"<html><body><doc><page width="595" height="842">
            <word xMin="100" yMin="100" xMax="108" yMax="360">上右</word>
            <word xMin="50"  yMin="100" xMax="58"  yMax="360">上左</word>
            <word xMin="100" yMin="420" xMax="108" yMax="700">下右</word>
            <word xMin="50"  yMin="420" xMax="58"  yMax="700">下左</word>
        </page></doc></body></html>"#;
        assert_eq!(reconstruct_vertical(xhtml), "上右\n上左\n下右\n下左");
    }

    #[test]
    fn segments_articles_on_circle_heading() {
        let text = "〇総務省令第七十七号\n電波法の一部を改正する省令\n〇農林水産省告示第七百六十四号\n規格を廃止する件";
        let arts = segment_articles(text);
        assert_eq!(arts.len(), 2);
        assert!(arts[0].contains("電波法"));
        assert!(arts[1].contains("規格を廃止"));
    }
}
