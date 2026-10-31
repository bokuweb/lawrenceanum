//! 画像だけで組まれた本文を、明示的に渡された OCR の認識領域から記事にする（opt-in）。
//!
//! 通常の [`crate::analyze_item`] は画像を認識しない。呼出元が別に実行した OCR（NDLOCR の
//! `recognize-page --trace-json`）の結果を [`OcrPage`] として渡したときだけ、
//! [`analyze_item_with_ocr`] がネイティブの本文に無い領域を読み順に並べて記事に足す。
//! OCR を渡さない（全ページ `None`）場合の結果は [`crate::analyze_item`] と同じ。
//!
//! 読み順は領域の枠だけで決める（XY-cut）。全領域を横切らない縦の隙間があれば段として左から右へ
//! （縦組みの領域だけなら右から左へ）、無ければいちばん幅の広い領域の上下で分ける。認識した字は補正しない。

use crate::{model, segment, Analysis, ArticleAnalysis, PageInput};

/// OCR の認識領域 1 つ。枠は認識画像の画素（左上原点、`[x0, y0, x1, y1]`）。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OcrRegion {
    pub text: String,
    pub bbox: [f64; 4],
    #[serde(default)]
    pub vertical: bool,
}

/// 1 ページ分の OCR 結果。
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OcrPage {
    pub regions: Vec<OcrRegion>,
}

impl OcrPage {
    /// NDLOCR `recognize-page --trace-json` の `stages.reading_order` から領域を読む。
    pub fn from_ndlocr_trace(json: &str) -> anyhow::Result<Self> {
        let v: serde_json::Value = serde_json::from_str(json)?;
        let stage = v["stages"]["reading_order"].as_array().ok_or_else(|| anyhow::anyhow!("trace has no stages.reading_order"))?;
        let mut regions = Vec::with_capacity(stage.len());
        for r in stage {
            let b = r["bbox"].as_array().filter(|b| b.len() == 4).ok_or_else(|| anyhow::anyhow!("region without a 4-number bbox"))?;
            let mut bbox = [0.0; 4];
            for (o, x) in bbox.iter_mut().zip(b) {
                *o = x.as_f64().ok_or_else(|| anyhow::anyhow!("non-numeric bbox"))?;
            }
            regions.push(OcrRegion { text: r["text"].as_str().unwrap_or("").to_string(), bbox, vertical: r["is_vertical"].as_bool().unwrap_or(false) });
        }
        Ok(OcrPage { regions })
    }
}

/// [`crate::analyze_item`] に、ページごとの OCR 結果（`ocr[i]` は `pages[i]`）を足す版。
///
/// ネイティブの記事はそのまま残し、各ページの OCR 領域のうちネイティブの本文に無いものを
/// 読み順に並べて（字全体がネイティブの本文に続けて現れる領域は二重に読んだものとして除く） [`segment::segment_articles`] で分け、ネイティブの記事の後ろへページ順に足す。
pub fn analyze_item_with_ocr(pages: &[PageInput], ocr: &[Option<OcrPage>]) -> Analysis {
    let mut analysis = crate::analyze_item(pages);
    if ocr.iter().all(Option::is_none) {
        return analysis;
    }
    let native: String = compact(&analysis.text).into_iter().collect();
    let mut lines: Vec<String> = Vec::new();
    for page in ocr.iter().flatten() {
        for r in reading_order(&page.regions) {
            let text = r.text.trim();
            if !text.is_empty() && !native.contains(&compact(text).into_iter().collect::<String>()) {
                lines.push(text.to_string());
            }
        }
    }
    // 画像の見出しの丸印（「〇」）は認識されないか、別の領域（「○」）になる。号数の見出しの形の行で記事を分ける。
    let mut chunks: Vec<Vec<String>> = vec![vec![]];
    for line in lines {
        if is_circle_mark(&line) {
            continue;
        }
        if numbered_heading(&line) && !chunks.last().unwrap().is_empty() {
            chunks.push(vec![]);
        }
        chunks.last_mut().unwrap().push(line);
    }
    analysis.articles.extend(
        chunks
            .iter()
            .flat_map(|c| segment::segment_articles(&c.join("\n")))
            .map(|t| ArticleAnalysis { document: model::Document::from_text(&t), text: t, notices: vec![] }),
    );
    analysis
}

