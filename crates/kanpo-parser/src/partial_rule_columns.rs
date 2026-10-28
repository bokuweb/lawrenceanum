//! A unique open horizontal divider may split only one outer strip of a tier.
//! Preserve the other strip as a whole, then read the divided strip top to bottom.
//! Closed cell grids, repeated row rules, crossing words and ambiguous cuts abstain.
use crate::vertical::Col;

pub(crate) fn parts(
    cols: &[Col],
    vrules: &[(f32, f32, f32)],
    hrules: &[(f32, f32, f32)],
) -> Option<Vec<Vec<Col>>> {
    if cols.is_empty()
        || cols.iter().any(|c| {
            ![c.x, c.x1, c.y, c.y1].iter().all(|v| v.is_finite())
                || c.x >= c.x1
                || c.y >= c.y1
                || c.text.is_empty()
        })
        || vrules
            .iter()
            .chain(hrules)
            .any(|r| ![r.0, r.1, r.2].iter().all(|v| v.is_finite()) || r.1 >= r.2)
    {
        return None;
    }
    let center = |c: &Col| c.x * 0.5 + c.x1 * 0.5;
    let xmin = cols.iter().map(&center).fold(f32::INFINITY, f32::min);
    let xmax = cols.iter().map(&center).fold(f32::NEG_INFINITY, f32::max);
    let mut plans = Vec::new();
    for &(y, x0, x1) in hrules {
        // A divider reaches precisely one occupied outer edge; its other end
        // must lie in an actual gutter, never through a source word.
        let (edge, left) = match (x0 <= xmin, xmax <= x1) {
            (true, false) => (x1, true),
            (false, true) => (x0, false),
            (true, true) => (
                cols.iter().map(|c| c.x1).fold(f32::NEG_INFINITY, f32::max),
                true,
            ),
            _ => continue,
        };
        if cols.iter().any(|c| c.x < edge && edge < c.x1) {
            continue;
        }
        let (side, outside): (Vec<_>, Vec<_>) =
            cols.iter().partition(|c| (center(c) < edge) == left);
        if side.is_empty()
            || (!outside.is_empty() && !outside.iter().any(|c| c.y < y && y < c.y1))
            || side.iter().any(|c| c.y < y && y < c.y1)
            || !side.iter().any(|c| c.y1 <= y)
            || !side.iter().any(|c| y <= c.y)
        {
            continue;
        }
        let sx0 = side.iter().map(|c| center(c)).fold(f32::INFINITY, f32::min);
        let sx1 = side
            .iter()
            .map(|c| center(c))
            .fold(f32::NEG_INFINITY, f32::max);
        let sy0 = side.iter().map(|c| c.y).fold(f32::INFINITY, f32::min);
        let sy1 = side.iter().map(|c| c.y1).fold(f32::NEG_INFINITY, f32::max);
        // A spanning vertical cell wall certifies a table, and a second
        // parallel spanning row rule makes this relationship ambiguous.
        if vrules
            .iter()
            .any(|&(x, a, b)| x0 <= x && x <= x1 && a < y && y < b)
            || hrules
                .iter()
                .filter(|&&(v, a, b)| sy0 < v && v < sy1 && a <= sx0 && sx1 <= b)
                .count()
                != 1
        {
            continue;
        }
        let above = side
            .iter()
            .filter(|c| c.y1 <= y)
            .map(|c| (*c).clone())
            .collect();
        let below = side
            .iter()
            .filter(|c| y <= c.y)
            .map(|c| (*c).clone())
            .collect();
        let outside = outside.into_iter().cloned().collect::<Vec<_>>();
        let mut plan = if left {
            vec![outside, above, below]
        } else {
            vec![above, below, outside]
        };
        plan.retain(|p| !p.is_empty());
        plans.push(plan);
    }
    if plans.len() == 1 {
        plans.pop()
    } else {
        None
    }
}

