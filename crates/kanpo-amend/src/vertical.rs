//! 縦書き `pdftotext -bbox-layout` 出力から読み順を復元する。

use scraper::{Html, Selector};

use crate::normalize::{is_margin_noise, is_private_use, normalize_char};
use crate::shinkyu::{detect_shinkyu_header, reconstruct_shinkyu};

/// 縦書き1カラム = 1 word とみなした語。`y`/`y1` は上端/下端。
pub(crate) struct Col {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) y1: f32,
    pub(crate) text: String,
}

/// ページ上下の余白帯にある柱（「官」「報」「令和 年 月 日」等）を落とすための閾値(pt)。
const TOP_MARGIN: f32 = 56.0;
const BOTTOM_MARGIN: f32 = 30.0;

/// `-bbox-layout` の XHTML から縦書き読み順（右→左・各カラム上→下）を復元する。
///
/// `pdftotext -bbox-layout` は各縦カラムを 1 つの `<word>`（幅≒1文字・高さ=文字数分、
/// テキストは上→下に整列済み）として出力する。よって **カラムを右→左に並べ替える**
/// だけで正しい読み順が復元でき、`-layout` で起きる「別記事の横方向への混線」を解消できる。
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
/// 再帰的に分割する。これにより本紙の 2 段組（上段/下段が完全に分かれる）と、
/// 新旧対照表（改正後=上段 / 改正前=下段、前文が境界を跨ぐ）の双方を段として
/// 正しく分離できる。
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

/// 1ページ分（または1段分）のカラム語を右→左・上→下に並べて本文に組む。
pub(crate) fn reconstruct_page(mut cols: Vec<Col>) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
