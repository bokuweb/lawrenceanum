//! 縦書き新旧対照表（改正後=上帯 / 改正前=下帯）の検出と 2 カラム分離。
//!
//! 官報の縦書き新旧対照表は縦の罫線(描画)を持たず、改正後/改正前を **y 座標の帯**で
//! 表す。「改正後」(上) と「改正前」(下) のラベルが表の右端に縦に並ぶことを手がかりに、
//! 見出し x(=表の右端) と上下帯の境界 y を求め、本体を 2 帯に分けて復元する。

use crate::grid::{find_grids_inside, Grid};
use crate::lines::PageRules;
use crate::segment::is_heading_mark;
use crate::trace::{Line, Table, TableCell, Unit};
use crate::vertical::{cell_lines, column_groups, page_lines, reconstruct_page, starts_item, Col};

/// 縦書き新旧対照表ページの「改正後／改正前」欄見出し列を検出する。
///
/// 見出しが複数ある（表が横に並ぶ）ときは最も右のものを返す。見つからなければ `None`
/// （＝通常ページ）。
pub(crate) fn detect_shinkyu_header(cols: &[Col]) -> Option<(f32, f32)> {
    detect_shinkyu_headers(cols).into_iter().next()
}

/// 「改正後／改正前」欄見出し列をすべて検出し、右から順に `(見出し x, 上下帯の境界 y)` を返す。
///
/// x 近傍でカラムをまとめ、その縦連結に両ラベルが現れる列を見出し列とみなす。境界は
/// 両ラベルの中心の中点。ラベルは 1〜3 文字の短い語（「改」「正」「後」や「改正後」）に
/// 限る。前文の「改正前欄に掲げる…改正後欄に…」を見出しと取り違えないため。
pub(crate) fn detect_shinkyu_headers(cols: &[Col]) -> Vec<(f32, f32)> {
    // x 近傍(±6pt)でグループ化。
    let mut idx: Vec<usize> = (0..cols.len()).filter(|&i| cols[i].text.chars().count() <= 3).collect();
    idx.sort_by(|&a, &b| cols[a].x.partial_cmp(&cols[b].x).unwrap_or(std::cmp::Ordering::Equal));
    let check = |g: &mut Vec<usize>| -> Option<(f32, f32)> {
        if g.len() < 2 {
            return None;
        }
        g.sort_by(|&a, &b| cols[a].y.partial_cmp(&cols[b].y).unwrap_or(std::cmp::Ordering::Equal));
        let chars: Vec<char> = g.iter().flat_map(|&i| cols[i].text.chars()).collect();
        // 各文字の y 中心（語が複数文字なら全文字に同じ中心を割当: 見出しは短いので十分）。
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
        match (go, mae) {
            (Some(gi), Some(mi)) if gi != mi => {
                let go_c = (centers[gi] + centers[gi + 1] + centers[gi + 2]) / 3.0;
                let mae_c = (centers[mi] + centers[mi + 1] + centers[mi + 2]) / 3.0;
                Some((header_x, (go_c + mae_c) / 2.0))
            }
            _ => None,
        }
    };
    let mut found: Vec<(f32, f32)> = Vec::new();
    let mut group: Vec<usize> = Vec::new();
    let mut rep = f32::NEG_INFINITY;
    for &i in &idx {
        if !group.is_empty() && cols[i].x - rep > 6.0 {
            found.extend(check(&mut group));
            group.clear();
        }
        if group.is_empty() {
            rep = cols[i].x;
        }
        group.push(i);
    }
    found.extend(check(&mut group));
    found.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    found
}

/// 1 つの段に新旧対照表が横に並ぶとき、表ごとの縦の切れ目で分ける（右から順）。
///
/// 官報は右から「前文 A・見出し A・表 A・前文 B・見出し B・表 B …」と組む。表の本体は
/// 改正後帯・改正前帯の中に収まるのに対し、次の前文は帯の境目をまたぐ長い列になるので、
/// 見出し A の左で最初に帯の境目をまたぐ列（または `〇…号` の記事見出しの列）の右端を
/// 切れ目にする。
pub(crate) fn split_side_by_side_tables(cols: Vec<Col>) -> Vec<Vec<Col>> {
    let heads = detect_shinkyu_headers(&cols);
    let mut cuts: Vec<f32> = Vec::new();
    for w in heads.windows(2) {
        let ((h1, d1), (h2, _)) = (w[0], w[1]);
        let cut = cols
            .iter()
            .filter(|c| c.x > h2 + 6.0 && c.x1 < h1 - 1.0 && ((c.y < d1 && c.y1 > d1) || c.text.starts_with('〇')))
            .map(|c| c.x1)
            .reduce(f32::max);
        if let Some(cut) = cut {
            cuts.push(cut + 1.0);
        }
    }
    if cuts.is_empty() {
        return vec![cols];
    }
    let mut slices: Vec<Vec<Col>> = vec![Vec::new(); cuts.len() + 1];
    for c in cols {
        let k = cuts.iter().filter(|&&cut| c.x < cut).count();
        slices[k].push(c);
    }
    slices.retain(|s| !s.is_empty());
    slices
}

/// 見出しの無い継続ページで、上下2帯（改正後/改正前）構造を幾何＋内容類似から検出する。
///
/// 多ページ新旧対照表の 2 ページ目以降は「改正後/改正前」の欄見出しを繰り返さないため、
/// [`detect_shinkyu_header`] では拾えない。代わりに次の 3 条件で継続ページを判定する:
///   1. ページ中央付近に列がほとんど跨がない横線（谷）があり、そこを境界 y とできる
///   2. 上帯と下帯の高さがほぼ等しい（表は上下対称）
///   3. 上帯と下帯の本文がほぼ同一（改正後 ≒ 改正前。bigram Jaccard で判定）
///
/// 人事異動一覧のような「上下で別内容」の 2 段組ページは (3) で弾かれる。
/// 条件を満たせば境界 y を返す。
pub(crate) fn detect_shinkyu_bands_headerless(cols: &[Col], height: f32) -> Option<f32> {
    if cols.len() < 8 {
        return None;
    }
    let center = |c: &Col| (c.y + c.y1) / 2.0;
    // 中央域 [0.35H, 0.65H] で跨ぎ最小の cut（候補は各列の下端）。
    let (lo, hi) = (0.35 * height, 0.65 * height);
    let mut best: Option<(usize, f32)> = None;
    for c in cols {
        let cut = c.y1 + 0.5;
        if cut < lo || cut > hi {
            continue;
        }
        let crossing = cols.iter().filter(|d| d.y < cut && cut < d.y1).count();
        // またぐ列が同じ数なら、ページ中央（上下の帯の境目）に近い方を採る。
        let closer = |b: (usize, f32)| crossing == b.0 && (cut - height / 2.0).abs() < (b.1 - height / 2.0).abs();
        if best.map(|b| crossing < b.0 || closer(b)).unwrap_or(true) {
            best = Some((crossing, cut));
        }
    }
    let (_, divider) = best?;
    let upper: Vec<Col> = cols.iter().filter(|c| center(c) < divider).cloned().collect();
    let lower: Vec<Col> = cols.iter().filter(|c| center(c) >= divider).cloned().collect();
    if upper.len() < 3 || lower.len() < 3 {
        return None;
    }
    // 上下帯の高さ対称性（表は改正後欄の高さ ≒ 改正前欄の高さ）。
    let band_height = |g: &[Col]| {
        let top = g.iter().map(|c| c.y).fold(f32::INFINITY, f32::min);
        let bot = g.iter().map(|c| c.y1).fold(f32::NEG_INFINITY, f32::max);
        bot - top
    };
    let (uh, lh) = (band_height(&upper), band_height(&lower));
    if uh.min(lh) / uh.max(lh) < 0.7 {
        return None;
    }
    // 内容類似（改正後 ≒ 改正前）。新旧対照表は両欄がほぼ同一テキストで差分のみ異なる。
    // 人事異動一覧のように上下で別内容のページはここで弾かれる。
    // 上下の本文が順序も含めてほぼ同じであること（新旧対照表は改正後 ≒ 改正前）。文字の
    // 集合だけ比べると、似た法令文が続く普通の 2 段組み（政令本文など）も似て見えるので、
    // 最長共通部分列の長さの比で比べる。
    let ut = reconstruct_page(upper);
    let lt = reconstruct_page(lower);
    if bigram_jaccard(&ut, &lt) < 0.5 || sequence_ratio(&ut, &lt) < 0.5 {
        return None;
    }
    Some(divider)
}

