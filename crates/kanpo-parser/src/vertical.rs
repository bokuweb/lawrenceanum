//! 縦書き `pdftotext -bbox-layout` 出力から読み順を復元する。

use scraper::{Html, Selector};

use crate::grid::{find_grids, Grid};
use crate::horizontal::{
    horizontal_lines, is_left_strip_header, keyed_tables,
    one_column_lines_with_tables, row_lines, LEFT_STRIP,
};
use crate::lines::PageRules;
use crate::normalize::{
    is_margin_noise, is_toc_page_number, private_cid_glyph, toc_page_numbers, word_text, CidGlyph,
};
use crate::shinkyu::{
    band_pair_unit, detect_shinkyu_bands_headerless, detect_shinkyu_header, lines_with_tables_by,
    place_tables, reconstruct_shinkyu, split_side_by_side_tables, tiers_look_like_bands,
    ShinkyuLayout,
};
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
/// 柱の文字列（「令和８年９月18日」「金曜日」）の行を捨てるのは、上端がこれより上の行だけ(pt)。
/// 柱は上の余白帯にあり、少しはみ出すものもある。本文の日付の行（告示の日付・決算公告の日付。
/// 横倒しのページでは本文の行の頭が 61pt あたりに来る）は残す。
const MARGIN_NOISE_TOP: f32 = TOP_MARGIN + 4.0;
/// 縦長のページの右の余白帯(pt)。号の 1 ページ目（分冊の先頭ページ）には、綴じ穴の印「○」2 つと
/// 中央の印「▷」（私用 CID で「才」に化ける）が本文の右の外（x≈570–588）に刷られている。本文の右端は
/// x≈553 まで。
pub(crate) const RIGHT_MARGIN: f32 = 30.0;
/// 紙面の上の余白の、綴じ穴の印の下端(pt)。
const PUNCH_MARK_BOTTOM: f32 = 30.0;

/// `-bbox-layout` の XHTML から縦書き読み順（右→左・各カラム上→下）を復元する。
///
/// `pdftotext -bbox-layout` は各縦カラムを 1 つの `<word>`（幅≒1文字・高さ=文字数分、
/// テキストは上→下に整列済み）として出力する。よって **カラムを右→左に並べ替える**
/// だけで正しい読み順が復元でき、`-layout` で起きる「別記事の横方向への混線」を解消できる。
/// 複数ページの PDF はページごとに復元して連結する。
pub fn reconstruct_vertical(xhtml: &str) -> String {
    let pages = analyze_pages(xhtml);
    crate::trace::join_page_texts(&pages, &|page| page.text.as_str())
}

/// ページごとに読み順を復元し、各語の割り当て（どの領域のどの行か）も返す。
pub(crate) fn analyze_pages(xhtml: &str) -> Vec<PageTrace> {
    analyze_pages_with_rules(xhtml, &[])
}

/// [`analyze_pages`] に罫線を与える版。`rules[i]` は `xhtml` の i ページ目の罫線（無ければ
/// 表の復元をしない）。罫線があると、新旧対照表の外枠で表外の列を分け、帯の中の罫線の表を
/// 2D 表として復元する。
pub(crate) fn analyze_pages_with_rules(xhtml: &str, rules: &[Option<PageRules>]) -> Vec<PageTrace> {
    let doc = Html::parse_document(xhtml);
    let page_sel = Selector::parse("page").unwrap();

    let mut pages: Vec<PageTrace> = Vec::new();
    // 表の目印の通し番号（ページをまたぐ）。
    let mut table_count = 0usize;
    // 前のページが新旧対照表で終わったか（表の続きのページの判定に使う）。
    let mut prev_table = false;
    // 前のページの、改正後・改正前の列の組が横に並ぶ新旧対照表の列の幅。
    let mut prev_pairs: Option<f32> = None;
    // 前のページが横書きの 2 列の新旧対照表か。
    let mut prev_two_columns = false;
    // その左右の境目の x。
    let mut prev_mid: Option<f32> = None;
    for (pi, page) in doc.select(&page_sel).enumerate() {
        let (width, height) = page_size(page);
        let (cols, mut words, rotated, sideways, merged, minus, native_styles, native_bounds, native_content_styles, native_axes, native_rulings, inline_merged) = page_words_with_styles(page, width, height, rules.get(pi).and_then(|r| r.as_ref()));
        // 負の金額の印「△」を付ける語の字（組んだ後で行の中の位置を探す）。
        let word_texts: std::collections::HashMap<usize, String> = if minus.is_empty() { Default::default() } else { cols.iter().map(|c| (c.id, c.text.clone())).collect() };
        // 長い区切り罫の右側の横統計表は元の向きで読み、縦本文の段切りから外す。
        let (cols, mixed_statistics) = if sideways {
            match rules.get(pi).and_then(|r| r.as_ref()) {
                Some(rules) => crate::horizontal::split_mixed_statistics(cols, &words, rules),
                None => (cols, Vec::new()),
            }
        } else { (cols, Vec::new()) };
        // 横倒しの縦書きページは語の座標を回して使うので、罫線も同じく回す（座標の表・砂防法の土地の表を
        // 罫線の表として組む。回さないと表の欄が 1 語ずつの行になっていた）。
        let turned: Option<PageRules> = rules.get(pi).and_then(|r| r.as_ref()).filter(|_| sideways).map(|r| turn_rules(r, width));
        let page_rules: Option<&PageRules> = turned.as_ref().or_else(|| rules.get(pi).and_then(|r| r.as_ref()));
        let mut grids: Vec<Grid> = page_rules.map(|r| find_grids(r, width.max(height), width.max(height))).unwrap_or_default();
        if !rotated && !is_horizontal_page(&cols) {
            if let Some(rules) = page_rules {
                grids.extend(crate::quantity_tables::infer_quantity_grids(&cols, rules));
            }
        }
        if sideways {
            grids.retain(|g| !unruled_records(g, &cols));
        } else if rotated || is_horizontal_page(&cols) {
            // 別の枠を囲む外枠（決算公告の囲み）は、中の枠の行が並ぶだけなので見ない。
            let cut: Vec<bool> = grids.iter().map(|g| !grids.iter().any(|o| !std::ptr::eq(g, o) && g.encloses(o)) && rows_cut_by_grid(g, &cols, page_rules)).collect();
            let mut k = 0;
            grids.retain(|_| {
                k += 1;
                !cut[k - 1]
            });
        }
        // 枠ごとの、中にある語の ID（普通の本文の段で表を組むのは、枠の中の語が全部その段に
        // あるときだけ。段に切ると表が左右に割れることがある）。
        let grid_words: Vec<Vec<usize>> = grids
            .iter()
            .map(|g| cols.iter().filter(|c| g.contains((c.x + c.x1) / 2.0, (c.y + c.y1) / 2.0, 0.5)).map(|c| c.id).collect())
            .collect();
        let mut layout = "tiers";
        let mut units: Vec<Unit> = Vec::new();
        // 回転ページ（柱が左端帯にある）は、テキスト層が柱だけでも横組みとして扱う。
        if rotated || is_horizontal_page(&cols) {
            layout = "horizontal";
            let horizontal_lines_with_tables =
                |cols: Vec<Col>, grids: &[Grid], vrules: &[(f32, f32, f32)], hrules: &[(f32, f32, f32)]| {
                    if sideways {
                        crate::horizontal::horizontal_lines_with_tables(cols, grids, vrules, hrules)
                    } else {
                        crate::horizontal::horizontal_lines_with_source_tables(
                            cols,
                            grids,
                            vrules,
                            hrules,
                            &native_bounds,
                            &native_axes,
                            &words,
                        )
                    }
                };
            let mut pairs_here = None;
            let mut two_columns_here = false;
            let mut mid_here = None;
            // 表の続きの後ろに同じページで始まる次の記事。
            let mut tail = None;
            let vrules: Vec<(f32, f32, f32)> = page_rules.map(|r| r.vertical.iter().map(|v| (v.x, v.y0, v.y1)).collect()).unwrap_or_default();
            let hrules: Vec<(f32, f32, f32)> = page_rules.map(|r| r.horizontal.iter().map(|h| (h.y, h.x0, h.x1)).collect()).unwrap_or_default();
            // 前のページが横書きの 2 列の新旧対照表なら、その左右の境目をまたぐ語が無く、左右に語があれば
            // 同じ境目で分ける（続きのページは左右の欄の行の高さがずれていて、同じ文が並ぶとは限らない）。
            // 境目は前のページの境目の近く（±30pt）で、またぐ語の最も少ない所（ページの左右の余白は偶数・奇数
            // ページで数 pt ずれる）。
            let local_mid = prev_mid.map(|m| {
                let crossing = |x: f32| cols.iter().filter(|c| c.x < x - 2.0 && c.x1 > x + 2.0).count();
                // 紙面を左右に分ける縦の罫線（ページの高さの半分以上）が近くにあり、またぐ語が無ければその線（語の切れ目で
                // 選ぶと、小数の「0」と「.24」の間で切れ、小数点から後ろが改正前の欄に入っていた）。
                let divider = vrules
                    .iter()
                    .filter(|&&(x, y0, y1)| (x - m).abs() <= 30.0 && y1 - y0 >= height.min(width) * 0.5 && crossing(x) == 0)
                    .map(|&(x, _, _)| x)
                    .min_by(|a, b| (a - m).abs().total_cmp(&(b - m).abs()));
                if let Some(x) = divider {
                    return x;
                }
                cols.iter()
                    .flat_map(|c| [c.x1 + 1.0, c.x - 1.0])
                    .filter(|x| (x - m).abs() <= 30.0)
                    .chain(std::iter::once(m))
                    .min_by(|a, b| crossing(*a).cmp(&crossing(*b)).then((a - m).abs().total_cmp(&(b - m).abs())))
                    .unwrap_or(m)
            });
            // 境目をまたぐ行（次の記事の紙面の幅いっぱいの行）や次の表の欄見出しがあれば、そこより上だけを見る。
            let cut_y = local_mid
                .map(|m| {
                    let crossing = cols.iter().filter(|c| c.x < m - 2.0 && c.x1 > m + 2.0).map(|c| c.y).reduce(f32::min);
                    let label = shinkyu_labels(&cols, '後').iter().map(|l| l.2).reduce(f32::min);
                    crossing.into_iter().chain(label).fold(f32::INFINITY, f32::min)
                })
                .unwrap_or(f32::INFINITY);
            let continued = local_mid.filter(|_| prev_table && prev_two_columns).filter(|&m| {
                let above = |c: &&Col| c.y1 <= cut_y - 1.0;
                cols.iter().filter(above).any(|c| c.x1 <= m) && cols.iter().filter(above).any(|c| c.x >= m)
            });
            // 横書きの 2 列の新旧対照表（左の列が改正後、右の列が改正前）を組む。欄見出しより上は前文。
            // 戻り値は（表, 表の下端より下の語の本文）。
            let two_columns = |cols: Vec<Col>, mid: f32, label_y: f32, label_ids: Vec<usize>| -> (Unit, Option<Unit>) {
                let (pre, cols): (Vec<Col>, Vec<Col>) =
                    cols.into_iter().filter(|c| !label_ids.contains(&c.id)).partition(|c| c.y1 <= label_y + 1.0);
                // 表の下端は改正後・改正前の欄の境の縦の罫線の下端。それより下（表の後ろの別の記事）は表の外。
                // 罫線は行ごとに途切れて描かれることがあるので、間の詰まった線はつなぐ。
                let top = cols.iter().map(|c| c.y).fold(f32::INFINITY, f32::min);
                let mut near: Vec<(f32, f32)> = vrules.iter().filter(|&&(x, _, _)| (x - mid).abs() < 15.0).map(|&(_, y0, y1)| (y0, y1)).collect();
                near.sort_by(|a, b| a.0.total_cmp(&b.0));
                // 表の枠の下の横線が境目をまたいで引かれていれば、その下の縦線（下の 2 段組の記事の段の間の線）とはつながない。
                let closed = |y: f32| {
                    page_rules.is_some_and(|r| r.horizontal.iter().any(|h| (h.y - y).abs() < 3.0 && h.x0 < mid - 50.0 && h.x1 > mid + 50.0))
                };
                let mut bottom = f32::NEG_INFINITY;
                for (y0, y1) in near {
                    if (!bottom.is_finite() && y0 <= top + 20.0) || (bottom.is_finite() && y0 <= bottom + 6.0 && !closed(bottom)) {
                        bottom = bottom.max(y1);
                    }
                }
                if bottom - top < 50.0 {
                    bottom = f32::NEG_INFINITY;
                }
                // 上端が下端から 1pt 以内の語まで表の中（表のすぐ下の「附則」は上端が下端の 1.3pt 下。表の最後の行の金額の
                // 小数点以下は下端の線をまたいで置かれることがあるので、字の中ほどでは見ない）。
                let (cols, after_table): (Vec<Col>, Vec<Col>) =
                    // 表の下端に境目をまたぐ横の罫線（表の枠の下の線）があれば、線より下から始まる語は表の外（線のすぐ
                    // 1pt 下の「備考　表中の〔　〕の記載は注記である。」）。
                    cols.into_iter().partition(|c| !bottom.is_finite() || c.y < bottom + if closed(bottom) { 0.0 } else { 1.0 });
                let tail = (!after_table.is_empty()).then(|| Unit::Plain(horizontal_lines_with_tables(after_table, &grids, &vrules, &hrules)));
                let (left, right): (Vec<Col>, Vec<Col>) = cols.into_iter().partition(|c| c.x1 <= mid);
                // 欄見出し（「改　正　後」）だけの行は本文ではない。
                // 左右の欄の表は欄ごとの枠で組む（左右の別表の罫線がつながって紙面の幅いっぱいの 1 つの
                // 枠になるページは、欄ごとの表が組めなかった）。
                // ページ全体の枠のうちその欄に収まるものを使い、無ければ欄の境目で切った罫線で作る。
                let side_grids = |lo: f32, hi: f32| -> Vec<Grid> {
                    let inside: Vec<Grid> = grids.iter().filter(|g| lo - 3.0 <= g.left() && g.right() <= hi + 3.0).cloned().collect();
                    if !inside.is_empty() {
                        return inside;
                    }
                    page_rules.map(|r| find_grids(&clip_rules(r, lo, hi), width.max(height), width.max(height))).unwrap_or_default()
                };
                let (left_grids, right_grids) = (side_grids(f32::NEG_INFINITY, mid), side_grids(mid, f32::INFINITY));
                let body = |cols: Vec<Col>, grids: &[Grid]| -> Vec<Line> {
                    let mut lines = one_column_lines_with_tables(cols, grids, &vrules, &hrules);
                    let label = |l: &Line| matches!(l.text.chars().filter(|c| !c.is_whitespace()).collect::<String>().as_str(), "改正後" | "改正前");
                    if lines.first().is_some_and(label) {
                        lines.remove(0);
                    }
                    lines
                };
                let unit = Unit::Shinkyu {
                    // 欄見出しより上（前の記事の表の続きが上にあるページも）も罫線の表を組んで読む。
                    preamble: {
                        let pre_geom = pre.clone();
                        // 前文の中の次の記事の標題（「〇厚生労働省告示第…号」）より上（前の記事の表の続き）と下は別に組む（標題が
                        // 表の続きの左右の欄の間に読まれていた）。
                        let marked = pre.iter().filter(|c| crate::segment::is_heading_mark(c.text.trim())).min_by(|a, b| a.y.total_cmp(&b.y));
                        let heading = marked.map_or(f32::INFINITY, |c| c.y);
                        let heading_x = marked.map_or(0.0, |c| (c.x + c.x1) / 2.0);
                        // 標題より上が左右の欄に分かれた表の続き（欄の境目の近くに、またぐ語の無い切れ目があるもの）のときだけ
                        // （紙面の幅いっぱいの表の途中の標題では切らない）。
                        let above: Vec<&Col> = pre.iter().filter(|c| c.y < heading - 1.0).collect();
                        // 標題と反対の側の欄は標題の高さで途切れる（そこで 2 行分以上空く。2 段組の紙面の片方の段の標題では、
                        // もう片方の段の表の行が続くので切らない）。
                        let two_sided = above.iter().map(|c| c.x1 + 1.0).filter(|x| (x - mid).abs() <= 40.0).any(|x| {
                            let other = |c: &&Col| ((c.x + c.x1) / 2.0 < x) != (heading_x < x) && !(c.x < x - 1.0 && c.x1 > x + 1.0);
                            !above.iter().any(|c| c.x < x - 1.0 && c.x1 > x + 1.0)
                                && above.iter().any(|c| c.x1 <= x)
                                && above.iter().any(|c| c.x >= x)
                                && {
                                    let end = pre.iter().filter(other).filter(|c| c.y < heading - 1.0).map(|c| c.y1).fold(f32::NEG_INFINITY, f32::max);
                                    let next = pre.iter().filter(other).filter(|c| c.y >= heading - 1.0).map(|c| c.y).fold(f32::INFINITY, f32::min);
                                    let em = marked.map_or(8.0, |c| (c.y1 - c.y).min(c.x1 - c.x).max(1.0));
                                    next - end >= em * 2.0
                                }
                        });
                        let heading = if two_sided { heading } else { f32::INFINITY };
                        let (head, rest): (Vec<Col>, Vec<Col>) = pre.into_iter().partition(|c| c.y < heading - 1.0);
                        let mut lines = horizontal_lines_with_tables(head, &grids, &vrules, &hrules);
                        lines.extend(horizontal_lines_with_tables(rest, &grids, &vrules, &hrules));
                        crate::horizontal::rejoin_split_preamble_numerals(&mut lines, &pre_geom);
                        lines
                    },
                    after: vec![body(left, &left_grids)],
                    before: vec![body(right, &right_grids)],
                    outside: Vec::new(),
                };
                (unit, tail)
            };
            // 前のページの新旧対照表（縦書きの表も）の続きで、左右に高さのそろった罫線の表が 1 つずつ並ぶ横向きの
            // ページは、その間を境目にする（号外第 215 号 323 ページ: 改正後・改正前の別表が左右に並ぶ）。
            let side_by_side = prev_table
                .then(|| {
                    let inner: Vec<&Grid> = grids.iter().filter(|g| g.cells.len() >= 4 && !grids.iter().any(|o| g.encloses(o))).collect();
                    let [l, r] = inner.as_slice() else { return None };
                    let (l, r) = if l.left() < r.left() { (l, r) } else { (r, l) };
                    let overlap = l.bottom().min(r.bottom()) - l.top().max(r.top());
                    let rows = |g: &Grid| g.ys.len() as f32;
                    let ok = l.right() < r.left()
                        && overlap >= (l.bottom() - l.top()).max(r.bottom() - r.top()) * 0.8
                        && (rows(l) - rows(r)).abs() <= rows(l).max(rows(r)) * 0.3;
                    let mid = (l.right() + r.left()) / 2.0;
                    (ok && !cols.iter().any(|c| c.x < mid - 2.0 && c.x1 > mid + 2.0)).then_some(mid)
                })
                .flatten();
            // 1 ページに新旧対照表が 2 つ以上あり（欄見出しの組が上下に 2 組以上）、前のページの表の続きでもないときは、
            // 各表の改正の指示の行（「…を次の表のように改正する。」）か欄見出しの上でページを上下の帯に分け、帯ごとに
            // 1 つの表として組む（号外第 202 号 2 ページ: 欄見出しが 2 組あって、どちらの表も左右に分けられなかった）。
            // 欄見出しの組（上端の y と、改正後の見出しの右端・改正前の見出しの左端）。
            let label_rows: Vec<(f32, f32, f32)> = {
                let (after, before) = (shinkyu_labels(&cols, '後'), shinkyu_labels(&cols, '前'));
                let mut rows: Vec<(f32, f32, f32)> = after
                    .iter()
                    .filter_map(|a| before.iter().find(|b| (a.2 - b.2).abs() <= 2.0 && a.1 < b.0).map(|b| (a.2, a.1, b.0)))
                    .collect();
                rows.sort_by(|p, q| p.0.total_cmp(&q.0));
                rows.dedup_by(|p, q| (p.0 - q.0).abs() < 5.0);
                rows
            };
            // 帯の境目は前の表の下端（改正後・改正前の欄の境の縦の罫線の下端）のすぐ下。罫線が無ければ次の表の改正の指示の行。
            let band_tops: Option<Vec<f32>> = (continued.is_none() && side_by_side.is_none() && label_rows.len() >= 2).then(|| {
                label_rows
                    .windows(2)
                    .map(|w| {
                        let ((y0, lo, hi), (y1, _, _)) = (w[0], w[1]);
                        let bottom = vrules
                            .iter()
                            .filter(|&&(x, top, end)| x > lo && x < hi && top <= y0 + 30.0 && end > y0 && end < y1)
                            .map(|&(x, _, mut end)| {
                                // 同じ境界線が表の行ごとに分割されている場合は、接続する線分の末尾まで追う。
                                // 左右の行数が違う表で途中の行境界を表末とすると、旧欄だけの行が次の前文へ落ちる。
                                loop {
                                    let next = vrules.iter()
                                        .filter(|&&(nx, top, bottom)| (nx - x).abs() < 1.0 && top <= end + 2.0 && bottom > end + 1.0 && bottom < y1)
                                        .map(|&(_, _, bottom)| bottom).fold(end, f32::max);
                                    if next <= end + 1.0 { break; }
                                    end = next;
                                }
                                end
                            })
                            .reduce(f32::max);
                        bottom.map(|b| b + 1.0).unwrap_or_else(|| {
                            cols.iter()
                                .filter(|c| c.y > y0 && c.y < y1 && (c.text.contains("ように改正する") || c.text.contains("ように改める")))
                                .map(|c| c.y)
                                .reduce(f32::max)
                                .unwrap_or(y1)
                                - 1.0
                        })
                    })
                    .collect()
            });
            if let Some(tops) = band_tops {
                let mut bands: Vec<Vec<Col>> = vec![Vec::new(); tops.len() + 1];
                // 字の中ほどで振り分ける（次の表の前の見出しは、上端が前の表の下端の線より少し上に出ることがある）。
                for c in cols {
                    let k = tops.iter().filter(|&&t| (c.y + c.y1) / 2.0 >= t).count();
                    bands[k].push(c);
                }
                for band in bands.into_iter().filter(|b| !b.is_empty()) {
                    match split_horizontal_shinkyu(&band, false) {
                        Some((mid, label_y, label_ids)) => {
                            let (unit, t) = two_columns(band, mid, label_y, label_ids);
                            (two_columns_here, mid_here) = (t.is_none(), Some(mid));
                            units.push(unit);
                            units.extend(t);
                        }
                        None => {
                            (two_columns_here, mid_here) = (false, None);
                            units.push(Unit::Plain(horizontal_lines_with_tables(band, &grids, &vrules, &hrules)));
                        }
                    }
                }
            } else {
                let split = continued
                    .or(side_by_side)
                    .map(|m| (m, f32::NEG_INFINITY, Vec::new()))
                    .or_else(|| split_horizontal_shinkyu(&cols, prev_table && prev_two_columns));
                // 続きのページに次の記事・次の表の欄見出しがあれば、そこより上を前の表の続き、下を新しい表か本文として組む。
                let next_label_y = continued.and(Some(cut_y)).filter(|y| y.is_finite());
                match (split, next_label_y) {
                    (Some((mid, _, _)), Some(label_y)) => {
                        let (top, rest): (Vec<Col>, Vec<Col>) = cols.into_iter().partition(|c| c.y1 <= label_y - 1.0);
                        // 次の表の改正の指示（「第二条　…を次の表のように改正する。」）から下は次の表の前文。
                        let instruction = top
                            .iter()
                            .filter(|c| c.text.contains("ように改正する") || c.text.contains("ように改める"))
                            .map(|c| c.y)
                            .reduce(f32::min)
                            .unwrap_or(f32::INFINITY);
                        let (top, mut pre): (Vec<Col>, Vec<Col>) = top.into_iter().partition(|c| c.y1 < instruction - 1.0);
                        // 前の表の続きは改正前の欄（右）の最後の行で終わる。その下の左の語も次の表の前文。
                        let right_bottom = top.iter().filter(|c| c.x >= mid).map(|c| c.y1).fold(f32::NEG_INFINITY, f32::max);
                        let (top, below): (Vec<Col>, Vec<Col>) = top.into_iter().partition(|c| c.x >= mid || c.y <= right_bottom + 4.0);
                        pre.extend(below);
                        if !top.is_empty() {
                            let (unit, tail) = two_columns(top, mid, f32::NEG_INFINITY, Vec::new());
                            units.push(unit);
                            units.extend(tail);
                        }
                        if !pre.is_empty() {
                            units.push(Unit::Plain(horizontal_lines_with_tables(pre, &grids, &vrules, &hrules)));
                        }
                        match split_horizontal_shinkyu(&rest, false) {
                            Some((mid, label_y, label_ids)) => {
                                let (unit, tail) = two_columns(rest, mid, label_y, label_ids);
                                (two_columns_here, mid_here) = (tail.is_none(), Some(mid));
                                units.push(unit);
                                units.extend(tail);
                            }
                            None => match split_horizontal_shinkyu_pairs(&rest, None, &vrules) {
                                Some((unit, rest, geometry)) => {
                                    if rest.is_empty() {
                                        pairs_here = Some(geometry);
                                    }
                                    units.push(unit);
                                    if !rest.is_empty() {
                                        units.push(Unit::Plain(horizontal_lines_with_tables(rest, &grids, &vrules, &hrules)));
                                    }
                                }
                                None => units.push(Unit::Plain(horizontal_lines_with_tables(rest, &grids, &vrules, &hrules))),
                            },
                        }
                    }
                    (Some((mid, label_y, label_ids)), None) => {
                        let (unit, t) = two_columns(cols, mid, label_y, label_ids);
                        (two_columns_here, mid_here) = (t.is_none(), Some(mid));
                        units.push(unit);
                        tail = t;
                    }
                    (None, _) => match split_horizontal_shinkyu_pairs(&cols, prev_pairs.filter(|_| prev_table), &vrules) {
                        Some((unit, rest, geometry)) => {
                            if rest.is_empty() {
                                pairs_here = Some(geometry);
                            } else {
                                tail = Some(Unit::Plain(horizontal_lines_with_tables(rest, &grids, &vrules, &hrules)));
                            }
                            units.push(unit);
                        }
                        // 組が横に 2 組入る幅でも、2 組に分けられなければ（改正後・改正前の表の下に 2 段組の別の記事が
                        // 続き、ページが広く見えるもの）、1 組の表として組む。
                        None => match split_horizontal_shinkyu_by(&cols, false, false).filter(|&(mid, label_y, _)| {
                            // 表の範囲がはっきりしているとき（境目の縦線が欄見出しの所から下へ引かれ、その下端で境目をまたぐ横線が
                            // 閉じる）だけ。
                            page_rules.is_some_and(|r| {
                                r.vertical.iter().filter(|v| (v.x - mid).abs() < 15.0 && v.y0 <= label_y + 20.0 && v.y1 - v.y0 >= 50.0).any(|v| {
                                    r.horizontal.iter().any(|h| (h.y - v.y1).abs() < 3.0 && h.x0 < mid - 50.0 && h.x1 > mid + 50.0)
                                })
                            })
                        }) {
                            Some((mid, label_y, label_ids)) => {
                                let (unit, t) = two_columns(cols, mid, label_y, label_ids);
                                (two_columns_here, mid_here) = (t.is_none(), Some(mid));
                                units.push(unit);
                                tail = t;
                            }
                            None => units.push(Unit::Plain(horizontal_lines_with_tables(cols, &grids, &vrules, &hrules))),
                        },
                    },
                }
            }
            units.extend(tail);
            crate::financial_flow::restore(&mut units, &mut words);
            prefix_minus(&mut units, &minus, &word_texts);
            if !sideways { crate::native_triangle_cells::restore(&mut units, &words, page); }
            move_trailing_notes(&mut units);
            merge_split_table_rows(&mut units);
            order_masthead(&mut units);
            join_joint_ministries(&mut units);
            crate::shared_table_labels::order_shared_table_labels(&mut units, &words);
            crate::fractions::restore_fraction_lines(&mut units, &words, &hrules);
            let short_bars: Vec<_> = page_rules.map(|r| r.short_fraction_bars.iter().map(|h| (h.y,h.x0,h.x1)).collect()).unwrap_or_default();
            crate::parallel_fractions::restore(&mut units, &words, &hrules, &short_bars);
            crate::numeric_fractions::restore(&mut units, &words, &hrules, &short_bars);
            crate::table_dashes::restore_table_dashes(&mut units, &mut words);
            crate::table_dashes::restore_ruled_table_dashes(&mut units, &mut words, &grids);
            crate::table_dashes::restore_plain_numeric_dashes(&mut units, &mut words, &vrules);
            crate::cell_row_dashes::restore_cell_record_dashes(&mut units, &mut words, &grids);
            crate::owned_dashes::restore_owned_dashes(&mut units, &mut words, &grids, &vrules);
            crate::ruled_plain_dashes::restore(&mut units, &mut words, &grids);
            crate::large_parentheses::restore_large_parentheses(&mut units, &words);
            crate::coordinate_symbols::restore_coordinate_symbols(&mut units, &mut words);
            crate::coordinate_rows::restore_coordinate_rows(&mut units, &mut words, rules.get(pi).and_then(|r| r.as_ref()), width.max(height));
            crate::area_units::restore_area_units(&mut units, &mut words);
            if !sideways { crate::blank_area_units::restore(&mut units, &mut words, page_rules, width.max(height)); }
            crate::reference_markers::restore(&mut units, &mut words);
            crate::inline_quotes::restore(&mut units, &mut words);
            crate::range_endpoints::restore(&mut units, &mut words);
            crate::micro_prefixes::restore(&mut units, &mut words);
            crate::chemical_line_end::restore(&mut units, &mut words);
            crate::circled_numbers::restore(&mut units, &mut words);
            if let Some(rules) = rules.get(pi).and_then(|r| r.as_ref()) {
                crate::financial_enclosures::order_with_source(
                    &mut units, &words, &rules.enclosures, width, height,
                    &native_styles, &native_content_styles, &native_bounds, &native_axes, &native_rulings, &pages,
                );
            }
            if !sideways {
                if let Some(rules) = rules.get(pi).and_then(|r| r.as_ref()) {
                    crate::common_financial_header::restore(&mut units,&words,&native_styles,&native_content_styles,&native_bounds,&rules.enclosures);
                }
            }
            crate::native_unit_dash::restore(&mut units, &mut words, &native_bounds, &native_axes, &native_styles, rules.get(pi).and_then(|r| r.as_ref()));
            let text = units.iter().map(Unit::text).filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n");
            let (doc_text, tables) = doc_text_of(&units, &mut table_count);
            prev_table = matches!(units.last(), Some(Unit::Shinkyu { .. }));
            prev_pairs = pairs_here;
            prev_two_columns = two_columns_here;
            prev_mid = mid_here;
            let mut regions: Vec<_> = units.into_iter().enumerate().flat_map(|(i, u)| u.regions(i)).collect();
            enclosed_warichu::expand(&mut regions,&inline_merged,&merged);
        mark_unused(&mut words, &regions, &merged);
            pages.push(PageTrace {native_axes,native_rulings,
                native_styles,
                native_bounds,
                native_content_styles,
                width,
                height,
                layout,
                words,
                regions,
                text,
                doc_text,
                tables,
            });
            continue;
        }
        // 多ページ新旧対照表の継続ページ（2 ページ目以降）は「改正後/改正前」の欄見出しを
        // 繰り返さない。ページ全体に見出しが無く、上下半がほぼ同一内容（改正後 ≒ 改正前）なら、
        // 継続ページとみなして上下帯に分け、改正後/改正前の見出しを補って復元する。
        // ※ 見出しのあるページ（先頭ページ）や、見出しはないが別記事が同居するページ
        //   （例: 郵便+型式認定）は段(tier)単位で扱う必要があるため、ここでは除外する。
        let headerless = detect_shinkyu_header(&cols)
            .is_none()
            .then(|| {
                detect_shinkyu_bands_headerless(&cols, height)
                    .or_else(|| prev_table.then(|| marker_band_divider(&cols, height)).flatten())
                    .or_else(|| prev_table.then(|| rule_band_divider(&cols, page_rules, height)).flatten())
            })
            .flatten();
        let marker_plan = (!sideways && !rotated)
            .then(|| ruled_marker_bands::native_layout_with_axes(
                &cols, &words, &native_bounds, &native_axes, &merged,
                &inline_merged, page_rules, &grids,
            ))
            .flatten();
        // A source-proven local pair can contradict a whole-page continuation
        // guess. In that case keep the ordinary tier pipeline and apply only the
        // certified local fields below; previous-page context cannot overrule them.
        let headerless = headerless.filter(|&divider| !marker_plan.as_ref().is_some_and(|plan|
            ruled_marker_bands::contradicts_global_divider(plan, &cols, divider)));
        if let Some(divider) = headerless {
            layout = "headerless_shinkyu";
            // 表の左に、上下の帯の境目をまたいで紙面の高さいっぱいに組まれた別記事（表の
            // 後ろの附則・次の記事）が同居していれば、表の後ろに本文として出す。
            let (table, rest) = split_off_full_height_left(cols, divider, page_rules, &grids);
            units.push(reconstruct_shinkyu(table, f32::MAX, divider, &grids, page_rules));
            if !rest.is_empty() {
                units.push(Unit::Plain(drop_stray_labels(page_lines(rest))));
            }
        } else {
            // 前のページの新旧対照表の続きが、ページの右端の罫線の欄（上下の欄）に収まり、その左に
            // 別の記事が続くページは、段に切る前に欄の中を表の続きとして切り出す（欄が上下の段を
            // またぐので、段に切ると改正前の欄が次の記事の表に入ってしまう）。
            // 前のページが分からなくても（ページごとの解析）、欄の左に紙面の高さいっぱいの別の記事（附則・次の省令）が
            // 上下の欄の境目をまたいで続くなら、同じく表の続きとして欄の上・下・左の順に読む（段に切ると、上の欄と下の
            // 欄の間に左の記事が挟まっていた）。
            let original = cols.clone();
            let cols = match split_off_leading_frame(cols, page_rules) {
                Ok((after, before, rest, labeled)) if labeled || prev_table || spans_divider(&rest, &after, &before) => {
                    units.push(band_pair_unit(after, before, &grids, page_rules));
                    rest
                }
                Ok(_) => original,
                Err(cols) => cols,
            };

            // 通常ページ: まず段組み(tier)に分ける。官報本紙は2段組が多く、段を分けないと
            // 「同 x にある上段の列と下段の列」が1列に連結され混線する。新旧対照表の見出しが
            // ある段は 2 帯へ、無ければ右→左復元する。表と無関係記事が同居するページも
            // tier で分かれ混入を防げる。見出しのある段に続く段の組（表がページ下半へ続く形）は、
            // 表の続きなら取り込む。
            // 1 つの段に表が横に並ぶなら表ごとに分ける（右から順）。
            // 罫線の表（他の枠を含まない枠で、中の字が縦組み、セルに文章の無いもの）は段の
            // 切れ目で割らない（セルの間の空きが段の境目に見えて、表が上下に割れていた）。
            // 新旧対照表のあるページ（見出しのある段・帯の組になる段がある）は帯の中の表を帯ごとに
            // 組むので、これまでどおりに切る。
            let hrules: Vec<(f32, f32, f32)> = page_rules.map(|r| r.horizontal.iter().map(|h| (h.y, h.x0, h.x1)).collect()).unwrap_or_default();
            let vrules: Vec<(f32, f32, f32)> = page_rules.map(|r| r.vertical.iter().map(|v| (v.x, v.y0, v.y1)).collect()).unwrap_or_default();
            // 罫線の無い縦組みの表（[`page_lines_keyed`]）は、欄の間の空きで段に切らない（認定番号の表の
            // 「認定を受けた者」の欄が下の段に分かれていた）。
            let keyed = keyed_table_boxes(&cols);
            // 横組みの囲み（決算公告）も、見出しと貸借対照表の間の空きで段に切らない。
            let boxes = horizontal_block_boxes(&cols);
            let plain_tiers =
                merge_split_header_tiers(split_tiers(cols.clone(), &TierRules { tables: keyed.clone(), boxes: boxes.clone(), hrules: hrules.clone(), vrules: vrules.clone(), sideways }));
            let shinkyu_page = plain_tiers.iter().any(|t| detect_shinkyu_header(t).is_some())
                || plain_tiers.windows(2).any(|w| tiers_look_like_bands(&w[0], &w[1]));
            let tables: Vec<(f32, f32, f32, f32)> = grids
                .iter()
                .filter(|g| g.cells.len() >= 2 && !grids.iter().any(|o| g.encloses(o)) && vertical_inside(&cols, g) && !prose_cells(&cols, g))
                .map(|g| (g.left(), g.right(), g.top(), g.bottom()))
                .chain(keyed)
                .collect();
            // 横組みの語の多いページ（横書きの表が並ぶ告示など）は段の組み方が当てにならない。
            let (v_chars, h_chars) = cols.iter().fold((0, 0), |(v, h), c| {
                let n = c.text.chars().count();
                if n >= 2 && c.y1 - c.y > (c.x1 - c.x) * 1.5 {
                    (v + n, h)
                } else if n >= 2 && c.x1 - c.x > (c.y1 - c.y) * 1.5 {
                    (v, h + n)
                } else {
                    (v, h)
                }
            });
            // 横倒しのページ（回した座標で組む）は語が横組みでも、罫線の表を段で割らない（型式認定の表の右半分の表が、欄ごとの
            // 段に分かれて欄ごとに読まれていた）。
            let tiers = if shinkyu_page || tables.is_empty() || (v_chars < h_chars * 2 && !sideways) {
                plain_tiers
            } else {
                merge_split_header_tiers(split_tiers(cols, &TierRules { tables, boxes, hrules: hrules.clone(), vrules: vrules.clone(), sideways }))
            };
            let mut tiers: Vec<Vec<Col>> = attach_toc_page_numbers(tiers.into_iter().flat_map(split_side_by_side_tables).collect(), &words);
            // 新旧対照表のページで、行の罫線の無い枠（帯の中の表）の欄の境の横線で切られた段は、前の段に戻す（改正前の
            // 帯の救助器具の表の「性能等」の欄が別の段になり、表から外れていた）。
            if shinkyu_page {
                rejoin_split_shinkyu_grid_tiers(&mut tiers, &grids);
                let center = |c: &Col| ((c.x + c.x1) / 2.0, (c.y + c.y1) / 2.0);
                let mut i = 1;
                while i < tiers.len() {
                    let inside = grids.iter().filter(|g| g.xs.len() <= 3 && g.ys.len() >= 4).any(|g| {
                        let within = |c: &Col| {
                            let (x, y) = center(c);
                            g.contains(x, y, 0.0)
                        };
                        !tiers[i].is_empty() && tiers[i].iter().all(within) && tiers[i - 1].iter().filter(|c| within(c)).count() >= 3
                    });
                    if inside {
                        let t = tiers.remove(i);
                        tiers[i - 1].extend(t);
                    } else {
                        i += 1;
                    }
                }
            }
            // 新旧対照表のあるページで、帯の外の罫線の表（行ごとの縦の罫線と欄の境の横の罫線のある表。表の後ろの別の告示の
            // 型式認定の表）が段の切れ目で欄ごとに割れたものは 1 つの段に戻す（欄ごとの一覧になり、どの番号の型式・者か
            // 分からなかった）。続く段がどれもほとんど表の中で（表の右の日付・署名の列は残ってよい）、表の語を全部含み、
            // 戻した段で表として組めるときだけ。改正後・改正前の帯（帯の組に見える段）は戻さない。
            if shinkyu_page && !prev_table {
                let center = |c: &Col| ((c.x + c.x1) / 2.0, (c.y + c.y1) / 2.0);
                let paired = |tiers: &[Vec<Col>], k: usize| {
                    detect_shinkyu_header(&tiers[k]).is_some()
                        || (k > 0 && tiers_look_like_bands(&tiers[k - 1], &tiers[k]))
                        || (k + 1 < tiers.len() && tiers_look_like_bands(&tiers[k], &tiers[k + 1]))
                };
                let mut i = 0;
                while i + 1 < tiers.len() {
                    let run = grids.iter().zip(&grid_words).filter(|(g, _)| g.xs.len() >= 4 && g.ys.len() >= 3 && !grids.iter().any(|o| g.encloses(o))).find_map(|(g, gw)| {
                        let within = |c: &Col| {
                            let (x, y) = center(c);
                            g.contains(x, y, 0.0)
                        };
                        let mostly = |t: &[Col]| {
                            let n = t.iter().filter(|c| within(c)).count();
                            n >= 3 && n * 10 >= t.len() * 8 && t.iter().all(|c| within(c) || c.x >= g.right())
                        };
                        let end = (i..tiers.len()).take_while(|&k| mostly(&tiers[k]) && !paired(&tiers, k)).last()?;
                        if end == i {
                            return None;
                        }
                        let merged: Vec<Col> = tiers[i..=end].iter().flatten().cloned().collect();
                        let ids: std::collections::HashSet<usize> = merged.iter().map(|c| c.id).collect();
                        (gw.iter().all(|id| ids.contains(id)) && vertical_inside(&merged, g) && !prose_cells(&merged, g)).then_some(end)
                    });
                    match run {
                        Some(end) => {
                            let rest: Vec<Col> = tiers.drain(i + 1..=end).flatten().collect();
                            tiers[i].extend(rest);
                        }
                        None => i += 1,
                    }
                }
            }
            // 変更後・変更前の枠（[`split_label_frames`]）の中の表だけの段は、枠の欄見出しのある前の段に戻す（表が別の段に
            // 分かれると、上の欄と下の欄の見出しを読んだ後に両方の欄の表が続いた）。
            if !shinkyu_page {
                let mut i = 1;
                while i < tiers.len() {
                    let inside = label_frames(&tiers[i - 1], &hrules, &vrules).iter().any(|&(x0, x1, top, bottom, _)| {
                        !tiers[i].is_empty()
                            && tiers[i].iter().all(|c| {
                                let (cx, cy) = ((c.x + c.x1) / 2.0, (c.y + c.y1) / 2.0);
                                cx > x0 && cx < x1 && cy > top && cy < bottom
                            })
                    });
                    if inside {
                        let t = tiers.remove(i);
                        tiers[i - 1].extend(t);
                    } else {
                        i += 1;
                    }
                }
            }
            // 段の中の罫線の表（他の枠を含まない枠で、中の語が全部この段にあり、中の字が縦組みで、
            // セルに文章の入っていないものだけ。横組みの表は行ごとに読むこれまでの組み方のほうがよい）。
            let inner_grids = |tier: &[Col]| -> Vec<&Grid> {
                let ids: std::collections::HashSet<usize> = tier.iter().map(|c| c.id).collect();
                grids
                    .iter()
                    .zip(&grid_words)
                    .filter(|(g, gw)| {
                        g.cells.len() >= 2
                            && !grids.iter().any(|o| g.encloses(o))
                            && gw.iter().all(|id| ids.contains(id))
                            && vertical_inside(tier, g)
                            && !prose_cells(tier, g)
                    })
                    .map(|(g, _)| g)
                    .collect()
            };
            // 前のページの新旧対照表の続きで、改正前の欄が空いている（新設の条の続き）ページは、
            // 上半分（改正後の帯）にだけ字がある。改正後だけの表として前の表につなぐ。
            if prev_table && tiers.len() == 1 && after_band_only(&tiers[0], height) {
                let tier = std::mem::take(&mut tiers[0]);
                tiers.clear();
                let inner = inner_grids(&tier);
                let after = lines_with_tables_by(tier, &inner, cell_lines);
                units.push(Unit::Shinkyu { preamble: Vec::new(), after: vec![after], before: vec![Vec::new()], outside: Vec::new() });
            }
            let mut i = 0;
            while i < tiers.len() {
                // 罫線の無い縦組みの表（欄ごとの低い段が上下に並び、行が同じ x の列に揃う名簿）は行ごとに組む。
                // 新旧対照表のあるページでも、新旧対照表の欄見出しの無い段（同じページの別の告示の名簿）なら見る。
                let plain_tier = !shinkyu_page || detect_shinkyu_header(&tiers[i]).is_none();
                if let Some((n, lines)) = (plain_tier && !prev_table)
                    .then(|| record_bands_with_suffix_context(&tiers[i..], (!sideways).then_some(height / 2.0), &units, &hrules))
                    .flatten()
                    // 新旧対照表のページでは、届出年月日などの日付で始まる行が 3 行以上あり、
                    // 「（略）」の無い名簿だけ（新旧対照表の帯の続きを名簿にしない）。
                    .filter(|(_, lines)| {
                        !shinkyu_page
                            || (!lines.iter().any(|l| l.text.contains("（略）"))
                                && lines
                                    .iter()
                                    .filter(|l| ["令和", "平成", "昭和"].iter().any(|e| l.text.starts_with(e)))
                                    .count()
                                    >= 3)
                    })
                {
                    units.push(Unit::Plain(lines));
                    i += n;
                    continue;
                }
                let tier = std::mem::take(&mut tiers[i]);
                i += 1;
                let unit = match detect_shinkyu_header(&tier) {
                    Some((header_x, divider)) => {
                        let mut layout = ShinkyuLayout::split(tier, header_x, divider);
                        layout.clip_to_frame(&grids, header_x, divider);
                        units.extend(layout.split_off_leading_continuation(divider, &grids, page_rules));
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
                        layout.into_unit(&grids, page_rules)
                    }
                    // 見出しの無い 2 段が改正後/改正前の組なら（4 段に組んだ継続ページ）表として組む。
                    None if i < tiers.len()
                        && detect_shinkyu_header(&tiers[i]).is_none()
                        && tiers_look_like_bands(&tier, &tiers[i]) =>
                    {
                        let next = std::mem::take(&mut tiers[i]);
                        i += 1;
                        band_pair_unit(tier, next, &grids, page_rules)
                    }
                    // 普通の本文の段でも、罫線の表と、行の見出しの記号が並ぶ罫線の無い表は表として組む。
                    // 上下の欄に分けた変更後・変更前の枠（[`split_label_frames`]）は、枠の右・上の欄・下の欄・枠の左の順に読む。
                    None => {
                        let mut lines = Vec::new();
                        for part in split_label_frames(tier, &hrules, &vrules) {
                            let inner = inner_grids(&part);
                            lines.extend(lines_with_tables_by(part, &inner, |c| page_lines_keyed(c, &vrules, &hrules)));
                        }
                        Unit::Plain(drop_stray_labels(lines))
                    }
                };
                units.push(unit);
            }
        }
        attach_band_tables(&mut units, &words);
        if sideways {
            unturn_tables(&mut units, width);
            if let Some(native_rules) = rules.get(pi).and_then(|r| r.as_ref()) {
                crate::horizontal::repair_native_ledger_tables(&mut units, &words, native_rules);
            }
        }
        if !mixed_statistics.is_empty() {
            units.push(Unit::Plain(mixed_statistics));
        }
        prefix_minus(&mut units, &minus, &word_texts);
        if !sideways { crate::native_triangle_cells::restore(&mut units, &words, page); }
        move_trailing_notes(&mut units);
        merge_split_table_rows(&mut units);
        order_masthead(&mut units);
        join_joint_ministries(&mut units);
        order_section_labels(&mut units, &words);
        crate::unruled_road::restore(&mut units, &words);
        crate::large_parentheses::restore_large_parentheses(&mut units, &words);
        crate::coordinate_symbols::restore_coordinate_symbols(&mut units, &mut words);
        crate::coordinate_rows::restore_coordinate_rows(&mut units, &mut words, rules.get(pi).and_then(|r| r.as_ref()), width.max(height));
        crate::area_units::restore_area_units(&mut units, &mut words);
        if !sideways { crate::blank_area_units::restore(&mut units, &mut words, page_rules, width.max(height)); }
        crate::reference_markers::restore(&mut units, &mut words);
        crate::inline_quotes::restore(&mut units, &mut words);
        crate::range_endpoints::restore(&mut units, &mut words);
        crate::micro_prefixes::restore(&mut units, &mut words);
        crate::chemical_line_end::restore(&mut units, &mut words);
        crate::circled_numbers::restore(&mut units, &mut words);
        if sideways { if let Some(native_rules) = rules.get(pi).and_then(|r| r.as_ref()) { crate::framed_amount_rows::restore(&mut units, &words, native_rules); } }
        if !sideways && !rotated { crate::facility_continuations::restore(&mut units, &words, page_rules, width, height); }
        if !sideways && !rotated { crate::framed_band_continuations::restore(&mut units, &words, page_rules, width, height); }
        if !sideways && !rotated { crate::paired_framed_continuations::restore(&mut units, &words, page_rules, width, height); }
        if !sideways && !rotated { crate::headerless_four_bands::restore(&mut units, &words, page_rules, width); }
        if !sideways && !rotated { crate::quoted_inline_tables::restore(&mut units, &words, page_rules); }
        if !sideways {
            if let Some(rules) = rules.get(pi).and_then(|r| r.as_ref()) {
                crate::common_financial_header::restore(&mut units,&words,&native_styles,&native_content_styles,&native_bounds,&rules.enclosures);
            }
        }
        if !sideways && !rotated { if let Some(rules) = rules.get(pi).and_then(|r| r.as_ref()) { crate::notice_boundaries::restore_ruled_corrections(&mut units, &words, &native_styles, &native_content_styles, &native_bounds, rules, &pages); } }
        // Certifying a local field says nothing about the following page. Keep
        // legacy continuation state independent of this late, page-local overlay.
        let legacy_continuation = matches!(units.last(), Some(Unit::Shinkyu { .. }));
        let marker_members = marker_plan
            .and_then(|plan| ruled_marker_bands::apply(&mut units, plan, &inline_merged));
        crate::native_unit_dash::restore(&mut units, &mut words, &native_bounds, &native_axes, &native_styles, rules.get(pi).and_then(|r| r.as_ref()));
        let text = units
            .iter()
            .map(Unit::text)
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        let (doc_text, tables) = doc_text_of(&units, &mut table_count);
        prev_table = legacy_continuation;
        prev_pairs = None;
        prev_two_columns = false;
        prev_mid = None;
        let mut regions: Vec<_> = units.into_iter().enumerate().flat_map(|(i, u)| u.regions(i)).collect();
        enclosed_warichu::expand(
            &mut regions, marker_members.as_deref().unwrap_or(&inline_merged), &merged,
        );
        mark_unused(&mut words, &regions, &merged);
        pages.push(PageTrace {native_axes,native_rulings,
                native_styles,
                native_bounds,
                native_content_styles,
            width,
            height,
            layout,
            words,
            regions,
            text,
            doc_text,
            tables,
        });
    }
    crate::notice_boundaries::restore_framed_reading_order(&mut pages, rules);
    pages
}

/// 横倒しのページの罫線の表で、行を区切る罫線の無いまま 1 つのセルに複数の行が入っているか（「クボタ浄化槽」
/// 「〃」「〃」と型式が 3 行並ぶ欄、「交Ｎ26－79」「交Ｎ26－80」が 1 つの欄に入る表）。表示の行は回した座標の
/// 縦の列なので、セルの中の列の 2 番目以降に同上（〃）があるか、書き出しの 2 字が同じ列が 2 つあるとき。そうした
/// 表は罫線の表にせず行として組む（行の区切りの無い横組みの表は、縦組みの表の行の頭の決め方では分けられない）。
fn unruled_records(g: &Grid, cols: &[Col]) -> bool {
    let center = |c: &Col| (c.x + c.x1) / 2.0;
    // セルごとの、語の列の中心 x（右から）と、その列の先頭の語。
    let mut cells: Vec<Vec<(f32, String)>> = vec![Vec::new(); g.cells.len()];
    for c in cols {
        if let Some(i) = g.cell_at(center(c), (c.y + c.y1) / 2.0) {
            match cells[i].iter_mut().find(|(x, _)| (x - center(c)).abs() < 2.0) {
                Some(_) => {}
                None => cells[i].push((center(c), c.text.clone())),
            }
        }
    }
    for cell in &mut cells {
        cell.sort_by(|a, b| b.0.total_cmp(&a.0));
    }
    let head = |t: &str| t.chars().filter(|c| !c.is_whitespace()).take(2).collect::<String>();
    cells.iter().any(|cell| {
        // 1 件の欄の「〃」（認定を受けた者の名前も住所も上の件と同じ「〃」「〃」）は除く。
        (!cell.first().is_some_and(|(_, t)| t.starts_with('〃')) && cell.iter().skip(1).any(|(_, t)| t.starts_with('〃')))
            // 書き出しの 2 字が同じで、字数もほぼ同じ列（「交Ｎ26－79」「交Ｎ26－80」）。名前と住所（「沖縄県」「沖縄県那覇市泉崎」）は違う。
            || cell.iter().enumerate().any(|(i, (_, a))| {
                let n = |t: &str| t.chars().filter(|c| !c.is_whitespace()).count() as i32;
                // 同じ字の語（名称の末尾の「006ＨＺ」と型式の「006ＨＺ」）は 2 件ではない。
                // 数を含む番号のような短い語だけ（1 件の欄に並べた共同の出願者の住所「茨城県つくば市…」、同じ姓の名前「菅谷嘉晃」
                // 「菅谷利男」は 2 件ではない）。
                let numbered = a.chars().any(|c| c.is_ascii_digit() || ('０'..='９').contains(&c));
                head(a).chars().count() == 2 && n(a) <= 12 && numbered && cell[i + 1..].iter().any(|(_, b)| head(a) == head(b) && (n(a) - n(b)).abs() <= 2 && a.trim() != b.trim())
            })
    })
}

/// 横組みのページの罫線の枠が、表の行の途中を切っているか（決算公告の損益計算書で、小計の線と金額の欄の縦線だけで
/// 枠ができ、左の科目の欄は枠の外になる。号外第 224 号 51 ページ: 「人件費」と「8,572」が別々に組まれていた）。
/// 1 つのセルに、枠のすぐ左の枠の外の科目（字のある語）と同じ行（高さの中心が 2pt 以内）に並ぶ金額（数字を
/// 含む語）が 3 つ以上縦に積まれていれば（行を区切る罫線が無く、行は枠の外の科目から続いている）、罫線の表にせず
/// 行として組む。枠との間に縦の罫線のある語（隣の表の語）は数えない。
fn rows_cut_by_grid(g: &Grid, cols: &[Col], rules: Option<&PageRules>) -> bool {
    let Some(&gx0) = g.xs.first() else { return false };
    let (Some(&gy0), Some(&gy1)) = (g.ys.first(), g.ys.last()) else { return false };
    let yc = |c: &Col| (c.y + c.y1) / 2.0;
    let inside = |c: &Col| g.contains((c.x + c.x1) / 2.0, yc(c), 0.5);
    let figure = |c: &Col| c.text.chars().any(|ch| ch.is_ascii_digit());
    // 枠の中に科目（字のある語）がある表（決算公告の囲みの表は、科目の欄も枠の中）は見ない。
    let label = |c: &Col| c.text.chars().any(char::is_alphabetic);
    if cols.iter().any(|c| inside(c) && label(c)) {
        return false;
    }
    let walled = |c: &Col| rules.is_some_and(|r| r.vertical.iter().any(|v| c.x1 - 1.0 < v.x && v.x < gx0 - 2.0 && v.y0 <= yc(c) && yc(c) <= v.y1));
    let left: Vec<&Col> = cols.iter().filter(|c| !inside(c) && c.x1 <= gx0 + 1.0 && gy0 < yc(c) && yc(c) < gy1 && !walled(c)).collect();
    let mut per_cell: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for c in cols.iter().filter(|c| inside(c) && figure(c)) {
        // 同じ行で枠のすぐ左にある語が科目か。隣の決算公告の囲みの金額は数えない。
        if !left.iter().filter(|l| (yc(l) - yc(c)).abs() < 2.0).max_by(|a, b| a.x1.total_cmp(&b.x1)).is_some_and(|l| label(l)) {
            continue;
        }
        if let Some(i) = g.cell_at((c.x + c.x1) / 2.0, yc(c)) {
            *per_cell.entry(i).or_default() += 1;
        }
    }
    per_cell.values().any(|&n| n >= 3)
}

/// 表だけの本文の単位（前のページから続く新旧対照表の、改正後の帯の表と改正前の帯の表が段の右に
/// 並ぶもの）のすぐ後に見出しの無い新旧対照表の単位があれば、上の帯の表を改正後の欄の頭に、下の帯の表を
/// 改正前の欄の頭に移す（表だけが別の段に分かれ、表の外の本文として前の表の行と組めなかった）。
fn attach_band_tables(units: &mut Vec<Unit>, words: &[Word]) {
    let mut i = 0;
    while i + 1 < units.len() {
        let (Unit::Plain(lines), Unit::Shinkyu { after, before, preamble, .. }) = (&units[i], &units[i + 1]) else {
            i += 1;
            continue;
        };
        if !preamble.is_empty() || lines.is_empty() || !lines.iter().all(|l| l.table.is_some()) {
            i += 1;
            continue;
        }
        let ys = |ls: &[Line]| -> (f32, f32) {
            ls.iter()
                .flat_map(|l| l.words.iter().chain(l.table.iter().flat_map(|t| t.cells.iter().flat_map(|c| c.words.iter()))))
                .filter_map(|&w| words.get(w))
                .fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), w| (a.min(w.y0), b.max(w.y1)))
        };
        let (Some(a0), Some(b0)) = (after.first(), before.first()) else {
            i += 1;
            continue;
        };
        let (_, after_bottom) = ys(a0);
        let (before_top, _) = ys(b0);
        if !(after_bottom < before_top) {
            i += 1;
            continue;
        }
        let divider = (after_bottom + before_top) / 2.0;
        let sides: Vec<Option<bool>> = lines
            .iter()
            .map(|l| {
                let (top, bottom) = ys(std::slice::from_ref(l));
                if bottom <= divider { Some(true) } else if top >= divider { Some(false) } else { None }
            })
            .collect();
        if sides.iter().any(Option::is_none) || !sides.contains(&Some(true)) || !sides.contains(&Some(false)) {
            i += 1;
            continue;
        }
        let Unit::Plain(lines) = units.remove(i) else { unreachable!() };
        if let Unit::Shinkyu { after, before, .. } = &mut units[i] {
            let (up, down): (Vec<(Line, Option<bool>)>, Vec<(Line, Option<bool>)>) = lines.into_iter().zip(sides).partition(|(_, s)| *s == Some(true));
            after[0].splice(0..0, up.into_iter().map(|(l, _)| l));
            before[0].splice(0..0, down.into_iter().map(|(l, _)| l));
        }
    }
}

/// 横倒しのページの表の枠・セルの座標を、回した座標から表示の座標に戻す（語の表示用の座標と重ねるため）。
fn unturn_tables(units: &mut [Unit], page_width: f32) {
    let unturn = |x0: &mut f32, y0: &mut f32, x1: &mut f32, y1: &mut f32| {
        let (a, b, c, d) = (*x0, *y0, *x1, *y1);
        *x0 = b;
        *x1 = d;
        *y0 = page_width - c;
        *y1 = page_width - a;
    };
    for unit in units {
        let lines: Vec<&mut Line> = match unit {
            Unit::Plain(lines) => lines.iter_mut().collect(),
            Unit::Shinkyu { preamble, after, before, outside } => preamble
                .iter_mut()
                .chain(after.iter_mut().flatten())
                .chain(before.iter_mut().flatten())
                .chain(outside.iter_mut().flatten())
                .collect(),
        };
        for t in lines.into_iter().filter_map(|l| l.table.as_mut()) {
            unturn(&mut t.x0, &mut t.y0, &mut t.x1, &mut t.y1);
            for c in &mut t.cells {
                unturn(&mut c.x0, &mut c.y0, &mut c.x1, &mut c.y1);
            }
        }
    }
}

/// 横倒しのページの罫線を、語と同じく 90° 回す（表示の上端 → 元のページの右端、左端 → 上端）。
fn turn_rules(r: &PageRules, page_width: f32) -> PageRules {
    PageRules {
        enclosures: Vec::new(), short_fraction_bars: Vec::new(),
        vertical: r.horizontal.iter().map(|h| crate::lines::VRule { x: page_width - h.y, y0: h.x0, y1: h.x1 }).collect(),
        horizontal: r.vertical.iter().map(|v| crate::lines::HRule { y: v.x, x0: page_width - v.y1, x1: page_width - v.y0 }).collect(),
    }
}

/// 罫線を x の範囲 `lo..=hi` で切り取る（範囲の端の縦線は残す）。
fn clip_rules(r: &PageRules, lo: f32, hi: f32) -> PageRules {
    PageRules {
        enclosures: Vec::new(), short_fraction_bars: Vec::new(),
        vertical: r.vertical.iter().filter(|v| lo - 3.0 <= v.x && v.x <= hi + 3.0).copied().collect(),
        horizontal: r
            .horizontal
            .iter()
            .map(|h| crate::lines::HRule { x0: h.x0.max(lo), x1: h.x1.min(hi), ..*h })
            .filter(|h| h.x1 - h.x0 > 4.0)
            .collect(),
    }
}

/// 前のページの新旧対照表の続きで、紙面の中ほど（高さの 0.4〜0.6）に語の範囲の 7 割以上にわたる横線（改正後・
/// 改正前の帯の境目の線）があり、それをまたぐ語が無く上下に語があれば、その高さ（号外第 215 号 335 ページ:
/// 改正前の欄は「（略）」「（新設）」が多いが文もあり、上下の文が似ていない）。中央に最も近いものを採る。
fn rule_band_divider(cols: &[Col], rules: Option<&PageRules>, height: f32) -> Option<f32> {
    // 次の条・次の記事が始まるページ（改正の指示・「（傍線部分は改正部分）」・記事の標題）は表の続きだけのページではない。
    let instruction = |t: &str| t.contains("改正する。") || t.contains("改める。") || t.contains("傍線部分は改正部分") || crate::segment::is_heading_mark(t);
    if cols.iter().any(|c| instruction(&c.text)) {
        return None;
    }
    let (x0, x1) = cols.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), c| (a.min(c.x), b.max(c.x1)));
    let crossing = |y: f32| cols.iter().any(|c| c.y < y - 1.0 && c.y1 > y + 1.0);
    let above = |y: f32| cols.iter().filter(|c| c.y1 <= y).count();
    let below = |y: f32| cols.iter().filter(|c| c.y >= y).count();
    // 線は表の縦線のところで途切れて描かれることがあるので、同じ高さの線分の長さを合計する。
    let hs = &rules?.horizontal;
    let span = |y: f32| hs.iter().filter(|h| (h.y - y).abs() <= 1.5).map(|h| (h.x1.min(x1) - h.x0.max(x0)).max(0.0)).sum::<f32>();
    hs.iter()
        .filter(|h| (height * 0.4..=height * 0.6).contains(&h.y) && span(h.y) >= (x1 - x0) * 0.7)
        .map(|h| h.y)
        .filter(|&y| !crossing(y) && above(y) >= 3 && below(y) >= 3 && looks_like_before_band(cols, y))
        .min_by(|a, b| (a - height / 2.0).abs().total_cmp(&(b - height / 2.0).abs()))
}

/// `y` より下の語が改正前の欄らしいか: 3 割以上が省略・新設の印（「（略）」「（新設）」「〔同上〕」）か、
/// 上の語にも同じ字列がある（改正前の欄は改正後の欄と同じ文が多い）。様式の枠など、紙面の中ほどに横線が
/// あるだけの別のものと区別する。
fn looks_like_before_band(cols: &[Col], y: f32) -> bool {
    let compact = |t: &str| t.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    let upper: std::collections::HashSet<String> = cols.iter().filter(|c| c.y1 <= y).map(|c| compact(&c.text)).collect();
    let lower: Vec<String> = cols.iter().filter(|c| c.y >= y).map(|c| compact(&c.text)).collect();
    let mark = |t: &str| {
        let inner = t.strip_prefix(['（', '〔', '［']).and_then(|r| r.strip_suffix(['）', '〕', '］']));
        inner.is_some_and(|i| matches!(i, "新設" | "削る" | "略" | "同上" | "同左"))
    };
    let alike = lower.iter().filter(|t| mark(t) || (t.chars().count() >= 4 && upper.contains(*t))).count();
    !lower.is_empty() && alike * 10 >= lower.len() * 3
}

/// 前のページの新旧対照表の続きで、下の帯（改正前の欄）が「（新設）」「（削る）」「（略）」「〔同上〕」などの
/// 印の語だけのページの、上下の帯の境目（号外第 215 号 93 ページ: 新設の条が続き、改正前の欄は「（新設）」
/// だけ）。上下の帯の文が似ていないので [`detect_shinkyu_bands_headerless`] では見つからず、下の帯の印が
/// 上の帯の同じ列の続きとしてつながっていた。印より上に印でない語が 3 つ以上あり、印でない語がすべて
/// 印より上にあって、境目が紙面の中ほど（0.35〜0.7）にあるときだけ。
fn marker_band_divider(cols: &[Col], height: f32) -> Option<f32> {
    let marker = |c: &Col| {
        let t: String = c.text.chars().filter(|ch| !ch.is_whitespace()).collect();
        let inner = t.strip_prefix(['（', '〔', '［']).and_then(|r| r.strip_suffix(['）', '〕', '］']));
        inner.is_some_and(|i| matches!(i, "新設" | "削る" | "略" | "同上" | "同左"))
    };
    let (marks, body): (Vec<&Col>, Vec<&Col>) = cols.iter().partition(|c| marker(c));
    let marks: Vec<&Col> = marks.into_iter().filter(|c| (c.y + c.y1) / 2.0 > height * 0.35).collect();
    if marks.is_empty() || body.len() < 3 {
        return None;
    }
    let mark_top = marks.iter().map(|c| c.y).fold(f32::INFINITY, f32::min);
    let body_bottom = body.iter().map(|c| c.y1).fold(f32::NEG_INFINITY, f32::max);
    if body_bottom > mark_top - 2.0 {
        return None;
    }
    let divider = (body_bottom + mark_top) / 2.0;
    (height * 0.35..=height * 0.7).contains(&divider).then_some(divider)
}

/// 見出しの無い新旧対照表のページで、帯の境目 `divider` をまたぐ長い列（3 列以上）があれば、
/// その右端より左を表の外として分ける（表は境目で上下の帯に分かれるので、境目をまたいで続く
/// 列は表の中には無い）。その右に帯の枠の左辺（境目をまたぐ縦の罫線か、境目の横の罫線の
/// 左端）があれば、そこより左を表の外にする（境目をまたがない附則の見出しなどの短い列も表の外になる）。右に上下の帯が
/// それぞれ残るときだけ分ける。`(表, 表の外)`。
fn split_off_full_height_left(
    cols: Vec<Col>,
    divider: f32,
    rules: Option<&PageRules>,
    grids: &[Grid],
) -> (Vec<Col>, Vec<Col>) {
    let mut widths: Vec<f32> = cols.iter().map(|c| c.x1 - c.x).collect();
    widths.sort_by(f32::total_cmp);
    let em = widths.get(widths.len() / 2).copied().unwrap_or(10.0);
    let mut crossing: Vec<&Col> = cols
        .iter()
        .filter(|c| c.y < divider - em * 2.0 && c.y1 > divider + em * 2.0)
        .collect();
    if crossing.is_empty() {
        return (cols, Vec::new());
    }
    let top = cols.iter().map(|c| c.y).fold(f32::INFINITY, f32::min);
    let bottom = cols.iter().map(|c| c.y1).fold(f32::NEG_INFINITY, f32::max);
    let boundary_of = |candidates: &[&Col]| {
        let mut boundary = candidates
            .iter()
            .map(|c| c.x1)
            .fold(f32::NEG_INFINITY, f32::max);
        // 縦の罫線は帯の高さいっぱいのもの（区分見出し「法規的告示」の囲みの罫線は使わない）。
        let verticals = rules
            .into_iter()
            .flat_map(|r| &r.vertical)
            .filter(|v| {
                v.y0 < divider - em * 4.0
                    && v.y1 > divider + em * 4.0
                    && v.y1 - v.y0 >= (bottom - top) * 0.6
            })
            .map(|v| v.x);
        let horizontals = rules
            .into_iter()
            .flat_map(|r| &r.horizontal)
            .filter(|h| (h.y - divider).abs() < em * 3.0 && h.x1 - h.x0 > em * 5.0)
            .map(|h| h.x0);
        if let Some(frame) = verticals
            .chain(horizontals)
            .filter(|&x| x > boundary)
            .reduce(f32::min)
        {
            boundary = frame;
        }
        boundary
    };
    let initial_boundary = boundary_of(&crossing);
    let right_columns: Vec<&Col> = cols
        .iter()
        .filter(|c| (c.x + c.x1) / 2.0 > initial_boundary + 1.0)
        .collect();
    // A source frame shared with every right-hand column is counterevidence
    // for an independent outside article. Center containment only holds the
    // current interpretation; it never authorizes a new ownership assignment.
    // Native advance rectangles may overhang the ruling while their ink stays
    // inside, so failing full-rectangle containment is not outside evidence.
    if !right_columns.is_empty() {
        crossing.retain(|c| {
            !grids.iter().any(|g| {
                g.cells.len() >= 2
                    && right_columns
                        .iter()
                        .all(|r| g.contains((r.x + r.x1) / 2., (r.y + r.y1) / 2., 0.))
                    && g.contains((c.x + c.x1) / 2., (c.y + c.y1) / 2., 0.)
            })
        });
        if crossing.is_empty() {
            return (cols, Vec::new());
        }
    }
    let boundary = boundary_of(&crossing);
    let right = |c: &Col| (c.x + c.x1) / 2.0 > boundary + 1.0;
    let center = |c: &Col| (c.y + c.y1) / 2.0;
    let upper = cols
        .iter()
        .filter(|c| right(c) && center(c) < divider)
        .count();
    let lower = cols
        .iter()
        .filter(|c| right(c) && center(c) >= divider)
        .count();
    if upper < 3 || lower < 3 {
        return (cols, Vec::new());
    }
    cols.into_iter().partition(right)
}

/// [`split_off_leading_frame`] の結果（改正後, 改正前, 残り, 枠内に対応する欄見出しがあるか）。
type LeadingFrame = (Vec<Col>, Vec<Col>, Vec<Col>, bool);

/// 欄の左の語（`rest`）に、上の欄の下端より下まで続く列と、下の欄の上端より上から始まる列があるか（左の記事が
/// 上下の欄の境目をまたいで続く）。
fn spans_divider(rest: &[Col], after: &[Col], before: &[Col]) -> bool {
    let after_bottom = after.iter().map(|c| c.y1).fold(f32::NEG_INFINITY, f32::max);
    let before_top = before.iter().map(|c| c.y).fold(f32::INFINITY, f32::min);
    rest.iter().any(|c| c.y < after_bottom && c.y1 > before_top)
}

/// ページの右端にある、前のページの新旧対照表の続きの欄を切り出す。`(改正後, 改正前, 残り)`。
///
/// 続きの欄は、紙面の高さのほとんどを通る縦の罫線の右にあり、その縦線から右へ伸びる横の罫線で
/// 上下（改正後・改正前）に分かれていて、欄見出しが無い（g00160 0059: 右端の上の欄に改正後の
/// 「５　前項の場合には…」、下の欄に改正前の「〔項を加える。〕」。その左に備考・附則・次の記事が続く）。
/// 罫線の枠では取れない（下の段の次の記事の表の横線が、この欄の縦線まで伸びて 1 つの枠になる）ので、
/// 罫線を直接見る。縦線の左に字が無い（ページ全体が表の続き）なら切り出さない（見出しの無い継続ページとして読む）。
/// 明示の「改正後／改正前」が枠の上・下に揃う場合も、左の記事を読む前に枠全体を切り出す。
/// 単独の「前」「後」だけがある枠は、これまでどおり候補から除く。
fn split_off_leading_frame(cols: Vec<Col>, rules: Option<&PageRules>) -> Result<LeadingFrame, Vec<Col>> {
    let Some(rules) = rules else {
        return Err(cols);
    };
    let top = cols.iter().map(|c| c.y).fold(f32::INFINITY, f32::min);
    let bottom = cols.iter().map(|c| c.y1).fold(f32::NEG_INFINITY, f32::max);
    let right_edge = cols.iter().map(|c| c.x1).fold(f32::NEG_INFINITY, f32::max);
    let center = |c: &Col| ((c.x + c.x1) / 2.0, (c.y + c.y1) / 2.0);
    let mut verticals: Vec<_> = rules.vertical.iter().filter(|v| v.y1 - v.y0 >= (bottom - top) * 0.8 && v.x < right_edge).collect();
    verticals.sort_by(|a, b| b.x.total_cmp(&a.x));
    // 横線は途中で切れて引かれることがあるので、同じ高さで続く線はつなぐ。
    let mut horizontal: Vec<(f32, f32, f32)> = rules.horizontal.iter().map(|h| (h.y, h.x0, h.x1)).collect();
    horizontal.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let mut joined: Vec<(f32, f32, f32)> = Vec::new();
    for h in horizontal {
        match joined.last_mut() {
            Some(last) if (last.0 - h.0).abs() <= 1.0 && h.1 <= last.2 + 2.0 => last.2 = last.2.max(h.2),
            _ => joined.push(h),
        }
    }
    let is_label_word = |c: &Col, hx: f32| {
        let text: String = c.text.chars().filter(|ch| !ch.is_whitespace()).collect();
        (c.x - hx).abs() < 6.0 && matches!(text.as_str(), "改" | "正" | "後" | "前" | "改正" | "正後" | "正前" | "改正後" | "改正前")
    };
    let found = verticals.into_iter().find_map(|v| {
        // 縦線の左端から右へ、右端の字の上まで伸びる横線が上下の欄の境目。
        let &(hy, _, _) = joined.iter().find(|&&(y, x0, x1)| (x0 - v.x).abs() <= 3.0 && x1 >= right_edge - 2.0 && y > v.y0 + (v.y1 - v.y0) * 0.25 && y < v.y1 - (v.y1 - v.y0) * 0.25)?;
        let right: Vec<&Col> = cols.iter().filter(|c| center(c).0 > v.x && center(c).1 > v.y0 && center(c).1 < v.y1).collect();
        // 境目をまたぐ列は無い（またぐなら上下の欄ではない）。
        if right.iter().any(|c| c.y < hy - 2.0 && c.y1 > hy + 2.0) {
            return None;
        }
        let upper = right.iter().filter(|c| center(c).1 < hy).count();
        let lower = right.len() - upper;
        let label = right.iter().any(|c| matches!(c.text.chars().filter(|ch| !ch.is_whitespace()).collect::<String>().as_str(), "改正後" | "改正前" | "改" | "正" | "後" | "前"));
        let right_cols: Vec<Col> = right.into_iter().cloned().collect();
        // 枠の上下に同じ x で正式な欄見出しが揃い、その境目が中央横線と対応するときだけ、
        // 見出し付きの枠を先に出す。本文中の「前」「後」や上下が逆のラベルは根拠にしない。
        let header = detect_shinkyu_header(&right_cols).filter(|&(hx, divider)| {
            let in_band = |last, upper: bool| {
                vertical_labels(&right_cols, last).iter().any(|&(x0, _, y0, y1)| {
                    (x0 - hx).abs() < 6.0 && if upper { y1 < hy } else { y0 > hy }
                })
            };
            (divider - hy).abs() <= 12.0
                && in_band('後', true)
                && in_band('前', false)
                && right_cols.iter().any(|c| center(c).1 < hy && !is_label_word(c, hx))
                && right_cols.iter().any(|c| center(c).1 >= hy && !is_label_word(c, hx))
        });
        let has_left_content = cols.iter().any(|c| c.x1 < v.x);
        (upper > 0 && lower > 0 && (!label || header.is_some()) && has_left_content).then_some((v.x, v.y0, v.y1, hy, header.map(|h| h.0)))
    });
    let Some((x, y0, y1, mid, header_x)) = found else {
        return Err(cols);
    };
    let (mut after, mut before, mut rest) = (Vec::new(), Vec::new(), Vec::new());
    for c in cols {
        let (cx, cy) = center(&c);
        if cx <= x || cy <= y0 || cy >= y1 {
            rest.push(c);
        } else {
            // band_pair_unit は欄見出しを補うため、検出済みの見出し列の語だけを取り除く。
            // 同じ「前」「後」でも、本文の別の列にある語は保持する。
            if header_x.is_some_and(|hx| is_label_word(&c, hx)) {
                continue;
            }
            if cy < mid {
                after.push(c);
            } else {
                before.push(c);
            }
        }
    }
    Ok((after, before, rest, header_x.is_some()))
}

/// 縦組みの欄見出し（「変更後」「改正後」。1 字ずつの語「変」「更」「後」も）の左右の範囲と上端・下端。同じ x に
/// 並ぶ 3 字までの語を、間が 80pt 以内なら上からつないで見る。
fn vertical_labels(cols: &[Col], last: char) -> Vec<(f32, f32, f32, f32)> {
    let mut short: Vec<&Col> = cols.iter().filter(|c| c.text.chars().filter(|ch| !ch.is_whitespace()).count() <= 3).collect();
    short.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    let mut out = Vec::new();
    for (i, c) in short.iter().enumerate() {
        let mut text = String::new();
        let mut end = c.y;
        for d in short[i..].iter().filter(|d| (d.x - c.x).abs() <= 1.5 && d.y >= c.y) {
            if d.y - end > 80.0 {
                break;
            }
            text.extend(d.text.chars().filter(|ch| !ch.is_whitespace()));
            end = d.y1;
            if text.chars().count() >= 3 {
                break;
            }
        }
        if text == format!("変更{last}") || text == format!("改正{last}") {
            out.push((c.x, c.x1, c.y, end));
        }
    }
    out
}

/// [`split_label_frames`] の枠 `(左端, 右端, 上端, 下端, 上下の欄の境)`。
fn label_frames(tier: &[Col], hrules: &[(f32, f32, f32)], vrules: &[(f32, f32, f32)]) -> Vec<(f32, f32, f32, f32, f32)> {
    let after = vertical_labels(tier, '後');
    let before = vertical_labels(tier, '前');
    after
        .iter()
        .filter_map(|a| {
            let b = before.iter().find(|b| (b.0 - a.0).abs() <= 1.5 && b.2 > a.3)?;
            let &(y, x0, x1) = hrules
                .iter()
                .filter(|&&(y, x0, x1)| y > a.3 && y < b.2 && x0 <= a.0 && x1 >= a.1)
                .max_by(|p, q| (p.2 - p.1).total_cmp(&(q.2 - q.1)))?;
            let side = |x: f32| vrules.iter().find(|v| (v.0 - x).abs() <= 3.0 && v.1 < y && v.2 > y).map(|v| (v.1, v.2));
            let (top, bottom) = match (side(x0), side(x1)) {
                (Some(l), Some(r)) => (l.0.min(r.0), l.1.max(r.1)),
                _ => (f32::NEG_INFINITY, f32::INFINITY),
            };
            let crosses = tier.iter().any(|c| {
                let edge = |x: f32| c.x < x - 3.0 && c.x1 > x + 3.0;
                let inside = c.x1 > x0 && c.x < x1;
                edge(x0) || edge(x1) || (inside && (c.y < top - 2.0 || c.y1 > bottom + 2.0)) || (inside && c.y < y - 1.0 && c.y1 > y + 1.0)
            });
            (!crosses).then_some((x0, x1, top, bottom, y))
        })
        .collect()
}

/// 縦組みの段の中の、横の罫線で上の欄（欄見出し「変更後」）と下の欄（「変更前」）に分けた枠を切り出す（航空灯火の
/// 告示の変更の表: 上の欄の見出し・表と下の欄の見出し・表が同じ x に並び、同じ x の見出しの列が上下の欄で 1 行に
/// つながって「変更後変更前」と読まれていた）。
///
/// 欄見出しの組が同じ x にあり、その間に見出しの左右に掛かる横の罫線があれば、その線の左右の範囲を枠とする。枠の
/// 上下の端は、線の両端を通る縦の罫線（無ければ段の上下の端）。枠の左右の境をまたぐ語や、枠の上・下にはみ出す
/// 語があれば切らない。戻り値は読む順の部分（枠の右・上の欄・下の欄・枠の左）。枠の左右は続けて同じように見る。
pub(crate) fn split_label_frames(tier: Vec<Col>, hrules: &[(f32, f32, f32)], vrules: &[(f32, f32, f32)]) -> Vec<Vec<Col>> {
    let found = label_frames(&tier, hrules, vrules).first().map(|f| (f.0, f.1, f.4));
    let Some((x0, x1, mid)) = found else {
        return vec![tier];
    };
    let (mut right, mut upper, mut lower, mut left) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for c in tier {
        let (cx, cy) = ((c.x + c.x1) / 2.0, (c.y + c.y1) / 2.0);
        if cx > x1 {
            right.push(c);
        } else if cx < x0 {
            left.push(c);
        } else if cy < mid {
            upper.push(c);
        } else {
            lower.push(c);
        }
    }
    let mut parts = if right.is_empty() { Vec::new() } else { split_label_frames(right, hrules, vrules) };
    parts.push(upper);
    parts.push(lower);
    if !left.is_empty() {
        parts.extend(split_label_frames(left, hrules, vrules));
    }
    parts
}

/// 号の 1 ページ目の題字の脇の欄（「（号外）」「発行　内閣府」「（原稿作成　国立印刷局）」）が、横倒しのページで本文と
/// 逆の向きに組まれ、語の順が崩れて読まれた（「内閣府」「国立印刷局）」「外）」「（原稿作成」「（号発行」）ものを、
/// 欄の順に組み直す。ページの頭から 40 行の中で、欄の語（「（号」「外）」「発行」「内閣府」「（原稿作成」「国立印刷局）」）
/// だけでできた行が、どの語もちょうど 1 度ずつ含むときだけ。
/// 記事の見出し（「〇こども家庭庁告示第十三号」）のすぐ後に区分見出し（「法規的告示」）が来ていれば、区分見出しを先にする
/// （見出しの下に本文が無いまま次の区分が始まることは無い。区分見出しは本文より大きな字で、見出しの列と上下にずれて
/// 組まれ、見出しが前の段に、区分見出しが次の段の頭に分かれていた）。段の境をまたぐものも見る。
fn order_section_labels(units: &mut [Unit], words: &[Word]) {
    let compact = |l: &Line| l.text.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    // 「〇」で始まり「号」で終わる 1 行（機関名がかなで始まる「〇こども家庭庁告示第十三号」も）。
    let heading = |l: &Line| {
        let t = compact(l);
        crate::segment::is_heading_mark(&t) || (t.starts_with('〇') && t.ends_with('号') && t.contains('第') && t.chars().count() <= 30)
    };
    let label = |l: &Line| crate::segment::is_section_label(&compact(l));
    for u in units.iter_mut() {
        if let Unit::Plain(lines) = u {
            for i in 1..lines.len() {
                if heading(&lines[i - 1]) && label(&lines[i]) {
                    lines.swap(i - 1, i);
                }
            }
        }
    }
    for k in 1..units.len() {
        let (head, tail) = units.split_at_mut(k);
        let Unit::Plain(prev) = &mut head[k - 1] else { continue };
        let next = match &mut tail[0] {
            Unit::Plain(lines) => lines,
            Unit::Shinkyu { preamble, .. } => preamble,
        };
        // 大きな区分見出しが小さな欄見出しのすぐ右に組まれ、上下にずれて別の段になった場合。
        // 直前の欄見出しと短い標題だけの前へ移す（本文や離れた別記事は越えない）。
        let bounds = |l: &Line| {
            let ws: Option<Vec<_>> = l.words.iter().map(|&id| words.get(id).filter(|w| w.id == id)).collect();
            let ws = ws.filter(|ws| !ws.is_empty())?;
            Some(ws.iter().fold((f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY),
                |(x0,y0,x1,y1), w| (x0.min(w.x0), y0.min(w.y0), x1.max(w.x1), y1.max(w.y1))))
        };
        let raised = next.first().filter(|l| label(l)).and_then(|primary| {
            let at = prev.iter().rposition(|l| label(l))?;
            if prev.len() - at > 3 || compact(primary) == compact(&prev[at]) { return None; }
            let (a, b) = (bounds(primary)?, bounds(&prev[at])?);
            let (large, em) = (a.2-a.0, b.2-b.0);
            let overlap = |a: (f32,f32,f32,f32), b: (f32,f32,f32,f32)| a.3.min(b.3)-a.1.max(b.1);
            if large < em*1.3 || a.3-a.1 < large*2.0 || b.3-b.1 < em*2.0
                || a.0 < b.2 || a.0-b.2 > em*3.0 || overlap(a,b) < em { return None; }
            prev[at+1..].iter().all(|l| {
                let t=compact(l);
                bounds(l).is_some_and(|h| t.chars().count() <= 30 && !t.contains(['。','、','：',':'])
                    && h.2 <= b.0+em*0.5 && b.0-h.0 <= em*3.0 && overlap(h,b) >= em)
            }).then_some(at)
        });
        if let Some(at) = raised {
            prev.insert(at, next.remove(0));
            continue;
        }
        if prev.last().is_some_and(|l| heading(l)) && next.first().is_some_and(|l| label(l)) {
            let l = next.remove(0);
            let at = prev.len() - 1;
            prev.insert(at, l);
        }
    }
}

fn order_masthead(units: &mut [Unit]) {
    // 「（号外）」は「（号」「外）」の 2 つ分。
    const PARTS: [&str; 7] = ["（号外）", "（号", "外）", "発行", "内閣府", "（原稿作成", "国立印刷局）"];
    // 行の字を欄の語に分ける（分けられなければ None）。
    fn split(t: &str) -> Option<Vec<usize>> {
        if t.is_empty() {
            return Some(Vec::new());
        }
        PARTS.iter().enumerate().find_map(|(k, p)| t.strip_prefix(p).and_then(split).map(|mut rest| {
            rest.insert(0, k);
            rest
        }))
    }
    let compact = |l: &Line| l.text.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    // 題字の脇の欄は最初の段にあるとは限らず（横倒しの 1 ページ目では目次の下の段に入る）、段をまたいで分かれることもある。
    // （段の番号, 行の番号, 欄の語）。
    let mut found: Vec<(usize, usize, Vec<usize>)> = Vec::new();
    // 欄の語の後ろに目次の項目の字が続いた行（「国立印刷局）〇」「国立印刷局）　係」）は、頭の欄の語だけを取る（行の残りはそのまま）。
    // （段の番号, 行の番号, 欄の語, 頭の字数）。
    let mut prefixed: Vec<(usize, usize, usize, usize)> = Vec::new();
    let mut seen_lines = 0;
    for (u, unit) in units.iter().enumerate() {
        let Unit::Plain(lines) = unit else { continue };
        for (i, l) in lines.iter().enumerate() {
            if seen_lines + i >= 120 {
                break;
            }
            let c = compact(l);
            if let Some(p) = split(&c).filter(|p| !p.is_empty()) {
                found.push((u, i, p));
            } else if let Some(k) = (0..PARTS.len()).find(|&k| {
                // 欄の語の直後は空白か「〇」。括弧の付いた語だけ（「内閣府」「発行」は目次の項目「内閣府　一三七」にもある）。
                PARTS[k].contains(['（', '）'])
                    && l.text.trim_start().strip_prefix(PARTS[k]).is_some_and(|rest| rest.starts_with(['\u{3000}', ' ', '〇']))
                    && l.words.len() >= 2
            }) {
                prefixed.push((u, i, k, PARTS[k].chars().count()));
            }
        }
        seen_lines += lines.len();
    }
    let seen_with_prefixed: Vec<usize> = found
        .iter()
        .flat_map(|(_, _, p)| p.clone())
        .chain(prefixed.iter().map(|p| p.2))
        .flat_map(|k| if k == 0 { vec![1, 2] } else { vec![k] })
        .collect();
    let mut all: Vec<usize> = seen_with_prefixed.clone();
    all.sort_unstable();
    all.dedup();
    if all == (1..PARTS.len()).collect::<Vec<_>>() && !prefixed.is_empty() {
        // 頭の欄の語を行から外し、欄の語だけの行として扱う。
        for &(u, i, k, n) in prefixed.iter().rev() {
            if let Unit::Plain(lines) = &mut units[u] {
                let l = &mut lines[i];
                let first = l.words.remove(0);
                let rest: String = l.text.trim_start().chars().skip(n).collect();
                l.text = rest.trim_start_matches(['\u{3000}', ' ']).to_string();
                lines.insert(i, Line::text(PARTS[k].to_string(), vec![first]));
            }
        }
        // 外した行を加えて、もう一度集める。
        return order_masthead(units);
    }
    let mut seen: Vec<usize> = found.iter().flat_map(|(_, _, p)| p.iter().flat_map(|&k| if k == 0 { vec![1, 2] } else { vec![k] })).collect();
    seen.sort_unstable();
    seen.dedup();
    if seen != (1..PARTS.len()).collect::<Vec<_>>() {
        return;
    }
    // 既に欄の順に組めている（「（号外）」「発行内閣府」「（原稿作成国立印刷局）」の行が同じ段に続く）なら何もしない。
    let line_of = |u: usize, i: usize| match &units[u] {
        Unit::Plain(lines) => compact(&lines[i]),
        _ => String::new(),
    };
    let texts: Vec<String> = found.iter().map(|(u, i, _)| line_of(*u, *i)).collect();
    let consecutive = found.windows(2).all(|w| w[1].0 == w[0].0 && w[1].1 == w[0].1 + 1);
    if consecutive && texts == ["（号外）", "発行内閣府", "（原稿作成国立印刷局）"] {
        return;
    }
    let (at_u, at_i) = (found[0].0, found[0].1);
    let mut ids: Vec<usize> = Vec::new();
    for (u, i, _) in found.iter().rev() {
        if let Unit::Plain(lines) = &mut units[*u] {
            ids.extend(lines.remove(*i).words);
        }
    }
    let rebuilt = [
        Line::text("（号外）".to_string(), ids),
        Line::text("発行\u{3000}内閣府".to_string(), Vec::new()),
        Line::text("（原稿作成\u{3000}国立印刷局）".to_string(), Vec::new()),
    ];
    if let Unit::Plain(lines) = &mut units[at_u] {
        for (k, l) in rebuilt.into_iter().enumerate() {
            lines.insert((at_i + k).min(lines.len()), l);
        }
    }
}

/// 共同省令の題名（「〇内閣府・総務省・文部科学省令第五号」）は府省の名前を小さい字で 2〜3 列に割って組むので、真ん中の
/// 名前だけが「〇」「令第五号」の列に入り、ほかの名前が前後の行に分かれる（「内閣府」「〇総務省令第五号」「文部科学省」）。
/// 前後の行が府省の名前だけなら、「・」でつないで 1 行の題名にする。
fn join_joint_ministries(units: &mut [Unit]) {
    fn ministry(t: &str) -> bool {
        let n = t.chars().count();
        (2..=8).contains(&n) && (t.ends_with('省') || t.ends_with('府') || t.ends_with('庁')) && !t.chars().any(|c| "〇（）()、。・".contains(c))
    }
    let compact = |l: &Line| l.text.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    for unit in units.iter_mut() {
        match unit {
            Unit::Plain(lines) => join_ministry_lines(lines, &compact, ministry),
            Unit::Shinkyu { preamble, outside, .. } => {
                join_ministry_lines(preamble, &compact, ministry);
                outside.iter_mut().for_each(|p| join_ministry_lines(p, &compact, ministry));
            }
        }
    }
}

fn join_ministry_lines(lines: &mut Vec<Line>, compact: &dyn Fn(&Line) -> String, ministry: fn(&str) -> bool) {
    let mut i = 0;
    while i < lines.len() {
        let t = compact(&lines[i]);
        let Some(rest) = t.strip_prefix('〇') else {
            i += 1;
            continue;
        };
        let Some(at) = ["令第", "規則第", "告示第", "訓令第"].iter().filter_map(|k| rest.find(k)).min() else {
            i += 1;
            continue;
        };
        let (mid, tail) = rest.split_at(at);
        // 府省の名前を読点で並べた行（9 府省の共同告示「内閣府、総務省、財務省、」「〇文部科学省、…農林水産省、告示第一号」
        // 「経済産業省、国土交通省、環境省」）も名前の並び。
        let names_of = |t: &str| -> Option<Vec<String>> {
            let items: Vec<&str> = t.trim_end_matches('、').split('、').collect();
            items.iter().all(|n| ministry(n)).then(|| items.iter().map(|n| n.to_string()).collect())
        };
        let mid_names = if mid.is_empty() { Some(Vec::new()) } else { names_of(mid) };
        // 2 つの府省の名前を割って組んだもの（「デジタル庁」「〇　告示第三十二号」「総務省」）は、真ん中の行に名前が無い。
        if mid_names.is_none() || !tail.ends_with('号') {
            i += 1;
            continue;
        }
        let before = i > 0 && lines[i - 1].table.is_none() && names_of(&compact(&lines[i - 1])).is_some();
        let after = i + 1 < lines.len() && lines[i + 1].table.is_none() && names_of(&compact(&lines[i + 1])).is_some();
        if (!before && !after) || (mid.is_empty() && !(before && after)) {
            i += 1;
            continue;
        }
        let mut names: Vec<String> = Vec::new();
        let mut words: Vec<usize> = Vec::new();
        let first = if before { i - 1 } else { i };
        let last = if after { i + 1 } else { i };
        for k in first..=last {
            words.extend(lines[k].words.iter().copied());
            if k == i {
                names.extend(mid_names.clone().unwrap_or_default());
            } else {
                names.extend(names_of(&compact(&lines[k])).unwrap_or_default());
            }
        }
        // 読点で並べた名前の行は、そのまま続ける（字を変えない。「内閣府、総務省、財務省、文部科学省、…環境省告示第一号」）。
        let listed = (first..=last).any(|k| compact(&lines[k]).contains('、'));
        let joined = if listed {
            let parts: Vec<String> = (first..=last).map(|k| if k == i { mid.to_string() } else { compact(&lines[k]) }).collect();
            Line::text(format!("〇{}{}", parts.concat(), tail), words)
        } else {
            Line::text(format!("〇{}{}", names.join("・"), tail), words)
        };
        lines.splice(first..=last, [joined]);
        i = first + 1;
    }
}

/// 上下に続けて組まれた罫線の表（同じ欄の数）で、下の表の 1 行目の頭の欄（登録番号の欄）が空なら、上の表の最後の行の
/// 欄の中の折り返し（「NCTアグリ株式会」「社」、「東京都千代田区外神田」「一丁目18番13号」）なので、上の表の最後の行の
/// 各欄の後ろにつなぐ（表の枠が行の途中で切れて 2 つの表になり、折り返しが別の行になっていた）。
fn merge_split_table_rows(units: &mut [Unit]) {
    let merge = |lines: &mut Vec<Line>| {
        let mut i = 1;
        while i < lines.len() {
            let (head, tail) = lines.split_at_mut(i);
            let (Some(a), Some(b)) = (head[i - 1].table.as_mut(), tail[0].table.as_mut()) else {
                i += 1;
                continue;
            };
            let filled = |t: &crate::trace::Table, row: usize| t.cells.iter().any(|c| c.row == row && c.col == 0 && !c.text.trim().is_empty());
            let last = a.rows.saturating_sub(1);
            let first: Vec<usize> = (0..b.cells.len()).filter(|&k| b.cells[k].row == 0 && !b.cells[k].text.trim().is_empty()).collect();
            let continued = a.cols == b.cols
                && a.cols >= 3
                && a.rows >= 2
                && b.rows >= 2
                && filled(a, last)
                && !filled(b, 0)
                && filled(b, 1)
                && !first.is_empty()
                // 上の欄は折り返すほどの長さの字（数だけの欄の下の「乗率」は次の表の見出し）。
                && first.iter().all(|&k| {
                    b.cells[k].text.chars().count() <= 20
                        && a.cells.iter().any(|c| {
                            c.row == last
                                && c.col == b.cells[k].col
                                && c.text.chars().count() >= 6
                                && !c.text.chars().all(|ch| ch.is_ascii_digit() || ".,△ 　".contains(ch))
                        })
                });
            if !continued {
                i += 1;
                continue;
            }
            for &k in &first {
                let (col, text, words) = (b.cells[k].col, b.cells[k].text.clone(), b.cells[k].words.clone());
                if let Some(c) = a.cells.iter_mut().find(|c| c.row == last && c.col == col) {
                    c.text.push_str(&text);
                    c.words.extend(words);
                }
            }
            b.cells.retain(|c| c.row != 0);
            for c in &mut b.cells {
                c.row -= 1;
            }
            b.rows -= 1;
            let (ta, tb) = (a.render(), b.render());
            head[i - 1].text = ta;
            tail[0].text = tb;
            i += 1;
        }
    };
    for unit in units.iter_mut() {
        match unit {
            Unit::Plain(lines) => merge(lines),
            Unit::Shinkyu { preamble, after, before, outside } => {
                merge(preamble);
                after.iter_mut().chain(before.iter_mut()).chain(outside.iter_mut()).for_each(|p| merge(p));
            }
        }
    }
}

/// 新旧対照表の改正後の欄の終わりに組まれた、表の後ろの備考（「備考　表中の〔　〕の記載は注記である。」）と附則（「附　則」と
/// 「…から施行する。」）を、改正前の欄の後ろ（表の外）へ移す（表の続きの欄の左に、上の帯の高さで組まれた備考・附則が
/// 改正後の欄に入り、改正後と改正前の間に読まれていた）。改正前の欄に字のあるときだけ。
fn move_trailing_notes(units: &mut [Unit]) {
    let text = |l: &Line| l.text.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    // 欄の終わりから、備考（折り返した続きの「…は注記である。」も）・附則・施行の行だけが続く所まで。備考・附則の
    // 行が無ければ None。
    let tail_of = |part: &[Line]| -> Option<usize> {
        let mut k = part.len();
        while k > 0 {
            let t = text(&part[k - 1]);
            let wrapped = t.ends_with("注記である。") && k >= 2 && text(&part[k - 2]).starts_with("備考");
            if t.starts_with("備考") || wrapped || t == "附則" || t.contains("から施行する") {
                k -= 1;
            } else {
                break;
            }
        }
        let tail: Vec<String> = part[k..].iter().map(text).collect();
        let noted = tail.iter().any(|t| t.starts_with("備考") && tail.iter().any(|u| u.contains("注記である")))
            || tail.windows(2).any(|w| w[0] == "附則" && w[1].contains("施行する"));
        (k < part.len() && noted).then_some(k)
    };
    for unit in units.iter_mut() {
        let Unit::Shinkyu { after, before, outside, .. } = unit else { continue };
        if before.iter().all(|p| p.iter().all(|l| l.text.trim().is_empty())) {
            continue;
        }
        let mut moved: Vec<Line> = Vec::new();
        if let Some(last) = after.last_mut() {
            if let Some(k) = tail_of(last) {
                moved.extend(last.drain(k..));
            }
        }
        // 改正前の欄の終わりの附則（備考は表の外にある）も。
        if let Some(last) = before.last_mut() {
            if let Some(k) = tail_of(last) {
                moved.extend(last.drain(k..));
            }
        }
        if moved.is_empty() {
            continue;
        }
        // 表の外の頭にある備考は附則より前（備考は表のすぐ下、附則はその後ろ）。
        let notes = outside.first().map_or(0, |p| p.iter().take_while(|l| text(l).starts_with("備考") || text(l).ends_with("注記である。")).count());
        let moved_has_note = moved.iter().any(|l| text(l).starts_with("備考"));
        if notes > 0 && !moved_has_note {
            let first = &mut outside[0];
            let rest: Vec<Line> = first.drain(notes..).collect();
            first.extend(moved);
            first.extend(rest);
        } else {
            outside.insert(0, moved);
        }
    }
}

/// 段の字が紙面の上半分（改正後の帯）にだけあり、新しい記事（「〇」で始まる行）も附則も欄見出しも無いか。
fn after_band_only(tier: &[Col], height: f32) -> bool {
    if tier.len() < 3 || detect_shinkyu_header(tier).is_some() {
        return false;
    }
    let bottom = tier.iter().map(|c| c.y1).fold(f32::NEG_INFINITY, f32::max);
    // 表の後ろの附則（改正する法令自身の附則）がページの上に来ただけのページは違う。
    let other = |l: &Line| {
        let t: String = l.text.chars().filter(|c| !c.is_whitespace()).collect();
        crate::segment::is_heading_mark(&l.text) || t.starts_with("附則")
    };
    let lines = page_lines(tier.to_vec());
    // 改正前の欄の印（「〔同上〕」「［同左］」）があれば改正後の帯だけではない（左右に並べた様式など）。
    let before_marks = lines.iter().any(|l| l.text.contains("同上") || l.text.contains("同左"));
    // 改正の指示（「〜の一部を次のように改正する」「様式を次のように改める」）は改め文の本文で、改正後の
    // 欄（法令自身の文）には出ない（様式を改める条の後に様式の図だけが続くページ）。
    let text: String = lines.iter().flat_map(|l| l.text.chars()).filter(|c| !c.is_whitespace()).collect();
    let instruction = text.contains("の一部を次のように改正する") || text.contains("を次のように改める");
    bottom < height * 0.55 && !before_marks && !instruction && !lines.iter().any(other)
}

/// 構造化用テキスト（表は目印）とそのページの表。`count` は表の通し番号。
fn doc_text_of(units: &[Unit], count: &mut usize) -> (String, Vec<crate::trace::Table>) {
    let start = *count;
    let mut tables = Vec::new();
    let parts: Vec<String> = units.iter().map(|u| u.doc_text(&mut tables, start)).collect();
    *count = start + tables.len();
    (parts.into_iter().filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n"), tables)
}

/// 新旧対照表と判定できなかった段に残った欄見出しの字だけの行（「改正後」「改」「正前」など）を
/// 捨てる。帯の中身が画像の様式などで文字が無いと、欄見出しだけが段に分かれて残り、後続の表の
/// セルに紛れ込む。
fn drop_stray_labels(mut lines: Vec<Line>) -> Vec<Line> {
    lines.retain(|l| {
        let t: String = l.text.chars().filter(|c| !c.is_whitespace()).collect();
        let label = !t.is_empty() && t.chars().count() <= 3 && t.chars().all(|c| "改正後前".contains(c)) && (t.contains('改') || t.contains('正'));
        !label
    });
    lines
}

/// 行に組まれなかった語に、捨てた理由（欄見出しのラベルか柱の行）を付ける。
fn mark_unused(words: &mut [Word], regions: &[crate::trace::Region], merged: &[(usize, usize)]) {
    // 表の行は語をセルの側に持つ。
    let mut used: std::collections::HashSet<usize> = regions
        .iter()
        .flat_map(|r| r.lines.iter())
        .flat_map(|l| l.words.iter().copied().chain(l.table.iter().flat_map(|t| t.cells.iter().flat_map(|c| c.words.iter().copied()))))
        .collect();
    // 割注にまとめた語は、まとめた先の語が組まれていれば組まれている。
    let extra: Vec<usize> = merged.iter().filter(|(_, owner)| used.contains(owner)).map(|&(id, _)| id).collect();
    used.extend(extra);
    for w in words.iter_mut().filter(|w| w.status == WordStatus::Used && !used.contains(&w.id)) {
        w.status = if matches!(w.text.as_str(), "改" | "正" | "後" | "前") {
            WordStatus::HeaderLabel
        } else {
            WordStatus::Noise
        };
    }
}

/// `-bbox-layout` の XHTML をページ単位に解析し、`(ページ高さ, 余白除去済みカラム列)` を返す。
/// 別表復元（罫線とテキストの突合）で文字座標が必要なときに使う。
pub(crate) fn parse_page_cols(xhtml: &str) -> Vec<(f32, Vec<Col>)> {
    let doc = Html::parse_document(xhtml);
    let page_sel = Selector::parse("page").unwrap();
    let mut out = Vec::new();
    for page in doc.select(&page_sel) {
        let (width, height) = page_size(page);
        let cols = page_words_traced(page, width, height, None).0;
        out.push((height, cols));
    }
    out
}

/// ページの幅と高さ（pt）。/Rotate 90・270 のページは、pdftotext が語の座標を回した後の
/// 向きで出すのに `<page>` の幅・高さは回す前のままなので、語が幅をはみ出して高さに収まる
/// ときは入れ替える（可視化で PDF の表示と語の枠を重ねるため）。
fn page_size(page: scraper::ElementRef) -> (f32, f32) {
    let dim = |k: &str, default: f32| page.value().attr(k).and_then(|s| s.parse::<f32>().ok()).unwrap_or(default);
    let (width, height) = (dim("width", 595.0), dim("height", 842.0));
    let word_sel = Selector::parse("word").unwrap();
    let (mut max_x, mut max_y) = (0f32, 0f32);
    for w in page.select(&word_sel) {
        let num = |k: &str| w.value().attr(k).and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.0);
        max_x = max_x.max(num("xmax"));
        max_y = max_y.max(num("ymax"));
    }
    if max_x > width + 2.0 && max_x <= height + 2.0 && max_y <= width + 2.0 {
        (height, width)
    } else {
        (width, height)
    }
}

/// 縦書きのページが横倒しに表示されているか。
///
/// /Rotate 付きのページでは pdftotext の座標が表示の向きで出るため、縦書きの列が
/// 横長の語に見える。本物の横書きとは、横長の語の中に縦書き用の約物（︑︒︵︶ など
/// 縦書き用の字形）が多いことで区別する（横書きは「、。（）」を使う）。
fn is_sideways_vertical(raws: &[(f32, f32, f32, f32, String)]) -> bool {
    let (mut vforms, mut hforms, mut horizontal, mut vertical) = (0usize, 0usize, 0usize, 0usize);
    for (x, y, x1, y1, t) in raws {
        let n = t.chars().count();
        if n < 2 {
            continue;
        }
        if x1 - x > (y1 - y) * 1.5 {
            horizontal += n;
            vforms += t.chars().filter(|c| ('\u{FE10}'..='\u{FE48}').contains(c)).count();
            hforms += t.chars().filter(|c| "、。（）「」".contains(*c)).count();
        } else if y1 - y > (x1 - x) * 1.5 {
            vertical += n;
        }
    }
    horizontal > vertical && vforms > hforms * 2 && vforms >= 5
}

/// 横倒しの縦書きのページで、縦書き用の書体の語（縦書き用の約物を含む語）が、横組み用の書体の語
/// （高さが整数 pt でない語）の行頭の位置より 1 字分（表示の左へ）ずれて出ているか。品種登録の取消しの
/// 告示のように、段の中に横組みの項目と縦書きの前文が並ぶページでは、前文の頭が前の段の終わりに
/// 付いて段に分かれなかった。横組みの語の無いページは縦書きの語どうしの位置が揃っているので動かさない。
fn sideways_vertical_font_shifted(raws: &[(f32, f32, f32, f32, String)]) -> bool {
    let integral = |h: f32| (h - h.round()).abs() < 0.01;
    let mut starts: Vec<f32> = raws.iter().filter(|(_, y, _, y1, _)| !integral(y1 - y)).map(|r| r.0).collect();
    starts.sort_by(f32::total_cmp);
    // 3 語以上が揃う横組みの行頭。
    let common = |x: f32| starts.iter().filter(|&&s| (s - x).abs() < 1.0).count() >= 3;
    let vform: Vec<(f32, f32)> = raws
        .iter()
        .filter(|(_, y, _, y1, t)| integral(y1 - y) && t.chars().any(|c| ('\u{FE10}'..='\u{FE48}').contains(&c)))
        .map(|(x, y, _, y1, _)| (*x, y1 - y))
        .collect();
    let shifted = vform.iter().filter(|&&(x, h)| common(x + h)).count();
    let aligned = vform.iter().filter(|&&(x, _)| common(x)).count();
    shifted >= 3 && shifted > aligned * 2
}

/// 横倒しのページで、縦書き用の書体の語（高さがちょうど整数 pt）を 1 字分右へ戻すと、表の縦の罫線を
/// またぐ語が半分以下に減るか（罫線は正しい位置にあり、語だけがずれている。号外の別表の「所　在　地」
/// 「北　海　道」が欄の境の罫線をまたいでいた）。
fn sideways_words_straddle_rules(raws: &[(f32, f32, f32, f32, String)], rules: Option<&PageRules>) -> bool {
    let Some(rules) = rules else { return false };
    let straddles = |dx: f32| {
        raws.iter()
            .filter(|(_, y, _, y1, t)| !t.is_empty() && ((y1 - y) - (y1 - y).round()).abs() < 0.01)
            .filter(|(x, y, x1, y1, _)| {
                let (x, x1) = (x + dx * (y1 - y), x1 + dx * (y1 - y));
                rules.vertical.iter().any(|v| x + 1.0 < v.x && v.x < x1 - 1.0 && v.y0 < *y && *y1 < v.y1)
            })
            .count()
    };
    let (now, shifted) = (straddles(0.0), straddles(1.0));
    now >= 3 && shifted * 2 <= now
}

/// 横組みの語（横長）の文字数が縦組みの語（縦長）より多いページか。号外の公告・決算公告の
/// ページなどは横組みで、縦書きの読み順の復元は合わない。
fn is_horizontal_page(cols: &[Col]) -> bool {
    let (mut horizontal, mut vertical) = (0usize, 0usize);
    for c in cols.iter().filter(|c| c.text.chars().count() >= 2) {
        let (w, h) = (c.x1 - c.x, c.y1 - c.y);
        // 字間を広く空けた横書きの見出し・表の欄の名前（「利　益　剰　余　金」）は文の量に数えない
        // （縦書きのページの中の横組みの表で、横書きが多く見える）。
        if w / c.text.chars().count() as f32 > h * 1.3 {
            continue;
        }
        if w > h * 1.5 {
            horizontal += c.text.chars().count();
        } else if h > w * 1.5 {
            vertical += c.text.chars().count();
        }
    }
    horizontal > vertical
}

/// 欄見出し「改正後」「改正前」（`last` が「後」か「前」）の (左端, 右端, 上端, 語の ID)。字間を空けた
/// 1 字ずつの語（「改　正　後」）か、1 語の「改正後」。
fn shinkyu_labels(cols: &[Col], last: char) -> Vec<(f32, f32, f32, Vec<usize>)> {
    let one = |c: &Col, ch: char| c.text.trim() == ch.to_string();
    let mut out: Vec<(f32, f32, f32, Vec<usize>)> = cols
        .iter()
        .filter(|c| c.text.chars().filter(|ch| !ch.is_whitespace()).eq(['改', '正', last]))
        .map(|c| (c.x, c.x1, c.y, vec![c.id]))
        .collect();
    for k in cols.iter().filter(|c| one(c, '改')) {
        let em = (k.y1 - k.y).max(1.0);
        let row = |ch: char| cols.iter().filter(move |c| one(c, ch) && (c.y - k.y).abs() < 2.0 && c.x > k.x && c.x - k.x < em * 24.0);
        if let (Some(z), Some(e)) = (row('正').min_by(|p, q| p.x.total_cmp(&q.x)), row(last).min_by(|p, q| p.x.total_cmp(&q.x))) {
            if z.x < e.x {
                out.push((k.x, e.x1, k.y, vec![k.id, z.id, e.id]));
            }
        }
    }
    out
}

/// 横書きの新旧対照表（左右 2 列）なら左右の境目の x と、欄見出しの上端・語（あれば）を返す。
///
/// 欄見出し「改正後」「改正前」（字間を空けた 1 字ずつの語も）が左右に 1 つずつあれば、その間で、欄見出しより
/// 下の語がまたがない切れ目を境目にする（欄見出しより上の紙面の幅いっぱいの前文は境目の判定に使わない）。
///
/// ページ中央付近に、横組みの語が 1 つもまたがない縦の切れ目があり、左右の同じ高さに
/// 対応する行（同じ文、または改正前欄の「〔同左〕」）が並ぶなら新旧対照表とみなす。前のページが
/// 横書きの新旧対照表なら（`continuation`）、欄見出しの無い続きのページも同じ文が並べば表とする。
fn split_horizontal_shinkyu(cols: &[Col], continuation: bool) -> Option<(f32, f32, Vec<usize>)> {
    split_horizontal_shinkyu_by(cols, continuation, true)
}

/// [`split_horizontal_shinkyu`] の、組が横に 2 組以上入る幅のページを除くか（`check_pairs`）を選べる版。
fn split_horizontal_shinkyu_by(cols: &[Col], continuation: bool, check_pairs: bool) -> Option<(f32, f32, Vec<usize>)> {
    if let ([a], [b]) = (shinkyu_labels(cols, '後').as_slice(), shinkyu_labels(cols, '前').as_slice()) {
        let (page_x0, page_x1) = cols.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(p, q), c| (p.min(c.x), q.max(c.x1)));
        // 改正後・改正前の組が横に 2 組以上入る幅のページは [`split_horizontal_shinkyu_pairs`] が見る。
        let gap = ((a.0 + a.1) - (b.0 + b.1)).abs() / 2.0;
        let pairs = if check_pairs { page_x1 - page_x0 >= gap * 3.5 } else { gap < (page_x1 - page_x0) * 0.4 };
        if (a.2 - b.2).abs() <= 2.0 && a.1 < b.0 && !pairs {
            let ids: Vec<usize> = a.3.iter().chain(&b.3).copied().collect();
            let below: Vec<&Col> = cols.iter().filter(|c| c.y > a.2 + 1.0 && !ids.contains(&c.id)).collect();
            let center = (a.1 + b.0) / 2.0;
            // 2 組の判定から回ってきたとき（`check_pairs` が偽）は、改正前の表の頭の「別表」のような短い語 1 つが
            // 下の 2 段組の本文の段の間にまたがるのは許す。
            // 境目をまたぐ語が表の下の紙面の幅いっぱいの行（次の記事・備考。改正後と改正前の欄見出しの間をまるごと横切る）
            // にしか無ければよい: またぐ語のいちばん上より上に、境目の左右とも 5 語以上ある（号外第 172 号 176 ページ: 表の
            // 下に次の告示が続き、どこにも境目が取れなかった）。欄の中の語（右端の小数点以下の数）をまたぐ所では切らない。
            let crossing_ok = |x: f32| {
                let crossing: Vec<&&Col> = below.iter().filter(|d| d.x < x && x < d.x1).collect();
                if crossing.is_empty() || (!check_pairs && crossing.len() == 1 && crossing[0].text.chars().count() <= 3) {
                    return true;
                }
                if !crossing.iter().all(|d| d.x <= a.1 && d.x1 >= b.0) {
                    return false;
                }
                let end = crossing.iter().map(|d| d.y).fold(f32::INFINITY, f32::min);
                let left = below.iter().filter(|d| d.x1 <= x && d.y1 <= end).count();
                let right = below.iter().filter(|d| d.x >= x && d.y1 <= end).count();
                left >= 5 && right >= 5
            };
            let mid = below
                .iter()
                .map(|c| c.x1 + 1.0)
                .filter(|&x| x > a.1 && x < b.0 && crossing_ok(x))
                .min_by(|p, q| (p - center).abs().total_cmp(&(q - center).abs()));
            if let Some(mid) = mid {
                return Some((mid, a.2, ids));
            }
        }
    }
    let body: Vec<&Col> = cols.iter().filter(|c| (c.x1 - c.x) > (c.y1 - c.y)).collect();
    if body.len() < 10 {
        return None;
    }
    // 回転したページ（/Rotate 90・270）は座標が横長の向きで出るので、ページ幅ではなく
    // 語の広がりから中央を求める。
    let left = body.iter().map(|c| c.x).reduce(f32::min)?;
    let right = body.iter().map(|c| c.x1).reduce(f32::max)?;
    let (span, center) = (right - left, (left + right) / 2.0);
    let (lo, hi) = (left + span * 0.35, left + span * 0.65);
    // 候補は語の右端。またぐ語が無い切れ目のうち中央に最も近いもの。中ほどに右端の語が無ければ（改正後の欄が
    // 「削除」「［略］」だけで短い）、語の左端の手前。
    // またぐかは 1〜2 字の語でも見る（小数の「0」「.」「24」の小数点と小数部は横長でないので、「0」の右で切れていた）。
    let gap = |xs: Vec<f32>| {
        xs.into_iter()
            .filter(|&x| x > lo && x < hi && !cols.iter().any(|d| d.x < x && x < d.x1))
            .min_by(|a, b| (a - center).abs().partial_cmp(&(b - center).abs()).unwrap_or(std::cmp::Ordering::Equal))
    };
    let mid = gap(cols.iter().map(|c| c.x1 + 1.0).collect()).or_else(|| gap(cols.iter().map(|c| c.x - 1.0).collect()))?;
    let (l, r): (Vec<&&Col>, Vec<&&Col>) = body.iter().partition(|c| c.x1 <= mid);
    let alike = |a: &str, b: &str| a == b || b.contains("同左");
    let matched = r
        .iter()
        .filter(|rc| l.iter().any(|lc| (lc.y - rc.y).abs() < 1.0 && alike(&lc.text, &rc.text)))
        .count();
    // 同じ定型文が左右に並ぶだけの普通の多段組み（最低賃金の公示など）と区別するため、
    // 改正前欄の省略記法「同左」か、欄見出しの「改正後」「改正前」があることも求める。
    let same_left = r.iter().filter(|c| c.text.contains("同左")).count();
    let labels = ["改正後", "改正前"].iter().all(|lb| cols.iter().any(|c| c.text.trim() == *lb));
    // 改正前の欄に長い本文がある（改正後の欄は「削除」だけ）と、揃う行の割合は小さくなる。「同左」が 4 つ以上あれば割合は見ない。
    (matched >= 5 && (matched * 10 >= r.len() * 3 || same_left >= 4) && (same_left >= 2 || labels || continuation)).then_some((mid, f32::NEG_INFINITY, Vec::new()))
}

/// 横向きのページで、改正後・改正前の列の組が横に 2 組以上並ぶ新旧対照表（本紙第 1760 号 3 ページ:
/// 「改正後｜改正前｜改正後の続き｜改正前の続き」）を、列ごとに改正後・改正前に分ける。
///
/// 欄見出し「改正後」「改正前」（字間を空けた「改　正　後」も）が同じ高さに 1 つずつあり、その中心の間隔を
/// 列の幅とする。列は語の左右の範囲の切れ目（溝）で分け、列の幅を超えない範囲の切れ目はまとめる（列の中の
/// 字下げや表）。改正後の欄見出しのある列から右へ、交互に改正後・改正前にする。欄見出しより上の、最初の
/// 組の列の語は前文（続きの列は紙面の上から始まる）。
/// 3 列以上あるときだけ（2 列の表は [`split_horizontal_shinkyu`] が見る）。欄見出しの無い続きのページは、
/// 前のページの列の幅（`prev`）で、左の列から交互に分ける。次の記事の標題（「〇…」）のある列から後ろは
/// 表の外として返す。戻り値は（表, 表の外の語, 次のページに渡す列の幅）。
///
/// `vrules` は縦の罫線 `(x, y0, y1)`。最初の組より右で欄見出しより上に終わる縦の罫線（紙面の幅いっぱいの表の欄の線）が
/// あり、その下端より上の最初の組より右の語が 12 語までなら、それも前文（表の右の欄「令和10年４月１日」を改正前の続きの
/// 列にしない）。
fn split_horizontal_shinkyu_pairs(cols: &[Col], prev: Option<f32>, vrules: &[(f32, f32, f32)]) -> Option<(Unit, Vec<Col>, f32)> {
    let labels = |last: char| shinkyu_labels(cols, last);
    let (after, before) = (labels('後'), labels('前'));
    let center = |l: &(f32, f32, f32, Vec<usize>)| (l.0 + l.1) / 2.0;
    // (列の幅, 欄見出しの高さ, 改正前の欄見出しの中心, 欄見出しの語, 改正後の欄見出しの中心)。
    let (width, label_y, before_x, label_ids, after_x) = match (after.as_slice(), before.as_slice()) {
        ([a], [b]) if (a.2 - b.2).abs() <= 2.0 && center(b) - center(a) >= 100.0 => {
            let width = center(b) - center(a);
            (width, a.2, center(b), a.3.iter().chain(&b.3).copied().collect::<Vec<usize>>(), Some(center(a)))
        }
        ([], []) => (prev?, f32::NEG_INFINITY, f32::NEG_INFINITY, Vec::new(), None),
        _ => return None,
    };
    // 改正後・改正前の組が横に 2 組以上入る幅のページだけ（紙面の幅いっぱいの表を左右に分けたものは違う）。
    let (page_x0, page_x1) = cols.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), c| (a.min(c.x), b.max(c.x1)));
    if page_x1 - page_x0 < width * 3.5 {
        return None;
    }
    // 前文は欄見出しより上で、最初の組の 2 列の中から始まる語（その上の別の記事の表なども）。
    let table_bottom = vrules
        .iter()
        .filter(|&&(x, y0, y1)| x > before_x + width / 2.0 && y0 < label_y && y1 <= label_y + 1.0)
        .map(|r| r.2)
        .fold(f32::NEG_INFINITY, f32::max);
    // 表の欄は語が少ない（12 語まで）。多ければ改正前の続きの列（上に前の表の罫線がある）。
    let beside = cols.iter().filter(|c| c.x >= before_x + width / 2.0 && c.y1 <= table_bottom + 2.0).count();
    let table_bottom = if beside <= 12 { table_bottom } else { f32::NEG_INFINITY };
    let is_preamble = |c: &Col| c.y1 <= label_y + 1.0 && (c.x < before_x + width / 2.0 || c.y1 <= table_bottom + 2.0);
    let body: Vec<&Col> = cols.iter().filter(|c| !label_ids.contains(&c.id) && !is_preamble(c)).collect();
    // 語の左右の範囲をつないで列にする。
    let mut spans: Vec<(f32, f32)> = body.iter().map(|c| (c.x, c.x1)).collect();
    spans.sort_by(|p, q| p.0.total_cmp(&q.0));
    let mut columns: Vec<(f32, f32)> = Vec::new();
    for (x, x1) in spans {
        match columns.last_mut() {
            Some(col) if x <= col.1 + 1.0 || x1 - col.0 <= width * 0.95 => col.1 = col.1.max(x1),
            _ => columns.push((x, x1)),
        }
    }
    let first = match after_x {
        Some(ax) => columns.iter().position(|&(x, x1)| x - 2.0 <= ax && ax <= x1 + 2.0)?,
        None => 0,
    };
    let column = |c: &Col| columns.iter().position(|&(x, x1)| x <= (c.x + c.x1) / 2.0 && (c.x + c.x1) / 2.0 <= x1);
    if body.iter().any(|c| column(c).is_some_and(|k| k < first)) {
        return None;
    }
    // 続きのページで次の記事の標題（「〇…」）のある列から後ろは表の外。
    let end = if after_x.is_none() {
        body.iter().filter(|c| crate::segment::is_heading_mark(&c.text)).filter_map(|c| column(c)).min().unwrap_or(columns.len())
    } else {
        columns.len()
    };
    // 欄の 1.4 倍より広い列（表の右の、紙面の半分の幅で組んだ本文。外務省告示の「別表の 185．…を次のように改める。」）
    // から後ろも表の外。
    let wide = (first + 2..end).find(|&k| columns[k].1 - columns[k].0 > width * 1.4);
    let end = wide.unwrap_or(end);
    if end < first + if after_x.is_some() && wide.is_none() { 3 } else { 2 } {
        return None;
    }
    let rest: Vec<Col> = body.iter().filter(|c| column(c).is_none_or(|k| k >= end)).map(|c| (*c).clone()).collect();
    let preamble: Vec<Col> = cols.iter().filter(|c| !label_ids.contains(&c.id) && is_preamble(c)).cloned().collect();
    let (mut after, mut before) = (Vec::new(), Vec::new());
    for k in first..end {
        let lines = horizontal_lines(body.iter().filter(|c| column(c) == Some(k)).map(|c| (*c).clone()).collect());
        if (k - first) % 2 == 0 {
            after.push(lines);
        } else {
            before.push(lines);
        }
    }
    Some((Unit::Shinkyu { preamble: horizontal_lines(preamble), after, before, outside: Vec::new() }, rest, width))
}

/// Keep only font runs enclosing ordinary-font Japanese prose quotations.
/// Isolated headings, margin marks, units and mathematical symbols cannot
/// establish this two-sided inline relationship.
fn retain_inline_native_quotes(
    raws: &[(f32, f32, f32, f32, String)],
    horizontal: &std::collections::HashSet<([u32; 4], String)>,
    native: &mut std::collections::HashMap<[u32; 4], Vec<(String, [f32; 4])>>,
) {
    let key = |r: &(f32, f32, f32, f32, String)| {
        [r.0.to_bits(), r.1.to_bits(), r.2.to_bits(), r.3.to_bits()]
    };
    let japanese = |t: &str| {
        t.chars()
            .filter(|c| {
                ('\u{3040}'..='\u{30ff}').contains(c) || ('\u{3400}'..='\u{9fff}').contains(c)
            })
            .count()
    };
    let candidates: Vec<_> = native
        .iter()
        .flat_map(|(key, values)| {
            values
                .iter()
                .map(move |(text, b)| (*key, text.clone(), *b, word_text(text)))
        })
        .collect();
    let plain: Vec<_> = raws
        .iter()
        .filter(|r| horizontal.contains(&(key(r), r.4.clone())))
        .collect();
    let mut selected = std::collections::HashSet::new();
    for (akey, atext, a, an) in &candidates {
        let opening = an.rsplit_once('「').is_some_and(|(_, tail)| {
            tail.chars()
                .all(|c| c.is_whitespace() || matches!(c, '（' | '('))
        });
        if !opening {
            continue;
        }
        let em = a[3] - a[1];
        for (bkey, btext, b, bn) in &candidates {
            if !bn.trim_start_matches(['）', ')']).starts_with('」')
                || japanese(an).max(japanese(bn)) < 3
                || b[0] - a[2] < em * 1.5
                || ((b[3] - b[1]) - em).abs() > em * 0.2
                || ((a[1] + a[3]) - (b[1] + b[3])).abs() > em * 0.5
            {
                continue;
            }
            let mut quoted: Vec<_> = plain
                .iter()
                .copied()
                .filter(|r| {
                    r.0 >= a[2] - em * 0.25
                        && r.2 <= b[0] + em * 0.25
                        && ((r.3 - r.1) - em).abs() <= em * 0.2
                        && ((r.1 + r.3) - (a[1] + a[3])).abs() <= em * 0.5
                })
                .collect();
            quoted.sort_by(|r, s| r.0.total_cmp(&s.0));
            let (Some(first), Some(last)) = (quoted.first(), quoted.last()) else {
                continue;
            };
            if (first.0 - a[2]).abs() > em * 0.25
                || (last.2 - b[0]).abs() > em * 0.25
                || quoted.windows(2).any(|p| p[1].0 - p[0].2 > em * 0.5)
                || quoted.iter().map(|r| japanese(&r.4)).sum::<usize>() < 2
            {
                continue;
            }
            selected.insert((*akey, atext.clone()));
            selected.insert((*bkey, btext.clone()));
        }
    }
    // A second quotation may wrap once to the proven prose left margin,
    // allowing the first line to have a one-em paragraph indent. Both ordinary-font fragments must touch
    // the surrounding native brackets; the baseline advance must be one line.
    for (akey, atext, a, an) in &candidates {
        if !selected.contains(&(*akey, atext.clone()))
            || !an.starts_with('」')
            || !an.ends_with('「')
        {
            continue;
        }
        let em = a[3] - a[1];
        let anchors: Vec<_> = candidates
            .iter()
            .filter(|(k, t, c, cn)| {
                selected.contains(&(*k, t.clone()))
                    && japanese(cn) >= 3
                    && !cn.starts_with('」')
                    && cn.ends_with('「')
                    && c[0] < a[0]
                    && ((c[1] + c[3]) - (a[1] + a[3])).abs() <= em * 0.5
            })
            .collect();
        if anchors.len() != 1 {
            continue;
        }
        let left = anchors[0].2[0];
        for (bkey, btext, b, bn) in &candidates {
            if !bn.starts_with('」')
                || bn.contains('「')
                || japanese(bn) < 3
                || ((b[3] - b[1]) - em).abs() > em * 0.2
            {
                continue;
            }
            let dy = (b[1] + b[3] - a[1] - a[3]) * 0.5;
            if !(em * 1.0..=em * 1.75).contains(&dy) {
                continue;
            }
            let mut first: Vec<_> = plain
                .iter()
                .copied()
                .filter(|r| {
                    r.0 >= a[2] - em * 0.25
                        && ((r.3 - r.1) - em).abs() <= em * 0.2
                        && ((r.1 + r.3) - (a[1] + a[3])).abs() <= em * 0.5
                })
                .collect();
            first.sort_by(|r, s| r.0.total_cmp(&s.0));
            let mut second: Vec<_> = plain
                .iter()
                .copied()
                .filter(|r| {
                    r.0 >= left - em * 1.25
                        && r.2 <= b[0] + em * 0.25
                        && ((r.3 - r.1) - em).abs() <= em * 0.2
                        && ((r.1 + r.3) - (b[1] + b[3])).abs() <= em * 0.5
                })
                .collect();
            second.sort_by(|r, s| r.0.total_cmp(&s.0));
            let (Some(f), Some(l), Some(sf), Some(sl)) =
                (first.first(), first.last(), second.first(), second.last())
            else {
                continue;
            };
            if (f.0 - a[2]).abs() > em * 0.25
                || sf.0 > left + em * 0.25
                || sf.0 < left - em * 1.25
                || (sl.2 - b[0]).abs() > em * 0.25
                || l.2 - f.0 < em * 3.
                || sl.2 - sf.0 < em * 3.
                || first
                    .windows(2)
                    .chain(second.windows(2))
                    .any(|p| p[1].0 - p[0].2 > em * 0.5)
                || first
                    .iter()
                    .chain(&second)
                    .map(|r| japanese(&r.4))
                    .sum::<usize>()
                    < 3
            {
                continue;
            }
            selected.insert((*bkey, btext.clone()));
        }
    }
    native.retain(|k, values| {
        values.retain(|(t, _)| selected.contains(&(*k, t.clone())));
        !values.is_empty()
    });
}

/// 1 ページの `<word>` を縦書きカラム語として読み出す。可視化用に全語の一覧と、回転ページ
/// （横組みの本文を `/Rotate 270` で置いたページ）かどうかも返す（カラム語の `id` は語の添字）。
///
/// 余白帯の柱（発行日・「官報」等の running header）は **語全体が余白に収まるもの**だけ
/// 落とす。縦書き語の bbox は実際の字面より約 1em 上にずれて出るため、段の先頭列
/// （例: 「〇総務省令第五十八号」）は上端 y が余白帯にかかる。上端だけで判定すると
/// 記事見出しや前文の列がまるごと消える。
///
/// 回転ページの柱は上端ではなく左端の帯に縦に並ぶ（上端の帯には本文が来る）ので、
/// 左端の帯に収まる語を柱として落とす（[`is_left_strip_header`]）。
/// 戻り値は（語の列、表示用の語、回転ページか、横倒しの縦書きページか）。
/// `merged` は旧割注、`inline_merged` は完全な括弧付き引用へまとめた `(元Word, 所有Word)`。
#[allow(clippy::type_complexity)]
fn page_words_traced(
    page: scraper::ElementRef,
    page_width: f32,
    height: f32,
    page_rules: Option<&PageRules>,
) -> (Vec<Col>, Vec<Word>, bool, bool, Vec<(usize, usize)>, std::collections::HashSet<usize>,
) {
    let (a,b,c,d,e,f,_,_,_,_,_,_)=page_words_with_styles(page,page_width,height,page_rules);
    (a,b,c,d,e,f)
}
fn page_words_with_styles(
    page: scraper::ElementRef,
    page_width: f32,
    height: f32,
    page_rules: Option<&PageRules>,
) -> (Vec<Col>, Vec<Word>, bool, bool, Vec<(usize, usize)>, std::collections::HashSet<usize>, Vec<Option<crate::trace::NativeStyles>>, Vec<crate::trace::NativeBounds>, Vec<crate::trace::ContentStyles>, Vec<crate::native_word_axes::NativeAxis>,Vec<crate::native_word_rulings::NativeRuling>, Vec<(usize,usize)>,
) {
    let word_sel = Selector::parse("word").unwrap();
    let line_sel = Selector::parse("line").unwrap();
    // (x0, y0, x1, y1, raw)
    let mut raws: Vec<(f32, f32, f32, f32, String)> = Vec::new();
    let mut resolved_aj1 = Vec::new();
    let mut source_candidates = Vec::new();
    let mut native_styles=Vec::new();
    let mut native_bounds=Vec::new();
    let mut native_content_styles=Vec::new();
    let mut native_top_down=Vec::new();
    let mut native_axes=Vec::new();
    let mut native_rulings=Vec::new();
    let mut native_vertical: std::collections::HashMap<[u32; 4], Vec<(String, [f32; 4])>> = Default::default();
    let mut native_horizontal_chars = 0usize;
    let mut horizontal_font_chars = 0usize;
    let mut horizontal_font_words: std::collections::HashSet<([u32; 4], String)> = Default::default();
    let mut push = |w: scraper::ElementRef| {
        let v = w.value();
        // scraper は HTML パーサのため属性名を小文字化する（xMin -> xmin）。
        let num = |k: &str| v.attr(k).and_then(|s| s.parse::<f32>().ok());
        if let (Some(x), Some(x1), Some(y), Some(y1)) = (num("xmin"), num("xmax"), num("ymin"), num("ymax")) {
            let text: String = w.text().collect();
            if v.attr("data-wmode") == Some("0") && x1-x > (y1-y)*1.5 {
                horizontal_font_chars += text.chars().count();
                horizontal_font_words.insert(([x.to_bits(),y.to_bits(),x1.to_bits(),y1.to_bits()],text.clone(),
                ));
            }
            if v.attr("data-wmode") == Some("1") {
                if let (Some(ax), Some(ay), Some(nx), Some(ny), Some(nx1), Some(ny1)) =
                    (num("data-advance-x"), num("data-advance-y"), num("data-native-xmin"), num("data-native-ymin"), num("data-native-xmax"), num("data-native-ymax"),
                )
                {
                    if [ax,ay,nx,ny,nx1,ny1].iter().all(|n| n.is_finite()) && ax.abs() > ay.abs()*1.5 && ax.abs() > 0.0 && nx1 > nx && ny1 > ny {
                        if x1-x > (y1-y)*1.5 { native_horizontal_chars += text.chars().count(); }
                        native_vertical.entry([x.to_bits(),y.to_bits(),x1.to_bits(),y1.to_bits()]).or_default().push((text.clone(),[nx,ny,nx1,ny1]));
                    }
                }
            }
            native_top_down.push(v.attr("data-wmode")==Some("1") && num("data-advance-x").is_some_and(|n|n.is_finite()&&n==0.) && num("data-advance-y").is_some_and(|n|n.is_finite()&&n>0.));
            native_axes.push(crate::native_word_axes::from_word(w));
            native_rulings.push(crate::native_word_rulings::from_word(w));
            native_bounds.push(crate::native_word_bounds::from_word(w));
            native_styles.push(v.attr("data-native-styles").and_then(|text| {
                let mut out=crate::trace::NativeStyles::new();
                for token in text.split_whitespace(){
                    let (f,b)=token.split_once(':')?;
                    let (f,b)=(f.parse::<usize>().ok()?,u64::from_str_radix(b,16).ok()?);
                    let size=f64::from_bits(b);
                    if !size.is_finite() || size<=0.0 {return None;}
                    out.insert((f,b));
                }
                (!out.is_empty()).then_some(out)
            }));
            native_content_styles.push(crate::native_content_styles::from_word(w, native_styles.last().unwrap().as_ref()));
            source_candidates.push(
                v.attr("data-source-aj1-text")
                    .and_then(|value| crate::source_aj1::candidate(&text, value)),
            );
            raws.push((x, y, x1, y1, text));
            resolved_aj1.push(v.attr("data-unicode-aj1") == Some("1"));
        }
    };
    if page.select(&line_sel).next().is_some() {
        for line in page.select(&line_sel) {
            for w in line.select(&word_sel) {
                push(w);
            }
        }
    } else {
        // 行構造の無い入力（テスト用の最小 XHTML など）は 1 語 1 行とみなす。
        for w in page.select(&word_sel) {
            push(w);
        }
    }
    // 語は座標の正規の順（x 降順・y 昇順）に並べ替えてから使う。`<word>` の並びは抽出器によって
    // 違い（poppler は段組みの解析順、kanpo-pdf はコンテンツストリームの順）、以降の処理の
    // 同点の扱いなどに入力順が出ないようにする。
    // Keep provenance attached to its own source occurrence, including exact
    // bbox/text ties with a word from another extractor or font.
    let mut sources: Vec<_> = raws.into_iter().zip(resolved_aj1).zip(source_candidates).zip(native_styles).zip(native_bounds).zip(native_content_styles).zip(native_top_down)
        .map(|((((((raw, flag), source), style), bounds), content), top_down)| (raw, (((((flag, source),style),bounds),content),top_down)))
        .zip(native_axes).map(|((raw,evidence),axis)|(raw,(evidence,axis)))
        .zip(native_rulings).map(|((raw,evidence),ruling)|(raw,(evidence,ruling)))
        .collect();
    sources.sort_by(|(a, _), (b, _)| {
        b.0.total_cmp(&a.0)
            .then(a.1.total_cmp(&b.1))
            .then(b.2.total_cmp(&a.2))
            .then(a.3.total_cmp(&b.3))
            .then_with(|| a.4.cmp(&b.4))
    });
    let (mut raws, evidence): (Vec<_>, Vec<_>) = sources.into_iter().unzip();
    let (evidence,native_rulings): (Vec<_>,Vec<_>) = evidence.into_iter().unzip();
    let (evidence,native_axes): (Vec<_>,Vec<_>) = evidence.into_iter().unzip();
    let (evidence,native_top_down): (Vec<_>, Vec<_>) = evidence.into_iter().unzip();
    let (evidence,native_content_styles): (Vec<_>, Vec<_>) = evidence.into_iter().unzip();
    let (evidence,native_bounds): (Vec<_>, Vec<_>) = evidence.into_iter().unzip();
    let (evidence,native_styles): (Vec<_>, Vec<_>) = evidence.into_iter().unzip();
    let (resolved_aj1, source_candidates): (Vec<_>, Vec<_>) = evidence.into_iter().unzip();
    // 縦書きのページが /Rotate で横倒しに表示されている（座標上は縦の列が横の行に見える）
    // なら、復元に使う座標を 90° 回して縦書きに戻す。語の表示用の座標（`Word`）は
    // PDF の表示と重なるよう元のまま残す。
    // pdftotext の縦書きの語の bbox は、字面より約 1 文字分（語の幅）上にずれて出る。
    // 原本の画像と突き合わせると、縦書き用の書体（語の幅＝文字の大きさがちょうど整数 pt）
    // の列はほぼ正確に 1em ずれているので 1em 下へ補正する（罫線の座標や PDF の表示と揃える）。
    // 横組み用の書体を回転して縦に置いた語（幅が 8.184pt など 1.023 倍）はずれないので触らない。
    // 1 字の正方形の語は縦横どちらの組みか語だけでは分からないので、ページの向き
    // （縦長の語と横長の語の文字数）で決める。
    // 目次のページ番号（私用 CID の数字）。位置で見るので、座標を補正する前に決める。
    let toc_numbers = toc_page_numbers(&raws);
    let source_boxes: Vec<_> = raws.iter().map(|(x, y, x1, y1, _)| [*x, *y, *x1, *y1]).collect();
    let (mut v_chars, mut h_chars) = (0usize, 0usize);
    for (x, y, x1, y1, t) in &raws {
        let n = t.chars().count();
        if n >= 2 && y1 - y > (x1 - x) * 1.5 {
            v_chars += n;
        } else if n >= 2 && x1 - x > (y1 - y) * 1.5 {
            h_chars += n;
        }
    }
    let vertical_page = v_chars >= h_chars;
    // 表示用の語の枠を左へ戻す量。縦書きの語の枠は横にも半 em 右へずれている（枠の左端が字の中心に
    // 来る）。読み順の復元はこのずれのままの座標で組んであるので、表示用の座標（`Word`）だけ戻す。
    let mut display_dx: Vec<f32> = vec![0.0; raws.len()];
    for ((x, y, x1, y1, t), dx) in raws.iter_mut().zip(display_dx.iter_mut()) {
        let em = *x1 - *x;
        let vertical_font = (em - em.round()).abs() < 0.01;
        // 1〜2 字の語は高さも整数 pt のものだけ（縦中横の「13」は横組み用の書体で高さが
        // 8.184pt などになり、ずれていない）。
        let h = *y1 - *y;
        // 半角の英数字 1 字の語（横組みの金額の頭の「3」）は、高さが幅の 2 倍でも縦組みではない
        // （縦組みの小数点「･」は 1 字でもずれている）。
        let half_width = t.chars().count() == 1 && t.chars().all(|c| c.is_ascii_alphanumeric());
        let tall = (h >= em * 1.5 && !half_width)
            || (vertical_page && t.chars().count() <= 2 && h >= em * 0.9 && (h - h.round()).abs() < 0.01);
        // 上の余白帯の柱（「官」「報」「第」「号」の 1 字の語）は動かさない。
        if vertical_font && *y1 > TOP_MARGIN && tall {
            *y += em;
            *y1 += em;
            *dx = em / 2.0;
        }
    }
    let sideways = is_sideways_vertical(&raws);
    let rotated = !sideways && raws.iter().any(|(_, _, x1, _, t)| is_left_strip_header(*x1, t));
    // Poppler's WMode-1 rectangle is intentionally offset from the glyph
    // rectangle. In horizontally set mixed pages, use the original rectangle
    // only when font metadata proves a horizontal advance. Pure vertical and
    // sideways pages retain their existing normalization.
    if !vertical_page && !sideways && horizontal_font_chars > native_horizontal_chars {
        // A coordinate/text key can occur more than once (overprinted text or
        // distinct font runs). Use it only when every source occurrence has a
        // valid native rectangle and all occurrences agree on that rectangle.
        // An unannotated word must never inherit another word's font metadata.
        native_vertical.retain(|key, values| {
            values.iter().all(|(text, b)| {
            let declared = values.iter().filter(|(t, _)| t == text).count();
            let sources = raws.iter().filter(|(x, y, x1, y1, t)| {
                        [x.to_bits(), y.to_bits(), x1.to_bits(), y1.to_bits()] == *key && t == text
                    }).count();
            declared == sources && values.iter().filter(|(t, _)| t == text).all(|(_, other)| other == b)
        })
        });
        retain_inline_native_quotes(&raws, &horizontal_font_words, &mut native_vertical);
        for (x,y,x1,y1,text) in &mut raws {
            if let Some((_,b)) = native_vertical.get(&[x.to_bits(),y.to_bits(),x1.to_bits(),y1.to_bits()]).and_then(|xs| xs.iter().find(|(t,_)| t == text)) {
                (*x, *y, *x1, *y1) = (b[0],b[1],b[2],b[3]);
            }
        }
    }
    let shift_sideways = sideways && (sideways_vertical_font_shifted(&raws) || sideways_words_straddle_rules(&raws, page_rules));
    // 横倒しに組んだ URL の語（下で半 em 寄せる語）。何列も続く欧文（英字の住所）は寄せない。
    let rotated_latin = |(x, y, x1, y1, t): &(f32, f32, f32, f32, String)| {
        let (w, h) = (x1 - x, y1 - y);
        // URL・メールアドレス（「/」「.」「@」を含む）だけ（表の欄の英字の名称は寄せない）。
        t.chars().all(|c| c.is_ascii_graphic()) && t.contains(['/', '.', '@']) && h >= w * 3.0 && (w - w.round()).abs() >= 0.01 && t.chars().count() >= 6
    };
    let latin_columns = raws.iter().filter(|r| rotated_latin(r)).count();
    let geom: Vec<(f32, f32, f32, f32)> = raws
        .iter()
        .map(|(x, y, x1, y1, t)| {
            let (x, y, x1, y1) = (*x, *y, *x1, *y1);
            if sideways {
                // 元のページの上端 → 表示の左端、右端 → 表示の上端。縦書き用の書体の語（字の大きさがちょうど
                // 整数 pt）は、立てたページと同じく 1 字分元のページの上（表示の左）にずれて出るので戻す
                // （段の頭の「〇農林水産省告示…」が前の段の終わりに付いて段に分かれなかった）。横組み用の
                // 書体の語（高さ 8.184pt など）はずれていない。
                let h = y1 - y;
                // ずれが確かめられたページだけ（[`sideways_vertical_font_shifted`]）。柱（表示の左端の帯の「官」
                // 「報」、題字の下の「（号外）」の欄）は動かさない。
                let vertical_font = (h - h.round()).abs() < 0.01 && !t.is_empty();
                let dx = if shift_sideways && vertical_font && x > TOP_MARGIN { h } else { 0.0 };
                (page_width - y1, x + dx, page_width - y, x1 + dx)
            } else if vertical_page && !rotated {
                // 縦書きのページの中で横倒しに組んだ欧文（URL「https://…」）の語は横組み用の書体で、縦書き用の書体の列の語
                // （枠が半 em 右にずれている）より半 em 左に出る。列の並びに合わせて半 em 右へ寄せる（折り返した URL の
                // 2 行目が左の列の語とつながっていた）。
                let near = raws.iter().filter(|r| rotated_latin(r) && (r.0 - x).abs() < (x1 - x) * 3.0).count();
                let dx = if rotated_latin(&(x, y, x1, y1, t.clone())) && latin_columns <= 6 && near <= 2 { (x1 - x) / 2.0 } else { 0.0 };
                (x + dx, y, x1 + dx, y1)
            } else {
                (x, y, x1, y1)
            }
        })
        .collect();
    let composed_numbers = crate::composite_numbers::overlaid_private_numbers(&raws);
    // The PDF bridge has proved these characters against actual AJ1 CID
    // mappings. Only the legacy untagged-CID guess is bypassed; word_text
    // still handles punctuation/PUA and tagged Identity glyphs normally.
    let glyphs: Vec<Option<CidGlyph>> = raws.iter().zip(&resolved_aj1).zip(&source_candidates)
        .map(|(((x, y, x1, y1, raw), resolved), source)| {
        if *resolved || source.is_some() { None } else { private_cid_glyph(raw, x1 - x, y1 - y) }
    }).collect();
    // 紛れのない私用 CID 記号が出ている系統。常用漢字と同じ符号はこの系統があるページで、
    // しかも前後の語と間を空けずに続く（「（イ）及び（ロ）に掲げる」）ときだけ写像する。
    let families: std::collections::HashSet<u8> =
        glyphs.iter().flatten().filter(|g| !g.ambiguous).map(|g| g.family).collect();
    let touches_neighbor = |i: usize| {
        let (x, y, x1, y1, _) = &raws[i];
        let em = x1 - x;
        raws.iter().enumerate().any(|(j, (u, v, u1, v1, _))| {
            j != i
                && (((u - x).abs() < 2.0 && ((v - y1).abs() < em * 0.5 || (y - v1).abs() < em * 0.5))
                    // 横組みでは左右の語と続く
                    || ((v - y).abs() < 2.0 && ((u - x1).abs() < em * 0.5 || (x - u1).abs() < em * 0.5)))
        })
    };

    let glyphs: Vec<Option<CidGlyph>> = glyphs
        .into_iter()
        .enumerate()
        .map(|(i, g)| {
            g.filter(|g| !g.ambiguous || (families.contains(&g.family) && touches_neighbor(i)))
        })
        .collect();

    let mut cols: Vec<Col> = Vec::new();
    let mut words: Vec<Word> = Vec::new();
    for ((((((x, y, x1, y1, raw), glyph), (gx, gy, gx1, gy1)), toc_number), dx), source_box) in raws.into_iter().zip(glyphs).zip(geom).zip(toc_numbers).zip(display_dx).zip(source_boxes) {
        // scraper が実体参照は復号済み。私用 CID の括弧付き数字等を復元し、約物を正規化し、
        // 私用領域(傍線等のフォント固有グリフ)を除去する。
        let text: String = source_candidates[words.len()]
            .as_ref()
            .map(|source| word_text(source))
            .unwrap_or_else(|| {
                composed_numbers[words.len()].clone().unwrap_or_else(|| match (toc_number, glyph) {
            (Some(n), _) => n,
            (None, Some(g)) => g.text.clone(),
            (None, None) => word_text(&raw),
                    })
            });
        // 綴じ穴の印「〇」2 つと中央の印（私用 CID の「△」の字形）は、横組みのページと号の 1 ページ目（横倒しとみなす）
        // では紙面の上端（本文の上端 y≈47 より上の y≈7–26）に刷られている（本文に「〇」「△」の行が出ていた）。
        let punch_mark = y1 <= PUNCH_MARK_BOTTOM && matches!(text.trim(), "〇" | "△");
        let in_margin = punch_mark || if rotated {
            gx1 <= LEFT_STRIP
        } else {
                // 横倒しのページは回した座標の高さが表示の幅（`page_width`）。表示の高さで見ると、表示の右の
                // 3 割ほど（号外226 p25 の 4 段目、座標の表の右端の欄）が余白として捨てられていた。
                let bottom = if sideways { page_width } else { height };
            gy1 <= TOP_MARGIN || gy >= bottom - BOTTOM_MARGIN || (!sideways && gx >= page_width - RIGHT_MARGIN)
        };
        let status = if in_margin {
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
                x: gx,
                x1: gx1,
                y: gy,
                y1: gy1,
                text: text.clone(),
            });
        }
        // 横倒しのページは表示の向きが違うので戻さない。
        let dx = if sideways { 0.0 } else { dx };
        words.push(Word {
            id,
            x0: x - dx,
            y0: y,
            x1: x1 - dx,
            y1,
            source_box,
            raw,
            text,
            status,
        });
    }
    if !sideways && !rotated {
        if let Some(rules)=page_rules {
            crate::framed_native_columns::restore(&mut cols,&words,&native_bounds,&native_top_down,&rules.enclosures);
        }
    }
    drop_ruby(&mut cols, &mut words);
    let minus = attach_minus_signs(&mut cols, &words);
    // Resolve complete native citation spans before the older lossy projections.
    let inline_merged = page_rules.map(|r| enclosed_warichu::merge(&mut cols,r)).unwrap_or_default();
    let protected = inline_merged.iter().map(|&(_, owner)| owner).collect();
    let merged = merge_warichu(&mut cols, &protected);
    (cols, words, rotated, sideways, merged, minus, native_styles, native_bounds, native_content_styles, native_axes, native_rulings, inline_merged)
}

/// 枠のセルに文章（200 字を超える、または 2 文以上）が入っているか。公告の囲み記事などの
/// 枠を表と取り違えないため。括弧の中の句点（「（…場合を含む。）」）は文の終わりに数えない（読替えの表の
/// 欄に括弧書きが 2 つあるだけで文章とみなし、表を行ごとに読んでいた。号外第 163 号 61 ページ）。
pub(crate) fn prose_cells(cols: &[Col], g: &Grid) -> bool {
    // 縦横とも 2 つ以上に区切られた格子は、セルが長くても文が 2 つ以上無ければ表（号外第 172 号 266 ページ: 「地方公共
    // 団体」の欄に市町村の一覧、「機能要件の標準」の欄に項の一覧が入る表。表として組まないと上下の欄の列が 1 行に
    // つながっていた）。段を区切る罫線で格子に見える記事（同 290 ページの下半分の告示）はセルに文がある。
    let grid = g.xs.len() >= 3 && g.ys.len() >= 3;
    // 表の枠の中の備考の欄（「備考　一　…。二　…。」）は文でも表の欄。
    let notes: std::collections::HashSet<usize> = cols.iter().filter(|c| c.text.trim_start().starts_with("備考")).filter_map(|c| g.cell_at((c.x + c.x1) / 2.0, (c.y + c.y1) / 2.0)).collect();
    let mut cells: std::collections::HashMap<usize, (usize, usize)> = std::collections::HashMap::new();
    for c in cols {
        if let Some(i) = g.cell_at((c.x + c.x1) / 2.0, (c.y + c.y1) / 2.0).filter(|i| !notes.contains(i)) {
            let e = cells.entry(i).or_default();
            e.0 += c.text.chars().count();
            let chars: Vec<char> = c.text.chars().collect();
            e.1 += chars
                .iter()
                .enumerate()
                .filter(|&(k, &ch)| matches!(ch, '。' | '︒') && !chars.get(k + 1).is_some_and(|n| matches!(n, '）' | ')' | '」' | '〕' | '︶' | '﹂' | '︼')))
                .count();
        }
    }
    cells.values().any(|&(chars, sentences)| (chars > 200 && !grid) || sentences >= 2)
}

/// 枠の中の語が縦組みか（縦長の語の字数が横長の語の字数以上）。
pub(crate) fn vertical_inside(cols: &[Col], g: &Grid) -> bool {
    let (mut v, mut h) = (0, 0);
    for c in cols.iter().filter(|c| g.contains((c.x + c.x1) / 2.0, (c.y + c.y1) / 2.0, 0.5)) {
        let n = c.text.chars().count();
        if n >= 2 && c.y1 - c.y > (c.x1 - c.x) * 1.5 {
            v += n;
        } else if n >= 2 && c.x1 - c.x > (c.y1 - c.y) * 1.5 {
            h += n;
        }
    }
    v > 0 && v >= h
}

/// 負の金額の印「△」だけの語を取り除き、すぐ後ろ（横組みなら右、縦組みなら下）の語の ID を返す（本文に組んだ後で、その語の
/// 頭に「△」を付ける。[`prefix_minus`]）。「△」を 1 つの語として残したり語の字に足したりすると、表の金額の欄の間の溝や
/// 語の字数で見る組み方の判定が変わり、表の列や横組みの欄の読み方が変わっていた。後ろの語は、同じ行（横組みなら上下が
/// 半分以上重なる、縦組みなら左右が半分以上重なる）で、1 字分以内（重なりも含む。数の語なら 12 字分以内。右揃えの金額の
/// 欄では「△」が欄の左端に置かれる）に始まるもの。数の語を先に選ぶ。位置は表示用の座標（`words`）で見る（横倒しのページは読み順の復元用に
/// 座標を回しているので、回した座標では「△」と数の並びの向きが揃わない）。
fn attach_minus_signs(cols: &mut Vec<Col>, words: &[Word]) -> std::collections::HashSet<usize> {
    let geom = |id: usize| words.get(id).map(|w| (w.x0, w.y0, w.x1, w.y1));
    let mut drop: Vec<usize> = Vec::new();
    let mut targets: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for i in 0..cols.len() {
        if cols[i].text.trim() != "△" {
            continue;
        }
        let Some((mx, my, mx1, my1)) = geom(cols[i].id) else { continue };
        let em = (mx1 - mx).max(my1 - my).max(1.0);
        let overlap = |a0: f32, a1: f32, b0: f32, b1: f32| (a1.min(b1) - a0.max(b0)) / (a1 - a0).min(b1 - b0).max(0.1);
        // 括弧の中の「△」（「税金等調整前当期純損失（△）」「（△は減少）」の説明の印）は数に付けず、括弧の間の字として残す。印は縦書き用の
        // 字形で少し上にずれ、前の語の「（」の右端に重なって置かれるので、括弧の間に置き直す（下の行の頭とみなして前の語の
        // 頭に付けていた）。
        let c = &cols[i];
        let open = cols.iter().find(|o| {
            (o.text.trim_end().ends_with('（') || o.text.trim_end().ends_with('(')) && overlap(o.y, o.y1, c.y, c.y1) >= 0.3 && o.x1 >= c.x - em && o.x1 <= c.x1 + 1.0
        });
        // 括弧の中の次の語（「）」「は減少）」）。
        let close = cols.iter().filter(|o| o.id != c.id && !o.text.trim().is_empty()).find(|o| {
            overlap(o.y, o.y1, c.y, c.y1) >= 0.3
                && o.x >= c.x1 - 1.0
                && o.x - c.x1 <= em * 1.5
        });
        if let (Some(o), Some(cl)) = (open, close) {
            let (x, x1, y, y1) = (o.x1, cl.x - 0.01, cl.y, cl.y1);
            let c = &mut cols[i];
            (c.x, c.x1, c.y, c.y1) = (x, x1.max(x + 0.02), y, y1);
            continue;
        }
        let target = (0..cols.len())
            // 開き括弧だけの語（「(△785)」の「(」）には付けない（「△」は括弧の中の数に付く。下の行の「△」が、字に合わせて
            // 下にずれて置かれた上の行の「(」に付いていた）。
            .filter(|&j| j != i && !matches!(cols[j].text.trim(), "△" | "" | "(" | "（") && !targets.contains(&cols[j].id))
            .filter_map(|j| {
                let c = &cols[j];
                let (cx, cy, cx1, cy1) = geom(c.id)?;
                // 様式の金額の欄の「××」も数。
                let number = c.text.trim_start().starts_with(|ch: char| ch.is_ascii_digit() || ('０'..='９').contains(&ch) || matches!(ch, '(' | '（' | '×'));
                // 右揃えの金額の欄では「△」が欄の左端に置かれ、短い金額とは 10 字分ほど離れることがある。
                let reach = if number { em * 12.0 } else { em };
                // 縦書き用の字形の印（私用 CID）は横組みでは字の半分ほど上にずれて置かれる（様式の表の「△××」）ので、上下は
                // 3 割の重なりまで見る。
                let right = overlap(my, my1, cy, cy1) >= 0.3 && cx >= mx + em * 0.25 && cx - mx1 <= reach;
                // 下の語は縦組みの語（横長の語は、同じ行の前の語に重なって置かれた印の下に見えるだけ）。
                // 下の行の横組みの数の頭の字（「1,128,902」の「1」。左右の語と接して並ぶ）も下の語ではない（右の短い金額より、すぐ下の
                // 行の長い金額の頭の字が近く、印が下の行に付いていた）。少し上にずれて数の頭の字の真上に置かれた印は除く。
                let in_run = || {
                    cols.iter().any(|o| {
                        o.id != c.id
                            && geom(o.id).is_some_and(|(ox, oy, ox1, oy1)| overlap(cy, cy1, oy, oy1) >= 0.5 && (ox - cx1).max(cx - ox1) <= em * 0.1 && !(cx..=cx1).contains(&((ox + ox1) / 2.0)))
                    })
                };
                let below = overlap(mx, mx1, cx, cx1) >= 0.5 && cy >= my + em * 0.25 && cy - my1 <= reach && cx1 - cx <= (cy1 - cy).max(em * 1.5) && !(cy - my1 > em * 0.3 && in_run());
                let gap = if right { cx - mx } else if below { cy - my } else { return None };
                Some((!number, gap, j))
            })
            // 数を先に（右の欄の金額より、すぐ下の行の見出しの字が近いことがある）。
            .min_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)))
            .map(|(_, _, j)| j);
        if let Some(j) = target {
            // A smaller-font horizontal amount may be farther right than the
            // next row is below. Keep the source sign in its native row rather
            // than synthesizing a prefix on the closer lower amount.
            let selected = &cols[j];
            if let Some((sx,sy,sx1,_)) = geom(selected.id) {
                let selected_below = overlap(mx,mx1,sx,sx1) >= 0.5
                    && sy >= my + em*0.25;
                let horizontal_amount = |k: usize| {
                    let n=&cols[k];
                    let Some((x,y,x1,y1))=geom(n.id) else {return false};
                    let numeric = !n.text.is_empty() && n.text.chars().all(|ch| ch.is_ascii_digit() || matches!(ch,','|'.'))
                        && n.text.chars().any(|ch|ch.is_ascii_digit());
                    numeric && x>=mx1 && x-mx1<=em*3.0
                        && overlap(my,my1,y,y1)>=0.8
                        && (((my+my1)-(y+y1))/2.0).abs()<=em*0.15
                        && cols.iter().enumerate().any(|(h,o)| h!=k && o.id!=cols[i].id
                            && o.text.chars().all(|ch|ch.is_ascii_digit() || matches!(ch,','|'.'))
                            && geom(o.id).is_some_and(|(u,v,u1,v1)| overlap(y,y1,v,v1)>=0.8
                                && (u-x1).max(x-u1)<=em*0.25 && u>=x))
                };
                if selected_below && (0..cols.len()).any(horizontal_amount) {continue;}
            }
            targets.insert(cols[j].id);
            drop.push(cols[i].id);
            // 印に重なる開き括弧だけの語（「(△135)」の「(」）は、印を付けた金額の語のすぐ左に置き直す（括弧だけが金額の
            // 欄の外の升に入り、次の行の頭に出ていた）。閉じ括弧で終わる金額のときだけ。
            if cols[j].text.trim_end().ends_with([')', '）']) {
                let (tx, ty, ty1) = (cols[j].x, cols[j].y, cols[j].y1);
                if let Some(k) = (0..cols.len()).find(|&k| {
                    matches!(cols[k].text.trim(), "(" | "（") && overlap(cols[k].y, cols[k].y1, my, my1) >= 0.3 && cols[k].x1 >= mx - 0.5 && cols[k].x <= mx1
                }) {
                    let w = (cols[k].x1 - cols[k].x).max(0.02);
                    (cols[k].x, cols[k].x1, cols[k].y, cols[k].y1) = (tx - w, tx - 0.01, ty, ty1);
                }
            }
        }
    }
    cols.retain(|c| !drop.contains(&c.id));
    targets
}

/// 組んだ行（表のセルも）の、[`attach_minus_signs`] で選んだ語の頭に「△」を付ける。行の語を順に行の文の中から探して
/// 位置を決める（`texts` は語の ID から語の字）。
fn prefix_minus(units: &mut [Unit], minus: &std::collections::HashSet<usize>, texts: &std::collections::HashMap<usize, String>) {
    if minus.is_empty() {
        return;
    }
    let insert = |text: &mut String, words: &[usize]| {
        let mut pos = 0;
        for id in words {
            let Some(wt) = texts.get(id).map(|t| t.trim()).filter(|t| !t.is_empty()) else { continue };
            // 語の並び（表のセルは回した座標の順のことがある）と文の字の順が違えば、文の頭から探し直す。
            let at = match text[pos..].find(wt) {
                Some(k) => pos + k,
                None if minus.contains(id) => match text.find(wt) {
                    Some(k) => k,
                    None => continue,
                },
                None => continue,
            };
            if minus.contains(id) {
                text.insert(at, '△');
                pos = at + '△'.len_utf8() + wt.len();
            } else {
                pos = at + wt.len();
            }
        }
    };
    let mut fix = |line: &mut Line| {
        if let Some(t) = line.table.as_mut() {
            let mut touched = false;
            for c in &mut t.cells {
                if c.words.iter().any(|w| minus.contains(w)) {
                    insert(&mut c.text, &c.words);
                    touched = true;
                }
            }
            if touched {
                line.text = t.render();
            }
        } else if line.words.iter().any(|w| minus.contains(w)) {
            let words = line.words.clone();
            insert(&mut line.text, &words);
        }
    };
    for u in units.iter_mut() {
        match u {
            Unit::Plain(lines) => lines.iter_mut().for_each(&mut fix),
            Unit::Shinkyu { preamble, after, before, outside } => {
                preamble.iter_mut().for_each(&mut fix);
                for part in after.iter_mut().chain(before.iter_mut()).chain(outside.iter_mut()) {
                    part.iter_mut().for_each(&mut fix);
                }
            }
        }
    }
}

/// 振り仮名（常用漢字表にない字などの脇に小さく組んだ読み。「強靱（じん）」「涵（かん）養」）を本文から除く。
/// 振り仮名の語は、ひらがなだけで 4 字まで、字の大きさ（横組みなら高さ、縦組みなら幅）が隣の本文の語の 6 割以下で、
/// 本文の語の行の外側（横組みなら上、縦組みなら右）に接し（3pt まで食い込む）、親字の範囲に収まる。本文の行に混ざると
/// 「強靱じん化」になり、別の行になると「じん」だけの行になっていた。
fn drop_ruby(cols: &mut Vec<Col>, words: &mut [Word]) {
    let kana = |t: &str| {
        let t = t.trim();
        (1..=4).contains(&t.chars().count()) && t.chars().all(|c| ('ぁ'..='ゖ').contains(&c))
    };
    let over_parent = |r: &Col| {
        let (rw, rh) = (r.x1 - r.x, r.y1 - r.y);
        cols.iter().any(|b| {
            if b.id == r.id || kana(&b.text) {
                return false;
            }
            let (bw, bh) = (b.x1 - b.x, b.y1 - b.y);
            // 横組みの本文の語の上（振り仮名の下端が本文の上端に接する）。
            // 接する所は、本文の字の枠に 3pt まで食い込むことがある（横倒しのページの「強靱（じん）」）。
            let touches = |gap: f32| (-3.0..=1.5).contains(&gap);
            // 1 字の親字（「春」「恵」）は正方形で、3 字の振り仮名（「めぐみ」）は親字からはみ出すので、振り仮名の中ほどが
            // 親字の範囲にあればよい。1 字の振り仮名（「じ」）は正方形になる。
            let (rcx, rcy) = ((r.x + r.x1) / 2.0, (r.y + r.y1) / 2.0);
            let above = bw >= bh * 0.9 && rw >= rh * 0.9 && rh <= bh * 0.6 && touches(b.y - r.y1) && rcx >= b.x - 1.0 && rcx <= b.x1 + 1.0;
            // 縦組みの本文の語の右（振り仮名の左端が本文の右端に接する）。
            let right = bh >= bw * 0.9 && rh >= rw * 0.9 && rw <= bw * 0.6 && touches(r.x - b.x1) && rcy >= b.y - 1.0 && rcy <= b.y1 + 1.0;
            above || right
        })
    };
    // 1 字の振り仮名は、同じ大きさで同じ行に字間を空けて並ぶ小さいかな（欄の見出しの「ふ　り　が　な」）の
    // 1 字が、たまたま親字の上に来たものでないこと。並びの中に親字の上に無い字があれば振り仮名ではない。
    let spaced = |r: &Col| {
        let (rw, rh) = (r.x1 - r.x, r.y1 - r.y);
        r.text.trim().chars().count() == 1
            && cols.iter().any(|s| {
                s.id != r.id
                    && kana(&s.text)
                    && ((s.x1 - s.x) - rw).abs() < 0.5
                    && ((s.y1 - s.y) - rh).abs() < 0.5
                    && (((s.y - r.y).abs() < 1.0 && (s.x - r.x).abs() < rw * 5.0)
                        || ((s.x - r.x).abs() < 1.0 && (s.y - r.y).abs() < rh * 5.0))
                    && !over_parent(s)
            })
    };
    let ruby: std::collections::HashSet<usize> =
        cols.iter().filter(|r| kana(&r.text) && over_parent(r) && !spaced(r)).map(|r| r.id).collect();
    if ruby.is_empty() {
        return;
    }
    cols.retain(|c| !ruby.contains(&c.id));
    for w in words.iter_mut().filter(|w| ruby.contains(&w.id)) {
        w.status = WordStatus::Ruby;
    }
}

/// 割注（1 つの列の中に 2 行に割って組んだ部分）を 1 つの語にまとめる。
///
/// 連名の告示の標題「〇個人情報保護委員会／金融庁告示第一号」や本文中の「（平成二十九年
/// 個人情報保護委員会／金融庁告示第一号）」は、機関名を右と左の半分の列に割って組む。
/// pdftotext は右半分を 1 語、左半分を字ごとの語（または続きの語の頭）にするので、
/// そのままでは別の列として読まれ、本文の間に「金融庁」の行が紛れ込む。
/// 親の列（「〇」や本文の列）の中心がちょうど右半分の左端・左半分の右端に当たり、親の列が
/// 割注のすぐ上で終わっているものを割注とし、右半分・左半分を「・」でつないで親の列に置く
/// （「個人情報保護委員会・金融庁」）。左半分の語が割注より下へ続く（「省告示第五号」の
/// ように続きの語とくっついた）ときは、割注の範囲の字だけを取り、残りは親の列に戻す。
/// 右半分が字間を空けた 1 字ずつの語（漢字・かな）のとき（「大　蔵　省」）はまとめて右半分とし、
/// 行頭の割注（上に親の列が無い）は、そのように字間を空けた右半分と左半分の上下がそろうものを
/// 割注とする。
#[path = "enclosed_warichu.rs"]
mod enclosed_warichu;
fn merge_warichu(cols: &mut Vec<Col>, protected: &std::collections::HashSet<usize>) -> Vec<(usize, usize)> {
    let mut merged = merge_warichu_pass(cols, false, protected);
    merged.extend(merge_warichu_pass(cols, true, protected));
    merged
}

/// [`merge_warichu`] の 1 回分。`ext` のときは、残った語から三段の割注だけを、字間を空けた段の語を広く
/// （読点を挟んだ「府、総」も）、中の段を字間を空けずに組んだ 1 語（「文部科学省、厚生労働省、農林水産省、」）も
/// 見て探す（本文の中の「（平成二十六年内閣府、総務省、…環境省告示第一号）」）。先に広く見ると、標題の
/// 「〇内閣府、総務省、…」の割注が崩れた。
fn merge_warichu_pass(cols: &mut Vec<Col>, ext: bool, protected: &std::collections::HashSet<usize>) -> Vec<(usize, usize)> {
    let mut merged = Vec::new();
    let mut i = 0;
    while i < cols.len() {
        if protected.contains(&cols[i].id) { i += 1; continue; }
        let w = cols[i].x1 - cols[i].x;
        let near = |a: f32, b: f32| (a - b).abs() < 0.6;
        // 右半分が左半分の長さに合わせて字間を空けて組まれ（「大　蔵　省」と「農林水産省」）、
        // 1 字ずつの語になっていることがある。同じ列に 1 em 程度の空きで続く 1 字の語をまとめる。
        // 機関名の字（漢字・かな）だけを対象にする（「1098、1099」の読点などを拾わない）。
        let name_char = |t: &str| {
            let mut cs = t.chars();
            matches!((cs.next(), cs.next()), (Some(c), None) if ('\u{4E00}'..='\u{9FFF}').contains(&c) || ('\u{3041}'..='\u{30FF}').contains(&c))
        };
        // 字間を空けた続きの語（「総」の後の「務」「省、」、読点を挟んだ「府、総」）。読点の付いた 3 字まで
        // （縦組みの読点「︑」も）。
        let comma = |c: char| matches!(c, '、' | '・' | '，' | '︑' | '︐');
        let spaced_part = |t: &str| {
            let cs: Vec<char> = t.chars().collect();
            name_char(t)
                || (cs.len() == 2 && name_char(&cs[0].to_string()) && (matches!(cs[1], '、' | '・' | '，') || (ext && comma(cs[1]))))
                || (ext && cs.len() == 3 && name_char(&cs[0].to_string()) && comma(cs[1]) && name_char(&cs[2].to_string()))
        };
        let mut spaced = vec![i];
        // 右半分の頭が複数字の語（「国家公安委員会、総」）でも、続きが字間を空けた語なら同じようにまとめる。
        let head_ok = name_char(&cols[i].text) || cols[i].y1 - cols[i].y >= w * 1.5;
        if head_ok {
            loop {
                let last = &cols[*spaced.last().unwrap()];
                let next = (0..cols.len()).find(|&j| {
                    !spaced.contains(&j) && !protected.contains(&cols[j].id)
                        && spaced_part(&cols[j].text)
                        && near(cols[j].x, last.x)
                        && near(cols[j].x1, last.x1)
                        && cols[j].y > last.y1 - w * 0.1
                        && cols[j].y - last.y1 <= w * 1.5
                });
                match next {
                    Some(j) => spaced.push(j),
                    None => break,
                }
            }
        }
        let r = if spaced.len() > 1 {
            let last = &cols[*spaced.last().unwrap()];
            Col {
                id: cols[i].id,
                x: cols[i].x,
                x1: cols[i].x1,
                y: cols[i].y,
                y1: last.y1,
                text: spaced.iter().map(|&j| cols[j].text.as_str()).collect(),
            }
        } else {
            cols[i].clone()
        };
        let n = r.text.chars().count();
        if !(4.0..=14.0).contains(&w) || n < 2 || r.y1 - r.y < w * 1.5 {
            i += 1;
            continue;
        }
        let parent = cols
            .iter()
            .enumerate()
            .any(|(j, p)| j != i && ((p.x + p.x1) / 2.0 - r.x).abs() < w * 0.3 && (p.y1 - r.y).abs() < w * 0.75 && p.y < r.y);
        // 左半分: 左端が右半分の 1 字分左で、右半分の範囲に掛かる語。字間を空けた右半分と同じ字の幅・同じ上下の
        // 範囲で、少し（半字分まで）離して組んだ機関名の語（「内　閣　府」と「農林水産省」）も。
        let organ = |t: &str| t.chars().count() >= 2 && t.ends_with(['省', '庁', '府', '会']) && t.chars().all(|c| name_char(&c.to_string()));
        let left: Vec<usize> = (0..cols.len())
            .filter(|&j| {
                let c = &cols[j];
                !spaced.contains(&j) && !protected.contains(&cols[j].id)
                    && (near(c.x, r.x - w)
                        || (spaced.len() > 1
                            && organ(&c.text)
                            && organ(&r.text)
                            && near(c.x1 - c.x, w)
                            && c.x1 <= r.x
                            && c.x1 >= r.x - w * 0.5
                            && (c.y - r.y).abs() < w * 0.5
                            && (c.y1 - r.y1).abs() < w * 0.5))
                    && c.y < r.y1 - w * 0.5
                    && c.y1 > r.y + w * 0.5
            })
            .collect();
        // 三段の割注（「金融庁」「財務省」「経済産業省」）: 上の段（右半分）と中の段が字間を空けた 1 字ずつの語で、
        // 中の段が親の列に並び、下の段がさらに 1 字分左。上下の範囲がそろうものに限る。
        let within = |c: &Col| c.y > r.y - w * 0.5 && c.y1 < r.y1 + w * 0.5;
        // 中の段は、字間を空けずに組んだ 1 語（「文部科学省、厚生労働省、農林水産省、」）のこともある。
        let middle = (left.len() > 1 && left.iter().all(|&j| name_char(&cols[j].text) && near(cols[j].x, r.x - w)))
            || (ext && left.len() == 1 && cols[left[0]].text.chars().count() >= 3 && near(cols[left[0]].x, r.x - w));
        let third: Vec<usize> = if spaced.len() > 1 && middle {
            (0..cols.len()).filter(|&j| !spaced.contains(&j) && !protected.contains(&cols[j].id) && !left.contains(&j) && near(cols[j].x, r.x - w * 2.0) && near(cols[j].x1, r.x - w) && within(&cols[j])).collect()
        } else {
            Vec::new()
        };
        if !third.is_empty() && left.iter().all(|&j| within(&cols[j])) {
            let mid = r.x - w / 2.0;
            let column_at = |y: f32, above: bool| {
                cols.iter().enumerate().any(|(j, p)| {
                    !spaced.contains(&j)
                        && !left.contains(&j)
                        && !third.contains(&j)
                        && ((p.x + p.x1) / 2.0 - mid).abs() < w * 0.3
                        && if above { (p.y1 - y).abs() < w * 0.75 && p.y < y } else { (p.y - y).abs() < w * 0.75 }
                })
            };
            // 広く見るときは、上の親の列が本文（4 字以上で「〇」で始まらない。標題の割注は別に組む）のものだけ。
            let body_above = !ext
                || cols.iter().any(|p| {
                    ((p.x + p.x1) / 2.0 - mid).abs() < w * 0.3
                        && (p.y1 - r.y).abs() < w * 0.75
                        && p.y < r.y
                        && p.text.chars().count() >= 4
                        && !p.text.trim_start().starts_with('〇')
                });
            if body_above && column_at(r.y, true) && column_at(r.y1, false) {
                let text_of = |js: &[usize]| {
                    let mut v: Vec<&Col> = js.iter().map(|&j| &cols[j]).collect();
                    v.sort_by(|a, b| a.y.total_cmp(&b.y));
                    v.iter().flat_map(|c| c.text.chars()).filter(|ch| !ch.is_whitespace()).collect::<String>()
                };
                cols[i] = Col {
                    id: r.id,
                    x: r.x - w,
                    x1: r.x,
                    y: r.y,
                    y1: r.y1,
                    // 読点で終わる段（「…財務省、」）の後ろには「・」を挟まない。
                    text: [r.text.clone(), text_of(&left), text_of(&third)].iter().fold(String::new(), |mut acc, part| {
                        if !acc.is_empty() && !(ext && acc.ends_with(comma)) {
                            acc.push('・');
                        }
                        acc.push_str(part);
                        acc
                    }),
                };
                let mut gone: Vec<usize> = left.iter().chain(third.iter()).copied().chain(spaced.iter().skip(1).copied()).collect();
                gone.sort_unstable();
                merged.extend(gone.iter().map(|&j| (cols[j].id, r.id)));
                let removed = gone.iter().filter(|&&j| j < i).count();
                for &j in gone.iter().rev() {
                    cols.remove(j);
                }
                i -= removed;
                i += 1;
                continue;
            }
        }
        if ext {
            i += 1;
            continue;
        }
        // 行頭の割注（本文の途中で列が改まった直後）は上に親の列が無い。そのときは右半分が字間を
        // 空けて左半分の長さに合わせてあり（「大　蔵　省」と「農林水産省」）、上下の範囲がそろって
        // いるものに限る（隣り合う本文の列や表の見出しと取り違えないため）。
        let balanced = || {
            if spaced.len() < 2 {
                return false;
            }
            let (top, bottom) = left.iter().fold((f32::MAX, f32::MIN), |(t, b), &j| (t.min(cols[j].y), b.max(cols[j].y1)));
            (top - r.y).abs() < w * 0.5 && (bottom - r.y1).abs() < w * 0.5
        };
        if left.is_empty() || left.iter().any(|&j| cols[j].y < r.y - w * 0.5) || !(parent || balanced()) {
            i += 1;
            continue;
        }
        let mut left_chars: Vec<(f32, char)> = Vec::new();
        let mut rest: Vec<Col> = Vec::new();
        for &j in &left {
            let c = &cols[j];
            let chars: Vec<char> = c.text.chars().collect();
            let pitch = (c.y1 - c.y) / chars.len().max(1) as f32;
            let k = chars.iter().enumerate().take_while(|(k, _)| c.y + pitch * (*k as f32 + 0.5) < r.y1).count();
            left_chars.extend(chars[..k].iter().enumerate().map(|(m, ch)| (c.y + pitch * m as f32, *ch)));
            if k < chars.len() {
                rest.push(Col {
                    id: c.id,
                    x: r.x - w / 2.0,
                    x1: r.x + w / 2.0,
                    y: c.y + pitch * k as f32,
                    y1: c.y1,
                    text: chars[k..].iter().collect(),
                });
            }
        }
        left_chars.sort_by(|a, b| a.0.total_cmp(&b.0));
        let left_text: String = left_chars.iter().map(|(_, ch)| ch).filter(|ch| !ch.is_whitespace()).collect();
        // 割注の後は親の列が続く（「告示第一号」）。表のセルの折り返しや行末の約物を割注と
        // 取り違えないため、すぐ下に親の列の続きがあるものに限る。
        let bottom = left_chars.last().map_or(r.y1, |(y, _)| r.y1.max(*y + w));
        let continues = !rest.is_empty()
            || cols.iter().enumerate().any(|(j, q)| {
                !spaced.contains(&j) && !left.contains(&j) && ((q.x + q.x1) / 2.0 - r.x).abs() < w * 0.3 && (q.y - bottom).abs() < w * 0.75
            });
        if left_text.is_empty() || !continues {
            i += 1;
            continue;
        }
        cols[i] = Col {
            id: r.id,
            x: r.x - w / 2.0,
            x1: r.x + w / 2.0,
            y: r.y,
            y1: r.y1,
            // 右半分が読点で終わる（「総務省、」）ときは「・」でつながない。
            text: if r.text.ends_with(['、', '・', '，']) { format!("{}{}", r.text, left_text) } else { format!("{}・{}", r.text, left_text) },
        };
        let mut gone: Vec<usize> = left.iter().copied().chain(spaced.iter().skip(1).copied()).collect();
        gone.sort_unstable();
        // 左半分の語で残りを列に戻したもの（`rest`）は、その語のまま組まれる。
        merged.extend(gone.iter().map(|&j| cols[j].id).filter(|id| !rest.iter().any(|c| c.id == *id)).map(|id| (id, r.id)));
        let mut removed = 0;
        for &j in gone.iter().rev() {
            cols.remove(j);
            if j < i {
                removed += 1;
            }
        }
        i -= removed;
        cols.extend(rest);
        i += 1;
    }
    merged
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
/// 段分けで使う罫線: 割らない表の枠 `(x0, x1, y0, y1)` と横線 `(y, x0, x1)`。
struct TierRules {
    tables: Vec<(f32, f32, f32, f32)>,
    /// 横組みの囲み（[`horizontal_block_boxes`]）。縦書きの列の段の境目でない cut は、囲みを横切らない。
    boxes: Vec<(f32, f32, f32, f32)>,
    hrules: Vec<(f32, f32, f32)>,
    /// 縦の罫線 `(x, y0, y1)`。
    vrules: Vec<(f32, f32, f32)>,
    /// 横倒しのページ（回した座標で組む）。
    sideways: bool,
}

/// 左端の全高の変更後・変更前枠と、直右で段の境目をまたぐ導入列を、通常の上下段の後に読む。
/// 導入列の直右に短い番号見出しがあり、その見出しと枠の上端が揃う場合だけ切り出す。
fn split_off_trailing_label_frame(cols: Vec<Col>, rules: &TierRules) -> Result<(Vec<Col>, Vec<Col>), Vec<Col>> {
    let top = cols.iter().map(|c| c.y).fold(f32::INFINITY, f32::min);
    let bottom = cols.iter().map(|c| c.y1).fold(f32::NEG_INFINITY, f32::max);
    let left = cols.iter().map(|c| c.x).fold(f32::INFINITY, f32::min);
    let center = |c: &Col| (c.x + c.x1) / 2.0;
    let found = label_frames(&cols, &rules.hrules, &rules.vrules).into_iter().find_map(|(x0, x1, y0, y1, mid)| {
        if !y0.is_finite() || !y1.is_finite() || y1 - y0 < (bottom - top) * 0.8 {
            return None;
        }
        let em = cols.iter().filter(|c| center(c) > x0 && center(c) < x1).map(|c| c.x1 - c.x).filter(|&w| w >= 3.0 && w <= 20.0).min_by(f32::total_cmp)?;
        // 枠の左に別の本文がある場合や、全高枠の外へ字がはみ出す場合は対象外。
        if x0 > left + em * 2.0 || cols.iter().any(|c| center(c) > x0 && center(c) < x1 && (c.y < y0 - em || c.y1 > y1 + em)) {
            return None;
        }
        let intro_x = cols.iter().filter(|c| center(c) > x1).map(center).min_by(f32::total_cmp)?;
        if intro_x - x1 > em * 2.5 {
            return None;
        }
        let intro: Vec<_> = cols.iter().filter(|c| (center(c) - intro_x).abs() <= 1.5).collect();
        let intro_top = intro.iter().map(|c| c.y).fold(f32::INFINITY, f32::min);
        let intro_bottom = intro.iter().map(|c| c.y1).fold(f32::NEG_INFINITY, f32::max);
        if intro_top > y0 + em * 3.0 || intro_top >= mid - em || intro_bottom <= mid + em || intro_bottom - intro_top < (y1 - y0) * 0.5 {
            return None;
        }
        let heading_x = cols.iter().filter(|c| center(c) > intro_x + em * 0.5).map(center).min_by(f32::total_cmp)?;
        if heading_x - intro_x > em * 3.0 {
            return None;
        }
        let mut heading: Vec<_> = cols.iter().filter(|c| (center(c) - heading_x).abs() <= 1.5).collect();
        heading.sort_by(|a, b| a.y.total_cmp(&b.y));
        let text: String = heading.iter().flat_map(|c| c.text.chars()).filter(|c| !c.is_whitespace()).collect();
        let number = |c: char| "一二三四五六七八九十百0123456789０１２３４５６７８９".contains(c);
        let prefix = text.chars().take_while(|&c| number(c)).count();
        // 節番号と短い標題だけを取る。直右の前の記事の本文列は取らない。
        if prefix == 0 || prefix > 3 || text.chars().count() <= prefix || text.chars().count() > 16
            || heading.first()?.y > intro_top + em || (heading.first()?.y - y0).abs() > em * 3.0
            || heading.iter().any(|c| c.y1 > mid - em)
        {
            return None;
        }
        let rest: Vec<_> = cols.iter().filter(|c| center(c) > heading_x + 1.5).collect();
        // 右側が上下の独立した段に分かれる L 字配置だけ。右の前文なども中央境界をまたぐ場合、
        // 枠だけを除くと、その長い列が上下の項目列より後へ回るため従来の段分割に任せる。
        if rest.iter().any(|c| c.y < mid - em && c.y1 > mid + em) {
            return None;
        }
        (rest.iter().filter(|c| c.y1 < mid).count() >= 3 && rest.iter().filter(|c| c.y > mid).count() >= 3).then_some(heading_x)
    });
    let Some(edge) = found else { return Err(cols) };
    Ok(cols.into_iter().partition(|c| center(c) > edge + 1.5))
}

fn split_tiers(cols: Vec<Col>, rules: &TierRules) -> Vec<Vec<Col>> {
    let (cols, trailing) = match split_off_trailing_label_frame(cols, rules) {
        Ok((rest, frame)) => (rest, Some(frame)),
        Err(cols) => (cols, None),
    };
    let mut out = Vec::new();
    split_tiers_rec(cols, 0, rules, false, &mut out);
    rejoin_split_labels(&mut out);
    rejoin_item_numbers(&mut out);
    rejoin_honoree_names(&mut out);
    rejoin_value_cells(&mut out);
    rejoin_prose_label_values(&mut out, rules);
    out.extend(trailing);
    out.retain(|t| !t.is_empty());
    out
}

/// 項目の番号（「１」「２」「三」「（４）」）だけの段が、すぐ下の段の同じ列の書き出しに続くなら下の段にまとめる
/// （横倒しの品種登録の告示で、番号と項目の間の 1 字ほどの空きで段に切られ、番号だけの行が続いていた）。
fn rejoin_item_numbers(tiers: &mut [Vec<Col>]) {
    let center = |c: &Col| (c.x + c.x1) / 2.0;
    let number = |t: &str| {
        let t = t.trim_start_matches(['（', '(']).trim_end_matches([')', '）']);
        // 漢数字だけの段は目次のページ番号のことがあるので除く。
        !t.is_empty() && t.chars().count() <= 2 && t.chars().all(|c| c.is_ascii_digit() || ('０'..='９').contains(&c))
    };
    for i in 0..tiers.len() {
        if tiers[i].len() < 3 || !tiers[i].iter().all(|c| number(&c.text)) {
            continue;
        }
        let bottom = tiers[i].iter().map(|c| c.y1).fold(f32::NEG_INFINITY, f32::max);
        let em = tiers[i].iter().map(|c| c.x1 - c.x).fold(0.0, f32::max).max(6.0);
        let Some(j) = (0..tiers.len()).find(|&j| {
            j != i && {
                let top = tiers[j].iter().map(|c| c.y).fold(f32::INFINITY, f32::min);
                let bottom_j = tiers[j].iter().map(|c| c.y1).fold(f32::NEG_INFINITY, f32::max);
                // 下の段は項目の文の段（座標の表の欄のような低い段ではない）。
                top >= bottom - 1.0 && top - bottom < em * 2.0 && bottom_j - top > 100.0
            }
        }) else {
            continue;
        };
        let followed = tiers[i]
            .iter()
            .filter(|n| tiers[j].iter().any(|c| (center(c) - center(n)).abs() < 3.0 && c.y >= n.y1 - 1.0 && c.y - n.y1 < em * 2.0))
            .count();
        if followed * 10 >= tiers[i].len() * 8 {
            let moved = std::mem::take(&mut tiers[i]);
            tiers[j].extend(moved);
        }
    }
}

/// 叙位・叙勲の名簿で、位・勲章の列（「正三位に叙する」「瑞宝双光章を授ける」）の間に下揃えで組まれた受ける人の
/// 氏名（「二橋　正弘」）が、別の段に切られたものを上の段に戻す（氏名がページの段の最後にまとめて並んでいた）。
/// 下の段は、すぐ下に続く 6 字以下の名前の語（記号・数字の無いもの）だけで、上の段の幅の中にあるもの。
fn rejoin_honoree_names(tiers: &mut [Vec<Col>]) {
    let honor = |t: &str| {
        let t = t.trim_end_matches("（各通）");
        t.ends_with("に叙する") || t.ends_with("を授ける")
    };
    let name = |t: &str| (1..=6).contains(&t.chars().count()) && t.chars().all(|c| !c.is_ascii() && !"、。，．・（）()「」〔〕〇○◎－―〃".contains(c) && !c.is_numeric());
    for i in 1..tiers.len() {
        let (above, below) = (&tiers[i - 1], &tiers[i]);
        if below.len() < 2 || !below.iter().all(|c| name(&c.text) && c.y1 - c.y >= (c.x1 - c.x) * 0.9) || !above.iter().any(|c| honor(&c.text)) {
            continue;
        }
        let ext = |b: &[Col]| b.iter().fold((f32::INFINITY, f32::NEG_INFINITY, f32::INFINITY, f32::NEG_INFINITY), |a, c| (a.0.min(c.x), a.1.max(c.x1), a.2.min(c.y), a.3.max(c.y1)));
        let (ax0, ax1, _, ay1) = ext(above);
        let (bx0, bx1, by0, _) = ext(below);
        let em = below.iter().map(|c| c.x1 - c.x).fold(0.0, f32::max).max(6.0);
        if bx0 < ax0 - em || bx1 > ax1 + em || by0 > ay1 + em * 2.0 {
            continue;
        }
        let moved = std::mem::take(&mut tiers[i]);
        tiers[i - 1].extend(moved);
    }
}

/// 罫線の無い縦組みの小さな表（道路の区域の告示の「区間」「変更前後別」「敷地の幅員」「延長」）で、数の欄
/// （「一六・九一〜二四・五六」「〇・〇五五」）が欄の間の空きで下の段に切られたものを上の段に戻す（区間の行の後に
/// 次の項目「（四）図面縦覧場所」が入り、幅員・延長の数がその後にまとめて並んでいた）。下の段は、すぐ下に続く
/// 12 語以下の低い段で、数の語が 2 つ以上あり、どの数の語も上の段の列と同じ x にあり、ほかの語は欄見出し・単位の
/// 短い語（6 字以下）だけのもの。
fn rejoin_value_cells(tiers: &mut [Vec<Col>]) {
    let center = |c: &Col| (c.x + c.x1) / 2.0;
    let number = value_number;
    let ext = |b: &[Col]| b.iter().fold((f32::INFINITY, f32::NEG_INFINITY, f32::INFINITY, f32::NEG_INFINITY), |a, c| (a.0.min(c.x), a.1.max(c.x1), a.2.min(c.y), a.3.max(c.y1)));
    for i in 1..tiers.len() {
        let Some(j) = (0..i).rev().find(|&j| !tiers[j].is_empty()) else { continue };
        let (above, below) = (&tiers[j], &tiers[i]);
        if below.is_empty() || below.len() > 12 {
            continue;
        }
        let (ax0, ax1, _, ay1) = ext(above);
        let (bx0, bx1, by0, by1) = ext(below);
        let em = below.iter().map(|c| c.x1 - c.x).fold(0.0, f32::max).max(6.0);
        if bx0 < ax0 - 1.0 || bx1 > ax1 + 1.0 || by0 < ay1 - 1.0 || by0 - ay1 > em * 2.0 || by1 - by0 > 100.0 {
            continue;
        }
        let values: Vec<&Col> = below.iter().filter(|c| number(&c.text)).collect();
        let on_column = |v: &Col| above.iter().any(|c| (center(c) - center(v)).abs() < 3.0 && ay1 - c.y1 < em * 3.0);
        if values.is_empty() || !values.iter().all(|v| on_column(v)) || !below.iter().all(|c| number(&c.text) || c.text.chars().count() <= 6) {
            continue;
        }
        let moved = std::mem::take(&mut tiers[i]);
        tiers[j].extend(moved);
    }
}

/// 表の欄の数だけの語（「一六・九一〜二四・五六」「〇・〇五五」「1,234」）。
fn value_number(t: &str) -> bool {
    t.chars().any(|c| "〇一二三四五六七八九".contains(c) || c.is_ascii_digit() || ('０'..='９').contains(&c))
        && t.chars().all(|c| "〇一二三四五六七八九十百千・〜．.，,－".contains(c) || c.is_ascii_digit() || ('０'..='９').contains(&c))
}

/// 罫線の無い縦組みの表（政治団体の届出の一覧など）。欄（名称・代表者の氏名・主たる事務所の所在地…）ごとの
/// 低い段が上下に 3 つ以上並び、各行（団体ごと）の列がどの段でも同じ x から始まる。段ごとに読むと欄ごとの
/// 一覧になり、どの団体の代表者か分からなくなるので、行の頭の x（半分以上の段に列のある x）で行に分け、
/// 行ごとに欄を全角空白で区切って 1 行に組む。`bands` の頭からいくつの段を使ったかと、組んだ行を返す。
/// `mid_line` は縦置きのページの高さの半分（上半分と下半分の表を 1 つの表にしない）。
#[cfg(test)]
fn record_bands(bands: &[Vec<Col>], mid_line: Option<f32>) -> Option<(usize, Vec<Line>)> {
    record_bands_with_suffix_context(bands,mid_line,&[],&[])
}
fn record_bands_with_suffix_context(bands: &[Vec<Col>], mid_line: Option<f32>, header_units: &[Unit], hrules: &[(f32,f32,f32)]) -> Option<(usize, Vec<Line>)> {
    let center = |c: &Col| (c.x + c.x1) / 2.0;
    let ext = |b: &[Col]| {
        (
            b.iter().map(|c| c.x).fold(f32::INFINITY, f32::min),
            b.iter().map(|c| c.x1).fold(f32::NEG_INFINITY, f32::max),
            b.iter().map(|c| c.y).fold(f32::INFINITY, f32::min),
            b.iter().map(|c| c.y1).fold(f32::NEG_INFINITY, f32::max),
        )
    };
    let first = bands.first()?;
    if first.len() < 3 {
        return None;
    }
    let (fx0, fx1, _, _) = ext(first);
    let mut n = 0;
    let mut prev_top = f32::NEG_INFINITY;
    // 低い段の決まり（130pt 以下、100pt を越えるなら短い語だけ）を越える高い段（長い団体名・所在地の欄）。
    // 表の段と見るには [`tall_record_band`] の確かめが要る。
    let mut tall_bands = Vec::new();
    for b in bands {
        // 字間を空けた欄見出しの 1 字だけの段（「番」、2 つの欄見出しの末字「称」「称」）は、表の続きとして数える。
        let label = label_band(b) || (n > 0 && label_tail_band(b));
        if b.len() < 2 && !label {
            break;
        }
        if label {
            n += 1;
            continue;
        }
        let (x0, x1, y0, y1) = ext(b);
        // 頭の段に表の右の文（告示の本文・日付）が入ることがあるので、2 列以上の段は狭い方の幅に対する重なりで見る。
        let inter = (x1.min(fx1) - x0.max(fx0)).max(0.0);
        let mut xs: Vec<f32> = b.iter().map(center).collect();
        xs.sort_by(f32::total_cmp);
        xs.dedup_by(|a, b| (*a - *b).abs() < 3.0);
        let overlap = if xs.len() >= 2 { inter / (x1 - x0).min(fx1 - fx0).max(1.0) } else { inter / (x1 - x0).max(fx1 - fx0).max(1.0) };
        let upright = b.iter().filter(|c| c.y1 - c.y >= (c.x1 - c.x) * 0.9).count();
        let mut mids: Vec<f32> = b.iter().map(|c| (c.y + c.y1) / 2.0).collect();
        mids.sort_by(f32::total_cmp);
        let mid = mids[mids.len() / 2];
        // 表はページの上半分・下半分（2 段分、約 360pt）ずつ組まれ、上下に同じ形の別の表が並ぶことがあるので、それを
        // 越えて続けない。
        let first_top = ext(first).2;
        // 100pt を越える段は、短い語（6 字以下。「〃」「2026－ＴＡ」の欄）だけのもの（文の段と取り違えないため）。
        let tall = y1 - y0 > 100.0 && b.iter().any(|c| c.text.chars().count() > 6);
        let across = mid_line.is_some_and(|m| (first_top < m) != (y0 < m));
        if overlap < 0.7 || mid <= prev_top || upright * 10 < b.len() * 9 || y1 - first_top > 370.0 || across {
            break;
        }
        if y1 - y0 > 130.0 || tall {
            tall_bands.push(n);
        }
        prev_top = mid;
        n += 1;
    }
    // 高い段は、確かめられなかった段の手前までを表とする。
    let mut suffix_tops = None;
    if let Some(&t) = tall_bands.iter().find(|&&t| t < n && !tall_record_band(&bands[..n], t, &tall_bands)) {
        if t+1==n && crate::record_suffix::has_prior_record_header(header_units) {
            suffix_tops = crate::record_suffix::infer_stacked_suffix(&bands[..n],t,hrules).map(|tops|(t,tops));
        }
        if suffix_tops.is_none() { n=t; }
    }
    if n == 2 {
        return name_pair_bands(&bands[..2]);
    }
    // 字間を空けた 1 字ずつの見出しだけの段（「種　類」「す　る　区　域」）から始まる組は表ではない（1 字の語が
    // 並ぶ表の段（都道府県名の頭の字「茨」「栃」…）は語が多い）。
    if n < 3 || (first.len() < 10 && first.iter().all(|c| c.text.chars().count() == 1)) {
        return None;
    }
    let all_bands = bands;
    // 上下に並ぶ段の頭からの最も長い組で、行の頭が揃うもの（ページの上半分と下半分は別の表のことがある）。
    let fit = |n: usize| -> Option<Vec<f32>> {
        let bands = &all_bands[..n];
        // 欄見出しの字だけの段は、行の頭を決める段の数に入れない。
        let real = bands.iter().filter(|b| !label_band(b) && !label_tail_band(b)).count();
        if real < 2 {
            return None;
        }
        // 段ごとの列の x（右から）。欄見出しの下の元号の注記（「（令和）」）は列に数えない（行が詰んで書き出しの少ない
        // 段で、行の頭に揃わない書き出しになる）。
        let era = |c: &&Col| ["（令和）", "（平成）", "（昭和）"].contains(&c.text.as_str());
        let columns: Vec<Vec<f32>> = bands
            .iter()
            .map(|b| {
                let mut xs: Vec<f32> = b.iter().filter(|c| !era(c)).map(center).collect();
                xs.sort_by(|a, b| b.total_cmp(a));
                xs.dedup_by(|a, b| (*a - *b).abs() < 3.0);
                xs
            })
            .collect();
        // 欄の書き出しの列（右隣 1.5 字以内に列の無い列。折り返しの列は除く）。
        let em = bands[0].iter().map(|c| c.x1 - c.x).fold(0.0, f32::max).max(6.0);
        let starts: Vec<Vec<f32>> = columns
            .iter()
            .map(|cs| cs.iter().copied().filter(|&x| !cs.iter().any(|&o| o > x + 1.0 && o - x < em * 1.5)).collect())
            .collect();
        // 行の頭は、半分以上の段で欄の書き出しになる x。
        let mut all: Vec<f32> = starts.iter().flatten().copied().collect();
        all.sort_by(|a, b| b.total_cmp(a));
        all.dedup_by(|a, b| (*a - *b).abs() < 3.0);
        // 行ごとに 1 列の詰んだ表は、行の頭が右隣の行と 1 字ほどしか離れず書き出しに見えないので、8 割以上の段に
        // 列のある x も行の頭にする。
        let mut every: Vec<f32> = columns.iter().flatten().copied().collect();
        every.sort_by(|a, b| b.total_cmp(a));
        every.dedup_by(|a, b| (*a - *b).abs() < 3.0);
        let mut anchors: Vec<f32> = all
            .into_iter()
            .filter(|&x| starts.iter().filter(|s| s.iter().any(|&y| (y - x).abs() < 3.0)).count() * 2 >= real.max(4))
            .chain(every.into_iter().filter(|&x| columns.iter().filter(|c| c.iter().any(|&y| (y - x).abs() < 3.0)).count() * 10 >= real * 8))
            .collect();
        anchors.sort_by(|a, b| b.total_cmp(a));
        anchors.dedup_by(|a, b| (*a - *b).abs() < 3.0);
        // 詰んだ行の間の 1 行分以上の空きにある、区分の番号（「（二）」）で始まる列（表の途中の「（二）その他の政治団体」
        // の見出し）も行の頭にする（前の行の欄に入っていた）。
        let pitch = anchors.windows(2).map(|w| w[0] - w[1]).fold(f32::INFINITY, f32::min);
        if pitch < em * 2.0 {
            let gaps: Vec<(f32, f32)> = anchors.windows(2).filter(|w| w[0] - w[1] > pitch * 1.6).map(|w| (w[0] - pitch * 0.6, w[1] + pitch * 0.6)).collect();
            let section = |t: &str| {
                t.strip_prefix('（').and_then(|r| r.split_once('）')).is_some_and(|(n, _)| {
                    (1..=2).contains(&n.chars().count()) && n.chars().all(|c| "一二三四五六七八九十イロハニホヘトチリヌ".contains(c) || c.is_ascii_digit() || ('０'..='９').contains(&c))
                })
            };
            let extra: Vec<f32> = bands
                .iter()
                .flatten()
                .filter(|c| section(&c.text) && !bands.iter().flatten().any(|o| (center(o) - center(c)).abs() < 3.0 && o.y < c.y))
                .map(center)
                .filter(|&x| gaps.iter().any(|&(hi, lo)| x < hi && x > lo))
                .collect();
            anchors.extend(extra);
            anchors.sort_by(|a, b| b.total_cmp(a));
            anchors.dedup_by(|a, b| (*a - *b).abs() < 3.0);
        }
        if anchors.len() < 3 {
            return None;
        }
        // どの段でも、欄の書き出しの 7 割以上が行の頭にある。行の頭より右の書き出し（段ごとにずれて組まれた欄見出し、
        // 表の前の文）は数えない。
        let lead = anchors[0];
        let aligned = |k: usize| {
            let inside: Vec<f32> = starts[k].iter().copied().filter(|&x| x <= lead + 3.0).collect();
            // 2 行に組んだ欄見出し（「代表者」「の氏名」）は行の頭から半字ほどずれる。
            let on = inside.iter().filter(|&&x| anchors.iter().any(|&a| (a - x).abs() < (em * 0.6).max(3.0))).count();
            !inside.is_empty() && starts[k].len() - inside.len() <= 2 && on * 10 >= inside.len() * 7
        };
        (0..n).all(aligned).then_some(anchors)
    };
    let limit = n;
    let (mut n, anchors) = (3..=n).rev().find_map(|m| fit(m).map(|a| (m, a)))?;
    // 行頭の確かめでは、複数の変更事項の小行も独立した行として保つ。末尾に日付だけの帯が
    // 余った場合は、既に決めた行頭と同じ列の日付だけをその行へ足す（小行をまとめ直さない）。
    if n < limit {
        let mut dates: Vec<&Col> = all_bands[n].iter().collect();
        dates.sort_by(|a, b| {
            let (ax, bx) = (center(a), center(b));
            if (ax - bx).abs() < 3.0 { a.y.total_cmp(&b.y) } else { bx.total_cmp(&ax) }
        });
        let columns: Vec<_> = dates.chunk_by(|a, b| (center(a) - center(b)).abs() < 3.0).collect();
        let date_only = columns.len() >= 3 && columns.len() * 2 >= anchors.len() && columns.iter().all(|column| {
            let x = center(column[0]);
            let text: String = column.iter().flat_map(|c| c.text.chars()).filter(|c| !c.is_whitespace()).collect();
            let parts: Vec<_> = text.split('、').collect();
            anchors.iter().any(|&a| (a - x).abs() < 3.0)
                && parts.len() == 3
                && parts.iter().all(|p| !p.is_empty() && p.chars().count() <= 3 && p.chars().all(|c| "〇零一二三四五六七八九十0123456789０１２３４５６７８９".contains(c)))
        });
        if date_only {
            n += 1;
        }
    }
    let bands = &all_bands[..n];
    let mut anchors = anchors;
    anchors.extend(record_section_anchors(bands, &anchors));
    anchors.sort_by(|a, b| b.total_cmp(a));
    anchors.dedup_by(|a,b| *a == *b);
    // 字の大きさ（縦組みの語の幅の中央値）。
    let em = {
        let mut ws: Vec<f32> = bands.iter().flatten().filter(|c| c.y1 - c.y >= (c.x1 - c.x) * 1.5).map(|c| c.x1 - c.x).collect();
        ws.sort_by(f32::total_cmp);
        ws.get(ws.len() / 2).copied().unwrap_or(8.0)
    };
    let record_of = |x: f32| anchors.iter().rposition(|&a| x <= a + 3.0).unwrap_or(0);
    let mut lines = Vec::new();
    // 行の頭から行の間隔の 1.5 倍より右の列は表の前の文（「次のとおり公告する。」「令和８年７月31日」）。列ごとに
    // 先に書く。その内側（行の頭の右）は、段ごとにずれて組まれた欄見出しで、行の頭の行（欄見出しの行）に入れる。
    let pitch = anchors.windows(2).map(|w| w[0] - w[1]).fold(f32::INFINITY, f32::min);
    let outside = anchors[0] + pitch * 1.5;
    let mut before: Vec<&Col> = bands.iter().flatten().filter(|c| center(c) > outside).collect();
    before.sort_by(|a, b| {
        let (ca, cb) = (center(a), center(b));
        if (ca - cb).abs() < 3.0 { a.y.total_cmp(&b.y) } else { cb.total_cmp(&ca) }
    });
    for col in before.chunk_by(|a, b| (center(a) - center(b)).abs() < 3.0) {
        lines.push(Line::text(col.iter().map(|c| c.text.as_str()).collect(), col.iter().map(|c| c.id).collect()));
    }
    let record_of = |x: f32| if x > outside { usize::MAX } else { record_of(x) };
    // 2 行に組んだ欄見出し（「代表者」「の氏名」）は行の頭をまたいで組まれ、右の列が行の頭から半字ほど右にずれる。
    // 同じ段の 1.5 字以内の左に、行の頭の左の欄見出しの列（「名称」「氏名」…で終わる）があれば、右の列もその行に入れる。
    let anchors = &anchors;
    let record_of = &record_of;
    let rows: std::collections::HashMap<usize, usize> = bands
        .iter()
        .flat_map(|b| {
            b.iter().map(move |c| {
                let x = center(c);
                let straddle = anchors.iter().position(|&a| x > a + 3.0 && x <= a + (em * 0.6).max(3.0)).filter(|&r| {
                    b.iter().any(|o| center(o) < anchors[r] - 3.0 && x - center(o) < em * 1.5 && RECORD_HEADS.iter().any(|h| o.text.ends_with(h)))
                        && x <= outside
                });
                (c.id, straddle.unwrap_or_else(|| record_of(x)))
            })
        })
        .collect();
    let record_of = |c: &Col| rows[&c.id];
    // 行の頭の行のいちばん右の列が日付（「令和八年九月一日」）で始まるなら、表の右上の日付・署名と表の前の見出しで、
    // 行ではない。列ごとに先に書く。
    let mut lead_cols: Vec<&Col> = bands.iter().flatten().filter(|c| record_of(c) == 0).collect();
    lead_cols.sort_by(|a, b| {
        let (ca, cb) = (center(a), center(b));
        if (ca - cb).abs() < 3.0 { a.y.total_cmp(&b.y) } else { cb.total_cmp(&ca) }
    });
    // 表の右の告示の本文の列（「…指定した。」）の後に日付が来ることもある。
    let dated = lead_cols.iter().take_while(|c| !c.text.starts_with("令和")).all(|c| c.text.ends_with('。') || center(c) - center(lead_cols[0]) > -3.0)
        && lead_cols.iter().any(|c| c.text.starts_with("令和") && c.text.ends_with('日'));
    // 欄見出しの列が行の頭の行にあれば（日付・署名の列の左に並ぶ）、その列は見出しの行として残し、右の列だけ先に書く。
    let mut printed: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let first = if anchors.len() > 3 && dated {
        let cols: Vec<&[&Col]> = lead_cols.chunk_by(|a, b| (center(a) - center(b)).abs() < 3.0).collect();
        let label = |col: &[&Col]| {
            let t: String = col.iter().map(|c| c.text.as_str()).collect();
            RECORD_HEADS.iter().any(|h| t.ends_with(h))
        };
        let label_x = cols.iter().filter(|col| label(col)).map(|col| center(col[0])).fold(f32::NEG_INFINITY, f32::max);
        for col in cols.iter().filter(|col| center(col[0]) > label_x + 3.0) {
            lines.push(Line::text(col.iter().map(|c| c.text.as_str()).collect(), col.iter().map(|c| c.id).collect()));
            printed.extend(col.iter().map(|c| c.id));
        }
        if label_x.is_finite() { 0 } else { 1 }
    } else {
        0
    };
    let record_of = |c: &Col| if printed.contains(&c.id) { usize::MAX } else { record_of(c) };
    // 1 つの段に重ねた欄（指定番号と車名及び型式、名称・代表者の氏名・異動事項…）は、見出しの行の欄見出しの高さで分ける。
    let bounds: Vec<Vec<f32>> = {
        let heads = |r: usize| bands.iter().flatten().filter(|c| record_of(c) == r && RECORD_HEADS.iter().any(|h| c.text.ends_with(h))).count();
        let header = (first..anchors.len()).max_by_key(|&r| heads(r)).filter(|&r| heads(r) >= 2);
        bands
            .iter()
            .enumerate()
            .map(|(k,b)| {
                if let Some((_,tops))=suffix_tops.as_ref().filter(|(t,_)|*t==k) {
                    return tops.clone();
                }
                match header {
                Some(h) => {
                    // 欄見出しの列（「名称」「氏名」…で終わる語のある列と、2 行の欄見出しの隣の列）だけ。区分の見出し
                    // （「（一）政党の支部」）の列は見ない。
                    let ws: Vec<&Col> = b.iter().filter(|c| record_of(c) == h).collect();
                    // 字間を空けた欄見出し（「摘　要」）は列ごとにつないで見る。
                    let column_text = |x: f32| {
                        let mut cs: Vec<&&Col> = ws.iter().filter(|c| (center(c) - x).abs() < 3.0).collect();
                        cs.sort_by(|a, b| a.y.total_cmp(&b.y));
                        cs.iter().map(|c| c.text.as_str()).collect::<String>()
                    };
                    let label_x: Vec<f32> = ws
                        .iter()
                        .map(|c| center(c))
                        .filter(|&x| {
                            let t = column_text(x);
                            RECORD_HEADS.iter().any(|h| t.ends_with(h) || t.contains(&format!("{h}（")))
                        })
                        .collect();
                    let section: Vec<f32> = ws.iter().filter(|c| c.text.starts_with('（') && c.text.chars().count() <= 4 && c.text.ends_with('）')).map(|c| center(c)).collect();
                    let ws: Vec<&Col> = ws
                        .into_iter()
                        .filter(|c| label_x.iter().any(|&x| (center(c) - x).abs() < em * 1.2) && !section.iter().any(|&x| (center(c) - x).abs() < 3.0))
                        .collect();
                    field_tops(&ws, em)
                }
                None => Vec::new(),
                }
            })
            .collect()
    };
    for r in first..anchors.len() {
        let mut fields: Vec<String> = Vec::new();
        let mut ids: Vec<usize> = Vec::new();
        let mut last: Option<(usize, f32)> = None;
        for (k, b) in bands.iter().enumerate() {
            let mut ws: Vec<&Col> = b.iter().filter(|c| record_of(c) == r).collect();
            // 同じ列の語（縦中横の「13」は列の中心から少しずれる）は上から。
            let same = (em * 0.6).max(3.0);
            ws.sort_by(|a, b| {
                let (ca, cb) = (center(a), center(b));
                if (ca - cb).abs() < same { a.y.total_cmp(&b.y) } else { cb.total_cmp(&ca) }
            });
            if !ws.is_empty() {
                let text = stacked_fields(&ws, &bounds[k], em);
                // 上の段の欄の続き（「八、」と「四、二〇」が別の段に割れた日付、欄見出しの末字だけの段の「称」）は区切らない。
                let continues = |f: &String| {
                    last.is_some_and(|(j, x)| j + 1 == k && (f.ends_with('、') || label_tail_band(b) && (center(ws[0]) - x).abs() < 3.0))
                };
                match fields.last_mut() {
                    Some(f) if continues(f) => f.push_str(&text),
                    _ => fields.push(text),
                }
                ids.extend(ws.iter().map(|c| c.id));
                last = Some((k, center(ws[ws.len() - 1])));
            }
        }
        if !fields.is_empty() {
            lines.push(Line::text(fields.join("\u{3000}"), ids));
        }
    }
    Some((n, lines))
}

/// [`record_bands`] の見出しの行の 1 つの段の欄見出しの語から、段の中に重ねた欄の境の高さを求める。
/// 字間を空けた 1 字ずつの欄見出し（「政　治　団　体」）と 2 行の欄見出し（「代表者」「の氏名」）は 1 つの欄。
fn field_tops(heads: &[&Col], em: f32) -> Vec<f32> {
    let mut by_y: Vec<&Col> = heads.to_vec();
    by_y.sort_by(|a, b| a.y.total_cmp(&b.y));
    let mut groups: Vec<(f32, f32, bool)> = Vec::new();
    for c in by_y {
        let single = c.text.chars().count() == 1;
        match groups.last_mut() {
            Some((_, y1, one)) if c.y - *y1 < em * 0.75 || (single && *one) => {
                *y1 = y1.max(c.y1);
                *one = *one && single;
            }
            _ => groups.push((c.y, c.y1, single)),
        }
    }
    // 欄の境は、前の欄見出しの下端のすぐ後（欄の値は欄見出しより上から始まることがある）。
    groups.windows(2).map(|w| w[0].1 + em * 0.25).collect()
}

/// [`record_bands`] の 1 つの行の 1 つの段の語を組む。段に重ねた欄（`tops` は欄の境の高さ）ごとに、列の右
/// から・列の中は上から並べ、欄を全角空白で区切る（列ごとに読むと「公明党衆議大森江里子会計責任者の天野明美院比例区東…」と
/// 欄が混ざっていた）。
fn stacked_fields(ws: &[&Col], tops: &[f32], em: f32) -> String {
    let center = |c: &Col| (c.x + c.x1) / 2.0;
    let same = (em * 0.6).max(3.0);
    let field = |c: &Col| tops.iter().filter(|&&t| c.y >= t).count();
    let mut fields: Vec<Vec<&Col>> = vec![Vec::new(); tops.len() + 1];
    for &c in ws {
        fields[field(c)].push(c);
    }
    fields
        .into_iter()
        .filter(|g| !g.is_empty())
        .map(|mut g| {
            g.sort_by(|a, b| {
                let (ca, cb) = (center(a), center(b));
                if (ca - cb).abs() < same { a.y.total_cmp(&b.y) } else { cb.total_cmp(&ca) }
            });
            g.iter().map(|c| c.text.as_str()).collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\u{3000}")
}

/// [`record_bands`] の表の欄見出しの末尾の語（「政治団体の名称」「代表者の氏名」「解散年月日」「主たる事務所の所在地」
/// 「施設名」「指定番号」「車名及び型式」「製作者等の住所」「摘要」）。
const RECORD_HEADS: [&str; 11] = ["名称", "氏名", "年月日", "所在地", "名", "番号", "型式", "住所", "摘要", "種類", "関係",
];

/// [`record_bands`] の、字間を空けた欄見出しの 1 字だけの段（「番」）。
fn label_band(b: &[Col]) -> bool {
    b.len() == 1 && b[0].text.chars().count() <= 2
}

/// [`record_bands`] の、字間を空けた欄見出しの末字（「称」）だけが並ぶ段。
fn label_tail_band(b: &[Col]) -> bool {
    b.len() <= 3 && b.iter().all(|c| c.text.chars().count() == 1)
}

/// [`record_bands`] の高い段（長い団体名・所在地の欄）が、低い段と同じ行の頭で組まれた表の欄か。本文の段も同じ行の
/// 送りで列が揃うので、次のどれも満たすときだけ表の欄とする。
/// - 句点（「。」）が無い（欄見出しのある摘要の欄の段は除く）。表の右の文（告示の本文・日付）は見ない。
/// - 隣の低い段（欄見出しを除いて 2 列以上）の列の 8 割以上が、この段の同じ x の列と組になる。
/// - 2 つ以上の段に欄見出し（[`RECORD_HEADS`] で終わる列）があるか、どの段の列も同じ x に 1 対 1 で並び（ページの
///   下半分に続く見出しの無い表）、組になった列の半分未満しか段の下端まで届かない（本文の列はどれも段の下端まで
///   組まれる）。
fn tall_record_band(bands: &[Vec<Col>], t: usize, tall_bands: &[usize]) -> bool {
    let center = |c: &Col| (c.x + c.x1) / 2.0;
    let columns = |b: &[Col]| -> Vec<(f32, String, f32)> {
        let mut cs: Vec<&Col> = b.iter().collect();
        cs.sort_by(|a, b| center(b).total_cmp(&center(a)).then(a.y.total_cmp(&b.y)));
        let mut out: Vec<(f32, String, f32)> = Vec::new();
        for c in cs {
            match out.last_mut() {
                Some((x, text, y1)) if (*x - center(c)).abs() < 3.0 => {
                    text.push_str(&c.text);
                    *y1 = y1.max(c.y1);
                }
                _ => out.push((center(c), c.text.clone(), c.y1)),
            }
        }
        out
    };
    // 隣の低い段（欄見出しの末字だけの段は飛ばす）。
    let short = |k: &usize| !tall_bands.contains(k) && bands[*k].len() >= 3 && !label_tail_band(&bands[*k]);
    let Some(k) = (t + 1..bands.len()).find(short).or_else(|| (0..t).rev().find(short)) else {
        return false;
    };
    let em = bands[t].iter().map(|c| c.x1 - c.x).fold(0.0, f32::max).max(6.0);
    // 欄見出しの列（「代表者」「の氏名」は字間を空けた「政治団体の名称」と同じ x に並ばない）は組にしない。
    let mut other = columns(&bands[k]);
    // 表の右の、表の前の文（告示の本文・日付・署名）は見ない。
    let edge = other.iter().map(|c| c.0).fold(f32::NEG_INFINITY, f32::max) + em * 1.5;
    let heads: Vec<f32> = other.iter().filter(|(_, text, _)| RECORD_HEADS.iter().any(|h| text.ends_with(h))).map(|c| c.0).collect();
    other.retain(|(x, ..)| !heads.iter().any(|h| x - h > -1.0 && x - h < em * 1.5));
    let band: Vec<Col> = bands[t].iter().filter(|c| center(c) <= edge).cloned().collect();
    let own = columns(&band);
    // 句点のある段は、欄見出しのある摘要の欄（「訓練施設として共同使用する。」）だけ。
    let labeled_here = own.iter().any(|(_, text, _)| RECORD_HEADS.iter().any(|h| text.ends_with(h)));
    if band.iter().any(|c| c.text.contains('。')) && !labeled_here {
        return false;
    }
    let pairs: Vec<&(f32, String, f32)> = own.iter().filter(|(x, ..)| other.iter().any(|(o, ..)| (o - x).abs() < 3.0)).collect();
    if other.len() < 2 || pairs.len() * 10 < other.len() * 8 {
        return false;
    }
    let labeled = bands.iter().filter(|b| columns(b).iter().any(|(_, text, _)| RECORD_HEADS.iter().any(|h| text.ends_with(h)))).count();
    let sets: Vec<Vec<(f32, String, f32)>> = bands.iter().filter(|b| b.len() >= 2).map(|b| columns(b)).collect();
    let bottom = band.iter().map(|c| c.y1).fold(f32::NEG_INFINITY, f32::max);
    let same = sets.len() >= 3
        && sets[0].len() >= 4
        && sets.iter().all(|s| s.len() == sets[0].len() && s.iter().zip(&sets[0]).all(|(a, b)| (a.0 - b.0).abs() < 3.0))
        && pairs.iter().filter(|(_, _, y1)| bottom - y1 < em).count() * 2 < pairs.len();
    labeled >= 2 || same
}

/// 氏と名の間の空きで段に切られた名簿（叙位・叙勲の「窪田」の段と「信正」の段）。2 つの低い段（30pt 以下、
/// 4 字以下の語）の列がどれも同じ x で 1 対 1 に組になれば、列ごとに「窪田　信正」とつなぐ。
fn name_pair_bands(bands: &[Vec<Col>]) -> Option<(usize, Vec<Line>)> {
    let center = |c: &Col| (c.x + c.x1) / 2.0;
    let short = |b: &[Col]| {
        let (y0, y1) = b.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(a, z), c| (a.min(c.y), z.max(c.y1)));
        y1 - y0 <= 30.0 && b.iter().all(|c| c.text.chars().count() <= 4)
    };
    let (a, b) = (&bands[0], &bands[1]);
    if a.len() < 4 || a.len() != b.len() || !short(a) || !short(b) {
        return None;
    }
    let mut a: Vec<&Col> = a.iter().collect();
    let mut b: Vec<&Col> = b.iter().collect();
    a.sort_by(|p, q| center(q).total_cmp(&center(p)));
    b.sort_by(|p, q| center(q).total_cmp(&center(p)));
    if a.iter().zip(&b).any(|(p, q)| (center(p) - center(q)).abs() > 2.0) {
        return None;
    }
    let lines = a.iter().zip(&b).map(|(p, q)| Line::text(format!("{}\u{3000}{}", p.text, q.text), vec![p.id, q.id])).collect();
    Some((2, lines))
}

/// 段の境目にかかる字間を空けた区分見出し（「法　規　的」と「告　示」が別の段に割れた「法規的告示」）を、
/// 頭の字のある段にまとめる。
fn rejoin_split_labels(tiers: &mut [Vec<Col>]) {
    let center = |c: &Col| (c.x + c.x1) / 2.0;
    for i in 0..tiers.len() {
        for j in 0..tiers.len() {
            if i == j {
                continue;
            }
            // tiers[i] の列の続きが tiers[j] の同じ列の下にあり、つなぐと区分見出しになるか。
            let xs: Vec<f32> = tiers[i].iter().map(center).collect();
            for x in xs {
                let mut head: Vec<&Col> = tiers[i].iter().filter(|c| (center(c) - x).abs() < 4.0).collect();
                let mut tail: Vec<&Col> = tiers[j].iter().filter(|c| (center(c) - x).abs() < 4.0).collect();
                if head.is_empty() || tail.is_empty() {
                    continue;
                }
                head.sort_by(|a, b| a.y.total_cmp(&b.y));
                tail.sort_by(|a, b| a.y.total_cmp(&b.y));
                let (head_bottom, tail_top) = (head.last().unwrap().y1, tail[0].y);
                let em = head[0].x1 - head[0].x;
                if tail_top < head_bottom || tail_top - head_bottom > em * 3.0 {
                    continue;
                }
                let text: String = head.iter().chain(tail.iter()).map(|c| c.text.as_str()).collect();
                if !crate::segment::is_section_label(&text) {
                    continue;
                }
                let ids: Vec<usize> = tail.iter().map(|c| c.id).collect();
                let moved: Vec<Col> = tiers[j].extract_if(.., |c| ids.contains(&c.id)).collect();
                tiers[i].extend(moved);
            }
        }
    }
}

/// 目次のページ番号を、その項目の段に戻す。
///
/// 目次では、項目の最後の行と同じ列の段の下端に、ページ番号（「二」「一〇」）を置く。段の境目の
/// 下に並ぶので段分割で別の段になり、どの項目の番号か分からなくなっていた。漢数字だけの語が
/// その列の最後の語で、すぐ上の段の同じ列の下に続くなら、その段の列の後ろに全角空白で区切って
/// つなぐ（初めから同じ段にある番号も区切る）。番号は目次の数字の字形（[`is_toc_page_number`]）のものだけで、同じ段に 2 つ以上
/// あるときだけ扱う（表の欄の数や、本文の段の頭の項目番号「二」と取り違えないため）。
fn attach_toc_page_numbers(mut tiers: Vec<Vec<Col>>, words: &[Word]) -> Vec<Vec<Col>> {
    let is_number = |c: &Col| words.get(c.id).is_some_and(|w| is_toc_page_number(&w.raw, &w.text));
    let center = |c: &Col| (c.x + c.x1) / 2.0;
    let mut moves: Vec<(usize, usize, usize)> = Vec::new();
    for (ti, tier) in tiers.iter().enumerate() {
        // 番号だけの段（目次の欄の下端にそろえて組んだページ番号「九」「三七」「四四」）は、項目の列の下端から離れていても
        // （12 字分まで）同じ列の項目へ戻す。
        let only_numbers = tier.len() >= 2 && tier.iter().all(|c| is_number(c));
        let candidates: Vec<(usize, usize)> = tier
            .iter()
            .enumerate()
            .filter(|&(_, c)| is_number(c))
            // その列の最後の語（下に続く語が無い）。
            .filter(|&(_, c)| !tier.iter().any(|d| d.id != c.id && (center(d) - center(c)).abs() < (c.x1 - c.x) * 0.5 && d.y > c.y))
            .filter_map(|(ci, c)| {
                let em = c.x1 - c.x;
                // すぐ上にある段で、同じ列の語の下端が番号の上にあるもの。
                let reach = if only_numbers { em * 12.0 } else { em * 2.0 };
                let ui = (0..tiers.len())
                    .filter(|&ui| {
                        let up = &tiers[ui];
                        let bottom = up.iter().map(|d| d.y1).fold(f32::NEG_INFINITY, f32::max);
                        ui != ti
                            && (0.0..reach).contains(&(c.y - bottom))
                            && up.iter().any(|d| (center(d) - center(c)).abs() < em * 0.5 && d.y1 <= c.y)
                    })
                    .min_by(|&a, &b| {
                        let bottom = |ui: usize| tiers[ui].iter().map(|d| d.y1).fold(f32::NEG_INFINITY, f32::max);
                        (c.y - bottom(a)).total_cmp(&(c.y - bottom(b)))
                    })?;
                Some((ci, ui))
            })
            .collect();
        if candidates.len() >= 2 {
            moves.extend(candidates.into_iter().map(|(ci, ui)| (ti, ci, ui)));
        }
    }
    // 後ろの添字から取り出す。
    moves.sort_by_key(|&(ti, ci, _)| std::cmp::Reverse((ti, ci)));
    for (ti, ci, ui) in moves {
        let mut c = tiers[ti].remove(ci);
        c.text = format!("　{}", c.text);
        tiers[ui].push(c);
    }
    // 初めから項目と同じ段にある番号も、同じ列の項目の後ろなら全角空白で区切る。
    for tier in &mut tiers {
        let follows: Vec<usize> = (0..tier.len())
            .filter(|&i| {
                let c = &tier[i];
                !c.text.starts_with('　')
                    && is_number(c)
                    && tier.iter().any(|d| d.id != c.id && (center(d) - center(c)).abs() < (c.x1 - c.x) * 0.5 && d.y1 <= c.y)
            })
            .collect();
        for i in follows {
            tier[i].text = format!("　{}", tier[i].text);
        }
    }
    tiers.retain(|t| !t.is_empty());
    tiers
}

/// 新旧欄の対応する格子表が段切りで割れた場合、閉じた表と同じ前文列の短い署名だけを元の段へ戻す。
fn rejoin_split_shinkyu_grid_tiers(tiers: &mut Vec<Vec<Col>>, grids: &[Grid]) {
    let mut i = 1;
    while i < tiers.len() {
        let Some((header_x, divider)) = detect_shinkyu_header(&tiers[i - 1]) else {
            i += 1;
            continue;
        };
        if detect_shinkyu_header(&tiers[i]).is_some() {
            i += 1;
            continue;
        }
        let Some(frame) = grids
            .iter()
            .filter(|g| g.left() <= header_x && header_x <= g.right() + 1. && g.top() < divider && divider < g.bottom())
            .min_by(|a, b| ((a.right() - a.left()) * (a.bottom() - a.top())).total_cmp(&((b.right() - b.left()) * (b.bottom() - b.top()))))
        else {
            i += 1;
            continue;
        };
        let mid = frame
            .ys
            .iter()
            .copied()
            .min_by(|a, b| (a - divider).abs().total_cmp(&(b - divider).abs()))
            .unwrap_or(divider);
        let pitch = (frame.bottom() - frame.top()) / 2.;
        let inner = |g: &Grid| {
            g.cells.len() >= 4
                && g.xs.len() >= 3
                && g.ys.len() >= 3
                && g.left() >= frame.left() - 0.5
                && g.right() < header_x - 1.
                && g.top() > frame.top() + 1.
                && g.bottom() < frame.bottom() - 1.
                && (g.bottom() < mid - 1. || g.top() > mid + 1.)
        };
        let matched = grids.iter().filter(|g| inner(g)).any(|g| {
            let twin = grids.iter().any(|other| {
                inner(other)
                    && (other.top() < mid) != (g.top() < mid)
                    && other.xs.len() == g.xs.len()
                    && other.ys.len() == g.ys.len()
                    && other.xs.iter().zip(&g.xs).all(|(a, b)| (a - b).abs() < 1.)
                    && ((other.top() - g.top()).abs() - pitch).abs() < 2.
                    && ((other.bottom() - other.top()) - (g.bottom() - g.top())).abs() < 1.
            });
            if !twin {
                return false;
            }
            let inside = |c: &Col| g.contains((c.x + c.x1) / 2., (c.y + c.y1) / 2., 0.5);
            if tiers[i - 1].iter().filter(|c| inside(c)).count() < 3 || tiers[i].iter().filter(|c| inside(c)).count() < 3 {
                return false;
            }
            // 二段の合計で表全体がそろうときだけ。さらに別の段に残る格子の語は巻き込まない。
            if tiers.iter().enumerate().any(|(j, t)| j != i - 1 && j != i && t.iter().any(inside)) {
                return false;
            }
            tiers[i].iter().all(|c| {
                if inside(c) {
                    return true;
                }
                let em = (c.x1 - c.x).max(1.);
                c.x > frame.right() + em * 0.25
                    && c.y1 - c.y >= em * 0.9
                    && c.text.chars().count() <= 12
                    && !c.text.contains(['。', '、'])
                    && !crate::segment::is_heading_mark(c.text.trim())
                    && tiers[i - 1].iter().any(|p| {
                        p.x > frame.right() && ((p.x + p.x1) - (c.x + c.x1)).abs() < em && p.y1 < c.y - em * 3. && p.y1 - p.y >= (p.x1 - p.x) * 0.9
                    })
            })
        });
        if matched {
            let tail = tiers.remove(i);
            tiers[i - 1].extend(tail);
        } else {
            i += 1;
        }
    }
}

#[cfg(test)]
#[path = "paired_shinkyu_grid_tests.rs"]
mod paired_shinkyu_grid_tests;

/// 改正後帯と改正前帯が別々の段に割れた新旧対照表を 1 段に戻す。
///
/// 欄見出しが字間を大きく空けて組まれ（「改　　正　　後」）、前文が 1 列だけ両帯を跨ぐ
/// ページでは、段分割が帯の境目で割ってしまい、どちらの段にも見出しが半分しか無くなる。
/// 隣り合う 2 段がそれぞれ見出しを持たず、合わせると見出しが揃うなら 1 段にまとめる。
fn merge_split_header_tiers(tiers: Vec<Vec<Col>>) -> Vec<Vec<Col>> {
    let mut out: Vec<Vec<Col>> = Vec::new();
    for tier in tiers {
        if let Some(prev) = out.last_mut() {
            if detect_shinkyu_header(prev).is_none() && detect_shinkyu_header(&tier).is_none() {
                let merged: Vec<Col> = prev.iter().chain(tier.iter()).cloned().collect();
                if detect_shinkyu_header(&merged).is_some() {
                    *prev = merged;
                    continue;
                }
            }
        }
        out.push(tier);
    }
    // 帯の中の表が欄ごとの段に割れ、見出しの字（「改」「正」「後」…「前」）が 4 つ以上の段に散ったときは、
    // 見出しの頭の「改」のある段から、合わせて見出しが揃うまでの段（12 段まで）をまとめる。
    let mut merged_out: Vec<Vec<Col>> = Vec::new();
    let mut i = 0;
    while i < out.len() {
        let has_head = out[i].iter().any(|c| c.text.starts_with('改') && c.text.chars().count() <= 3);
        if has_head && detect_shinkyu_header(&out[i]).is_none() {
            let end = (i + 3..out.len().min(i + 12)).find(|&j| {
                out[i..=j].iter().skip(1).all(|t| detect_shinkyu_header(t).is_none())
                    && detect_shinkyu_header(&out[i..=j].concat()).is_some()
            });
            if let Some(j) = end {
                merged_out.push(out[i..=j].concat());
                i = j + 1;
                continue;
            }
        }
        merged_out.push(std::mem::take(&mut out[i]));
        i += 1;
    }
    merged_out
}

/// 段の境目 `cut` をまたぐ列がすべて片側に寄っていれば、その脇で縦に切って
/// `(右側, 左側, またぐ列が右側にあるか)` を返す。またがない側が `cut` で段に割れる（またぐ列が 1 本以下で、
/// 境目の上下に段間の空きがあり、上下とも複数列ある）ときだけ切る。
///
/// 段の内部（1 段分の高さ）では切らない。短い列の下端を段の境目と取り違えて段を
/// 細切れにしないため。新旧対照表の見出しを含む領域も帯の扱いを崩さないよう切らない。
fn split_beside_crossing(cols: &[Col], cut: f32, spaced: &[(f32, f32, f32)], hrules: &[(f32, f32, f32)]) -> Option<(Vec<Col>, Vec<Col>, bool)> {
    /// 複数の段にまたがる領域とみなす高さ(pt)。官報の 1 段は約 176pt。
    const MIN_REGION_HEIGHT: f32 = 300.0;
    /// 段の境目の空き(pt)。
    const MIN_TIER_GAP: f32 = 4.0;
    let ymin = cols.iter().map(|c| c.y).reduce(f32::min)?;
    let ymax = cols.iter().map(|c| c.y1).reduce(f32::max)?;
    // 欄見出しの半分（同じ x に「改」「正」の 1 字の語が上下に並ぶ）を含む領域も切らない。
    // 帯の境目で上下に割れた新旧対照表の半分を細切れにすると、段を戻せなくなる。
    let has_label = cols.iter().filter(|c| c.text == "改").any(|k| {
        cols.iter().any(|c| c.text == "正" && (c.x - k.x).abs() < 2.0 && c.y > k.y && c.y - k.y < 120.0)
    });
    if ymax - ymin < MIN_REGION_HEIGHT || has_label || detect_shinkyu_header(cols).is_some() {
        return None;
    }
    let crosses = |c: &Col| c.y < cut && cut < c.y1;
    let center = |c: &Col| (c.x + c.x1) / 2.0;
    let cx_min = cols.iter().filter(|c| crosses(c)).map(|c| c.x).reduce(f32::min)?;
    let cx_max = cols.iter().filter(|c| crosses(c)).map(|c| c.x1).reduce(f32::max)?;
    let splits_at_cut = |side: &[Col]| {
        let (above, below): (Vec<&Col>, Vec<&Col>) =
            side.iter().filter(|c| !crosses(c)).partition(|c| (c.y + c.y1) / 2.0 < cut);
        let gap = match (above.iter().map(|c| c.y1).reduce(f32::max), below.iter().map(|c| c.y).reduce(f32::min)) {
            (Some(a), Some(b)) => b - a,
            _ => return false,
        };
        let xs = side.iter().map(|c| c.x).reduce(f32::min).unwrap_or(0.0);
        let xe = side.iter().map(|c| c.x1).reduce(f32::max).unwrap_or(0.0);
        // 欄の見出しの字と字の間で切るなら、段の境目ではなく表の中。
        let in_label = spaced.iter().any(|&(px, y1, y)| xs <= px && px <= xe && y1 < cut && cut < y);
        side.iter().filter(|c| crosses(c)).count() <= MAX_BRIDGE
            && above.len() >= 2
            && below.len() >= 2
            // 段の境目の横線（その側の幅いっぱいの 1 本）の上下にちょうど 4pt 空いた段は、浮動小数の誤差で
            // 4pt をわずかに下回っても段に分ける（表の行の罫線は同じ幅の線が何本もある）。
            && gap >= MIN_TIER_GAP - if ruled_alone(hrules, side, cut) { 0.01 } else { 0.0 }
            && !in_label
    };
    // またぐ列が右に寄っている: 左側を段に割れるか。
    let (right, left): (Vec<Col>, Vec<Col>) = cols.iter().cloned().partition(|c| center(c) >= cx_min - 1.0);
    if splits_at_cut(&left) && left.iter().all(|c| c.x1 < cx_min + 1.0) {
        return Some((right, left, true));
    }
    // またぐ列が左に寄っている: 右側を段に割れるか。またぐ列のすぐ右で、またぐ列の上端の高さから
    // 始まる記事の見出し（「〇国土交通省告示第千二百三号」の後に段をまたぐ前文が続く）は、またぐ側に付ける。
    let cross_top = cols.iter().filter(|c| crosses(c)).map(|c| c.y).fold(f32::INFINITY, f32::min);
    let heads: Vec<&Col> = cols
        .iter()
        .filter(|c| {
            let em = c.x1 - c.x;
            !crosses(c) && c.text.starts_with('〇') && c.x > cx_max - 1.0 && c.x - cx_max < em * 3.0 && (c.y - cross_top).abs() < em * 2.0
        })
        .collect();
    // 見出しの列の語（「〇」の後に割注「財務省・農林水産省」と「令第四号」が続く）はまとめて付ける。
    let heading = |c: &Col| !crosses(c) && heads.iter().any(|h| (center(h) - center(c)).abs() < (h.x1 - h.x) * 0.5);
    let (right, left): (Vec<Col>, Vec<Col>) = cols.iter().cloned().partition(|c| center(c) > cx_max + 1.0 && !heading(c));
    if splits_at_cut(&right) && right.iter().all(|c| c.x > cx_max - 1.0) {
        return Some((right, left, false));
    }
    None
}

/// `cut` の所に `side` の幅いっぱいの横線が 1 本だけあるか（段の境目の線）。同じ幅の横線が `side` の
/// 高さの中にほかにもあれば、表の行の罫線。
fn ruled_alone(hrules: &[(f32, f32, f32)], side: &[Col], cut: f32) -> bool {
    let (x0, x1) = (side.iter().map(|c| c.x).fold(f32::INFINITY, f32::min), side.iter().map(|c| c.x1).fold(f32::NEG_INFINITY, f32::max));
    let (y0, y1) = (side.iter().map(|c| c.y).fold(f32::INFINITY, f32::min), side.iter().map(|c| c.y1).fold(f32::NEG_INFINITY, f32::max));
    let spans = |rx0: f32, rx1: f32| rx1.min(x1) - rx0.max(x0) >= (x1 - x0) * 0.9;
    hrules.iter().any(|&(y, rx0, rx1)| (y - cut).abs() < 12.0 && spans(rx0, rx1))
        && hrules.iter().filter(|&&(y, rx0, rx1)| y0 + 4.0 < y && y < y1 - 4.0 && spans(rx0, rx1)).count() == 1
}

/// 1 字ずつの語が同じ列に同じ間隔で 3 つ以上並ぶ（欄の高さに合わせて字間を空けた見出し
/// 「区　間　変　更　前」）ときの、字と字の間 `(列の中央, 上の字の下端, 下の字の上端)`。
fn spaced_label_gaps(cols: &[Col]) -> Vec<(f32, f32, f32)> {
    /// 表の欄の見出しの字の大きさの上限(pt)。これより大きい字は記事の種類の見出し
    /// （「そ　の　他　告　示」）で、段をまたいで組まれるので段の境目を妨げない。
    const MAX_LABEL_SIZE: f32 = 11.0;
    let center = |c: &Col| (c.x + c.x1) / 2.0;
    let mut singles: Vec<&Col> = cols
        .iter()
        .filter(|c| c.text.chars().count() == 1 && c.x1 - c.x < MAX_LABEL_SIZE && c.y1 - c.y >= (c.x1 - c.x) * 0.9)
        .collect();
    singles.sort_by(|a, b| center(a).total_cmp(&center(b)).then(a.y.total_cmp(&b.y)));
    let mut out = Vec::new();
    let mut i = 0;
    while i < singles.len() {
        let w = singles[i].x1 - singles[i].x;
        let mut j = i + 1;
        while j < singles.len() && (center(singles[j]) - center(singles[i])).abs() < w * 0.5 {
            j += 1;
        }
        let column = &singles[i..j];
        let mut k = 0;
        while k + 2 < column.len() {
            let gap = |n: usize| column[n + 1].y - column[n].y1;
            let g0 = gap(k);
            if g0 <= 0.0 || g0 > w * 2.5 {
                k += 1;
                continue;
            }
            let mut end = k + 1;
            while end < column.len() - 1 && (gap(end) - g0).abs() < w * 0.15 {
                end += 1;
            }
            if end - k >= 2 {
                out.extend((k..end).map(|n| (center(column[n]), column[n].y1, column[n + 1].y)));
            }
            k = end;
        }
        i = j;
    }
    out
}

/// 跨ぎカラム数の許容上限（前文 1 本程度の橋渡しを段境界として許す）。
const MAX_BRIDGE: usize = 1;
/// 官報の本文の 1 段の高さ(pt)。
const TIER_HEIGHT: f32 = 176.0;
/// 同じ列で続く語の空きがこれ未満なら同じ文の続き（字間を空けた見出し・欧文の語間。半角
/// 0.5em まで）とみなし、そこを段の境目にしない(em 比)。
const TOUCH_EM: f32 = 0.6;
/// 同じ列で続く語の空きがこれ未満なら、全角空白 1 字ほどの区切り（同じセル・同じ行の中）とみなす(em 比)。
const SPACE_EM: f32 = 1.25;

/// `ruled_only` なら、段の幅の大半にわたる横線のある所でだけ切る（表の脇で縦に切った残り。
/// 罫線の無い表を段の境目と取り違えて割らないため）。
fn split_tiers_rec(cols: Vec<Col>, depth: usize, rules: &TierRules, ruled_only: bool, out: &mut Vec<Vec<Col>>) {
    let tables = &rules.tables;
    // 過分割の防止: 段が小さい / 深すぎる場合はこれ以上分けない。
    if cols.len() < 4 || depth >= 6 {
        out.push(cols);
        return;
    }
    // 1 つの列に字間を空けて組んだ区分見出し（「そ　の　他　告　示」）だけの領域は分けない。
    {
        let mut sorted: Vec<&Col> = cols.iter().collect();
        sorted.sort_by(|a, b| a.y.total_cmp(&b.y));
        let x = (sorted[0].x + sorted[0].x1) / 2.0;
        let one_column = sorted.iter().all(|c| ((c.x + c.x1) / 2.0 - x).abs() < 4.0);
        let text: String = sorted.iter().map(|c| c.text.as_str()).collect();
        if one_column && crate::segment::is_section_label(&text) {
            out.push(cols);
            return;
        }
    }
    let ymin = cols.iter().map(|c| c.y).fold(f32::INFINITY, f32::min);
    let ymax = cols.iter().map(|c| c.y1).fold(f32::NEG_INFINITY, f32::max);
    // 頁の片側だけに区分見出しへ続く横線がある場合、先に左右へ分ける。
    // 反対側の長い本文や表が横線をまたぐため、列の下端だけでは段の境目を選べない。
    // 分割後も横線のある所だけで切り、叙勲の氏名の列や表のセルを細分化しない。
    if let Some(edge) = section_rule_edge(&cols, rules, ymin, ymax) {
        let (right, left): (Vec<Col>, Vec<Col>) = cols.into_iter().partition(|c| (c.x + c.x1) / 2.0 >= edge);
        split_tiers_rec(right, depth + 1, rules, true, out);
        split_tiers_rec(left, depth + 1, rules, true, out);
        return;
    }
    let margin = (ymax - ymin) * 0.2;
    let (lo, hi) = (ymin + margin, ymax - margin);

    // 中央域で「跨ぐカラム数」が最小の cut を探す。候補はカラムの下端(y1)。この段の語を含む
    // 罫線の表を横切る cut は、表を横切らない cut が無いときだけ使う（セルの間の空きが段の
    // 境目に見えて、表が上下に割れていた）。
    let center = |c: &Col| (c.x + c.x1) / 2.0;
    let upright = |c: &Col| c.y1 - c.y >= (c.x1 - c.x) * 0.9;
    let crosses = |cut: f32, &(x0, x1, y0, y1): &(f32, f32, f32, f32)| {
        y0 + 1.0 < cut && cut < y1 - 1.0 && cols.iter().any(|c| x0 < center(c) && center(c) < x1 && y0 < c.y1 && c.y < y1)
    };
    // 囲みの外の縦書きの列（2 字以上）が cut の上（3 字分以内）で終わり、その列の下から cut までに語が無いなら、縦書きの
    // 段の境目（囲みの横に組んだ合併公告の上下の段）なので、囲みがあっても切る。
    let in_box = |c: &Col| rules.boxes.iter().any(|&(x0, x1, y0, y1)| x0 < center(c) && center(c) < x1 && y0 < c.y1 && c.y < y1);
    let column_break = |cut: f32| {
        cols.iter().any(|c| {
            c.text.chars().count() >= 2
                && c.y1 - c.y > (c.x1 - c.x) * 1.5
                && c.y1 <= cut
                && cut - c.y1 < (c.x1 - c.x) * 3.0
                && !in_box(c)
                && !cols.iter().any(|d| d.x < center(c) && center(c) < d.x1 && d.y1 > c.y1 + 0.5 && d.y < cut)
        })
    };
    let table_at = |cut: f32| tables.iter().find(|t| crosses(cut, t)).or_else(|| rules.boxes.iter().find(|b| crosses(cut, b) && !column_break(cut)));
    let (xmin, xmax) = (cols.iter().map(|c| c.x).fold(f32::INFINITY, f32::min), cols.iter().map(|c| c.x1).fold(f32::NEG_INFINITY, f32::max));
    let ruled = |cut: f32| {
        rules.hrules.iter().any(|&(y, x0, x1)| (y - cut).abs() < 12.0 && x1.min(xmax) - x0.max(xmin) >= (xmax - xmin) * 0.6)
    };
    // 領域の中の縦の罫線が 2 本以上、cut の上下に 3 字分以上延びていれば、表の中で段の境目ではない（新旧対照表の改正前の欄の
    // 表の「市町村」「地域」の欄の間の空きで切られ、表の下の欄見出しの「域」と署名の「簗　和生」が別の段になっていた）。
    // 横の罫線（左右の表で 2 本に分かれた線も合わせて）が領域の幅の 6 割以上に引かれているか。
    let ruled_across = |cut: f32| {
        let covered: f32 = rules.hrules.iter().filter(|&&(y, _, _)| (y - cut).abs() < 24.0).map(|&(_, x0, x1)| (x1.min(xmax) - x0.max(xmin)).max(0.0)).sum();
        covered >= (xmax - xmin) * 0.6
    };
    let ruled_through = |cut: f32| {
        let em = cols.iter().map(|c| c.x1 - c.x).fold(f32::INFINITY, f32::min).max(4.0);
        rules.vrules.iter().filter(|&&(x, y0, y1)| xmin < x && x < xmax && y0 < cut - em * 3.0 && y1 > cut + em * 3.0).count() >= 2
    };
    // 同じ列で上下に続く縦組みの語の組（上の語の下端, 下の語の上端, 空き(em)）。字の大きさの
    // 違う語や横組みの語（見出し「（号外）」の下の発行者の欄など）は、同じ文の続きではない。
    let mut by_center: Vec<&Col> = cols.iter().filter(|c| upright(c)).collect();
    by_center.sort_by(|a, b| center(a).total_cmp(&center(b)));
    let pairs: Vec<(f32, f32, f32)> = by_center
        .iter()
        .filter_map(|&a| {
            let w = a.x1 - a.x;
            let from = by_center.partition_point(|b| center(b) <= center(a) - w * 0.5);
            let to = by_center.partition_point(|b| center(b) < center(a) + w * 0.5);
            by_center[from..to]
                .iter()
                .filter(|b| b.y >= a.y1 - 0.5 && (center(a) - center(b)).abs() < w.min(b.x1 - b.x) * 0.5)
                .filter(|b| ((b.x1 - b.x) - w).abs() <= w.max(b.x1 - b.x) * 0.05)
                .min_by(|p, q| p.y.total_cmp(&q.y))
                .map(|b| (a.y1, b.y, (b.y - a.y1) / w.min(b.x1 - b.x).max(1.0)))
        })
        .collect();
    // 横組みの欄で上下に続く行の組も加える（「（号　外）」と「発行　内閣府」の間で段に切ると、
    // 欄が 1 行ずつに割れて横組みの欄として読めない）。横組みの語と、そのすぐ上か下の縦組みでない語
    // （字間を空けて 1 字ずつの語になった「発　行　内　閣　府」、字の大きさの違う行）の組。
    let wide = |c: &Col| c.text.chars().count() >= 2 && c.x1 - c.x > (c.y1 - c.y) * 1.5;
    let flat: Vec<&Col> = cols.iter().filter(|c| !(c.text.chars().count() >= 2 && c.y1 - c.y > (c.x1 - c.x) * 1.5)).collect();
    let below = |a: &Col, b: &Col| b.y >= a.y1 - 0.5 && b.x < a.x1 && a.x < b.x1;
    let pairs: Vec<(f32, f32, f32)> = pairs
        .into_iter()
        .chain(flat.iter().filter_map(|&a| {
            flat.iter()
                .filter(|&&b| below(a, b) && (wide(a) || wide(b)))
                .min_by(|p, q| p.y.total_cmp(&q.y))
                .map(|b| (a.y1, b.y, (b.y - a.y1) / (a.y1 - a.y).min(b.y1 - b.y).max(1.0)))
        }))
        .collect();
    // cut が同じ列の語と語の間を、字間ほどの空き（[`TOUCH_EM`] 未満）で切るか。字間を空けた
    // 欄見出し（「名　称」）や欧文の語間を段の境目と取り違えないため。
    // 署名の欄（「内閣総理大臣」の下に字 1 つ分空けて「高市　早苗」）の官職と氏名の間は、3 列以上並ぶなら段の境目の候補に
    // しない（官職の列と氏名の列が別の段に分かれ、官職の一覧の後に氏名の一覧が読まれていた）。
    let signers: Vec<(f32, f32)> = by_center
        .iter()
        .filter(|a| a.text.ends_with("大臣") || a.text.ends_with("長官") || a.text.ends_with('長'))
        .filter_map(|&a| {
            let w = a.x1 - a.x;
            let next = |p: &Col| {
                by_center
                    .iter()
                    .filter(|b| b.y >= p.y1 - 0.5 && (center(a) - center(b)).abs() < w * 0.5 && b.y - p.y1 < w * 1.6)
                    .min_by(|p, q| p.y.total_cmp(&q.y))
                    .copied()
            };
            // 官職の下の氏名の語（「井林」「辰憲」）の終わりまで。
            let first = next(a)?;
            let mut last = first;
            for _ in 0..2 {
                match next(last) {
                    Some(b) if b.text.chars().count() <= 4 => last = b,
                    _ => break,
                }
            }
            Some((a.y1, last.y))
        })
        .collect();
    // 語の境目ちょうど（空きの無い 2 語「令第四条第一項第四号」「に規定する…」の間）の cut も字間を切る。
    let touching = |cut: f32| pairs.iter().any(|&(y1, y, gap)| y1 - 0.5 < cut && cut < y + 1.0 && gap < TOUCH_EM);
    let signature = |cut: f32| signers.iter().filter(|&&(y1, y)| y1 < cut && cut < y).count() >= 3;
    // 候補の順位。跨ぐカラム数の少ないものを採る。同数のときは、跨ぐ列の無い cut なら字間を
    // 切らないものの中で上のもの、跨ぐ列のある cut なら右の列の下端（語の正規の順で先に来る列）を
    // 採る（語の入力順に左右されないように）。
    // （跨ぐカラム数, 字間を切るか, cut, 候補を作った列の x, y）。
    // 短い記事の列のうち 1〜2 本だけが長く続き、cut の下がその続きだけなら、段の境目ではない（「様式第六中「…」を
    // 「…」に」の 1 行目の続きが、隣の短い列の下の空きで切られて次の段になっていた）。cut の上の列の多く（3 列以上のうち
    // 続く列以外）は cut より 2 字分以上手前で終わる（段の境目なら、上の段の列はどれも段の下端まで組まれる）。
    let lone_column = |cut: f32| {
        let em = cols.iter().map(|c| c.x1 - c.x).fold(f32::INFINITY, f32::min).max(4.0);
        let below: Vec<&Col> = cols.iter().filter(|c| c.y >= cut).collect();
        let mut xs: Vec<f32> = cols.iter().filter(|c| c.y1 <= cut).map(|c| center(c)).collect();
        xs.sort_by(f32::total_cmp);
        xs.dedup_by(|a, b| (*a - *b).abs() < em * 0.75);
        let reaching = |x: f32| cols.iter().any(|a| a.y1 <= cut + 1.0 && a.y1 > cut - em * 2.0 && (center(a) - x).abs() < em * 0.75);
        let mut bx: Vec<f32> = below.iter().map(|c| center(c)).collect();
        bx.sort_by(f32::total_cmp);
        bx.dedup_by(|a, b| (*a - *b).abs() < em * 0.75);
        let reach_n = xs.iter().filter(|&&x| reaching(x)).count();
        !below.is_empty() && bx.len() <= 2 && xs.len() >= 3 && reach_n <= 2 && bx.iter().all(|&x| reaching(x))
    };
    let name_foot = |cut: f32| {
        let em = cols.iter().map(|c| c.x1 - c.x).fold(f32::INFINITY, f32::min).max(4.0);
        let below: Vec<&Col> = cols.iter().filter(|c| c.y >= cut && c.y - cut < em * 3.0).collect();
        let named = below
            .iter()
            .filter(|c| {
                let n = c.text.trim().chars().count();
                // 同じ列で cut より上の一番下の語。
                let above_of = |y: f32| cols.iter().filter(|a| a.y1 <= y + 0.5 && (center(a) - center(c)).abs() < em * 0.5).max_by(|p, q| p.y1.total_cmp(&q.y1));
                let above = above_of(cut);
                // 氏名は漢字だけの 2〜5 字。
                let kanji = c.text.trim().chars().all(|ch| ('\u{4E00}'..='\u{9FFF}').contains(&ch) || ch == '々');
                // 上の語は官職（「…判事」「…事務局長」「…審議官」、折り返しの「記官」）。
                let post = |t: &str| {
                    let t = t.trim();
                    ["官", "長", "事", "員", "監", "補", "佐"].iter().any(|e| t.ends_with(e)) || ["裁判所", "省", "庁", "局"].iter().any(|k| t.contains(k))
                };
                let is_kanji = |t: &str| t.trim().chars().all(|ch| ('\u{4E00}'..='\u{9FFF}').contains(&ch) || ch == '々');
                // 上の語が官職か、官職の下の姓（「家令」の下の「和典」）。
                let titled = |a: &Col| {
                    (a.text.chars().count() >= 2 && post(&a.text))
                        || (is_kanji(&a.text) && a.text.trim().chars().count() <= 4 && above_of(a.y).is_some_and(|b| post(&b.text)))
                };
                (2..=5).contains(&n) && kanji && above.is_some_and(titled)
            })
            .count();
        // 新旧対照表の帯の境目（欄見出しの「改」「正」「後」「前」）では見ない。
        let band = cols.iter().any(|c| matches!(c.text.trim(), "改" | "正" | "後" | "前" | "改正後" | "改正前"));
        !band && named >= 5 && named * 2 >= below.len()
    };
    type Cand = (usize, bool, f32, f32, f32);
    let better = |a: &Cand, b: &Option<Cand>| {
        b.is_none_or(|b| {
            let tie = if a.0 == 0 {
                a.1.cmp(&b.1).then(a.2.total_cmp(&b.2))
            } else {
                b.3.total_cmp(&a.3).then(a.4.total_cmp(&b.4)).then(a.2.total_cmp(&b.2))
            };
            a.0.cmp(&b.0).then(tie).is_lt()
        })
    };
    // best_free は表を横切らないものの中の最良。
    let mut best: Option<Cand> = None;
    let mut best_free: Option<Cand> = None;
    // 候補は列の下端と、この段の語を含む罫線の表の下端（表の字は下の枠の線より上で終わるので、表のすぐ下の段の境目が
    // 列の下端の候補に無く、表の中でしか切れなかった）。
    // 表の下端は、表を横切らない良い候補が列の下端に無いときだけ見る。
    let table_bottoms: Vec<(f32, f32, f32)> = tables
        .iter()
        .filter(|&&(x0, x1, y0, y1)| cols.iter().any(|c| x0 < center(c) && center(c) < x1 && y0 < c.y1 && c.y < y1))
        .map(|&(x0, _, y0, y1)| (y1 + 0.5, x0, y0))
        .collect();
    // A sparse text layer can leave its last upper column outside the central
    // search window. An actual tier rule inside a large empty gap is still a
    // candidate; ordinary table row gaps do not qualify.
    let ruled_gaps = rules.hrules.iter().filter_map(|&(y, x0, x1)| {
        let above = cols.iter().filter(|c| c.y1 <= y).map(|c| c.y1).fold(f32::NEG_INFINITY, f32::max);
        let below = cols.iter().filter(|c| c.y >= y).map(|c| c.y).fold(f32::INFINITY, f32::min);
        (above.is_finite() && below.is_finite()
            && below - above >= TIER_HEIGHT * 0.5
            && !cols.iter().any(|c| c.y < y && y < c.y1)
            && x1.min(xmax) - x0.max(xmin) >= (xmax - xmin) * 0.9)
            .then_some((y - 0.5, xmin, above))
    });
    let column_bottoms: Vec<(f32, f32, f32)> = cols.iter().map(|c| (c.y1, c.x, c.y)).chain(ruled_gaps).collect();
    for pass in 0..2 {
        if pass == 1 && best_free.is_some_and(|b: Cand| b.0 <= MAX_BRIDGE) {
            break;
        }
        let candidates = if pass == 0 { &column_bottoms } else { &table_bottoms };
        for &(c_y1, c_x, c_y) in candidates {
            let cut = c_y1 + 0.5;
            if cut < lo || cut > hi || (ruled_only && !ruled(cut)) || label_strip(&cols, cut) || spaced_label_split(&cols, cut) || signature(cut) {
                continue;
            }
            let crossing = cols.iter().filter(|d| d.y < cut && cut < d.y1).count();
            // 紙面の高さほどの領域で、領域の上端から下端まで（2 字分以内）延びる 1 列（表の右の前文の列）だけがまたぎ、横の罫線の無い cut だけ（新旧対照表の
            // 改正後・改正前の帯の境目は、帯をまたぐ枠の縦の罫線を横切るが、帯の境の横の罫線がある）。
            // 罫線の表の縦の線が 4 本以上、cut の上下に 3 字分以上延び、領域の上端から下端まで通っていれば、表の中（新旧対照表の
            // 改正前の欄の表の「備考」の段の上の空き）で段の境目ではない。
            let em = cols.iter().map(|c| c.x1 - c.x).fold(f32::INFINITY, f32::min).max(4.0);
            // 線が領域の上端から下端まで（1 字分の余裕で）通る（領域がまるごと表の中にある）ときだけ。
            let table_lines = rules
                .vrules
                .iter()
                .filter(|&&(x, y0, y1)| xmin < x && x < xmax && y0 < cut - em * 3.0 && y1 > cut + em * 3.0 && y0 <= ymin + em && y1 >= ymax - em)
                .count();
            // 新旧対照表の帯の境目（cut のすぐ上下に欄見出しの「改」「正」「後」「前」の 1 字の語がある）は切る。
            let band_label = cols.iter().any(|c| matches!(c.text.trim(), "改" | "正" | "後" | "前") && (c.y1 - cut).abs().min((c.y - cut).abs()) <= em * 3.0);
            if crossing <= MAX_BRIDGE && table_lines >= 4 && !band_label {
                continue;
            }
            let through = crossing == 1
                && ymax - ymin >= 500.0
                && cols.iter().any(|d| d.y < cut && cut < d.y1 && d.y - ymin <= (d.x1 - d.x) * 2.0 && ymax - d.y1 <= (d.x1 - d.x) * 2.0);
            if through && ruled_through(cut) && !ruled_across(cut) {
                continue;
            }
            // 表の下端の候補は、どの列もまたがないときだけ（表の横の本文の列が表の下まで続くなら段の境目ではない）。
            if pass == 1 && crossing > 0 {
                continue;
            }
            // cut の下（上）の語がどれも cut をまたぐ列の続きなら、1 つの列の途中を切るだけで段の境目ではない（改正前の欄の
            // 長い 1 列「第四条令第四条第一項第四号／に規定する…」の下の部分だけが次の段になり、隣の〔号を加える。〕の後に
            // 読まれていた）。
            if crossing == 0 && lone_column(cut) {
                continue;
            }
            // cut のすぐ下（3 字分以内）の語の半分以上が、同じ列の上の官職の語（折り返しの「記官」も）の下に組んだ短い氏名
            // （「家令」「和典」。漢字だけの 2〜5 字が 5 つ以上）なら、名簿の氏名の手前で段の境目ではない（官職の一覧の後に、氏名が下の段の列と 1 行ずつつながっていた）。
            if crossing == 0 && name_foot(cut) {
                continue;
            }
            // 文の列（またぐ語も続きの語も 6 字以上）だけを見る（日付の列の下の署名、題字の「法」「律」は切る）。
            if crossing == 1 {
                let span = cols.iter().find(|d| d.y < cut && cut < d.y1).unwrap();
                // 括弧の字（「」を「」）は本文の字より少し右に置かれるので、字の幅の 4 分の 3 まで。
                let same = |c: &Col| (center(c) - center(span)).abs() < (span.x1 - span.x) * 0.75;
                let long = |c: &Col| c.text.chars().count() >= 6;
                let below: Vec<&Col> = cols.iter().filter(|c| c.y >= cut).collect();
                // 15 字以上の長い文の語に空けずに続くなら、続きが短い語（行末の「とす」）でもよい。
                let glued = below.iter().map(|c| c.y).fold(f32::INFINITY, f32::min) - span.y1 < 1.0;
                // 短い語（括弧の字「」を「」）でも、同じ列の上下の語に 1.25 字分以内で挟まれていれば列の途中。
                let w = span.x1 - span.x;
                let chained = cols.iter().any(|a| a.id != span.id && same(a) && a.y < span.y && a.y1 >= span.y - w * 1.25)
                    && below.iter().any(|c| c.y <= span.y1 + w * 1.25);
                if below.iter().all(|c| same(c)) && ((long(span) && (below.iter().any(|c| long(c)) || (span.text.chars().count() >= 15 && glued))) || (chained && below.iter().any(|c| long(c)))) {
                    continue;
                }
            }
            // 横倒しのページ全体では、上の語の下端と下の語の上端が 1pt 未満重なるだけで、その間ならどの列も跨がない
            // なら、そこで切る（表示の左の横組みの表と右の縦書きの告示が、回した座標でわずかに重なる）。
            let (cut, crossing) = if crossing > 0 {
                let snug = c_y1 - 0.75;
                let n = cols.iter().filter(|d| d.y < snug - 0.75 && snug + 0.75 < d.y1).count();
                if rules.sideways && depth == 0 && n == 0 && cols.iter().filter(|d| d.y < snug && snug < d.y1).all(|d| (d.y1 - snug).abs() < 1.0 || (snug - d.y).abs() < 1.0 || (d.y < snug - 0.75 && snug + 0.75 < d.y1)) { (snug, n) } else { (cut, crossing) }
            } else {
                (cut, crossing)
            };
            let cand = (crossing, touching(cut), cut, c_x, c_y);
            if better(&cand, &best) {
                best = Some(cand);
            }
            if table_at(cut).is_none() && better(&cand, &best_free) {
                best_free = Some(cand);
            }
        }
    }
    if best_free.is_some_and(|b| b.0 <= MAX_BRIDGE) {
        best = best_free;
    }
    // どの cut も 2 本以上の列が跨ぐとき、跨ぐ語が cut の上か下に 2pt ほどしか掛からない（段の境目の直前で始まる
    // 名簿の名の語）ものを数えなければ跨ぐ列が 1 本以下になる cut があれば、そこを段の境目とする（右に段をまたぐ
    // 名簿の表があり、左の本文の段の境目で名の語の頭がわずかに上の段に掛かる）。
    if best.is_some_and(|b| b.0 > MAX_BRIDGE) {
        let slight = cols
            .iter()
            .map(|c| c.y1 + 0.5)
            .filter(|&cut| lo <= cut && cut <= hi && !(ruled_only && !ruled(cut)) && table_at(cut).is_none() && !touching(cut) && !lone_column(cut))
            // 2 段以上の高さの領域で、上端から 1 段の高さの所（段の境目）。
            .filter(|&cut| ymax - ymin >= 300.0 && (cut - ymin - TIER_HEIGHT).abs() <= 16.0)
            // わずかに掛かる語は短い語（名簿の名）だけ。
            .filter(|&cut| {
                cols.iter()
                    .filter(|d| d.y < cut && cut < d.y1 && !(d.y < cut - 2.0 && cut + 2.0 < d.y1))
                    .all(|d| d.text.chars().count() <= 3)
            })
            .map(|cut| (cols.iter().filter(|d| d.y < cut - 2.0 && cut + 2.0 < d.y1).count(), cut))
            .filter(|&(n, _)| n <= MAX_BRIDGE)
            .min_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
        if let Some((_, cut)) = slight {
            let crossing = cols.iter().filter(|d| d.y < cut && cut < d.y1).count();
            best = Some((crossing, false, cut, 0.0, 0.0));
        }
    }

    // 表の脇で縦に切った残りに横線のある cut が無くても、列の重ならない縦の境目の左に記事の見出し（「〇…号」）
    // があり、左だけならどの列も跨がない cut で段に割れるなら、そこで縦に切る（横倒しのページで、横組みの
    // 表の左に縦書きの告示が 2 段に組まれたもの。表の右から読む）。
    if ruled_only && best.is_none() {
        if let Some((right, left)) = split_beside_tiered_notice(&cols) {
            split_tiers_rec(right, depth + 1, rules, ruled_only, out);
            split_tiers_rec(left, depth + 1, rules, false, out);
            return;
        }
    }
    let Some((crossing, touches, cut, _, _)) = best else {
        out.push(cols);
        return;
    };
    // 段より十分低い領域を字間ほどの空きでしか切れなければ段の境目ではない（横倒しの号外の 1 ページの
    // 題字の下の横組みの欄「（号外）」「発行　内閣府」の行の間）。
    if touches && ymax - ymin < 60.0 {
        out.push(cols);
        return;
    }
    // 語の少ない（8 語まで）低い領域（120pt 未満）で、cut の上で終わる列がどれも同じ列で 1.5em 以内に cut の下へ続くなら、
    // 列の中の空き（表の欄の「二十三」「外国紋章」を 1em 空けて組んだもの）で段の境目ではない。
    if ymax - ymin < 120.0 && cols.len() <= 8 {
        let ends: Vec<&Col> = cols.iter().filter(|c| c.y1 < cut && !cols.iter().any(|d| d.y >= c.y1 - 0.5 && d.y1 < cut && (center(d) - center(c)).abs() < (c.x1 - c.x) * 0.5)).collect();
        let continued = |c: &Col| pairs.iter().any(|&(y1, y, gap)| (y1 - c.y1).abs() < 0.5 && y > cut && gap <= 1.5);
        if !ends.is_empty() && ends.iter().all(|c| continued(c)) {
            out.push(cols);
            return;
        }
    }
    // 低い領域（120pt 未満）で、cut の下になる語（中ほどが cut より下）が 2 語までで、どれも上の語に同じ列で 1.5em 以内に
    // 続くなら、列の中の空き（改正前の欄の「二十一・二十二」の 1em 下の「（略）」）で段の境目ではない（「（略）」だけが
    // 次の段になり、帯の終わりに読まれていた）。
    if ymax - ymin < 120.0 {
        let below: Vec<&Col> = cols.iter().filter(|c| (c.y + c.y1) / 2.0 >= cut).collect();
        let continues = |c: &Col| {
            pairs.iter().any(|&(y1, y, gap)| (y - c.y).abs() < 0.5 && y1 < cut && gap <= 1.5)
                // A permitted bridging column is assigned above by its midpoint.
                // Its bottom may cross the candidate cut; the following word is
                // still its continuation when their original column and gap agree.
                || cols.iter().any(|a| {
                    let em = (a.x1 - a.x).min(c.x1 - c.x).max(1.0);
                    a.id != c.id
                        && upright(a) && upright(c)
                        && (a.y + a.y1) / 2.0 < cut && a.y1 >= cut
                        && (center(a) - center(c)).abs() < em * 0.5
                        && ((a.x1 - a.x) - (c.x1 - c.x)).abs() <= em * 0.05
                        && c.y >= a.y1 - 0.5 && c.y - a.y1 <= em * 1.5
                        && !rules.hrules.iter().any(|&(y,x0,x1)| {
                            a.y1 + 0.5 < y && y < c.y - 0.5
                                && x0 < center(c) && center(c) < x1
                        })
                })
        };
        // 上の欄が何列も cut のすぐ上まで組まれている（「神奈川県、新潟県、」… の下の欄「令和八年四／月二十日」）なら、
        // 帯の表の欄の境目。
        let em = cols.iter().map(|c| c.x1 - c.x).fold(f32::INFINITY, f32::min).max(4.0);
        let top = below.iter().map(|c| c.y).fold(f32::INFINITY, f32::min);
        let ends = cols.iter().filter(|c| (c.y + c.y1) / 2.0 < cut && c.y1 >= top - em * 1.5).count();
        if !below.is_empty() && below.len() <= 2 && below.iter().all(|c| continues(c)) && ends <= below.len() + 2 {
            out.push(cols);
            return;
        }
    }
    // それでも表を横切るなら、表を割らずに表の脇で縦に切る。表の左に語があれば表の左端で、
    // 無ければ表の右端で切り、右側から順に段へ分ける。表が段の幅いっぱいなら切らない。
    if let Some(&(x0, x1, _, _)) = table_at(cut) {
        for (edge, table_right) in [(x0 - 1.0, true), (x1 + 1.0, false)] {
            let (right, left): (Vec<Col>, Vec<Col>) = cols.iter().cloned().partition(|c| center(c) >= edge);
            if !right.is_empty() && !left.is_empty() {
                split_tiers_rec(right, depth + 1, rules, ruled_only || !table_right, out);
                split_tiers_rec(left, depth + 1, rules, ruled_only || table_right, out);
                return;
            }
        }
        out.push(cols);
        return;
    }
    if crossing > MAX_BRIDGE {
        // 段の境目をまたぐ列（表など）が左右どちらかに寄っていれば、先にそこで縦に切り、
        // 右側から順に段へ分ける。下半分の右に段をまたぐ表、左に段ごとの本文がある
        // ページで、本文の上下の段の列が 1 行に連結されるのを防ぐ。
        // またぐ列のある側（表など）は 1 段のまま置き、もう一方だけを段に分ける。
        let spaced = spaced_label_gaps(&cols);
        if let Some((right, left, crossing_right)) = split_beside_crossing(&cols, cut, &spaced, &rules.hrules) {
            // またがない側は cut で段に割れることを確かめてある。その側の中央域の外に cut が来ると
            // 分け直しでは切れない（短い下の帯（改正前欄の「〔項を加える。〕」だけの帯など）が上の帯の
            // 列に付く）ので、そのときは cut で上下に切ってから分ける。同じ列が全角 1 字ほどの空きで
            // 続く（表のセルの中の「八、　五、二〇」）なら段の境目ではないので切らない。
            let in_cell = pairs.iter().any(|&(y1, y, gap)| y1 < cut && cut < y && gap < SPACE_EM);
            let split_side = |side: Vec<Col>, out: &mut Vec<Vec<Col>>| {
                let top = side.iter().map(|c| c.y).fold(f32::INFINITY, f32::min);
                let bottom = side.iter().map(|c| c.y1).fold(f32::NEG_INFINITY, f32::max);
                let central = (top + (bottom - top) * 0.2..=bottom - (bottom - top) * 0.2).contains(&cut);
                if in_cell || central {
                    split_tiers_rec(side, depth + 1, rules, ruled_only, out);
                    return;
                }
                let (top, bottom): (Vec<Col>, Vec<Col>) = side.into_iter().partition(|c| (c.y + c.y1) / 2.0 < cut);
                for part in [top, bottom].into_iter().filter(|p| !p.is_empty()) {
                    split_tiers_rec(part, depth + 1, rules, ruled_only, out);
                }
            };
            if crossing_right {
                // またぐ列が段の中ほどにあり、その右にも段ごとの本文があれば（右の段の本文、またぐ
                // 列の記事、左の段の本文の順）、右の本文も段に分ける。
                // 本文より大きな字の語（区分見出しの囲みの「官庁報告」）があれば分けない（囲みが段の境目にかかる）。
                let mut ems: Vec<f32> = right.iter().map(|c| c.x1 - c.x).collect();
                ems.sort_by(f32::total_cmp);
                let body_em = ems.get(ems.len() / 2).copied().unwrap_or(0.0);
                // 上下とも 4 列以上の本文でなければ分けない（字間を空けた区分見出し「法　規　的　告　示」の 1 列、
                // 数列の表）。
                let columns = |side: &[Col], upper: bool| {
                    let mut xs: Vec<f32> = side.iter().filter(|c| (c.y1 <= cut) == upper).map(|c| (c.x + c.x1) / 2.0).collect();
                    xs.sort_by(f32::total_cmp);
                    xs.dedup_by(|a, b| (*a - *b).abs() < 4.0);
                    xs.len()
                };
                // 段の境目は、上の段の上端から 1 段の高さ（約 176pt）の所（字間・表のセルの間の空きではない）。
                let off_tier = |side: &[Col]| {
                    let top = side.iter().map(|c| c.y).fold(f32::INFINITY, f32::min);
                    (cut - top - TIER_HEIGHT).abs() > 16.0
                };
                // 段の本文なら、上の段の列の多くは段の下端まで、下の段の列の多くは段の上端から組まれている
                // （表のセルの列は短く、セルの間の空きで切ると列の多くが境目から離れている）。
                let filled = |side: &[Col]| {
                    let mut ends: Vec<(f32, f32, f32)> = Vec::new(); // (列の x, 上の段の下端, 下の段の上端)
                    for c in side {
                        let x = (c.x + c.x1) / 2.0;
                        let i = ends.iter().position(|e| (e.0 - x).abs() < 4.0).unwrap_or_else(|| {
                            ends.push((x, f32::NEG_INFINITY, f32::INFINITY));
                            ends.len() - 1
                        });
                        if c.y1 <= cut {
                            ends[i].1 = ends[i].1.max(c.y1);
                        } else {
                            ends[i].2 = ends[i].2.min(c.y);
                        }
                    }
                    let em = body_em.max(1.0);
                    let lower_top = ends.iter().map(|e| e.2).fold(f32::INFINITY, f32::min);
                    let above: Vec<f32> = ends.iter().map(|e| e.1).filter(|y| y.is_finite()).collect();
                    let below: Vec<f32> = ends.iter().map(|e| e.2).filter(|y| y.is_finite()).collect();
                    let full_above = above.iter().filter(|&&y| y >= cut - em * 3.0).count();
                    let full_below = below.iter().filter(|&&y| y <= lower_top + em * 3.0).count();
                    // 表の欄（名簿の名称の欄など）は 1 つの列に短い語が縦にいくつも並ぶ。本文の列は 1 段に数語。
                    let words_per_column = side.len() as f32 / ends.len().max(1) as f32;
                    full_above * 5 >= above.len() * 2 && full_below * 5 >= below.len() * 2 && words_per_column <= 4.0
                };
                // cut の所にその側の幅いっぱいの横線が 1 本だけあれば、段の境目（列の短い表のような段でも）。
                let ruled_side = |side: &[Col]| ruled_alone(&rules.hrules, side, cut);
                let large = |side: &[Col]| {
                    side.iter().any(|c| c.x1 - c.x > body_em * 1.3)
                        || columns(side, true) < 4
                        || columns(side, false) < 4
                        || (!ruled_side(side) && (off_tier(side) || !filled(side)))
                };
                match split_beside_crossing(&right, cut, &spaced, &rules.hrules) {
                    Some((beyond, crossing_cols, false)) if !large(&beyond) => {
                        split_side(beyond, out);
                        out.push(crossing_cols);
                    }
                    _ => out.push(right),
                }
                split_side(left, out);
            } else {
                split_side(right, out);
                out.push(left);
            }
        } else if let Some(edge) = partial_rule_edge(&cols, rules, lo, hi) {
            // 段の境目の横線が段の幅の一部（右か左に寄せて）にしか無ければ、その端で縦に
            // 切る（横線の側は段に分かれた記事、反対側は段を通して組まれた別の記事や表）。
            let (right, left): (Vec<Col>, Vec<Col>) = cols.into_iter().partition(|c| (c.x + c.x1) / 2.0 >= edge);
            split_tiers_rec(right, depth + 1, rules, ruled_only, out);
            split_tiers_rec(left, depth + 1, rules, ruled_only, out);
        } else {
            out.push(cols);
        }
        return;
    }
    // ページ全体（最上位）では、跨ぐ列の無い横の空白の帯はどれも段の境目なので、まとめて切る
    // （1 つずつ切ると、先に切った位置で残りの領域の中央域が変わり、どれを先に切るかで目次の
    // ページ番号の行などが前後の段に付いたり付かなかったりする）。字間を切るものは除く。
    if crossing == 0 && depth == 0 {
        let mut cuts: Vec<f32> = cols
            .iter()
            .map(|c| c.y1 + 0.5)
            .filter(|&cut| lo <= cut && cut <= hi && table_at(cut).is_none() && !touching(cut))
            .filter(|&cut| !cols.iter().any(|d| d.y < cut && cut < d.y1))
            // 欄見出しの帯の下（灯台の告示の「名称」「位置」…の帯と値の間）は段の境目ではない。
            .filter(|&cut| !label_strip(&cols, cut) && !spaced_label_split(&cols, cut))
            .collect();
        cuts.sort_by(f32::total_cmp);
        let part_of = |c: &Col, cuts: &[f32]| cuts.partition_point(|&cut| cut <= (c.y + c.y1) / 2.0);
        // 語の無い区間を作る cut（ほぼ同じ位置の cut）を除く。
        cuts = cuts
            .iter()
            .enumerate()
            .filter(|&(i, _)| cols.iter().any(|c| part_of(c, &cuts) == i))
            .map(|(_, &cut)| cut)
            .collect();
        // 1 語だけの区間が、同じ字の大きさの列の続き（セルの区切りほど空いていない）で隣の区間と
        // つながるなら、その cut では切らない（段の境目の直後の項目番号「二」が、続く「名古屋市…」
        // の列から離れないように。目次のページ番号の行は字の大きさが本文と違うので切る）。
        let continues = |cut: f32| pairs.iter().any(|&(y1, y, gap)| y1 < cut && cut < y && gap < CELL_BREAK_EM);
        loop {
            let sizes: Vec<usize> = (0..=cuts.len()).map(|i| cols.iter().filter(|c| part_of(c, &cuts) == i).count()).collect();
            let Some(i) = (0..cuts.len()).find(|&i| (sizes[i] < 2 || sizes[i + 1] < 2) && continues(cuts[i])) else {
                break;
            };
            cuts.remove(i);
        }
        if !cuts.is_empty() {
            let mut parts: Vec<Vec<Col>> = vec![Vec::new(); cuts.len() + 1];
            for c in cols {
                let k = part_of(&c, &cuts);
                parts[k].push(c);
            }
            for p in parts.into_iter().filter(|p| !p.is_empty()) {
                split_tiers_rec(p, depth + 1, rules, ruled_only, out);
            }
            return;
        }
    }
    // 1 段の高さほどの領域で、cut より下が短い語（5 字以下）だけで、その多くが上の同じ列の続き（人事異動の
    // 「（内閣委員会専門員）同」の列の下に寄せた氏名「田中　仁」）なら、段の境目ではないので切らない。
    if ymax - ymin <= TIER_HEIGHT + 20.0 && ymax - ymin >= TIER_HEIGHT * 0.8 {
        let lower: Vec<&Col> = cols.iter().filter(|c| (c.y + c.y1) / 2.0 > cut).collect();
        // 上の同じ列の最後の語が官職・肩書き（「…同」「…官」「…大臣」「…長」「…員」）で終わる（その後の氏名）。
        let continued = lower
            .iter()
            .filter(|c| {
                cols.iter()
                    .filter(|u| u.y1 <= cut && (center(u) - center(c)).abs() < 3.0)
                    .max_by(|a, b| a.y1.total_cmp(&b.y1))
                    .is_some_and(|u| u.text.ends_with(['同', '官', '臣', '長', '員', '事', '師', '授', '監']))
            })
            .count();
        // 上の部分にも列の跨がない 1 字以上の空き（欄の境目）があれば、欄ごとの段が並ぶ表なので除く。
        let upper: Vec<&Col> = cols.iter().filter(|c| (c.y + c.y1) / 2.0 <= cut).collect();
        let em = cols.iter().map(|c| c.x1 - c.x).fold(0.0, f32::max).max(6.0);
        let banded = upper.iter().any(|a| {
            let y = a.y1;
            let below = upper.iter().filter(|c| c.y >= y).map(|c| c.y).fold(f32::INFINITY, f32::min);
            below.is_finite() && below - y >= em && upper.iter().any(|c| c.y1 <= y) && !upper.iter().any(|c| c.y < y && c.y1 > y)
        });
        if !banded && lower.len() >= 3 && lower.iter().all(|c| c.text.chars().count() <= 5) && continued * 10 >= lower.len() * 8 {
            out.push(cols);
            return;
        }
    }
    // cut で上下に分割（橋渡しカラムは中心 y が属する側へ。ただし上の段の同じ列の語に続く
    // （「第三条」の後の「歯科医師法施行規則…の一部を次のように改正する。」）なら上の段へ）。
    let continues_above = |c: &Col| {
        let w = c.x1 - c.x;
        c.y < cut
            && cut < c.y1
            && cols.iter().any(|d| d.id != c.id && d.y1 <= c.y + 0.5 && c.y - d.y1 < w * SPACE_EM && (center(d) - center(c)).abs() < w * 0.5)
    };
    // 隣の列（3 列分以内、同じ字の大きさ。折り返しの 1〜2 字の続き「係」は除く）で cut より上で終わる語と同じ高さから始まり、cut より 3 字分以上上から始まる語（目次の「裁判所」の
    // 下の「相続、失踪、破産、特別清算、再生、」。隣の「所有者不明関係」と頭がそろう）も上の段へ。
    let starts_above = |c: &Col| {
        let w = c.x1 - c.x;
        c.y < cut
            && cut < c.y1
            && cut - c.y >= w * 3.0
            && cols.iter().any(|d| d.id != c.id && (d.y - c.y).abs() < 1.0 && d.y1 <= cut && (center(d) - center(c)).abs() < w * 3.0 && ((d.x1 - d.x) - w).abs() < 0.5 && d.text.chars().count() >= 3)
    };
    let upper: std::collections::HashSet<usize> = cols.iter().filter(|c| continues_above(c) || starts_above(c)).map(|c| c.id).collect();
    let (mut top, mut bottom): (Vec<Col>, Vec<Col>) = (Vec::new(), Vec::new());
    for c in cols {
        if (c.y + c.y1) / 2.0 < cut || upper.contains(&c.id) {
            top.push(c);
        } else {
            bottom.push(c);
        }
    }
    if top.is_empty() || bottom.is_empty() {
        out.push(top.into_iter().chain(bottom).collect());
        return;
    }
    split_tiers_rec(top, depth + 1, rules, ruled_only, out);
    split_tiers_rec(bottom, depth + 1, rules, ruled_only, out);
}

/// `cut` が字間を空けた 1 字ずつの欄見出し（「名　　称」「位　　置」）の字の間を切るか。3 列以上で、同じ列の 1 字の語が
/// `cut` の上（8em 以内）で終わり、下（8em 以内）から始まり、続けると欄見出しの頭になる。
fn spaced_label_split(cols: &[Col], cut: f32) -> bool {
    let center = |c: &Col| (c.x + c.x1) / 2.0;
    let single = |c: &&Col| c.text.chars().filter(|ch| !ch.is_whitespace()).count() == 1;
    // 上と下の字を続けると欄見出しの頭になるもの（「名」「称」、「所」「在」）だけ。
    const HEADS: [&str; 16] = ["名称", "位置", "所在地", "北緯", "東経", "灯質", "光度", "高さ", "記事", "種類", "番号", "住所", "氏名", "区分", "距離", "周期"];
    let split = cols
        .iter()
        .filter(single)
        .filter(|a| {
            let em = (a.x1 - a.x).max(1.0);
            a.y1 <= cut
                && cut - a.y1 < em * 8.0
                && cols.iter().filter(single).any(|b| {
                    let joined = format!("{}{}", a.text.trim(), b.text.trim());
                    (center(b) - center(a)).abs() < em * 0.5 && b.y >= cut && b.y - cut < em * 8.0 && HEADS.iter().any(|h| h.starts_with(&joined))
                })
        })
        .count();
    split >= 3
}

/// 列の重ならない縦の境目で `cols` を右と左に分ける。左に記事の見出し（「〇…号」）があり、左だけならどの列も
/// 跨がない（字間でもない）横の空きで上下に割れ、右は割れず見出しも無いときだけ。境目は見出しの記事に
/// いちばん近いもの（左から順に試して最初のもの）。
/// `cut` の上が縦組みの表の欄見出しの帯（航路標識の表の「灯質」「光度」「光達距離」…）で、各欄の値がそのすぐ下の同じ
/// 列に続くか。帯で段に切ると、欄見出しを全部読んでから値を全部読む（見出しと値が離れる）。
///
/// 帯: `cut` のすぐ上（8em 以内の高さ）に収まる 8 字以下の語の列が 8 本以上あり、半分以上が欄見出しの語（名称・位置・
/// 年月日…）で終わり、その 7 割以上の真下（2.5em 以内）に同じ列の値が 1 つ始まる。
fn label_strip(cols: &[Col], cut: f32) -> bool {
    let em = {
        let mut ws: Vec<f32> = cols.iter().map(|c| c.x1 - c.x).collect();
        ws.sort_by(f32::total_cmp);
        ws.get(ws.len() / 2).copied().unwrap_or(10.0).max(1.0)
    };
    let center = |c: &Col| (c.x + c.x1) / 2.0;
    // 帯の語は、cut の上の長い語（上の段の本文・値）の終わりより下で始まるもの（上の段の末尾の短い語で帯の上端が上がり、
    // 上の段の本文を帯の上の語とみなしていた）。
    let text_end = cols.iter().filter(|c| c.y1 <= cut && c.text.chars().count() > 8).map(|c| c.y1).fold(f32::NEG_INFINITY, f32::max);
    let labels: Vec<&Col> =
        cols.iter().filter(|c| c.y1 <= cut && cut - c.y1 < em * 8.0 && c.y >= text_end - 1.0 && c.text.chars().count() <= 8).collect();
    // 帯の上には何も無い（帯は段の頭。段の境の上の段の列の末尾の短い語を帯にしない）。帯は縦組み（横組みの語の
    // ある横組みの表の行の間では切ってよい）。
    // 上の段の語（帯の上端より上で終わる語。表の段が上下に並ぶページの 2 段目の帯）は見ない。
    let band_top = labels.iter().map(|c| c.y).fold(f32::INFINITY, f32::min);
    let above: Vec<&Col> = cols.iter().filter(|c| c.y1 <= cut && c.y1 > band_top + 1.0).collect();
    // 字間を大きく空けた欄見出し（「名　　　称」）や長い欄見出し（「変更した事項」）の帯は 16em まで高くてよい。
    let reach = |c: &Col| if c.text.chars().count() <= 8 { em * 16.0 } else { em * 8.0 };
    if above.iter().any(|c| cut - c.y > reach(c) || (c.text.chars().count() >= 2 && c.x1 - c.x > c.y1 - c.y)) {
        return false;
    }
    // 字間を空けた欄見出し（「高　さ」）は 1 字ずつの語なので、同じ列の語をまとめて列の下端を見る。
    let mut groups: Vec<(f32, f32, String)> = Vec::new();
    let mut sorted = labels.clone();
    sorted.sort_by(|a, b| center(a).total_cmp(&center(b)).then(a.y.total_cmp(&b.y)));
    for c in sorted {
        match groups.last_mut() {
            Some(g) if (center(c) - g.0).abs() < em * 0.5 => {
                g.1 = g.1.max(c.y1);
                g.2.push_str(&c.text);
            }
            _ => groups.push((center(c), c.y1, c.text.clone())),
        }
    }
    // 欄見出しは名詞（「灯質」「光達距離」）。記録の欄の末尾の日付（「八、」「（令和）」）や数は欄見出しではない。
    let label_like = |t: &str| !t.chars().any(|c| "、。，．（）()".contains(c)) && !t.chars().all(|c| "〇一二三四五六七八九十百千".contains(c) || c.is_ascii_digit());
    if groups.iter().filter(|g| label_like(&g.2)).count() * 10 < groups.len() * 7 {
        return false;
    }
    // 半分以上が欄見出しの語で終わる（記録の欄の値「大路渡」「吉野純子」の並びは欄見出しの帯ではない）。
    const HEADS: [&str; 24] = [
        "名称", "位置", "所在地", "年月日", "記事", "種類", "距離", "光度", "灯質", "明孤", "高さ", "北緯", "東経", "周波数", "レベル", "周期", "区域",
        "型式", "番号", "住所", "氏名", "時間", "電力", "区分",
    ];
    if groups.iter().filter(|g| HEADS.iter().any(|h| g.2.ends_with(h))).count() * 2 < groups.len() {
        return false;
    }
    if groups.len() < 8 {
        return false;
    }
    let below = groups
        .iter()
        .filter(|&&(x, y1, _)| cols.iter().any(|v| v.y >= cut && v.y - y1 < em * 2.5 && (center(v) - x).abs() < em * 0.5))
        .count();
    // 欄見出しの下はそれぞれ 1 つの値（2 つまでの語）。下に記録が何件も並ぶ表（政治団体の名称・代表者の氏名…の欄の
    // 下に団体ごとの行が続く）は、欄見出しの帯を段に分けて読む（[`record_bands`]）。
    // 値は帯のすぐ下の段の中（下の段の記録の値は数えない）。
    // 字の間を空けずに続く語（座標の「三四－〇二－」「五〇」…の 1 つの値が語に割れたもの）は 1 つの値と数える。
    let values = |x: f32| {
        let mut vs: Vec<&Col> = cols.iter().filter(|v| v.y >= cut && v.y - cut < em * 16.0 && (center(v) - x).abs() < em * 0.5).collect();
        vs.sort_by(|a, b| a.y.total_cmp(&b.y));
        vs.iter().enumerate().filter(|&(i, v)| i == 0 || v.y - vs[i - 1].y1 > em * 0.8).count()
    };
    let single = groups.iter().filter(|g| values(g.0) <= 2).count();
    below * 10 >= groups.len() * 7 && single * 10 >= groups.len() * 7
}

fn split_beside_tiered_notice(cols: &[Col]) -> Option<(Vec<Col>, Vec<Col>)> {
    let mut edges: Vec<f32> = cols.iter().map(|c| c.x).collect();
    edges.sort_by(f32::total_cmp);
    edges.dedup_by(|a, b| (*a - *b).abs() < 0.5);
    // 左の部分が、どの列も跨がない横の空き（上下とも 2 列以上、空きは 4pt 以上）で割れるか。
    let tiered = |side: &[Col]| {
        let (top, bottom) = (side.iter().map(|c| c.y).fold(f32::INFINITY, f32::min), side.iter().map(|c| c.y1).fold(f32::NEG_INFINITY, f32::max));
        let (lo, hi) = (top + (bottom - top) * 0.2, bottom - (bottom - top) * 0.2);
        side.iter().map(|c| c.y1).filter(|&y| lo <= y && y <= hi).any(|y| {
            let next = side.iter().filter(|c| c.y >= y).map(|c| c.y).fold(f32::INFINITY, f32::min);
            next - y >= 4.0
                && !side.iter().any(|c| c.y < y + 0.5 && c.y1 > y + 0.5)
                && side.iter().filter(|c| c.y1 <= y + 0.5).count() >= 2
                && side.iter().filter(|c| c.y >= next - 0.5).count() >= 2
        })
    };
    for edge in edges {
        if cols.iter().any(|c| c.x < edge - 0.5 && c.x1 > edge + 0.5) {
            continue;
        }
        let (right, left): (Vec<Col>, Vec<Col>) = cols.iter().cloned().partition(|c| c.x >= edge - 0.5);
        if right.len() < 4 || left.len() < 8 {
            continue;
        }
        let heading = |side: &[Col]| side.iter().any(|c| crate::segment::is_heading_mark(&c.text));
        if heading(&left) && !heading(&right) && tiered(&left) && !tiered(&right) {
            return Some((right, left));
        }
    }
    None
}

/// `part` の語の右から 3 列までに記事の始まり（「〇…第○号」の標題・「…をここに公布する。」）があるか。
fn starts_article(cols: &[Col], part: &dyn Fn(&Col) -> bool) -> bool {
    column_groups(cols.iter().filter(|c| part(c)).cloned().collect()).iter().take(3).any(|g| {
        let text: String = g.iter().map(|c| c.text.as_str()).collect();
        crate::segment::is_heading_mark(&text) || text.contains("をここに公布する")
    })
}

/// 部分横線の端が縦組みの字の中ほどにある場合、1 字以内の最寄りの縦溝へ寄せる。
/// 語を二分せず、領域全体の語の箱の間にある 1pt 以上の空きだけを使う。
fn section_rule_gutter(cols: &[Col], edge: f32) -> Option<f32> {
    if !cols.iter().any(|c| c.x < edge - 2.0 && edge + 2.0 < c.x1) {
        return Some(edge);
    }
    let mut widths: Vec<f32> = cols
        .iter()
        .filter(|c| c.y1 - c.y > (c.x1 - c.x) * 1.5)
        .map(|c| c.x1 - c.x)
        .filter(|&w| (4.0..=20.0).contains(&w))
        .collect();
    widths.sort_by(f32::total_cmp);
    let em = *widths.get(widths.len() / 2)?;
    let reach = em * 0.75;
    let mut spans: Vec<(f32, f32)> = cols.iter().map(|c| (c.x, c.x1)).collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut union: Vec<(f32, f32)> = Vec::new();
    for (x0, x1) in spans {
        match union.last_mut() {
            Some(last) if x0 <= last.1 => last.1 = last.1.max(x1),
            _ => union.push((x0, x1)),
        }
    }
    union
        .windows(2)
        .filter(|p| p[1].0 - p[0].1 >= 1.0)
        .map(|p| (p[0].1 + p[1].0) / 2.0)
        .filter(|&x| (x - edge).abs() <= reach)
        .min_by(|a, b| (a - edge).abs().total_cmp(&(b - edge).abs()))
}

/// 片側の上下段の間に横線があり、一方の段に区分見出しがある領域の左右の境目。
fn section_rule_edge(cols: &[Col], rules: &TierRules, ymin: f32, ymax: f32) -> Option<f32> {
    if !cols.iter().any(|c| crate::segment::is_section_label(&c.text)) {
        return None;
    }
    let xmin = cols.iter().map(|c| c.x).fold(f32::INFINITY, f32::min);
    let xmax = cols.iter().map(|c| c.x1).fold(f32::NEG_INFINITY, f32::max);
    let width = xmax - xmin;
    // まず頁幅の横線で上下を分ける。先に左右を分けると上段の読み順が変わる。
    if rules
        .hrules
        .iter()
        .any(|&(y, x0, x1)| ymin < y && y < ymax && x1.min(xmax) - x0.max(xmin) > width * 0.8)
    {
        return None;
    }
    rules.hrules.iter().find_map(|&(y, x0, x1)| {
        if y <= ymin + (ymax - ymin) * 0.2
            || y >= ymax - (ymax - ymin) * 0.2
            || x1 - x0 < width * 0.15
        {
            return None;
        }
        let edge = if x1 >= xmax - 5.0 && x0 > xmin + 20.0 {
            x0
        } else if x0 <= xmin + 5.0 && x1 < xmax - 20.0 {
            x1
        } else {
            return None;
        };
        let edge = section_rule_gutter(cols, edge)?;
        let (sx0, sx1) = if x0 <= xmin + 5.0 {
            (x0, edge)
        } else {
            (edge, x1)
        };
        let inside = |c: &&Col| sx0 - 1.0 <= c.x && c.x1 <= sx1 + 1.0;
        let side: Vec<&Col> = cols.iter().filter(inside).collect();
        // 横線が本文や表のセルを横切らず、その上か下の近くに独立した区分見出しがあることを確認。
        // 官庁報告の見出しから始まる上の段の下に、同じ欄幅で別の記事が続くこともある。
        let below = x1 - x0 >= width * 0.25
            && side
                .iter()
                .any(|c| crate::segment::is_section_label(&c.text) && c.y > y && c.y - y < 120.0);
        let above = side
            .iter()
            .any(|c| crate::segment::is_section_label(&c.text) && c.y1 < y && y - c.y1 < 120.0);
        ((below || above)
            && side.iter().any(|c| c.y1 < y)
            && side.iter().any(|c| c.y > y)
            && !side.iter().any(|c| c.y < y && y < c.y1)
            && !cols.iter().any(|c| c.x < edge - 2.0 && edge + 2.0 < c.x1))
        .then_some(edge)
    })
}

/// 段の幅の 4 割以上にわたり、右端か左端に寄せて引かれた横線（`lo..hi` の高さ）の、寄せて
/// いない側の端の x。その外が別の記事の始まりのときだけ。
fn partial_rule_edge(cols: &[Col], rules: &TierRules, lo: f32, hi: f32) -> Option<f32> {
    let xmin = cols.iter().map(|c| c.x).fold(f32::INFINITY, f32::min);
    let xmax = cols.iter().map(|c| c.x1).fold(f32::NEG_INFINITY, f32::max);
    let em = cols.iter().map(|c| c.x1 - c.x).fold(f32::INFINITY, f32::min).max(4.0);
    let center = |c: &Col| (c.x + c.x1) / 2.0;
    // 新旧対照表の改正後帯と改正前帯の境目の横線（欄見出しのある表の幅の線）は段の境目ではない。
    let dividers = crate::shinkyu::detect_shinkyu_headers(cols);
    rules
        .hrules
        .iter()
        .filter(|&&(y, x0, x1)| lo < y && y < hi && x1.min(xmax) - x0.max(xmin) >= (xmax - xmin) * 0.4)
        .filter(|&&(y, x0, x1)| !dividers.iter().any(|&(hx, divider)| (divider - y).abs() < 15.0 && x0 - 5.0 <= hx && hx <= x1 + 5.0))
        // 横線の側に新旧対照表の欄見出しがあり、外にも欄見出しがある（別々の記事の表が並ぶ）か、横線の側の
        // 表の欄見出しが横線の上か下の一方に収まる（表は横線の段にだけあり、横線は表の上か下の段の境目。
        // 号外第 178 号 28 ページ: 右半分の下の段に新しい省令の表、上の段に前の記事の附則、左半分は段を
        // 通して組んだ次の記事）ときだけ。前の表の末尾の帯の境目の線（欄見出しが無い）などで切らないため。
        .filter(|&&(y, x0, x1)| {
            let inside = |hx: f32| x0 - 5.0 <= hx && hx <= x1 + 5.0;
            let one_side = |hx: f32| {
                let label: Vec<&Col> = cols
                    .iter()
                    .filter(|c| (c.x - hx).abs() < 6.0 && c.text.chars().count() <= 3 && c.text.chars().all(|ch| matches!(ch, '改' | '正' | '後' | '前')))
                    .collect();
                !label.is_empty() && (label.iter().all(|c| c.y > y) || label.iter().all(|c| c.y1 < y))
            };
            dividers.iter().any(|&(hx, _)| inside(hx))
                && (dividers.iter().any(|&(hx, _)| !inside(hx)) || dividers.iter().filter(|&&(hx, _)| inside(hx)).all(|&(hx, _)| one_side(hx)))
        })
        .filter_map(|&(_, x0, x1)| {
            if x1 >= xmax - em * 2.0 && x0 > xmin + em * 2.0 && starts_article(cols, &|c: &Col| center(c) < x0 - 2.0) {
                Some((x1 - x0, x0))
            } else if x0 <= xmin + em * 2.0 && x1 < xmax - em * 2.0 && starts_article(cols, &|c: &Col| center(c) > x1 + 2.0) {
                Some((x1 - x0, x1))
            } else {
                None
            }
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, edge)| edge)
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
    let render = |cells: Vec<Cell>| {
        Line::text(
            cells.iter().map(|c| c.text.as_str()).collect::<Vec<_>>().join("\u{3000}"),
            cells.into_iter().flat_map(|c| c.words).collect(),
        )
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
            if !(top < MARGIN_NOISE_TOP && is_margin_noise(&line.text)) {
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

/// 縦に置いた欧文（学名など）の語間か。前の語が半角の欧字・数字・記号で終わり、次の語が
/// 半角の欧字・数字で始まり、少しでも空いていれば半角空白を入れる。
fn latin_space(prev: &str, prev_end: f32, next: &Col) -> bool {
    let end = prev.chars().last().is_some_and(|c| c.is_ascii_graphic());
    let start = next.text.chars().next().is_some_and(|c| c.is_ascii_alphanumeric());
    end && start && next.y - prev_end > 0.5
}

/// 1 列（y 昇順の語）を、空きの大きさに応じて空白挿入・セル分割する。
fn split_cells(g: Vec<Col>, em: f32) -> Vec<Cell> {
    let mut cells: Vec<Cell> = Vec::new();
    for c in g {
        match cells.last_mut() {
            Some(cell) if c.y - cell.y1 < em * CELL_BREAK_EM => {
                if latin_space(&cell.text, cell.y1, &c) {
                    cell.text.push(' ');
                } else if c.y - cell.y1 >= em * GAP_EM {
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
pub(crate) fn starts_item(text: &str) -> bool {
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
/// x 中心の差が 0.75 文字分以内なら同じ列とみなす（本文の列間隔は約 1.5 文字分だが、
/// 表の中は行間なしで 1 文字分おきに列が並ぶことがある）。半角幅の記号（`'` 等）は
/// 列の右半分に置かれ xMin が半字ずれるため、左端ではなく中心で比べる。縦中横の数字
/// （幅が広い）は列の文字の大きさで比べる。
pub(crate) fn column_groups(mut cols: Vec<Col>) -> Vec<Vec<Col>> {
    let center = |c: &Col| (c.x + c.x1) / 2.0;
    cols.sort_by(|a, b| center(b).total_cmp(&center(a)).then_with(|| a.y.total_cmp(&b.y)).then_with(|| a.text.cmp(&b.text)));
    let mut groups: Vec<Vec<Col>> = Vec::new();
    let (mut rep_x, mut rep_em) = (f32::INFINITY, f32::INFINITY);
    // 上下に 1.5 字分以上重なる語は、隣の記事や表の欄なので同じ列にしない。
    // 欧文の語（横倒しに組んだ URL）と上下に 1 字分以上重なる語も、同じ列ではない（URL の 2 行目「p4rtbmpp/」が、少し
    // 左にずれた次の列の日付「令和八年十月十六日」と同じ列になり、「令和八年十月十六日p4rtbmpp/」になっていた）。
    let latin = |c: &Col| {
        let n = c.text.chars().count();
        // 縦中横の数（「684」）は除く。英字だけの語は 3 字から（「TECH」）。
        (n >= 5 && c.text.chars().filter(|ch| ch.is_ascii_graphic()).count() * 10 >= n * 8) || (n >= 3 && c.text.chars().all(|ch| ch.is_ascii_alphabetic()))
    };
    // 和字に英字の混じった語（「株式会社Diversity」）も、字の箱が右にずれて左隣の列（「代表取締役」）と同じ列になる
    // （「株式会社Diversity代表取締役山畔Study」）ため、列を分けるときは英字が 3 字以上あれば欧文の語とみる。
    let lettered = |c: &Col| latin(c) || c.text.chars().filter(|ch| ch.is_ascii_alphabetic()).count() >= 3;
    for c in cols {
        let em = (c.x1 - c.x).min(rep_em).max(6.0);
        let stacked = groups.last().is_some_and(|g| {
            g.iter().any(|o| {
                let overlap = o.y1.min(c.y1) - o.y.max(c.y);
                // 半字ずれは割注・目次の引用の小さな列。同じ大きさの字が 0.6 字より離れているときだけ、
                // 重なりから隣の列と判断する（異なる字号の「改正前欄」と行番号は従来どおり組む）。
                let same_size = ((o.x1 - o.x) - (c.x1 - c.x)).abs() < em * 0.1;
                (overlap > em * 1.5 && (center(o) - center(&c)).abs() > em * 0.6 && same_size)
                    || ((lettered(o) || lettered(&c)) && overlap > em)
            })
        });
        if rep_x - center(&c) > em * 0.75 || stacked {
            groups.push(Vec::new());
            rep_x = center(&c);
            rep_em = c.x1 - c.x;
        }
        groups.last_mut().unwrap().push(c);
    }
    // 区分見出し（「その他告示」の囲み。本文より大きな字）は、同じ x の本文の列とつなげず 1 つの列にする
    // （本文の列の折り返しの「号」とつながって「第百三十八その他告示号」になっていた）。
    // 目次の「〔その他告示〕」のように本文と同じ大きさの字なら、そのまま列に組む。
    let mut labels = Vec::new();
    for g in &mut groups {
        let body_em = g.iter().filter(|c| !crate::segment::is_section_label(&c.text)).map(|c| c.x1 - c.x).fold(f32::INFINITY, f32::min);
        if body_em.is_finite() {
            labels.extend(g.extract_if(.., |c| crate::segment::is_section_label(&c.text) && c.x1 - c.x > body_em * 1.3 && c.y1 - c.y > (c.x1 - c.x) * 1.5));
        }
        g.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));
    }
    for l in labels {
        let at = groups.iter().position(|g| g.iter().map(center).fold(f32::NEG_INFINITY, f32::max) < center(&l)).unwrap_or(groups.len());
        groups.insert(at, vec![l]);
    }
    groups.retain(|g| !g.is_empty());
    // 横倒しの URL の折り返し（「…/corporate/」の続きの「notice/」）が、次の列（次の「（乙）」の列）の、前の列の URL と同じ高さから
    // 組まれていれば、前の列の URL の後ろへ移す（「（乙）notice/」になっていた）。前の列の URL が「/」「.」「-」「_」で終わるときだけ。
    for gi in 1..groups.len() {
        let (head, tail) = groups.split_at_mut(gi);
        let prev = &mut head[gi - 1];
        let cur = &mut tail[0];
        // URL の下線が combining low line で符号化されたものも区切りとみなす。一般の英字名は広げない。
        let path_end = |p: &Col| p.text.trim_end().ends_with(['/', '.', '-', '_'])
            || (p.text.trim_end().ends_with('\u{0332}') && p.text.contains(['/', '.', '@']));
        let Some(p) = prev.last().filter(|p| latin(p) && path_end(p)).cloned() else { continue };
        // 次の URL の頭（「http」「www.」）は続きではない。
        let fresh = |c: &Col| c.text.trim_start().starts_with("http") || c.text.trim_start().starts_with("www.");
        if let Some(k) = cur.iter().position(|c| latin(c) && !fresh(c) && (c.y - p.y).abs() < 1.0) {
            if k > 0 {
                let c = cur.remove(k);
                prev.push(c);
            }
        }
    }
    groups
}

/// 1ページ分（または1段分）のカラム語を右→左・上→下に並べて本文に組む。
pub(crate) fn reconstruct_page(cols: Vec<Col>) -> String {
    join_lines(&page_lines(cols))
}

/// カラム語を右→左・上→下に並べ、縦の列ごとの行にする。柱（発行日・号数）の列は捨てる。
///
/// 縦書きの段の中に横組みで置いた欄（号外の目次ページの「官報」の題字の下の「（号　外）」
/// 「発行　内閣府」「（原稿作成　国立印刷局）」など）は、縦の列として読むと語が右から並んで
/// 「外）」「（号」と逆になる。そうした横組みの語は横の行に組み、その欄の右端の位置
/// （右→左の読み順で欄に差し掛かったところ）に置く。
pub(crate) fn page_lines(cols: Vec<Col>) -> Vec<Line> {
    page_lines_with(cols, &[], &[])
}

/// [`page_lines`] の、縦の罫線 `vrules`（`(x, y0, y1)`）・横の罫線 `hrules`（`(y, x0, x1)`）で横組みの囲みの中の
/// 縦書きの見出し（貸借対照表の「負債及び純資産の部」）を見分ける版。
fn page_lines_with(cols: Vec<Col>, vrules: &[(f32, f32, f32)], hrules: &[(f32, f32, f32)]) -> Vec<Line> {
    let (cols, blocks) = split_off_horizontal_blocks(cols, vrules, hrules);
    let mut blocks = blocks.into_iter().peekable();
    let mut lines: Vec<Line> = Vec::new();
    let groups = column_groups(cols);
    let continued = record_continuations(&groups);
    for (gi, g) in groups.iter().enumerate() {
        let x = g.iter().map(|c| (c.x + c.x1) / 2.0).fold(f32::NEG_INFINITY, f32::max);
        while let Some((_, block)) = blocks.next_if(|(right, _)| *right > x) {
            lines.extend(block);
        }
        // 罫線の無い縦組みの名簿で、欄の字を次の列へ折り返した行（「中道改革連／合衆議院比／例区南関東…」）は、続きの列の
        // 字を欄ごとにつないで 1 行にする。
        if let Some(conts) = continued.get(&gi) {
            if conts.is_empty() {
                continue;
            }
            let heads = record_cells(g);
            let mut cells: Vec<String> = heads.iter().map(|(_, _, ws)| ws.iter().map(|c| c.text.as_str()).collect()).collect();
            let mut ids: Vec<usize> = g.iter().map(|c| c.id).collect();
            for &k in conts {
                for c in &groups[k] {
                    let at = heads.iter().rposition(|h| h.0 <= c.y + 1.0).unwrap_or(0);
                    cells[at].push_str(&c.text);
                    ids.push(c.id);
                }
            }
            lines.push(Line::text(cells.join("\u{3000}"), ids));
            continue;
        }
        let g = g.clone();
        let mut text = String::new();
        let mut prev_end = f32::MIN;
        let mut prev_chars = 0;
        for c in &g {
            let em = (c.x1 - c.x).max(1.0);
            let chars = c.text.chars().filter(|ch| !ch.is_whitespace()).count();
            // 字間を空けた 1 字ずつの見出し（「府　令」「目　次」）は空けない。
            let spaced_heading = chars == 1 && prev_chars == 1;
            // 列の語と語の間に横の罫線があれば別のセル（縦組みの表の幅員「二五・〇〇〜二七・一三」と延長「〇・〇三四」が
            // 続けて組まれていた）。
            // 括弧の字の前後（罫線で描いた「　」の線）は空けない。
            let ruled = !text.is_empty()
                && !text.ends_with(['「', '（', '『', '〔'])
                && !c.text.starts_with(['」', '）', '』', '〕'])
                && hrules.iter().any(|&(y, x0, x1)| y > prev_end - 0.5 && y < c.y + 0.5 && x0 <= c.x + 1.0 && x1 >= c.x1 - 1.0);
            // 数と数が 0.75 字分以上空いて続くなら別の値（縦組みの表の幅員「…二七・一三」と延長「〇・〇三四」）。
            let numeral = |ch: char| ch.is_ascii_digit() || "〇一二三四五六七八九十・．".contains(ch);
            // 小数どうし（「…二七・一三」と「〇・〇三四」）か、列の頭の項目番号（「一」「二十六」の後の「一般職の…」）だけ。
            // 桁区切りの「、」の字形の無い金額（「六七」「四一五」）や、字間を空けた年月日（「八・」「四・二三」）は分けない。
            let last: String = g.iter().take_while(|w| w.id != c.id).last().map(|w| w.text.clone()).unwrap_or_default();
            let decimals = last.contains(['・', '．']) && c.text.contains(['・', '．']) && !last.ends_with(['・', '．']);
            let item = text == last && (1..=4).contains(&last.chars().count()) && last.chars().all(numeral) && !last.contains('・');
            let values = text.ends_with(numeral) && c.text.starts_with(numeral) && c.y - prev_end >= em * 0.75 && (decimals || item);
            if latin_space(&text, prev_end, c) {
                text.push(' ');
            } else if !text.is_empty() && !spaced_heading && !text.ends_with(char::is_whitespace) && !c.text.starts_with(char::is_whitespace) && (c.y - prev_end >= em * 1.5 || ruled || values) {
                // 列の中で 1.5 字分以上空けて続く語（住所の終わり「〇三号室」と、下にそろえた社名「藍牧科技株式会社」）は
                // 別の項目なので、全角空白で区切る（「〇三号室藍牧科技株式会社」のようにつながっていた）。
                text.push('\u{3000}');
            }
            text.push_str(&c.text);
            prev_end = prev_end.max(c.y1);
            prev_chars = chars;
        }
        let top = g.iter().map(|c| c.y).fold(f32::INFINITY, f32::min);
        if top < MARGIN_NOISE_TOP && is_margin_noise(&text) {
            continue;
        }
        lines.push(Line::text(text, g.iter().map(|c| c.id).collect()));
    }
    lines.extend(blocks.flat_map(|(_, block)| block));
    lines
}

/// 縦組みの列の語を欄に分ける（0.75 字分以上空いた所で区切る）。欄ごとの (上端, 下端, 語)。
fn record_cells(g: &[Col]) -> Vec<(f32, f32, Vec<&Col>)> {
    let mut cells: Vec<(f32, f32, Vec<&Col>)> = Vec::new();
    for c in g {
        let em = (c.x1 - c.x).max(1.0);
        match cells.last_mut() {
            Some(last) if c.y - last.1 < em * 0.75 => {
                last.1 = last.1.max(c.y1);
                last.2.push(c);
            }
            _ => cells.push((c.y, c.y1, vec![c])),
        }
    }
    cells
}

/// [`page_lines_with`] の、罫線の無い縦組みの名簿の行（列）の欄を次の列へ折り返したもの。行の頭の列（欄が 4 つ以上）の
/// すぐ左に続く、欄の少ない列で、どの語も頭の列の欄の上端から次の欄の上端までに収まり、列の頭が頭の列の欄の上端に
/// そろうものを、頭の列の続きとする。頭の列の添字から続きの列の添字への対応（続きの列は空の対応）。頭の列が 2 つ以上
/// ある組（名簿）だけ。
fn record_continuations(groups: &[Vec<Col>]) -> std::collections::HashMap<usize, Vec<usize>> {
    let mut out = std::collections::HashMap::new();
    // 欄が 5 つ以上（名称・代表者・会計責任者・所在地・…）で、1 字だけの欄（字間を空けた欄見出しの字）は 2 つまで。
    let cands: Vec<usize> = (0..groups.len())
        .filter(|&i| {
            let cells = record_cells(&groups[i]);
            // 数の欄（届出年月日「八、四、二七」、延長「〇・〇六九」、番号「2026－ＴＡ0003」）のある行（氏名だけの名簿は除く）。
            let numbered = cells.iter().any(|c| {
                let t: String = c.2.iter().map(|w| w.text.as_str()).collect();
                t.chars().any(|ch| ch.is_ascii_digit() || ('０'..='９').contains(&ch))
                    || (t.chars().any(|ch| "〇一二三四五六七八九十".contains(ch)) && t.chars().any(|ch| "、・･〜︑".contains(ch)))
            });
            numbered && cells.len() >= 5 && cells.iter().filter(|c| c.2.iter().map(|w| w.text.chars().count()).sum::<usize>() == 1).count() <= 2
        })
        .collect();
    // 名簿の行の頭の列は、ほかの行の頭の列と欄の上端が 4 つ以上そろう（欄の段がそろう表）。
    let tops = |i: usize| record_cells(&groups[i]).iter().map(|c| c.0).collect::<Vec<f32>>();
    let heads: Vec<usize> = cands
        .iter()
        .copied()
        .filter(|&i| {
            let ti = tops(i);
            cands.iter().any(|&j| j != i && tops(j).iter().filter(|&&t| ti.iter().any(|&u| (u - t).abs() < 1.0)).count() >= 4)
        })
        .collect();
    if heads.len() < 2 {
        return out;
    }
    let center = |g: &[Col]| g.iter().map(|c| (c.x + c.x1) / 2.0).sum::<f32>() / g.len().max(1) as f32;
    for &h in &heads {
        let head = record_cells(&groups[h]);
        let em = groups[h].iter().map(|c| c.x1 - c.x).fold(0.0, f32::max).max(1.0);
        // 欄 i の範囲は、欄の上端から次の欄の上端まで（最後の欄は下端まで）。
        let band = |y0: f32, y1: f32| {
            (0..head.len()).find(|&i| {
                let lo = head[i].0 - 1.0;
                let hi = head.get(i + 1).map_or(head[i].1 + 1.0, |n| n.0 + 1.0);
                y0 >= lo && y1 <= hi
            })
        };
        let mut conts = Vec::new();
        let mut prev = center(&groups[h]);
        for k in h + 1..groups.len() {
            let g = &groups[k];
            if g.is_empty() || heads.contains(&k) {
                break;
            }
            let x = center(g);
            // すぐ左の列（2 字分まで）で、欄が頭の列より少なく、列の頭が頭の列のどれかの欄の上端にそろう。
            let first: usize = head[0].2.iter().map(|c| c.text.chars().count()).sum();
            // 所在地の番地だけが次の列へ折り返す場合は、「一四」「－」「四」のように 1 字の語も含む。
            // 項目の「称」「党」や同上の「〃」を続きにしないよう、所在地欄に入る数字・番地記号に限る。
            let number = |ch: char| ch.is_ascii_digit() || ('０'..='９').contains(&ch) || "〇零一二三四五六七八九十百千万－−-".contains(ch);
            let address = |i: usize| {
                let text: String = head[i].2.iter().map(|c| c.text.as_str()).collect();
                text.chars().take(4).any(|ch| "都道府県".contains(ch))
                    && text.chars().any(|ch| "市区町村郡".contains(ch))
                    // 二段の変更届で同じ欄に住所と会計責任者の氏名が続く行は対象外。
                    && text.chars().last().is_some_and(number)
            };
            let address_number = |c: &Col| !c.text.is_empty() && c.text.chars().all(number) && band(c.y, c.y1).is_some_and(address);
            // 通常は 2〜14 字。1 字は所在地欄の数字・番地記号だけを許す。
            let fragment = |c: &Col| {
                let n = c.text.chars().count();
                ((2..=14).contains(&n) || (n == 1 && address_number(c))) && !c.text.chars().any(|ch| ch.is_ascii_alphabetic())
            };
            let address_only = g.iter().all(address_number);
            let dated = head[0].2.first().is_some_and(|c| c.text.starts_with("令和"));
            // 続きの列の頭は、頭の列の長い欄（名称・所在地）の上端にそろう。
            let starts_cell = head.iter().any(|hc| (hc.0 - g[0].y).abs() < 1.0 && hc.2.iter().map(|c| c.text.chars().count()).sum::<usize>() >= 4);
            if prev - x > em * 2.0 || record_cells(g).len() >= head.len() || !starts_cell || (first < 5 && !address_only) || dated || !g.iter().all(fragment) {
                break;
            }
            // 続きの語の入る欄は、欄の中で折り返す長い欄（4 字以上の名称・所在地。2 字の氏の欄に別の行の氏名をつながない）。
            let long = |i: usize| head[i].2.iter().map(|c| c.text.chars().count()).sum::<usize>() >= 4;
            if !g.iter().all(|c| band(c.y, c.y1).is_some_and(long)) {
                break;
            }
            conts.push(k);
            prev = x;
        }
        if !conts.is_empty() {
            for &k in &conts {
                out.insert(k, Vec::new());
            }
            out.insert(h, conts);
        }
    }
    out
}

/// [`page_lines`] の、罫線の無い縦組みの表を表として組む版。行の見出しの記号（自動車の認定番号
/// 「2026－ＧＥ0002」など）が各行（縦の列）の頭に同じ高さで 3 つ以上並ぶ表は、縦の列のままだと
/// 欄が切れずに 1 列ずつの行になる。縦組みの表は 90 度回すと横組みの表なので、語を回して
/// [`keyed_tables`] で組み、表を回し戻して読み順の位置に差し込む。
///
/// `vrules`・`hrules` は [`page_lines_with`] に渡す罫線。
fn page_lines_keyed(cols: Vec<Col>, vrules: &[(f32, f32, f32)], hrules: &[(f32, f32, f32)]) -> Vec<Line> {
    let tables = vertical_keyed_tables(&cols);
    if tables.is_empty() {
        return page_lines_with(cols, vrules, hrules);
    }
    let used: std::collections::HashSet<usize> = tables.iter().flat_map(|t| t.cells.iter().flat_map(|c| c.words.iter().copied())).collect();
    let rest: Vec<Col> = cols.into_iter().filter(|c| !used.contains(&c.id)).collect();
    place_tables(rest, tables, |c| page_lines_with(c, vrules, hrules))
}

/// 罫線の無い縦組みの表を組む。欄が 3 つ以上のものだけ（縦組みの段では、セルの 2 行目に組まれた型式
/// （「ＡＳＡＬ263ＡＮ」）が並ぶだけでも番号の列に見えるので、欄の少ないものは表にしない）。
fn vertical_keyed_tables(cols: &[Col]) -> Vec<crate::trace::Table> {
    // 右の列が上の行になるように回す（x → y、y → -x）。
    let turned: Vec<Col> =
        cols.iter().map(|c| Col { id: c.id, x: c.y, x1: c.y1, y: -c.x1, y1: -c.x, text: c.text.clone() }).collect();
    let (tables, _) = keyed_tables(turned, &[]);
    let back = |(x0, x1, y0, y1): (f32, f32, f32, f32)| (-y1, -y0, x0, x1);
    tables
        .into_iter()
        .filter(|t| t.cols >= 3)
        .map(|mut t| {
            (t.x0, t.x1, t.y0, t.y1) = back((t.x0, t.x1, t.y0, t.y1));
            for c in &mut t.cells {
                (c.x0, c.x1, c.y0, c.y1) = back((c.x0, c.x1, c.y0, c.y1));
            }
            t
        })
        .collect()
}

/// 罫線の無い縦組みの表の枠 `(x0, x1, y0, y1)`（段の切り方で表を割らないため）。
fn keyed_table_boxes(cols: &[Col]) -> Vec<(f32, f32, f32, f32)> {
    vertical_keyed_tables(cols).into_iter().map(|t| (t.x0, t.x1, t.y0, t.y1)).collect()
}

/// 横組みの囲みの表の左端の、縦の罫線で区切った縦書きの見出し（貸借対照表の「資産の部」「負債及び純資産の部」）の行。
/// 見出しは 1 字ずつの語か縦長の語で、2 列に分けて組んだもの（「負債及び」と「純資産の部」）は左の列から読む。
/// 罫線の左の語がどれも見出しの字（10 字までの漢字・仮名の、1 字か縦長の語）で、幅が 4 字分までのときだけ。見出しは
/// 上下に 1.5 字分より離れているか、間に見出しの欄を横切る横の罫線があれば別の見出しにする。
fn side_labels(block: &[&Col], vrules: &[(f32, f32, f32)], hrules: &[(f32, f32, f32)]) -> Vec<Line> {
    let label_like = |c: &Col| {
        let n = c.text.chars().count();
        (1..=10).contains(&n)
            && c.text.chars().all(|ch| ('\u{3040}'..='\u{30FF}').contains(&ch) || ('\u{4E00}'..='\u{9FFF}').contains(&ch))
            && (n == 1 || c.y1 - c.y > (c.x1 - c.x) * 1.5)
    };
    let center = |c: &Col| ((c.x + c.x1) / 2.0, (c.y + c.y1) / 2.0);
    let (bx0, bx1) = (block.iter().map(|c| c.x).fold(f32::INFINITY, f32::min), block.iter().map(|c| c.x1).fold(f32::NEG_INFINITY, f32::max));
    for &(x, ry0, ry1) in vrules {
        if !(bx0 + 2.0 < x && x < bx1 - 2.0) {
            continue;
        }
        let within = |c: &Col| ry0 < center(c).1 && center(c).1 < ry1;
        let left: Vec<&Col> = block.iter().copied().filter(|c| within(c) && c.x1 <= x + 1.0).collect();
        let right = block.iter().filter(|c| within(c) && c.x >= x - 1.0).count();
        let em = left.iter().map(|c| (c.x1 - c.x).min(c.y1 - c.y)).fold(0f32, f32::max);
        let span = left.iter().map(|c| c.x1).fold(f32::NEG_INFINITY, f32::max) - left.iter().map(|c| c.x).fold(f32::INFINITY, f32::min);
        if left.len() < 2 || right < 2 || !left.iter().all(|c| label_like(c)) || span > em * 4.0 {
            continue;
        }
        let (lx0, lx1) = (left.iter().map(|c| c.x).fold(f32::INFINITY, f32::min), left.iter().map(|c| c.x1).fold(f32::NEG_INFINITY, f32::max));
        let ruled = |a: f32, b: f32| hrules.iter().any(|&(y, x0, x1)| a - 1.0 <= y && y <= b + 1.0 && x0 <= lx0 + 1.0 && lx1 - 1.0 <= x1);
        let mut left = left;
        left.sort_by(|a, b| a.y.total_cmp(&b.y));
        let mut groups: Vec<Vec<&Col>> = Vec::new();
        let mut bottom = f32::NEG_INFINITY;
        for c in left {
            if c.y - bottom > em * 1.5 || (bottom.is_finite() && ruled(bottom, c.y)) {
                groups.push(Vec::new());
            }
            bottom = bottom.max(c.y1);
            groups.last_mut().unwrap().push(c);
        }
        return groups
            .into_iter()
            .map(|mut g| {
                // 左の列から、列の中は上から。
                g.sort_by(|a, b| center(a).0.total_cmp(&center(b).0));
                let mut columns: Vec<Vec<&Col>> = Vec::new();
                for c in g {
                    match columns.last_mut() {
                        Some(col) if center(c).0 - center(col[0]).0 < em * 0.5 => col.push(c),
                        _ => columns.push(vec![c]),
                    }
                }
                let words: Vec<&Col> = columns
                    .into_iter()
                    .flat_map(|mut col| {
                        col.sort_by(|a, b| a.y.total_cmp(&b.y));
                        col
                    })
                    .collect();
                Line::text(words.iter().map(|c| c.text.as_str()).collect::<String>(), words.iter().map(|c| c.id).collect())
            })
            .collect();
    }
    Vec::new()
}

/// 横組みの欄を、欄の高さいっぱいの溝（16pt 以上）で左右に分けた囲みごとの欄 `[(右端, 語)]`。分けた囲みがどれも
/// 100pt 以上の幅で、幅がそろい（最も広いものが最も狭いものの 1.2 倍まで）、4 行以上あるときだけ分ける（行の高さの
/// そろう囲みが横に並ぶと、溝の左右が表の欄に見えて囲みの行が 1 行につながる）。
fn split_side_by_side_boxes(x1: f32, block: Vec<Col>) -> Vec<(f32, Vec<Col>)> {
    let mut spans: Vec<(f32, f32)> = block.iter().map(|c| (c.x, c.x1)).collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut cuts: Vec<f32> = Vec::new();
    let mut right = f32::NEG_INFINITY;
    for (a, b) in spans {
        if right.is_finite() && a - right >= 16.0 {
            cuts.push((a + right) / 2.0);
        }
        right = right.max(b);
    }
    if cuts.is_empty() {
        return vec![(x1, block)];
    }
    let mut parts: Vec<Vec<Col>> = vec![Vec::new(); cuts.len() + 1];
    for c in &block {
        let k = cuts.iter().filter(|&&cut| cut < c.x).count();
        parts[k].push(c.clone());
    }
    let width = |p: &[Col]| p.iter().map(|c| c.x1).fold(f32::NEG_INFINITY, f32::max) - p.iter().map(|c| c.x).fold(f32::INFINITY, f32::min);
    let widths: Vec<f32> = parts.iter().map(|p| width(p)).collect();
    let (narrow, wide) = (widths.iter().copied().fold(f32::INFINITY, f32::min), widths.iter().copied().fold(f32::NEG_INFINITY, f32::max));
    if narrow < 100.0 || wide > narrow * 1.2 || parts.iter().any(|p| row_lines(p.clone()).len() < 4) {
        return vec![(x1, block)];
    }
    parts.into_iter().map(|p| (p.iter().map(|c| c.x1).fold(f32::NEG_INFINITY, f32::max), p)).collect()
}

/// 縦書きの段の中の横組みの欄の枠 `(x0, x1, y0, y1)`（段の切り方で欄を割らないため。下の段に横に並ぶ決算公告の
/// 囲みが、見出しと貸借対照表の間の空きで上下の段に割れ、見出しと表が離れていた）。
fn horizontal_block_boxes(cols: &[Col]) -> Vec<(f32, f32, f32, f32)> {
    let (_, blocks) = split_off_horizontal_blocks(cols.to_vec(), &[], &[]);
    // 囲みのように何行もある欄だけ（号外の題字の下の発行者の欄などは、これまでどおり段に切ってから組む）。
    blocks
        .iter()
        .filter(|(_, lines)| lines.len() >= 6)
        .map(|(_, lines)| {
            let ids: std::collections::HashSet<usize> = lines.iter().flat_map(|l| l.words.iter().copied()).collect();
            cols.iter().filter(|c| ids.contains(&c.id)).fold((f32::INFINITY, f32::NEG_INFINITY, f32::INFINITY, f32::NEG_INFINITY), |(x0, x1, y0, y1), c| {
                (x0.min(c.x), x1.max(c.x1), y0.min(c.y), y1.max(c.y1))
            })
        })
        .filter(|b| b.0 < b.1)
        .collect()
}

/// 縦書きの段の中の横組みの欄を取り出す。`(残りの語, [(欄の右端, 欄の行)])`（欄は右端の降順）。
///
/// 横組みの語は 2 字以上で横長のもの。縦の列の途中に挟まった語（縦中横の数字など）は除く。
/// 横組みの語の行が 2 行以上（上下に 1.5 行分以内の間隔で続き、左右が重なる）続く欄だけを
/// 取り出す（1 行だけの横長の語は、表の欄の名前などと区別が付かないので列のまま読む）。
fn split_off_horizontal_blocks(cols: Vec<Col>, vrules: &[(f32, f32, f32)], hrules: &[(f32, f32, f32)]) -> (Vec<Col>, Vec<(f32, Vec<Line>)>) {
    // 縦の列（2 字以上の縦長の語）のすぐ上か下（全角空白 1 字ほどの空きまで。「高橋　佑」）に続く語。
    // `inside` の中で語のすぐ下から始まる列（欄に入れる縦書きの見出し「資産の部」）は見ない（そのすぐ上の欄見出し
    // 「科　目」の「科」が欄から外れていた）。縦の罫線のすぐ左（3 字分以内）の語なら、すぐ上の列も見ない（表の左端の
    // 縦書きの見出しの欄で、「資産の部」の下の「負債及び純資産の部」の頭の「純」が欄から外れていた）。
    let in_column_but = |w: &Col, inside: Option<(f32, f32, f32, f32)>| {
        let em = w.y1 - w.y;
        let near = |gap: f32| -em * 0.5 < gap && gap < em * 1.5;
        let wy = (w.y + w.y1) / 2.0;
        let label_cell = vrules.iter().any(|&(x, y0, y1)| y0 < wy && wy < y1 && w.x1 <= x + 1.0 && x - w.x1 < em * 3.0);
        cols.iter().any(|c| {
            let (cx, cy) = ((c.x + c.x1) / 2.0, (c.y + c.y1) / 2.0);
            let label = inside.is_some_and(|(x0, y0, x1, y1)| x0 < cx && cx < x1 && y0 < cy && cy < y1 && c.text.chars().count() <= 10);
            c.text.chars().count() >= 2
                && c.y1 - c.y > (c.x1 - c.x) * 1.5
                && w.x < (c.x + c.x1) / 2.0
                && (c.x + c.x1) / 2.0 < w.x1
                && ((near(w.y - c.y1) && !(label && label_cell)) || (near(c.y - w.y1) && !label))
        })
    };
    let wide: Vec<usize> = (0..cols.len())
        .filter(|&i| {
            let c = &cols[i];
            c.text.chars().count() >= 2 && c.x1 - c.x > (c.y1 - c.y) * 1.5 && !in_column_but(c, None)
        })
        .collect();
    if wide.len() < 2 {
        return (cols, Vec::new());
    }
    // 横組みの語を、上下に続き左右が重なる語ごとの欄にまとめる（union-find）。
    let mut parent: Vec<usize> = (0..wide.len()).collect();
    fn root(parent: &mut [usize], i: usize) -> usize {
        let mut r = i;
        while parent[r] != r {
            r = parent[r];
        }
        parent[i] = r;
        r
    }
    for a in 0..wide.len() {
        for b in a + 1..wide.len() {
            let (p, q) = (&cols[wide[a]], &cols[wide[b]]);
            let em = (p.y1 - p.y).max(q.y1 - q.y);
            let same_row = (p.y + p.y1 - q.y - q.y1).abs() / 2.0 < em * 0.5 && (p.x.max(q.x) - p.x1.min(q.x1)) < em * 3.0;
            let stacked = p.x < q.x1 && q.x < p.x1 && (p.y.max(q.y) - p.y1.min(q.y1)) < em * 1.5;
            if same_row || stacked {
                let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
                parent[ra] = rb;
            }
        }
    }
    let mut groups: std::collections::BTreeMap<usize, Vec<usize>> = std::collections::BTreeMap::new();
    for (k, &i) in wide.iter().enumerate() {
        let r = root(&mut parent, k);
        groups.entry(r).or_default().push(i);
    }
    // 縦組みの列（2 字以上の縦長の語）。
    let tall = |c: &Col| c.text.chars().count() >= 2 && c.y1 - c.y > (c.x1 - c.x) * 1.5;
    let center = |c: &Col| ((c.x + c.x1) / 2.0, (c.y + c.y1) / 2.0);
    let mut taken = vec![false; cols.len()];
    let mut blocks: Vec<(f32, Vec<Col>)> = Vec::new();
    // 欄に入れた縦書きの見出し（行に組むときは頭の 1 字分の高さだけを見る）。
    let mut shrink: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for mut members in groups.into_values() {
        // 先に取り出した欄が広がって取り込んだ語は除く。
        members.retain(|&i| !taken[i]);
        if members.is_empty() {
            continue;
        }
        // 欄の中の 1 字ずつの語・正方形の語（字間を空けた見出し「第　67　期　決　算　公　告」、表の
        // 「流　動　資　産」、2 桁の数字）も欄に入れる。欄の左右の範囲で、上下に 2 行分まで広げる。
        let bounds = |members: &[usize]| {
            members.iter().fold((f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY, 0f32), |(x0, y0, x1, y1, em), &i| {
                let c = &cols[i];
                (x0.min(c.x), y0.min(c.y), x1.max(c.x1), y1.max(c.y1), em.max((c.y1 - c.y).min(c.x1 - c.x)))
            })
        };
        // 同じ行で 4 字分以内に続く語（表の行頭の「流　動　資　産」と隣の欄の金額の間）も入れる。
        loop {
            let (x0, y0, x1, y1, em) = bounds(&members);
            let beside = |c: &Col| {
                let cy = center(c).1;
                y0 < cy
                    && cy < y1
                    && members.iter().any(|&j| {
                        let m = &cols[j];
                        (center(m).1 - cy).abs() < em * 0.5 && m.x.max(c.x) - m.x1.min(c.x1) < em * 4.0
                    })
            };
            // 欄の右の外の縦の列のすぐ隣（1.5 字分以内）で上下に重なる 1 字の語（住所の列「…一一番一九」の続きの「号」）は、縦組みの
            // 列の字（欄に入れると欄が広がり、隣の公告の署名の列「（乙）有限会社つる軒」まで欄に取り込んでいた）。
            let column_char = |c: &Col| {
                let em = (c.x1 - c.x).max(c.y1 - c.y);
                c.text.trim().chars().count() == 1
                    && cols.iter().any(|t| {
                        let tx = (t.x + t.x1) / 2.0;
                        // 欄の右の列だけ（表の左端の縦書きの見出し「資産の部」の右の「流動資産」の頭の「流」は欄の字）。
                        tall(t) && tx > x1 && (tx - center(c).0).abs() <= em * 1.5 && t.y < c.y1 && c.y < t.y1
                    })
            };
            let more: Vec<usize> = (0..cols.len())
                .filter(|&i| !members.contains(&i) && !taken[i] && !tall(&cols[i]) && !in_column_but(&cols[i], Some((x0, y0, x1, y1))) && !column_char(&cols[i]))
                .filter(|&i| {
                    let (cx, cy) = center(&cols[i]);
                    (x0 - em * 0.5 < cx && cx < x1 + em * 0.5 && y0 - em * 2.0 < cy && cy < y1 + em * 2.0) || beside(&cols[i])
                })
                .collect();
            if more.is_empty() {
                break;
            }
            members.extend(more);
        }
        // 横に並ぶ行（横組みの語か、横に並ぶ 2 語以上の行）が 2 行以上ある欄だけ（本紙の題字の下は
        // 「発　行　内　閣　府」が 1 字ずつの語で、横組みの語の行は「（原稿作成　国立印刷局）」だけ）。
        let wide_ids: std::collections::HashSet<usize> = wide.iter().map(|&i| cols[i].id).collect();
        let rows = row_lines(members.iter().map(|&i| cols[i].clone()).collect())
            .iter()
            .filter(|l| l.words.len() >= 2 || l.words.iter().any(|id| wide_ids.contains(id)))
            .count();
        if rows < 2 {
            continue;
        }
        // 欄の中に縦の列が入り込んでいれば（横組みの語が縦書きの表の中に散っているだけ）取り出さない。ただし入り込んだ
        // 列がどれも 10 字までで、字数の合計が欄の字数の 5 分の 1 までなら、横組みの表の縦書きの見出し（貸借対照表の
        // 「資産の部」「負債及び純資産の部」）とみなして欄に入れる（号外の決算公告のページで、縦組みの公告と同居する
        // 横組みの決算公告の貸借対照表を列ごとに読み、「日資資有形固定資産無形固定資産」のようになっていた）。
        let (x0, y0, x1, y1, _) = bounds(&members);
        let intruders: Vec<usize> = cols
            .iter()
            .enumerate()
            .filter(|&(i, c)| {
                let (cx, cy) = center(c);
                !members.contains(&i) && !taken[i] && tall(c) && x0 < cx && cx < x1 && y0 < cy && cy < y1
            })
            .map(|(i, _)| i)
            .collect();
        if !intruders.is_empty() {
            let chars = |i: &usize| cols[*i].text.chars().filter(|c| !c.is_whitespace()).count();
            let block_chars: usize = members.iter().map(chars).sum();
            let label_chars: usize = intruders.iter().map(chars).sum();
            if intruders.iter().any(|i| chars(i) > 10) || label_chars * 5 > block_chars {
                continue;
            }
            // 縦書きの見出しは何行にもまたがるので、行に組むときは頭の 1 字分の高さだけを見る（そのままだと見出しの高さに
            // 掛かる行が 1 行につながる）。
            shrink.extend(intruders.iter().copied());
            members.extend(intruders);
        }
        for &i in &members {
            taken[i] = true;
        }
        blocks.push((
            x1,
            members
                .iter()
                .map(|&i| {
                    let mut c = cols[i].clone();
                    if shrink.contains(&i) {
                        c.y1 = c.y + (c.x1 - c.x);
                    }
                    c
                })
                .collect(),
        ));
    }
    // 同じ幅の囲みが左右に並ぶ欄（下の段の決算公告の囲み）は、囲みごとの欄に分ける（右から読む）。
    let blocks: Vec<(f32, Vec<Col>)> = blocks.into_iter().flat_map(|(x1, block)| split_side_by_side_boxes(x1, block)).collect();
    let mut blocks = blocks;
    blocks.sort_by(|a, b| b.0.total_cmp(&a.0));
    // 幅の広い欄（決算公告の囲みが左右に並ぶもの）は、横組みのページと同じく列の溝で左右に切ってから行に組む（行ごとに
    // 組むと、左右の囲みの行が 1 行につながる）。
    let by_id: std::collections::HashMap<usize, &Col> = cols.iter().map(|c| (c.id, c)).collect();
    let blocks = blocks
        .into_iter()
        .map(|(x1, block)| {
            // 囲みの中の縦書きの見出しは、見出しの欄の上端（見出しの上の、見出しの列を横切る横の罫線）より下の最初の行の
            // 頭に置く。
            let originals: Vec<&Col> = block.iter().filter_map(|c| by_id.get(&c.id).copied()).collect();
            let labels = side_labels(&originals, vrules, hrules);
            let label_ids: std::collections::HashSet<usize> = labels.iter().flat_map(|l| l.words.iter().copied()).collect();
            let block: Vec<Col> = block.into_iter().filter(|c| !label_ids.contains(&c.id)).collect();
            let x0 = block.iter().map(|c| c.x).fold(f32::INFINITY, f32::min);
            let mut lines = if x1 - x0 >= 300.0 { horizontal_lines(block) } else { row_lines(block) };
            for label in labels {
                let ws: Vec<&Col> = label.words.iter().filter_map(|id| by_id.get(id).copied()).collect();
                let top = ws.iter().map(|c| c.y).fold(f32::INFINITY, f32::min);
                let (lx0, lx1) = (ws.iter().map(|c| c.x).fold(f32::INFINITY, f32::min), ws.iter().map(|c| c.x1).fold(f32::NEG_INFINITY, f32::max));
                let rule = hrules.iter().filter(|&&(y, x0, x1)| y <= top && x0 <= lx0 + 1.0 && lx1 - 1.0 <= x1).map(|r| r.0).fold(f32::NEG_INFINITY, f32::max);
                let top = if rule.is_finite() { rule + 2.0 } else { top };
                let center = |l: &Line| {
                    let ys: Vec<f32> = l.words.iter().filter_map(|id| by_id.get(id)).map(|c| (c.y + c.y1) / 2.0).collect();
                    ys.iter().sum::<f32>() / ys.len().max(1) as f32
                };
                let at = lines.iter().position(|l| center(l) >= top - 2.0).unwrap_or(lines.len());
                // 見出しは欄の 1 行目（金額のある科目の行）の頭に付ける（罫線の表の行をまたぐ欄の名前と同じ）。
                match lines.get_mut(at) {
                    Some(first) if first.table.is_none() && first.text.trim_end().ends_with(|c: char| c.is_ascii_digit() || c == ')' || c == '）') => {
                        let words: Vec<usize> = label.words.iter().chain(&first.words).copied().collect();
                        *first = Line::text(format!("{}\u{3000}{}", label.text, first.text), words);
                    }
                    _ => lines.insert(at, label),
                }
            }
            (x1, lines)
        })
        .collect();
    let rest = cols.into_iter().zip(taken).filter(|(_, t)| !t).map(|(c, _)| c).collect();
    (rest, blocks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warichu_right_half_continues_with_letter_spaced_words() {
        // 割注の右半分が複数字の語の後に字間を空けた語で続く（「国家公安委員会、総」「務」「省、」）。
        // 左半分も字間を空けた 1 字の語（「文」「部」「科」「学」）と続きの語。
        let col = |id: usize, x: f32, y: f32, y1: f32, text: &str| Col { id, x, x1: x + 8.0, y, y1, text: text.to_string(),
        };
        let mut cols = vec![
            col(0, 403.6, 188.8, 236.8, "（令和二年"),
            col(1, 407.6, 236.8, 308.8, "国家公安委員会、総"),
            col(2, 407.6, 316.8, 324.8, "務"),
            col(3, 407.6, 332.8, 348.8, "省、"),
            col(4, 399.6, 236.8, 244.8, "文"),
            col(5, 399.6, 248.8, 256.8, "部"),
            col(6, 399.6, 260.8, 268.8, "科"),
            col(7, 399.6, 272.8, 280.8, "学"),
            col(8, 399.6, 284.8, 340.8, "省、国土交通省"),
            col(9, 403.6, 348.8, 508.8, "告示第一号）の一部を次のように改正する。",
            ),
        ];
        merge_warichu(&mut cols, &Default::default());
        let texts: Vec<&str> = cols.iter().map(|c| c.text.as_str()).collect();
        assert!(texts.contains(&"国家公安委員会、総務省、文部科学省、国土交通省"), "{texts:?}");
    }

    #[test]
    fn letter_spaced_horizontal_labels_do_not_make_a_horizontal_page() {
        // 縦書きの本文（縦長の語）の中に、字間を空けた横書きの表の欄の名前が多いページ。
        let col = |x: f32, y: f32, x1: f32, y1: f32, text: &str| Col { id: 0, x, x1, y, y1, text: text.to_string(),
        };
        let mut cols = vec![col(500.0, 60.0, 508.0, 140.0, "縦書きの本文の一行"), col(490.0, 60.0, 498.0, 140.0, "縦書きの本文の二行"),
        ];
        for i in 0..4 {
            // 5 字を 0.5em 空けて 1 語にしたもの（幅 56pt、高さ 8pt）。
            cols.push(col(100.0, 200.0 + 10.0 * i as f32, 156.0, 208.0 + 10.0 * i as f32, "利益剰余金",
            ));
        }
        assert!(!is_horizontal_page(&cols));
        // 字間を空けていない横書きの語なら横組みのページ。
        cols.push(col(100.0, 300.0, 260.0, 308.0, "横書きの本文の一行に並ぶ二十字の文字列です",
        ));
        assert!(is_horizontal_page(&cols));
    }

    #[test]
    fn display_boxes_of_vertical_words_sit_on_the_glyphs() {
        // 縦書きの語の枠は字面より右へ半 em・上へ 1 em ずれて出る。表示用の語の座標はどちらも戻す
        // （読み順の復元に使う座標は上下だけ戻す）。横組みの語は動かさない。
        let xhtml = r#"<html><body><doc><page width="595" height="842">
            <word xMin="100" yMin="100" xMax="108" yMax="180">縦書きの本文の列です</word>
            <word xMin="200" yMin="300" xMax="280" yMax="308">横組みの語の並びです</word>
        </page></doc></body></html>"#;
        let page = &analyze_pages(xhtml)[0];
        let w = |t: &str| page.words.iter().find(|w| w.text == t).unwrap();
        let v = w("縦書きの本文の列です");
        assert_eq!((v.x0, v.y0, v.x1, v.y1), (96.0, 108.0, 104.0, 188.0));
        let h = w("横組みの語の並びです");
        assert_eq!((h.x0, h.y0, h.x1, h.y1), (200.0, 300.0, 280.0, 308.0));
    }

    #[test]
    fn a_frame_containing_only_header_labels_is_not_a_leading_table() {
        // 右端の狭いラベル欄の内部罫線を、表全体の左端として切り出さない。
        let col = |id, x, y, y1, text: &str| Col { id, x, x1: x + 8.0, y, y1, text: text.into(),
        };
        let cols = vec![
            col(0, 540.0, 240.0, 264.0, "改正後"),
            col(1, 540.0, 600.0, 624.0, "改正前"),
            col(2, 450.0, 80.0, 410.0, "表の本体の右の列"),
            col(3, 438.0, 80.0, 410.0, "表の本体の次の列"),
            col(4, 426.0, 80.0, 410.0, "表の本体の左の列"),
        ];
        let rules = PageRules {
            enclosures: Vec::new(), short_fraction_bars: Vec::new(),
            vertical: vec![crate::lines::VRule { x: 532.0, y0: 60.0, y1: 790.0,
            }],
            horizontal: vec![crate::lines::HRule { y: 430.0, x0: 532.0, x1: 553.0,
            }],
        };
        assert!(split_off_leading_frame(cols, Some(&rules)).is_err());
    }

    #[test]
    fn trailing_frame_requires_a_closed_frame_and_a_numbered_bridging_intro() {
        let col = |x, y, y1, text: &str| Col { id: 0, x, x1: x + 8.0, y, y1, text: text.into(),
        };
        let cols = vec![
            col(100.0, 200.0, 224.0, "変更後"), col(100.0, 600.0, 624.0, "変更前"),
            col(5.0, 50.0, 300.0, "上欄本文"), col(5.0, 450.0, 750.0, "下欄本文"),
            col(130.0, 0.0, 600.0, "変更指示の長い本文"), col(146.0, 0.0, 56.0, "二変更した事項"),
            col(162.0, 0.0, 350.0, "前記事末尾"), col(178.0, 0.0, 350.0, "前記事本文"), col(194.0, 0.0, 350.0, "前記事先頭"),
            col(170.0, 420.0, 700.0, "次記事末尾"), col(186.0, 420.0, 700.0, "次記事本文"), col(202.0, 420.0, 700.0, "次記事先頭"),
        ];
        let mut rules = TierRules { tables: vec![], boxes: vec![], hrules: vec![(400.0, 0.0, 120.0)], vrules: vec![(0.0, 0.0, 800.0), (120.0, 0.0, 800.0)], sideways: false,
        };
        let Ok((rest, frame)) = split_off_trailing_label_frame(cols.clone(), &rules) else { panic!("閉じた全高枠") };
        assert!(rest.iter().any(|c| c.text == "前記事末尾"));
        assert!(frame.iter().any(|c| c.text == "二変更した事項"));
        for variant in 0..4 {
            let mut negative = cols.clone();
            match variant {
                0 => negative[5].text = "前記事の末尾".into(),
                1 => negative[4].y1 = 300.0,
                2 => { negative[0].text = "変更前".into(); negative[1].text = "変更後".into(); }
                _ => negative[6].y1 = 700.0,
            }
            assert!(split_off_trailing_label_frame(negative, &rules).is_err());
        }
        rules.vrules.clear();
        assert!(split_off_trailing_label_frame(cols, &rules).is_err());
    }

    #[test]
    fn finds_lower_band_of_only_marks() {
        // 新設の条が続く新旧対照表の続きのページ: 上の帯に改正後の条文、下の帯に改正前の「（新設）」だけ。
        let col = |x: f32, y: f32, y1: f32, t: &str| Col { id: 0, x, x1: x + 8.0, y, y1, text: t.to_string(),
        };
        let mut cols = vec![
            col(500.0, 60.0, 380.0, "第九十八条の十四　医薬部外品の製造業者は",
            ),
            col(488.0, 60.0, 380.0, "号のいずれかに該当する責任技術者を"),
            col(476.0, 60.0, 300.0, "一　薬剤師"),
            col(500.0, 440.0, 472.0, "（新設）"),
        ];
        let divider = marker_band_divider(&cols, 842.0).unwrap();
        assert!(380.0 < divider && divider < 440.0);
        // 印でない語が下の帯にもあれば違う。
        cols.push(col(300.0, 450.0, 600.0, "改正前の条文"));
        assert_eq!(marker_band_divider(&cols, 842.0), None);
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
}

#[cfg(test)]
mod raised_section_label_tests {
    use super::*;
    fn fixture() -> (Vec<Unit>, Vec<Word>) {
        let words = [(500.,480.,509.,545.,"官庁事項"),(484.,428.,492.,501.,"関東地方整備局公示"),(525.,528.,539.,668.,"官庁報告"),
        ]
            .into_iter().enumerate().map(|(id,(x0,y0,x1,y1,text))| Word{source_box: [x0, y0, x1, y1],id,x0,y0,x1,y1,raw:text.into(),text:text.into(),status:WordStatus::Used,
        }).collect();
        let units=vec![Unit::Plain(vec![Line::text("官庁事項".into(),vec![0]),Line::text("関東地方整備局公示".into(),vec![1]),
            ]), Unit::Plain(vec![Line::text("官庁報告".into(),vec![2])]),
        ];
        (units,words)
    }
    fn texts(units: &[Unit]) -> String {units.iter().map(Unit::text).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n")}
    #[test]
    fn adjacent_larger_vertical_section_precedes_its_field_and_short_title() {
        let (mut u,w)=fixture();order_section_labels(&mut u, &w);assert_eq!(texts(&u),"官庁報告\n官庁事項\n関東地方整備局公示");
        let (mut u,w)=fixture();if let Unit::Plain(p) = &mut u[0]{p.insert(0,Line::text("前の記事の終わり。".into(),vec![]));}order_section_labels(&mut u, &w);assert!(texts(&u).starts_with("前の記事の終わり。\n官庁報告\n官庁事項"));
        let (mut u,w)=fixture();if let Unit::Plain(p) = &mut u[0]{p.pop();}order_section_labels(&mut u, &w);assert_eq!(texts(&u),"官庁報告\n官庁事項");
    }
    #[test]
    fn separated_equal_sized_horizontal_and_body_lines_are_not_reordered() {
        for case in 0..8 {
            let (mut u,mut w)=fixture();
            match case {
                0=>w[2].x1=w[2].x0+9.,
                1=>{w[2].x0=450.;w[2].x1=464.;}
                2=>{w[2].y0=700.;w[2].y1=840.;}
                3=>{w[2].x0=600.;w[2].x1=614.;}
                4=> {
                    if let Unit::Plain(p) = &mut u[0]{p[1].text="本文が始まる。".into();}
                }
                5=> {
                    if let Unit::Plain(p) = &mut u[1]{p[0].words=vec![99];}
                }
                6=> {
                    if let Unit::Plain(p) = &mut u[0]{p[0].text="通常の本文".into();}
                }
                7=>{w[0].x1=550.;w[0].y1=489.;}
                _=>unreachable!(),
            }
            let before=texts(&u);order_section_labels(&mut u, &w);assert_eq!(texts(&u),before,"negative {case}");
        }
        let mut u=vec![Unit::Plain(vec![Line::text("〇総務省告示第三号".into(),vec![])]),Unit::Plain(vec![Line::text("その他告示".into(),vec![])]),
        ];
        order_section_labels(&mut u, &[]);assert_eq!(texts(&u),"その他告示\n〇総務省告示第三号");
    }
}

#[cfg(test)]
mod low_line_url_tests {
    use super::*;
    fn columns(prev: &str, tail: &str, dy: f32) -> Vec<Vec<Col>> {
        column_groups(vec![
            Col{id:0,x:100.,x1:108.,y:40.,y1:160.,text:prev.into(),
            },
            Col{id:1,x:84.,x1:92.,y:0.,y1:39.,text:"五、意見書の提出".into(),
            },
            Col{id:2,x:89.,x1:97.,y:40.+dy,y1:100.+dy,text:tail.into(),
            },
        ])
    }
    #[test]
    fn low_line_delimiter_keeps_url_tail_before_the_next_heading() {
        for prev in ["business/wind̲power/kankyo̲", "https://example.test/path_", "https://example.test/path/",
        ] {
            let groups=columns(prev,"assessment/",0.);
            assert_eq!(groups[0].iter().map(|c|c.id).collect::<Vec<_>>(),vec![0,2],"{prev}");
            assert_eq!(groups[1].iter().map(|c|c.id).collect::<Vec<_>>(),vec![1]);
        }
    }
    #[test]
    fn independent_urls_shifted_lines_and_identifiers_are_not_carried() {
        for (prev,tail,dy) in [
            ("business/wind̲power/kankyo̲","https://other.test/",0.),
            ("business/wind̲power/kankyo̲","www.other.test/",0.),
            ("business/wind̲power/kankyo̲","assessment/",3.),
            ("https://example.test/path","assessment/",0.),
            ("identifier̲","assessment/",0.),
        ] {
            let groups=columns(prev,tail,dy);
            assert!(!groups[0].iter().any(|c|c.id==2),"{prev} {tail} {dy}");
        }
    }
}

#[cfg(test)]
#[path = "mixed_native_bbox_tests.rs"]
mod mixed_native_bbox_tests;

#[cfg(test)]
#[path = "wrapped_native_quote_tests.rs"]
mod wrapped_native_quote_tests;

/// 横倒しの縦組みで、連続する番号付きの短い欄見出しと、その直下の本文だけを同じ段へ戻す。
/// 次記事が上段の左側にあるときも、移す範囲は見出し群の横幅の中に限る。
fn rejoin_prose_label_values(tiers: &mut [Vec<Col>], rules: &TierRules) {
    if !rules.sideways {
        return;
    }
    // An unknown separator location cannot establish an unbroken bridge.
    if rules
        .hrules
        .iter()
        .chain(&rules.vrules)
        .any(|&(a, b, c)| ![a, b, c].iter().all(|v| v.is_finite()))
        || rules
            .tables
            .iter()
            .chain(&rules.boxes)
            .any(|&(a, b, c, d)| ![a, b, c, d].iter().all(|v| v.is_finite()))
    {
        return;
    }
    fn ordinal(t: &str) -> Option<usize> {
        let t = t.strip_prefix('（')?.strip_suffix('）')?;
        ["一", "二", "三", "四", "五", "六", "七", "八", "九", "十"]
            .iter()
            .position(|&n| n == t)
            .map(|n| n + 1)
    }
    fn japanese(c: char) -> bool {
        ('\u{3040}'..='\u{30ff}').contains(&c)
            || ('\u{3400}'..='\u{9fff}').contains(&c)
            || c == '々'
    }
    let center = |c: &Col| (c.x + c.x1) / 2.0;
    for i in 1..tiers.len() {
        let Some(j) = (0..i).rev().find(|&j| !tiers[j].is_empty()) else {
            continue;
        };
        let (above, below) = (&tiers[j], &tiers[i]);
        // A transfer needs known source rectangles and distinct occurrences.
        // Unknown geometry or a repeated ID cannot establish membership.
        let mut source_ids = std::collections::HashSet::new();
        if above.iter().chain(below).any(|c| {
            ![c.x, c.x1, c.y, c.y1].iter().all(|v| v.is_finite())
                || c.x >= c.x1
                || c.y >= c.y1
                || !source_ids.insert(c.id)
        }) {
            continue;
        }
        let mut groups: Vec<Vec<&Col>> = Vec::new();
        let mut ordered: Vec<_> = above.iter().collect();
        ordered.sort_by(|a, b| center(b).total_cmp(&center(a)).then(a.y.total_cmp(&b.y)));
        for c in ordered {
            if let Some(g) = groups
                .last_mut()
                .filter(|g| (center(g[0]) - center(c)).abs() <= 1.0)
            {
                g.push(c);
            } else {
                groups.push(vec![c]);
            }
        }
        for g in &mut groups {
            g.sort_by(|a, b| a.y.total_cmp(&b.y));
        }
        let labels: Vec<_> = groups
            .iter()
            .enumerate()
            .filter_map(|(k, g)| {
                let n = ordinal(&g[0].text)?;
                let suffix: String = g.iter().skip(1).map(|c| c.text.as_str()).collect();
                let length = suffix.chars().count();
                if !(2..=24).contains(&length) || !suffix.chars().all(japanese) {
                    return None;
                }
                // 見出しの字同士は、字間を空けた組み方を含めて同じ狭い列にある。
                if g.windows(2)
                    .any(|w| w[1].y < w[0].y1 - 1.0 || w[1].y - w[0].y1 > 16.0)
                {
                    return None;
                }
                Some((
                    k,
                    n,
                    g[0].x,
                    g[0].x1,
                    g[0].y,
                    g.iter().map(|c| c.y1).fold(f32::NEG_INFINITY, f32::max),
                ))
            })
            .collect();
        if labels.len() < 3
            || labels.len() > 10
            || labels
                .windows(2)
                .any(|w| w[1].0 != w[0].0 + 1 || w[1].1 != w[0].1 + 1)
        {
            continue;
        }
        let x0 = labels.iter().map(|l| l.2).fold(f32::INFINITY, f32::min);
        let x1 = labels.iter().map(|l| l.3).fold(f32::NEG_INFINITY, f32::max);
        let top = labels[0].4;
        let bottom = labels[0].5;
        let em = labels.iter().map(|l| l.3 - l.2).fold(0.0, f32::max);
        // Column width is a source measure, not an absolute page-point class.
        // The remaining alignment, spacing and fence checks still apply.
        if !em.is_finite()
            || em <= 0.0
            || labels.iter().any(|l| {
                (l.4 - top).abs() > 1.0
                    || (l.5 - bottom).abs() > 1.0
                    || (l.3 - l.2 - em).abs() > 1.0
            })
        {
            continue;
        }
        let values: Vec<_> = below
            .iter()
            .filter(|c| c.x1 > x0 + 0.5 && c.x < x1 - 0.5)
            .collect();
        if values.len() < labels.len() || values.len() > 24 {
            continue;
        }
        // The band begins at its earliest geometric top, regardless of source
        // vector order. Transferred columns still keep their original order.
        let start = values.iter().map(|c| c.y).fold(f32::INFINITY, f32::min);
        let gap = start - bottom;
        if gap < em * 0.75 - 0.1 || gap > em * 2.0 + 0.1 {
            continue;
        }
        if values.iter().any(|c| {
            c.x < x0 - 1.0
                || c.x1 > x1 + 1.0
                || (c.y - start).abs() > 1.0
                || (c.x1 - c.x - em).abs() > 1.0
                || c.y1 - c.y < em * 0.9
                || c.y1 - c.y > em * 40.0
                || c.text.starts_with(['○', '〇', '◎'])
                || crate::segment::is_section_label(&c.text)
                || ordinal(&c.text).is_some()
        }) {
            continue;
        }
        if !values.iter().any(|c| {
            c.text.chars().count() >= 12 && c.text.chars().any(japanese) && !value_number(&c.text)
        }) {
            continue;
        }
        if labels.iter().any(|l| {
            !values
                .iter()
                .any(|c| (center(c) - (l.2 + l.3) / 2.0).abs() <= 1.0)
        }) {
            continue;
        }
        if values.iter().enumerate().any(|(k, a)| {
            values[k + 1..]
                .iter()
                .any(|b| (center(a) - center(b)).abs() < em * 0.9)
        }) {
            continue;
        }
        // 見出しと値の橋渡しの範囲に原罫・確定した枠があれば、段の境界を保つ。
        if rules
            .hrules
            .iter()
            .any(|&(y, a, b)| y > bottom + 0.5 && y < start - 0.5 && b > x0 && a < x1)
            || rules.vrules.iter().any(|&(x, a, b)| {
                x > x0 + 0.5 && x < x1 - 0.5 && b > bottom + 0.5 && a < start - 0.5
            })
            || rules
                .tables
                .iter()
                .chain(&rules.boxes)
                .any(|&(a, b, c, d)| b > x0 && a < x1 && d > bottom + 0.5 && c < start - 0.5)
        {
            continue;
        }
        let ids: std::collections::HashSet<_> = values.iter().map(|c| c.id).collect();
        let old = std::mem::take(&mut tiers[i]);
        for c in old {
            if ids.contains(&c.id) {
                tiers[j].push(c);
            } else {
                tiers[i].push(c);
            }
        }
    }
}

#[cfg(test)]
#[path = "prose_label_values_tests.rs"]
mod prose_label_values_tests;

#[cfg(test)]
#[path = "shorthand_tier_tests.rs"]
mod shorthand_tier_tests;

#[cfg(test)]
#[path = "triangle_sign_tests.rs"]
mod triangle_sign_tests;

#[cfg(test)]
#[path="aj1_provenance_parser_tests.rs"]
mod aj1_provenance_parser_tests;

#[cfg(test)]
#[path = "source_aj1_parser_tests.rs"]
mod source_aj1_parser_tests;

#[test]
fn original_vertical_advance_metadata_authorizes_only_its_own_complete_frame(){
    let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR"));let inputs=crate::page_inputs(&std::fs::read(root.join("tests/archive/pages/20260803g00174/20260803g001740136.pdf")).unwrap()).unwrap();let input=&inputs[0];let rules=input.rules.clone().unwrap();let mut no_frames=rules.clone();no_frames.enclosures.clear();
    let columns=|xhtml:&str,rules:&PageRules|{let doc=Html::parse_document(xhtml);let sel=Selector::parse("page").unwrap();let page=doc.select(&sel).next().unwrap();let(w,h)=page_size(page);let(cols,_,_,_,_,_,_,_,_,_,_,_)=page_words_with_styles(page,w,h,Some(rules));cols.into_iter().map(|c|(c.id,[c.x.to_bits(),c.y.to_bits(),c.x1.to_bits(),c.y1.to_bits()],c.text)).collect::<Vec<_>>()};
    let valid=columns(&input.xhtml,&rules);assert_ne!(valid,columns(&input.xhtml,&no_frames));
    // Printed source occurrences are fixture oracles; neither text is a runtime condition.
    let change_word=|source:&str,text:&str,key:&str,value:Option<&str>|{
        let needle=format!(">{text}</word>");let start=source.find(&needle).unwrap();let from=source[..start].rfind("<word ").unwrap();let end=start+needle.len();let mut tag=source[from..end].to_string();let attr=format!("{key}=\"");let a=tag.find(&attr).unwrap();let value_start=a+attr.len();let value_end=value_start+tag[value_start..].find('"').unwrap();if let Some(v)=value{tag.replace_range(value_start..value_end,v);}else{tag.replace_range(a..value_end+1,"");}let mut out=source.to_string();out.replace_range(from..end,&tag);out
    };
    for(key,value)in[("data-advance-x",Some("NaN")),("data-advance-x",Some("inf")),("data-advance-x",Some("1")),("data-advance-x",None),("data-advance-y",Some("0")),("data-advance-y",Some("-8")),("data-advance-y",Some("NaN")),("data-wmode",Some("unknown"))]{let x=change_word(&input.xhtml,"せ",key,value);assert_eq!(columns(&x,&rules),columns(&x,&no_frames),"{key} {value:?} cannot authorize a frame");}
    let outside=change_word(&input.xhtml,"農林債の元利金お支払い終了のお知らせ","data-advance-x",Some("NaN"));assert_eq!(columns(&outside,&rules),valid,"an unrelated frame must not inherit or revoke this source advance evidence");
}

#[cfg(test)]
#[path="source_axis_binding_tests.rs"]
mod source_axis_binding_tests;

#[cfg(test)]#[path="source_ruling_binding_tests.rs"]mod source_ruling_binding_tests;

/// Refine an already detected record table only when an isolated numbered
/// section crosses field bands and is followed by distinct typed field labels.
/// No record pitch, caption length or caption value is evidence of ownership.
fn record_section_anchors(bands: &[Vec<Col>], anchors: &[f32]) -> Vec<f32> {
    use std::collections::HashSet;
    let mut ids = HashSet::new();
    if anchors.len() < 2
        || !anchors.iter().all(|v| v.is_finite())
        || !anchors.windows(2).all(|w| w[0] > w[1])
        || !bands.iter().flatten().all(|c| {
            ids.insert(c.id)
                && [c.x, c.x1, c.y, c.y1].iter().all(|v| v.is_finite())
                && c.x < c.x1 && c.y < c.y1
                && (c.x1-c.x).is_finite() && (c.y1-c.y).is_finite()
                && !c.text.trim().is_empty()
                && !c.text.chars().any(|ch| matches!(ch as u32,
                    0xe000..=0xf8ff | 0xf0000..=0xffffd | 0x100000..=0x10fffd))
        })
    {
        return Vec::new();
    }
    let center = |c: &Col| c.x + (c.x1 - c.x) / 2.;
    let numbered = |text: &str| {
        text.strip_prefix('（').and_then(|s| s.split_once('）')).is_some_and(|(n, caption)| {
            !n.is_empty() && !caption.trim().is_empty()
                && n.chars().all(|ch| ch.is_ascii_digit() || ('０'..='９').contains(&ch)
                    || "〇零一二三四五六七八九十百千万億兆".contains(ch))
        })
    };
    let all: Vec<Col> = bands.iter().flatten().cloned().collect();
    let mut result = Vec::new();
    for pair in anchors.windows(2) {
        let mut proposals = Vec::new();
        let mut inside: Vec<_> = all.iter().filter(|c| center(c) < pair[0] && center(c) > pair[1]).cloned().collect();
        inside.sort_by(|a,b| b.x1.total_cmp(&a.x1).then(b.x.total_cmp(&a.x)).then(a.y.total_cmp(&b.y)));
        let mut columns: Vec<Vec<Col>> = Vec::new();
        let mut left = f32::INFINITY;
        for c in inside {
            if columns.is_empty() || c.x1 <= left {
                left = c.x;
                columns.push(vec![c]);
            } else {
                left = left.min(c.x);
                columns.last_mut().unwrap().push(c);
            }
        }
        for mut column in columns {
            column.sort_by(|a,b|a.y.total_cmp(&b.y));
            let lo = column.iter().map(|c| c.x).fold(f32::NEG_INFINITY, f32::max);
            let hi = column.iter().map(|c| c.x1).fold(f32::INFINITY, f32::min);
            // A transitive column chain or two overprinted values is ambiguous.
            if lo >= hi || !column.windows(2).all(|w| w[0].y1 <= w[1].y) {
                continue;
            }
            let text: String = column.iter().map(|c| c.text.as_str()).collect();
            if !numbered(&text) { continue; }
            let section_ids: HashSet<_> = column.iter().map(|c| c.id).collect();
            let x0 = column.iter().map(|c| c.x).fold(f32::INFINITY, f32::min);
            let x1 = column.iter().map(|c| c.x1).fold(f32::NEG_INFINITY, f32::max);
            let y0 = column.first().unwrap().y;
            let y1 = column.last().unwrap().y1;
            // The complete source column is isolated. A word that crosses its
            // side cannot be assigned to either row by its centre alone.
            if all.iter().any(|c| !section_ids.contains(&c.id) && c.x < x1 && c.x1 > x0) {
                continue;
            }
            // A parenthesised field value inside one band is not a section.
            let spanned = bands.iter().filter(|b| b.iter().any(|c| {
                !section_ids.contains(&c.id) && c.y < y1 && c.y1 > y0
            })).count();
            if spanned < 2 { continue; }
            let header: Vec<_> = bands.iter().enumerate().flat_map(|(bi,b)| {
                b.iter().filter(move |c| c.x1 <= x0 && center(c) > pair[1]).map(move |c| (bi,c))
            }).collect();
            // Overprinting inside the proposed header is ambiguous. Geometry
            // in unrelated records (e.g. horizontal digits in vertical text)
            // does not become evidence for or against this local refinement.
            if header.iter().enumerate().any(|(i,(_,a))| header[i+1..].iter().any(|(_,b)| {
                a.x < b.x1 && b.x < a.x1 && a.y < b.y1 && b.y < a.y1
            })) { continue; }
            // Two different field roles in different bands are independent
            // witnesses. Generic trailing 名 alone is not a typed field label.
            let roles: Vec<_> = header.iter().filter_map(|(bi,c)| {
                RECORD_HEADS.iter().filter(|&&role| role != "名")
                    .find(|&&role| c.text.ends_with(role)).map(|role| (*bi,*role))
            }).collect();
            if !roles.iter().any(|a| roles.iter().any(|b| a.0 != b.0 && a.1 != b.1)) {
                continue;
            }
            let header_x = header.iter().map(|(_,c)| center(c)).fold(f32::NEG_INFINITY, f32::max);
            let section_x = lo + (hi - lo) / 2.;
            if !(pair[0] > section_x && section_x > header_x && header_x > pair[1]) { continue; }
            // Existing record ownership must keep every section word together.
            // No source word may straddle the new section/header separator.
            let cut = header.iter().map(|(_,c)| c.x1).fold(f32::NEG_INFINITY,f32::max);
            if cut >= x0 || all.iter().any(|c| c.x < cut && c.x1 > x0) { continue; }
            proposals.push((section_x,header_x));
        }
        // Competing section interpretations within one record gap are held.
        if proposals.len() == 1 { result.extend([proposals[0].0,proposals[0].1]); }
    }
    result
}


#[cfg(test)]
#[path = "record_section_tests.rs"]
mod record_section_tests;

#[cfg(test)]
#[path="enclosed_warichu_tests.rs"]
mod enclosed_warichu_tests;

#[cfg(test)]
mod full_height_scope_tests {
    use super::*;
    use crate::grid::GridCell;
    pub(super) fn scene(outside: bool) -> (Vec<Col>, PageRules, Vec<Grid>) {
        let mut cols = vec![];
        for (y, y1) in [(70., 410.), (432., 780.)] {
            for k in 0..8 {
                let x = 200. + k as f32 * 42.;
                cols.push(Col {
                    id: cols.len(),
                    x,
                    x1: x + 8.,
                    y,
                    y1,
                    text: "任意の欄本文".into(),
                });
            }
        }
        cols.push(Col {
            id: 16,
            x: 151.,
            x1: 159.,
            y: 80.,
            y1: 480.,
            text: "語彙によらない共通欄".into(),
        });
        let mut rules = PageRules::default();
        for x in [150., 160., 550.] {
            rules.vertical.push(crate::lines::VRule {
                x,
                y0: 70.,
                y1: 790.,
            });
        }
        rules.horizontal.push(crate::lines::HRule {
            y: 421.,
            x0: 160.,
            x1: 550.,
        });
        let mut grids = vec![Grid {
            xs: vec![150., 160., 550.],
            ys: vec![70., 421., 790.],
            cells: vec![
                GridCell {
                    x0: 0,
                    x1: 1,
                    y0: 0,
                    y1: 2,
                },
                GridCell {
                    x0: 1,
                    x1: 2,
                    y0: 0,
                    y1: 1,
                },
                GridCell {
                    x0: 1,
                    x1: 2,
                    y0: 1,
                    y1: 2,
                },
            ],
        }];
        if outside {
            cols.push(Col {
                id: 17,
                x: 120.,
                x1: 128.,
                y: 90.,
                y1: 750.,
                text: "独立した枠の本文".into(),
            });
            for x in [20., 140.] {
                rules.vertical.push(crate::lines::VRule {
                    x,
                    y0: 70.,
                    y1: 790.,
                });
            }
            grids.push(Grid {
                xs: vec![20., 140.],
                ys: vec![70., 160., 790.],
                cells: vec![
                    GridCell {
                        x0: 0,
                        x1: 1,
                        y0: 0,
                        y1: 1,
                    },
                    GridCell {
                        x0: 0,
                        x1: 1,
                        y0: 1,
                        y1: 2,
                    },
                ],
            });
        }
        (cols, rules, grids)
    }
    #[test]
    fn frame_owned_crossing_is_not_outside_article_evidence() {
        let (cols, rules, grids) = scene(false);
        let expected: Vec<_> = cols.iter().map(|c| c.id).collect();
        let (table, rest) = split_off_full_height_left(cols, 421., Some(&rules), &grids);
        assert!(rest.is_empty());
        assert_eq!(table.iter().map(|c| c.id).collect::<Vec<_>>(), expected);
    }
    #[test]
    fn a_separate_box_does_not_belong_to_the_comparison_frame() {
        let (cols, rules, grids) = scene(true);
        let (table, rest) = split_off_full_height_left(cols, 421., Some(&rules), &grids);
        assert!(table.iter().any(|c| c.id == 16));
        assert!(!table.iter().any(|c| c.id == 17));
        assert_eq!(rest.iter().map(|c| c.id).collect::<Vec<_>>(), [17]);
    }
}

#[test]
fn a_native_advance_overhang_cannot_authorize_an_outside_article() {
    for scale in [0.125, 1., 8.] {
        let (mut cols, mut rules, mut grids) = full_height_scope_tests::scene(false);
        cols[0].x = 545.;
        cols[0].x1 = 553.;
        for c in &mut cols {
            c.x *= scale;
            c.x1 *= scale;
            c.y *= scale;
            c.y1 *= scale;
        }
        for v in &mut rules.vertical {
            v.x *= scale;
            v.y0 *= scale;
            v.y1 *= scale;
        }
        for h in &mut rules.horizontal {
            h.y *= scale;
            h.x0 *= scale;
            h.x1 *= scale;
        }
        for g in &mut grids {
            for x in &mut g.xs {
                *x *= scale;
            }
            for y in &mut g.ys {
                *y *= scale;
            }
        }
        let (table, rest) = split_off_full_height_left(cols, 421. * scale, Some(&rules), &grids);
        assert!(
            rest.is_empty(),
            "source frame overlap must hold an uncertain assignment, scale={scale}"
        );
        assert_eq!(table.len(), 17);
    }
}

#[path = "ruled_marker_bands.rs"]
mod ruled_marker_bands;
