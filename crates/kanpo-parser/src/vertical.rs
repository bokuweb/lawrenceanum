//! 縦書き `pdftotext -bbox-layout` 出力から読み順を復元する。

use scraper::{Html, Selector};

use crate::normalize::{fix_private_cid_glyphs, is_margin_noise, is_private_use, normalize_char};
use crate::shinkyu::{detect_shinkyu_bands_headerless, detect_shinkyu_header, reconstruct_shinkyu, ShinkyuLayout};
use crate::trace::{join_lines, Line, PageTrace, Unit, Word, WordStatus};

/// 縦書き1カラム = 1 word とみなした語。`y`/`y1` は上端/下端。
#[derive(Clone)]
pub(crate) struct Col {
    /// ページ内の語 ID（[`Word::id`]）。
    pub(crate) id: usize,
    pub(crate) x: f32,
    pub(crate) x1: f32,
    pub(crate) y: f32,
    pub(crate) y1: f32,
    pub(crate) text: String,
}

/// ページ上下の余白帯にある柱（「官」「報」「令和 年 月 日」等）を落とすための閾値(pt)。
pub(crate) const TOP_MARGIN: f32 = 56.0;
pub(crate) const BOTTOM_MARGIN: f32 = 30.0;

/// `-bbox-layout` の XHTML から縦書き読み順（右→左・各カラム上→下）を復元する。
///
/// `pdftotext -bbox-layout` は各縦カラムを 1 つの `<word>`（幅≒1文字・高さ=文字数分、
/// テキストは上→下に整列済み）として出力する。よって **カラムを右→左に並べ替える**
/// だけで正しい読み順が復元でき、`-layout` で起きる「別記事の横方向への混線」を解消できる。
/// 複数ページの PDF はページごとに復元して連結する。
pub fn reconstruct_vertical(xhtml: &str) -> String {
    analyze_pages(xhtml)
        .iter()
        .map(|p| p.text.as_str())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// ページごとに読み順を復元し、各語の割り当て（どの領域のどの行か）も返す。
pub(crate) fn analyze_pages(xhtml: &str) -> Vec<PageTrace> {
    let doc = Html::parse_document(xhtml);
    let page_sel = Selector::parse("page").unwrap();

    let mut pages: Vec<PageTrace> = Vec::new();
    for page in doc.select(&page_sel) {
        let dim = |k: &str, default: f32| page.value().attr(k).and_then(|s| s.parse::<f32>().ok()).unwrap_or(default);
        let (width, height) = (dim("width", 595.0), dim("height", 842.0));
        let (cols, mut words) = page_words_traced(page, height);
        let mut layout = "tiers";
        let mut units: Vec<Unit> = Vec::new();
        // 多ページ新旧対照表の継続ページ（2 ページ目以降）は「改正後/改正前」の欄見出しを
        // 繰り返さない。ページ全体に見出しが無く、上下半がほぼ同一内容（改正後 ≒ 改正前）なら、
        // 継続ページとみなして上下帯に分け、改正後/改正前の見出しを補って復元する。
        // ※ 見出しのあるページ（先頭ページ）や、見出しはないが別記事が同居するページ
        //   （例: 郵便+型式認定）は段(tier)単位で扱う必要があるため、ここでは除外する。
        let headerless = detect_shinkyu_header(&cols)
            .is_none()
            .then(|| detect_shinkyu_bands_headerless(&cols, height))
            .flatten();
        if let Some(divider) = headerless {
            layout = "headerless_shinkyu";
            units.push(reconstruct_shinkyu(cols, f32::MAX, divider));
        } else {
            // 通常ページ: まず段組み(tier)に分ける。官報本紙は2段組が多く、段を分けないと
            // 「同 x にある上段の列と下段の列」が1列に連結され混線する。新旧対照表の見出しが
            // ある段は 2 帯へ、無ければ右→左復元する。表と無関係記事が同居するページも
            // tier で分かれ混入を防げる。見出しのある段に続く段の組（表がページ下半へ続く形）は、
            // 表の続きなら取り込む。
            let mut tiers = split_tiers(cols);
            let mut i = 0;
            while i < tiers.len() {
                let tier = std::mem::take(&mut tiers[i]);
                i += 1;
                let unit = match detect_shinkyu_header(&tier) {
                    Some((header_x, divider)) => {
                        let mut layout = ShinkyuLayout::split(tier, header_x, divider);
                        while i + 1 < tiers.len()
                            && detect_shinkyu_header(&tiers[i]).is_none()
                            && detect_shinkyu_header(&tiers[i + 1]).is_none()
                        {
                            let (head, tail) = tiers.split_at_mut(i + 1);
                            let Some(more) = layout.absorb_continuation(&mut head[i], &mut tail[0]) else {
                                break;
                            };
                            i += 2;
                            if !more {
                                break;
                            }
                        }
                        layout.into_unit()
                    }
                    None => Unit::Plain(page_lines(tier)),
                };
                units.push(unit);
            }
        }
        let text = units
            .iter()
            .map(Unit::text)
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        let regions: Vec<_> = units.into_iter().enumerate().flat_map(|(i, u)| u.regions(i)).collect();
        // 行に組まれなかった語は、欄見出しのラベルか柱の行として捨てたもの。
        let used: std::collections::HashSet<usize> =
            regions.iter().flat_map(|r| r.lines.iter().flat_map(|l| l.words.iter().copied())).collect();
        for w in words.iter_mut().filter(|w| w.status == WordStatus::Used && !used.contains(&w.id)) {
            w.status = if matches!(w.text.as_str(), "改" | "正" | "後" | "前") {
                WordStatus::HeaderLabel
            } else {
                WordStatus::Noise
            };
        }
        pages.push(PageTrace {
            width,
            height,
            layout,
            words,
            regions,
            text,
        });
    }
    pages
}

/// `-bbox-layout` の XHTML をページ単位に解析し、`(ページ高さ, 余白除去済みカラム列)` を返す。
/// 別表復元（罫線とテキストの突合）で文字座標が必要なときに使う。
pub(crate) fn parse_page_cols(xhtml: &str) -> Vec<(f32, Vec<Col>)> {
    let doc = Html::parse_document(xhtml);
    let page_sel = Selector::parse("page").unwrap();
    let mut out = Vec::new();
    for page in doc.select(&page_sel) {
        let height = page
            .value()
            .attr("height")
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(842.0);
        let cols = page_words(page, height);
        out.push((height, cols));
    }
    out
}

fn page_words(page: scraper::ElementRef, height: f32) -> Vec<Col> {
    page_words_traced(page, height).0
}

/// 1 ページの `<word>` を縦書きカラム語として読み出す。可視化用に全語の一覧も返す
/// （カラム語の `id` はその添字）。
///
/// 余白帯の柱（発行日・「官報」等の running header）は **語全体が余白に収まるもの**だけ
/// 落とす。縦書き語の bbox は実際の字面より約 1em 上にずれて出るため、段の先頭列
/// （例: 「〇総務省令第五十八号」）は上端 y が余白帯にかかる。上端だけで判定すると
/// 記事見出しや前文の列がまるごと消える。
fn page_words_traced(page: scraper::ElementRef, height: f32) -> (Vec<Col>, Vec<Word>) {
    let word_sel = Selector::parse("word").unwrap();
    let mut cols: Vec<Col> = Vec::new();
    let mut words: Vec<Word> = Vec::new();
    for w in page.select(&word_sel) {
        let v = w.value();
        // scraper は HTML パーサのため属性名を小文字化する（xMin -> xmin）。
        let num = |k: &str| v.attr(k).and_then(|s| s.parse::<f32>().ok());
        let (Some(x), Some(x1), Some(y), Some(y1)) = (num("xmin"), num("xmax"), num("ymin"), num("ymax")) else {
            continue;
        };
        let raw: String = w.text().collect();
        // scraper が実体参照は復号済み。私用 CID の括弧付き数字等を復元し、約物を正規化し、
        // 私用領域(傍線等のフォント固有グリフ)を除去する。
        let text: String = match fix_private_cid_glyphs(&raw, x1 - x, y1 - y) {
            Some(fixed) => fixed,
            None => raw.chars().filter(|c| !is_private_use(*c)).map(normalize_char).collect(),
        };
        let status = if y1 <= TOP_MARGIN || y >= height - BOTTOM_MARGIN {
            WordStatus::Margin
        } else if text.trim().is_empty() {
            WordStatus::Empty
        } else {
            WordStatus::Used
        };
        let id = words.len();
        if status == WordStatus::Used {
            cols.push(Col {
                id,
                x,
                x1,
                y,
                y1,
                text: text.clone(),
            });
        }
        words.push(Word {
            id,
            x0: x,
            y0: y,
            x1,
            y1,
            raw,
            text,
            status,
        });
    }
    (cols, words)
}

/// `Col`（縦書き 1 カラム語）から、別表復元で使う中心 y 付きの語へ。
pub(crate) fn col_to_word(c: &Col) -> crate::table::PlacedWord {
    crate::table::PlacedWord {
        x: c.x,
        yc: (c.y + c.y1) / 2.0,
        text: c.text.clone(),
    }
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

/// 新旧対照表の帯を行に組む。[`page_lines`] に加えて表のセルを扱う。
///
/// - 同じ列の中で語と語の間が 1em 前後以上空いていれば全角空白を挟む。帯の中では列内の
///   空きが条見出しと本文の区切り（`第六十二条　かつお…`）やセル境界（物質名｜係数、
///   `（略）｜（略）`）を表すため、詰めると別のセルの語が連結してしまう。
/// - 2em 以上の空きで 2 つ以上のセルに分かれる列（表の行頭列）の後に、字下げして始まり
///   各セルの y 範囲に収まる列が続けば、それは折り返したセルの続きとみなし、該当セルへ
///   連結する（`（２）Ｎ－アルキル－Ｎ・` ＋ `Ｎ・Ｎ－トリメチル…` → 係数の前に組む）。
pub(crate) fn cell_lines(cols: Vec<Col>) -> Vec<Line> {
    let mut lines: Vec<Line> = Vec::new();
    // 表の行: (行頭列の上端 y, 1em, セル列)。
    let mut row: Option<(f32, f32, Vec<Cell>)> = None;
    let render = |cells: Vec<Cell>| Line {
        text: cells.iter().map(|c| c.text.as_str()).collect::<Vec<_>>().join("\u{3000}"),
        words: cells.into_iter().flat_map(|c| c.words).collect(),
    };
    for g in column_groups(cols) {
        let em = (g[0].x1 - g[0].x).max(1.0);
        let cells = split_cells(g, em);
        let top = cells[0].y0;
        if let Some((row_top, row_em, row_cells)) = row.as_mut() {
            let slots: Option<Vec<usize>> = (top > *row_top + *row_em * 0.5)
                .then(|| cells.iter().map(|c| cell_slot(row_cells, c, *row_em)).collect())
                .flatten();
            if let Some(slots) = slots {
                for (c, i) in cells.into_iter().zip(slots) {
                    row_cells[i].text.push_str(&c.text);
                    row_cells[i].words.extend(c.words);
                }
                continue;
            }
            lines.push(render(row.take().unwrap().2));
        }
        if cells.len() >= 2 {
            row = Some((top, em, cells));
        } else {
            let line = render(cells);
            if !is_margin_noise(&line.text) {
                lines.push(line);
            }
        }
    }
    if let Some((_, _, cells)) = row {
        lines.push(render(cells));
    }
    lines
}

/// 列内の空きを空白とみなす閾値（em 比）。0em（フォント切替などの分割）とは区別する。
const GAP_EM: f32 = 0.75;
/// 列内の空きをセル境界とみなす閾値（em 比）。1em の字間（条見出し・均等割り）とは区別する。
const CELL_BREAK_EM: f32 = 1.75;

/// 縦の列の中の 1 セル（y 範囲と、1em 空きを全角空白にしたテキスト）。
struct Cell {
    y0: f32,
    y1: f32,
    text: String,
    words: Vec<usize>,
}

/// 1 列（y 昇順の語）を、空きの大きさに応じて空白挿入・セル分割する。
fn split_cells(g: Vec<Col>, em: f32) -> Vec<Cell> {
    let mut cells: Vec<Cell> = Vec::new();
    for c in g {
        match cells.last_mut() {
            Some(cell) if c.y - cell.y1 < em * CELL_BREAK_EM => {
                if c.y - cell.y1 >= em * GAP_EM {
                    cell.text.push('\u{3000}');
                }
                cell.y1 = cell.y1.max(c.y1);
                cell.text.push_str(&c.text);
                cell.words.push(c.id);
            }
            _ => cells.push(Cell {
                y0: c.y,
                y1: c.y1,
                text: c.text,
                words: vec![c.id],
            }),
        }
    }
    cells
}

/// 折り返しセル `c` が収まる行のセル番号。`c` の上端が属するセルを選ぶ。
///
/// 次の場合はセルの続きではない（新しい小項目の行）として `None`:
/// - 次のセルの上端を越えて伸びる（セルをまたぐ）
/// - 項目の頭（`（新設）`・`十六　…`・`十七〜…`・`２　…`）で始まる
/// - そのセルの直前の断片が文末（`。`）で終わっている
fn cell_slot(row: &[Cell], c: &Cell, em: f32) -> Option<usize> {
    let i = row.iter().rposition(|r| r.y0 <= c.y0 + em * 0.5)?;
    if row.get(i + 1).is_some_and(|next| c.y1 > next.y0 + em * 0.5)
        || starts_item(&c.text)
        || row[i].text.ends_with('。')
    {
        return None;
    }
    Some(i)
}

/// 表のセル内で新しい小項目を始める頭か。
fn starts_item(text: &str) -> bool {
    const MARKS: &[&str] = &["（略）", "（新設）", "（削る）", "〔略〕", "〔同上〕"];
    if MARKS.iter().any(|m| text.starts_with(m)) {
        return true;
    }
    // 括弧付き数字 `（１５）`
    if let Some(rest) = text.strip_prefix('（') {
        let digits = rest.chars().take_while(|c| c.is_ascii_digit() || ('０'..='９').contains(c)).count();
        if digits > 0 && rest.chars().nth(digits) == Some('）') {
            return true;
        }
    }
    // 号・項の番号 + 空白/範囲（`十六　`・`十七〜`・`２　`）
    let is_num = |c: char| "一二三四五六七八九十百".contains(c) || c.is_ascii_digit() || ('０'..='９').contains(&c);
    let n = text.chars().take_while(|c| is_num(*c)).count();
    n > 0 && matches!(text.chars().nth(n), Some('\u{3000}') | Some('〜'))
}

/// カラム語を x 降順（右→左）に縦の列へ束ね、各列を y 昇順（上→下）に並べる。
/// 列間隔(≒13px)に対し、x 中心の差 10px 以内を同じ列とみなす。半角幅の記号（`'` 等）は
/// 列の右半分に置かれ xMin が半字ずれるため、左端ではなく中心で比べる。
fn column_groups(mut cols: Vec<Col>) -> Vec<Vec<Col>> {
    let center = |c: &Col| (c.x + c.x1) / 2.0;
    cols.sort_by(|a, b| center(b).partial_cmp(&center(a)).unwrap_or(std::cmp::Ordering::Equal));
    const COL_THRESHOLD: f32 = 10.0;
    let mut groups: Vec<Vec<Col>> = Vec::new();
    let mut rep_x = f32::INFINITY;
    for c in cols {
        if rep_x - center(&c) > COL_THRESHOLD {
            groups.push(Vec::new());
            rep_x = center(&c);
        }
        groups.last_mut().unwrap().push(c);
    }
    for g in &mut groups {
        g.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));
    }
    groups
}

/// 1ページ分（または1段分）のカラム語を右→左・上→下に並べて本文に組む。
pub(crate) fn reconstruct_page(cols: Vec<Col>) -> String {
    join_lines(&page_lines(cols))
}

/// カラム語を右→左・上→下に並べ、縦の列ごとの行にする。柱（発行日・号数）の列は捨てる。
pub(crate) fn page_lines(cols: Vec<Col>) -> Vec<Line> {
    let mut lines: Vec<Line> = Vec::new();
    for g in column_groups(cols) {
        let text: String = g.iter().map(|c| c.text.as_str()).collect();
        if is_margin_noise(&text) {
            continue;
        }
        lines.push(Line {
            text,
            words: g.iter().map(|c| c.id).collect(),
        });
    }
    lines
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
