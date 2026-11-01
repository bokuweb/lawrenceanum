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
/// 「改正後／改正前」「変更後／変更前」の完全な語を、共通する列の矩形と
/// 元の Word の連続性から認定する。両役割の中心の中点を境界にする。
/// 本文の一部や途中の Word を飛ばした組合せは欄見出しにしない。
pub(crate) fn detect_shinkyu_headers(cols: &[Col]) -> Vec<(f32, f32)> {
    crate::comparison_labels::headers(cols).into_iter().map(|h|(h.x,h.divider)).collect()
}

// A reconstructed line may begin with a heading while also containing a
// neighboring table cell. Only a complete numbered publication lexeme can
// authorize a new preamble boundary; a prefix cannot claim the full line.
fn complete_numbered_publication_heading(text:&str)->bool{
    let text:String=text.chars().filter(|c|!c.is_whitespace()).collect();
    if !is_heading_mark(&text){return false;}
    let Some(core)=text.strip_suffix('号')else{return false;};
    let Some((name,number))=core.rsplit_once('第')else{return false;};
    !name.contains('号')
        &&["告示","省令","府令","規則","庁令","訓令","命令","令"].iter().any(|kind|name.ends_with(kind))
        &&!number.is_empty()
        &&number.chars().all(|c|c.is_ascii_digit()||('０'..='９').contains(&c)||"〇零一二三四五六七八九十百千万".contains(c))
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
        let between = |c: &&Col| c.x > h2 + 6.0 && c.x1 < h1 - 1.0;
        // An independent publication heading certifies the next preamble's
        // source boundary. A spanning note to its right still belongs to the
        // preceding table; crossing the role divider alone cannot reassign it.
        let heading_cols:Vec<_>=cols.iter().filter(between).cloned().collect();
        // Reuse the source column renderer so a joint ministry heading split
        // across original Words has the same boundary as a single-Word heading.
        // Every reconstructed heading Word must bind uniquely to this slice.
        let cut = page_lines(heading_cols.clone()).iter().filter(|l|complete_numbered_publication_heading(&l.text)).filter_map(|line|{
            if line.words.is_empty(){return None;}
            let mut ids=std::collections::HashSet::new();
            let mut right=f32::NEG_INFINITY;
            for id in &line.words{
                if !ids.insert(*id){return None;}
                let mut found=heading_cols.iter().filter(|c|c.id==*id);
                let c=found.next()?;if found.next().is_some(){return None;}
                right=right.max(c.x1);
            }
            Some(right)
        }).reduce(f32::max)
            .or_else(||cols.iter().filter(between).filter(|c|c.y<d1&&c.y1>d1).map(|c|c.x1).reduce(f32::max));
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
    // 中央域 [0.35H, 0.65H] で跨ぎ最小の cut（候補は各列の下端。またぐ列がそれより少なくなれば列の上端も。改正後の帯が
    // 「（削る）」だけだと、中ほどに列の下端が無い）。
    let (lo, hi) = (0.35 * height, 0.65 * height);
    let pick = |cuts: Vec<f32>| -> Option<(usize, f32)> {
        let mut best: Option<(usize, f32)> = None;
        for cut in cuts {
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
        best
    };
    let bottoms = pick(cols.iter().map(|c| c.y1 + 0.5).collect());
    let tops = pick(cols.iter().map(|c| c.y - 0.5).collect());
    let best = match (bottoms, tops) {
        (Some(b), Some(t)) if t.0 < b.0 => Some(t),
        (None, t) => t,
        (b, _) => b,
    };
    let (crossing, divider) = best?;
    let upper: Vec<Col> = cols.iter().filter(|c| center(c) < divider).cloned().collect();
    let lower: Vec<Col> = cols.iter().filter(|c| center(c) >= divider).cloned().collect();
    // 下の帯が「（新設）」「（削る）」の印だけ（改正前の欄が空で、新設した条の頭の高さに印だけがある）か、上の帯が印だけ
    // （改正後の欄で条を削った）なら、本文は比べられないが新旧対照表の続き。境をまたぐ列が無く、もう片方の帯に本文が
    // あるときだけ。
    let mark = |c: &Col| {
        let t = c.text.trim();
        t.chars().count() <= 6 && ["新設）", "新設〕", "新設］", "削る）", "削る〕", "削る］"].iter().any(|m| t.ends_with(m))
    };
    if crossing == 0 && ((upper.len() >= 8 && !lower.is_empty() && lower.iter().all(mark)) || (lower.len() >= 8 && !upper.is_empty() && upper.iter().all(mark))) {
        return Some(divider);
    }
    if upper.len() < 3 || lower.len() < 3 {
        return None;
    }
    // 改正の指示（「〜の一部を次のように改正する」「様式を次のように改める」）は改め文の本文で、新旧対照表の
    // 欄（法令自身の文）には出ない（様式を改める条が上下に並び、様式の図の中の字が似ているページ）。
    // 上の帯（改正後の欄になるはずのところ）に収まる語にあるときだけ見る（表の左に紙面の高さいっぱいに
    // 組まれた次の記事の指示や、表の下に続く指示はある）。
    let text: String = upper.iter().filter(|c| c.y1 < divider).flat_map(|c| c.text.chars()).collect();
    // 改め文（「第四十二条第一項第一号の次に次の一号を加える。」「中「前項第二号」を「前項第三号」に改め」）も
    // 新旧対照表の欄には出ない（上下の段に似た号が並ぶ改め文のページ）。
    let amending = ["の一部を次のように改正する", "を次のように改める", "の次に次の", "」に改め", "」を削り"];
    if amending.iter().any(|p| text.contains(p)) {
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
    // 上下の帯がどちらも、真ん中あたりで列のまたがない横の空きで 2 段に分かれ、上下の帯に記事の
    // 見出し（「〇農林水産省告示第…号」）があれば、4 段組みの普通のページ（同じ形の告示が並ぶ
    // 保安林の指定施業要件の変更など。上半分と下半分が似て見える）。
    let has_heading = |g: &[Col]| g.iter().any(|c| c.text.starts_with('〇') && (c.text.contains("告示第") || c.text.contains("令第")));
    if has_heading(&upper) && has_heading(&lower) && splits_into_tiers(&upper) && splits_into_tiers(&lower) {
        return None;
    }
    // 上下の帯の記事の見出しがどれも違えば、上下に別の告示が並ぶページ（同じ形の商標の告示が上下に並び、
    // 本文が似て見える）。新旧対照表なら改正後と改正前の欄に同じ見出しが並ぶ。
    let headings = |g: &[Col]| -> Vec<String> {
        g.iter()
            .filter(|c| c.text.starts_with('〇') && (c.text.contains("告示第") || c.text.contains("令第")))
            .map(|c| c.text.chars().filter(|ch| !ch.is_whitespace()).collect())
            .collect()
    };
    let (upper_heads, lower_heads) = (headings(&upper), headings(&lower));
    if !upper_heads.is_empty() && !lower_heads.is_empty() && !upper_heads.iter().any(|h| lower_heads.contains(h)) {
        return None;
    }
    // 内容類似（改正後 ≒ 改正前）。新旧対照表は両欄がほぼ同一テキストで差分のみ異なる。
    // 人事異動一覧のように上下で別内容のページはここで弾かれる。
    // 上下の本文が順序も含めてほぼ同じであること（新旧対照表は改正後 ≒ 改正前）。文字の
    // 集合だけ比べると、似た法令文が続く普通の 2 段組み（政令本文など）も似て見えるので、
    // 最長共通部分列の長さの比で比べる。
    // 上下の段の列の頭の項番号（「25」「26」…）か条番号が重ならず、下の段が上の段の続きの番号なら、改め文の続き（同じ形の項が
    // 並ぶ「…五年を経過したときは、…審判手続開始の決定をすることができない。」）。新旧対照表の改正前の欄は改正後の欄と
    // 同じ項番号を繰り返す。
    let ((ui, ua), (li, la)) = (item_numbers(&upper), item_numbers(&lower));
    // 号の続きの行の頭に字だけの数（化学名の位置番号「八」「二」）があっても、1 ずつ増える最も長い並びだけを見る
    // （号外第 209 号 39 ページ: 上の段の「十三」〜「二十五」の間に「五」、下の段の「二十六」〜「四十一」の間に「八」「二」）。
    let chain = |v: &[u32]| -> Vec<u32> {
        (0..v.len())
            .map(|i| v[i..].iter().fold(vec![v[i]], |mut run, &n| { if n == run[run.len() - 1] + 1 { run.push(n); } run }))
            .max_by_key(|run| run.len())
            .unwrap_or_default()
    };
    let continues = |u: &[u32], l: &[u32]| {
        let (u, l) = (chain(u), chain(l));
        u.len() >= 3 && l.len() >= 3 && l.iter().min() > u.iter().max()
    };
    // 条の見出しは上下で 2 つ以上ずつ（条の中の項番号「２」「３」は条ごとに振り直すので、条番号を先に見る）。
    if continues(&ui, &li) || (ua.len() >= 2 && la.len() >= 1 && la.iter().min() > ua.iter().max() && ua.len() + la.len() >= 4) {
        return None;
    }
    // 料金表などの行名「第四項第一号…第四項第七号」「第四項第八号…」も、項と号を合わせて
    // 比べる。条番号だけを拾うと、似た金額が上下に続く通常の表を新旧対照表と取り違える。
    let (ur, lr) = (provision_references(&upper), provision_references(&lower));
    if ur.len() >= 3 && lr.len() >= 3 && lr.iter().min() > ur.iter().max() {
        return None;
    }
    let (uk, lk) = (kana_headings(&upper), kana_headings(&lower));
    let lower_starts_heading = lower
        .iter()
        .max_by(|a, b| a.x.total_cmp(&b.x).then_with(|| b.y.total_cmp(&a.y)))
        .is_some_and(|c| kana_number(&c.text).is_some());
    let ut = reconstruct_page(upper);
    let lt = reconstruct_page(lower);
    // 同じ小項目を持つ説明（イの下にもロの下にも（イ）〜（ホ）がある等）は上下で似る。
    // 大項目のカナ番号が進み、帯の切れ目でカタカナ語まで続いているなら、通常の段送り。
    // カナ番号の変更だけでは新旧対照表の号の繰下げと区別できないので、両方を求める。
    let katakana = |c: char| ('ァ'..='ヶ').contains(&c) || c == 'ー';
    if !uk.is_empty()
        && !lk.is_empty()
        && lk.iter().min() > uk.iter().max()
        && !lower_starts_heading
        && ut.trim_end().chars().rev().take_while(|&c| katakana(c)).count() >= 2
        && (1..=2).contains(&lt.trim_start().chars().take_while(|&c| katakana(c)).count())
    {
        return None;
    }
    if bigram_jaccard(&ut, &lt) < 0.5 || sequence_ratio(&ut, &lt) < 0.5 {
        return None;
    }
    Some(divider)
}

/// 列頭の条・項・号を合わせた参照番号（「第二条第四項第八号」など）。
/// 同じ参照の「ただし書」の列は一度だけ数える。
fn provision_references(band: &[Col]) -> Vec<[u32; 3]> {
    let mut refs = Vec::new();
    for g in column_groups(band.to_vec()) {
        let Some(top) = g.iter().min_by(|a, b| a.y.total_cmp(&b.y)) else { continue };
        let mut rest = top.text.trim();
        let mut numbers = [0; 3];
        let mut last = None;
        while let Some(s) = rest.strip_prefix('第') {
            let n: String = s.chars().take_while(|c| "〇一二三四五六七八九十百千".contains(*c)).collect();
            let Some(value) = kanji_number(&n) else { break };
            let Some(unit) = s[n.len()..].chars().next() else { break };
            let index = match unit {
                '条' => 0,
                '項' => 1,
                '号' => 2,
                _ => break,
            };
            if last.is_some_and(|i| index <= i) {
                break;
            }
            numbers[index] = value;
            last = Some(index);
            rest = &s[n.len() + unit.len_utf8()..];
        }
        if last == Some(2) && numbers[..2] != [0, 0] {
            refs.push(numbers);
        }
    }
    refs.sort_unstable();
    refs.dedup();
    refs
}

/// 括弧の無いカナの号見出し。本文の語頭「ローカル」等は数えない。
fn kana_headings(band: &[Col]) -> Vec<usize> {
    column_groups(band.to_vec())
        .iter()
        .filter_map(|g| {
            let top = g.iter().min_by(|a, b| a.y.total_cmp(&b.y))?;
            kana_number(&top.text)
        })
        .collect()
}

fn kana_number(text: &str) -> Option<usize> {
    let mut chars = text.trim_start().chars();
    let c = chars.next()?;
    if chars.next().is_some_and(|next| !next.is_whitespace()) {
        return None;
    }
    "イロハニホヘトチリヌルヲワカヨタレソツネナラムウヰノオクヤマケフコエテアサキユメミシヱヒモセス".chars().position(|k| k == c)
}

/// 段の列の頭の番号（算用数字で始まる列の頭の項番号と、条の見出し「第二十八条」の条番号を別々に。枝番の「の二」は
/// 同じ条として見る）。
fn item_numbers(band: &[Col]) -> (Vec<u32>, Vec<u32>) {
    let (mut items, mut articles) = (Vec::new(), Vec::new());
    for g in column_groups(band.to_vec()) {
        let Some(top) = g.iter().min_by(|a, b| a.y.total_cmp(&b.y)) else { continue };
        let t = top.text.trim();
        let digit = |c: char| c.to_digit(10).or_else(|| ('０'..='９').contains(&c).then(|| c as u32 - '０' as u32));
        let digits: Vec<u32> = t.chars().map_while(digit).collect();
        // 漢数字の号の番号（「九十九　アルキル…」「百　アンモニウム…」）も項番号。
        let kanji: String = t.chars().take_while(|c| "〇一二三四五六七八九十百千".contains(*c)).collect();
        let after = t[kanji.len()..].chars().next();
        if (1..=3).contains(&digits.len()) {
            items.push(digits.iter().fold(0, |n, d| n * 10 + d));
        } else if !kanji.is_empty() && kanji.chars().count() <= 6 && matches!(after, None | Some('\u{3000}') | Some(' ')) {
            if let Some(n) = kanji_number(&kanji) {
                items.push(n);
            }
        } else if let Some(rest) = t.strip_prefix('第') {
            let kanji: String = rest.chars().take_while(|c| "〇一二三四五六七八九十百千".contains(*c)).collect();
            if let Some(n) = rest[kanji.len()..].starts_with('条').then(|| kanji_number(&kanji)).flatten() {
                articles.push(n);
            }
        }
    }
    (items, articles)
}

/// 漢数字（「二十八」「百五十六」「千二百」）の値。
fn kanji_number(s: &str) -> Option<u32> {
    let digit = |c: char| "〇一二三四五六七八九".find(c).map(|i| (i / '一'.len_utf8()) as u32);
    let (mut total, mut cur) = (0u32, 0u32);
    for c in s.chars() {
        match c {
            '十' | '百' | '千' => {
                let unit = match c {
                    '十' => 10,
                    '百' => 100,
                    _ => 1000,
                };
                total += cur.max(1) * unit;
                cur = 0;
            }
            _ => cur = cur * 10 + digit(c)?,
        }
    }
    (!s.is_empty()).then_some(total + cur)
}

/// 帯が、高さの真ん中あたり（3 割〜7 割）で列のまたがない横の空きにより、それぞれ 4 分の 1 以上の
/// 語を持つ上下 2 つの段に分かれるか。段の終わりの列が下の段の頭に少し掛かる（行末のぶら下げ）ことがあるので、
/// またぐ列は 2 つまで許す（本紙第 1766 号 2 ページ: 保安林の告示が 4 段に並ぶページの上の段の左端の列）。
fn splits_into_tiers(band: &[Col]) -> bool {
    let top = band.iter().map(|c| c.y).fold(f32::INFINITY, f32::min);
    let bottom = band.iter().map(|c| c.y1).fold(f32::NEG_INFINITY, f32::max);
    let h = bottom - top;
    band.iter().map(|c| c.y1 + 0.5).filter(|&y| y > top + h * 0.3 && y < top + h * 0.7).any(|y| {
        let above = band.iter().filter(|c| c.y1 < y).count();
        let below = band.iter().filter(|c| c.y > y).count();
        above + below + 2 >= band.len() && above * 4 >= band.len() && below * 4 >= band.len()
    })
}

/// 見出しの無い隣り合う 2 段（上の段 `a`・下の段 `b`）が、新旧対照表の改正後帯・改正前帯の
/// 組に見えるか。継続ページが 4 段（改正後・改正前・改正後・改正前）に組まれる場合に、段の組
/// ごとに判定する。高さと左右の範囲がほぼ揃い、本文が順序も含めてほぼ同じで、省略・新設の印が
/// あること、または上の段に「略」・下の段に「同上」（か「（削る）」「（新設）」）があって両段の列が
/// 同じ x に並ぶことを求める。
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
    if marked && bigram_jaccard(&at, &bt) >= 0.5 && sequence_ratio(&at, &bt) >= 0.5 {
        return true;
    }
    // 改正前の欄が「〔同上〕」ばかりで本文が似ていなくても、上の段に「略」、下の段に「同上」
    // 「同左」があるか、「（削る）」「（新設）」（改め文の「…を削る。」とは違って括弧に入る）が
    // 合わせて 2 つ以上あって（削った条ばかりの帯は似ていない）、両段の列が同じ x に並んでいれば
    // 帯の組（対応する行を同じ x に組む）。
    let count = |t: &str, marks: &[&str]| marks.iter().map(|m| t.matches(m).count()).sum::<usize>();
    let omitted = count(&at, &["略）", "略〕", "略］"]) >= 2 && count(&bt, &["同上）", "同上〕", "同左）", "同左］"]) >= 2;
    let changed = |t: &str| count(t, &["削る）", "削る〕", "削る］", "新設）", "新設〕", "新設］", "削る。〕", "加える。〕"]);
    let changed = changed(&at) + changed(&bt) >= 2;
    if !omitted && !changed {
        return false;
    }
    let heads = |g: &[Col]| -> Vec<f32> { column_groups(g.to_vec()).iter().map(|g| g[0].x).collect() };
    let (ha, hb) = (heads(a), heads(b));
    let hit = |x: f32, other: &[f32]| other.iter().any(|y| (x - y).abs() <= 1.5);
    let matched = hb.iter().filter(|&&x| hit(x, &ha)).count();
    // 前のページの続き（片方の帯の右端にだけある列）は数えない: 右から最初に揃う列までを除く。
    let rest = |own: &[f32], other: &[f32]| own.len() - own.iter().take_while(|&&x| !hit(x, other)).count();
    let (ra, rb) = (rest(&ha, &hb), rest(&hb, &ha));
    if omitted && matched >= 4 && matched * 10 >= ra.max(rb) * 6 {
        return true;
    }
    // 削った条・新設の条の行は片方の帯が空いて列が疎になるので、疎な方の段の列がほぼ全部もう
    // 片方の段の列と揃うこと（同じ行送りの 2 段組みでも揃うので、印のあるときだけ）。両方の印が
    // あれば、行の長さが違って行頭だけが揃う帯も採る。
    let need = if omitted && changed { 6 } else { 7 };
    changed && matched >= 6 && matched * 10 >= ra.min(rb) * need
}

/// 新旧対照表に特有の省略・新設の印（「（略）」「〔九〜一六　略〕」「〔同上〕」「［同左］」
/// 「（新設）」「（削る）」「〔条を削る。〕」「〔項を加える。〕」）があるか。
fn has_shinkyu_marks(t: &str) -> bool {
    const MARKS: [&str; 15] = [
        "略）", "略〕", "略］", "同上）", "同上〕", "同左］", "同左）", "新設）", "新設〕", "新設］", "削る）", "削る〕",
        // 「〔条を削る。〕」「〔項を加える。〕」の書き方。
        "削る。〕", "加える。〕",
        // 表の注記（「備考　表中の〔　〕の記載は注記である。」）。
        "記載は注記である",
    ];
    MARKS.iter().any(|m| t.contains(m))
}

/// [`tiers_look_like_bands`] の 2 段を改正後・改正前として組む。
pub(crate) fn band_pair_unit(after: Vec<Col>, before: Vec<Col>, grids: &[Grid], rules: Option<&PageRules>) -> Unit {
    ShinkyuLayout {labels:None,
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
#[cfg(test)]
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
    let mut outside: Vec<Col> = layout.trailing.drain(..).flatten().collect();
    // 改正後・改正前の帯を囲む表の外枠（帯の境目をまたぎ、帯の語の半分以上を囲む枠）の左の外にある改正後の帯の列（表の後ろ
    // の「備考」「附則」）は、表の外（改正前の帯の後ろ）に置く（改正後と改正前の間に読まれていた）。
    if let Some(after) = layout.after.first_mut() {
        let center = |c: &Col| ((c.x + c.x1) / 2.0, (c.y + c.y1) / 2.0);
        let all: Vec<&Col> = after.iter().chain(layout.before.iter().flatten()).collect();
        let frame_left = grids
            .iter()
            .filter(|g| g.top() < divider && divider < g.bottom())
            .filter(|g| all.iter().filter(|c| g.contains(center(c).0, center(c).1, 0.5)).count() * 2 >= all.len())
            .map(|g| g.left())
            .reduce(f32::min);
        if let Some(left) = frame_left {
            let (inside, beyond): (Vec<Col>, Vec<Col>) = std::mem::take(after).into_iter().partition(|c| center(c).0 >= left - 2.0);
            *after = inside;
            outside.extend(beyond);
        }
    }
    if let Some(before) = layout.before.first_mut() {
        before.extend(outside);
    }
    layout.into_unit(grids, rules)
}

/// 上下の数表の行番号が順に続くなら、前の記事の通常表の段送り。新旧帯なら同じ番号を繰り返す。
/// 各段の右端から整数見出しが3列以上連続し、上段全体が連番、下段先頭がその次の番号であること。
/// 本文が数値だけの列に限り、略・新設などの新旧表の印がある場合は従来の判定に残す。
fn continues_numbered_numeric_table(after: &[Col], before: &[Col]) -> bool {
    if after.iter().chain(before).any(|c| has_shinkyu_marks(&c.text)) {
        return false;
    }
    let rows = |band: &[Col], complete: bool| -> Option<Vec<(u32, f32)>> {
        let mut out = Vec::new();
        let mut top = None;
        for g in column_groups(band.to_vec()) {
            let head = g.iter().min_by(|a, b| a.y.total_cmp(&b.y))?;
            let text = head.text.trim();
            let integer = !text.is_empty() && text.chars().count() <= 4 && text.chars().all(|c| c.is_ascii_digit() || ('０'..='９').contains(&c));
            if !integer {
                if complete {
                    return None;
                }
                break;
            }
            let n = text
                .chars()
                .fold(0, |n, c| n * 10 + c.to_digit(10).unwrap_or_else(|| c as u32 - '０' as u32));
            // 説明文の項番号や、次の列へ折り返した数値を見出しにしない。
            if top.is_some_and(|y: f32| (head.y - y).abs() > 2.0)
                || g.len() < 4
                || !g.iter().filter(|c| c.id != head.id).all(|c| {
                    !c.text.is_empty()
                        && c.text
                            .chars()
                            .all(|ch| ch.is_whitespace() || ch.is_ascii_digit() || ('０'..='９').contains(&ch) || ".．,，＋+－-−△()（）".contains(ch))
                })
            {
                return None;
            }
            top = Some(head.y);
            out.push((n, (head.x + head.x1) / 2.0));
        }
        (out.len() >= 3 && out.windows(2).all(|w| w[1].0 == w[0].0 + 1)).then_some(out)
    };
    let (Some(a), Some(b)) = (rows(after, true), rows(before, false)) else {
        return false;
    };
    b[0].0 == a.last().unwrap().0 + 1 && (a[0].1 - b[0].1).abs() <= 2.0
}

/// 新旧対照表 1 件分のカラム配置。
///
/// 帯ごとのカラム群をページ内の段ごとに分けて持つ（同じ x に別の段の列があるため、
/// 1 つの Vec に混ぜると右→左復元で列が連結されてしまう）。
pub(crate) struct ShinkyuLayout {
    /// 罫線の外枠から求めた改正後帯・改正前帯の矩形 `(x0, x1, y0, y1)`（見出し段のみ）。
    band_boxes: Option<[(f32, f32, f32, f32); 2]>,
    labels: Option<crate::ComparisonLabels>,
    preamble: Vec<Col>,
    after: Vec<Vec<Col>>,
    before: Vec<Vec<Col>>,
    /// 表の外にある列（表の下・左に同居する別記事）。表の後ろに読み順で出す。
    trailing: Vec<Vec<Col>>,
}

impl ShinkyuLayout {
    pub(crate) fn native_fields(after:Vec<Col>,before:Vec<Col>,boxes:[(f32,f32,f32,f32);2])->Self { Self {labels:None, band_boxes:Some(boxes),preamble:vec![],after:vec![after],before:vec![before],trailing:vec![] } }
    /// 見出し列 x より右(=`x >= header_x`)は前文・標題とみなし先頭にそのまま置く。
    /// 表本体(`x < header_x`)を境界 y で上下に分ける。
    pub(crate) fn split(cols: Vec<Col>, header_x: f32, divider: f32) -> Self {
        let header = crate::comparison_labels::headers(&cols).into_iter().find(|h|h.x==header_x && h.divider==divider);
        let labels=header.as_ref().map(|h|h.labels).filter(|p|*p!=crate::ComparisonLabels::AMENDMENT);
        let label_words=header.map(|h|h.words).unwrap_or_default();
        let mut preamble: Vec<Col> = Vec::new();
        let mut upper: Vec<Col> = Vec::new();
        let mut lower_all: Vec<Col> = Vec::new();
        for c in cols {
            if label_words.contains(&c.id) {continue;}
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
            labels,
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
        // 行末の「、」「。」だけが帯の境目を越える列（段の字詰めの端の外へ半字分まで出して組んだ、ぶら下げ）は跨がない
        // （号外第172号275ページ: 前の記事の「（注）…については、」の「、」が境目を 0.2pt 越え、前文の右端がその列に
        // なって、前の記事の続きと次の記事の前文が新旧対照表の前文と 1 行ずつつながっていた）。
        let hangs = |c: &Col| matches!(c.text.chars().last(), Some('、' | '。')) && c.y1 - divider <= (c.x1 - c.x) * 0.5;
        let spans = |c: &Col| c.y < divider && c.y1 > divider && !hangs(c);
        // 前文が上の帯に収まる（両帯を跨ぐ列が無い）ときは、上下の帯で同じ x に並ぶ列（3 つ以上）を前の表の末尾とし、
        // その左端を前文の右端とみなす（前の表の末尾の上下の行が 1 行につながっていた）。
        let paired: Vec<&Col> = self
            .preamble
            .iter()
            .filter(|c| c.y1 <= divider && self.preamble.iter().any(|d| d.y >= divider && (d.x - c.x).abs() <= 2.0))
            .collect();
        let (span_right, em) = match self.preamble.iter().filter(|c| spans(c)).map(|c| c.x1).reduce(f32::max) {
            Some(r) => (r, self.preamble.iter().filter(|c| spans(c)).map(|c| c.x1 - c.x).fold(f32::INFINITY, f32::min)),
            None if paired.len() >= 3 => (
                paired.iter().map(|c| c.x).fold(f32::INFINITY, f32::min) - 3.0,
                paired.iter().map(|c| c.x1 - c.x).fold(f32::INFINITY, f32::min),
            ),
            None => return Vec::new(),
        };
        // 字の大きさは幅と高さの小さい方で比べる（縦中横の「(120)」は幅だけ広い）。
        let glyph = |c: &Col| (c.x1 - c.x).min(c.y1 - c.y);
        // 表の帯（罫線の外枠があればその上端）より上（段の上の方の前文）は末尾ではない。
        let band_top = match self.band_boxes {
            Some([(_, _, top, _), _]) => top,
            None => self.after.iter().flatten().map(|c| c.y).fold(f32::INFINITY, f32::min),
        };
        let is_tail = |c: &Col| c.x > span_right + 2.0 && !spans(c) && glyph(c) <= em * 1.3 && c.y1 > band_top - em;
        let (tail, rest): (Vec<Col>, Vec<Col>) = std::mem::take(&mut self.preamble).into_iter().partition(is_tail);
        let (upper, lower): (Vec<Col>, Vec<Col>) = tail.into_iter().partition(|c| c.y1 <= divider || hangs(c));
        let mut rest = rest;
        // 標題の列から左（次の記事）を切る。連名の標題（「内閣府、総務省、／〇財務省、…令第七号」）
        // は「、」で終わる右隣の行も標題に含める。標題より左の下の帯（`below`）にも列があれば、その標題の記事は
        // 上の段から下の段へ続く記事で、新旧対照表の記事ではないので、そこでは切らずにより左の標題を探す（号外第172号
        // 275ページ: 上の段の「〇経済産業省告示第九十八号」の前文の後、下の段に本文「特定引受基準」が続く。前文だけが
        // 新旧対照表の前文に回り、本文の後に読まれていた）。
        let mut cut_at_heading = |part: Vec<Col>, below: &[Col]| -> Vec<Col> {
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
            let heading = (0..lines.len()).filter(|&h| is_heading_mark(&lines[h].0.text)).map(|h| {
                let mut ext = lines[h].1;
                for (l, e) in lines[..h].iter().rev() {
                    if !l.text.ends_with('、') || e.0 > ext.0 + em * 2.0 {
                        break;
                    }
                    ext = (e.0, ext.1.max(e.1));
                }
                ext
            }).find(|&(right, _)| !below.iter().any(|c| (c.x + c.x1) / 2.0 <= right));
            let next_article = |c: &Col| heading.is_some_and(|(right, _)| (c.x + c.x1) / 2.0 <= right);
            let (next, cont): (Vec<Col>, Vec<Col>) = part.into_iter().partition(next_article);
            rest.extend(next);
            cont
        };
        let upper = cut_at_heading(upper, &lower);
        let lower = cut_at_heading(lower, &[]);
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
        // 改正前の帯だけに残った文の続き（改正前の欄が長く次の段へ送られたもの）は、新旧対照表
        // として出し、構造化で前の表の最後の行の改正前につなぐ。見出し（「（…の一部改正）」）や
        // 標題・印だけのもの、附則の本文はこれまでどおり本文にする。
        let before_text = reconstruct_page(before.clone());
        let continued_before = after.is_empty()
            && out_after.is_empty()
            && before_text.chars().count() >= 10
            && !before_text.starts_with(['（', '〇', '('])
            // 改正する法令自身の附則（「この省令は、…から施行する。」）は表の続きではない。
            && !(before_text.starts_with("この") && (before_text.contains("施行") || before_text.contains("適用")));
        let outside: Vec<Line> = cell_lines(out_after).into_iter().chain(cell_lines(out_before)).collect();
        let mut units = Vec::new();
        if one_sided && continued_before {
            units.push(Unit::Shinkyu {labels:None,
                preamble: Vec::new(),
                after: vec![cell_lines(after)],
                before: vec![cell_lines(before)],
                outside: Vec::new(),
            });
            if !outside.is_empty() {
                units.push(Unit::Plain(outside));
            }
            return units;
        }
        if one_sided || !looks_like_table() {
            let lines: Vec<Line> = cell_lines(after).into_iter().chain(cell_lines(before)).chain(outside).collect();
            if !lines.is_empty() {
                units.push(Unit::Plain(lines));
            }
            return units;
        }
        let numbered_continuation = continues_numbered_numeric_table(&after, &before);
        let unit = ShinkyuLayout {labels:None,
            band_boxes: None,
            preamble: Vec::new(),
            after: vec![after],
            before: vec![before],
            trailing: Vec::new(),
        }.into_unit(grids, rules);
        // 罫線や数値の類似だけでは前の記事の表を新旧表にしない。通常の数表と分かったときは、
        // 組み上がった行・Table・語IDはそのままに、新旧属性だけを外す。
        units.push(if numbered_continuation {
            let Unit::Shinkyu { preamble, after, before, outside ,..} = unit else { unreachable!() };
            Unit::Plain(preamble.into_iter().chain(after.into_iter().flatten()).chain(before.into_iter().flatten()).chain(outside.into_iter().flatten()).collect())
        } else { unit });
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
        // 帯の矩形: 左右は枠の左端から欄見出しの列の左の罫線まで、上下は枠の上端・帯の境目・下端。
        let label_left = frame.xs.iter().copied().filter(|&x| x < header_x - 1.0).fold(frame.left(), f32::max);
        let mid = frame.ys.iter().copied().min_by(|a, b| (a - divider).abs().partial_cmp(&(b - divider).abs()).unwrap()).unwrap_or(divider);
        // 枠の左端に、帯の境目の横線が届かず上下の帯をまたぐ細い欄（表の後ろの「備考」、数行）が
        // あり、その列が帯の境目をまたいでいれば（長い備考）、帯の中心で上下に分けると備考が割れる。
        // 欄の列はまとめて改正後の帯の終わりに置き（短い備考が改正後の帯に収まるときと同じ）、
        // 欄の右の罫線を帯の左端にする。セルが両帯をまたぐことは罫線の格子で判断し、
        // 本文幅に対する割合では制限しない。共通セルの幅だけでは所属は変わらない。
        let left = frame.left();
        let mut note_right = left;
        while let Some(c) = frame.cells.iter().find(|c| {
            (frame.xs[c.x0] - note_right).abs() < 1.0
                && frame.ys[c.y0] < mid - 1.0
                && frame.ys[c.y1] > mid + 1.0
        }) {
            note_right = frame.xs[c.x1];
        }
        let in_note = |c: &Col| (left..=note_right).contains(&((c.x + c.x1) / 2.0));
        let split_note = self.after.first().into_iter().chain(self.before.first()).flatten().any(|c| in_note(c) && c.y < mid && c.y1 > mid);
        let band_left = if split_note { note_right } else { left };
        self.band_boxes = Some([(band_left, label_left, frame.top(), mid), (band_left, label_left, mid, frame.bottom())]);
        let mut out: Vec<Col> = Vec::new();
        let mut note: Vec<Col> = Vec::new();
        for band in [&mut self.after, &mut self.before] {
            if let Some(part) = band.first_mut() {
                let (inside, outside): (Vec<Col>, Vec<Col>) = std::mem::take(part).into_iter().partition(|c| (c.x + c.x1) / 2.0 > left);
                let (inside, in_note_cell): (Vec<Col>, Vec<Col>) = inside.into_iter().partition(|c| !split_note || !in_note(c));
                *part = inside;
                note.extend(in_note_cell);
                out.extend(outside);
            }
        }
        if let Some(after) = self.after.first_mut() {
            after.extend(note);
        }
        // 枠の左の外の列が、帯の境目の近くで上下の段に分かれていれば（境目の近くに、またぐ列の
        // ほとんど無い横の空きがある）、上の段と下の段に分けて置く（同じ x に上下の段の列があり、
        // 1 つにまとめると右→左に組むときに上の段の列と下の段の列がつながる）。空きをまたぐ列
        // （段を通して組んだ附則）とそれに続く語は上に置く。横組みの欄（表の下の横書きの
        // 新旧対照表など）があれば、欄を上下に割らないよう分けない。
        let wide = out.iter().any(|c| c.text.chars().count() >= 2 && c.x1 - c.x > (c.y1 - c.y) * 1.5);
        let center = |c: &Col| (c.x + c.x1) / 2.0;
        // y をまたぐ列の数（語がまたぐか、同じ列で上下に 1.5 字以内で続く語の間が y にかかる）。
        let crossing = |y: f32| {
            let mut xs: Vec<f32> = out
                .iter()
                .filter(|c| {
                    let em = c.x1 - c.x;
                    (c.y < y && y < c.y1)
                        || (c.y1 <= y && out.iter().any(|d| (center(d) - center(c)).abs() < em * 0.5 && d.y >= y && d.y - c.y1 < em * 1.5))
                })
                .map(center)
                .collect();
            xs.sort_by(|a, b| a.total_cmp(b));
            xs.dedup_by(|a, b| (*a - *b).abs() < 4.0);
            xs.len()
        };
        let mut columns: Vec<f32> = out.iter().map(center).collect();
        columns.sort_by(|a, b| a.total_cmp(b));
        columns.dedup_by(|a, b| (*a - *b).abs() < 4.0);
        let cut = (!wide)
            .then(|| {
                out.iter()
                    .flat_map(|c| [c.y - 0.5, c.y1 + 0.5])
                    .filter(|y| (y - mid).abs() <= 40.0)
                    // 罫線の表（帯の外の別の記事の表）を横切る所では分けない。
                    .filter(|&y| {
                        !grids.iter().any(|g| g.top() < y && y < g.bottom() && out.iter().any(|c| g.contains(center(c), (c.y + c.y1) / 2.0, 0.5)))
                    })
                    .map(|y| (crossing(y), y))
                    .min_by(|a, b| a.0.cmp(&b.0).then((a.1 - mid).abs().total_cmp(&(b.1 - mid).abs())))
            })
            .flatten()
            .filter(|&(n, _)| n * 5 <= columns.len());
        let cut_y = cut.map(|(_, y)| y).unwrap_or(f32::INFINITY);
        let (mut upper, lower): (Vec<Col>, Vec<Col>) = out.into_iter().partition(|c| c.y < cut_y);
        // 空きをまたぐ語の下に続く語。
        let mut spanning: Vec<(f32, f32, f32)> = upper.iter().filter(|c| c.y1 > cut_y).map(|c| (center(c), c.y1, c.x1 - c.x)).collect();
        let mut lower_sorted = lower;
        lower_sorted.sort_by(|a, b| a.y.total_cmp(&b.y));
        // 下の段の列は段の上端から始まる。段の上端から離れた所で始まる列の最初の語で、上に同じ列の
        // 語があるもの（下に寄せた署名「総務大臣林芳正」、字間を空けた標題の「省　令」の「令」）は上の段の列の続き。
        let lower_top = lower_sorted.first().map(|c| c.y).unwrap_or(f32::INFINITY);
        let mut lower = Vec::new();
        for c in lower_sorted {
            let em = c.x1 - c.x;
            let same_column = |d: &Col| (center(d) - center(&c)).abs() < em * 0.5;
            let continues = spanning.iter().any(|&(x, y1, _)| (x - center(&c)).abs() < em * 0.5 && y1 <= c.y + 1.0 && c.y - y1 < em * 1.5)
                || (c.y - lower_top > em * 3.0 && !lower.iter().any(|d: &Col| same_column(d)) && upper.iter().any(|u| same_column(u) && u.y1 <= c.y));
            if continues {
                spanning.push((center(&c), c.y1, em));
                upper.push(c);
            } else {
                lower.push(c);
            }
        }
        // 下の段が 1 列だけか、空きのすぐ下から始まらなければ段の境目ではない（字間を空けた標題の字、
        // 下に寄せた署名だけが下に残った）。
        let mut lower_columns: Vec<f32> = lower.iter().map(center).collect();
        lower_columns.sort_by(|a, b| a.total_cmp(b));
        lower_columns.dedup_by(|a, b| (*a - *b).abs() < 4.0);
        let starts_at_cut = lower.iter().any(|c| c.y - cut_y <= (c.x1 - c.x) * 3.0);
        if lower_columns.len() < 2 || !starts_at_cut {
            upper.append(&mut lower);
        }
        for part in [lower, upper] {
            if !part.is_empty() {
                self.trailing.insert(0, part);
            }
        }
    }

    /// 帯ごとに行へ組む（テキスト化は [`Unit::text`]）。帯の中の罫線の表は表として組む。
    pub(crate) fn into_unit_with_source(self,grids:&[Grid],rules:Option<&PageRules>,source:&[Vec<usize>])->Unit{self.into_unit_impl(grids,rules,Some(source))}
    pub(crate) fn into_unit(self,grids:&[Grid],rules:Option<&PageRules>)->Unit{self.into_unit_impl(grids,rules,None)}
    fn into_unit_impl(self, grids: &[Grid], rules: Option<&PageRules>,source:Option<&[Vec<usize>]>) -> Unit {
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
        // 表の外の段の罫線の表（帯の語を含まない枠。表の後ろの別の記事の表）も表に組む。号外第 164 号 56 ページ:
        // 新旧対照表の下の段の厚生労働省告示の年齢階層の表は、これまで改正前の帯に紛れて表になっていた。
        let outside_grids: Vec<&Grid> = self
            .trailing
            .iter()
            .flat_map(|part| {
                let banded = |g: &&Grid| all_after.iter().chain(all_before.iter()).any(|c| inside(g, c));
                inner.iter().copied().filter(move |g| !banded(g) && part.iter().any(|c| inside(g, c)) && crate::vertical::vertical_inside(part, g) && !crate::vertical::prose_cells(part, g))
            })
            .collect();
        // Geometry alone cannot promote the last note of a previous field into
        // this preamble. Require the complete original grid membership, with
        // every source occurrence owned here exactly once.
        let complete_preamble_grid=|g:&Grid|{
            let Some(source)=source.filter(|s|s.len()==grids.len())else{return false;};
            let Some(i)=grids.iter().position(|original|original==g)else{return false;};
            let expected:std::collections::BTreeSet<_>=source[i].iter().copied().collect();
            let actual:Vec<_>=self.preamble.iter().filter(|c|inside(g,c)).map(|c|c.id).collect();
            let own:std::collections::BTreeSet<_>=actual.iter().copied().collect();
            !expected.is_empty()&&expected.len()==source[i].len()&&own.len()==actual.len()&&own==expected
        };
        let preamble_grids:Vec<&Grid>=inner.iter().copied().filter(|g|
            complete_preamble_grid(g)&&self.preamble.iter().any(|c|inside(g,c))
            && !all_after.iter().chain(all_before.iter()).any(|c|inside(g,c))
            && crate::vertical::vertical_inside(&self.preamble,g)
            && !crate::vertical::prose_cells(&self.preamble,g)).collect();
        Unit::Shinkyu {
            labels:self.labels,
            preamble: lines_with_tables_by(self.preamble,&preamble_grids,page_lines),
            after: self.after.into_iter().map(|p| lines_with_tables(p, &after_grids)).collect(),
            before: self.before.into_iter().map(|p| lines_with_tables(p, &before_grids)).collect(),
            // 表の外の記事の変更後・変更前の枠は、上の欄・下の欄の順に読む（[`crate::vertical::split_label_frames`]）。
            outside: self
                .trailing
                .into_iter()
                .map(|p| {
                    let hrules: Vec<(f32, f32, f32)> = rules.map(|r| r.horizontal.iter().map(|h| (h.y, h.x0, h.x1)).collect()).unwrap_or_default();
                    let vrules: Vec<(f32, f32, f32)> = rules.map(|r| r.vertical.iter().map(|v| (v.x, v.y0, v.y1)).collect()).unwrap_or_default();
                    crate::vertical::split_label_frames(p, &hrules, &vrules)
                        .into_iter()
                        .flat_map(|part| lines_with_tables_by(part, &outside_grids, page_lines))
                        .collect()
                })
                .collect(),
        }
    }
}

/// 帯の語を、罫線の表に入るものと入らないものに分け、表は表として、残りは行として組み、
/// 表を読み順（右→左）の位置に差し込む。
pub(crate) fn lines_with_tables(cols: Vec<Col>, grids: &[&Grid]) -> Vec<Line> {
    lines_with_tables_by(cols, grids, cell_lines)
}

/// [`lines_with_tables`] の表の外の語の組み方を指定する版（普通の本文の段は [`page_lines`]）。
pub(crate) fn lines_with_tables_by(cols: Vec<Col>, grids: &[&Grid], rest_lines: impl Fn(Vec<Col>) -> Vec<Line>) -> Vec<Line> {
    let mut tables: Vec<Table> = Vec::new();
    let mut rest = cols;
    for g in grids {
        let inside = |c: &Col| g.contains((c.x + c.x1) / 2.0, (c.y + c.y1) / 2.0, 0.5);
        let (mine, others): (Vec<Col>, Vec<Col>) = rest.into_iter().partition(inside);
        rest = others;
        if !mine.is_empty() {
            let t = grid_table(g, mine.clone());
            // 1 セルだけ（枠で囲んだだけの文）は表にせず本文に戻す。2 マスの枠で字のあるセルが 1 つだけのもの（号外第
            // 160 号 59 ページ: 前のページから続く新旧対照表の欄の枠に、改正前の「〔項を加える。〕」だけ）も同じ。
            let filled = t.cells.iter().filter(|c| !c.text.trim().is_empty()).count();
            if t.rows * t.cols <= 1 || (t.rows * t.cols <= 2 && filled <= 1) {
                rest.extend(mine);
            } else {
                tables.push(t);
            }
        }
    }
    // 表の脇（左か右）の語が、表の高さの中ほどにある、どの語もまたがない横の空きで上下 2 つの段に分かれるなら、段ごとに
    // 組む（表の左の告示の本文が上下 2 段に組まれたもの。表の字が段の境をまたぐので段に分けられず、上の段の列と下の段の
    // 列が 1 行ずつつながっていた）。
    let cut = beside_table_cut(&rest, &tables);
    place_tables(rest, tables, |r: Vec<Col>| match cut {
        Some((y, left_edge)) => {
            // 表の右の語（先に読む）は上の段に入れる。
            let (top, bottom): (Vec<Col>, Vec<Col>) = r.into_iter().partition(|c| c.x1 > left_edge + 1.0 || (c.y + c.y1) / 2.0 < y);
            let mut lines = rest_lines(top);
            lines.extend(rest_lines(bottom));
            lines
        }
        None => rest_lines(r),
    })
}

/// [`lines_with_tables_by`] の表の左の語を上下の段に分ける y と表の左端。表の左の語の、表の上端と下端の間の横の空き
/// （どの語もまたがず、上下の語の間が字の半分以上空く）で、上下とも 2 字以上の縦組みの列が 3 列以上あるもの。
fn beside_table_cut(all: &[Col], tables: &[Table]) -> Option<(f32, f32)> {
    let t = tables.iter().min_by(|a, b| a.x0.total_cmp(&b.x0))?;
    let rest: Vec<Col> = all.iter().filter(|c| c.x1 <= t.x0 + 1.0).cloned().collect();
    let rest = rest.as_slice();
    if rest.is_empty() {
        return None;
    }
    let upright = |c: &Col| c.text.chars().count() >= 2 && c.y1 - c.y > (c.x1 - c.x) * 1.5;
    let em = rest.iter().filter(|c| upright(c)).map(|c| c.x1 - c.x).fold(f32::INFINITY, f32::min);
    if !em.is_finite() {
        return None;
    }
    let columns = |sel: &dyn Fn(&Col) -> bool| {
        let mut v: Vec<f32> = rest.iter().filter(|c| upright(c) && sel(c)).map(|c| (c.x + c.x1) / 2.0).collect();
        v.sort_by(f32::total_cmp);
        v.dedup_by(|a, b| (*a - *b).abs() < em * 0.5);
        v
    };
    // 上と下の段の列の x がほとんど揃わない（列の間隔の違う別の組。帯の表「都道府県名」「市区町村名」の上下の欄は
    // 列が揃う）。
    let misaligned = |up: &[f32], down: &[f32]| down.iter().filter(|&&x| up.iter().any(|&u| (u - x).abs() < 1.0)).count() * 2 < down.len();
    rest.iter()
        .map(|c| c.y1)
        .filter(|&y1| y1 > t.y0 + em * 3.0 && y1 < t.y1 - em * 3.0)
        .filter_map(|y1| {
            let next = rest.iter().filter(|c| c.y >= y1).map(|c| c.y).fold(f32::INFINITY, f32::min);
            let cut = (y1 + next) / 2.0;
            let (up, down) = (columns(&|c: &Col| c.y1 <= cut), columns(&|c: &Col| c.y >= cut));
            (next - y1 >= em * 0.5 && !rest.iter().any(|c| c.y < cut && cut < c.y1) && up.len() >= 3 && down.len() >= 3 && misaligned(&up, &down))
                .then_some((next - y1, cut))
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, cut)| (cut, t.x0))
}

/// 表の外の語 `rest` を `rest_lines` で行に組み、表を読み順（右→左）の位置に差し込む。
pub(crate) fn place_tables(rest: Vec<Col>, mut tables: Vec<Table>, rest_lines: impl Fn(Vec<Col>) -> Vec<Line>) -> Vec<Line> {
    let geom: std::collections::HashMap<usize, (f32, f32)> = rest.iter().map(|c| (c.id, ((c.x + c.x1) / 2.0, c.y))).collect();
    let mut lines = rest_lines(rest);
    // 右端の x が大きい表から、その表より左にある最初の行（行の中心が表の左端より左）か、表の
    // 下にある最初の行の手前へ差し込む（表の左の罫線に接して組まれた次の記事の標題や、表の下の
    // 備考を表の前に出さない）。
    // 左右の範囲のほとんど重なる表（横倒しのページで上下に並んだ座標の表）は上の表から。
    tables.sort_by(|a, b| b.x1.total_cmp(&a.x1));
    let mut stack: Vec<(usize, f32, f32)> = Vec::new(); // (組, 左端, 右端)
    let mut keyed: Vec<((usize, f32), Table)> = Vec::new();
    for t in tables {
        let overlaps = |&(_, x0, x1): &(usize, f32, f32)| (t.x1.min(x1) - t.x0.max(x0)) > (t.x1 - t.x0).min(x1 - x0) * 0.5;
        let group = match stack.iter().find(|s| overlaps(s)) {
            Some(&(g, _, _)) => g,
            None => {
                stack.push((stack.len(), t.x0, t.x1));
                stack.len() - 1
            }
        };
        keyed.push(((group, t.y0), t));
    }
    keyed.sort_by(|a, b| a.0 .0.cmp(&b.0 .0).then(a.0 .1.total_cmp(&b.0 .1)));
    let tables: Vec<Table> = keyed.into_iter().map(|(_, t)| t).collect();
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
pub(crate) fn grid_table(g: &Grid, cols: Vec<Col>) -> Table {
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
    // 帯ごとの行の頭の列の x と、数の欄で行を決めたか。
    let strip_info: Vec<(Vec<Vec<f32>>, bool)> = (0..nx)
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
            // 項目の番号で始まる行の頭が無くても、帯の中に字がどれも短い数（「一」「二十」）だけの欄（必要個数の欄）が
            // あれば、その欄に字のある列を行の頭にする（行の罫線の無い表。数の欄は折り返さない）。
            let mut counted = false;
            let tops: Vec<Option<f32>> = if tops.iter().all(Option::is_none) {
                // 個数の数え方（「一」「五」「二十」「十五」）。位取りの数字（無線の周波数の「一三」）は除く。
                let numeral = |t: &str| {
                    let n = t.chars().count();
                    (1..=3).contains(&n)
                        && t.chars().all(|c| "一二三四五六七八九十百".contains(c))
                        && (n == 1 || t.contains(['十', '百']))
                };
                // 数の欄の字は、どれも 1 つの列に 1 つだけで、その列のほかの欄にも字がある（行の頭の列）。
                let count_field = single.iter().copied().find(|&i| {
                    let ws = &cell_words[i];
                    let fy = Some(g.cells[i].y0);
                    let one_each = groups.iter().all(|grp| {
                        let n = grp.iter().filter(|c| field_of(c) == fy).count();
                        n == 0 || (n == 1 && grp.iter().any(|c| field_of(c) != fy))
                    });
                    ws.len() >= 3 && ws.iter().all(|c| numeral(&c.text)) && fy != top_field && one_each
                });
                counted = count_field.is_some();
                match count_field {
                    Some(f) => groups
                        .iter()
                        .map(|grp| {
                            let fy = g.cells[f].y0;
                            grp.iter().any(|c| field_of(c) == Some(fy)).then(|| grp.iter().map(|c| c.y).fold(f32::INFINITY, f32::min)).map(|_| 0.0)
                        })
                        .collect(),
                    None => tops,
                }
            } else {
                tops
            };
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
            (records, counted)
        })
        .collect();
    let strip_records: Vec<Vec<Vec<f32>>> = strip_info.iter().map(|(r, _)| r.clone()).collect();
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
            // 数だけの短い列が並ぶ欄（無線の通信路の「一二」「一四」「六六」）は、列ごとに別の項目。
            let numeral = |t: &str| {
                let t = t.trim();
                (1..=4).contains(&t.chars().count()) && t.chars().all(|c| "〇一二三四五六七八九十百".contains(c) || c.is_ascii_digit() || ('０'..='９').contains(&c))
            };
            let numbers = lines.get(i + 1).is_some_and(|n| numeral(&l.text) && numeral(&n.text));
            if i + 1 < lines.len() && (l.text.ends_with('。') || short || spaced(&l.text) || numbers) {
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
            // 数の欄で行を決めた帯で、初めの数行にだけ字のある欄（「省令別表／第一に掲／げる救助／器具」の区分の欄）は、
            // 行をまたぐ 1 つのセルの折り返しなので、最初の行にまとめる。
            let filled = per.iter().take_while(|ws| !ws.is_empty()).count();
            if strip_info[s_right].1 && filled >= 2 && filled * 3 <= per.len() && per[filled..].iter().all(Vec::is_empty) {
                let rest: Vec<Col> = per[1..filled].iter_mut().flat_map(std::mem::take).collect();
                per[0].extend(rest);
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
    use super::kanji_number;
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
    fn reads_kanji_numbers() {
        for (s, n) in [("二十八", 28), ("百五十六", 156), ("千二百三十七", 1237), ("十", 10), ("三", 3), ("二〇", 20)] {
            assert_eq!(kanji_number(s), Some(n), "{s}");
        }
        assert_eq!(kanji_number(""), None);
    }

    #[test]
    fn headerless_table_may_renumber_kana_items() {
        // 改正後のイと改正前のロが対応する。上帯の末尾がカタカナでも、下帯は号見出しから
        // 始まるので、前の語の続きとして扱わない。
        use crate::vertical::Col;
        let mut cols = Vec::new();
        for (y, label) in [(80.0, "イ"), (450.0, "ロ")] {
            for (x, offset, end, text) in [
                (540.0, 0.0, 8.0, label),
                (540.0, 16.0, 320.0, "事業を継続するために必要な措置を実施するものとする。"),
                (520.0, 0.0, 320.0, "事業を実施するときは計画を策定し適切な実施体制を整備する。"),
                (500.0, 0.0, 320.0, "事業計画に基づく取組を実施した者はその結果を報告する。"),
                (480.0, 0.0, 320.0, "事業の種別については次のとおりとするカタカナ"),
            ] {
                cols.push(Col { id: cols.len(), x, x1: x + 8.0, y: y + offset, y1: y + end, text: text.into() });
            }
        }
        assert!(super::detect_shinkyu_bands_headerless(&cols, 842.0).is_some());
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
    fn note_cell_spanning_both_bands_stays_in_one_piece() {
        // 実ページ（総務省令の新旧対照表）と同じ形: 外枠 x 74〜333.5・y 69.5〜412.5 の左端に、帯の境目
        // （y 241）の横線が届かない幅 37pt の欄（x 74〜111.25）があり、表の後ろの備考が上下の帯を
        // またいで 1 行に組まれている。備考は割らずに改正後の帯の終わりに置く。
        use super::ShinkyuLayout;
        use crate::grid::find_grids;
        use crate::lines::{HRule, PageRules, VRule};
        use crate::trace::Unit;
        use crate::vertical::Col;
        let rules = PageRules {
            enclosures: Vec::new(), short_fraction_bars: Vec::new(),
            vertical: [74.0, 111.25, 310.9, 333.5].iter().map(|&x| VRule { x, y0: 69.5, y1: 412.5 }).collect(),
            horizontal: vec![
                HRule { y: 69.5, x0: 74.0, x1: 333.5 },
                HRule { y: 241.0, x0: 111.25, x1: 333.5 },
                HRule { y: 412.5, x0: 74.0, x1: 333.5 },
            ],
        };
        let grids = find_grids(&rules, 595.0, 842.0);
        let col = |id: usize, x: f32, y: f32, y1: f32, text: &str| Col { id, x, x1: x + 8.0, y, y1, text: text.to_string() };
        let cols = vec![
            col(0, 200.0, 75.0, 139.0, "改正後の本文"),
            col(1, 200.0, 250.0, 314.0, "改正前の本文"),
            // 備考の行（「表中の〔」と「〕の記載…」の間が 1em 空く）。後ろの語は帯の境目をまたぎ、
            // 中心は下の帯にある。
            col(2, 100.0, 75.0, 123.0, "備考表中の〔"),
            col(3, 100.0, 131.0, 398.0, "〕の記載は注記である。"),
        ];
        let mut layout = ShinkyuLayout::split(cols, 315.0, 241.0);
        layout.clip_to_frame(&grids, 315.0, 241.0);
        let Unit::Shinkyu { after, before, outside, .. } = layout.into_unit(&grids, Some(&rules)) else { panic!() };
        let text = |parts: &[Vec<crate::trace::Line>]| parts.iter().flatten().map(|l| l.text.clone()).collect::<Vec<_>>().join("\n");
        assert_eq!(text(&after), "改正後の本文\n備考表中の〔　〕の記載は注記である。");
        assert_eq!(text(&before), "改正前の本文");
        assert_eq!(text(&outside), "");
    }

    /// 号外第172号275ページの形: 新旧対照表の前文（x 310・帯の境目 294.8 をまたぐ）の右に、上の段に前の記事の末尾と
    /// 次の記事（甲）の標題・前文、下の段に甲の本文が組まれている。前の記事の「（注）…については、」の行は、`hang` だけ
    /// 段の字詰めの端（292）の外に出る。`lower` でなければ甲の本文を下の段に置かない。
    fn two_tier_beside_preamble(hang: f32, lower: bool) -> (Vec<crate::trace::Unit>, Vec<String>) {
        use super::ShinkyuLayout;
        use crate::vertical::Col;
        let mut cols: Vec<Col> = Vec::new();
        let mut col = |x: f32, y: f32, y1: f32, text: &str| {
            cols.push(Col { id: cols.len(), x, x1: x + 8.0, y, y1, text: text.to_string() })
        };
        col(310.0, 68.0, 580.0, "表の記事の前文");
        col(322.0, 60.0, 164.0, "〇乙省告示第二号");
        col(400.0, 68.0, 292.0, "前の記事の本文の続き");
        col(388.0, 68.0, 292.0 + hang, "（注）この指針の用語については、");
        col(376.0, 76.0, 132.0, "その例による。");
        col(364.0, 60.0, 164.0, "〇甲省告示第一号");
        col(352.0, 68.0, 292.0, "甲の前文");
        col(340.0, 76.0, 196.0, "令和八年七月三十一日");
        if lower {
            for (i, x) in [400.0, 388.0, 376.0, 364.0, 352.0, 340.0].into_iter().enumerate() {
                col(x, 300.0, 580.0, &format!("甲の本文{i}"));
            }
        }
        let after = vec![Col { id: 100, x: 250.0, x1: 258.0, y: 60.0, y1: 200.0, text: "改正後の本文".into() }];
        let before = vec![Col { id: 101, x: 250.0, x1: 258.0, y: 310.0, y1: 450.0, text: "改正前の本文".into() }];
        let mut layout = ShinkyuLayout { labels: None, band_boxes: None, preamble: cols, after: vec![after], before: vec![before], trailing: vec![] };
        let units = layout.split_off_leading_continuation(294.8, &[], None);
        let mut preamble: Vec<&Col> = layout.preamble.iter().collect();
        preamble.sort_by(|a, b| b.x.total_cmp(&a.x));
        (units, preamble.iter().map(|c| c.text.clone()).collect())
    }

    fn unit_lines(units: &[crate::trace::Unit]) -> Vec<String> {
        units
            .iter()
            .flat_map(|u| match u {
                crate::trace::Unit::Plain(lines) => lines.iter().map(|l| l.text.clone()).collect::<Vec<_>>(),
                _ => panic!("plain only"),
            })
            .collect()
    }

    #[test]
    fn a_two_tier_article_beside_the_preamble_is_read_by_tiers_before_it() {
        let (units, preamble) = two_tier_beside_preamble(4.0, true);
        let mut expected: Vec<String> = ["前の記事の本文の続き", "（注）この指針の用語については、", "その例による。", "〇甲省告示第一号", "甲の前文", "令和八年七月三十一日"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        expected.extend((0..6).map(|i| format!("甲の本文{i}")));
        assert_eq!(unit_lines(&units), expected);
        assert_eq!(preamble, ["〇乙省告示第二号", "表の記事の前文"]);
    }

    #[test]
    fn a_line_crossing_the_divider_beyond_its_last_mark_still_spans_it() {
        // 半字分より深く境目を越える列は、行末が「、」でも両帯をまたぐ前文の列（前文の右端はその列）。
        let (_, preamble) = two_tier_beside_preamble(8.0, true);
        assert!(preamble.contains(&"（注）この指針の用語については、".to_string()), "{preamble:?}");
    }

    #[test]
    fn a_heading_without_a_lower_tier_beside_it_still_heads_the_preamble() {
        // 標題の左の下の段に続きが無ければ、上の段の最初の標題から左はこれまでどおり前文に回す。
        let (units, preamble) = two_tier_beside_preamble(4.0, false);
        assert_eq!(unit_lines(&units), ["前の記事の本文の続き", "（注）この指針の用語については、", "その例による。"]);
        assert_eq!(preamble, ["〇甲省告示第一号", "甲の前文", "令和八年七月三十一日", "〇乙省告示第二号", "表の記事の前文"]);
    }
}

#[cfg(test)]
#[path = "leading_numeric_table_tests.rs"]
mod leading_numeric_table_tests;

#[cfg(test)]
#[path="shared_cell_controls138.rs"]
mod shared_cell_controls;

#[cfg(test)]#[path="preamble_grid_source_tests147.rs"]mod preamble_grid_source_tests147;