/// 見出しの無い隣り合う 2 段（上の段 `a`・下の段 `b`）が、新旧対照表の改正後帯・改正前帯の
/// 組に見えるか。継続ページが 4 段（改正後・改正前・改正後・改正前）に組まれる場合に、段の組
/// ごとに判定する。高さと左右の範囲がほぼ揃い、本文が順序も含めてほぼ同じで、省略・新設の印が
/// あることを求める。
pub(crate) fn tiers_look_like_bands(a: &[Col], b: &[Col]) -> bool {
    if a.len() < 3 || b.len() < 3 {
        return false;
    }
    let ext = |g: &[Col]| {
        (
            g.iter().map(|c| c.x).fold(f32::INFINITY, f32::min),
            g.iter().map(|c| c.x1).fold(f32::NEG_INFINITY, f32::max),
            g.iter().map(|c| c.y).fold(f32::INFINITY, f32::min),
            g.iter().map(|c| c.y1).fold(f32::NEG_INFINITY, f32::max),
        )
    };
    let ((ax0, ax1, ay0, ay1), (bx0, bx1, by0, by1)) = (ext(a), ext(b));
    let (ha, hb) = (ay1 - ay0, by1 - by0);
    let overlap = (ax1.min(bx1) - ax0.max(bx0)).max(0.0) / (ax1 - ax0).max(bx1 - bx0).max(1.0);
    if ha.min(hb) / ha.max(hb) < 0.7 || overlap < 0.7 {
        return false;
    }
    let (at, bt) = (reconstruct_page(a.to_vec()), reconstruct_page(b.to_vec()));
    // 新旧対照表に特有の省略・新設の印があること（同じひな形の告示が上下の段に並ぶページ、
    // たとえば灯台や道路の告示の続きを取り違えないため）。
    let marked = has_shinkyu_marks(&at) || has_shinkyu_marks(&bt);
    marked && bigram_jaccard(&at, &bt) >= 0.5 && sequence_ratio(&at, &bt) >= 0.5
}

/// 新旧対照表に特有の省略・新設の印（「（略）」「〔九〜一六　略〕」「〔同上〕」「［同左］」
/// 「（新設）」「（削る）」）があるか。
fn has_shinkyu_marks(t: &str) -> bool {
    const MARKS: [&str; 13] = [
        "略）", "略〕", "略］", "同上）", "同上〕", "同左］", "同左）", "新設）", "新設〕", "新設］", "削る）", "削る〕",
        // 表の注記（「備考　表中の〔　〕の記載は注記である。」）。
        "記載は注記である",
    ];
    MARKS.iter().any(|m| t.contains(m))
}

/// [`tiers_look_like_bands`] の 2 段を改正後・改正前として組む。
pub(crate) fn band_pair_unit(after: Vec<Col>, before: Vec<Col>, grids: &[Grid], rules: Option<&PageRules>) -> Unit {
    ShinkyuLayout {
        band_boxes: None,
        preamble: Vec::new(),
        after: vec![after],
        before: vec![before],
        trailing: Vec::new(),
    }
    .into_unit(grids, rules)
}

