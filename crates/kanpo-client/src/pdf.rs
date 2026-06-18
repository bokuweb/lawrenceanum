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
            // scraper が実体参照は復号済み。約物を正規化し、私用領域(傍線等の
            // フォント固有グリフ)を除去する。
            let text: String = w
                .text()
                .collect::<String>()
                .chars()
                .filter(|c| !is_private_use(*c))
                .map(normalize_char)
                .collect();
            if text.trim().is_empty() {
                continue;
            }
            cols.push(Col { x, y, y1, text });
        }
        // まず段組み(tier)に分ける。官報本紙は2段組が多く、段を分けないと「同 x にある
        // 上段の列と下段の列」が1列に連結され混線する。表と無関係記事が同居するページも
        // tier で分かれるので、混入を防げる。
        for tier in split_tiers(cols) {
            // 新旧対照表の段は「改正後=上帯 / 改正前=下帯」を縦の罫線(描画)ではなく y 座標の
            // 帯で表す。改正後／改正前の欄見出し列を検出できたら、その x を表の右端、見出し
            // 中心の中点を上下帯の境界として 2 帯に分けて復元する。前文カラムが境界を跨いでも
            // 改正後/改正前が混線しない。見出しが無ければ通常の右→左復元にフォールバック。
            let page_text = match detect_shinkyu_header(&tier) {
                Some((header_x, divider)) => reconstruct_shinkyu(tier, header_x, divider),
                None => reconstruct_page(tier),
            };
            if !page_text.is_empty() {
                pages.push(page_text);
            }
        }
    }
    pages.join("\n").trim().to_string()
}

/// カラム語を縦方向の段(tier)に分割する。
///
/// 各段は独立した右→左の縦フロー。段の境目は「跨ぐカラムがほとんど無い y の横線」
/// として現れる。完全な空白だけでなく、わずかな橋渡しカラム（新旧対照表で本文の
/// 右側に伸びる前文カラム等）を許容して谷を検出するため、横断カラム数の極小点で
/// 再帰的に分割する。これにより
///   - 本紙の2段組（上段/下段が完全に分かれる）
///   - 新旧対照表（改正後=上段 / 改正前=下段、前文が境界を跨ぐ）
/// の双方を段として正しく分離できる。
fn split_tiers(cols: Vec<Col>) -> Vec<Vec<Col>> {
    let mut out = Vec::new();
    split_tiers_rec(cols, 0, &mut out);
    out
}

/// 跨ぎカラム数の許容上限（前文 1 本程度の橋渡しを段境界として許す）。
const MAX_BRIDGE: usize = 1;

