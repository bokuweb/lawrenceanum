//! `pdftotext -bbox-layout` と同じ形の XHTML を作る（既存の kanpo-parser への橋渡し）。
//!
//! 文字を語にまとめ、語の枠は poppler の出力に合わせる。poppler の挙動を外から観察して
//! 合わせたもので、poppler のコードは使っていない。
//!
//! - 横書きの語: 字面の枠（FontDescriptor の Ascent〜Descent）をつないだもの
//! - 縦書きの語: 文字の原点（字の上端の中央）を左上とする正方形をつないだもの。実際の字面より
//!   右へ半 em、上へ 1 em ずれる（kanpo-parser はこのずれを前提に補正している。読み順の復元では
//!   上下だけ、表示用の語の座標では左右も戻す）
//! - poppler が読めないフォント（[`poppler_readable`]）の文字は出さない
//! - ToUnicode の無い `Identity` の CID フォントの字は、CID を私用領域 U+F0000 + CID に置く
//!   （[`IDENTITY_CID_BASE`]。poppler は CID をそのまま符号位置として無関係な漢字を出す）
//! - 同じ字がほぼ同じ位置に重ねて描かれていたら 2 つ目は捨てる。横書きで別の字が前の字と同じ位置に
//!   重ねて置かれた私用領域の字は、その字だけで 1 語にする
//! - 縦書きはフォントが変わると語を切る。字間（Tc）で空けた字は別の語にする
//! - 横書きでも、字間（Tc）だけで空けた字は 0.55 em まで同じ語にする（字間を空けた見出し・表の欄の名前）
//! - 字と字の間に罫線（字送りに直交する線）があれば語を切る（表のセルの境目）

use std::fmt::Write;

use crate::page::{FontInfo, Glyph, Page};
use crate::rules::{page_rules, PageRules};

/// 語。
#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    pub text: String,
    pub x_min: f64,
    pub y_min: f64,
    pub x_max: f64,
    pub y_max: f64,
    /// 語を作った文字（[`Page::glyphs`] の添字）。
    pub glyphs: Vec<usize>,
}

/// 文字が語の続きとみなせる、字送りの方向の空きの許容（字の大きさに対する比）。
/// 横書きは 0.2 em 程度の字間までを同じ語にする。官報 296 ページの poppler の出力で、
/// 0.19 em の空きはほぼつなぎ、0.22 em 以上はほぼ切っていた。
const GAP: f64 = 0.21;
/// 縦書きは均等割りの見出し（「変更した事項」）など、0.35 em 程度の字間までを同じ語にする。
const GAP_VERTICAL: f64 = 0.35;
/// 前の字に重なって置かれた字も続きとみなす重なりの許容（字の大きさに対する比）。約物の詰め
/// （半 em）や、私用 CID の 2 字を同じ位置に重ねて 1 字を作る組み方（1 em）がある。
const OVERLAP: f64 = 1.05;
/// 字送りに直交する方向のずれの許容（字の大きさに対する比）。
const OFFSET: f64 = 0.1;
/// 字間（Tc）だけで空けた字を同じ語にする空きの上限（字の大きさに対する比）。
const TC_MAX: f64 = 0.55;
/// 字の間の罫線で語を切るのは、この空き（字の大きさに対する比）より空いているときだけ。詰めて
/// 組んだ字の境目に重なる線（クリップされて見えない線など）では切らない。
const RULE_GAP: f64 = 0.05;

fn poppler_box(g: &Glyph) -> [f64; 4] {
    if !g.vertical {
        return g.bbox;
    }
    // 字送りの向き u と、それを右に 90° 回した向き r（u が下向きなら r は右向き）で、
    // 原点 + [0, size]·r + [-size, 0]·u の正方形。回転ページでも同じ式で合う。
    let (ax, ay) = (g.end_x - g.x, g.end_y - g.y);
    let len = ax.hypot(ay);
    let (ux, uy) = if len > 1e-9 { (ax / len, ay / len) } else { (0.0, 1.0) };
    let (rx, ry) = (uy, -ux);
    let s = g.size;
    let pts = [(0.0, -s), (s, -s), (0.0, 0.0), (s, 0.0)].map(|(a, b)| (g.x + a * rx + b * ux, g.y + a * ry + b * uy));
    let xs = pts.map(|p| p.0);
    let ys = pts.map(|p| p.1);
    [
        xs.iter().copied().fold(f64::INFINITY, f64::min),
        ys.iter().copied().fold(f64::INFINITY, f64::min),
        xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        ys.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    ]
}

