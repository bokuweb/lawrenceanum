//! Complete source advance origins form a band; they are never rounded into
//! an invented common baseline. The exact-flow contract remains separate; only source-enabled XHTML emits this witness.
use super::*;
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Band {
    pub start: f64,
    pub min_baseline: f64,
    pub max_baseline: f64,
    pub end: f64,
    pub projection: [f64; 4],
    pub glyph_bounds: [f64; 4],
    pub leading: f64,
    pub trailing: f64,
    pub prefix: f64,
    pub first: usize,
    pub after: usize,
}
pub(super) fn from_word(page: &Page, word: &Word) -> Option<Band> {
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
            .all(|(i, &id)| first.checked_add(i) == Some(id))
    {
        return None;
    }
    let a = page.glyphs.get(first)?;
    let z = page.glyphs.get(after - 1)?;
    let mut projection = poppler_box(a);
    let mut glyph_bounds = a.bbox;
    let mut min_baseline = a.y;
    let mut max_baseline = a.y;
    let mut previous = None;
    for &id in &word.glyphs {
        let g = page.glyphs.get(id)?;
        let b = poppler_box(g);
        let raw = g.bbox;
        if g.vertical
            || !g.spacing.is_finite()
            || !g.kern.is_finite()
            || !b.iter().chain(raw.iter()).all(|n| n.is_finite())
            || b[0] >= b[2]
            || b[1] >= b[3]
            || raw[0] >= raw[2]
            || raw[1] >= raw[3]
            || previous.is_some_and(|(x, end)| g.x < x || g.end_x < end)
        {
            return None;
        }
        // Reconstruct the renderer's original Word projection, which retains the
        // first glyph's cross-axis extent. The actual glyph envelope is separate.
        projection[0] = projection[0].min(b[0]);
        projection[2] = projection[2].max(b[2]);
        glyph_bounds = [
            glyph_bounds[0].min(raw[0]),
            glyph_bounds[1].min(raw[1]),
            glyph_bounds[2].max(raw[2]),
            glyph_bounds[3].max(raw[3]),
        ];
        min_baseline = min_baseline.min(g.y);
        max_baseline = max_baseline.max(g.y);
        previous = Some((g.x, g.end_x));
    }
    if projection != [word.x_min, word.y_min, word.x_max, word.y_max] {
        return None;
    }
    let start = a.x;
    let end = z.end_x;
    let leading = a.end_x - a.x;
    let trailing = z.spacing;
    let prefix = a.kern;
    if ![
        start,
        end,
        leading,
        trailing,
        prefix,
        min_baseline,
        max_baseline,
        end + trailing,
        leading + prefix,
    ]
    .iter()
    .all(|n| n.is_finite())
        || start >= end
        || leading <= 0.
        || leading > end - start
        || glyph_bounds[1] > min_baseline
        || max_baseline > glyph_bounds[3]
    {
        return None;
    }
    Some(Band {
        start,
        min_baseline,
        max_baseline,
        end,
        leading,
        trailing,
        prefix,
        projection,
        glyph_bounds,
        first,
        after,
    })
}
#[cfg(test)]
#[path = "native_advance_band_tests.rs"]
mod tests;

impl Band {
    pub(super) fn attribute(&self) -> String {
        // Projection references use the same six-decimal representation as the
        // emitted Word. Original origins/envelope retain full binary64 precision.
        format!(
            "1 {} {} {} {} {} {} {} {} {} {:.6} {:.6} {:.6} {:.6} {} {} {} {}",
            self.first,
            self.after,
            self.start,
            self.min_baseline,
            self.max_baseline,
            self.end,
            self.leading,
            self.trailing,
            self.prefix,
            self.projection[0],
            self.projection[1],
            self.projection[2],
            self.projection[3],
            self.glyph_bounds[0],
            self.glyph_bounds[1],
            self.glyph_bounds[2],
            self.glyph_bounds[3]
        )
    }
}
