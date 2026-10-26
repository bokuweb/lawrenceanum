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
            ..} => {
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
            ..} => {
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

/// Recover a complete vertical ordinary table from its painted source grid.
/// This is a local replacement after comparison tier planning: an independent
/// grid cannot change unrelated role divisions elsewhere on the page.
pub(crate) fn restore_independent_vertical(
    units: &mut [Unit],
    plans: &[(Grid, Vec<Col>)],
    words: &[Word],
    bounds: &[NativeBounds],
    axes: &[NativeAxis],
    styles: &[Option<crate::trace::NativeStyles>],
    flows: &[Option<crate::native_column_flow::AxisFlow>],
) {
    for (g, cols) in plans {
        if !crate::ruled_cell_flow::partition(g) {
            continue;
        }
        let all: BTreeSet<_> = cols.iter().map(|c| c.id).collect();
        if all.is_empty() || all.len() != cols.len() {
            continue;
        }
        // Plans must name the complete native inventory inside the frame, not
        // merely a convenient subset whose own characters happen to match.
        let mut native_members = BTreeSet::new();
        let mut source_known = true;
        for (id, w) in words
            .iter()
            .enumerate()
            .filter(|(_, w)| w.status == WordStatus::Used)
        {
            let Some(NativeBounds::Glyphs(b)) = bounds.get(id) else {
                source_known = false;
                break;
            };
            if w.id != id || !valid(*b) {
                source_known = false;
                break;
            }
            if g.contains((b[0] + b[2]) * 0.5, (b[1] + b[3]) * 0.5, 0.5) {
                native_members.insert(id);
            }
        }
        if !source_known || native_members != all {
            continue;
        }
        let certified = cols.iter().all(|c| {
            let Some(w) = words.get(c.id) else {
                return false;
            };
            let Some(NativeBounds::Glyphs(b)) = bounds.get(c.id) else {
                return false;
            };
            let Some(style) = styles.get(c.id).and_then(Option::as_ref) else {
                return false;
            };
            let Some(flow) = flows.get(c.id).copied().flatten() else {
                return false;
            };
            w.id == c.id
                && w.status == WordStatus::Used
                && w.text == c.text
                && valid(*b)
                && axes.get(c.id) == Some(&NativeAxis::Down)
                && flow
                    .vertical_center_envelope(*b, style)
                    .is_some_and(|centers| vertical_center_cell(g, centers).is_some())
                && !crate::segment::is_heading_mark(&c.text)
        });
        if !certified {
            continue;
        }
        let mut flat = vec![];
        visit(units, |allowed, l| flat.push((allowed, l.clone())));
        let mut counts = HashMap::new();
        let mut selected = HashSet::new();
        let mut ok = true;
        for (i, (allowed, line)) in flat.iter().enumerate() {
            let Some(own) = ids(line) else {
                ok = false;
                break;
            };
            for id in &own {
                *counts.entry(*id).or_insert(0usize) += 1;
            }
            if own.is_disjoint(&all) {
                continue;
            }
            // Preserve every pre-existing typed table, logical comparison role,
            // and any line containing a source word outside this physical grid.
            if !allowed || line.table.is_some() || !own.is_subset(&all) {
                ok = false;
                break;
            }
            selected.insert(i);
        }
        if !ok || all.iter().any(|id| counts.get(id) != Some(&1)) {
            continue;
        }
        let Some(first) = selected.iter().min().copied() else {
            continue;
        };
        let table = crate::shinkyu::grid_table(g, cols.clone());
        let line = Line {
            text: table.render(),
            words: vec![],
            table: Some(table),
        };
        if ids(&line) != Some(all)
            || material([line.text.as_str()])
                != material(selected.iter().map(|&i| flat[i].1.text.as_str()))
        {
            continue;
        }
        replace(units, &selected, first, line);
    }
}

#[cfg(test)]
#[path = "independent_vertical_grid_tests147.rs"]
mod independent_vertical_tests147;

/// All certified glyph centers of the original column must belong to one
/// native cell. A segment crossing a painted boundary remains ambiguous.
fn vertical_center_cell(g: &Grid, [x, first, last]: [f64; 3]) -> Option<usize> {
    if ![x, first, last].iter().all(|v| v.is_finite()) || first > last {
        return None;
    }
    let cells: Vec<_> = (0..g.cells.len())
        .filter(|&i| {
            let r = cell(g, i);
            r[0] as f64 <= x && x <= r[2] as f64 && r[1] as f64 <= first && last <= r[3] as f64
        })
        .collect();
    (cells.len() == 1).then(|| cells[0])
}