/// 2 つの文の、順序を考慮した近さ（2 × 最長共通部分列の長さ ÷ 両方の長さの和、0.0–1.0）。
/// 長い文は先頭 2000 字で比べる。
fn sequence_ratio(a: &str, b: &str) -> f32 {
    let a: Vec<char> = a.chars().filter(|c| !c.is_whitespace()).take(2000).collect();
    let b: Vec<char> = b.chars().filter(|c| !c.is_whitespace()).take(2000).collect();
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let mut prev = vec![0u32; b.len() + 1];
    let mut cur = vec![0u32; b.len() + 1];
    for &ca in &a {
        for (j, &cb) in b.iter().enumerate() {
            cur[j + 1] = if ca == cb { prev[j] + 1 } else { prev[j + 1].max(cur[j]) };
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    2.0 * prev[b.len()] as f32 / (a.len() + b.len()) as f32
}

/// 2 文字 n-gram 集合の Jaccard 係数（0.0–1.0）。改正後/改正前の本文類似度に使う。
pub(crate) fn bigram_jaccard(a: &str, b: &str) -> f32 {
    use std::collections::HashSet;
    let bigrams = |s: &str| -> HashSet<(char, char)> {
        let chars: Vec<char> = s.chars().filter(|c| !c.is_whitespace()).collect();
        chars.windows(2).map(|w| (w[0], w[1])).collect()
    };
    let (sa, sb) = (bigrams(a), bigrams(b));
    if sa.is_empty() && sb.is_empty() {
        return 0.0;
    }
    let inter = sa.intersection(&sb).count() as f32;
    let union = sa.union(&sb).count() as f32;
    inter / union
}

/// `chars` 中で連続部分列 `pat` が最初に現れる開始位置を返す。
fn find_run(chars: &[char], pat: &[char]) -> Option<usize> {
    if pat.is_empty() || chars.len() < pat.len() {
        return None;
    }
    (0..=chars.len() - pat.len()).find(|&i| &chars[i..i + pat.len()] == pat)
}

/// 新旧対照表の段を改正後(上帯)/改正前(下帯)に分けて復元する。
/// 見出しの無い継続ページ用: ページ全体が表なので、表の下の別記事を切り離す判定はしない。
pub(crate) fn reconstruct_shinkyu(cols: Vec<Col>, header_x: f32, divider: f32, grids: &[Grid], rules: Option<&PageRules>) -> Unit {
    let mut layout = ShinkyuLayout::split(cols, header_x, divider);
    let outside: Vec<Col> = layout.trailing.drain(..).flatten().collect();
    if let Some(before) = layout.before.first_mut() {
        before.extend(outside);
    }
    layout.into_unit(grids, rules)
}

/// 新旧対照表 1 件分のカラム配置。
///
/// 帯ごとのカラム群をページ内の段ごとに分けて持つ（同じ x に別の段の列があるため、
/// 1 つの Vec に混ぜると右→左復元で列が連結されてしまう）。
pub(crate) struct ShinkyuLayout {
    /// 罫線の外枠から求めた改正後帯・改正前帯の矩形 `(x0, x1, y0, y1)`（見出し段のみ）。
    band_boxes: Option<[(f32, f32, f32, f32); 2]>,
    preamble: Vec<Col>,
    after: Vec<Vec<Col>>,
    before: Vec<Vec<Col>>,
    /// 表の外にある列（表の下・左に同居する別記事）。表の後ろに読み順で出す。
    trailing: Vec<Vec<Col>>,
}

impl ShinkyuLayout {
    /// 見出し列 x より右(=`x >= header_x`)は前文・標題とみなし先頭にそのまま置く。
    /// 表本体(`x < header_x`)を境界 y で上下に分ける。
    pub(crate) fn split(cols: Vec<Col>, header_x: f32, divider: f32) -> Self {
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
        // (`2*divider - table_top`) を下端として下帯を制限し、はみ出た列は表外として
        // 表の後ろへ回す。表が紙面全体を占める通常ケースでは下端がページ外になり実質無効。
        let (lower, outside): (Vec<Col>, Vec<Col>) = match upper.iter().map(|c| c.y).reduce(f32::min) {
            Some(table_top) => {
                // 帯の高さの差や行末の短い列のはみ出しを見込んで 20pt の余裕を持たせる。
                let table_bottom = 2.0 * divider - table_top + 20.0;
                lower_all.into_iter().partition(|c| (c.y + c.y1) / 2.0 < table_bottom)
            }
            None => (lower_all, Vec::new()),
        };
        ShinkyuLayout {
            band_boxes: None,
            preamble,
            after: vec![upper],
            before: vec![lower],
            trailing: vec![outside],
        }
    }

    /// 前文の右側に、前ページから続く別の表の末尾が同居していれば切り出す。
    ///
    /// 前の項目の新旧対照表がページ上部の右端で終わり、そのすぐ左から次の項目（前文＋
    /// 新しい表）が始まるページがある。前文は両帯を跨ぐ長い列になるのに対し、前の表の
    /// 末尾は改正後帯・改正前帯それぞれに収まる短い列になる。両帯を跨ぐ前文の列より右に
    /// あって帯の中に収まる列を前の表の続きとし、上帯を改正後・下帯を改正前とする
    /// （両帯にそれぞれ列があるときだけ）。区分見出し（「法　規　的　告　示」）のような
    /// 大きい字は帯の境目で割れても前の表ではないので、前文と同じ字の大きさの列に限る。
    ///
    /// 帯の中の次の記事の標題（「〇」で始まる行）から左は前文に残す。表の末尾が罫線で
    /// 囲まれていれば、枠の外の列（表の後ろの備考・附則）は表の後ろに本文として出す。
    /// 表の末尾らしくなければ（2 段に組んだ前の記事の続きなど）、上下の段を混ぜて読まない
    /// よう、段ごとに組んで前文より先に出す。
    pub(crate) fn split_off_leading_continuation(&mut self, divider: f32, grids: &[Grid], rules: Option<&PageRules>) -> Vec<Unit> {
        let spans = |c: &Col| c.y < divider && c.y1 > divider;
        let Some(span_right) = self.preamble.iter().filter(|c| spans(c)).map(|c| c.x1).reduce(f32::max) else {
            return Vec::new();
        };
        let em = self.preamble.iter().filter(|c| spans(c)).map(|c| c.x1 - c.x).fold(f32::INFINITY, f32::min);
        // 字の大きさは幅と高さの小さい方で比べる（縦中横の「(120)」は幅だけ広い）。
        let glyph = |c: &Col| (c.x1 - c.x).min(c.y1 - c.y);
        // 表の帯（罫線の外枠があればその上端）より上（段の上の方の前文）は末尾ではない。
        let band_top = match self.band_boxes {
            Some([(_, _, top, _), _]) => top,
            None => self.after.iter().flatten().map(|c| c.y).fold(f32::INFINITY, f32::min),
        };
        let is_tail = |c: &Col| c.x > span_right + 2.0 && !spans(c) && glyph(c) <= em * 1.3 && c.y1 > band_top - em;
        let (tail, rest): (Vec<Col>, Vec<Col>) = std::mem::take(&mut self.preamble).into_iter().partition(is_tail);
        let (upper, lower): (Vec<Col>, Vec<Col>) = tail.into_iter().partition(|c| c.y1 <= divider);
        let mut rest = rest;
        // 標題の列から左（次の記事）を切る。連名の標題（「内閣府、総務省、／〇財務省、…令第七号」）
        // は「、」で終わる右隣の行も標題に含める。
        let mut cut_at_heading = |part: Vec<Col>| -> Vec<Col> {
            // 行の右端・下端。行は同じ x の離れた語（帯の下の方の表の末尾）も含むので、上から
            // 続く分だけ見る。
            let extent = |l: &Line| {
                let mut cols: Vec<&Col> = part.iter().filter(|c| l.words.contains(&c.id)).collect();
                cols.sort_by(|a, b| a.y.total_cmp(&b.y));
                let mut ext = (f32::NEG_INFINITY, f32::NEG_INFINITY);
                for c in cols {
                    if ext.1 > f32::NEG_INFINITY && c.y > ext.1 + em * 2.0 {
                        break;
                    }
                    ext = (ext.0.max(c.x1), ext.1.max(c.y1));
                }
                ext
            };
            let mut lines: Vec<(Line, (f32, f32))> = page_lines(part.clone()).into_iter().map(|l| { let e = extent(&l); (l, e) }).collect();
            lines.sort_by(|a, b| b.1 .0.total_cmp(&a.1 .0));
            let heading = lines.iter().position(|(l, _)| is_heading_mark(&l.text)).map(|h| {
                let mut ext = lines[h].1;
                for (l, e) in lines[..h].iter().rev() {
                    if !l.text.ends_with('、') || e.0 > ext.0 + em * 2.0 {
                        break;
                    }
                    ext = (e.0, ext.1.max(e.1));
                }
                ext
            });
            let next_article = |c: &Col| heading.is_some_and(|(right, _)| (c.x + c.x1) / 2.0 <= right);
            let (next, cont): (Vec<Col>, Vec<Col>) = part.into_iter().partition(next_article);
            rest.extend(next);
            cont
        };
        let (upper, lower) = (cut_at_heading(upper), cut_at_heading(lower));
        // 枠は末尾の右端を含むもの（末尾の中の入れ子の表の枠は使わない）で、上下の帯ごとに
        // その帯にかかる枠だけを使う（片方の帯にしか枠が見つからないことがある）。
        let tail_right = upper.iter().chain(&lower).map(|c| c.x1).fold(f32::NEG_INFINITY, f32::max);
        let frame_left = |is_upper: bool| {
            grids
                .iter()
                .filter(|g| g.left() < tail_right && g.right() >= tail_right - 6.0 && g.left() > span_right - 2.0)
                .filter(|g| if is_upper { g.top() < divider - 6.0 } else { g.bottom() > divider + 6.0 })
                .map(|g| g.left())
                .reduce(f32::min)
        };
        let split_frame = |part: Vec<Col>, left: Option<f32>| -> (Vec<Col>, Vec<Col>) {
            part.into_iter().partition(|c| left.is_none_or(|l| (c.x + c.x1) / 2.0 > l))
        };
        let (after, out_after) = split_frame(upper, frame_left(true));
        let (before, out_before) = split_frame(lower, frame_left(false));
        // 表の末尾なら上下（改正後・改正前）の本文が似ているか、省略・新設の印がある。2 段に組んだ
        // 普通の改め文（「様式第○中…に改める」「附則」）を表の末尾と取り違えないため。
        let looks_like_table = || {
            let (at, bt) = (reconstruct_page(after.clone()), reconstruct_page(before.clone()));
            // 表の末尾は短く改正部分の差も大きいので、段の組の判定より緩めにする（実測: 表の末尾
            // 0.41/0.49、2 段組みの改め文 0.18/0.42）。
            // 罫線があれば、上下とも罫線の枠の中に収まっていることも表の証拠にする（上下で中身の
            // 違う長い一覧の表の末尾など）。
            let framed = |part: &[Col]| {
                let inside = part.iter().filter(|c| grids.iter().any(|g| g.contains((c.x + c.x1) / 2.0, (c.y + c.y1) / 2.0, 0.5))).count();
                inside * 10 >= part.len() * 8
            };
            has_shinkyu_marks(&at)
                || has_shinkyu_marks(&bt)
                || (bigram_jaccard(&at, &bt) >= 0.35 && sequence_ratio(&at, &bt) >= 0.4)
                || (!grids.is_empty() && framed(&after) && framed(&before))
        };
        let one_sided = after.is_empty() || before.is_empty();
        // 片方の帯だけなら表の末尾とは言えないが、もう片方の帯に何も無い（あっても次の記事だけ）
        // なら、残った帯は前の記事の続き。前文に混ぜると同じ x の前文の行と 1 行に組まれる。
        let other_empty = (after.is_empty() && out_after.is_empty()) || (before.is_empty() && out_before.is_empty());
        if one_sided && !other_empty {
            self.preamble = rest.into_iter().chain(after).chain(out_after).chain(before).chain(out_before).collect();
            return Vec::new();
        }
        self.preamble = rest;
        let outside: Vec<Line> = cell_lines(out_after).into_iter().chain(cell_lines(out_before)).collect();
        let mut units = Vec::new();
        if one_sided || !looks_like_table() {
            let lines: Vec<Line> = cell_lines(after).into_iter().chain(cell_lines(before)).chain(outside).collect();
            if !lines.is_empty() {
                units.push(Unit::Plain(lines));
            }
            return units;
        }
        units.push(
            ShinkyuLayout {
                band_boxes: None,
                preamble: Vec::new(),
                after: vec![after],
                before: vec![before],
                trailing: Vec::new(),
            }
            .into_unit(grids, rules),
        );
        if !outside.is_empty() {
            units.push(Unit::Plain(outside));
        }
        units
    }

    /// 見出し段における改正後帯→改正前帯の y ピッチ。
    ///
    /// 新旧対照表は改正後/改正前の対応する行を同じ x に並べるので、両帯で x が一致する
    /// 列頭どうしの y 差の最頻値が帯のピッチになる。下の段へ続く表の判定に使う。
    fn band_pitch(&self) -> Option<f32> {
        let (a, b) = (self.after.first()?, self.before.first()?);
        let (ha, hb) = (column_heads(a), column_heads(b));
        let dys: Vec<f32> = hb
            .iter()
            .filter_map(|h| ha.iter().find(|g| (g.x - h.x).abs() <= 1.0).map(|g| h.y - g.y))
            .collect();
        // 許容 1.5pt で最多の y 差を採る（同数なら先勝ち）。
        let best = dys
            .iter()
            .map(|&d| {
                let near: Vec<f32> = dys.iter().copied().filter(|e| (e - d).abs() <= 1.5).collect();
                (near.len(), near.iter().sum::<f32>() / near.len() as f32)
            })
            .max_by_key(|(n, _)| *n)?;
        (best.0 >= 2).then_some(best.1)
    }

    /// 見出し段の下に続く段の組 `(a, b)` が同じ表の続き（a=改正後, b=改正前）なら取り込む。
    ///
    /// 官報は表の欄見出しを同じページの下の段では繰り返さないため、見出しの無い段の組を
    /// 次の条件で表の続きと判定する:
    ///   1. b の列頭に、x が一致し y 差が見出し段の帯ピッチに等しい a の列頭がある
    ///      （改正後/改正前の対応行が同じ x に並ぶ）。一致が 3 組以上。
    ///   2. 両段の最右列どうしが一致する（表は段の右端から始まる続き）。
    ///   3. a の最右列が記事見出し（`〇…`）で始まらない。
    ///   4. 表の範囲（一致した最左の x より右）にある b の列頭の 6 割以上が一致する。
    ///
    /// 段落の通常の段送りは段ピッチ（表の帯ピッチとは独立）で並ぶため 1. で弾かれる。
    /// 表の範囲より左の列は別記事として `trailing` に回す。取り込んだ段の組に表外の列が
    /// あれば表はそこで終わりとみなし `false`（続けて次の段の組を見ない）を返す。
    pub(crate) fn absorb_continuation(&mut self, a: &mut Vec<Col>, b: &mut Vec<Col>) -> Option<bool> {
        let pitch = self.band_pitch()?;
        let (ha, hb) = (column_heads(a), column_heads(b));
        let (ra, rb) = (ha.first()?, hb.first()?);
        if (ra.x - rb.x).abs() > 1.0 || ra.lead.starts_with('〇') {
            return None;
        }
        let matched: Vec<f32> = hb
            .iter()
            .filter(|h| ha.iter().any(|g| (g.x - h.x).abs() <= 1.0 && (h.y - g.y - pitch).abs() <= 1.5))
            .map(|h| h.x)
            .collect();
        if matched.len() < 3 || matched.first() != Some(&rb.x) {
            return None;
        }
        // 列頭の x は列間隔（≒12pt）の半分の余裕を見て表の左端とする。
        let left = matched.iter().copied().fold(f32::INFINITY, f32::min) - 6.0;
        let in_table = hb.iter().filter(|h| h.x >= left).count();
        if (matched.len() as f32) < in_table as f32 * 0.6 {
            return None;
        }
        let (a_tab, a_rest): (Vec<Col>, Vec<Col>) = std::mem::take(a).into_iter().partition(|c| c.x >= left);
        let (b_tab, b_rest): (Vec<Col>, Vec<Col>) = std::mem::take(b).into_iter().partition(|c| c.x >= left);
        let ends_here = !a_rest.is_empty() || !b_rest.is_empty();
        self.after.push(a_tab);
        self.before.push(b_tab);
        self.trailing.push(a_rest);
        self.trailing.push(b_rest);
        Some(!ends_here)
    }

    /// 新旧対照表の外枠（欄見出しを含み、帯の境目をまたぐ罫線の枠）が取れれば、枠の左の外に
    /// ある列（表の後ろの「附則」など）を改正後/改正前の帯から外して表外へ回す。
    pub(crate) fn clip_to_frame(&mut self, grids: &[Grid], header_x: f32, divider: f32) {
        let frame = grids
            .iter()
            .filter(|g| g.left() <= header_x && header_x <= g.right() + 1.0 && g.top() < divider && divider < g.bottom())
            .min_by(|a, b| {
                let area = |g: &Grid| (g.right() - g.left()) * (g.bottom() - g.top());
                area(a).partial_cmp(&area(b)).unwrap_or(std::cmp::Ordering::Equal)
            });
        let Some(frame) = frame else { return };
        let left = frame.left();
        // 帯の矩形: 左右は枠の左端から欄見出しの列の左の罫線まで、上下は枠の上端・帯の境目・下端。
        let label_left = frame.xs.iter().copied().filter(|&x| x < header_x - 1.0).fold(left, f32::max);
        let mid = frame.ys.iter().copied().min_by(|a, b| (a - divider).abs().partial_cmp(&(b - divider).abs()).unwrap()).unwrap_or(divider);
        self.band_boxes = Some([(left, label_left, frame.top(), mid), (left, label_left, mid, frame.bottom())]);
        let mut out: Vec<Col> = Vec::new();
        for band in [&mut self.after, &mut self.before] {
            if let Some(part) = band.first_mut() {
                let (inside, outside): (Vec<Col>, Vec<Col>) = std::mem::take(part).into_iter().partition(|c| (c.x + c.x1) / 2.0 > left);
                *part = inside;
                out.extend(outside);
            }
        }
        if !out.is_empty() {
            self.trailing.insert(0, out);
        }
    }

    /// 帯ごとに行へ組む（テキスト化は [`Unit::text`]）。帯の中の罫線の表は表として組む。
    pub(crate) fn into_unit(self, grids: &[Grid], rules: Option<&PageRules>) -> Unit {
        // 見出し段の帯は、帯の内側の罫線だけで表を探す（表の罫線が帯の枠に接していても拾える）。
        // それ以外（下の段への続き・見出しの無い継続ページ）は、ページの枠のうち他の枠を含まない
        // ものを表の候補にする。
        // 帯の矩形: 見出し段は外枠から、それ以外（下の段への続き・継続ページ・段の組）は
        // 帯の語の広がりから求める。
        let word_box = |part: &[Col]| -> Option<(f32, f32, f32, f32)> {
            if part.is_empty() {
                return None;
            }
            let pad = 4.0;
            Some((
                part.iter().map(|c| c.x).fold(f32::INFINITY, f32::min) - pad,
                part.iter().map(|c| c.x1).fold(f32::NEG_INFINITY, f32::max) + pad,
                part.iter().map(|c| c.y).fold(f32::INFINITY, f32::min) - pad,
                part.iter().map(|c| c.y1).fold(f32::NEG_INFINITY, f32::max) + pad,
            ))
        };
        // 語の広がりの上下を、そのすぐ外側にある帯の幅いっぱいの横線まで広げる（帯の中の欄が
        // 空のとき、欄の境目の線が語の端と重なって帯の縁とみなされないように）。
        let snap = |(x0, x1, y0, y1): (f32, f32, f32, f32)| -> (f32, f32, f32, f32) {
            let Some(r) = rules else { return (x0, x1, y0, y1) };
            let full_at = |y: f32| r.horizontal.iter().filter(|h| (h.y - y).abs() < 1.0).map(|h| h.x1.min(x1) - h.x0.max(x0)).filter(|w| *w > 0.0).sum::<f32>() >= (x1 - x0) * 0.8;
            let ys: Vec<f32> = r.horizontal.iter().map(|h| h.y).filter(|&y| full_at(y)).collect();
            let top = ys.iter().copied().filter(|&y| y <= y0 + 6.0).fold(y0, |a, y| if a == y0 || y > a { y } else { a });
            let bottom = ys.iter().copied().filter(|&y| y >= y1 + 6.0).fold(y1, |a, y| if a == y1 || y < a { y } else { a });
            (x0, x1, top.min(y0), bottom.max(y1))
        };
        let mut boxes: Vec<(f32, f32, f32, f32)> = Vec::new();
        for (k, band) in [&self.after, &self.before].into_iter().enumerate() {
            for (i, part) in band.iter().enumerate() {
                let b = match (i, self.band_boxes) {
                    (0, Some(bb)) => Some(bb[k]),
                    _ => word_box(part).map(snap),
                };
                boxes.extend(b);
            }
        }
        let band_grids: Vec<Grid> = match rules {
            Some(r) => boxes
                .iter()
                .flat_map(|&(x0, x1, y0, y1)| find_grids_inside(r, x0, x1, y0, y1))
                .filter(|g| g.cells.len() >= 2)
                .collect(),
            None => Vec::new(),
        };
        // 帯の内側で表が見つかった帯では、ページ全体の枠からの候補は使わない（同じ表の二重取り）。
        let in_boxes = |g: &Grid| {
            band_grids.iter().any(|b| b.left() - 6.0 <= g.left() && g.right() <= b.right() + 6.0 && b.top() - 6.0 <= g.top() && g.bottom() <= b.bottom() + 6.0)
                || self.band_boxes.is_some_and(|b| b.iter().any(|&(x0, x1, y0, y1)| x0 - 1.0 <= g.left() && g.right() <= x1 + 1.0 && y0 - 1.0 <= g.top() && g.bottom() <= y1 + 1.0))
        };
        let inner: Vec<&Grid> = grids
            .iter()
            .filter(|g| g.cells.len() >= 2 && !grids.iter().any(|o| g.encloses(o)) && !in_boxes(g))
            .chain(band_grids.iter())
            .collect();
        let all_after: Vec<&Col> = self.after.iter().flatten().collect();
        let all_before: Vec<&Col> = self.before.iter().flatten().collect();
        let inside = |g: &Grid, c: &Col| g.contains((c.x + c.x1) / 2.0, (c.y + c.y1) / 2.0, 0.5);
        // 片方の帯の語だけを含む枠をその帯の表とする（帯の境目をまたぐ外枠などは除く）。
        let tables_for = |own: &[&Col], other: &[&Col]| -> Vec<&Grid> {
            inner
                .iter()
                .copied()
                .filter(|g| own.iter().any(|c| inside(g, c)) && !other.iter().any(|c| inside(g, c)))
                .collect()
        };
        let after_grids = tables_for(&all_after, &all_before);
        let before_grids = tables_for(&all_before, &all_after);
        Unit::Shinkyu {
            preamble: page_lines(self.preamble),
            after: self.after.into_iter().map(|p| lines_with_tables(p, &after_grids)).collect(),
            before: self.before.into_iter().map(|p| lines_with_tables(p, &before_grids)).collect(),
            outside: self.trailing.into_iter().map(page_lines).collect(),
        }
    }
}

/// 帯の語を、罫線の表に入るものと入らないものに分け、表は表として、残りは行として組み、
/// 表を読み順（右→左）の位置に差し込む。
pub(crate) fn lines_with_tables(cols: Vec<Col>, grids: &[&Grid]) -> Vec<Line> {
    lines_with_tables_by(cols, grids, cell_lines)
}

/// [`lines_with_tables`] の表の外の語の組み方を指定する版（普通の本文の段は [`page_lines`]）。
pub(crate) fn lines_with_tables_by(cols: Vec<Col>, grids: &[&Grid], rest_lines: fn(Vec<Col>) -> Vec<Line>) -> Vec<Line> {
    let mut tables: Vec<Table> = Vec::new();
    let mut rest = cols;
    for g in grids {
        let inside = |c: &Col| g.contains((c.x + c.x1) / 2.0, (c.y + c.y1) / 2.0, 0.5);
        let (mine, others): (Vec<Col>, Vec<Col>) = rest.into_iter().partition(inside);
        rest = others;
        if !mine.is_empty() {
            let t = grid_table(g, mine.clone());
            // 1 セルだけ（枠で囲んだだけの文）は表にせず本文に戻す。
            if t.rows * t.cols <= 1 {
                rest.extend(mine);
            } else {
                tables.push(t);
            }
        }
    }
    let geom: std::collections::HashMap<usize, (f32, f32)> = rest.iter().map(|c| (c.id, ((c.x + c.x1) / 2.0, c.y))).collect();
    let mut lines = rest_lines(rest);
    // 右端の x が大きい表から、その表より左にある最初の行（行の中心が表の左端より左）か、表の
    // 下にある最初の行の手前へ差し込む（表の左の罫線に接して組まれた次の記事の標題や、表の下の
    // 備考を表の前に出さない）。
    tables.sort_by(|a, b| b.x1.partial_cmp(&a.x1).unwrap_or(std::cmp::Ordering::Equal));
    // 差し込む位置は表の無い行の並びで決める。
    let mut placed: Vec<(usize, Table)> = tables
        .into_iter()
        .map(|t| {
            let after_table = |l: &Line| {
                let g: Vec<(f32, f32)> = l.words.iter().filter_map(|w| geom.get(w)).copied().collect();
                let center = g.iter().map(|p| p.0).fold(f32::MIN, f32::max);
                let top = g.iter().map(|p| p.1).fold(f32::MAX, f32::min);
                !g.is_empty() && (center < t.x0 || (center <= t.x1 && top >= t.y1 - 2.0))
            };
            (lines.iter().position(after_table).unwrap_or(lines.len()), t)
        })
        .collect();
    // 後ろの位置から差し込む。同じ位置には左の表から差し込むと、右の表が前に来る。
    placed.reverse();
    placed.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    for (at, t) in placed {
        lines.insert(at, Line { text: t.render(), words: Vec::new(), table: Some(t) });
    }
    lines
}

/// 格子のセルに語を割り当てて表にする。縦組みなので「縦の列＝行」に転置する
/// （右の列が 1 行目、上の欄が 1 列目）。
///
/// 罫線で区切られた縦の帯の中に、行を区切る罫線が無いまま複数の行が並ぶ表もある
/// （「物質｜係数」の表で物質ごとの行）。帯の中で、複数の欄にまたがって同じ x に字がある
/// 文字の列（物質名と係数が同じ縦の線上に並ぶ）を 1 行の頭とし、1 つの欄にしか字の無い
/// 列（セルの中の折り返し）はその行に入れる。
fn grid_table(g: &Grid, cols: Vec<Col>) -> Table {
    let nx = g.xs.len() - 1;
    // 語 → セル。縦中横の短い語（「３番13号」の「13」）は字面が列から横にずれて隣のセルの
    // 境に掛かることがあるので、すぐ上に続く語のある列の中心で決める。
    let centers: Vec<f32> = cols
        .iter()
        .map(|c| {
            let center = (c.x + c.x1) / 2.0;
            if c.text.chars().count() > 3 {
                return center;
            }
            let w = c.x1 - c.x;
            cols.iter()
                .find(|a| a.id != c.id && (a.y1 - c.y).abs() < w * 1.5 && a.x < c.x1 && c.x < a.x1 && a.text.chars().count() > 3)
                .map_or(center, |a| (a.x + a.x1) / 2.0)
        })
        .collect();
    let mut cell_words: Vec<Vec<Col>> = vec![Vec::new(); g.cells.len()];
    for (c, cx) in cols.into_iter().zip(centers) {
        if let Some(i) = g.cell_at(cx, (c.y + c.y1) / 2.0) {
            cell_words[i].push(c);
        }
    }
    // 縦の帯（x 区間、右から）ごとに行の頭を決める。行の頭はその帯の文字の列（右→左）の
    // 添字の列。帯の中で最初の欄にある列の上端が、最も上の列から 0.5 文字以内なら頭。
    let strip_records: Vec<Vec<Vec<f32>>> = (0..nx)
        .rev()
        .map(|xi| {
            let single: Vec<usize> = (0..g.cells.len()).filter(|&i| g.cells[i].x0 == xi && g.cells[i].x1 == xi + 1).collect();
            // 字のある最初の欄（二重線の枠の間の空の細い欄などは飛ばす）。
            let top_field = single.iter().filter(|&&i| !cell_words[i].is_empty()).map(|&i| g.cells[i].y0).min();
            let words: Vec<Col> = single.iter().flat_map(|&i| cell_words[i].iter().cloned()).collect();
            let groups = column_groups(words);
            let field_of = |c: &Col| g.cell_at((c.x + c.x1) / 2.0, (c.y + c.y1) / 2.0).map(|i| g.cells[i].y0);
            let tops: Vec<Option<f32>> = groups
                .iter()
                .map(|grp| {
                    // 複数の欄にまたがり、最初の欄が項目の頭（（１）・（新設）・（略）・一　…）で
                    // 始まる列だけを行の頭にする（並行して折り返すセルと取り違えないため）。
                    let fields: std::collections::BTreeSet<usize> = grp.iter().filter_map(&field_of).collect();
                    let mut in_top: Vec<&Col> = grp.iter().filter(|c| field_of(c) == top_field).collect();
                    in_top.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));
                    let lead: String = in_top.iter().map(|c| c.text.as_str()).collect::<Vec<_>>().join("\u{3000}");
                    if fields.len() < 2 || !starts_item(&lead) {
                        return None;
                    }
                    in_top.first().map(|c| c.y)
                })
                .collect();
            let base = tops.iter().flatten().copied().reduce(f32::min);
            // 行ごとの文字の列の中心 x（その行に属する列を後で探すため）。
            let mut records: Vec<Vec<f32>> = Vec::new();
            for (grp, top) in groups.iter().zip(&tops) {
                let em = grp[0].x1 - grp[0].x;
                let cx = (grp[0].x + grp[0].x1) / 2.0;
                let head = matches!((top, base), (Some(t), Some(b)) if *t <= b + em * 0.5);
                match records.last_mut() {
                    Some(r) if !head => r.push(cx),
                    _ => records.push(vec![cx]),
                }
            }
            // 最初の頭より右の列（最初の欄に字の無い列）は最初の行に入れる。
            if records.len() >= 2 && !matches!((tops.first(), base), (Some(Some(t)), Some(b)) if *t <= b + 4.0) {
                let first = records.remove(0);
                records[0].splice(0..0, first);
            }
            if records.is_empty() {
                records.push(Vec::new());
            }
            records
        })
        .collect();
    // 帯（右から k 番目）の先頭の行番号。
    let mut offset = vec![0usize; nx + 1];
    for k in 0..nx {
        offset[k + 1] = offset[k] + strip_records[k].len();
    }
    let strip_of = |xi: usize| nx - 1 - xi; // x 区間 → 右からの番号
    let cell_text = |words: Vec<Col>| -> String {
        // 横組みの語（学名などの欧文）が多いセルは、行ごとに左から並べて語間に空白を入れる。
        let horizontal = words.iter().filter(|c| c.x1 - c.x > (c.y1 - c.y) * 1.5).count();
        if horizontal * 2 > words.len() {
            let mut ws = words;
            ws.sort_by(|a, b| {
                let row = |c: &Col| ((c.y + c.y1) / 2.0 / 4.0).round() as i64;
                row(a).cmp(&row(b)).then(a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
            });
            let mut text = String::new();
            let mut prev: Option<&Col> = None;
            for c in &ws {
                if let Some(p) = prev {
                    let em = (c.y1 - c.y).max(1.0);
                    let same_row = ((p.y + p.y1) / 2.0 - (c.y + c.y1) / 2.0).abs() < em * 0.5;
                    if !same_row || c.x - p.x1 > em * 0.2 {
                        text.push(' ');
                    }
                }
                text.push_str(&c.text);
                prev = Some(c);
            }
            return text;
        }
        let geom: std::collections::HashMap<usize, (f32, f32)> = words.iter().map(|c| (c.id, (c.y1, c.x1 - c.x))).collect();
        let lines: Vec<Line> = cell_lines(words);
        let bottom = |l: &Line| l.words.iter().filter_map(|w| geom.get(w)).map(|b| b.0).fold(f32::MIN, f32::max);
        let em = geom.values().map(|b| b.1).fold(f32::MAX, f32::min);
        let max_bottom = lines.iter().map(bottom).fold(f32::MIN, f32::max);
        // 空きを 2 つ以上含む項目（字間を空けた見出し「製　造　業　者　名」、氏名「代表取締役
        // 中島　栄一」）と、同上の「〃　ＤＫＴ２Ａ２型」。折り返した文の列の空きは項目の番号の
        // 後（「（一）　山形県山形」）くらい。
        let spaced = |t: &str| {
            let t = t.trim();
            t.matches('\u{3000}').count() >= 2 || t.starts_with('〃')
        };
        // セルの中の列の折り返しは連結し、文末（。）・列の途中で終わる短い項目（「大管工業㈱」）・
        // 空きのある項目の後で改行する。
        let mut text = String::new();
        for (i, l) in lines.iter().enumerate() {
            text.push_str(&l.text);
            let short = bottom(l) < max_bottom - em * 1.5;
            if i + 1 < lines.len() && (l.text.ends_with('。') || short || spaced(&l.text)) {
                text.push('\n');
            }
        }
        text
    };
    let mut cells: Vec<TableCell> = Vec::new();
    for (cell, words) in g.cells.iter().zip(cell_words) {
        let (s_right, s_left) = (strip_of(cell.x1 - 1), strip_of(cell.x0));
        let records = &strip_records[s_right];
        if cell.x1 - cell.x0 == 1 && records.len() >= 2 {
            // 行の区切りの無い帯: 行ごとのセルに分ける。
            let mut per: Vec<Vec<Col>> = vec![Vec::new(); records.len()];
            for w in words {
                let cx = (w.x + w.x1) / 2.0;
                let r = records
                    .iter()
                    .position(|rec| rec.iter().any(|&x| (x - cx).abs() < (w.x1 - w.x).max(6.0) * 0.75))
                    .unwrap_or(0);
                per[r].push(w);
            }
            for (r, ws) in per.into_iter().enumerate() {
                let (x0, x1) = ws
                    .iter()
                    .fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), c| (a.min(c.x), b.max(c.x1)));
                cells.push(TableCell {
                    row: offset[s_right] + r,
                    col: cell.y0,
                    rowspan: 1,
                    colspan: cell.y1 - cell.y0,
                    x0: if x0.is_finite() { x0 } else { g.xs[cell.x0] },
                    y0: g.ys[cell.y0],
                    x1: if x1.is_finite() { x1 } else { g.xs[cell.x1] },
                    y1: g.ys[cell.y1],
                    words: ws.iter().map(|c| c.id).collect(),
                    text: cell_text(ws),
                });
            }
        } else {
            cells.push(TableCell {
                row: offset[s_right],
                col: cell.y0,
                rowspan: offset[s_left + 1] - offset[s_right],
                colspan: cell.y1 - cell.y0,
                x0: g.xs[cell.x0],
                y0: g.ys[cell.y0],
                x1: g.xs[cell.x1],
                y1: g.ys[cell.y1],
                words: words.iter().map(|c| c.id).collect(),
                text: cell_text(words),
            });
        }
    }
    cells.sort_by_key(|c| (c.row, c.col));
    trim_empty_edges(Table {
        x0: g.left(),
        y0: g.top(),
        x1: g.right(),
        y1: g.bottom(),
        rows: offset[nx],
        cols: g.ys.len() - 1,
        cells,
    })
}

