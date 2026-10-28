//! Counterpart insertion/deletion rows can refine records, not invent roles.
use crate::{
    grid::{find_grids, Grid},
    lines::PageRules,
    native_word_axes::NativeAxis,
    trace::{NativeBounds, NativeStyles, Table, TableCell, Unit, Word, WordStatus},
};
use std::collections::{BTreeMap, BTreeSet};
type Rect = [f32; 4];
fn valid(b: Rect) -> bool {
    b.iter().all(|x| x.is_finite()) && b[0] < b[2] && b[1] < b[3]
}
fn contains(a: Rect, b: Rect) -> bool {
    a[0] <= b[0] && b[2] <= a[2] && a[1] <= b[1] && b[3] <= a[3]
}
fn overlaps(a: Rect, b: Rect) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}
fn outer(t: &Table) -> Rect {
    [t.x0, t.y0, t.x1, t.y1]
}
fn grid_box(g: &Grid) -> Rect {
    [g.left(), g.top(), g.right(), g.bottom()]
}
fn material(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn rows(t: &Table) -> Option<Vec<Vec<&TableCell>>> {
    if t.cols < 2
        || t.rows == 0
        || !valid(outer(t))
        || t.cells.len() != t.rows.checked_mul(t.cols)?
    {
        return None;
    }
    let mut out = vec![vec![]; t.rows];
    let mut seen = BTreeSet::new();
    for c in &t.cells {
        if c.row >= t.rows
            || c.col >= t.cols
            || c.rowspan != 1
            || c.colspan != 1
            || !seen.insert((c.row, c.col))
        {
            return None;
        }
        out[c.row].push(c);
    }
    for r in &mut out {
        r.sort_by_key(|c| c.col);
        if r.len() != t.cols {
            return None;
        }
    }
    Some(out)
}
struct Source<'a> {
    scene: &'a crate::native_source_scene::Scene,
    words: &'a [Word],
    bounds: &'a [NativeBounds],
    axes: &'a [NativeAxis],
    styles: &'a [Option<NativeStyles>],
    counts: BTreeMap<usize, usize>,
}
impl Source<'_> {
    fn bound(&self, id: usize) -> Option<Rect> {
        let NativeBounds::Glyphs(b) = *self.bounds.get(id)? else {
            return None;
        };
        valid(b).then_some(b)
    }
    fn word(&self, id: usize, g: &Grid, col: usize) -> Option<Rect> {
        let w = self.words.get(id)?;
        let b = self.bound(id)?;
        if w.id != id
            || w.status != WordStatus::Used
            || w.text.trim().is_empty()
            || self.counts.get(&id) != Some(&1)
            || self.axes.get(id) != Some(&NativeAxis::Down)
            || self.styles.get(id)?.as_ref()?.is_empty()
            || crate::normalize::word_text(&w.raw) != w.text
        {
            return None;
        }
        let i = g.cell_at((b[0] + b[2]) / 2., (b[1] + b[3]) / 2.)?;
        let c = g.cells[i];
        (c.y0 == col
            && c.y1 == col + 1
            && contains([g.xs[c.x0], g.ys[c.y0], g.xs[c.x1], g.ys[c.y1]], b))
        .then_some(b)
    }
    fn groups(&self, c: &TableCell, g: &Grid) -> Option<Vec<Vec<usize>>> {
        if c.words.is_empty() {
            return None;
        }
        let mut ws = Vec::new();
        let mut seen = BTreeSet::new();
        for &id in &c.words {
            if !seen.insert(id) {
                return None;
            }
            ws.push((id, self.word(id, g, c.col)?));
        }
        ws.sort_by(|a, b| b.1[0].total_cmp(&a.1[0]).then(a.1[1].total_cmp(&b.1[1])));
        let mut out: Vec<Vec<usize>> = vec![];
        let (mut left, mut right) = (f32::INFINITY, f32::NEG_INFINITY);
        for (id, b) in ws {
            if left.max(b[0]) >= right.min(b[2]) {
                out.push(vec![]);
                left = b[0];
                right = b[2];
            } else {
                left = left.max(b[0]);
                right = right.min(b[2]);
            }
            out.last_mut()?.push(id);
        }
        for group in &mut out {
            group.sort_by(|&a, &b| self.bound(a).unwrap()[1].total_cmp(&self.bound(b).unwrap()[1]));
            for ids in group.windows(2) {
                if self.bound(ids[0])?[3] > self.bound(ids[1])?[1] {
                    return None;
                }
            }
        }
        (out.iter().flatten().copied().collect::<Vec<_>>() == c.words).then_some(out)
    }
    fn axis(&self, ids: impl IntoIterator<Item = usize>) -> Option<(f32, f32)> {
        let (mut lo, mut hi) = (f32::NEG_INFINITY, f32::INFINITY);
        let mut any = false;
        for id in ids {
            let b = self.bound(id)?;
            lo = lo.max(b[0]);
            hi = hi.min(b[2]);
            any = true;
        }
        (any && lo < hi).then_some((lo, hi))
    }
    fn clear(&self, cells: &[&TableCell], groups: &[Vec<Vec<usize>>], g: &Grid) -> bool {
        let ids = cells
            .iter()
            .flat_map(|c| c.words.iter().copied())
            .collect::<BTreeSet<_>>();
        let b = ids.iter().filter_map(|&id| self.bound(id)).fold(
            [f32::INFINITY, g.top(), f32::NEG_INFINITY, g.bottom()],
            |mut a, b| {
                a[0] = a[0].min(b[0]);
                a[2] = a[2].max(b[2]);
                a
            },
        );
        !groups.is_empty()
            && self.scene.clear(b, g)
            && self.words.iter().enumerate().all(|(id, w)| {
                if ids.contains(&id) || !w.raw.is_empty() && w.raw.chars().all(char::is_whitespace)
                {
                    return true;
                }
                let Some(wb) = self.bound(id) else {
                    return false;
                };
                !overlaps(b, wb)
            })
    }
}

