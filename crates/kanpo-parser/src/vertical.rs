//! 縦書き `pdftotext -bbox-layout` 出力から読み順を復元する。

use scraper::{Html, Selector};

use crate::horizontal::{horizontal_lines, is_left_strip_header, LEFT_STRIP};
use crate::normalize::{is_margin_noise, is_private_use, normalize_char, private_cid_glyph, CidGlyph};
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
    for (pi, page) in doc.select(&page_sel).enumerate() {
        let (width, height) = page_size(page);
        let page_rules: Option<&PageRules> = rules.get(pi).and_then(|r| r.as_ref());
        let (cols, mut words, rotated, sideways) = page_words_traced(page, width, height);
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
                    Unit::Shinkyu {
                        preamble: Vec::new(),
                        after: vec![horizontal_lines(left)],
                        before: vec![horizontal_lines(right)],
                        outside: Vec::new(),
                    }
                }
                None => Unit::Plain(horizontal_lines(cols)),
            });
            let text = units.iter().map(Unit::text).filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n");
            let (doc_text, tables) = doc_text_of(&units, &mut table_count);
            let regions: Vec<_> = units.into_iter().enumerate().flat_map(|(i, u)| u.regions(i)).collect();
            mark_unused(&mut words, &regions);
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
            units.push(reconstruct_shinkyu(cols, f32::MAX, divider, &grids, page_rules));
        } else {
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
            let mut tiers: Vec<Vec<Col>> = tiers.into_iter().flat_map(split_side_by_side_tables).collect();
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
                    // 普通の本文の段でも、罫線の表は表として組む（他の枠を含まない枠で、中の語が全部
                    // この段にあり、中の字が縦組みで、セルに文章の入っていないものだけ。横組みの表は
                    // 行ごとに読むこれまでの組み方のほうがよい）。
                    None => {
                        let ids: std::collections::HashSet<usize> = tier.iter().map(|c| c.id).collect();
                        let inner: Vec<&Grid> = grids
                            .iter()
                            .zip(&grid_words)
                            .filter(|(g, gw)| {
                                g.cells.len() >= 2
                                    && !grids.iter().any(|o| g.encloses(o))
                                    && gw.iter().all(|id| ids.contains(id))
                                    && vertical_inside(&tier, g)
                                    && !prose_cells(&tier, g)
                            })
                            .map(|(g, _)| g)
                            .collect();
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
        let regions: Vec<_> = units.into_iter().enumerate().flat_map(|(i, u)| u.regions(i)).collect();
        mark_unused(&mut words, &regions);
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
fn mark_unused(words: &mut [Word], regions: &[crate::trace::Region]) {
    // 表の行は語をセルの側に持つ。
    let used: std::collections::HashSet<usize> = regions
        .iter()
        .flat_map(|r| r.lines.iter())
        .flat_map(|l| l.words.iter().copied().chain(l.table.iter().flat_map(|t| t.cells.iter().flat_map(|c| c.words.iter().copied()))))
        .collect();
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
fn page_words_traced(page: scraper::ElementRef, page_width: f32, height: f32) -> (Vec<Col>, Vec<Word>, bool, bool) {
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
    // 縦書きのページが /Rotate で横倒しに表示されている（座標上は縦の列が横の行に見える）
    // なら、復元に使う座標を 90° 回して縦書きに戻す。語の表示用の座標（`Word`）は
    // PDF の表示と重なるよう元のまま残す。
    // pdftotext の縦書きの語の bbox は、字面より約 1 文字分（語の幅）上にずれて出る。
    // 原本の画像と突き合わせると、縦書き用の書体（語の幅＝文字の大きさがちょうど整数 pt）
    // の列はほぼ正確に 1em ずれているので 1em 下へ補正する（罫線の座標や PDF の表示と揃える）。
    // 横組み用の書体を回転して縦に置いた語（幅が 8.184pt など 1.023 倍）はずれないので触らない。
    // 1 字の正方形の語は縦横どちらの組みか語だけでは分からないので、ページの向き
    // （縦長の語と横長の語の文字数）で決める。
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
        let tall = h >= em * 1.5 || (vertical_page && t.chars().count() <= 2 && h >= em * 0.9 && (h - h.round()).abs() < 0.01);
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
    for (((x, y, x1, y1, raw), glyph), (gx, gy, gx1, gy1)) in raws.into_iter().zip(glyphs).zip(geom) {
        // scraper が実体参照は復号済み。私用 CID の括弧付き数字等を復元し、約物を正規化し、
        // 私用領域(傍線等のフォント固有グリフ)を除去する。
        let text: String = match glyph {
            Some(g) => g.text.clone(),
            None => raw.chars().filter(|c| !is_private_use(*c)).map(normalize_char).collect(),
        };
        let in_margin = if rotated {
            gx1 <= LEFT_STRIP
        } else {
            gy1 <= TOP_MARGIN || gy >= height - BOTTOM_MARGIN
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
    merge_warichu(&mut cols);
    (cols, words, rotated, sideways)
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
fn merge_warichu(cols: &mut Vec<Col>) {
    let mut i = 0;
    while i < cols.len() {
        let r = cols[i].clone();
        let w = r.x1 - r.x;
        let n = r.text.chars().count();
        if !(4.0..=14.0).contains(&w) || n < 2 || r.y1 - r.y < w * 1.5 {
            i += 1;
            continue;
        }
        let near = |a: f32, b: f32| (a - b).abs() < 0.6;
        let parent = cols
            .iter()
            .enumerate()
            .any(|(j, p)| j != i && ((p.x + p.x1) / 2.0 - r.x).abs() < w * 0.3 && (p.y1 - r.y).abs() < w * 0.75 && p.y < r.y);
        // 左半分: 左端が右半分の 1 字分左で、右半分の範囲に掛かる語。
        let left: Vec<usize> = (0..cols.len())
            .filter(|&j| j != i && near(cols[j].x, r.x - w) && cols[j].y < r.y1 - w * 0.5 && cols[j].y1 > r.y + w * 0.5)
            .collect();
        if !parent || left.is_empty() || left.iter().any(|&j| cols[j].y < r.y - w * 0.5) {
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
                j != i && !left.contains(&j) && ((q.x + q.x1) / 2.0 - r.x).abs() < w * 0.3 && (q.y - bottom).abs() < w * 0.75
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
            text: format!("{}・{}", r.text, left_text),
        };
        let mut removed = 0;
        for &j in left.iter().rev() {
            cols.remove(j);
            if j < i {
                removed += 1;
            }
        }
        i -= removed;
        cols.extend(rest);
        i += 1;
    }
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
fn split_beside_crossing(cols: &[Col], cut: f32) -> Option<(Vec<Col>, Vec<Col>, bool)> {
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
        side.iter().filter(|c| crosses(c)).count() <= MAX_BRIDGE && above.len() >= 2 && below.len() >= 2 && gap >= MIN_TIER_GAP
    };
    // またぐ列が右に寄っている: 左側を段に割れるか。
    let (right, left): (Vec<Col>, Vec<Col>) = cols.iter().cloned().partition(|c| center(c) >= cx_min - 1.0);
    if splits_at_cut(&left) && left.iter().all(|c| c.x1 < cx_min + 1.0) {
        return Some((right, left, true));
    }
    // またぐ列が左に寄っている: 右側を段に割れるか。
    let (right, left): (Vec<Col>, Vec<Col>) = cols.iter().cloned().partition(|c| center(c) > cx_max + 1.0);
    if splits_at_cut(&right) && right.iter().all(|c| c.x > cx_max - 1.0) {
        return Some((right, left, false));
    }
    None
}

/// 跨ぎカラム数の許容上限（前文 1 本程度の橋渡しを段境界として許す）。
const MAX_BRIDGE: usize = 1;

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
    let table_at = |cut: f32| {
        tables.iter().find(|&&(x0, x1, y0, y1)| {
            y0 + 1.0 < cut && cut < y1 - 1.0 && cols.iter().any(|c| x0 < center(c) && center(c) < x1 && y0 < c.y1 && c.y < y1)
        })
    };
    let (xmin, xmax) = (cols.iter().map(|c| c.x).fold(f32::INFINITY, f32::min), cols.iter().map(|c| c.x1).fold(f32::NEG_INFINITY, f32::max));
    let ruled = |cut: f32| {
        rules.hrules.iter().any(|&(y, x0, x1)| (y - cut).abs() < 12.0 && x1.min(xmax) - x0.max(xmin) >= (xmax - xmin) * 0.6)
    };
    // （跨ぐカラム数, cut）。best_free は表を横切らないものの中の最良。
    let mut best: Option<(usize, f32)> = None;
    let mut best_free: Option<(usize, f32)> = None;
    for c in &cols {
        let cut = c.y1 + 0.5;
        if cut < lo || cut > hi || (ruled_only && !ruled(cut)) {
            continue;
        }
        let crossing = cols.iter().filter(|d| d.y < cut && cut < d.y1).count();
        if best.map(|b| crossing < b.0).unwrap_or(true) {
            best = Some((crossing, cut));
        }
        if table_at(cut).is_none() && best_free.map(|b| crossing < b.0).unwrap_or(true) {
            best_free = Some((crossing, cut));
        }
    }
    if best_free.is_some_and(|(crossing, _)| crossing <= MAX_BRIDGE) {
        best = best_free;
    }

    let Some((crossing, cut)) = best else {
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
        if let Some((right, left, crossing_right)) = split_beside_crossing(&cols, cut) {
            if crossing_right {
                out.push(right);
                split_tiers_rec(left, depth + 1, rules, ruled_only, out);
            } else {
                split_tiers_rec(right, depth + 1, rules, ruled_only, out);
                out.push(left);
            }
        } else {
            out.push(cols);
        }
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
    split_tiers_rec(top, depth + 1, rules, ruled_only, out);
    split_tiers_rec(bottom, depth + 1, rules, ruled_only, out);
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
pub(crate) fn page_lines(cols: Vec<Col>) -> Vec<Line> {
    let mut lines: Vec<Line> = Vec::new();
    for g in column_groups(cols) {
        let mut text = String::new();
        let mut prev_end = f32::MIN;
        for c in &g {
            if latin_space(&text, prev_end, c) {
                text.push(' ');
            }
            text.push_str(&c.text);
            prev_end = prev_end.max(c.y1);
        }
        if is_margin_noise(&text) {
            continue;
        }
        lines.push(Line::text(text, g.iter().map(|c| c.id).collect()));
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