/// XHTML の語の枠を、PDF の実際のグリフの枠へ合わせる。
/// 縦書きフォントの poppler 互換の枠は字面からずれる。ページ全体の向きではなく、
/// 個々のグリフの座標を使うため、回転・縦横混在でも同じように補正できる。
/// 語の切り方が抽出器によって異なっていても、含まれるグリフで照合する。
/// 照合できない語は推測せず None を返す。
pub fn display_boxes(page: &Page, boxes: &[[f64; 4]]) -> Vec<Option<[f64; 4]>> {
    let mut indices: Vec<_> = words(page).into_iter().flat_map(|w| w.glyphs).collect();
    indices.sort_unstable();
    indices.dedup();
    let glyphs: Vec<_> = indices.into_iter().map(|i| (poppler_box(&page.glyphs[i]), page.glyphs[i].bbox)).collect();
    let union = |a: [f64; 4], b: [f64; 4]| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])];
    boxes.iter().map(|source| {
        // poppler は途中で基線がわずかに動いても、語の先頭の横位置を使うことがある。
        // 丸め誤差とこの差を許容する（隣の列の文字までは含めない）。
        const TOLERANCE: f64 = 0.25;
        let contained = |b: &[f64; 4]| b[0] >= source[0] - TOLERANCE && b[1] >= source[1] - TOLERANCE
            && b[2] <= source[2] + TOLERANCE && b[3] <= source[3] + TOLERANCE;
        let mut bounds: Option<([f64; 4], [f64; 4])> = None;
        for (raw, display) in &glyphs {
            if contained(raw) {
                bounds = Some(match bounds {
                    None => (*raw, *display),
                    Some((a, b)) => (union(a, *raw), union(b, *display)),
                });
            }
        }
        bounds.filter(|(raw, _)| raw.iter().zip(source).all(|(a, b)| (a - b).abs() < TOLERANCE)).map(|(_, display)| display)
    }).collect()
}

/// poppler が文字として出すフォントか。poppler は対応表を持たない CID の文字集合
/// （Adobe-Npb1 など）で ToUnicode も無いフォントを読めずに捨てる。官報では傍線などの
/// 罫線用の字形のフォントがこれにあたる。
pub fn poppler_readable(font: &FontInfo) -> bool {
    match font.ordering.as_deref() {
        None => true,
        Some("Japan1" | "Japan2" | "GB1" | "CNS1" | "Korea1" | "KR" | "Identity") => true,
        Some(_) => font.has_to_unicode,
    }
}

/// ToUnicode の無い `Identity` の CID フォントの字を置く私用領域の先頭（U+F0000 + CID）。
///
/// 官報では項目記号（(1)・①・㈠）や目次のページ番号の数字などが、ToUnicode の無い
/// `DFHSMinchoRPro6N-W3-Identity-V`（Ordering が `Identity`）で組まれる。CID はフォントの字形の番号で
/// 文字の対応は無い。poppler のように CID をそのまま符号位置にすると本物の漢字（`掠`・`採`）と
/// 区別がつかないので、私用領域に置いて利用側（kanpo-parser）が字形の番号から字を決められるようにする。
pub const IDENTITY_CID_BASE: u32 = 0xF0000;

/// 語に入れる文字列。
pub(crate) fn glyph_text(g: &Glyph, fonts: &[FontInfo]) -> String {
    let unmapped = fonts.get(g.font).is_some_and(|f| f.ordering.as_deref() == Some("Identity") && !f.has_to_unicode);
    match char::from_u32(IDENTITY_CID_BASE + g.cid) {
        Some(c) if unmapped && g.cid <= 0xFFFD => c.to_string(),
        _ => g.text.clone(),
    }
}

