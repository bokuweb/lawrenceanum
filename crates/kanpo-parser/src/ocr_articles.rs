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
/// 読み順に並べて（枠の過半がネイティブの語の枠に重なる領域は、ネイティブの字を二重に読んだものとして除く。字では比べない:
/// 見出しの語（「行旅死亡人」）は項目の別のページのネイティブの本文にも現れる） [`segment::segment_articles`] で分け、ネイティブの記事の後ろへページ順に足す。
pub fn analyze_item_with_ocr(pages: &[PageInput], ocr: &[Option<OcrPage>]) -> Analysis {
    let mut analysis = crate::analyze_item(pages);
    if ocr.iter().all(Option::is_none) {
        return analysis;
    }
    let mut lines: Vec<String> = Vec::new();
    for (input, page) in pages.iter().zip(ocr).filter_map(|(i, o)| o.as_ref().map(|o| (i, o))) {
        let (words, rules) = native_boxes(input, page);
        let image: Vec<OcrRegion> = page.regions.iter().filter(|r| !covered(r.bbox, &words)).cloned().collect();
        for r in reading_order_with_rules(&image, &rules) {
            let text = r.text.trim();
            if !text.is_empty() {
                lines.push(text.to_string());
            }
        }
    }
    // 画像の見出しの丸印（「〇」）は認識されないか、別の領域（「○」）になる。号数の見出しの形の行で記事を分ける。
    // 同じ標題の公告が続く（「教育職員免許状失効公告」が 2 件）とき、標題の行は文でない同じ行として繰り返す。最初の行で分けると、
    // 欄の記事がその標題から始まる。
    let titled = |l: &str| l.chars().count() >= 2 && !l.contains(['。', '、']) && !l.chars().all(|c| c.is_ascii_digit());
    let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for l in lines.iter().filter(|l| titled(l)) {
        *counts.entry(l.as_str()).or_default() += 1;
    }
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let repeated: Vec<bool> = lines.iter().map(|l| counts.get(l.as_str()).is_some_and(|&n| n > 1) && seen.insert(l.clone())).collect();
    let mut chunks: Vec<Vec<String>> = vec![vec![]];
    for (line, first_repeat) in lines.into_iter().zip(repeated) {
        if is_circle_mark(&line) {
            continue;
        }
        // 号数の見出しで始まった告示の中の繰り返す行（帰化の「住所東京都新宿区」）では分けない。
        let in_numbered = chunks.last().unwrap().first().is_some_and(|l| numbered_heading(l));
        if (numbered_heading(&line) || (first_repeat && !in_numbered)) && !chunks.last().unwrap().is_empty() {
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
    // 号数の字が認識されず、行が「愛知県公安委員会告示配第」で切れることがある。
    if let Some(head) = t.strip_suffix('第') {
        return head.chars().count() >= 4 && head.chars().all(kanji);
    }
    let Some(body) = t.strip_suffix('号') else { return false };
    let Some((head, number)) = body.rsplit_once('第') else { return false };
    // 機関名は 2 字以上（「告第13号」は前の段の「…告示第13号」の切れ端）。
    head.chars().count() >= 2
        && head.chars().all(kanji)
        && !number.is_empty()
        && number.chars().all(|c| NUMERALS.contains(c))
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

/// [`crate::pick_items`] に、OCR の字の誤り（「無縁墳墓」を「無縁填墓」、「告示配」を「告示記」）を 1 字まで許す選び方を足す版（opt-in の OCR の経路だけで使う）。
///
/// 「…関係」の項目で記事を選べなかったとき、目次の語（[`crate::listed_words`] の順）と同じ長さの行頭が 1 字だけ違う、句点の無い行を
/// 持つ記事、または語で終わる短い見出しの行（目次「財団関係」・本文「工場財団」。行は語の 2 倍の字数まで）を持つ記事がちょうど
/// 1 つなら、それを採る。その行が記事の途中なら、その行から後ろを別の記事にして（末尾に足す）採る。
/// 1 字違いを許すのは 4 字以上の語だけ（短い語は別の行の頭と紛れる）。
pub fn pick_items_with_ocr(titles: &[&str], articles: &mut Vec<ArticleAnalysis>) -> Vec<Vec<usize>> {
    let mut picks = crate::pick_items(titles, articles);
    // 標題の番号（「（法務省告示配一〇九）」）の記事を選べていないとき、見出しの号数が合い、機関名の字が 1 字の誤り
    // （「法務省告示記第百九号」）を除いて見出しに順に現れる記事がちょうど 1 つなら、それを採る。
    for (title, pick) in titles.iter().zip(picks.iter_mut()) {
        let Some((agencies, n)) = crate::title_number(title) else { continue };
        if agencies.is_empty() || pick.first().is_some_and(|&i| crate::has_number(&articles[i], &agencies, n)) {
            continue;
        }
        let near = |a: &ArticleAnalysis| {
            let head = a.text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
            if crate::heading_number(head) != Some(n) {
                return false;
            }
            let name: Vec<char> = head.split('第').next().unwrap_or("").chars().collect();
            let mut misses = 0;
            for ag in &agencies {
                let mut at = 0;
                for c in ag.chars() {
                    match name[at..].iter().position(|&h| h == c) {
                        Some(k) => at += k + 1,
                        None => misses += 1,
                    }
                }
            }
            misses == 1
        };
        if let [i] = articles.iter().enumerate().filter(|(_, a)| near(a)).map(|(i, _)| i).collect::<Vec<_>>()[..] {
            *pick = vec![i];
        }
    }
    // 「…関係」の項目で、標題の鍵で選んだ記事が目次の最初の語で始まらず、その語で始まる文でない行が全体でちょうど 1 つあるなら、
    // その行（画像の欄の小見出し「旅行業者営業保証金の権利実行申立てに係る公告」）からの記事を採る（記事の途中なら分ける）。
    for (title, pick) in titles.iter().zip(picks.iter_mut()).filter(|(_, p)| !p.is_empty()) {
        let Some(word) = crate::listed_words(title).into_iter().next() else { continue };
        // 小見出しは目次の語の頭の機関名を省くことがある（目次「日本弁護士連合会裁決取消訴訟の判決確定」、本文「裁決取消訴訟の
        // 判決確定の公告」）ので、語の後ろの半分以上で始まる行も採る。
        let chars: Vec<char> = word.chars().collect();
        let tails: Vec<String> = (0..=chars.len() / 2).filter(|&k| k == 0 || chars.len() - k >= 4).map(|k| chars[k..].iter().collect()).collect();
        let starts = |line: &str| {
            let line: String = line.chars().filter(|c| !c.is_whitespace()).collect();
            !line.contains(['。', '、']) && tails.iter().any(|t| line.starts_with(t.as_str()))
        };
        let head = |l: &str| {
            let l: String = l.chars().filter(|c| !c.is_whitespace()).collect();
            l.starts_with(&word)
        };
        if articles[pick[0]].text.lines().find(|l| !l.trim().is_empty()).is_some_and(head) {
            continue;
        }
        let hits: Vec<(usize, usize)> =
            articles.iter().enumerate().flat_map(|(i, a)| a.text.split('\n').enumerate().filter(|(_, l)| starts(l)).map(move |(j, _)| (i, j))).collect();
        if let [(i, j)] = hits[..] {
            *pick = vec![split_at_line(articles, i, j)];
        }
    }
    for (title, pick) in titles.iter().zip(picks.iter_mut()).filter(|(_, p)| p.is_empty()) {
        for word in crate::listed_words(title) {
            let word: Vec<char> = word.chars().collect();
            let near = |line: &str| {
                let line: Vec<char> = line.chars().filter(|c| !c.is_whitespace()).collect();
                if line.contains(&'。') || line.contains(&'、') {
                    return false;
                }
                let head = &line[..word.len().min(line.len())];
                (word.len() >= 4 && head.len() == word.len() && head.iter().zip(&word).filter(|(a, b)| a != b).count() <= 1)
                    || (word.len() >= 2 && line.len() <= 2 * word.len() && line.ends_with(&word))
            };
            let hits: Vec<(usize, usize)> =
                articles.iter().enumerate().filter_map(|(i, a)| a.text.split('\n').position(near).map(|j| (i, j))).collect();
            if let [(i, j)] = hits[..] {
                *pick = vec![split_at_line(articles, i, j)];
            }
            if !hits.is_empty() {
                break;
            }
        }
    }
    picks
}

/// 記事 `i` を `j` 行目から分け、後ろを末尾の記事にしてその番号を返す（`j` が 0 なら分けずに `i`）。
fn split_at_line(articles: &mut Vec<ArticleAnalysis>, i: usize, j: usize) -> usize {
    if j == 0 {
        return i;
    }
    let lines: Vec<&str> = articles[i].text.split('\n').collect();
    let (before, after) = (lines[..j].join("\n"), lines[j..].join("\n"));
    articles[i] = ArticleAnalysis { document: model::Document::from_text(&before), text: before, notices: vec![] };
    articles.push(ArticleAnalysis { document: model::Document::from_text(&after), text: after, notices: vec![] });
    articles.len() - 1
}
