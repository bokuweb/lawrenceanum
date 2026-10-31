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
    /// 認識画像の画素の幅・高さ（ページの表示の向き）。0 なら枠をネイティブの語と比べない。
    #[serde(default)]
    pub width: f64,
    #[serde(default)]
    pub height: f64,
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
        Ok(OcrPage { width: v["width"].as_f64().unwrap_or(0.0), height: v["height"].as_f64().unwrap_or(0.0), regions })
    }
}

/// [`crate::analyze_item`] に、ページごとの OCR 結果（`ocr[i]` は `pages[i]`）を足す版。
///
/// ネイティブの記事はそのまま残し、各ページの OCR 領域のうちネイティブの本文に無いものを
/// 読み順に並べて（枠の過半がネイティブの語の枠に重なる領域と、字全体がネイティブの本文に続けて現れる領域は、
/// ネイティブの字を二重に読んだものとして除く） [`segment::segment_articles`] で分け、ネイティブの記事の後ろへページ順に足す。
pub fn analyze_item_with_ocr(pages: &[PageInput], ocr: &[Option<OcrPage>]) -> Analysis {
    let mut analysis = crate::analyze_item(pages);
    if ocr.iter().all(Option::is_none) {
        return analysis;
    }
    let native: String = compact(&analysis.text).into_iter().collect();
    let mut lines: Vec<String> = Vec::new();
    for (input, page) in pages.iter().zip(ocr).filter_map(|(i, o)| o.as_ref().map(|o| (i, o))) {
        let (words, rules) = native_boxes(input, page);
        let image: Vec<OcrRegion> = page.regions.iter().filter(|r| !covered(r.bbox, &words)).cloned().collect();
        for r in reading_order_with_rules(&image, &rules) {
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

/// ページのネイティブの語の枠と縦罫（`[x, y0, x1=x, y1]`）を、認識画像の画素へ写す。座標は表示の向きの左上原点なので、
/// 縦横の和の比で拡大する。
fn native_boxes(input: &PageInput, page: &OcrPage) -> (Vec<[f64; 4]>, Vec<[f64; 4]>) {
    let attr = |tag: &str, name: &str| -> Option<f64> {
        let at = tag.find(&format!(" {name}=\""))? + name.len() + 3;
        tag[at..].split('"').next()?.parse().ok()
    };
    let x = &input.xhtml;
    let Some(head) = x.find("<page ").map(|at| &x[at..at + x[at..].find('>').unwrap_or(0)]) else { return (vec![], vec![]) };
    let (Some(w), Some(h)) = (attr(head, "width"), attr(head, "height")) else { return (vec![], vec![]) };
    if page.width <= 0.0 || page.height <= 0.0 {
        return (vec![], vec![]);
    }
    let scale = (page.width + page.height) / (w + h);
    let rules = input.rules.iter().flat_map(|r| &r.vertical).map(|v| {
        let x = f64::from(v.x) * scale;
        [x, f64::from(v.y0.min(v.y1)) * scale, x, f64::from(v.y0.max(v.y1)) * scale]
    });
    let words = x.match_indices("<word ")
        .filter_map(|(at, _)| {
            let tag = &x[at..at + x[at..].find('>')?];
            Some([attr(tag, "xMin")? * scale, attr(tag, "yMin")? * scale, attr(tag, "xMax")? * scale, attr(tag, "yMax")? * scale])
        })
        .collect();
    (words, rules.collect())
}

/// 領域の枠の面積の過半が語の枠に覆われるか。
fn covered(b: [f64; 4], words: &[[f64; 4]]) -> bool {
    let area = (b[2] - b[0]) * (b[3] - b[1]);
    let hit: f64 = words.iter().map(|w| (b[2].min(w[2]) - b[0].max(w[0])).max(0.0) * (b[3].min(w[3]) - b[1].max(w[1])).max(0.0)).sum();
    area > 0.0 && 2.0 * hit > area
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

/// 縦罫で段を分けてから読み順に並べる。罫の縦の範囲でどの領域にも横切られない罫だけを段の境にする。
/// 罫の範囲より上の領域を先に、下の領域を後に読む。
pub fn reading_order_with_rules<'a>(regions: &'a [OcrRegion], rules: &[[f64; 4]]) -> Vec<&'a OcrRegion> {
    let overlaps = |r: &OcrRegion, l: &[f64; 4]| r.bbox[1] < l[3] && r.bbox[3] > l[1];
    let mut cuts: Vec<&[f64; 4]> = rules
        .iter()
        .filter(|l| {
            let inside: Vec<&OcrRegion> = regions.iter().filter(|r| overlaps(r, l)).collect();
            inside.iter().any(|r| r.bbox[2] <= l[0])
                && inside.iter().any(|r| r.bbox[0] >= l[0])
                && !inside.iter().any(|r| r.bbox[0] < l[0] && r.bbox[2] > l[0])
        })
        .collect();
    if cuts.is_empty() {
        return reading_order(regions);
    }
    cuts.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let top = cuts.iter().map(|l| l[1]).fold(f64::INFINITY, f64::min);
    let bottom = cuts.iter().map(|l| l[3]).fold(f64::NEG_INFINITY, f64::max);
    let mid = |r: &OcrRegion| (r.bbox[1] + r.bbox[3]) / 2.0;
    let mut out = Vec::with_capacity(regions.len());
    cut(regions.iter().filter(|r| mid(r) < top).collect(), &mut out);
    let band: Vec<&OcrRegion> = regions.iter().filter(|r| (top..=bottom).contains(&mid(r))).collect();
    let mut columns: Vec<Vec<&OcrRegion>> = vec![vec![]; cuts.len() + 1];
    for r in band {
        let cx = (r.bbox[0] + r.bbox[2]) / 2.0;
        columns[cuts.iter().filter(|l| l[0] < cx).count()].push(r);
    }
    if columns.iter().flatten().all(|r| r.vertical) {
        columns.reverse();
    }
    for c in columns {
        cut(c, &mut out);
    }
    cut(regions.iter().filter(|r| mid(r) > bottom).collect(), &mut out);
    out
}

fn cut<'a>(set: Vec<&'a OcrRegion>, out: &mut Vec<&'a OcrRegion>) {
    if set.len() <= 1 {
        out.extend(set);
        return;
    }
    // 縦組みが多い集まりは、段（上下の隙間）を先に分け、段の中の行を右から読む。横組みは縦の隙間の段を左から読む。
    let vertical = 2 * set.iter().filter(|r| r.vertical).count() > set.len();
    if vertical {
        let tiers = gaps(&set, 1);
        if tiers.len() > 1 {
            tiers.into_iter().for_each(|t| cut(t, out));
            return;
        }
    }
    let mut columns = gaps(&set, 0);
    if columns.len() > 1 {
        if vertical {
            columns.reverse();
        }
        columns.into_iter().for_each(|c| cut(c, out));
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
        if vertical {
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

/// 軸（0 = x、1 = y）の方向に、どの領域の枠にも覆われない隙間で分けた集まり（座標の小さい順）。
fn gaps<'a>(set: &[&'a OcrRegion], axis: usize) -> Vec<Vec<&'a OcrRegion>> {
    let mut sorted = set.to_vec();
    sorted.sort_by(|a, b| a.bbox[axis].total_cmp(&b.bbox[axis]));
    let mut groups: Vec<Vec<&OcrRegion>> = Vec::new();
    let mut end = f64::NEG_INFINITY;
    for r in sorted {
        if groups.is_empty() || r.bbox[axis] >= end {
            groups.push(Vec::new());
        }
        end = end.max(r.bbox[axis + 2]);
        groups.last_mut().unwrap().push(r);
    }
    groups
}