/// 表の端の、字の無い細い（20pt 未満）行・列を落とす（二重線の枠の間の欄など）。空でも
/// 幅のある欄は実在する欄なので残す（ページをまたいだ表の列数を揃えるため）。
fn trim_empty_edges(mut t: Table) -> Table {
    const THIN: f32 = 20.0;
    let empty_row = |t: &Table, r: usize| {
        let cs: Vec<&TableCell> = t.cells.iter().filter(|c| c.row <= r && r < c.row + c.rowspan).collect();
        cs.iter().all(|c| c.text.is_empty()) && cs.iter().all(|c| c.rowspan > 1 || c.x1 - c.x0 < THIN)
    };
    let empty_col = |t: &Table, k: usize| {
        let cs: Vec<&TableCell> = t.cells.iter().filter(|c| c.col <= k && k < c.col + c.colspan).collect();
        cs.iter().all(|c| c.text.is_empty()) && cs.iter().all(|c| c.colspan > 1 || c.y1 - c.y0 < THIN)
    };
    let drop_row = |t: &mut Table, r: usize| {
        t.cells.retain(|c| !(c.row == r && c.rowspan == 1));
        for c in &mut t.cells {
            if c.row > r {
                c.row -= 1;
            } else if c.row + c.rowspan > r {
                c.rowspan -= 1;
            }
        }
        t.rows -= 1;
    };
    let drop_col = |t: &mut Table, k: usize| {
        t.cells.retain(|c| !(c.col == k && c.colspan == 1));
        for c in &mut t.cells {
            if c.col > k {
                c.col -= 1;
            } else if c.col + c.colspan > k {
                c.colspan -= 1;
            }
        }
        t.cols -= 1;
    };
    while t.rows > 1 && empty_row(&t, t.rows - 1) {
        let r = t.rows - 1;
        drop_row(&mut t, r);
    }
    while t.rows > 1 && empty_row(&t, 0) {
        drop_row(&mut t, 0);
    }
    while t.cols > 1 && empty_col(&t, t.cols - 1) {
        let k = t.cols - 1;
        drop_col(&mut t, k);
    }
    while t.cols > 1 && empty_col(&t, 0) {
        drop_col(&mut t, 0);
    }
    t
}