/// 文字を語にまとめる。文字はコンテンツストリームの順に見て、前の文字と同じフォント・大きさで、
/// 前の文字の字送りの終点に続いて（重なりは 1 em まで）置かれていれば同じ語にする。
/// 空白文字は語を区切る。
pub fn words(page: &Page) -> Vec<Word> {
    let mut out: Vec<Word> = Vec::new();
    let mut prev: Option<&Glyph> = None;
    // 重ねて置かれた字の次の字は、新しい語にする。
    let mut break_next = false;
    let readable: Vec<bool> = page.fonts.iter().map(poppler_readable).collect();
    let rules = page_rules(page);
    for (i, g) in page.glyphs.iter().enumerate() {
        if !readable.get(g.font).copied().unwrap_or(true) {
            // 捨てた文字の前後は別の語になる。
            prev = None;
            continue;
        }
        if g.text.chars().all(char::is_whitespace) && !g.text.is_empty() {
            prev = None;
            continue;
        }
        if prev.is_some_and(|p| duplicate(p, g)) {
            continue;
        }
        // 横書きで私用領域の字を前の字と同じ位置に重ねたもの（私用 CID の 2 字で 1 字を作る組み方）は、
        // その字だけで 1 語にする（poppler と同じ。縦書きは poppler も切らない）。
        let overlap = !g.vertical && prev.is_some_and(|p| overlaps(p, g));
        // 横書きの負の印「△」（私用 CID の字も）は、数の後ろに続くなら語の頭にする。表の欄の左端に寄せた印が、左の欄の
        // 金額のすぐ右に続いて置かれ、左の欄の金額の語に入っていた（「323,360,778△」）。
        // 前の字が数字・桁区切り・「－」のとき（表の金額の欄の並び）だけ。文の中の「（△は減）」は切らない。
        // 次の字が数字のとき（科目のすぐ右に置いた印「利益剰余金△1,053,214」）も、印は次の金額の頭。
        // 前の字は科目の漢字（文の中の「うち、△12,091,223円は」は切らない）。
        let digit_next = page.glyphs.get(i + 1).is_some_and(|n| !n.vertical && n.text.chars().next().is_some_and(|c| c.is_ascii_digit()))
            && prev.is_some_and(|p| p.text.chars().last().is_some_and(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c)));
        let sign = !g.vertical
            && matches!(g.text.as_str(), "\u{E888}" | "△" | "▲")
            && (digit_next || prev.is_some_and(|p| p.text.chars().all(|c| c.is_ascii_digit() || matches!(c, ',' | '.' | '\u{E61C}' | '－'))));
        let forced_break = std::mem::replace(&mut break_next, overlap) || overlap || sign;
        let continues = !forced_break && prev.is_some_and(|p| {
            // 縦書きはフォントが変わると語を切る（横書きは切らない）。poppler の出力に合わせた。
            if (g.vertical && p.font != g.font)
                || p.vertical != g.vertical
                || (p.size - g.size).abs() > 0.01 * g.size.max(p.size)
            {
                return false;
            }
            let (dx, dy) = (g.x - p.end_x, g.y - p.end_y);
            let (along, across) = advance_axes(p, dx, dy);
            // 横書きでフォントが変わるところは、前の字に重ねて置かれていれば切る（縦中横の数字の
            // 間に別のフォントの記号を重ねる組み方など）。
            if p.font != g.font && along < -0.01 * g.size {
                return false;
            }
            // 横書きで数字の後に空いて続く漢字・仮名は別の語（T 字の表の金額「100,068」と右の欄の科目「流動負債」）。
            // 数の後に詰めて続く字（「８条」「1,000円」）は空かない。
            if !g.vertical
                && along > 0.15 * g.size
                && p.text.chars().all(|c| c.is_ascii_digit())
                && g.text.chars().next().is_some_and(|c| ('\u{3040}'..='\u{30FF}').contains(&c) || ('\u{4E00}'..='\u{9FFF}').contains(&c))
            {
                return false;
            }
            let gap = if g.vertical { GAP_VERTICAL } else { GAP };
            let gap = if letter_spaced(p, g, along) { gap.max(TC_MAX) } else { gap };
            (-OVERLAP * g.size..=gap * g.size).contains(&along)
                && across.abs() <= OFFSET * g.size
                && !(along > RULE_GAP * g.size && rule_between(&rules, p, g))
        });
        let b = poppler_box(g);
        let text = glyph_text(g, &page.fonts);
        match out.last_mut() {
            Some(w) if continues => {
                w.text.push_str(&text);
                // 語の枠は字送りの向きにだけ伸ばす（直交する向きは先頭の字の枠のまま）。
                if (g.end_x - g.x).abs() >= (g.end_y - g.y).abs() {
                    w.x_min = w.x_min.min(b[0]);
                    w.x_max = w.x_max.max(b[2]);
                } else {
                    w.y_min = w.y_min.min(b[1]);
                    w.y_max = w.y_max.max(b[3]);
                }
                w.glyphs.push(i);
            }
            _ => out.push(Word {
                text,
                x_min: b[0],
                y_min: b[1],
                x_max: b[2],
                y_max: b[3],
                glyphs: vec![i],
            }),
        }
        prev = Some(g);
    }
    out
}

/// 字間（Tc）だけで空けて続く字か（TJ の数値で空けておらず、空きが前の字の字間とちょうど同じ）。
/// 字間を空けて組んだ語（横書きの見出し「振　替　法　の　適　用　等」が 0.33 em、決算公告の
/// 「純　資　産　合　計」が 0.25 em）は、普通の字間より空いていても 1 語にする。見出しの最後の字だけ
/// 別の演算子で描かれていることがある（「肥　料　の　名　称」の「称」）ので、演算子は問わない。
fn letter_spaced(p: &Glyph, g: &Glyph, along: f64) -> bool {
    let tol = 0.01 * g.size;
    g.kern.abs() < tol && p.spacing > tol && (along - p.spacing).abs() < tol
}

/// 前の字の送りの終点と次の字の原点の間を、字送りに直交する罫線が横切っているか。表のセルの
/// 境目で、前のセルの最後の字と次のセルの最初の字が詰めて置かれていることがある（縦書きの表の
/// 「静　岡」と次の欄の「清水港…」の間が 0.25 em）。
fn rule_between(rules: &PageRules, p: &Glyph, g: &Glyph) -> bool {
    // 罫線の太さと座標の丸めの分の余裕（pt）。
    const PAD: f64 = 0.5;
    let between = |a: f64, b: f64, v: f64| a.min(b) - PAD < v && v < a.max(b) + PAD;
    let (ax, ay) = (p.end_x - p.x, p.end_y - p.y);
    if ay.abs() > ax.abs() {
        // 縦に送る字: 横線。字の幅の中心を通る線だけを見る。
        let x = (g.bbox[0] + g.bbox[2]) / 2.0;
        rules
            .horizontal
            .iter()
            .any(|h| between(p.end_y, g.y, h.y as f64) && (h.x0 as f64) < x && x < h.x1 as f64)
    } else {
        // 横に送る字: 縦線。
        let y = (g.bbox[1] + g.bbox[3]) / 2.0;
        rules
            .vertical
            .iter()
            .any(|v| between(p.end_x, g.x, v.x as f64) && (v.y0 as f64) < y && y < v.y1 as f64)
    }
}

/// 直前の字と同じ字がほぼ同じ位置（字の大きさの 1 割以内）に重ねて描かれているか。
/// 重ね描き（太字の代わりなど）の 2 つ目は poppler と同じく語に入れない。
fn duplicate(p: &Glyph, g: &Glyph) -> bool {
    const DUP: f64 = 0.1;
    p.text == g.text && (p.x - g.x).abs() < DUP * g.size && (p.y - g.y).abs() < DUP * g.size
}

