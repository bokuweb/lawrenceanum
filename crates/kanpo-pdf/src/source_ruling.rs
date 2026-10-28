//! Whole-word evidence for a continuous chain of original outline rectangles.
use crate::{bbox::Word, Page};
pub(crate) type Rect = [f64; 4];
pub(crate) fn word_ruling(
    page: &Page,
    word: &Word,
    rects: &[Option<Rect>],
) -> Option<(&'static str, Rect)> {
    // A chain has more than one source member. No upper glyph/character cap.
    if rects.len() != page.glyphs.len()
        || word.glyphs.len() < 2
        || word.glyphs.windows(2).any(|p| p[0] >= p[1])
    {
        return None;
    }
    let mut direction = None;
    let mut previous = None;
    let mut union: Option<Rect> = None;
    let mut text = String::new();
    for &id in &word.glyphs {
        let g = page.glyphs.get(id)?;
        page.fonts.get(g.font)?;
        if g.invisible
            || !g.size.is_finite()
            || g.size <= 0.
            || ![g.x, g.y, g.end_x, g.end_y].iter().all(|n| n.is_finite())
            || !g.bbox.iter().all(|n| n.is_finite())
        {
            return None;
        }
        let b = (*rects.get(id)?)?;
        if b.iter().any(|n| !n.is_finite()) || b[0] >= b[2] || b[1] >= b[3] {
            return None;
        }
        let d = match (g.end_x - g.x, g.end_y - g.y) {
            (x, 0.) if x > 0. => (0, 1.),
            (x, 0.) if x < 0. => (0, -1.),
            (0., y) if y > 0. => (1, 1.),
            (0., y) if y < 0. => (1, -1.),
            _ => return None,
        };
        if direction.is_some_and(|old| old != d) {
            return None;
        }
        direction = Some(d);
        let cross = 1 - d.0;
        let (low, high) = if d.1 > 0. {
            (b[d.0], b[d.0 + 2])
        } else {
            (-b[d.0 + 2], -b[d.0])
        };
        let origin = if d.0 == 0 { g.x * d.1 } else { g.y * d.1 };
        if let Some((a, z, c0, c1, p)) = previous {
            if b[cross] != c0
                || b[cross + 2] != c1
                || low <= a
                || high <= z
                || low > z
                || origin <= p
            {
                return None;
            }
        }
        previous = Some((low, high, b[cross], b[cross + 2], origin));
        union = Some(union.map_or(b, |a| {
            [
                a[0].min(b[0]),
                a[1].min(b[1]),
                a[2].max(b[2]),
                a[3].max(b[3]),
            ]
        }));
        text.push_str(&crate::bbox::glyph_text(g, &page.fonts));
    }
    if text != word.text {
        return None;
    }
    Some((
        if direction?.0 == 0 {
            "horizontal"
        } else {
            "vertical"
        },
        union?,
    ))
}
// A single painted rectangle or a complete connected chain has the same
// geometric authority. The legacy ruling classifier still requires a chain.
// This separate word certificate does not turn a short value mark into a rule.
pub(crate) fn word_outline_attribute(
    page: &Page,
    word: &Word,
    rects: &[Option<Rect>],
) -> Option<String> {
    let (first, after) = crate::bbox::native_glyph_range(page, word)?;
    crate::bbox::native_flow_tuple(page, word)?;
    if rects.len() != page.glyphs.len()
        || page.glyphs[first..after].iter().any(|g| g.invisible)
        || page.glyphs[first..after]
            .iter()
            .map(|g| crate::bbox::glyph_text(g, &page.fonts))
            .collect::<String>()
            != word.text
    {
        return None;
    }
    let ink = if after == first + 1 {
        rects[first]?
    } else {
        word_ruling(page, word, rects)?.1
    };
    let legacy = [word.x_min, word.y_min, word.x_max, word.y_max];
    if legacy.iter().chain(&ink).any(|n| !n.is_finite())
        || legacy[0] >= legacy[2]
        || legacy[1] >= legacy[3]
        || ink[0] >= ink[2]
        || ink[1] >= ink[3]
        || ink[0] < legacy[0]
        || ink[1] < legacy[1]
        || ink[2] > legacy[2]
        || ink[3] > legacy[3]
    {
        return None;
    }
    let raw = word
        .text
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let numbers = legacy
        .into_iter()
        .chain(ink)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    Some(format!("1 {first} {after} {raw} {numbers}"))
}
#[cfg(test)]
#[path = "source_ruling_tests.rs"]
mod tests;
