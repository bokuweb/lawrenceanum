//! `pdftotext -bbox-layout` と同じ形の XHTML を作る（既存の kanpo-parser への橋渡し）。
//!
//! 文字を語にまとめ、語の枠は poppler の出力に合わせる。poppler の挙動を外から観察して
//! 合わせたもので、poppler のコードは使っていない。
//!
//! - 横書きの語: 字面の枠（FontDescriptor の Ascent〜Descent）をつないだもの
//! - 縦書きの語: 文字の原点（字の上端の中央）を左上とする正方形をつないだもの。実際の字面より
//!   右へ半 em、上へ 1 em ずれる（kanpo-parser はこのずれを前提に補正している）
//! - poppler が読めないフォント（[`poppler_readable`]）の文字は出さない
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
        let forced_break = std::mem::replace(&mut break_next, overlap) || overlap;
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
            let gap = if g.vertical { GAP_VERTICAL } else { GAP };
            let gap = if letter_spaced(p, g, along) { gap.max(TC_MAX) } else { gap };
            (-OVERLAP * g.size..=gap * g.size).contains(&along)
                && across.abs() <= OFFSET * g.size
                && !(along > RULE_GAP * g.size && rule_between(&rules, p, g))
        });
        let b = poppler_box(g);
        let text = g.text.clone();
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

/// ページを `pdftotext -bbox-layout` 形式の XHTML にする。語ごとに 1 つの flow・block・line に入れる。
pub fn bbox_layout(pages: &[Page]) -> String {
    let mut s = String::from(
        "<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.0 Transitional//EN\" \"http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd\">\
         <html xmlns=\"http://www.w3.org/1999/xhtml\">\n<head>\n<title></title>\n</head>\n<body>\n<doc>\n",
    );
    for page in pages {
        // poppler は /Rotate の前の幅・高さを書く。
        let (w, h) = if page.rotate % 180 == 90 {
            (page.height, page.width)
        } else {
            (page.width, page.height)
        };
        let _ = writeln!(s, "  <page width=\"{w:.6}\" height=\"{h:.6}\">");
        for word in words(page) {
            let attrs = format!(
                "xMin=\"{:.6}\" yMin=\"{:.6}\" xMax=\"{:.6}\" yMax=\"{:.6}\"",
                word.x_min, word.y_min, word.x_max, word.y_max
            );
            let _ = writeln!(
                s,
                "    <flow>\n      <block {attrs}>\n        <line {attrs}>\n          <word {attrs}>{}</word>\n        </line>\n      </block>\n    </flow>",
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

    fn page(glyphs: Vec<Glyph>) -> Page {
        let font = FontInfo { name: "F".into(), vertical: false, ordering: Some("Japan1".into()), has_to_unicode: false };
        Page { width: 595.0, height: 842.0, rotate: 0, glyphs, paths: Vec::new(), fonts: vec![font.clone(), font] }
    }

    fn texts(p: &Page) -> Vec<String> {
        words(p).into_iter().map(|w| w.text).collect()
    }

    #[test]
    fn joins_adjacent_glyphs_and_splits_at_gaps() {
        let p = page(vec![glyph("令", 10.0, 0), glyph("和", 18.0, 0), glyph("八", 40.0, 0)]);
        assert_eq!(texts(&p), ["令和", "八"]);
    }

    #[test]
    fn drops_duplicated_glyph_drawn_on_top() {
        // 同じ字を 0.04pt ずらして重ね描きしたものは 1 字にする。
        let p = page(vec![glyph("太", 10.0, 0), glyph("太", 10.04, 0), glyph("字", 18.0, 0)]);
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
