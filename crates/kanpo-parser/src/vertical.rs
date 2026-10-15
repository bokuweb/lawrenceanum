//! 縦書き `pdftotext -bbox-layout` 出力から読み順を復元する。

use scraper::{Html, Selector};

use crate::horizontal::{horizontal_lines, horizontal_lines_with_tables, is_left_strip_header, row_lines, LEFT_STRIP};
use crate::normalize::{is_margin_noise, is_toc_page_number, private_cid_glyph, toc_page_numbers, word_text, CidGlyph};
use crate::shinkyu::{
    band_pair_unit, detect_shinkyu_bands_headerless, detect_shinkyu_header, lines_with_tables_by, reconstruct_shinkyu, split_side_by_side_tables,
    tiers_look_like_bands, ShinkyuLayout,
};
use crate::grid::{find_grids, Grid};
use crate::lines::PageRules;
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
    for (pi, page) in doc.select(&page_sel).enumerate() {
        let (width, height) = page_size(page);
        let page_rules: Option<&PageRules> = rules.get(pi).and_then(|r| r.as_ref());
        let (cols, mut words, rotated, sideways, merged) = page_words_traced(page, width, height);
        // 横倒しの縦書きページは語の座標を回して使うので、回していない罫線の枠は使わない。
        let grids: Vec<Grid> = if sideways {
            Vec::new()
        } else {
            page_rules.map(|r| find_grids(r, width.max(height), width.max(height))).unwrap_or_default()
        };
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
            units.push(match split_horizontal_shinkyu(&cols) {
                // 横書きの新旧対照表: 左の列が改正後、右の列が改正前。
                Some(mid) => {
                    let (left, right): (Vec<Col>, Vec<Col>) = cols.into_iter().partition(|c| c.x1 <= mid);
                    // 欄見出し（「改　正　後」）だけの行は本文ではない。
                    let body = |cols: Vec<Col>| -> Vec<Line> {
                        let mut lines = horizontal_lines(cols);
                        let label = |l: &Line| matches!(l.text.chars().filter(|c| !c.is_whitespace()).collect::<String>().as_str(), "改正後" | "改正前");
                        if lines.first().is_some_and(label) {
                            lines.remove(0);
                        }
                        lines
                    };
                    Unit::Shinkyu {
                        preamble: Vec::new(),
                        after: vec![body(left)],
                        before: vec![body(right)],
                        outside: Vec::new(),
                    }
                }
                None => {
                    let vrules: Vec<(f32, f32, f32)> = page_rules.map(|r| r.vertical.iter().map(|v| (v.x, v.y0, v.y1)).collect()).unwrap_or_default();
                    Unit::Plain(horizontal_lines_with_tables(cols, &grids, &vrules))
                }
            });
            let text = units.iter().map(Unit::text).filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n");
            let (doc_text, tables) = doc_text_of(&units, &mut table_count);
            prev_table = matches!(units.last(), Some(Unit::Shinkyu { .. }));
            let regions: Vec<_> = units.into_iter().enumerate().flat_map(|(i, u)| u.regions(i)).collect();
            mark_unused(&mut words, &regions, &merged);
            pages.push(PageTrace {
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
            .then(|| detect_shinkyu_bands_headerless(&cols, height))
            .flatten();
        if let Some(divider) = headerless {
            layout = "headerless_shinkyu";
            // 表の左に、上下の帯の境目をまたいで紙面の高さいっぱいに組まれた別記事（表の
            // 後ろの附則・次の記事）が同居していれば、表の後ろに本文として出す。
            let (table, rest) = split_off_full_height_left(cols, divider, page_rules);
            units.push(reconstruct_shinkyu(table, f32::MAX, divider, &grids, page_rules));
            if !rest.is_empty() {
                units.push(Unit::Plain(drop_stray_labels(page_lines(rest))));
            }
        } else {
            // 前のページの新旧対照表の続きが、ページの右端の罫線の欄（上下の欄）に収まり、その左に
            // 別の記事が続くページは、段に切る前に欄の中を表の続きとして切り出す（欄が上下の段を
            // またぐので、段に切ると改正前の欄が次の記事の表に入ってしまう）。
            let cols = if prev_table {
                match split_off_leading_frame(cols, page_rules) {
                    Ok((after, before, rest)) => {
                        units.push(band_pair_unit(after, before, &grids, page_rules));
                        rest
                    }
                    Err(cols) => cols,
                }
            } else {
                cols
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
            let plain_tiers = merge_split_header_tiers(split_tiers(cols.clone(), &TierRules { tables: Vec::new(), hrules: hrules.clone() }));
            let shinkyu_page = plain_tiers.iter().any(|t| detect_shinkyu_header(t).is_some())
                || plain_tiers.windows(2).any(|w| tiers_look_like_bands(&w[0], &w[1]));
            let tables: Vec<(f32, f32, f32, f32)> = grids
                .iter()
                .filter(|g| g.cells.len() >= 2 && !grids.iter().any(|o| g.encloses(o)) && vertical_inside(&cols, g) && !prose_cells(&cols, g))
                .map(|g| (g.left(), g.right(), g.top(), g.bottom()))
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
            let tiers = if shinkyu_page || tables.is_empty() || v_chars < h_chars * 2 {
                plain_tiers
            } else {
                merge_split_header_tiers(split_tiers(cols, &TierRules { tables, hrules }))
            };
            let mut tiers: Vec<Vec<Col>> = attach_toc_page_numbers(tiers.into_iter().flat_map(split_side_by_side_tables).collect(), &words);
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
                    // 普通の本文の段でも、罫線の表は表として組む。
                    None => {
                        let inner = inner_grids(&tier);
                        Unit::Plain(drop_stray_labels(lines_with_tables_by(tier, &inner, page_lines)))
                    }
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
        let (doc_text, tables) = doc_text_of(&units, &mut table_count);
        prev_table = matches!(units.last(), Some(Unit::Shinkyu { .. }));
        let regions: Vec<_> = units.into_iter().enumerate().flat_map(|(i, u)| u.regions(i)).collect();
        mark_unused(&mut words, &regions, &merged);
        pages.push(PageTrace {
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
    pages
}

/// 見出しの無い新旧対照表のページで、帯の境目 `divider` をまたぐ長い列（3 列以上）があれば、
/// その右端より左を表の外として分ける（表は境目で上下の帯に分かれるので、境目をまたいで続く
/// 列は表の中には無い）。その右に帯の枠の左辺（境目をまたぐ縦の罫線か、境目の横の罫線の
/// 左端）があれば、そこより左を表の外にする（境目をまたがない附則の見出しなどの短い列も表の外になる）。右に上下の帯が
/// それぞれ残るときだけ分ける。`(表, 表の外)`。
fn split_off_full_height_left(cols: Vec<Col>, divider: f32, rules: Option<&PageRules>) -> (Vec<Col>, Vec<Col>) {
    let mut widths: Vec<f32> = cols.iter().map(|c| c.x1 - c.x).collect();
    widths.sort_by(f32::total_cmp);
    let em = widths.get(widths.len() / 2).copied().unwrap_or(10.0);
    let crossing: Vec<&Col> = cols.iter().filter(|c| c.y < divider - em * 2.0 && c.y1 > divider + em * 2.0).collect();
    if crossing.len() < 3 {
        return (cols, Vec::new());
    }
    let mut boundary = crossing.iter().map(|c| c.x1).fold(f32::NEG_INFINITY, f32::max);
    // 縦の罫線は帯の高さいっぱいのもの（区分見出し「法規的告示」の囲みの罫線は使わない）。
    let top = cols.iter().map(|c| c.y).fold(f32::INFINITY, f32::min);
    let bottom = cols.iter().map(|c| c.y1).fold(f32::NEG_INFINITY, f32::max);
    let verticals = rules
        .into_iter()
        .flat_map(|r| &r.vertical)
        .filter(|v| v.y0 < divider - em * 4.0 && v.y1 > divider + em * 4.0 && v.y1 - v.y0 >= (bottom - top) * 0.6)
        .map(|v| v.x);
    let horizontals = rules
        .into_iter()
        .flat_map(|r| &r.horizontal)
        .filter(|h| (h.y - divider).abs() < em * 3.0 && h.x1 - h.x0 > em * 5.0)
        .map(|h| h.x0);
    if let Some(frame) = verticals.chain(horizontals).filter(|&x| x > boundary).reduce(f32::min) {
        boundary = frame;
    }
    let right = |c: &Col| (c.x + c.x1) / 2.0 > boundary + 1.0;
    let center = |c: &Col| (c.y + c.y1) / 2.0;
    let upper = cols.iter().filter(|c| right(c) && center(c) < divider).count();
    let lower = cols.iter().filter(|c| right(c) && center(c) >= divider).count();
    if upper < 3 || lower < 3 {
        return (cols, Vec::new());
    }
    cols.into_iter().partition(right)
}

/// [`split_off_leading_frame`] の結果（改正後, 改正前, 残り）。
type LeadingFrame = (Vec<Col>, Vec<Col>, Vec<Col>);

/// ページの右端にある、前のページの新旧対照表の続きの欄を切り出す。`(改正後, 改正前, 残り)`。
///
/// 続きの欄は、紙面の高さのほとんどを通る縦の罫線の右にあり、その縦線から右へ伸びる横の罫線で
/// 上下（改正後・改正前）に分かれていて、欄見出しが無い（g00160 0059: 右端の上の欄に改正後の
/// 「５　前項の場合には…」、下の欄に改正前の「〔項を加える。〕」。その左に備考・附則・次の記事が続く）。
/// 罫線の枠では取れない（下の段の次の記事の表の横線が、この欄の縦線まで伸びて 1 つの枠になる）ので、
/// 罫線を直接見る。縦線の左に字が無い（ページ全体が表の続き）なら切り出さない（見出しの無い継続ページとして読む）。
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
    let found = verticals.into_iter().find_map(|v| {
        // 縦線の左端から右へ、右端の字の上まで伸びる横線が上下の欄の境目。
        let h = rules.horizontal.iter().find(|h| (h.x0 - v.x).abs() <= 3.0 && h.x1 >= right_edge - 2.0 && h.y > v.y0 + (v.y1 - v.y0) * 0.25 && h.y < v.y1 - (v.y1 - v.y0) * 0.25)?;
        let right: Vec<&Col> = cols.iter().filter(|c| center(c).0 > v.x && center(c).1 > v.y0 && center(c).1 < v.y1).collect();
        // 境目をまたぐ列は無い（またぐなら上下の欄ではない）。
        if right.iter().any(|c| c.y < h.y - 2.0 && c.y1 > h.y + 2.0) {
            return None;
        }
        let upper = right.iter().filter(|c| center(c).1 < h.y).count();
        let lower = right.len() - upper;
        let label = right.iter().any(|c| matches!(c.text.chars().filter(|ch| !ch.is_whitespace()).collect::<String>().as_str(), "改正後" | "改正前" | "改" | "正" | "後" | "前"));
        let left_of = cols.iter().filter(|c| c.x1 < v.x).count();
        (upper > 0 && lower > 0 && !label && left_of >= 3).then_some((v.x, v.y0, v.y1, h.y))
    });
    let Some((x, y0, y1, mid)) = found else {
        return Err(cols);
    };
    let (mut after, mut before, mut rest) = (Vec::new(), Vec::new(), Vec::new());
    for c in cols {
        let (cx, cy) = center(&c);
        if cx <= x || cy <= y0 || cy >= y1 {
            rest.push(c);
        } else if cy < mid {
            after.push(c);
        } else {
            before.push(c);
        }
    }
    Ok((after, before, rest))
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
        let cols = page_words_traced(page, width, height).0;
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

/// 横書きの新旧対照表（左右 2 列）なら左右の境目の x を返す。
///
/// ページ中央付近に、横組みの語が 1 つもまたがない縦の切れ目があり、左右の同じ高さに
/// 対応する行（同じ文、または改正前欄の「〔同左〕」）が並ぶなら新旧対照表とみなす。
fn split_horizontal_shinkyu(cols: &[Col]) -> Option<f32> {
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
    // 候補は語の右端。またぐ語が無い切れ目のうち中央に最も近いもの。
    let mid = body
        .iter()
        .map(|c| c.x1 + 1.0)
        .filter(|&x| x > lo && x < hi && !body.iter().any(|d| d.x < x && x < d.x1))
        .min_by(|a, b| (a - center).abs().partial_cmp(&(b - center).abs()).unwrap_or(std::cmp::Ordering::Equal))?;
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
    (matched >= 5 && matched * 10 >= r.len() * 3 && (same_left >= 2 || labels)).then_some(mid)
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
/// 戻り値の最後は割注にまとめて消えた語 `(語, まとめた先の語)`（[`merge_warichu`]）。
#[allow(clippy::type_complexity)]
fn page_words_traced(page: scraper::ElementRef, page_width: f32, height: f32) -> (Vec<Col>, Vec<Word>, bool, bool, Vec<(usize, usize)>) {
    let word_sel = Selector::parse("word").unwrap();
    let line_sel = Selector::parse("line").unwrap();
    // (x0, y0, x1, y1, raw)
    let mut raws: Vec<(f32, f32, f32, f32, String)> = Vec::new();
    let mut push = |w: scraper::ElementRef| {
        let v = w.value();
        // scraper は HTML パーサのため属性名を小文字化する（xMin -> xmin）。
        let num = |k: &str| v.attr(k).and_then(|s| s.parse::<f32>().ok());
        if let (Some(x), Some(x1), Some(y), Some(y1)) = (num("xmin"), num("xmax"), num("ymin"), num("ymax")) {
            raws.push((x, y, x1, y1, w.text().collect()));
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
    raws.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then(a.1.total_cmp(&b.1))
            .then(b.2.total_cmp(&a.2))
            .then(a.3.total_cmp(&b.3))
            .then_with(|| a.4.cmp(&b.4))
    });
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
    for (x, y, x1, y1, t) in raws.iter_mut() {
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
        }
    }
    let sideways = is_sideways_vertical(&raws);
    let rotated = !sideways && raws.iter().any(|(_, _, x1, _, t)| is_left_strip_header(*x1, t));
    let geom: Vec<(f32, f32, f32, f32)> = raws
        .iter()
        .map(|&(x, y, x1, y1, _)| {
            if sideways {
                // 元のページの上端 → 表示の左端、右端 → 表示の上端。
                (page_width - y1, x, page_width - y, x1)
            } else {
                (x, y, x1, y1)
            }
        })
        .collect();
    let glyphs: Vec<Option<CidGlyph>> = raws.iter().map(|(x, y, x1, y1, raw)| private_cid_glyph(raw, x1 - x, y1 - y)).collect();
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
        .map(|(i, g)| g.filter(|g| !g.ambiguous || (families.contains(&g.family) && touches_neighbor(i))))
        .collect();

    let mut cols: Vec<Col> = Vec::new();
    let mut words: Vec<Word> = Vec::new();
    for ((((x, y, x1, y1, raw), glyph), (gx, gy, gx1, gy1)), toc_number) in raws.into_iter().zip(glyphs).zip(geom).zip(toc_numbers) {
        // scraper が実体参照は復号済み。私用 CID の括弧付き数字等を復元し、約物を正規化し、
        // 私用領域(傍線等のフォント固有グリフ)を除去する。
        let text: String = match (toc_number, glyph) {
            (Some(n), _) => n,
            (None, Some(g)) => g.text.clone(),
            (None, None) => word_text(&raw),
        };
        let in_margin = if rotated {
            gx1 <= LEFT_STRIP
        } else {
            gy1 <= TOP_MARGIN || gy >= height - BOTTOM_MARGIN || (!sideways && gx >= page_width - RIGHT_MARGIN)
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
    let merged = merge_warichu(&mut cols);
    (cols, words, rotated, sideways, merged)
}

/// 枠のセルに文章（200 字を超える、または 2 文以上）が入っているか。公告の囲み記事などの
/// 枠を表と取り違えないため。
fn prose_cells(cols: &[Col], g: &Grid) -> bool {
    let mut cells: std::collections::HashMap<usize, (usize, usize)> = std::collections::HashMap::new();
    for c in cols {
        if let Some(i) = g.cell_at((c.x + c.x1) / 2.0, (c.y + c.y1) / 2.0) {
            let e = cells.entry(i).or_default();
            e.0 += c.text.chars().count();
            e.1 += c.text.matches('。').count();
        }
    }
    cells.values().any(|&(chars, sentences)| chars > 200 || sentences >= 2)
}

/// 枠の中の語が縦組みか（縦長の語の字数が横長の語の字数以上）。
fn vertical_inside(cols: &[Col], g: &Grid) -> bool {
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
fn merge_warichu(cols: &mut Vec<Col>) -> Vec<(usize, usize)> {
    let mut merged = Vec::new();
    let mut i = 0;
    while i < cols.len() {
        let w = cols[i].x1 - cols[i].x;
        let near = |a: f32, b: f32| (a - b).abs() < 0.6;
        // 右半分が左半分の長さに合わせて字間を空けて組まれ（「大　蔵　省」と「農林水産省」）、
        // 1 字ずつの語になっていることがある。同じ列に 1 em 程度の空きで続く 1 字の語をまとめる。
        // 機関名の字（漢字・かな）だけを対象にする（「1098、1099」の読点などを拾わない）。
        let name_char = |t: &str| {
            let mut cs = t.chars();
            matches!((cs.next(), cs.next()), (Some(c), None) if ('\u{4E00}'..='\u{9FFF}').contains(&c) || ('\u{3041}'..='\u{30FF}').contains(&c))
        };
        // 字間を空けた続きの語（「総」の後の「務」「省、」）。読点の付いた 2 字まで。
        let spaced_part = |t: &str| {
            let cs: Vec<char> = t.chars().collect();
            name_char(t) || (cs.len() == 2 && name_char(&cs[0].to_string()) && matches!(cs[1], '、' | '・' | '，'))
        };
        let mut spaced = vec![i];
        // 右半分の頭が複数字の語（「国家公安委員会、総」）でも、続きが字間を空けた語なら同じようにまとめる。
        let head_ok = name_char(&cols[i].text) || cols[i].y1 - cols[i].y >= w * 1.5;
        if head_ok {
            loop {
                let last = &cols[*spaced.last().unwrap()];
                let next = (0..cols.len()).find(|&j| {
                    !spaced.contains(&j)
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
        // 左半分: 左端が右半分の 1 字分左で、右半分の範囲に掛かる語。
        let left: Vec<usize> = (0..cols.len())
            .filter(|&j| !spaced.contains(&j) && near(cols[j].x, r.x - w) && cols[j].y < r.y1 - w * 0.5 && cols[j].y1 > r.y + w * 0.5)
            .collect();
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
    hrules: Vec<(f32, f32, f32)>,
}

fn split_tiers(cols: Vec<Col>, rules: &TierRules) -> Vec<Vec<Col>> {
    let mut out = Vec::new();
    split_tiers_rec(cols, 0, rules, false, &mut out);
    out
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
        let candidates: Vec<(usize, usize)> = tier
            .iter()
            .enumerate()
            .filter(|&(_, c)| is_number(c))
            // その列の最後の語（下に続く語が無い）。
            .filter(|&(_, c)| !tier.iter().any(|d| d.id != c.id && (center(d) - center(c)).abs() < (c.x1 - c.x) * 0.5 && d.y > c.y))
            .filter_map(|(ci, c)| {
                let em = c.x1 - c.x;
                // すぐ上にある段で、同じ列の語の下端が番号の上にあるもの。
                let ui = (0..tiers.len()).find(|&ui| {
                    let up = &tiers[ui];
                    let bottom = up.iter().map(|d| d.y1).fold(f32::NEG_INFINITY, f32::max);
                    ui != ti
                        && (0.0..em * 2.0).contains(&(c.y - bottom))
                        && up.iter().any(|d| (center(d) - center(c)).abs() < em * 0.5 && d.y1 <= c.y)
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
    out
}

/// 段の境目 `cut` をまたぐ列がすべて片側に寄っていれば、その脇で縦に切って
/// `(右側, 左側, またぐ列が右側にあるか)` を返す。またがない側が `cut` で段に割れる（またぐ列が 1 本以下で、
/// 境目の上下に段間の空きがあり、上下とも複数列ある）ときだけ切る。
///
/// 段の内部（1 段分の高さ）では切らない。短い列の下端を段の境目と取り違えて段を
/// 細切れにしないため。新旧対照表の見出しを含む領域も帯の扱いを崩さないよう切らない。
fn split_beside_crossing(cols: &[Col], cut: f32, spaced: &[(f32, f32, f32)]) -> Option<(Vec<Col>, Vec<Col>, bool)> {
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
            && gap >= MIN_TIER_GAP
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
    let ymin = cols.iter().map(|c| c.y).fold(f32::INFINITY, f32::min);
    let ymax = cols.iter().map(|c| c.y1).fold(f32::NEG_INFINITY, f32::max);
    let margin = (ymax - ymin) * 0.2;
    let (lo, hi) = (ymin + margin, ymax - margin);

    // 中央域で「跨ぐカラム数」が最小の cut を探す。候補はカラムの下端(y1)。この段の語を含む
    // 罫線の表を横切る cut は、表を横切らない cut が無いときだけ使う（セルの間の空きが段の
    // 境目に見えて、表が上下に割れていた）。
    let center = |c: &Col| (c.x + c.x1) / 2.0;
    let upright = |c: &Col| c.y1 - c.y >= (c.x1 - c.x) * 0.9;
    let table_at = |cut: f32| {
        tables.iter().find(|&&(x0, x1, y0, y1)| {
            y0 + 1.0 < cut && cut < y1 - 1.0 && cols.iter().any(|c| x0 < center(c) && center(c) < x1 && y0 < c.y1 && c.y < y1)
        })
    };
    let (xmin, xmax) = (cols.iter().map(|c| c.x).fold(f32::INFINITY, f32::min), cols.iter().map(|c| c.x1).fold(f32::NEG_INFINITY, f32::max));
    let ruled = |cut: f32| {
        rules.hrules.iter().any(|&(y, x0, x1)| (y - cut).abs() < 12.0 && x1.min(xmax) - x0.max(xmin) >= (xmax - xmin) * 0.6)
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
    let touching = |cut: f32| pairs.iter().any(|&(y1, y, gap)| y1 < cut && cut < y && gap < TOUCH_EM);
    // 候補の順位。跨ぐカラム数の少ないものを採る。同数のときは、跨ぐ列の無い cut なら字間を
    // 切らないものの中で上のもの、跨ぐ列のある cut なら右の列の下端（語の正規の順で先に来る列）を
    // 採る（語の入力順に左右されないように）。
    // （跨ぐカラム数, 字間を切るか, cut, 候補を作った列の x, y）。
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
    for c in &cols {
        let cut = c.y1 + 0.5;
        if cut < lo || cut > hi || (ruled_only && !ruled(cut)) {
            continue;
        }
        let crossing = cols.iter().filter(|d| d.y < cut && cut < d.y1).count();
        let cand = (crossing, touching(cut), cut, c.x, c.y);
        if better(&cand, &best) {
            best = Some(cand);
        }
        if table_at(cut).is_none() && better(&cand, &best_free) {
            best_free = Some(cand);
        }
    }
    if best_free.is_some_and(|b| b.0 <= MAX_BRIDGE) {
        best = best_free;
    }

    let Some((crossing, _, cut, _, _)) = best else {
        out.push(cols);
        return;
    };
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
        if let Some((right, left, crossing_right)) = split_beside_crossing(&cols, cut, &spaced_label_gaps(&cols)) {
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
                out.push(right);
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
    // cut で上下に分割（橋渡しカラムは中心 y が属する側へ。ただし上の段の同じ列の語に続く
    // （「第三条」の後の「歯科医師法施行規則…の一部を次のように改正する。」）なら上の段へ）。
    let continues_above = |c: &Col| {
        let w = c.x1 - c.x;
        c.y < cut
            && cut < c.y1
            && cols.iter().any(|d| d.id != c.id && d.y1 <= c.y + 0.5 && c.y - d.y1 < w * SPACE_EM && (center(d) - center(c)).abs() < w * 0.5)
    };
    let upper: std::collections::HashSet<usize> = cols.iter().filter(|c| continues_above(c)).map(|c| c.id).collect();
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

/// `part` の語の右から 3 列までに記事の始まり（「〇…第○号」の標題・「…をここに公布する。」）があるか。
fn starts_article(cols: &[Col], part: &dyn Fn(&Col) -> bool) -> bool {
    column_groups(cols.iter().filter(|c| part(c)).cloned().collect()).iter().take(3).any(|g| {
        let text: String = g.iter().map(|c| c.text.as_str()).collect();
        crate::segment::is_heading_mark(&text) || text.contains("をここに公布する")
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
        // 横線の側にも外にも新旧対照表の欄見出しがある（別々の記事の表が並ぶ）ときだけ。前の表の
        // 末尾の帯の境目の線（欄見出しが無い）などで切らないため。
        .filter(|&&(_, x0, x1)| {
            let inside = |hx: f32| x0 - 5.0 <= hx && hx <= x1 + 5.0;
            dividers.iter().any(|&(hx, _)| inside(hx)) && dividers.iter().any(|&(hx, _)| !inside(hx))
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
    cols.sort_by(|a, b| center(b).partial_cmp(&center(a)).unwrap_or(std::cmp::Ordering::Equal));
    let mut groups: Vec<Vec<Col>> = Vec::new();
    let (mut rep_x, mut rep_em) = (f32::INFINITY, f32::INFINITY);
    for c in cols {
        let em = (c.x1 - c.x).min(rep_em).max(6.0);
        if rep_x - center(&c) > em * 0.75 {
            groups.push(Vec::new());
            rep_x = center(&c);
            rep_em = c.x1 - c.x;
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
///
/// 縦書きの段の中に横組みで置いた欄（号外の目次ページの「官報」の題字の下の「（号　外）」
/// 「発行　内閣府」「（原稿作成　国立印刷局）」など）は、縦の列として読むと語が右から並んで
/// 「外）」「（号」と逆になる。そうした横組みの語は横の行に組み、その欄の右端の位置
/// （右→左の読み順で欄に差し掛かったところ）に置く。
pub(crate) fn page_lines(cols: Vec<Col>) -> Vec<Line> {
    let (cols, blocks) = split_off_horizontal_blocks(cols);
    let mut blocks = blocks.into_iter().peekable();
    let mut lines: Vec<Line> = Vec::new();
    for g in column_groups(cols) {
        let x = g.iter().map(|c| (c.x + c.x1) / 2.0).fold(f32::NEG_INFINITY, f32::max);
        while let Some((_, block)) = blocks.next_if(|(right, _)| *right > x) {
            lines.extend(block);
        }
        let mut text = String::new();
        let mut prev_end = f32::MIN;
        for c in &g {
            if latin_space(&text, prev_end, c) {
                text.push(' ');
            }
            text.push_str(&c.text);
            prev_end = prev_end.max(c.y1);
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

/// 縦書きの段の中の横組みの欄を取り出す。`(残りの語, [(欄の右端, 欄の行)])`（欄は右端の降順）。
///
/// 横組みの語は 2 字以上で横長のもの。縦の列の途中に挟まった語（縦中横の数字など）は除く。
/// 横組みの語の行が 2 行以上（上下に 1.5 行分以内の間隔で続き、左右が重なる）続く欄だけを
/// 取り出す（1 行だけの横長の語は、表の欄の名前などと区別が付かないので列のまま読む）。
fn split_off_horizontal_blocks(cols: Vec<Col>) -> (Vec<Col>, Vec<(f32, Vec<Line>)>) {
    // 縦の列（2 字以上の縦長の語）のすぐ上か下（全角空白 1 字ほどの空きまで。「高橋　佑」）に続く語。
    let in_column = |w: &Col| {
        let em = w.y1 - w.y;
        let near = |gap: f32| -em * 0.5 < gap && gap < em * 1.5;
        cols.iter().any(|c| {
            c.text.chars().count() >= 2
                && c.y1 - c.y > (c.x1 - c.x) * 1.5
                && w.x < (c.x + c.x1) / 2.0
                && (c.x + c.x1) / 2.0 < w.x1
                && (near(w.y - c.y1) || near(c.y - w.y1))
        })
    };
    let wide: Vec<usize> = (0..cols.len())
        .filter(|&i| {
            let c = &cols[i];
            c.text.chars().count() >= 2 && c.x1 - c.x > (c.y1 - c.y) * 1.5 && !in_column(c)
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
            let more: Vec<usize> = (0..cols.len())
                .filter(|&i| !members.contains(&i) && !taken[i] && !tall(&cols[i]) && !in_column(&cols[i]))
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
        // 欄の中に縦の列が入り込んでいれば（横組みの語が縦書きの表の中に散っているだけ）取り出さない。
        let (x0, y0, x1, y1, _) = bounds(&members);
        let intruded = cols.iter().enumerate().any(|(i, c)| {
            let (cx, cy) = center(c);
            !members.contains(&i) && tall(c) && x0 < cx && cx < x1 && y0 < cy && cy < y1
        });
        if intruded {
            continue;
        }
        for &i in &members {
            taken[i] = true;
        }
        blocks.push((x1, members.iter().map(|&i| cols[i].clone()).collect()));
    }
    blocks.sort_by(|a, b| b.0.total_cmp(&a.0));
    let blocks = blocks.into_iter().map(|(x1, block)| (x1, row_lines(block))).collect();
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
        let col = |id: usize, x: f32, y: f32, y1: f32, text: &str| Col { id, x, x1: x + 8.0, y, y1, text: text.to_string() };
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
            col(9, 403.6, 348.8, 508.8, "告示第一号）の一部を次のように改正する。"),
        ];
        merge_warichu(&mut cols);
        let texts: Vec<&str> = cols.iter().map(|c| c.text.as_str()).collect();
        assert!(texts.contains(&"国家公安委員会、総務省、文部科学省、国土交通省"), "{texts:?}");
    }

    #[test]
    fn letter_spaced_horizontal_labels_do_not_make_a_horizontal_page() {
        // 縦書きの本文（縦長の語）の中に、字間を空けた横書きの表の欄の名前が多いページ。
        let col = |x: f32, y: f32, x1: f32, y1: f32, text: &str| Col { id: 0, x, x1, y, y1, text: text.to_string() };
        let mut cols = vec![col(500.0, 60.0, 508.0, 140.0, "縦書きの本文の一行"), col(490.0, 60.0, 498.0, 140.0, "縦書きの本文の二行")];
        for i in 0..4 {
            // 5 字を 0.5em 空けて 1 語にしたもの（幅 56pt、高さ 8pt）。
            cols.push(col(100.0, 200.0 + 10.0 * i as f32, 156.0, 208.0 + 10.0 * i as f32, "利益剰余金"));
        }
        assert!(!is_horizontal_page(&cols));
        // 字間を空けていない横書きの語なら横組みのページ。
        cols.push(col(100.0, 300.0, 260.0, 308.0, "横書きの本文の一行に並ぶ二十字の文字列です"));
        assert!(is_horizontal_page(&cols));
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