/// 私用領域の字を、前の字と同じ位置から重ねて書いたか（私用 CID の 2 字を重ねて 1 字を作る組み方）。
/// 「）（」のような約物を同じ位置に重ねる詰めは含まない（poppler もつなぐ）。
fn overlaps(p: &Glyph, g: &Glyph) -> bool {
    let private = |t: &str| t.chars().any(|c| ('\u{E000}'..='\u{F8FF}').contains(&c));
    let (along, across) = advance_axes(p, g.x - p.x, g.y - p.y);
    private(&g.text) && along.abs() < 0.1 * g.size && across.abs() < 0.2 * g.size
}

/// 字送りの方向の成分と、それに直交する成分。
fn advance_axes(p: &Glyph, dx: f64, dy: f64) -> (f64, f64) {
    let (ax, ay) = (p.end_x - p.x, p.end_y - p.y);
    let len = (ax * ax + ay * ay).sqrt();
    if len < 1e-9 {
        return (dx.hypot(dy), 0.0);
    }
    let (ux, uy) = (ax / len, ay / len);
    (dx * ux + dy * uy, -dx * uy + dy * ux)
}

/// Keep the historical Poppler-compatible rectangle while exposing the
/// original glyph rectangle in display/page coordinates and its writing direction.
fn native_vertical_attrs(page: &Page, word: &Word) -> String {
    let Some(first) = word.glyphs.first().and_then(|&i| page.glyphs.get(i)) else { return String::new() };
    if !first.vertical {
        return if word.glyphs.iter().all(|&i| page.glyphs.get(i).is_some_and(|g| !g.vertical)) { " data-wmode=\"0\"".into() } else { String::new() };
    }
    if !word.glyphs.iter().all(|&i| page.glyphs.get(i).is_some_and(|g| g.vertical)) { return String::new(); }
    let mut b = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
    for &i in &word.glyphs {
        let g = &page.glyphs[i];
        b[0] = b[0].min(g.bbox[0]); b[1] = b[1].min(g.bbox[1]);
        b[2] = b[2].max(g.bbox[2]); b[3] = b[3].max(g.bbox[3]);
    }
    format!(" data-wmode=\"1\" data-advance-x=\"{:.6}\" data-advance-y=\"{:.6}\" data-native-xmin=\"{:.6}\" data-native-ymin=\"{:.6}\" data-native-xmax=\"{:.6}\" data-native-ymax=\"{:.6}\"",
        first.end_x-first.x, first.end_y-first.y, b[0],b[1],b[2],b[3])
}

/// Prove Unicode resolution from each original AJ1 CID, rather than treating
/// an arbitrary CJK scalar as a legacy Identity CID. A partial ToUnicode map
/// or its presence alone is insufficient: the actual emitted text must equal
/// the existing AJ1 mapping. No public Glyph/Word fields are changed.
fn resolved_aj1_word(page: &Page, word: &Word) -> bool {
    !word.glyphs.is_empty() && word.glyphs.iter().all(|&id| {
        page.glyphs.get(id).is_some_and(|g| {
            page.fonts.get(g.font).is_some_and(|f| f.ordering.as_deref() == Some("Japan1"))
                && crate::font::aj1(g.cid).is_some_and(|c| g.text.chars().eq(std::iter::once(c)))
        })
    })
}

