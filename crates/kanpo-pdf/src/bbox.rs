//! `pdftotext -bbox-layout` と同じ形の XHTML を作る（既存の kanpo-parser への橋渡し）。
//!
//! 文字を語にまとめ、語の枠は poppler の出力に合わせる。poppler の挙動を外から観察して
//! 合わせたもので、poppler のコードは使っていない。
//!
//! - 横書きの語: 字面の枠（FontDescriptor の Ascent〜Descent）をつないだもの
//! - 縦書きの語: 文字の原点（字の上端の中央）を左上とする正方形をつないだもの。実際の字面より
//!   右へ半 em、上へ 1 em ずれる（kanpo-parser はこのずれを前提に補正している）
//! - poppler が読めないフォント（[`poppler_readable`]）の文字は出さない
//! - 縦書きはフォントが変わると語を切る。字間（Tc）で空けた字は別の語にする

use std::fmt::Write;

use crate::page::{FontInfo, Glyph, Page};

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
    let readable: Vec<bool> = page.fonts.iter().map(poppler_readable).collect();
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
        let continues = prev.is_some_and(|p| {
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
            (-OVERLAP * g.size..=gap * g.size).contains(&along) && across.abs() <= OFFSET * g.size
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