use crate::native_word_axes::NativeAxis;
use crate::trace::{Line, NativeBounds, Unit, Word};
use std::collections::HashSet;
fn compact(t: &str) -> String {
    t.chars().filter(|c| !c.is_whitespace()).collect()
}
fn source_columns(
    lines: &[Line],
    words: &[Word],
    bounds: &[NativeBounds],
    axes: &[NativeAxis],
) -> Option<Vec<Col>> {
    let mut ids = HashSet::new();
    let mut out = Vec::new();
    for l in lines {
        if l.table.is_some() || l.words.is_empty() {
            return None;
        }
        let mut text = String::new();
        for &id in &l.words {
            let w = words.get(id)?;
            if w.id != id
                || w.status != crate::trace::WordStatus::Used
                || !ids.insert(id)
                || axes.get(id) != Some(&NativeAxis::Down)
            {
                return None;
            }
            let NativeBounds::Glyphs(b) = bounds.get(id)? else {
                return None;
            };
            if !b.iter().all(|x| x.is_finite()) || b[0] >= b[2] || b[1] >= b[3] || w.text.is_empty()
            {
                return None;
            }
            text.push_str(&w.text);
            out.push(Col {
                id,
                x: b[0],
                y: b[1],
                x1: b[2],
                y1: b[3],
                text: w.text.clone(),
            });
        }
        if compact(&text) != compact(&l.text) {
            return None;
        }
    }
    Some(out)
}
pub(crate) fn restore(
    units: &mut [Unit],
    words: &[Word],
    bounds: &[NativeBounds],
    axes: &[NativeAxis],
    vrules: &[(f32, f32, f32)],
    hrules: &[(f32, f32, f32)],
    render: impl Fn(Vec<Col>) -> Vec<Line>,
) {
    // The same source occurrence cannot support two logical owners.
    let mut owned = HashSet::new();
    for u in units.iter() {
        let all: Vec<&Line> = match u {
            Unit::Plain(l) => l.iter().collect(),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            ..} => preamble
                .iter()
                .chain(after.iter().flatten())
                .chain(before.iter().flatten())
                .chain(outside.iter().flatten())
                .collect(),
        };
        for l in all {
            let mut ids: HashSet<_> = l.words.iter().copied().collect();
            if let Some(t) = &l.table {
                ids.extend(t.cells.iter().flat_map(|c| c.words.iter().copied()));
            }
            if ids.into_iter().any(|id| !owned.insert(id)) {
                return;
            }
        }
    }
    for u in units {
        let Unit::Plain(lines) = u else {
            continue;
        };
        let mut start = 0;
        while start < lines.len() {
            if source_columns(&lines[start..start + 1], words, bounds, axes).is_none() {
                start += 1;
                continue;
            }
            let mut end = start + 1;
            while end < lines.len()
                && source_columns(&lines[end..end + 1], words, bounds, axes).is_some()
            {
                end += 1;
            }
            if let Some(cols) = source_columns(&lines[start..end], words, bounds, axes) {
                if let Some(parts) = parts(&cols, vrules, hrules) {
                    let before = cols.iter().map(|c| c.id).collect::<Vec<_>>();
                    let after = parts
                        .iter()
                        .flat_map(|p| p.iter().map(|c| c.id))
                        .collect::<Vec<_>>();
                    if before != after {
                        let original: std::collections::HashMap<_, _> = lines[start..end]
                            .iter()
                            .map(|l| (l.words.clone(), l))
                            .collect();
                        let new = parts
                            .into_iter()
                            .flat_map(&render)
                            .map(|l| {
                                original
                                    .get(&l.words)
                                    .map_or(l.clone(), |old| (*old).clone())
                            })
                            .collect::<Vec<_>>();
                        // Verify the complete rewritten material and ownership before applying.
                        let expected: HashSet<_> = before.into_iter().collect();
                        let actual = new
                            .iter()
                            .flat_map(|l| l.words.iter().copied())
                            .collect::<Vec<_>>();
                        if actual.len() == expected.len()
                            && actual.iter().copied().collect::<HashSet<_>>() == expected
                            && compact(&new.iter().map(|l| l.text.as_str()).collect::<String>())
                                == compact(
                                    &after
                                        .iter()
                                        .map(|&id| words[id].text.as_str())
                                        .collect::<String>(),
                                )
                        {
                            let count = new.len();
                            lines.splice(start..end, new);
                            end = start + count;
                        }
                    }
                }
            }
            start = end;
        }
    }
}