/// ページを `pdftotext -bbox-layout` 形式の XHTML にする。語ごとに 1 つの flow・block・line に入れる。
pub fn bbox_layout(pages: &[Page]) -> String {
    layout(pages.iter().map(|p| (p,None,None)), false)
}
/// Render only owned original pages with their own source evidence.
/// There is no external Page argument to which evidence could be transferred.
///
/// The old paired-slices call cannot attach one extraction's evidence to
/// another native page:
///
/// ```compile_fail
/// let pages: &[kanpo_pdf::Page] = &[];
/// let sources: &[kanpo_pdf::Aj1PageSources] = &[];
/// let _ = kanpo_pdf::bbox::bbox_layout_with_aj1_sources(pages, sources);
/// ```
pub fn bbox_layout_with_aj1_sources(sources: &[crate::Aj1PageSources]) -> String {
    layout(sources.iter().map(|s| (s.page(),Some(s.resolved()),None)), false)
}
/// Original source font/size identities. These are provenance, not inferred boldness.
/// Each style is bound to the glyphs of its own emitted word.
pub fn bbox_layout_with_native_styles(sources: &[crate::Aj1PageSources]) -> String {
    layout(sources.iter().map(|s| (s.page(), Some(s.resolved()),Some(s.ink_rects()))), true)
}
fn native_styles(page: &Page, word: &Word) -> Option<String> {
    let mut styles = std::collections::BTreeSet::new();
    for &id in &word.glyphs {
        let g = page.glyphs.get(id)?;
        if !g.size.is_finite() || g.size <= 0.0 || page.fonts.get(g.font).is_none() {
            return None;
        }
        styles.insert((g.font, g.size.to_bits()));
    }
    if styles.is_empty() {
        return None;
    }
    Some(
        styles
            .into_iter()
            .map(|(f, b)| format!("{f}:{b:016x}"))
            .collect::<Vec<_>>()
            .join(" "),
    )
}
/// Source fonts of alphanumeric glyphs, without treating punctuation fonts
/// as content typography. The complete raw word must be reproduced first.
fn native_content_styles(page: &Page, word: &Word, resolved: Option<&[Option<char>]>) -> Option<String> {
    native_styles(page, word)?;
    if word.glyphs.windows(2).any(|pair| pair[0] >= pair[1]) { return None; }
    let candidate = resolved.and_then(|s| crate::source_aj1::word_candidate(page, word, s));
    let mut candidate_chars = candidate.as_deref().map(str::chars);
    let mut raw = String::new();
    let mut content = std::collections::BTreeSet::new();
    for &id in &word.glyphs {
        let g = page.glyphs.get(id)?;
        let font = page.fonts.get(g.font)?;
        let emitted = glyph_text(g, &page.fonts);
        raw.push_str(&emitted);
        let semantic = if font.ordering.as_deref() == Some("Identity") && !font.has_to_unicode {
            candidate_chars.as_mut()?.next()?.to_string()
        } else {
            emitted
        };
        // Opaque private glyph codes do not establish a Unicode character class.
        if semantic.is_empty() || semantic.chars().any(|c| c.is_control() || matches!(c,
            '\u{e000}'..='\u{f8ff}' | '\u{f0000}'..='\u{ffffd}' | '\u{100000}'..='\u{10fffd}')) { return None; }
        if semantic.chars().any(char::is_alphanumeric) { content.insert((g.font, g.size.to_bits())); }
    }
    if raw != word.text || candidate_chars.as_mut().is_some_and(|cs| cs.next().is_some()) { return None; }
    Some(content.into_iter().map(|(f,b)| format!("{f}:{b:016x}")).collect::<Vec<_>>().join(" "))
}
/// Source advances certify a displayed reading axis independently of font mode
/// and word aspect. No rounding or tolerance can turn a diagonal into an axis.
fn native_source_axis(page: &Page, word: &Word) -> Option<&'static str> {
    native_styles(page, word)?;
    if word.glyphs.windows(2).any(|p| p[0] >= p[1]) { return None; }
    let mut axis = None;
    let mut raw = String::new();
    for &id in &word.glyphs {
        let glyph = page.glyphs.get(id)?;
        if glyph.invisible || ![glyph.x,glyph.y,glyph.end_x,glyph.end_y].iter().all(|x|x.is_finite()) { return None; }
        let (x,y) = (glyph.end_x-glyph.x,glyph.end_y-glyph.y);
        let direction = match (x,y) {
            (x,0.) if x.is_finite() && x>0. => "right",
            (0.,y) if y.is_finite() && y>0. => "down",
            (x,0.) if x.is_finite() && x<0. => "left",
            (0.,y) if y.is_finite() && y<0. => "up",
            _ => return None,
        };
        if axis.is_some_and(|a|a!=direction) { return None; }
        axis = Some(direction);
        raw.push_str(&glyph_text(glyph,&page.fonts));
    }
    (raw == word.text).then_some(axis).flatten()
}
// Exact original rightward baseline and first-glyph advance. Font metrics
// may overlap adjacent lines; no rounding/tolerance merges their baselines.
fn native_flow_tuple(page:&Page,word:&Word)->Option<[f64;4]>{
    if native_source_axis(page,word)? != "right" {return None;}
    let first=page.glyphs.get(*word.glyphs.first()?)?;
    let last=page.glyphs.get(*word.glyphs.last()?)?;
    let mut rect=poppler_box(first);
    let mut previous=None;
    for &i in &word.glyphs{
        let g=page.glyphs.get(i)?;
        let b=poppler_box(g);
        if !g.spacing.is_finite() || !g.kern.is_finite() || g.y!=first.y || !b.iter().all(|n|n.is_finite()) || b[0]>=b[2] || b[1]>=b[3] || previous.is_some_and(|(x,end)|g.x<x || g.end_x<end){return None;}
        rect[0]=rect[0].min(b[0]);rect[2]=rect[2].max(b[2]);previous=Some((g.x,g.end_x));
    }
    if rect != [word.x_min,word.y_min,word.x_max,word.y_max]{return None;}
    let flow=[first.x,first.y,last.end_x,first.end_x-first.x];
    (flow.iter().all(|n|n.is_finite()) && flow[0]<flow[2] && flow[3]>0. && flow[3]<=flow[2]-flow[0] && (last.end_x+last.spacing).is_finite() && (flow[3]+first.kern).is_finite()).then_some(flow)
}
fn layout<'a>(pages: impl IntoIterator<Item=(&'a Page,Option<&'a [Option<char>]>,Option<&'a [Option<[f64;4]>]>)>, emit_styles:bool) -> String {
    let mut s = String::from(
        "<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.0 Transitional//EN\" \"http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd\">\
         <html xmlns=\"http://www.w3.org/1999/xhtml\">\n<head>\n<title></title>\n</head>\n<body>\n<doc>\n",
    );
    for (page, resolved_source, ink_rects) in pages {
        // poppler は /Rotate の前の幅・高さを書く。
        let (w, h) = if page.rotate % 180 == 90 {
            (page.height, page.width)
        } else {
            (page.width, page.height)
        };
        let count=if emit_styles{format!(" data-native-glyph-count=\"{}\"",page.glyphs.len())}else{String::new()};
        let _ = writeln!(s, "  <page width=\"{w:.6}\" height=\"{h:.6}\"{count}>");
        for word in words(page) {
            let attrs = format!(
                "xMin=\"{:.6}\" yMin=\"{:.6}\" xMax=\"{:.6}\" yMax=\"{:.6}\"",
                word.x_min, word.y_min, word.x_max, word.y_max
            );
            let native = native_vertical_attrs(page, &word);
            let styles=if emit_styles { native_styles(page,&word).map(|s|format!(" data-native-styles=\"{s}\"")).unwrap_or_default() }else{String::new()};
            let axis = if emit_styles { format!(" data-native-axis=\"{}\"",native_source_axis(page,&word).unwrap_or("unknown")) } else { String::new() };
            let ruling=if emit_styles {ink_rects.and_then(|r|crate::source_ruling::word_ruling(page,&word,r)).map(|(axis,[x0,y0,x1,y1])|{
                format!(" data-native-ruling=\"{axis}\" data-native-ink-xmin=\"{x0}\" data-native-ink-ymin=\"{y0}\" data-native-ink-xmax=\"{x1}\" data-native-ink-ymax=\"{y1}\"")
            }).unwrap_or_default()}else{String::new()};
            let flow=if emit_styles {native_axis_flow_tuple(page,&word).map(|[start,baseline,end,leading]|{ let trailing=page.glyphs[*word.glyphs.last().unwrap()].spacing; let prefix=page.glyphs[word.glyphs[0]].kern; format!(" data-native-flow-start=\"{start}\" data-native-flow-baseline=\"{baseline}\" data-native-flow-end=\"{end}\" data-native-flow-leading=\"{leading}\" data-native-flow-trailing=\"{trailing}\" data-native-flow-prefix=\"{prefix}\"") }).unwrap_or_default()}else{String::new()};
            let advance_band = if emit_styles && native_axis_flow_tuple(page, &word).is_none() {
                native_advance_band::from_word(page, &word)
                    .map(|band| format!(" data-native-advance-band=\"{}\"", band.attribute()))
                    .unwrap_or_default()
            } else { String::new() };
            let range=if emit_styles{native_glyph_range(page,&word).map(|(first,after)|format!(" data-native-flow-first-glyph=\"{first}\" data-native-flow-after-glyph=\"{after}\"")).unwrap_or_default()}else{String::new()};
            let content = if emit_styles {
                let value = native_content_styles(page, &word, resolved_source).unwrap_or_else(|| "unknown".into());
                format!(" data-native-content-styles=\"{value}\"")
            } else { String::new() };
            let source_attr = resolved_source
                .and_then(|source| crate::source_aj1::word_candidate(page, &word, source))
                .map(|text| format!(" data-source-aj1-text=\"{}\"", escape(&text)))
                .unwrap_or_default();
            let cell_origin=if emit_styles{native_cell_origin::prove(page,&word).map(|o|format!(" data-native-cell-origin=\"2 {} {} {} {} {} {} {} {} {} {} {} {} {} {}\"",o.first,o.after,o.flow[0],o.flow[1],o.flow[2],o.flow[3],o.flow[4],o.flow[5],o.bounds[0],o.bounds[1],o.bounds[2],o.bounds[3],o.sizes[0],o.sizes[1])).unwrap_or_default()}else{String::new()};
            let resolved = if resolved_aj1_word(page, &word) { " data-unicode-aj1=\"1\"" } else { "" };
            let _ = writeln!(
                s,
                "    <flow>\n      <block {attrs}>\n        <line {attrs}>\n          <word {attrs}{native}{resolved}{source_attr}{styles}{content}{axis}{ruling}{flow}{range}{cell_origin}{advance_band}>{}</word>\n        </line>\n      </block>\n    </flow>",
                escape(&word.text)
            );
        }
        s.push_str("  </page>\n");
    }
    s.push_str("</doc>\n</body>\n</html>\n");
    s
}

