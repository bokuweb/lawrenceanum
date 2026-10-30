//! Source-coordinate evidence for a closed horizontal ruling section.
use super::{table_bottom_cut, trace, y_center, Block, Col};
#[derive(Clone, Copy, Default)]
pub(super) struct SourceGeometry<'a> {
    pub(super) bands: &'a [Option<crate::native_advance_band::Band>],
    pub(super) enclosures: &'a [[f32;4]],
    pub(super) flows: &'a [Option<crate::native_word_flow::NativeFlow>],
    pub(super) bounds: &'a [trace::NativeBounds],
    pub(super) axes: &'a [crate::native_word_axes::NativeAxis],
    pub(super) words: &'a [trace::Word],
    /// Original content fonts per Word (empty when unknown).
    pub(super) styles: &'a [trace::ContentStyles],
}

// A ruling may be tested against original owned glyph cells without replacing
// compatibility/display coordinates. Only complete rightward source metadata
// may authorize the extra cut; original partition membership must agree.
pub(super) fn source_table_bottom_cut(
    words: &[Col],
    blocks: &[Block],
    hrules: &[(f32, f32, f32)],
    vrules: &[(f32, f32, f32)],
    bounds: &[trace::NativeBounds],
    axes: &[crate::native_word_axes::NativeAxis],
) -> Option<f32> {
    let mut seen = std::collections::HashSet::new();
    let native: Option<Vec<Col>> = words
        .iter()
        .map(|w| {
            if !seen.insert(w.id)
                || axes.get(w.id) != Some(&crate::native_word_axes::NativeAxis::Right)
            {
                return None;
            }
            let trace::NativeBounds::Glyphs(b) = bounds.get(w.id)? else {
                return None;
            };
            if !b.iter().all(|n| n.is_finite())
                || b[0] >= b[2]
                || b[1] >= b[3]
                || ![w.x, w.y, w.x1, w.y1].iter().all(|n| n.is_finite())
                || w.x >= w.x1
                || w.y >= w.y1
            {
                return None;
            }
            Some(Col {
                id: w.id,
                x: b[0],
                y: b[1],
                x1: b[2],
                y1: b[3],
                text: w.text.clone(),
            })
        })
        .collect();
    let native = native?;
    let clear: Vec<_> = hrules
        .iter()
        .copied()
        .filter(|r| {
            r.0.is_finite()
                && r.1.is_finite()
                && r.2.is_finite()
                && r.1 < r.2
                && native.iter().zip(words).all(|(n, w)| {
                    (n.y1 <= r.0 || n.y >= r.0) && (y_center(n) < r.0) == (y_center(w) < r.0)
                })
                && !blocks.iter().any(|b| b.1 < r.0 && b.3 > r.0)
        })
        .collect();
    table_bottom_cut(&native, blocks, &clear, vrules)
}

impl SourceGeometry<'_> {
    // Projection cells can have been merged, moved, or dropped after source
    // parsing. A cell belonging to only one original occurrence cannot certify
    // geometry for any of those altered projections.
    pub(super) fn sources_unchanged(&self, cols: &[Col]) -> bool {
        let mut seen = std::collections::HashSet::new();
        cols.iter().all(|col| {
            let Some(word) = self.words.get(col.id) else {
                return false;
            };
            seen.insert(col.id)
                && word.id == col.id
                && word.status == trace::WordStatus::Used
                && word.text == col.text
                && [word.x0, word.y0, word.x1, word.y1] == [col.x, col.y, col.x1, col.y1]
        })
    }
}
#[cfg(test)]
#[path = "source_geometry_ownership_tests.rs"]
mod source_geometry_ownership_tests;
