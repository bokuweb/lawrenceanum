//! Keep the flow inside each source-ruled cell without inventing a new Table.
use crate::{
    grid::Grid,
    lines::PageRules,
    native_word_axes::NativeAxis,
    trace::{Line, NativeBounds, Unit, Word, WordStatus},
    vertical::Col,
};
use std::collections::{BTreeMap, HashMap, HashSet};
type Rect = [f32; 4];
struct Candidate {
    outer: Rect,
    envelope: Rect,
    rowwise: bool,
    physical_rows: Vec<(usize, usize)>,
    members: BTreeMap<usize, usize>,
    order: Vec<usize>,
}
fn rect(b: Rect) -> bool {
    b.iter().all(|x| x.is_finite()) && b[0] < b[2] && b[1] < b[3]
}
fn intersects(a: Rect, b: Rect) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}
fn contains(a: Rect, b: Rect) -> bool {
    a[0] <= b[0] && a[1] <= b[1] && b[2] <= a[2] && b[3] <= a[3]
}
fn cell_rect(g: &Grid, i: usize) -> Rect {
    let c = g.cells[i];
    [g.xs[c.x0], g.ys[c.y0], g.xs[c.x1], g.ys[c.y1]]
}
pub(crate) fn partition(g: &Grid) -> bool {
    if g.xs.len() < 2 || g.ys.len() < 2 || g.cells.len() < 2 {
        return false;
    }
    if g.xs.iter().chain(&g.ys).any(|x| !x.is_finite())
        || g.xs.windows(2).any(|w| w[0] >= w[1])
        || g.ys.windows(2).any(|w| w[0] >= w[1])
    {
        return false;
    }
    let (nx, ny) = (g.xs.len() - 1, g.ys.len() - 1);
    let Some(n) = nx.checked_mul(ny) else {
        return false;
    };
    let mut slots = vec![false; n];
    for c in &g.cells {
        if c.x0 >= c.x1 || c.y0 >= c.y1 || c.x1 > nx || c.y1 > ny {
            return false;
        }
        for y in c.y0..c.y1 {
            for x in c.x0..c.x1 {
                let i = y * nx + x;
                if slots[i] {
                    return false;
                }
                slots[i] = true;
            }
        }
    }
    slots.into_iter().all(|x| x)
}
pub(crate) fn cover(mut intervals: Vec<(f32, f32)>, start: f32, end: f32) -> bool {
    if !start.is_finite() || !end.is_finite() || start >= end {
        return false;
    }
    intervals.retain(|&(a, b)| a.is_finite() && b.is_finite() && a < b);
    intervals.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut cursor = start;
    for (a, b) in intervals {
        if b <= cursor {
            continue;
        }
        if a > cursor {
            return false;
        }
        cursor = cursor.max(b);
        if cursor >= end {
            return true;
        }
    }
    false
}
pub(crate) fn fence(a: Rect, b: Rect, ga: Rect, gb: Rect, rules: &PageRules) -> bool {
    if ga[3] <= gb[1] || gb[3] <= ga[1] {
        let (top, bottom) = if ga[3] <= gb[1] { (a, b) } else { (b, a) };
        rules
            .horizontal
            .iter()
            .filter(|h| h.y.is_finite() && top[3] <= h.y && h.y <= bottom[1])
            .any(|seed| {
                cover(
                    rules
                        .horizontal
                        .iter()
                        .filter(|h| h.y == seed.y)
                        .map(|h| (h.x0, h.x1))
                        .collect(),
                    top[0].min(bottom[0]),
                    top[2].max(bottom[2]),
                )
            })
    } else if ga[2] <= gb[0] || gb[2] <= ga[0] {
        let (left, right) = if ga[2] <= gb[0] { (a, b) } else { (b, a) };
        rules
            .vertical
            .iter()
            .filter(|v| v.x.is_finite() && left[2] <= v.x && v.x <= right[0])
            .any(|seed| {
                cover(
                    rules
                        .vertical
                        .iter()
                        .filter(|v| v.x == seed.x)
                        .map(|v| (v.y0, v.y1))
                        .collect(),
                    left[1].min(right[1]),
                    left[3].max(right[3]),
                )
            })
    } else {
        false
    }
}
fn visit(units: &[Unit], mut f: impl FnMut(&[Line])) {
    for unit in units {
        match unit {
            Unit::Plain(ls) => f(ls),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            } => {
                f(preamble);
                for ls in after.iter().chain(before).chain(outside) {
                    f(ls);
                }
            }
        }
    }
}
fn visit_mut(units: &mut [Unit], mut f: impl FnMut(&mut Vec<Line>)) {
    for unit in units {
        match unit {
            Unit::Plain(ls) => f(ls),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            } => {
                f(preamble);
                for ls in after
                    .iter_mut()
                    .chain(before.iter_mut())
                    .chain(outside.iter_mut())
                {
                    f(ls);
                }
            }
        }
    }
}
fn candidates(
    words: &[Word],
    bounds: &[NativeBounds],
    axes: &[NativeAxis],
    grids: &[Grid],
    rules: &PageRules,
    counts: &HashMap<usize, usize>,
    tables: &HashSet<usize>,
) -> Vec<Candidate> {
    // Complete original membership cannot be proved from a partly annotated page.
    let mut boxes = vec![None; words.len()];
    for (i, w) in words.iter().enumerate() {
        if w.status != WordStatus::Used {
            continue;
        }
        if w.id != i {
            return vec![];
        }
        let Some(NativeBounds::Glyphs(b)) = bounds.get(i) else {
            return vec![];
        };
        if !rect(*b) {
            return vec![];
        }
        boxes[i] = Some(*b);
    }
    let mut plans = Vec::new();
    for g in grids {
        if !partition(g) || grids.iter().any(|other| g.encloses(other)) {
            continue;
        }
        let outer = [g.left(), g.top(), g.right(), g.bottom()];
        let mut members = BTreeMap::new();
        let mut envelopes: BTreeMap<usize, Rect> = BTreeMap::new();
        let mut valid = true;
        for (id, b) in boxes.iter().enumerate() {
            let Some(b) = b else { continue };
            if !intersects(outer, *b) {
                continue;
            }
            let cells: Vec<_> = (0..g.cells.len())
                .filter(|&ci| contains(cell_rect(g, ci), *b))
                .collect();
            if cells.len() != 1
                || axes.get(id) != Some(&NativeAxis::Right)
                || counts.get(&id) != Some(&1)
                || tables.contains(&id)
            {
                valid = false;
                break;
            }
            let ci = cells[0];
            members.insert(id, ci);
            envelopes
                .entry(ci)
                .and_modify(|r| {
                    r[0] = r[0].min(b[0]);
                    r[1] = r[1].min(b[1]);
                    r[2] = r[2].max(b[2]);
                    r[3] = r[3].max(b[3]);
                })
                .or_insert(*b);
        }
        if !valid || envelopes.len() < 2 {
            continue;
        }
        let occupied: Vec<_> = envelopes.keys().copied().collect();
        for (i, &a) in occupied.iter().enumerate() {
            for &b in &occupied[i + 1..] {
                if !fence(
                    envelopes[&a],
                    envelopes[&b],
                    cell_rect(g, a),
                    cell_rect(g, b),
                    rules,
                ) {
                    valid = false;
                }
            }
        }
        if !valid {
            continue;
        }
        let mut order = occupied;
        order.sort_by_key(|&i| {
            let c = g.cells[i];
            (c.y0, c.x0, c.y1, c.x1)
        });
        plans.push(Candidate {
            outer,
            envelope: envelopes.values().copied().reduce(union).unwrap(),
            rowwise: repeated_row_correspondence(&members, &boxes),
            physical_rows: g.cells.iter().map(|c| (c.y0, c.y1)).collect(),
            members,
            order,
        });
    }
    // Equal or overlapping source grids must not lend the same occurrence to two plans.
    let mut owners = HashMap::new();
    for p in &plans {
        for &id in p.members.keys() {
            *owners.entry(id).or_insert(0usize) += 1;
        }
    }
    plans.retain(|p| p.members.keys().all(|id| owners[id] == 1));
    plans
}
// Native glyph intervals define typographic bands without a font-size tolerance.
// Repeated alignment across cells is evidence of records, including blank entries.
fn repeated_row_correspondence(members: &BTreeMap<usize, usize>, boxes: &[Option<Rect>]) -> bool {
    let mut cells: BTreeMap<usize, Vec<(f32, f32)>> = BTreeMap::new();
    for (&id, &ci) in members {
        let b = boxes[id].unwrap();
        cells.entry(ci).or_default().push((b[1], b[3]));
    }
    let bands: Vec<Vec<(f32, f32)>> = cells
        .into_values()
        .map(|mut intervals| {
            intervals.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut merged: Vec<(f32, f32)> = Vec::new();
            for (a, b) in intervals {
                if let Some(last) = merged.last_mut().filter(|last| a <= last.1) {
                    last.1 = last.1.max(b);
                } else {
                    merged.push((a, b));
                }
            }
            merged
        })
        .collect();
    for (i, a) in bands.iter().enumerate() {
        for b in &bands[i + 1..] {
            let (mut ai, mut bi, mut matches) = (0, 0, 0);
            while ai < a.len() && bi < b.len() {
                if a[ai].0 < b[bi].1 && b[bi].0 < a[ai].1 {
                    matches += 1;
                    // Two distinct bands in both cells are the minimum evidence
                    // of repetition; one tall glyph cannot count as two records.
                    if matches >= 2 {
                        return true;
                    }
                    ai += 1;
                    bi += 1;
                } else if a[ai].1 <= b[bi].0 {
                    ai += 1;
                } else {
                    bi += 1;
                }
            }
        }
    }
    false
}
// A logical row and a common native glyph band certify a printed record.
// Legacy line splitting is not a witness: the previous extractor may already
// have split each field into a separate line. Intervening paragraph lines or
// a different source frame prevent joining without any text classification.
fn preserve_printed_rows(
    candidate: Vec<Line>,
    bounds: &[NativeBounds],
    owner: &HashMap<usize, (usize, usize)>,
    plans: &[Candidate],
    words: &[Word],
) -> Vec<Line> {
    let key = |line: &Line| -> Option<(usize, (usize, usize))> {
        let &id = line.words.first()?;
        let &(pi, ci) = owner.get(&id)?;
        let row = plans[pi].physical_rows[ci];
        line.words
            .iter()
            .all(|id| {
                let (p, c) = owner[id];
                p == pi && plans[p].physical_rows[c] == row
            })
            .then_some((pi, row))
    };
    let band = |line: &Line| -> Option<(f32, f32)> {
        let mut lo = f32::NEG_INFINITY;
        let mut hi = f32::INFINITY;
        for &id in &line.words {
            let NativeBounds::Glyphs(b) = bounds.get(id)? else {
                return None;
            };
            lo = lo.max(b[1]);
            hi = hi.min(b[3]);
        }
        (lo < hi).then_some((lo, hi))
    };
    let mut out: Vec<Line> = Vec::new();
    for line in candidate {
        if let Some(last) = out.last() {
            if let (Some(k), Some(a), Some(b)) = (key(&line), band(last), band(&line)) {
                if key(last) == Some(k) && a.0.max(b.0) < a.1.min(b.1) {
                    let mut ids = last.words.clone();
                    ids.extend_from_slice(&line.words);
                    // Reconstruction must retain the candidate's exact source
                    // occurrence order and produce one typographic row.
                    if let Some(joined) = fragment(&ids, words) {
                        *out.last_mut().unwrap() = joined;
                        continue;
                    }
                }
            }
        }
        out.push(line);
    }
    out
}
fn preserve_complete_lines(candidate: Vec<Line>, baseline: &[Line]) -> Vec<Line> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < candidate.len() {
        let old = baseline
            .iter()
            .find(|old| old.words.first() == candidate[i].words.first());
        let mut consumed = None;
        if let Some(old) = old {
            let mut ids = Vec::new();
            for (offset, part) in candidate[i..].iter().enumerate() {
                ids.extend_from_slice(&part.words);
                if ids == old.words {
                    consumed = Some(offset + 1);
                    break;
                }
                if !old.words.starts_with(&ids) {
                    break;
                }
            }
            if let Some(n) = consumed {
                out.push(old.clone());
                i += n;
                continue;
            }
        }
        out.push(candidate[i].clone());
        i += 1;
    }
    out
}
fn cols(ids: &[usize], words: &[Word]) -> Vec<Col> {
    ids.iter()
        .map(|&id| {
            let w = &words[id];
            Col {
                id,
                x: w.x0,
                y: w.y0,
                x1: w.x1,
                y1: w.y1,
                text: w.text.clone(),
            }
        })
        .collect()
}
fn fragment(ids: &[usize], words: &[Word]) -> Option<Line> {
    let mut ls = crate::horizontal::row_lines(cols(ids, words));
    if ls.len() != 1 || ls[0].words != ids {
        return None;
    }
    Some(ls.remove(0))
}
fn char_count(lines: &[Line]) -> BTreeMap<char, usize> {
    let mut out = BTreeMap::new();
    for c in lines
        .iter()
        .flat_map(|l| l.text.chars())
        .filter(|c| !c.is_whitespace())
    {
        *out.entry(c).or_default() += 1;
    }
    out
}
struct Source<'a> {
    words: &'a [Word],
    bounds: &'a [NativeBounds],
    axes: &'a [NativeAxis],
    rules: &'a PageRules,
    counts: &'a HashMap<usize, usize>,
}
fn union(a: Rect, b: Rect) -> Rect {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}
// A ruling certifies order only across the shared orthogonal projection.
// Diagonal, disconnected supports have no implied reading order.
pub(crate) fn certified_before(a: Rect, ea: Rect, b: Rect, eb: Rect, rules: &PageRules) -> bool {
    if a[2] <= b[0] && a[1].max(b[1]) < a[3].min(b[3]) {
        return rules
            .vertical
            .iter()
            .filter(|v| v.x.is_finite() && ea[2] <= v.x && v.x <= eb[0])
            .any(|seed| {
                cover(
                    rules
                        .vertical
                        .iter()
                        .filter(|v| v.x == seed.x)
                        .map(|v| (v.y0, v.y1))
                        .collect(),
                    ea[1].max(eb[1]),
                    ea[3].min(eb[3]),
                )
            });
    }
    if a[3] <= b[1] && a[0].max(b[0]) < a[2].min(b[2]) {
        return rules
            .horizontal
            .iter()
            .filter(|h| h.y.is_finite() && ea[3] <= h.y && h.y <= eb[1])
            .any(|seed| {
                cover(
                    rules
                        .horizontal
                        .iter()
                        .filter(|h| h.y == seed.y)
                        .map(|h| (h.x0, h.x1))
                        .collect(),
                    ea[0].max(eb[0]),
                    ea[2].min(eb[2]),
                )
            });
    }
    false
}
// Outside text is opaque: retain its complete original occurrence order.
// A relocation needs both a predecessor and a successor, each independently
// certified by native rulings. Unbounded outside text keeps the legacy flow.
fn bounded_gap(
    ids: &[usize],
    group: &[(usize, usize)],
    plans: &[Candidate],
    source: &Source<'_>,
) -> Option<usize> {
    let mut envelope = None;
    for &id in ids {
        let w = source.words.get(id)?;
        if w.id != id
            || w.status != WordStatus::Used
            || source.axes.get(id) != Some(&NativeAxis::Right)
            || source.counts.get(&id) != Some(&1)
        {
            return None;
        }
        let NativeBounds::Glyphs(b) = source.bounds.get(id)? else {
            return None;
        };
        if !rect(*b) {
            return None;
        }
        envelope = Some(envelope.map_or(*b, |r| union(r, *b)));
    }
    let outside = envelope?;
    if plans.iter().any(|p| intersects(outside, p.outer)) {
        return None;
    }
    let before = |a: &Candidate, b: &Candidate| {
        certified_before(a.outer, a.envelope, b.outer, b.envelope, source.rules)
    };
    if !group
        .windows(2)
        .all(|pair| before(&plans[pair[0].1], &plans[pair[1].1]))
    {
        return None;
    }
    for (i, &(_, a)) in group.iter().enumerate() {
        if group[i + 1..]
            .iter()
            .any(|&(_, b)| before(&plans[b], &plans[a]))
        {
            return None;
        }
    }
    let preceding = group
        .iter()
        .enumerate()
        .filter(|(_, (_, pi))| {
            let p = &plans[*pi];
            certified_before(p.outer, p.envelope, outside, outside, source.rules)
        })
        .map(|(i, _)| i)
        .max()?;
    let following = group
        .iter()
        .enumerate()
        .filter(|(_, (_, pi))| {
            let p = &plans[*pi];
            certified_before(outside, outside, p.outer, p.envelope, source.rules)
        })
        .map(|(i, _)| i)
        .min()?;
    (preceding + 1 == following).then_some(following)
}
fn fragments(baseline: &[Line], ids: &HashSet<usize>, words: &[Word]) -> Option<Vec<Line>> {
    let mut out = Vec::new();
    for line in baseline {
        let selected: Vec<_> = line
            .words
            .iter()
            .copied()
            .filter(|id| ids.contains(id))
            .collect();
        if !selected.is_empty() {
            out.push(fragment(&selected, words)?);
        }
    }
    let selected: Vec<_> = out.iter().flat_map(|l| l.words.iter().copied()).collect();
    let source = cols(&selected, words);
    crate::horizontal::orthogonal_order_preserves_spans(&source, &out, &out).then_some(out)
}
fn rewrite(lines: &mut Vec<Line>, source: &Source<'_>, plans: &[Candidate]) {
    let (words, bounds) = (source.words, source.bounds);
    let mut positions = HashMap::new();
    let mut flat = 0;
    for (li, l) in lines.iter().enumerate() {
        for &id in &l.words {
            positions.insert(id, (li, flat));
            flat += 1;
        }
    }
    let mut windows = Vec::new();
    for (pi, p) in plans.iter().enumerate() {
        if p.members.keys().all(|id| positions.contains_key(id)) {
            let first = p.members.keys().map(|id| positions[id]).min().unwrap();
            let end = p.members.keys().map(|id| positions[id].0).max().unwrap();
            windows.push((first.0, end, first.1, pi));
        }
    }
    windows.sort_by_key(|w| (w.0, w.2));
    let mut groups: Vec<(usize, usize, Vec<(usize, usize)>)> = Vec::new();
    for (start, end, first, pi) in windows {
        if let Some(last) = groups.last_mut().filter(|g| start <= g.1) {
            last.1 = last.1.max(end);
            last.2.push((first, pi));
        } else {
            groups.push((start, end, vec![(first, pi)]));
        }
    }
    for (start, end, mut group) in groups.into_iter().rev() {
        group.sort_by_key(|p| p.0);
        let mut owner = HashMap::new();
        for &(_, pi) in &group {
            for (&id, &ci) in &plans[pi].members {
                owner.insert(id, (pi, ci));
            }
        }
        let baseline = &lines[start..=end];
        if baseline
            .iter()
            .any(|l| l.table.is_some() || l.words.is_empty())
        {
            continue;
        }
        // Text reconstructed from original occurrences must still equal the untouched
        // current line. Never erase earlier repairs or a merged constituent's text.
        if baseline
            .iter()
            .any(|l| fragment(&l.words, words).as_ref() != Some(l))
        {
            continue;
        }
        let foreign: Vec<_> = baseline
            .iter()
            .flat_map(|l| l.words.iter().copied())
            .filter(|id| !owner.contains_key(id))
            .collect();
        let outside = if foreign.is_empty() {
            None
        } else {
            let Some(slot) = bounded_gap(&foreign, &group, plans, source) else {
                continue;
            };
            let Some(parts) = fragments(baseline, &foreign.iter().copied().collect(), words) else {
                continue;
            };
            Some((slot, parts))
        };
        let mut candidate = Vec::new();
        let mut valid = true;
        for (at, &(_, pi)) in group.iter().enumerate() {
            if let Some((slot, parts)) = &outside {
                if at == *slot {
                    candidate.extend(parts.iter().cloned());
                }
            }
            let p = &plans[pi];
            let cell_groups: Vec<Option<usize>> = if p.rowwise {
                vec![None]
            } else {
                p.order.iter().copied().map(Some).collect()
            };
            for ci in cell_groups {
                let mut cell_lines = Vec::new();
                for line in baseline {
                    let ids: Vec<_> = line
                        .words
                        .iter()
                        .copied()
                        .filter(|id| {
                            owner.get(id).is_some_and(|&(p, c)| {
                                p == pi && ci.is_none_or(|wanted| c == wanted)
                            })
                        })
                        .collect();
                    if ids.is_empty() {
                        continue;
                    }
                    match fragment(&ids, words) {
                        Some(l) => cell_lines.push(l),
                        None => {
                            valid = false;
                            break;
                        }
                    }
                }
                let ids: Vec<_> = cell_lines
                    .iter()
                    .flat_map(|l| l.words.iter().copied())
                    .collect();
                let source = cols(&ids, words);
                if !crate::horizontal::orthogonal_order_preserves_spans(
                    &source,
                    &cell_lines,
                    &cell_lines,
                ) {
                    valid = false;
                }
                candidate.extend(cell_lines);
            }
        }
        if !valid {
            continue;
        }
        let candidate = preserve_printed_rows(candidate, bounds, &owner, plans, words);
        let candidate = preserve_complete_lines(candidate, baseline);
        let before: Vec<_> = baseline
            .iter()
            .flat_map(|l| l.words.iter().copied())
            .collect();
        let after: Vec<_> = candidate
            .iter()
            .flat_map(|l| l.words.iter().copied())
            .collect();
        if before == after {
            continue;
        }
        let mut a = before.clone();
        let mut b = after.clone();
        a.sort_unstable();
        b.sort_unstable();
        if a != b
            || a.windows(2).any(|w| w[0] == w[1])
            || char_count(baseline) != char_count(&candidate)
        {
            continue;
        }
        lines.splice(start..=end, candidate);
    }
}
pub(crate) fn restore(
    units: &mut [Unit],
    words: &[Word],
    bounds: &[NativeBounds],
    axes: &[NativeAxis],
    grids: &[Grid],
    rules: &PageRules,
) {
    let mut counts = HashMap::new();
    let mut tables = HashSet::new();
    visit(units, |ls| {
        for l in ls {
            for &id in &l.words {
                *counts.entry(id).or_insert(0usize) += 1;
            }
            if let Some(t) = &l.table {
                for c in &t.cells {
                    for &id in &c.words {
                        *counts.entry(id).or_insert(0usize) += 1;
                        tables.insert(id);
                    }
                }
            }
        }
    });
    let plans = candidates(words, bounds, axes, grids, rules, &counts, &tables);
    if plans.is_empty() {
        return;
    }
    let source = Source {
        words,
        bounds,
        axes,
        rules,
        counts: &counts,
    };
    visit_mut(units, |lines| rewrite(lines, &source, &plans));
}
#[cfg(test)]
#[path = "ruled_cell_flow_tests.rs"]
mod tests;
