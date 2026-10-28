//! Separate an atomic native lane from text beyond its original open fences.
use crate::native_word_flow::{NativeFlow, NativeMode};
use crate::{
    lines::PageRules,
    native_word_axes::NativeAxis,
    trace::{Line, NativeBounds, Unit, Word, WordStatus},
    vertical::Col,
};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Certificate {
    pub(crate) words: BTreeSet<usize>,
    pub(crate) frame: [f32; 4],
}
fn material<'a>(xs: impl IntoIterator<Item = &'a str>) -> BTreeMap<char, usize> {
    let mut out = BTreeMap::new();
    for c in xs
        .into_iter()
        .flat_map(str::chars)
        .filter(|c| !c.is_whitespace())
    {
        *out.entry(c).or_default() += 1
    }
    out
}
fn row(ids: &[usize], words: &[Word], bounds: &[NativeBounds]) -> Vec<Line> {
    crate::horizontal::row_lines(
        ids.iter()
            .map(|&id| {
                let NativeBounds::Glyphs(b) = bounds[id] else {
                    unreachable!()
                };
                Col {
                    id,
                    x: b[0],
                    y: b[1],
                    x1: b[2],
                    y1: b[3],
                    text: words[id].text.clone(),
                }
            })
            .collect(),
    )
}
fn candidate(
    lines: &[Line],
    words: &[Word],
    bounds: &[NativeBounds],
    axes: &[NativeAxis],
    flows: &[Option<NativeFlow>],
    rules: &PageRules,
) -> Option<(Vec<Line>, usize, Certificate)> {
    if lines
        .iter()
        .any(|l| l.table.is_some() || l.words.is_empty())
    {
        return None;
    }
    let ids = lines
        .iter()
        .flat_map(|l| l.words.iter().copied())
        .collect::<Vec<_>>();
    let own = ids.iter().copied().collect::<BTreeSet<_>>();
    if own.len() != ids.len() {
        return None;
    }
    let mut glyphs = BTreeMap::new();
    for &id in &ids {
        let w = words.get(id)?;
        if w.id != id || w.status != WordStatus::Used || axes.get(id) != Some(&NativeAxis::Right) {
            return None;
        }
        let NativeBounds::Glyphs(b) = bounds.get(id)? else {
            return None;
        };
        if !b.iter().all(|v| v.is_finite()) || b[0] >= b[2] || b[1] >= b[3] {
            return None;
        }
        let flow = flows.get(id)?.as_ref()?;
        if flow.mode != Some(NativeMode::Horizontal) || !flow.belongs(*b) {
            return None;
        }
        glyphs.insert(id, *b);
    }
    if material(lines.iter().map(|l| l.text.as_str()))
        != material(ids.iter().map(|&id| words[id].text.as_str()))
    {
        return None;
    }
    let top = glyphs.values().map(|b| b[1]).reduce(f32::min)?;
    let bottom = glyphs.values().map(|b| b[3]).reduce(f32::max)?;
    let baseline_top = ids
        .iter()
        .map(|&id| flows[id].unwrap().baseline)
        .reduce(f64::min)?;
    let baseline_bottom = ids
        .iter()
        .map(|&id| flows[id].unwrap().baseline)
        .reduce(f64::max)?;
    let mut fences = rules
        .vertical
        .iter()
        .filter(|r| {
            r.x.is_finite()
                && r.y0.is_finite()
                && r.y1.is_finite()
                && r.y0 < r.y1
                && r.y0 as f64 <= baseline_top
                && baseline_bottom <= r.y1 as f64
        })
        .filter(|r| {
            !rules
                .horizontal
                .iter()
                .any(|h| top < h.y && h.y < bottom && h.x0 < r.x && r.x < h.x1)
        })
        .filter(|r| glyphs.values().all(|b| b[2] <= r.x || r.x <= b[0]))
        .collect::<Vec<_>>();
    fences.sort_by(|a, b| a.x.total_cmp(&b.x));
    let mut possibilities = vec![];
    for pair in fences.windows(2) {
        let (left, right) = (pair[0].x, pair[1].x);
        let frame = [
            left,
            pair[0].y0.max(pair[1].y0),
            right,
            pair[0].y1.min(pair[1].y1),
        ];
        if frame[0] >= frame[2] || frame[1] >= frame[3] {
            continue;
        }
        if rules
            .enclosures
            .iter()
            .any(|e| e[0] < frame[2] && frame[0] < e[2] && e[1] < frame[3] && frame[1] < e[3])
        {
            continue;
        }
        if rules
            .horizontal
            .iter()
            .any(|h| frame[1] <= h.y && h.y <= frame[3] && h.x0 < right && left < h.x1)
        {
            continue;
        }
        let lane = own
            .iter()
            .copied()
            .filter(|id| left <= glyphs[id][0] && glyphs[id][2] <= right)
            .collect::<BTreeSet<_>>();
        if lane.is_empty() {
            continue;
        }
        // An atomic original baseline, independent of PDF word wrappers, is
        // not a limit on prose length or on the number of emitted IR lines.
        let y0 = lane.iter().map(|id| glyphs[id][1]).reduce(f32::max)?;
        let y1 = lane.iter().map(|id| glyphs[id][3]).reduce(f32::min)?;
        if y0 >= y1 {
            continue;
        }
        let baseline = flows[*lane.first()?].unwrap().baseline;
        if lane
            .iter()
            .any(|&id| flows[id].unwrap().baseline != baseline)
        {
            continue;
        }
        // No word elsewhere in the page may belong to this same physical lane.
        if words.iter().any(|w|w.status==WordStatus::Used&&!own.contains(&w.id)&&matches!(bounds.get(w.id),Some(NativeBounds::Glyphs(b))if left<=b[0]&&b[2]<=right&&b[1]<pair[0].y1.min(pair[1].y1)&&pair[0].y0.max(pair[1].y0)<b[3])){continue}
        let mixed = lines.iter().any(|l| {
            l.words.iter().any(|id| lane.contains(id))
                && l.words.iter().any(|id| !lane.contains(id))
        });
        if !mixed {
            continue;
        }
        let atomic = row(&lane.iter().copied().collect::<Vec<_>>(), words, bounds);
        if atomic.len() != 1 {
            continue;
        }
        let mut remainder = vec![];
        let mut invalid = false;
        for l in lines {
            if l.words.iter().all(|id| !lane.contains(id)) {
                remainder.push(l.clone());
                continue;
            }
            let rest = l
                .words
                .iter()
                .copied()
                .filter(|id| !lane.contains(id))
                .collect::<Vec<_>>();
            if !rest.is_empty() {
                remainder.extend(row(&rest, words, bounds))
            }
        }
        let mut right_seen = false;
        let mut insert = remainder.len();
        for (i, l) in remainder.iter().enumerate() {
            let left_side = l.words.iter().all(|id| glyphs[id][2] <= left);
            let right_side = l.words.iter().all(|id| right <= glyphs[id][0]);
            if right_side {
                if !right_seen {
                    insert = i;
                    right_seen = true
                }
            } else if !left_side || right_seen {
                invalid = true;
                break;
            }
        }
        if invalid {
            continue;
        }
        remainder.insert(insert, atomic[0].clone());
        if material(remainder.iter().map(|l| l.text.as_str()))
            != material(lines.iter().map(|l| l.text.as_str()))
        {
            continue;
        }
        let used = remainder
            .iter()
            .flat_map(|l| l.words.iter().copied())
            .collect::<Vec<_>>();
        if used.len() != ids.len() || used.iter().copied().collect::<BTreeSet<_>>() != own {
            continue;
        }
        // A baseline can certify the neighboring prose's ruled span, while
        // every glyph in the atomic lane itself must remain inside its walls.
        // Reject the candidate as a whole; do not resolve an ambiguity by
        // dropping one incompletely enclosed interpretation.
        if lane
            .iter()
            .any(|id| glyphs[id][1] < frame[1] || frame[3] < glyphs[id][3])
        {
            return None;
        }
        possibilities.push((remainder, insert, Certificate { words: lane, frame }));
    }
    if possibilities.len() == 1 {
        possibilities.pop()
    } else {
        None
    }
}
pub(crate) fn restore(
    units: &mut Vec<Unit>,
    words: &[Word],
    bounds: &[NativeBounds],
    axes: &[NativeAxis],
    flows: &[Option<NativeFlow>],
    rules: Option<&PageRules>,
) -> Vec<Certificate> {
    let Some(rules) = rules else { return vec![] };
    // Comparative roles and typed tables retain their existing source scope.
    let mut owners = vec![0usize; words.len()];
    for unit in units.iter() {
        let Unit::Plain(lines) = unit else {
            return vec![];
        };
        for line in lines {
            if line.table.is_some() {
                return vec![];
            }
            for &id in &line.words {
                let Some(n) = owners.get_mut(id) else {
                    return vec![];
                };
                *n += 1;
                if *n > 1 {
                    return vec![];
                }
            }
        }
    }
    for (id, w) in words.iter().enumerate() {
        if owners[id] > 0 && w.status != WordStatus::Used {
            return vec![];
        }
        if w.status != WordStatus::Used {
            continue;
        }
        let Some(NativeBounds::Glyphs(b)) = bounds.get(id) else {
            return vec![];
        };
        let Some(Some(flow)) = flows.get(id) else {
            return vec![];
        };
        if w.id != id
            || w.text.is_empty()
            || axes.get(id) != Some(&NativeAxis::Right)
            || flow.mode != Some(NativeMode::Horizontal)
            || !flow.belongs(*b)
        {
            return vec![];
        }
    }
    let mut result = vec![];
    let mut certificates = vec![];
    for unit in std::mem::take(units) {
        let Unit::Plain(lines) = unit else {
            unreachable!()
        };
        if let Some((mut new, at, certificate)) =
            candidate(&lines, words, bounds, axes, flows, rules)
        {
            certificates.push(certificate);
            let after = new.split_off(at + 1);
            let atom = new.pop().unwrap();
            if !new.is_empty() {
                result.push(Unit::Plain(new))
            }
            result.push(Unit::Plain(vec![atom]));
            if !after.is_empty() {
                result.push(Unit::Plain(after))
            }
        } else {
            result.push(Unit::Plain(lines))
        }
    }
    *units = result;
    certificates
}

#[cfg(test)]
#[path = "native_atomic_column_tests.rs"]
mod tests;