// Bind a derived logical table by its complete original occurrences and field
// ownership. Its legacy outer rectangle is not an original source identity.
fn source_grid<'a>(t: &Table, grids: &'a [Grid], s: &Source) -> Option<&'a Grid> {
    let cells = rows(t)?;
    let ids = cells
        .iter()
        .flatten()
        .flat_map(|c| c.words.iter().copied())
        .collect::<BTreeSet<_>>();
    if ids.is_empty() || ids.len() != t.cells.iter().map(|c| c.words.len()).sum::<usize>() {
        return None;
    }
    let candidates = grids
        .iter()
        .filter(|g| {
            if !s.scene.clear(grid_box(g), g) {
                return false;
            }
            if g.ys.len() != t.cols + 1 {
                return false;
            }
            if t.cells.iter().any(|c| {
                c.words.iter().any(|&id| {
                    let Some(w) = s.words.get(id) else {
                        return true;
                    };
                    let Some(b) = s.bound(id) else {
                        return true;
                    };
                    w.id != id
                        || w.status != WordStatus::Used
                        || s.counts.get(&id) != Some(&1)
                        || s.axes.get(id) != Some(&NativeAxis::Down)
                        || s.styles.get(id).and_then(Option::as_ref).is_none()
                        || !contains(outer(t), b)
                        || c.y0 > b[1]
                        || b[3] > c.y1
                        || !contains([g.left(), g.ys[c.col], g.right(), g.ys[c.col + 1]], b)
                })
            }) {
                return false;
            }
            s.words.iter().enumerate().all(|(id, w)| {
                if ids.contains(&id) || !w.raw.is_empty() && w.raw.chars().all(char::is_whitespace)
                {
                    return true;
                }
                let Some(b) = s.bound(id) else {
                    return false;
                };
                !overlaps(grid_box(g), b)
            })
        })
        .collect::<Vec<_>>();
    match candidates.as_slice() {
        [g] => Some(*g),
        _ => None,
    }
}
fn paired_grid<'a>(
    a: &Table,
    b: &Table,
    grids: &'a [Grid],
    s: &Source,
) -> Option<(&'a Grid, &'a Grid)> {
    let ga = source_grid(a, grids, s)?;
    let gb = source_grid(b, grids, s)?;
    if ga.xs != gb.xs || ga.ys.len() != gb.ys.len() || a.cols != b.cols || ga.cells != gb.cells {
        return None;
    }
    let dy = gb.top() - ga.top();
    if dy == 0. || ga.ys.iter().zip(&gb.ys).any(|(&a, &b)| a + dy != b) {
        return None;
    }
    let parents = grids
        .iter()
        .filter(|p| {
            let selected = |g: &Grid| {
                p.cells
                    .iter()
                    .filter(|c| {
                        contains(
                            [p.xs[c.x0], p.ys[c.y0], p.xs[c.x1], p.ys[c.y1]],
                            grid_box(g),
                        )
                    })
                    .collect::<Vec<_>>()
            };
            let a = selected(ga);
            let b = selected(gb);
            matches!((a.as_slice(),b.as_slice()),([a],[b])if a.x0==b.x0&&a.x1==b.x1&&a.y0!=b.y0)
        })
        .count();
    (parents == 1).then_some((ga, gb))
}