fn escape(t: &str) -> String {
    let mut out = String::with_capacity(t.len());
    for c in t.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::FontInfo;

    /// 横書き 8pt の字（字幅 1 em）。
    fn glyph(text: &str, x: f64, font: usize) -> Glyph {
        Glyph {
            text: text.into(),
            code: 0,
            cid: 0,
            font,
            x,
            y: 100.0,
            end_x: x + 8.0,
            end_y: 100.0,
            size: 8.0,
            vertical: false,
            bbox: [x, 93.0, x + 8.0, 102.0],
            invisible: false,
            spacing: 0.0,
            kern: 0.0,
            show: 0,
        }
    }

    pub(super) fn page(glyphs: Vec<Glyph>) -> Page {
        let font = FontInfo { name: "F".into(), vertical: false, ordering: Some("Japan1".into()), has_to_unicode: false,
        };
        Page { width: 595.0, height: 842.0, rotate: 0, glyphs, paths: Vec::new(), fonts: vec![font.clone(), font],
        }
    }

    fn texts(p: &Page) -> Vec<String> {
        words(p).into_iter().map(|w| w.text).collect()
    }

    #[test]
    fn display_boxes_follow_vertical_glyphs_in_all_four_directions() {
        for (end, raw, display) in [
            ([100.0, 108.0], [100.0, 92.0, 108.0, 100.0], [96.0, 100.0, 104.0, 108.0]),
            ([92.0, 100.0], [100.0, 100.0, 108.0, 108.0], [92.0, 96.0, 100.0, 104.0]),
            ([100.0, 92.0], [92.0, 100.0, 100.0, 108.0], [96.0, 92.0, 104.0, 100.0]),
            ([108.0, 100.0], [92.0, 92.0, 100.0, 100.0], [100.0, 96.0, 108.0, 104.0]),
        ] {
            let mut vertical = glyph("縦", 100.0, 0);
            vertical.vertical = true;
            vertical.end_x = end[0];
            vertical.end_y = end[1];
            vertical.bbox = display;
            let horizontal = glyph("横", 200.0, 0);
            let horizontal_box = horizontal.bbox;
            // 同じページに横組みの文字があっても、その座標は動かさない。
            let p = page(vec![vertical, horizontal]);
            assert_eq!(display_boxes(&p, &[raw, horizontal_box, [0.0, 0.0, 8.0, 8.0]]),
                vec![Some(display), Some(horizontal_box), None]);
        }
    }

    #[test]
    fn display_boxes_allow_different_word_grouping_and_small_baseline_drift() {
        let p = page(vec![glyph("本", 100.0, 0), glyph("文", 108.06, 0)]);
        assert_eq!(display_boxes(&p, &[[100.0, 93.0, 116.0, 102.0], [108.06, 93.0, 116.06, 102.0]]),
            vec![Some([100.0, 93.0, 116.06, 102.0]), Some([108.06, 93.0, 116.06, 102.0])]);
    }

    #[test]
    fn joins_adjacent_glyphs_and_splits_at_gaps() {
        let p = page(vec![glyph("令", 10.0, 0), glyph("和", 18.0, 0), glyph("八", 40.0, 0),
        ]);
        assert_eq!(texts(&p), ["令和", "八"]);
    }

    #[test]
    fn drops_duplicated_glyph_drawn_on_top() {
        // 同じ字を 0.04pt ずらして重ね描きしたものは 1 字にする。
        let p = page(vec![glyph("太", 10.0, 0), glyph("太", 10.04, 0), glyph("字", 18.0, 0),
        ]);
        assert_eq!(texts(&p), ["太字"]);
        assert_eq!(words(&p)[0].glyphs, vec![0, 2]);
    }

    #[test]
    fn overlapping_different_glyph_is_its_own_word() {
        // 「度」の後に、私用 CID の 2 字を同じ位置に重ねて 1 字を作る組み方。重ねた 2 字目は
        // 1 語になり、その次の字（」）も新しい語になる。
        let p = page(vec![
            glyph("度", 10.0, 0),
            glyph("\u{EAD6}", 18.0, 1),
            glyph("\u{EADF}", 18.0, 1),
            glyph("」", 26.0, 0),
        ]);
        assert_eq!(texts(&p), ["度\u{EAD6}", "\u{EADF}", "」"]);
    }
}

