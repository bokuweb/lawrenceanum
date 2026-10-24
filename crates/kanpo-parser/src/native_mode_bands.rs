//! Separate disjoint native writing-mode bands only when their original source
//! baselines repair a demonstrable line-affinity conflict. No vocabulary or
//! guessed column width supplies evidence.
use super::*;
use crate::native_word_flow::{NativeFlow, NativeMode};
use crate::{native_word_axes::NativeAxis, trace::NativeBounds};
use std::collections::{HashMap, HashSet};
fn complete(cols: &[Col], s: SourceGeometry<'_>) -> Option<HashMap<usize, NativeFlow>> {
    if !s.sources_unchanged(cols) {
        return None;
    }
    let mut out = HashMap::new();
    for c in cols {
        if s.axes.get(c.id) != Some(&NativeAxis::Right) {
            return None;
        }
        let NativeBounds::Glyphs(b) = s.bounds.get(c.id)? else {
            return None;
        };
        let flow = (*s.flows.get(c.id)?)?;
        if !flow.belongs(*b) || flow.mode.is_none() || out.insert(c.id, flow).is_some() {
            return None;
        }
    }
    Some(out)
}
fn valid_rules(v: &[(f32, f32, f32)], h: &[(f32, f32, f32)]) -> bool {
    v.iter()
        .chain(h)
        .all(|&(a, b, c)| a.is_finite() && b.is_finite() && c.is_finite() && b < c)
}
pub(super) fn cut(
    cols: &[Col],
    s: SourceGeometry<'_>,
    vrules: &[(f32, f32, f32)],
    hrules: &[(f32, f32, f32)],
) -> Option<f32> {
    let flows = complete(cols, s)?;
    if !s
        .enclosures
        .iter()
        .all(|b| b.iter().all(|n| n.is_finite()) && b[0] < b[2] && b[1] < b[3])
    {
        return None;
    }
    if !valid_rules(vrules, hrules) {
        return None;
    }
    let mut bands = [(f32::INFINITY, f32::NEG_INFINITY); 2];
    let mut top = f32::INFINITY;
    let mut bottom = f32::NEG_INFINITY;
    for c in cols {
        let NativeBounds::Glyphs(b) = s.bounds[c.id] else {
            unreachable!()
        };
        let i = usize::from(flows[&c.id].mode == Some(NativeMode::Vertical));
        bands[i].0 = bands[i].0.min(b[0]);
        bands[i].1 = bands[i].1.max(b[2]);
        top = top.min(b[1]);
        bottom = bottom.max(b[3]);
    }
    if bands.iter().any(|&(a, b)| a >= b) {
        return None;
    }
    let (left, right) = if bands[0].1 < bands[1].0 {
        (0, 1)
    } else if bands[1].1 < bands[0].0 {
        (1, 0)
    } else {
        return None;
    };
    let x = ((f64::from(bands[left].1) + f64::from(bands[right].0)) / 2.) as f32;
    if !(bands[left].1 < x && x < bands[right].0)
        || hrules
            .iter()
            .any(|&(y, a, b)| a < x && x < b && top < y && y < bottom)
    {
        return None;
    }
    // The existing projection must agree with the exact source partition. A
    // compatibility rectangle cannot override explicit native geometry.
    if cols.iter().any(|c| {
        let side = usize::from(flows[&c.id].mode == Some(NativeMode::Vertical));
        if side == left {
            c.x1 > x
        } else {
            c.x < x
        }
    }) {
        return None;
    }
    Some(x)
}
fn same(a: NativeFlow, b: NativeFlow) -> bool {
    a.mode == b.mode && a.baseline == b.baseline
}
fn index(
    lines: &[trace::Line],
    cols: &HashMap<usize, &Col>,
) -> Option<HashMap<usize, (usize, usize)>> {
    let mut out = HashMap::new();
    for (i, l) in lines.iter().enumerate() {
        if l.table.is_some() || l.words.is_empty() {
            return None;
        }
        let cs: Option<Vec<Col>> = l
            .words
            .iter()
            .map(|id| cols.get(id).map(|c| (*c).clone()))
            .collect();
        if render_line(&cs?) != l.text {
            return None;
        }
        for (j, &id) in l.words.iter().enumerate() {
            if out.insert(id, (i, j)).is_some() {
                return None;
            }
        }
    }
    (out.len() == cols.len()).then_some(out)
}
pub(super) fn preserves_source_lines(
    cols: &[Col],
    s: SourceGeometry<'_>,
    before: &[trace::Line],
    after: &[trace::Line],
    vrules: &[(f32, f32, f32)],
    hrules: &[(f32, f32, f32)],
) -> bool {
    fn prove(
        cols: &[Col],
        s: SourceGeometry<'_>,
        before: &[trace::Line],
        after: &[trace::Line],
        vrules: &[(f32, f32, f32)],
        hrules: &[(f32, f32, f32)],
    ) -> Option<()> {
        let flows = complete(cols, s)?;
        cut(cols, s, vrules, hrules)?;
        let cs: HashMap<usize, &Col> = cols.iter().map(|c| (c.id, c)).collect();
        let old = index(before, &cs)?;
        let new = index(after, &cs)?;
        let homogeneous = |l: &trace::Line| {
            let f = flows[&l.words[0]];
            l.words.iter().all(|id| flows[id].mode == f.mode)
        };
        let mut last = [None, None];
        for l in before {
            // A homogeneous old line, or a maximal same-mode run in a mixed old line,
            // must remain whole, contiguous and in the original word order.
            let mut start = 0;
            while start < l.words.len() {
                let mode = flows[&l.words[start]].mode;
                let end = (start + 1..l.words.len())
                    .find(|&i| flows[&l.words[i]].mode != mode)
                    .unwrap_or(l.words.len());
                let &(line, at) = new.get(&l.words[start])?;
                if after[line].words.get(at..at + end - start) != Some(&l.words[start..end]) {
                    return None;
                }
                start = end;
            }
            if homogeneous(l) {
                let key = usize::from(flows[&l.words[0]].mode == Some(NativeMode::Vertical));
                let pos = new[&l.words[0]];
                if last[key].is_some_and(|previous| previous >= pos) {
                    return None;
                }
                last[key] = Some(pos);
            }
        }
        let mut repaired = false;
        for l in after {
            let origins: HashSet<_> = l.words.iter().map(|id| old[id].0).collect();
            if origins.len() == 1 {
                continue;
            }
            let key = flows[&l.words[0]];
            if !l.words.iter().all(|id| same(flows[id], key)) {
                return None;
            }
            let targets: Vec<_> = before
                .iter()
                .enumerate()
                .filter(|(_, oldline)| oldline.words.iter().all(|id| same(flows[id], key)))
                .map(|(i, _)| i)
                .collect();
            let [target] = targets.as_slice() else {
                return None;
            };
            if !origins.contains(target) {
                return None;
            }
            for &origin in &origins {
                if origin == *target {
                    continue;
                }
                let host = &before[origin];
                if homogeneous(host) {
                    return None;
                }
                // Different source modes on the same native baseline may be intentional
                // inline typography. Only a conflicting baseline supplies this witness.
                if !host.words.iter().any(|id| flows[id].mode != key.mode)
                    || host
                        .words
                        .iter()
                        .filter(|id| flows[id].mode != key.mode)
                        .any(|id| flows[id].baseline == key.baseline)
                {
                    return None;
                }
            }
            let mut b = [
                f32::INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
            ];
            for id in &l.words {
                let NativeBounds::Glyphs(q) = s.bounds[*id] else {
                    unreachable!()
                };
                b[0] = b[0].min(q[0]);
                b[1] = b[1].min(q[1]);
                b[2] = b[2].max(q[2]);
                b[3] = b[3].max(q[3]);
            }
            if vrules
                .iter()
                .any(|&(x, y0, y1)| b[0] < x && x < b[2] && y0 < b[3] && b[1] < y1)
                || hrules
                    .iter()
                    .any(|&(y, x0, x1)| b[1] < y && y < b[3] && x0 < b[2] && b[0] < x1)
            {
                return None;
            }
            // A closed native frame is independent of the table-rule lists.
            // A new source row may stay inside its own frame, but cannot join
            // across an edge, straddle it, or bridge a frame with no text.
            if s.enclosures.iter().any(|f| {
                let intersects = f[0] < b[2] && b[0] < f[2] && f[1] < b[3] && b[1] < f[3];
                let contains = f[0] <= b[0] && f[1] <= b[1] && b[2] <= f[2] && b[3] <= f[3];
                intersects && !contains
            }) {
                return None;
            }
            repaired = true;
        }
        repaired.then_some(())
    }
    prove(cols, s, before, after, vrules, hrules).is_some()
}
#[cfg(test)]
#[path = "native_mode_band_tests.rs"]
mod tests;