fn chunks(c: &TableCell, groups: &[Vec<usize>], words: &[Word]) -> Option<Vec<String>> {
    let expected = groups
        .iter()
        .flat_map(|g| g.iter())
        .map(|&id| material(&words[id].text))
        .collect::<String>();
    if material(&c.text) != expected {
        return None;
    }
    let mut offsets = vec![0usize];
    let mut n = 0;
    for group in groups {
        n += group
            .iter()
            .map(|&id| material(&words[id].text).chars().count())
            .sum::<usize>();
        offsets.push(n);
    }
    let mut positions = Vec::new();
    let mut count = 0;
    for (i, ch) in c.text.char_indices() {
        if !ch.is_whitespace() {
            positions.push(i);
            count += 1;
        }
    }
    if count != n {
        return None;
    }
    let mut out = vec![];
    for i in 0..groups.len() {
        let start = if i == 0 { 0 } else { positions[offsets[i]] };
        let end = if i + 1 == groups.len() {
            c.text.len()
        } else {
            positions[offsets[i + 1]]
        };
        out.push(c.text[start..end].to_owned());
    }
    Some(out)
}
fn repair(t: &Table, peer: &Table, g: &Grid, pg: &Grid, s: &Source) -> Option<Table> {
    let own = rows(t)?;
    let other = rows(peer)?;
    let mut anchors = vec![];
    for cells in other {
        let label = material(&cells[0].text);
        if super::ruled_marker_bands::marker(&label) != Some(true)
            || cells.iter().any(|c| material(&c.text) != label)
        {
            continue;
        }
        let groups = cells
            .iter()
            .map(|c| s.groups(c, pg))
            .collect::<Option<Vec<_>>>()?;
        if groups.iter().any(|g| g.len() != 1) || !s.clear(&cells, &groups, pg) {
            continue;
        }
        anchors.push(s.axis(cells.iter().flat_map(|c| c.words.iter().copied()))?);
    }
    if anchors.is_empty() {
        return None;
    }
    let mut result = t.clone();
    result.cells.clear();
    let mut next = 0usize;
    let mut changed = false;
    for cells in own {
        let original_row = next;
        let maybe = (|| {
            let groups = cells
                .iter()
                .map(|c| s.groups(c, g))
                .collect::<Option<Vec<_>>>()?;
            let n = groups[0].len();
            if n < 2 || groups.iter().any(|v| v.len() != n) || !s.clear(&cells, &groups, g) {
                return None;
            }
            let mut axes = vec![];
            for i in 0..n {
                axes.push(s.axis(groups.iter().flat_map(|v| v[i].iter().copied()))?);
            }
            let mut matches = vec![0usize; n];
            for &(a, b) in &anchors {
                let hits = (0..n)
                    .filter(|&i| a < axes[i].1 && axes[i].0 < b)
                    .collect::<Vec<_>>();
                if hits.len() > 1 {
                    return None;
                }
                if let [i] = hits.as_slice() {
                    matches[*i] += 1;
                }
            }
            let cuts = matches
                .iter()
                .enumerate()
                .filter(|(_, n)| **n == 1)
                .map(|(i, _)| i)
                .collect::<BTreeSet<_>>();
            let first = *cuts.first()?;
            // A marker cannot certify a partial wrapped remainder. Every column from
            // the first transferred head to the old endpoint needs its own counterpart.
            if (first..n).any(|i| !cuts.contains(&i)) {
                return None;
            }
            let mut cuts = cuts;
            cuts.insert(0);
            cuts.insert(n);
            let cuts = cuts.into_iter().collect::<Vec<_>>();
            if cuts.len() < 3 {
                return None;
            }
            let texts = cells
                .iter()
                .zip(&groups)
                .map(|(c, v)| chunks(c, v, s.words))
                .collect::<Option<Vec<_>>>()?;
            let mut out = vec![];
            for pair in cuts.windows(2) {
                for (ci, c) in cells.iter().enumerate() {
                    let mut new = (*c).clone();
                    new.row = next;
                    new.words = groups[ci][pair[0]..pair[1]]
                        .iter()
                        .flatten()
                        .copied()
                        .collect();
                    new.text = texts[ci][pair[0]..pair[1]].concat();
                    // Boundary whitespace becomes the new table-row boundary; inner padding
                    // within a retained record remains exact.
                    if pair[0] > 0 {
                        new.text = new.text.trim_start().to_owned();
                    }
                    if pair[1] < n {
                        new.text = new.text.trim_end().to_owned();
                    }
                    new.x0 = new
                        .words
                        .iter()
                        .map(|&id| s.words[id].x0)
                        .fold(f32::INFINITY, f32::min);
                    new.x1 = new
                        .words
                        .iter()
                        .map(|&id| s.words[id].x1)
                        .fold(f32::NEG_INFINITY, f32::max);
                    out.push(new);
                }
                next += 1;
            }
            Some(out)
        })();
        if let Some(new) = maybe {
            changed = true;
            result.cells.extend(new);
        } else {
            next = original_row;
            for c in cells {
                let mut c = c.clone();
                c.row = next;
                result.cells.push(c);
            }
            next += 1;
        }
    }
    result.rows = next;
    changed.then_some(result)
}
#[derive(Clone, Copy)]
pub(super) struct Inputs<'a> {
    pub words: &'a [Word],
    pub bounds: &'a [NativeBounds],
    pub axes: &'a [NativeAxis],
    pub styles: &'a [Option<NativeStyles>],
    pub rules: &'a PageRules,
    pub size: [f32; 2],
    pub scene: &'a crate::native_source_scene::Scene,
}
fn changed_marker(t: &Table) -> bool {
    t.cells
        .iter()
        .any(|c| super::ruled_marker_bands::marker(&material(&c.text)) == Some(true))
}
pub(super) fn has_changed_marker(units: &[Unit]) -> bool {
    units.iter().any(|u| match u {
        Unit::Shinkyu { after, before, .. } => after
            .iter()
            .flatten()
            .chain(before.iter().flatten())
            .any(|l| l.table.as_ref().is_some_and(changed_marker)),
        _ => false,
    })
}
pub(super) fn restore(units: &mut [Unit], input: Inputs<'_>) {
    let Inputs {
        words,
        bounds,
        axes,
        styles,
        rules,
        size: [width, height],
        scene,
    } = input;
    if !has_changed_marker(units) || !scene.matches(words) {
        return;
    }
    if [bounds.len(), axes.len(), styles.len()]
        .iter()
        .any(|&n| n != words.len())
    {
        return;
    }
    let grids = find_grids(rules, width, height);
    let mut counts = BTreeMap::new();
    let mut count = |l: &crate::trace::Line| {
        if let Some(t) = &l.table {
            for id in t.cells.iter().flat_map(|c| &c.words) {
                *counts.entry(*id).or_default() += 1;
            }
        } else {
            for &id in &l.words {
                *counts.entry(id).or_default() += 1;
            }
        }
    };
    for u in units.iter() {
        match u {
            Unit::Plain(ls) => {
                for l in ls {
                    count(l)
                }
            }
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
                ..
            } => {
                for l in preamble
                    .iter()
                    .chain(after.iter().flatten())
                    .chain(before.iter().flatten())
                    .chain(outside.iter().flatten())
                {
                    count(l)
                }
            }
        }
    }
    let source = Source {
        scene,
        words,
        bounds,
        axes,
        styles,
        counts,
    };
    for u in units {
        let Unit::Shinkyu { after, before, .. } = u else {
            continue;
        };
        if after.len() != before.len() {
            continue;
        }
        for (a, b) in after.iter_mut().zip(before) {
            let old_a = a.clone();
            let old_b = b.clone();
            let mut plans = vec![];
            for (ai, al) in old_a.iter().enumerate() {
                let Some(at) = al
                    .table
                    .as_ref()
                    .filter(|t| al.words.is_empty() && al.text == t.render())
                else {
                    continue;
                };
                for (bi, bl) in old_b.iter().enumerate() {
                    let Some(bt) = bl
                        .table
                        .as_ref()
                        .filter(|t| bl.words.is_empty() && bl.text == t.render())
                    else {
                        continue;
                    };
                    if !changed_marker(at) && !changed_marker(bt) {
                        continue;
                    }
                    let Some((ag, bg)) = paired_grid(at, bt, &grids, &source) else {
                        continue;
                    };
                    plans.push((
                        ai,
                        bi,
                        repair(at, bt, &ag, &bg, &source),
                        repair(bt, at, &bg, &ag, &source),
                    ));
                }
            }
            let unique = |ai, bi| {
                plans
                    .iter()
                    .filter(|(x, y, _, _)| *x == ai || *y == bi)
                    .count()
                    == 1
            };
            for (ai, bi, at, bt) in &plans {
                if !unique(*ai, *bi) {
                    continue;
                }
                if let Some(t) = at {
                    a[*ai].text = t.render();
                    a[*ai].table = Some(t.clone());
                }
                if let Some(t) = bt {
                    b[*bi].text = t.render();
                    b[*bi].table = Some(t.clone());
                }
            }
        }
    }
}
#[cfg(test)]
#[path = "native_marker_record_tests.rs"]
mod tests;