fn is_circle_mark(line: &str) -> bool {
    matches!(line.trim(), "〇" | "○" | "◯")
}

/// 号数の見出しだけの行（「法務省告示配第百十号」「〇総務省令第七十七号」）か。丸印の後は漢字で始まり、
/// 句読点を含まず「第…号」で終わる。
fn numbered_heading(line: &str) -> bool {
    const NUMERALS: &str = "〇一二三四五六七八九十百千0123456789０１２３４５６７８９";
    let t = line.trim().trim_start_matches(['〇', '○', '◯']);
    let kanji = |c: char| ('\u{4E00}'..='\u{9FFF}').contains(&c);
    let Some(body) = t.strip_suffix('号') else { return false };
    let Some((head, number)) = body.rsplit_once('第') else { return false };
    head.chars().next().is_some_and(kanji)
        && head.chars().all(kanji)
        && !number.is_empty()
        && number.chars().all(|c| NUMERALS.contains(c))
}

fn compact(s: &str) -> Vec<char> {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// 領域を読み順に並べる（XY-cut）。
pub fn reading_order(regions: &[OcrRegion]) -> Vec<&OcrRegion> {
    let mut out = Vec::with_capacity(regions.len());
    cut(regions.iter().collect(), &mut out);
    out
}

fn cut<'a>(mut set: Vec<&'a OcrRegion>, out: &mut Vec<&'a OcrRegion>) {
    if set.len() <= 1 {
        out.extend(set);
        return;
    }
    // 縦の隙間（どの領域にも覆われない x）で段に分ける。
    set.sort_by(|a, b| a.bbox[0].total_cmp(&b.bbox[0]));
    let mut columns: Vec<Vec<&OcrRegion>> = Vec::new();
    let mut right = f64::NEG_INFINITY;
    for r in set.iter().copied() {
        if r.bbox[0] >= right || columns.is_empty() {
            columns.push(Vec::new());
        }
        right = right.max(r.bbox[2]);
        columns.last_mut().unwrap().push(r);
    }
    if columns.len() > 1 {
        if set.iter().all(|r| r.vertical) {
            columns.reverse();
        }
        for c in columns {
            cut(c, out);
        }
        return;
    }
    // 段を横切る領域（いちばん幅の広いもの）の上・同じ高さ・下に分ける。
    let wide = set.iter().copied().max_by(|a, b| (a.bbox[2] - a.bbox[0]).total_cmp(&(b.bbox[2] - b.bbox[0]))).unwrap();
    let (top, bottom) = (wide.bbox[1], wide.bbox[3]);
    let mid = |r: &OcrRegion| (r.bbox[1] + r.bbox[3]) / 2.0;
    let above: Vec<_> = set.iter().copied().filter(|r| mid(r) < top).collect();
    let below: Vec<_> = set.iter().copied().filter(|r| mid(r) > bottom).collect();
    let row: Vec<_> = set.iter().copied().filter(|r| (top..=bottom).contains(&mid(r))).collect();
    if above.is_empty() && below.is_empty() {
        // 分けられない（同じ高さに並ぶ領域）。縦組みは右から、横組みは上から読む。
        let mut row = row;
        if row.iter().all(|r| r.vertical) {
            row.sort_by(|a, b| b.bbox[2].total_cmp(&a.bbox[2]).then(a.bbox[1].total_cmp(&b.bbox[1])));
        } else {
            row.sort_by(|a, b| a.bbox[1].total_cmp(&b.bbox[1]).then(a.bbox[0].total_cmp(&b.bbox[0])));
        }
        out.extend(row);
        return;
    }
    cut(above, out);
    cut(row, out);
    cut(below, out);
}