#[cfg(test)]
#[path = "native_vertical_bbox_tests.rs"]
mod native_vertical_bbox_tests;
#[cfg(test)]
mod native_style_tests {
    use super::*;
    pub(super) fn glyph(text: &str, x: f64, font: usize, size: f64) -> Glyph {
        Glyph {
            text: text.into(),
            code: 0,
            cid: 0,
            font,
            x,
            y: 100.,
            end_x: x + 8.,
            end_y: 100.,
            size,
            vertical: false,
            bbox: [x, 93., x + 8., 102.],
            invisible: false,
            spacing: 0.,
            kern: 0.,
            show: 0,
        }
    }
    pub(super) fn page(glyphs: Vec<Glyph>) -> Page {
        let f = crate::page::FontInfo {
            name: "arbitrary".into(),
            vertical: false,
            ordering: None,
            has_to_unicode: false,
        };
        Page {
            width: 595.,
            height: 842.,
            rotate: 0,
            glyphs,
            paths: vec![],
            fonts: vec![f.clone(), f],
        }
    }
    #[test]
    fn style_identity_belongs_to_each_source_word_not_its_text() {
        let p = page(vec![glyph("同", 10., 0, 8.), glyph("同", 40., 1, 9.)]);
        let ws = words(&p);
        assert_eq!(ws.len(), 2);
        assert_eq!(
            native_styles(&p, &ws[0]).as_deref(),
            Some("0:4020000000000000")
        );
        assert_eq!(
            native_styles(&p, &ws[1]).as_deref(),
            Some("1:4022000000000000")
        );
    }
    #[test]
    fn a_multi_glyph_word_keeps_all_source_style_identities() {
        let p = page(vec![glyph("甲", 10., 0, 8.), glyph("乙", 18., 1, 8.)]);
        let w = Word {
            text: "甲乙".into(),
            x_min: 10.,
            y_min: 93.,
            x_max: 26.,
            y_max: 102.,
            glyphs: vec![0, 1],
        };
        assert_eq!(
            native_styles(&p, &w).as_deref(),
            Some("0:4020000000000000 1:4020000000000000")
        );
    }
    #[test]
    fn incomplete_or_invalid_source_styles_are_not_partially_emitted() {
        let p = page(vec![glyph("甲", 10., 0, 8.)]);
        let mut w = words(&p).remove(0);
        w.glyphs.push(10);
        assert_eq!(native_styles(&p, &w), None);
        w.glyphs.clear();
        assert_eq!(native_styles(&p, &w), None);
        for size in [0., -1., f64::NAN, f64::INFINITY] {
            let p = page(vec![glyph("甲", 10., 0, size)]);
            let w = Word {
                text: "甲".into(),
                x_min: 10.,
                y_min: 93.,
                x_max: 18.,
                y_max: 102.,
                glyphs: vec![0],
            };
            assert_eq!(native_styles(&p, &w), None);
        }
        let p = page(vec![glyph("甲", 10., 20, 8.)]);
        let w = Word {
            text: "甲".into(),
            x_min: 10.,
            y_min: 93.,
            x_max: 18.,
            y_max: 102.,
            glyphs: vec![0],
        };
        assert_eq!(native_styles(&p, &w), None);
    }
    #[test]
    fn source_style_renderer_only_adds_metadata_to_the_legacy_rendering() {
        let p = page(vec![glyph("同", 10., 0, 8.), glyph("同", 40., 1, 9.)]);
        let sources = vec![crate::Aj1PageSources { ink_rects:vec![],
            resolved: vec![None, None],
            original: p.clone(),
        }];
        let old = bbox_layout_with_aj1_sources(&sources);
        let mut new = bbox_layout_with_native_styles(&sources);
        assert_eq!(new.matches(" data-native-styles=\"").count(), 2);
        for attr in ["data-native-cell-origin", "data-native-glyph-count", "data-native-flow-first-glyph", "data-native-flow-after-glyph", "data-native-flow-start","data-native-flow-baseline","data-native-flow-end","data-native-flow-leading", "data-native-flow-trailing", "data-native-flow-prefix","data-native-styles", "data-native-content-styles", "data-native-axis", "data-native-ruling", "data-native-ink-xmin", "data-native-ink-ymin", "data-native-ink-xmax", "data-native-ink-ymax"] {
            let needle = format!(" {attr}=\"");
            while let Some(start) = new.find(&needle) {
                let from = start + needle.len();
                let end = from + new[from..].find('"').unwrap() + 1;
                new.replace_range(start..end, "");
            }
        }
        assert_eq!(old, new);
        assert_eq!(old, bbox_layout(&[p]));
    }
}