/// 縦の列（x 近傍のカラム語の束）の頭。`lead` は列の先頭語。
struct ColumnHead {
    x: f32,
    y: f32,
    lead: String,
}

/// カラム語を [`reconstruct_page`] と同じ規則（右→左、x 差 10pt 以内を 1 列）で列に束ね、
/// 各列の頭（代表 x・最上端 y・先頭語）を右から順に返す。
fn column_heads(cols: &[Col]) -> Vec<ColumnHead> {
    let mut sorted: Vec<&Col> = cols.iter().collect();
    sorted.sort_by(|a, b| b.x.partial_cmp(&a.x).unwrap_or(std::cmp::Ordering::Equal));
    let mut heads: Vec<ColumnHead> = Vec::new();
    for c in sorted {
        match heads.last_mut() {
            Some(h) if h.x - c.x <= 10.0 => {
                if c.y < h.y {
                    h.y = c.y;
                    h.lead = c.text.clone();
                }
            }
            _ => heads.push(ColumnHead {
                x: c.x,
                y: c.y,
                lead: c.text.clone(),
            }),
        }
    }
    heads
}

#[cfg(test)]
mod tests {
    use crate::format::detect_format;
    use crate::reconstruct_vertical;

    #[test]
    fn finds_run_subsequence() {
        assert_eq!(super::find_run(&['改', '正', '後'], &['改', '正', '後']), Some(0));
        assert_eq!(
            super::find_run(&['x', '改', '正', '前'], &['改', '正', '前']),
            Some(1)
        );
        assert_eq!(super::find_run(&['改', '正'], &['改', '正', '後']), None);
    }

