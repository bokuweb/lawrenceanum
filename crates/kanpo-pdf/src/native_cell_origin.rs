//! A separate origin contract for cell typography. The original strict flow
//! remains absent when a punctuation glyph has a different baseline. Here the
//! common alphanumeric baseline and the full original glyph envelope are both
//! retained; no baseline is rounded, averaged, or inferred from a font label.
use super::*;
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Origin {
    pub first: usize,
    pub after: usize,
    pub flow: [f64; 6],
    pub bounds: [f64; 4],
    pub sizes: [f64; 2],
}
pub(super) fn prove(page: &Page, word: &Word) -> Option<Origin> {
    if native_source_axis(page, word)? != "right" {
        return None;
    }
    let first = *word.glyphs.first()?;
    let after = word.glyphs.last()?.checked_add(1)?;
    if after > page.glyphs.len()
        || !word
            .glyphs
            .iter()
            .enumerate()
            .all(|(i, &n)| first.checked_add(i) == Some(n))
    {
        return None;
    }
    let a = page.glyphs.get(first)?;
    let z = page.glyphs.get(after - 1)?;
    let mut original = poppler_box(a);
    let mut bounds = original;
    let mut previous = None;
    let mut common = Some(a.y);
    let mut content = None;
    for &id in &word.glyphs {
        let g = page.glyphs.get(id)?;
        let b = poppler_box(g);
        if g.vertical
            || !g.spacing.is_finite()
            || !g.kern.is_finite()
            || !b.iter().all(|n| n.is_finite())
            || b[0] >= b[2]
            || b[1] >= b[3]
            || previous.is_some_and(|(x, end)| g.x < x || g.end_x < end)
        {
            return None;
        }
        original[0] = original[0].min(b[0]);
        original[2] = original[2].max(b[2]);
        bounds = [
            bounds[0].min(b[0]),
            bounds[1].min(b[1]),
            bounds[2].max(b[2]),
            bounds[3].max(b[3]),
        ];
        if common.is_some_and(|baseline| baseline != g.y) {
            common = None;
        }
        if glyph_text(g, &page.fonts)
            .chars()
            .any(char::is_alphanumeric)
        {
            if content.is_some_and(|baseline| baseline != g.y) {
                return None;
            }
            content = Some(g.y);
        }
        previous = Some((g.x, g.end_x));
    }
    if original != [word.x_min, word.y_min, word.x_max, word.y_max] {
        return None;
    }
    let baseline = common.or(content)?;
    let leading = a.end_x - a.x;
    let flow = [a.x, baseline, z.end_x, leading, z.spacing, a.kern];
    let sizes = [a.size, z.size];
    if sizes.iter().any(|n| !n.is_finite() || *n <= 0.) {
        return None;
    }
    (flow.iter().all(|n| n.is_finite())
        && flow[0] < flow[2]
        && leading > 0.
        && leading <= flow[2] - flow[0]
        && bounds[1] <= baseline
        && baseline <= bounds[3]
        && (z.end_x + z.spacing).is_finite()
        && (leading + a.kern).is_finite())
    .then_some(Origin {
        first,
        after,
        flow,
        bounds,
        sizes,
    })
}
#[cfg(test)]
#[path = "native_cell_origin_tests.rs"]
mod tests;
