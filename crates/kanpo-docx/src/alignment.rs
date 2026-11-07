//! Conservative alignment in an original, completely matched table cell.
//! Geometry is evaluated along the original column axis, so vertical bottom
//! alignment becomes right alignment when text is reflowed horizontally.
use kanpo_parser::trace::{PageTrace, Table, WordStatus};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Alignment {
    Left,
    Center,
    Right,
}
impl Alignment {
    pub fn value(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
        }
    }
}

fn infer(cell: [f32; 4], words: &[[f32; 4]], axis: usize) -> Option<Alignment> {
    if words.is_empty() || axis > 1 {
        return None;
    }
    let cross = 1 - axis;
    let em = words
        .iter()
        .map(|b| b[cross + 2] - b[cross])
        .reduce(f32::min)?;
    if !em.is_finite() || em <= 0. {
        return None;
    }
    let eps = em * 0.2;
    let mut words = words.to_vec();
    words.sort_by(|a, b| a[cross].total_cmp(&b[cross]));
    let mut lines: Vec<(f32, f32, f32)> = vec![];
    for b in words {
        if b.iter().any(|v| !v.is_finite())
            || (0..2).any(|i| b[i] < cell[i] - eps || b[i + 2] > cell[i + 2] + eps)
        {
            return None;
        }
        if let Some(line) = lines
            .last_mut()
            .filter(|line| (line.0 - b[cross]).abs() <= eps)
        {
            line.1 = line.1.min(b[axis]);
            line.2 = line.2.max(b[axis + 2]);
        } else {
            lines.push((b[cross], b[axis], b[axis + 2]));
        }
    }
    let mut agreed = None;
    for (_, lo, hi) in lines {
        let lead = lo - cell[axis];
        let trail = cell[axis + 2] - hi;
        // Filled lines do not distinguish alignment from ordinary wrapping.
        // Wide indents and mixed line anchors are also left unspecified.
        if lead + trail < 2. * em {
            return None;
        }
        let own = if (lead - trail).abs() <= eps {
            Alignment::Center
        } else if trail <= em * 0.75 && lead >= 2. * em {
            Alignment::Right
        } else if lead <= em * 0.75 && trail >= 2. * em {
            Alignment::Left
        } else {
            return None;
        };
        if agreed.is_some_and(|previous| previous != own) {
            return None;
        }
        agreed = Some(own);
    }
    agreed
}

pub(crate) fn source(
    table: &Table,
    page: &PageTrace,
    axis: usize,
) -> BTreeMap<(usize, usize), Alignment> {
    let mut out = BTreeMap::new();
    for c in &table.cells {
        if c.text.trim().is_empty() {
            continue;
        }
        // Relocated amount signs and synthesized separators may not be owned
        // by these ordinary cell words. Do not align an entire value using
        // only the numeric suffix's geometry.
        let bag = |s: String| {
            let mut counts = BTreeMap::new();
            for ch in s.chars().filter(|c| !c.is_whitespace()) {
                *counts.entry(ch).or_insert(0usize) += 1;
            }
            counts
        };
        let original = c
            .words
            .iter()
            .map(|&id| page.words.get(id).map(|w| w.text.as_str()))
            .collect::<Option<Vec<_>>>();
        if original.is_none_or(|words| bag(words.concat()) != bag(c.text.clone())) {
            continue;
        }
        let words = c
            .words
            .iter()
            .map(|&id| {
                let w = page.words.get(id)?;
                (w.id == id && w.status == WordStatus::Used).then_some(())?;
                page.original_glyph_bounds(id)
            })
            .collect::<Option<Vec<_>>>();
        if let Some(alignment) =
            words.and_then(|words| infer([c.x0, c.y0, c.x1, c.y1], &words, axis))
        {
            out.insert((c.row, c.col), alignment);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn line_anchors_work_in_both_writing_axes_under_translation_and_scaling() {
        for (word, expected) in [
            ([4., 12., 28., 20.], Alignment::Left),
            ([38., 12., 62., 20.], Alignment::Center),
            ([72., 12., 96., 20.], Alignment::Right),
        ] {
            for axis in [0, 1] {
                for (scale, dx, dy) in [(0.5, 30., 40.), (1., 0., 0.), (2., 150., 70.)] {
                    let transform = |mut b: [f32; 4]| {
                        if axis == 1 {
                            b.swap(0, 1);
                            b.swap(2, 3);
                        }
                        [
                            b[0] * scale + dx,
                            b[1] * scale + dy,
                            b[2] * scale + dx,
                            b[3] * scale + dy,
                        ]
                    };
                    assert_eq!(
                        infer(transform([0., 0., 100., 40.]), &[transform(word)], axis),
                        Some(expected)
                    );
                }
            }
        }
    }
    #[test]
    fn wrapped_ambiguous_and_outside_lines_do_not_supply_alignment() {
        let c = [0., 0., 100., 50.];
        for words in [
            vec![],
            vec![[4., 12., 96., 20.]],
            vec![[20., 12., 70., 20.]],
            vec![[38., 12., 62., 20.], [4., 24., 28., 32.]],
            vec![[-10., 12., 62., 20.]],
            vec![[38., 12., f32::NAN, 20.]],
        ] {
            assert_eq!(infer(c, &words, 0), None);
        }
    }
}
