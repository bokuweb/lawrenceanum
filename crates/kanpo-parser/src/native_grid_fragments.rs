//! Reassemble fragmented tables only inside a complete original native grid.
//! No record vocabulary, value, row count or physical distance selects a grid.
use crate::{
    grid::{find_grids, Grid},
    lines::PageRules,
    native_word_axes::NativeAxis,
    trace::{Line, NativeBounds, Table, TableCell, Unit, Word, WordStatus},
    vertical::Col,
};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
type Rect = [f32; 4];
fn valid(b: Rect) -> bool {
    b.iter().all(|v| v.is_finite()) && b[0] < b[2] && b[1] < b[3]
}
fn intersects(a: Rect, b: Rect) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}
fn contains(a: Rect, b: Rect) -> bool {
    a[0] <= b[0] && b[2] <= a[2] && a[1] <= b[1] && b[3] <= a[3]
}
fn outer(g: &Grid) -> Rect {
    [g.left(), g.top(), g.right(), g.bottom()]
}
fn cell(g: &Grid, i: usize) -> Rect {
    let c = g.cells[i];
    [g.xs[c.x0], g.ys[c.y0], g.xs[c.x1], g.ys[c.y1]]
}
fn table_box(t: &Table) -> Rect {
    [t.x0, t.y0, t.x1, t.y1]
}
fn material<'a>(s: impl IntoIterator<Item = &'a str>) -> BTreeMap<char, usize> {
    let mut out = BTreeMap::new();
    for c in s
        .into_iter()
        .flat_map(str::chars)
        .filter(|c| !c.is_whitespace())
    {
        *out.entry(c).or_default() += 1;
    }
    out
}
fn ids(l: &Line) -> Option<BTreeSet<usize>> {
    let plain = l.words.iter().copied().collect::<BTreeSet<_>>();
    if plain.len() != l.words.len() {
        return None;
    }
    let Some(t) = &l.table else {
        return Some(plain);
    };
    let all = t
        .cells
        .iter()
        .flat_map(|c| c.words.iter().copied())
        .collect::<Vec<_>>();
    let inside = all.iter().copied().collect::<BTreeSet<_>>();
    if inside.len() != all.len() || (!plain.is_empty() && plain != inside) {
        return None;
    }
    Some(inside)
}
fn visit(units: &[Unit], mut f: impl FnMut(bool, &Line)) {
    for u in units {
        match u {
            Unit::Plain(l) => {
                for l in l {
                    f(true, l)
                }
            }
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            } => {
                for l in preamble
                    .iter()
                    .chain(after.iter().flatten())
                    .chain(before.iter().flatten())
                {
                    f(false, l)
                }
                for l in outside.iter().flatten() {
                    f(true, l)
                }
            }
        }
    }
}
fn replace(units: &mut [Unit], selected: &HashSet<usize>, first: usize, replacement: Line) {
    let mut at = 0;
    let mut edit = |lines: &mut Vec<Line>| {
        let mut out = Vec::new();
        for l in std::mem::take(lines) {
            if at == first {
                out.push(replacement.clone());
            }
            if !selected.contains(&at) {
                out.push(l);
            }
            at += 1;
        }
        *lines = out;
    };
    for u in units {
        match u {
            Unit::Plain(l) => edit(l),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            } => {
                edit(preamble);
                for l in after
                    .iter_mut()
                    .chain(before.iter_mut())
                    .chain(outside.iter_mut())
                {
                    edit(l)
                }
            }
        }
    }
}
fn candidate(
    units: &[Unit],
    words: &[Word],
    bounds: &[NativeBounds],
    axes: &[NativeAxis],
    g: &Grid,
    rules: &PageRules,
) -> Option<(HashSet<usize>, usize, Line)> {
    if !crate::ruled_cell_flow::partition(g) {
        return None;
    }
    let frame = outer(g);
    if !valid(frame) {
        return None;
    }
    let mut members = BTreeMap::new();
    let mut envelopes: HashMap<usize, Rect> = HashMap::new();
    for (id, w) in words.iter().enumerate() {
        if w.status != WordStatus::Used {
            continue;
        }
        if w.id != id {
            return None;
        }
        let NativeBounds::Glyphs(b) = bounds.get(id)? else {
            return None;
        };
        if !valid(*b) {
            return None;
        }
        if !intersects(frame, *b) {
            continue;
        }
        if axes.get(id) != Some(&NativeAxis::Right) || w.text.is_empty() {
            return None;
        }
        let cells = (0..g.cells.len())
            .filter(|&ci| contains(cell(g, ci), *b))
            .collect::<Vec<_>>();
        if cells.len() != 1 {
            return None;
        }
        let ci = cells[0];
        members.insert(id, ci);
        envelopes
            .entry(ci)
            .and_modify(|e| {
                e[0] = e[0].min(b[0]);
                e[1] = e[1].min(b[1]);
                e[2] = e[2].max(b[2]);
                e[3] = e[3].max(b[3]);
            })
            .or_insert(*b);
    }
    if envelopes.len() < 2 {
        return None;
    }
    let occupied = envelopes.keys().copied().collect::<Vec<_>>();
    for (i, &a) in occupied.iter().enumerate() {
        for &b in &occupied[i + 1..] {
            if !crate::ruled_cell_flow::fence(
                envelopes[&a],
                envelopes[&b],
                cell(g, a),
                cell(g, b),
                rules,
            ) {
                return None;
            }
        }
    }
    let mut flat = Vec::new();
    visit(units, |allowed, l| flat.push((allowed, l.clone())));
    let mut owners: HashMap<usize, usize> = HashMap::new();
    for (_, l) in &flat {
        for id in ids(l)? {
            *owners.entry(id).or_default() += 1;
        }
    }
    if members.keys().any(|id| owners.get(id) != Some(&1)) {
        return None;
    }
    let all = members.keys().copied().collect::<BTreeSet<_>>();
    let mut selected = HashSet::new();
    let mut anchored = false;
    // A painted cell can contain several already recovered logical records.
    // Missing internal rules do not authorize merging those separate fields.
    let mut logical_fields = HashSet::new();
    for (i, (allowed, l)) in flat.iter().enumerate() {
        let own = ids(l)?;
        if own.is_disjoint(&all) {
            continue;
        }
        // Mixed unknown text, comparative roles or a table crossing the source
        // frame cannot be borrowed as evidence for this complete grid.
        if !allowed || !own.is_subset(&all) {
            return None;
        }
        if crate::segment::is_heading_mark(l.text.trim())
            || crate::segment::is_section_label(&l.text)
        {
            return None;
        }
        if let Some(t) = &l.table {
            if !contains(frame, table_box(t)) {
                return None;
            }
            for old in &t.cells {
                let targets = old
                    .words
                    .iter()
                    .map(|id| members[id])
                    .collect::<BTreeSet<_>>();
                if targets.len() == 1 && !logical_fields.insert(*targets.first()?) {
                    return None;
                }
            }
            anchored = true;
        }
        selected.insert(i);
    }
    let first = *selected.iter().min()?;
    let last = *selected.iter().max()?;
    if !anchored || (first..=last).any(|i| !selected.contains(&i)) {
        return None;
    }
    let old = flat[first..=last]
        .iter()
        .map(|(_, l)| l)
        .collect::<Vec<_>>();
    if crate::segment::segment_articles(
        &old.iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .len()
        != 1
    {
        return None;
    }
    // Every original word already belonging to one typed table is not a
    // fragmentation repair. Its virtual rows and original rendering prevail.
    if old.len() == 1 {
        return None;
    }
    if material(old.iter().map(|l| l.text.as_str()))
        != material(all.iter().map(|&id| words[id].text.as_str()))
    {
        return None;
    }
    let mut cells = Vec::new();
    for (ci, c) in g.cells.iter().enumerate() {
        let cols = members
            .iter()
            .filter(|(_, i)| **i == ci)
            .map(|(&id, _)| {
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
            .collect();
        let lines = crate::horizontal::row_lines(cols);
        let b = cell(g, ci);
        let same_source = members
            .iter()
            .filter_map(|(&id, &target)| (target == ci).then_some(id))
            .collect::<BTreeSet<_>>();
        let intact = old
            .iter()
            .filter_map(|l| l.table.as_ref())
            .flat_map(|t| &t.cells)
            .find(|c| {
                !c.words.is_empty()
                    && c.words.iter().copied().collect::<BTreeSet<_>>() == same_source
            });
        let (text, word_ids) = if let Some(c) = intact {
            (c.text.clone(), c.words.clone())
        } else {
            (
                lines
                    .iter()
                    .map(|l| l.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n"),
                lines.iter().flat_map(|l| l.words.iter().copied()).collect(),
            )
        };
        cells.push(TableCell {
            row: c.y0,
            col: c.x0,
            rowspan: c.y1 - c.y0,
            colspan: c.x1 - c.x0,
            x0: b[0],
            y0: b[1],
            x1: b[2],
            y1: b[3],
            text,
            words: word_ids,
        });
    }
    cells.sort_by_key(|c| (c.row, c.col));
    let t = Table {
        x0: frame[0],
        y0: frame[1],
        x1: frame[2],
        y1: frame[3],
        rows: g.ys.len() - 1,
        cols: g.xs.len() - 1,
        cells,
    };
    let l = Line {
        text: t.render(),
        words: vec![],
        table: Some(t),
    };
    if ids(&l)? != all
        || material([l.text.as_str()]) != material(old.iter().map(|l| l.text.as_str()))
    {
        return None;
    }
    Some((selected, first, l))
}
pub(crate) fn restore(
    units: &mut [Unit],
    words: &[Word],
    bounds: &[NativeBounds],
    axes: &[NativeAxis],
    rules: &PageRules,
    width: f32,
    height: f32,
) {
    let grids = find_grids(rules, width, height);
    for (i, g) in grids.iter().enumerate() {
        if grids
            .iter()
            .enumerate()
            .any(|(j, o)| i != j && intersects(outer(g), outer(o)))
        {
            continue;
        }
        if let Some((selected, first, line)) = candidate(units, words, bounds, axes, g, rules) {
            replace(units, &selected, first, line);
        }
    }
}

#[cfg(test)]
#[path = "native_grid_fragment_tests.rs"]
mod tests;