    /// 見出し段（帯ピッチ 170）の下に、列頭の y 差が `lower_pitch` の段の組が続くページ。
    fn page_with_lower_tiers(lower_pitch: f32) -> String {
        let w = |x: f32, y: f32, h: f32, t: &str| {
            format!(r#"<word xMin="{x}" yMin="{y}" xMax="{}" yMax="{}">{t}</word>"#, x + 8.0, y + h)
        };
        // 前文は複数列で上下帯を跨ぐ（実ページ同様、帯の境界で段に割れない）。
        let mut words = vec![
            w(344.0, 60.0, 350.0, "前文一"),
            w(332.0, 60.0, 350.0, "前文二"),
            w(320.0, 60.0, 350.0, "前文三"),
            // 欄見出しは各帯の中央に縦に並ぶ。
            w(300.0, 100.0, 8.0, "改"),
            w(300.0, 116.0, 8.0, "正"),
            w(300.0, 132.0, 8.0, "後"),
            w(300.0, 270.0, 8.0, "改"),
            w(300.0, 286.0, 8.0, "正"),
            w(300.0, 302.0, 8.0, "前"),
        ];
        for (i, x) in [280.0, 268.0, 256.0].into_iter().enumerate() {
            words.push(w(x, 70.0, 100.0, &format!("甲{i}")));
            words.push(w(x, 240.0, 100.0, &format!("乙{i}")));
            words.push(w(x, 430.0, 100.0, &format!("丙{i}")));
            words.push(w(x, 430.0 + lower_pitch, 100.0, &format!("丁{i}")));
        }
        format!(r#"<page width="595" height="842">{}</page>"#, words.join(""))
    }

    fn after_block(out: &str) -> String {
        let lines: Vec<&str> = out.lines().collect();
        let go = lines.iter().position(|l| *l == "改正後").unwrap();
        let mae = lines.iter().position(|l| *l == "改正前").unwrap();
        lines[go + 1..mae].concat()
    }

    #[test]
    fn absorbs_lower_tiers_aligned_at_band_pitch() {
        // 下の段の組が見出し段と同じ帯ピッチで揃う → 表の続き（丙=改正後, 丁=改正前）。
        let out = reconstruct_vertical(&page_with_lower_tiers(170.0));
        let after = after_block(&out);
        assert!(after.contains("甲0") && after.contains("丙0"), "{out}");
        assert!(!after.contains("丁0"), "{out}");
    }

    #[test]
    fn keeps_lower_tiers_at_other_pitch_out_of_table() {
        // 段送りのピッチ（184）で揃うだけの通常記事は表に取り込まない。
        let out = reconstruct_vertical(&page_with_lower_tiers(184.0));
        let after = after_block(&out);
        assert!(after.contains("甲0") && !after.contains("丙0"), "{out}");
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
}