fn split_tiers_rec(cols: Vec<Col>, depth: usize, out: &mut Vec<Vec<Col>>) {
    // 過分割の防止: 段が小さい / 深すぎる場合はこれ以上分けない。
    if cols.len() < 4 || depth >= 6 {
        out.push(cols);
        return;
    }
    let ymin = cols.iter().map(|c| c.y).fold(f32::INFINITY, f32::min);
    let ymax = cols.iter().map(|c| c.y1).fold(f32::NEG_INFINITY, f32::max);
    let margin = (ymax - ymin) * 0.2;
    let (lo, hi) = (ymin + margin, ymax - margin);

    // 中央域で「跨ぐカラム数」が最小の cut を探す。候補はカラムの下端(y1)。
    let mut best: Option<(usize, f32)> = None;
    for c in &cols {
        let cut = c.y1 + 0.5;
        if cut < lo || cut > hi {
            continue;
        }
        let crossing = cols.iter().filter(|d| d.y < cut && cut < d.y1).count();
        if best.map(|b| crossing < b.0).unwrap_or(true) {
            best = Some((crossing, cut));
        }
    }

    let Some((crossing, cut)) = best else {
        out.push(cols);
        return;
    };
    if crossing > MAX_BRIDGE {
        out.push(cols);
        return;
    }
    // cut で上下に分割（橋渡しカラムは中心 y が属する側へ）。
    let (mut top, mut bottom): (Vec<Col>, Vec<Col>) = (Vec::new(), Vec::new());
    for c in cols {
        if (c.y + c.y1) / 2.0 < cut {
            top.push(c);
        } else {
            bottom.push(c);
        }
    }
    if top.is_empty() || bottom.is_empty() {
        out.push(top.into_iter().chain(bottom).collect());
        return;
    }
    split_tiers_rec(top, depth + 1, out);
    split_tiers_rec(bottom, depth + 1, out);
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

/// 縦書き新旧対照表ページの「改正後／改正前」欄見出し列を検出する。
///
/// 官報の縦書き新旧対照表は、表の右端に「改正後」(上) と「改正前」(下) のラベルが
/// 同一 x 上に縦に並ぶ。x 近傍でカラムをまとめ、その縦連結に両ラベルが現れる列を
/// 見出し列とみなし、`(見出し x, 上下帯の境界 y)` を返す。境界は両ラベルの中心の中点。
/// 見つからなければ `None`（＝通常ページ）。
fn detect_shinkyu_header(cols: &[Col]) -> Option<(f32, f32)> {
    // x 近傍(±6pt)でグループ化。
    let mut idx: Vec<usize> = (0..cols.len()).collect();
    idx.sort_by(|&a, &b| cols[a].x.partial_cmp(&cols[b].x).unwrap_or(std::cmp::Ordering::Equal));
    let mut group: Vec<usize> = Vec::new();
    let mut rep = f32::NEG_INFINITY;
    let flush_and_check = |g: &mut Vec<usize>| -> Option<(f32, f32)> {
        if g.len() < 3 {
            g.clear();
            return None;
        }
        g.sort_by(|&a, &b| cols[a].y.partial_cmp(&cols[b].y).unwrap_or(std::cmp::Ordering::Equal));
        let chars: Vec<char> = g.iter().flat_map(|&i| cols[i].text.chars()).collect();
        // 各文字の y 中心（cols[i] が複数文字なら全文字に同じ中心を割当: 見出しは1文字列なので十分）。
        let centers: Vec<f32> = g
            .iter()
            .flat_map(|&i| {
                let c = (cols[i].y + cols[i].y1) / 2.0;
                cols[i].text.chars().map(move |_| c)
            })
            .collect();
        let header_x = g.iter().map(|&i| cols[i].x).fold(f32::INFINITY, f32::min);
        let go = find_run(&chars, &['改', '正', '後']);
        let mae = find_run(&chars, &['改', '正', '前']);
        g.clear();
        match (go, mae) {
            (Some(gi), Some(mi)) if gi != mi => {
                let go_c = (centers[gi] + centers[gi + 1] + centers[gi + 2]) / 3.0;
                let mae_c = (centers[mi] + centers[mi + 1] + centers[mi + 2]) / 3.0;
                Some((header_x, (go_c + mae_c) / 2.0))
            }
            _ => None,
        }
    };
    for &i in &idx {
        if !group.is_empty() && cols[i].x - rep > 6.0 {
            if let Some(found) = flush_and_check(&mut group) {
                return Some(found);
            }
        }
        if group.is_empty() {
            rep = cols[i].x;
        }
        group.push(i);
    }
    flush_and_check(&mut group)
}

/// `chars` 中で連続部分列 `pat` が最初に現れる開始位置を返す。
fn find_run(chars: &[char], pat: &[char]) -> Option<usize> {
    if pat.is_empty() || chars.len() < pat.len() {
        return None;
    }
    (0..=chars.len() - pat.len()).find(|&i| &chars[i..i + pat.len()] == pat)
}

/// 新旧対照表ページを改正後(上帯)/改正前(下帯)に分けて復元する。
///
/// 見出し列 x より右(=`x >= header_x`)は前文・標題とみなし先頭にそのまま置く。
/// 表本体(`x < header_x`)を境界 y で上下に分け、各帯を右→左に復元して
/// `改正後`／`改正前` の欄見出し行を冠して返す。見出し行はフロントの新旧対照表
/// 組み(parseShinkyu)と `detect_format` の shinkyu 判定の双方に整合する。
fn reconstruct_shinkyu(cols: Vec<Col>, header_x: f32, divider: f32) -> String {
    let mut preamble: Vec<Col> = Vec::new();
    let mut upper: Vec<Col> = Vec::new();
    let mut lower_all: Vec<Col> = Vec::new();
    for c in cols {
        if c.x >= header_x - 1.0 {
            // 見出しラベル自身(改/正/後/前)は捨て、それ以外(前文・標題)は前文へ。
            if matches!(c.text.as_str(), "改" | "正" | "後" | "前") && (c.x - header_x).abs() < 6.0 {
                continue;
            }
            preamble.push(c);
        } else if (c.y + c.y1) / 2.0 < divider {
            upper.push(c);
        } else {
            lower_all.push(c);
        }
    }
    // 新旧対照表は境界 y を挟んでほぼ上下対称（改正後欄の高さ ≒ 改正前欄の高さ）。
    // 改正前(下帯)が表の下端を越えて広がる場合、それは表の下に同居する別記事
    // （型式認定一覧など）の可能性が高い。上帯の最上端を表頂点とみなし、その鏡像
    // (`2*divider - table_top`) を下端として下帯を制限し、表外の記事の取り込みを防ぐ。
    // 表が紙面全体を占める通常ケースでは下端がページ外になり実質無効（＝無害）。
    let lower: Vec<Col> = if let Some(table_top) = upper
        .iter()
        .map(|c| c.y)
        .fold(None, |acc: Option<f32>, y| Some(acc.map_or(y, |a| a.min(y))))
    {
        let table_bottom = 2.0 * divider - table_top;
        lower_all
            .into_iter()
            .filter(|c| (c.y + c.y1) / 2.0 < table_bottom)
            .collect()
    } else {
        lower_all
    };
    let mut out: Vec<String> = Vec::new();
    let pre = reconstruct_page(preamble);
    if !pre.is_empty() {
        out.push(pre);
    }
    let after = reconstruct_page(upper);
    let before = reconstruct_page(lower);
    if !after.is_empty() || !before.is_empty() {
        out.push("改正後".to_string());
        out.push(after);
        out.push("改正前".to_string());
        out.push(before);
    }
    out.join("\n").trim().to_string()
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
/// 1ページに複数の法令/告示が詰まることがあるので、記事の先頭マーカで区切る。
/// - 省令・告示・規則: `〇` 見出し（例: 「〇総務省令第七十七号」）。
/// - 法律・政令の公布: 「{件名}をここに公布する。」で始まるブロック。
///   （これが無いと複数の政令が1記事に混ざり、best_segment が誤った巨大ブロックを返す。）
pub fn segment_articles(text: &str) -> Vec<String> {
    let mut articles: Vec<String> = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    for line in text.lines() {
        if is_article_boundary(line) && !cur.is_empty() {
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

/// その行が新しい記事の先頭か（省令/告示の `〇` 見出し、または法律/政令の公布行）。
fn is_article_boundary(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with('〇') || t.contains("をここに公布する")
}

/// 縦書き約物の正規化 + ページ柱ノイズ除去。
pub fn normalize_text(raw: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for line in raw.lines() {
        let line: String = line
            .chars()
            .filter(|c| !is_private_use(*c))
            .map(normalize_char)
            .collect();
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

/// 私用領域(PUA)の文字か。官報 PDF は傍線・下線などをフォント固有グリフ(U+E000–
/// U+F8FF)で埋め込むことがあり、テキストとしては意味を持たないため除去する。
fn is_private_use(c: char) -> bool {
    ('\u{E000}'..='\u{F8FF}').contains(&c)
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
///
/// 新旧対照表(shinkyu)は「改正後」「改正前」が**欄見出し（独立行）**として現れるもの
/// だけに限定する。単なる本文中の部分一致（廃止文や隣接記事の混入で出てくる「改正後／
/// 改正前」）を新旧対照表と誤判定しないため。これによりフロントの表組み可否（独立見出し
/// 行を要求する parseShinkyu）と判定が一致する。
fn detect_format(text: &str) -> String {
    let has_header = |label: &str| text.lines().any(|l| l.trim() == label);
    if has_header("改正後") && has_header("改正前") {
        return "shinkyu".to_string();
    }
    if text.contains("に改める")
        || text.contains("次のように改正する")
        || text.contains("を加える")
        || text.contains("を削る")
        || text.contains("廃止する")
        || text.contains("定める")
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
    fn reconstructs_vertical_shinkyu_into_two_bands() {
        // 縦書き新旧対照表の最小ケース: x=150 に「改正後」(上) /「改正前」(下) の
        // 欄見出し列、その左 x=120 に各帯の本文セル(改正後=甲種 / 改正前=乙類)。
        // 見出し列より右(x=200,210)に上下帯を跨ぐ前文カラムを置くことで、実ページ同様に
        // 段分割されず 1 段にまとまり、見出しの y 帯で改正後/改正前に分けて復元される。
        let xhtml = r#"<page width="300" height="400">
          <word xMin="200" yMin="65" xMax="210" yMax="300">前文左</word>
          <word xMin="210" yMin="65" xMax="220" yMax="300">前文右</word>
          <word xMin="150" yMin="70" xMax="160" yMax="80">改</word>
          <word xMin="150" yMin="90" xMax="160" yMax="100">正</word>
          <word xMin="150" yMin="110" xMax="160" yMax="120">後</word>
          <word xMin="150" yMin="250" xMax="160" yMax="260">改</word>
          <word xMin="150" yMin="270" xMax="160" yMax="280">正</word>
          <word xMin="150" yMin="290" xMax="160" yMax="300">前</word>
          <word xMin="120" yMin="70" xMax="130" yMax="110">甲種</word>
          <word xMin="120" yMin="250" xMax="130" yMax="290">乙類</word>
        </page>"#;
        let out = reconstruct_vertical(xhtml);
        // 欄見出し行が独立して現れ、各帯の本文が正しい側に入る。
        assert!(out.lines().any(|l| l.trim() == "改正後"), "out=\n{out}");
        assert!(out.lines().any(|l| l.trim() == "改正前"), "out=\n{out}");
        let after_idx = out.find("改正後").unwrap();
        let before_idx = out.find("改正前").unwrap();
        let kou = out.find("甲種").unwrap();
        let otsu = out.find("乙類").unwrap();
        assert!(after_idx < kou && kou < before_idx, "甲種は改正後側: {out}");
        assert!(before_idx < otsu, "乙類は改正前側: {out}");
        // 欄見出しが揃うので shinkyu と判定される。
        assert_eq!(detect_format(&out), "shinkyu");
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
    fn strips_private_use_glyphs() {
        // U+E0A8 等の PUA(傍線グリフ)は除去される。
        let raw = "本文\u{E0A8}\u{E0A8}\u{E0A8}\n令和八年";
        let out = normalize_text(raw);
        assert!(!out.contains('\u{E0A8}'));
        assert!(out.contains("本文"));
        assert!(out.contains("令和八年"));
    }

    #[test]
    fn detects_shinkyu() {
        // 「改正後」「改正前」が独立見出し行のときだけ shinkyu。
        assert_eq!(detect_format("改正後\n（見出し）\n改正前\n（見出し）"), "shinkyu");
        assert_eq!(detect_format("第一条中「甲」を「乙」に改める。"), "prose");
    }

    #[test]
    fn substring_kaiseigo_is_not_shinkyu() {
        // 本文中に部分一致で「改正後／改正前」が出ても（廃止文・隣接記事の混入など）
        // 独立見出し行でなければ新旧対照表とはしない。
        let repeal = "次に掲げる府令は、廃止する。\n改正後の規定は…改正前の例による。";
        assert_eq!(detect_format(repeal), "prose");
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
    fn splits_tiers_tolerating_one_bridging_column() {
        // 新旧対照表: 上段(改正後) / 下段(改正前) を、境界を跨ぐ前文カラム1本があっても
        // 段として分離する（横断カラム数の谷で分割）。
        let xhtml = r#"<html><body><doc><page width="595" height="842">
            <word xMin="100" yMin="100" xMax="108" yMax="360">後右</word>
            <word xMin="50"  yMin="100" xMax="58"  yMax="360">後左</word>
            <word xMin="100" yMin="420" xMax="108" yMax="700">前右</word>
            <word xMin="50"  yMin="420" xMax="58"  yMax="700">前左</word>
            <word xMin="200" yMin="100" xMax="208" yMax="700">前文</word>
        </page></doc></body></html>"#;
        // 前文(橋渡し)は中心 y が下段側なので下段の先頭(最も右)に来る。
        assert_eq!(reconstruct_vertical(xhtml), "後右\n後左\n前文\n前右\n前左");
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

    #[test]
    fn segments_articles_on_promulgation_boundary() {
        // 同一ページに複数の政令公布が並ぶケース。「をここに公布する」で記事を分ける。
        let text = "労働組合法施行令の一部を改正する政令をここに公布する。\n御名御璽\n労働組合法施行令の一部を次のように改正する。\n美容師法施行令の一部を改正する政令をここに公布する。\n御名御璽\n美容師法施行令の一部を次のように改正する。";
        let arts = segment_articles(text);
        assert_eq!(arts.len(), 2);
        assert!(arts[0].contains("労働組合法施行令") && !arts[0].contains("美容師"));
        assert!(arts[1].contains("美容師法施行令") && !arts[1].contains("労働組合"));
    }
}
