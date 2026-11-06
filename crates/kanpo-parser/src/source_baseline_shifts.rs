//! Strict, bracketed baseline shifts observed in original source glyphs.
//! This describes placement, without interpreting a formula or a footnote.
use crate::{
    source_underlines::Basis,
    trace::{PageTrace, WordStatus},
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
type Rect = [f64; 4];
type Location = (usize, usize, Option<usize>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Shift {
    Raised,
    Lowered,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceRun {
    pub word: usize,
    pub glyphs: Vec<usize>,
    pub shift: Shift,
    pub bounds: Rect,
    pub size: f64,
    pub baseline: f64,
    pub previous_glyph: usize,
    pub following_glyph: usize,
    pub parent_size: f64,
    pub parent_baselines: [f64; 2],
    pub parent_bounds: [Rect; 2],
    #[serde(rename = "writing_basis")]
    pub(crate) basis: Basis,
}

/// Original raw text and six-decimal bbox identify occurrences, never a
/// normalized-text lookup. Shared source glyphs invalidate both occurrences.
pub(crate) fn occurrences(
    page: &PageTrace,
    native: &kanpo_pdf::Page,
) -> BTreeMap<usize, Vec<usize>> {
    let words = kanpo_pdf::bbox::words(native);
    let mut out = BTreeMap::new();
    let mut identities = BTreeMap::<usize, usize>::new();
    for w in &page.words {
        *identities.entry(w.id).or_default() += 1;
    }
    for (index, w) in page.words.iter().enumerate() {
        if w.id != index || identities[&w.id] != 1 {
            continue;
        }
        let found = words
            .iter()
            .filter(|s| {
                s.text == w.raw
                    && [s.x_min, s.y_min, s.x_max, s.y_max]
                        .map(|n| format!("{n:.6}").parse::<f32>().unwrap())
                        == w.source_box
            })
            .collect::<Vec<_>>();
        if found.len() == 1 {
            out.insert(w.id, found[0].glyphs.clone());
        }
    }
    let mut counts = BTreeMap::<usize, usize>::new();
    for ids in out.values() {
        for &id in ids {
            *counts.entry(id).or_default() += 1;
        }
    }
    out.retain(|_, ids| ids.iter().all(|id| counts[id] == 1));
    out
}

fn locations(page: &PageTrace) -> BTreeMap<usize, Vec<Location>> {
    let mut out = BTreeMap::<usize, Vec<Location>>::new();
    for (ri, r) in page.regions.iter().enumerate() {
        for (li, l) in r.lines.iter().enumerate() {
            for &id in &l.words {
                out.entry(id).or_default().push((ri, li, None));
            }
            if let Some(t) = &l.table {
                for (ci, c) in t.cells.iter().enumerate() {
                    for &id in &c.words {
                        out.entry(id).or_default().push((ri, li, Some(ci)));
                    }
                }
            }
        }
    }
    out
}
fn positive(b: Rect) -> bool {
    b.iter().all(|n| n.is_finite()) && b[0] < b[2] && b[1] < b[3]
}
fn overlaps(a: Rect, b: Rect) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn union(a: Rect, b: Rect) -> Rect {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}
#[derive(Clone)]
struct Part {
    id: usize,
    word: usize,
    location: Location,
    basis: Basis,
    bounds: Rect,
    start: f64,
    end: f64,
    baseline: f64,
    size: f64,
}
impl Part {
    fn center(&self) -> f64 {
        (self.start + self.end) / 2.
    }
}

fn shifted(
    own: &[&Part],
    all: &[Part],
    native: &kanpo_pdf::Page,
    paint: &BTreeMap<usize, Vec<Rect>>,
) -> Option<SourceRun> {
    let first = *own.first()?;
    let last = *own.last()?;
    if own.iter().any(|p| {
        p.word != first.word
            || p.location != first.location
            || p.basis != first.basis
            || p.size != first.size
            || p.baseline != first.baseline
    }) || own.windows(2).any(|w| w[0].center() >= w[1].center())
    {
        return None;
    }
    let bounds = own.iter().map(|p| p.bounds).reduce(union)?;
    let body = all
        .iter()
        .filter(|p| {
            p.word != first.word
                && p.location == first.location
                && p.basis == first.basis
                && p.size > first.size
        })
        .collect::<Vec<_>>();
    // A ruby reading lies over an occupied body advance. A shifted inline run
    // must occupy a free primary slot, even when the surrounding font differs.
    if body.iter().any(|p| {
        own.iter()
            .any(|q| p.start <= q.center() && q.center() < p.end)
    }) {
        return None;
    }
    let before = body
        .iter()
        .copied()
        .filter(|p| {
            first.bounds[1] <= p.end && p.end <= first.center() && p.center() < first.bounds[1]
        })
        .collect::<Vec<_>>();
    let after = body
        .iter()
        .copied()
        .filter(|p| {
            last.center() <= p.start && p.start <= last.bounds[3] && last.bounds[3] < p.center()
        })
        .collect::<Vec<_>>();
    let mut pairs = Vec::new();
    for a in &before {
        for b in &after {
            if a.id == b.id
                || a.size != b.size
                || !(a.bounds[0] <= b.baseline
                    && b.baseline <= a.bounds[2]
                    && b.bounds[0] <= a.baseline
                    && a.baseline <= b.bounds[2])
                || !(bounds[0] < a.bounds[2]
                    && a.bounds[0] < bounds[2]
                    && bounds[0] < b.bounds[2]
                    && b.bounds[0] < bounds[2])
            {
                continue;
            }
            let shift = if bounds[2] < a.baseline.min(b.baseline) {
                Shift::Raised
            } else if bounds[0] > a.baseline.max(b.baseline) {
                Shift::Lowered
            } else {
                continue;
            };
            pairs.push((*a, *b, shift));
        }
    }
    let [(a, b, shift)] = pairs.as_slice() else {
        return None;
    };
    let ids = own.iter().map(|p| p.id).collect::<BTreeSet<_>>();
    // Preserve uncertainty about visible overprints and unmapped text. The
    // two bordering parent glyphs may touch the shifted run through kerning.
    if native.glyphs.iter().enumerate().any(|(id, g)| {
        !ids.contains(&id)
            && id != a.id
            && id != b.id
            && !g.invisible
            && positive(g.bbox)
            && paint.get(&id).map_or_else(
                || overlaps(bounds, first.basis.rect(g.bbox)),
                |rects| rects.iter().any(|&b| overlaps(bounds, first.basis.rect(b))),
            )
    }) {
        return None;
    }
    Some(SourceRun {
        word: first.word,
        glyphs: own.iter().map(|p| p.id).collect(),
        shift: *shift,
        bounds: own.iter().map(|p| native.glyphs[p.id].bbox).reduce(union)?,
        size: first.size,
        baseline: first.baseline,
        previous_glyph: a.id,
        following_glyph: b.id,
        parent_size: a.size,
        parent_baselines: [a.baseline, b.baseline],
        parent_bounds: [a.bounds, b.bounds],
        basis: first.basis,
    })
}

/// Geometry only; field correspondence and final article boundaries must be
/// checked separately before attaching these observations to document text.
pub fn for_page(page: &PageTrace, source: &kanpo_pdf::Aj1PageSources) -> Vec<SourceRun> {
    let mapping = occurrences(page, source.page());
    let locations = locations(page);
    let mut parts = Vec::new();
    let mut complete = BTreeMap::new();
    for (&word, ids) in &mapping {
        let Some(w) = page.words.get(word).filter(|w| {
            w.id == word
                && w.status == WordStatus::Used
                && !w.text.contains('\u{fffd}')
                && !compact(&w.text).is_empty()
        }) else {
            continue;
        };
        let Some([location]) = locations.get(&word).map(Vec::as_slice) else {
            continue;
        };
        let mut own = Vec::new();
        let mut raw = String::new();
        for &id in ids {
            let Some(g) = source.page().glyphs.get(id) else {
                break;
            };
            let Some(token) = source.glyph_bbox_text(id) else {
                break;
            };
            let Some(basis) = source
                .glyph_text_axes(id)
                .and_then(|axes| Basis::glyph(g, axes))
            else {
                break;
            };
            let start = basis.point([g.x, g.y]);
            let end = basis.point([g.end_x, g.end_y]);
            if g.invisible
                || !positive(g.bbox)
                || !g.size.is_finite()
                || g.size <= 0.
                || !start.iter().chain(&end).all(|n| n.is_finite())
                || start[1] >= end[1]
            {
                break;
            }
            raw.push_str(&token);
            own.push(Part {
                id,
                word,
                location: *location,
                basis,
                bounds: basis.rect(g.bbox),
                start: start[1],
                end: end[1],
                baseline: start[0],
                size: g.size,
            });
        }
        if own.len() == ids.len()
            && raw == w.raw
            && compact(&crate::normalize::word_text(&raw)) == compact(&w.text)
        {
            complete.insert(word, ids.clone());
            parts.extend(own);
        }
    }
    // A ruling's em box is not its ink. Only use smaller paint envelopes when
    // the complete original outline and drawing context prove them.
    let paint = (0..source.page().glyphs.len())
        .filter_map(|id| {
            let boxes = source
                .glyph_painted_rectangle(id)
                .map(|b| vec![b])
                .or_else(|| {
                    source
                        .glyph_separate_painted_rectangles(id)
                        .map(|v| v.to_vec())
                })
                .or_else(|| {
                    source
                        .glyph_rectangle_frame(id)
                        .map(|f| f.painted_rectangles().to_vec())
                })?;
            Some((id, boxes))
        })
        .collect::<BTreeMap<_, _>>();
    complete
        .iter()
        .filter_map(|(&word, ids)| {
            let own = ids
                .iter()
                .map(|id| parts.iter().find(|p| p.id == *id && p.word == word))
                .collect::<Option<Vec<_>>>()?;
            shifted(&own, &parts, source.page(), &paint)
        })
        .collect()
}

#[cfg(test)]
#[path = "source_baseline_shifts_tests.rs"]
mod tests;