#[cfg(test)]
#[path="native_content_style_tests.rs"]
mod native_content_style_tests;

#[cfg(test)]
#[path="native_axis_tests.rs"]
mod native_axis_tests;

#[cfg(test)]
#[path="native_flow_contracts.rs"]
mod native_flow_contracts;

fn source_point(axis:&str,x:f64,y:f64)->Option<(f64,f64)>{Some(match axis{"right"=>(x,y),"down"=>(y,-x),"left"=>(-x,-y),"up"=>(-y,x),_=>return None})}
fn native_axis_flow_tuple(page:&Page,word:&Word)->Option<[f64;4]>{
    let axis=native_source_axis(page,word)?;
    if axis=="right"{return native_flow_tuple(page,word);}
    let first=page.glyphs.get(*word.glyphs.first()?)?;
    let last=page.glyphs.get(*word.glyphs.last()?)?;
    let start=source_point(axis,first.x,first.y)?;
    let end=source_point(axis,last.end_x,last.end_y)?;
    let lead=source_point(axis,first.end_x,first.end_y)?.0-start.0;
    let mut rect=poppler_box(first);let mut previous=None;
    for &id in &word.glyphs{
        let g=page.glyphs.get(id)?;let b=poppler_box(g);
        let a=source_point(axis,g.x,g.y)?;let z=source_point(axis,g.end_x,g.end_y)?;
        if !g.spacing.is_finite()||!g.kern.is_finite()||a.1!=start.1||z.1!=start.1||!b.iter().all(|v|v.is_finite())||b[0]>=b[2]||b[1]>=b[3]||previous.is_some_and(|(x,e)|a.0<x||z.0<e){return None;}
        rect=[rect[0].min(b[0]),rect[1].min(b[1]),rect[2].max(b[2]),rect[3].max(b[3])];previous=Some((a.0,z.0));
    }
    if rect!=[word.x_min,word.y_min,word.x_max,word.y_max]{return None;}
    let result=[start.0,start.1,end.0,lead];
    (result.iter().all(|v|v.is_finite())&&start.0<end.0&&lead>0.&&lead<=end.0-start.0&&(end.0+last.spacing).is_finite()&&(lead+first.kern).is_finite()).then_some(result)
}
fn native_glyph_range(page:&Page,word:&Word)->Option<(usize,usize)>{
    native_axis_flow_tuple(page,word)?;
    let first=*word.glyphs.first()?;let after=word.glyphs.last()?.checked_add(1)?;
    (after<=page.glyphs.len()&&word.glyphs.iter().enumerate().all(|(i,&id)|first.checked_add(i)==Some(id))).then_some((first,after))
}
#[cfg(test)]
#[path="native_projected_flow_tests.rs"]
mod native_projected_flow_tests;

#[path="native_cell_origin.rs"]mod native_cell_origin;

#[path = "native_advance_band.rs"]
mod native_advance_band;
