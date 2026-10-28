//! Original rectangle amounts inside the unique native extent of a table label.
use crate::{
    grid::Grid,
    lines::PageRules,
    native_terminal_glyph::{self, Terminals},
    native_word_axes::NativeAxis,
    trace::{ContentStyles, NativeBounds, NativeStyles, Table, TableCell, Unit, Word, WordStatus},
};
use std::collections::BTreeSet;
type Rect = [f32; 4];
fn valid(b: Rect) -> bool {
    b.iter().all(|v| v.is_finite()) && b[0] < b[2] && b[1] < b[3]
}
fn contains(a: Rect, b: Rect) -> bool {
    valid(a) && valid(b) && a[0] <= b[0] && a[1] <= b[1] && b[2] <= a[2] && b[3] <= a[3]
}
fn overlap(a: Rect, b: Rect) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}
fn union(a: Rect, b: Rect) -> Rect {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}
fn numeric(t: &str) -> bool {
    t.chars().any(|c| c.is_ascii_digit())
        && t.chars()
            .all(|c| c.is_ascii_digit() || ",.△▲−－+-".contains(c))
}
// A sweep over half-open source slots also rejects duplicate empty cells.
// No row/column-count budget or expansion of large merged spans is needed.
fn disjoint(mut slots: Vec<(usize, usize, usize, usize)>) -> bool {
    slots.sort_unstable();
    let mut active: Vec<(usize, usize, usize)> = vec![];
    for (a, z, x, y) in slots {
        if a >= z || x >= y {
            return false;
        }
        active.retain(|&(end, _, _)| end > a);
        if active.iter().any(|&(_, left, right)| x < right && left < y) {
            return false;
        }
        active.push((z, x, y));
    }
    true
}
fn partition(t: &Table, g: &Grid) -> bool {
    if t.rows == 0
        || t.cols == 0
        || g.xs.len() < 2
        || g.ys.len() < 2
        || g.xs.iter().chain(&g.ys).any(|v| !v.is_finite())
        || g.xs.windows(2).any(|w| w[0] >= w[1])
        || g.ys.windows(2).any(|w| w[0] >= w[1])
    {
        return false;
    }
    let Some(logical) = t
        .cells
        .iter()
        .map(|c| {
            let row = c.row.checked_add(c.rowspan)?;
            let col = c.col.checked_add(c.colspan)?;
            (row <= t.rows && col <= t.cols).then_some((c.row, row, c.col, col))
        })
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    if !disjoint(logical) {
        return false;
    }
    let mut area = 0usize;
    let mut physical = vec![];
    for p in &g.cells {
        if p.x0 >= p.x1 || p.x1 >= g.xs.len() || p.y0 >= p.y1 || p.y1 >= g.ys.len() {
            return false;
        }
        let Some(a) = (p.x1 - p.x0)
            .checked_mul(p.y1 - p.y0)
            .and_then(|a| area.checked_add(a))
        else {
            return false;
        };
        area = a;
        physical.push((p.y0, p.y1, p.x0, p.x1));
    }
    disjoint(physical) && (g.xs.len() - 1).checked_mul(g.ys.len() - 1) == Some(area)
}
struct Source<'a> {
    words: &'a [Word],
    bounds: &'a [NativeBounds],
    axes: &'a [NativeAxis],
    styles: &'a [Option<NativeStyles>],
    content: &'a [ContentStyles],
    owners: Vec<usize>,
    terminals: Terminals,
}
impl Source<'_> {
    fn bounds(&self, id: usize, used: bool) -> Option<Rect> {
        let w = self.words.get(id)?;
        let NativeBounds::Glyphs(b) = *self.bounds.get(id)? else {
            return None;
        };
        (w.id == id
            && !w.raw.is_empty()
            && valid(b)
            && b == w.source_box
            && b == [w.x0, w.y0, w.x1, w.y1]
            && self.axes.get(id) == Some(&NativeAxis::Right)
            && (!used || (w.status == WordStatus::Used && self.owners[id] == 1))
            && self.terminals.get(w)?.origins_for(w).is_some())
        .then_some(b)
    }
    fn cell(&self, c: &TableCell) -> Option<Rect> {
        if c.rowspan != 1 || c.colspan != 1 || c.words.is_empty() {
            return None;
        }
        let mut b = None;
        let mut text = String::new();
        for &id in &c.words {
            let bb = self.bounds(id, true)?;
            if !matches!(self.content.get(id),Some(ContentStyles::Glyphs(s))if !s.is_empty()) {
                return None;
            }
            b = Some(b.map_or(bb, |a| union(a, bb)));
            text.push_str(&self.words[id].text);
        }
        let compact = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
        (compact(&text) == compact(&c.text)).then_some(b?)
    }
    fn clear_words(&self, b: Rect, ids: &BTreeSet<usize>) -> bool {
        self.words
            .iter()
            .all(|w| ids.contains(&w.id) || !overlap(b, w.source_box))
    }
}
struct Plan {
    unit: usize,
    line: usize,
    id: usize,
    row: usize,
    col: usize,
    physical: Rect,
    grid: usize,
    bounds: [Rect; 2],
    witnesses: Vec<[Rect; 3]>,
}
fn proposals(
    t: &Table,
    g: &Grid,
    s: &Source<'_>,
    id: usize,
    ink: [f64; 4],
) -> Option<(usize, usize, Rect, [Rect; 2], Vec<[Rect; 3]>)> {
    if t.cols.checked_add(1) != Some(g.xs.len())
        || g.ys.len() < 2
        || [t.x0, t.y0, t.x1, t.y1] != [g.left(), g.top(), g.right(), g.bottom()]
    {
        return None;
    }
    if !partition(t, g) {
        return None;
    }
    let mark = s.bounds(id, false)?;
    let physical = g
        .cells
        .iter()
        .filter_map(|p| {
            let b = [
                *g.xs.get(p.x0)?,
                *g.ys.get(p.y0)?,
                *g.xs.get(p.x1)?,
                *g.ys.get(p.y1)?,
            ];
            (p.x1 == p.x0 + 1 && contains(b, mark)).then_some((p, b))
        })
        .collect::<Vec<_>>();
    let [(p, area)] = physical.as_slice() else {
        return None;
    };
    let col = p.x0;
    // Original physical columns are authoritative after their contents have
    // been partitioned into logical rows. No inferred row-height tolerance.
    let column = t
        .cells
        .iter()
        .filter(|c| c.col == col && c.y0 == area[1] && c.y1 == area[3])
        .collect::<Vec<_>>();
    if column.is_empty()
        || column.iter().any(|c| {
            c.rowspan != 1
                || c.colspan != 1
                || c.x0 != area[0]
                || c.x1 != area[2]
                || (!c.text.trim().is_empty() && c.text != "－" && !numeric(&c.text))
        })
    {
        return None;
    }
    let labels = t
        .cells
        .iter()
        .filter_map(|c| {
            if c.col == col || c.row >= t.rows || c.text.trim().is_empty() {
                return None;
            }
            let b = s.cell(c)?;
            if b[1] > mark[1] || mark[3] > b[3] {
                return None;
            }
            let parent = g
                .cells
                .iter()
                .filter(|l| l.x0 == c.col && l.x1 == c.col + 1 && l.y0 == p.y0 && l.y1 == p.y1)
                .collect::<Vec<_>>();
            let [l] = parent.as_slice() else { return None };
            let lb = [g.xs[l.x0], g.ys[l.y0], g.xs[l.x1], g.ys[l.y1]];
            (contains(lb, b) && [c.x0, c.y0, c.x1, c.y1] == lb).then_some((c, b))
        })
        .collect::<Vec<_>>();
    let [(label, lb)] = labels.as_slice() else {
        return None;
    };
    let row = label.row;
    if t.cells.iter().any(|c| {
        c.row <= row
            && row < c.row + c.rowspan
            && c.col <= col
            && col < c.col + c.colspan
            && (!c.text.is_empty()
                || !c.words.is_empty()
                || c.row != row
                || c.col != col
                || c.rowspan != 1
                || c.colspan != 1
                || !contains(*area, [c.x0, c.y0, c.x1, c.y1])
                || !contains([c.x0, c.y0, c.x1, c.y1], mark))
    }) {
        return None;
    }
    let style = s.styles.get(id)?.as_ref()?;
    if style.is_empty()
        || style
            .iter()
            .any(|p| !f64::from_bits(p.1).is_finite() || f64::from_bits(p.1) <= 0.)
    {
        return None;
    }
    let edge = s.terminals.get(&s.words[id])?.origins_for(&s.words[id])?[1];
    let bounds = [*lb, [area[0], mark[1], area[2], mark[3]]];
    let ink_strip = [area[0], ink[1] as f32, area[2], ink[3] as f32];
    if !s.clear_words(ink_strip, &BTreeSet::from([id])) {
        return None;
    }
    let mut witnesses = vec![];
    for value in column {
        if !numeric(&value.text) {
            continue;
        }
        let Some(vb) = s.cell(value) else { continue };
        if !contains(*area, vb)
            || !value
                .words
                .iter()
                .all(|&i| matches!(s.content.get(i),Some(ContentStyles::Glyphs(x))if !x.is_empty()&&x.iter().map(|p|p.1).collect::<BTreeSet<_>>()==style.iter().map(|p|p.1).collect::<BTreeSet<_>>()))
        {
            continue;
        }
        let Some(last) = value
            .words
            .last()
            .and_then(|&i| s.terminals.get(&s.words[i])?.origins_for(&s.words[i]))
        else {
            continue;
        };
        if last[1] != edge {
            continue;
        }
        let cells = t
            .cells
            .iter()
            .filter(|c| c.row == value.row && c.col == label.col)
            .collect::<Vec<_>>();
        let [other] = cells.as_slice() else { continue };
        let Some(ob) = s.cell(other) else { continue };
        if ob[0] >= lb[2] || lb[0] >= ob[2] || ob[1] < area[1] || ob[3] > area[3] {
            continue;
        }
        let baseline = last[2];
        if !value.words.iter().all(|&i| {
            s.terminals
                .get(&s.words[i])
                .and_then(|t| t.origins_for(&s.words[i]))
                .is_some_and(|p| p[2] == baseline)
        }) {
            continue;
        }
        let ids = value.words.iter().copied().collect::<BTreeSet<_>>();
        let clear = s.words.iter().all(|w| {
            ids.contains(&w.id)
                || !(w.source_box[0] < area[2]
                    && area[0] < w.source_box[2]
                    && w.source_box[1] as f64 <= baseline
                    && baseline <= w.source_box[3] as f64)
        });
        if clear {
            witnesses.push([ob, [area[0], vb[1], area[2], vb[3]], vb])
        }
    }
    (!witnesses.is_empty()).then_some((row, col, *area, bounds, witnesses))
}
pub(crate) fn restore(
    units: &mut [Unit],
    words: &mut [Word],
    bounds: &[NativeBounds],
    axes: &[NativeAxis],
    styles: &[Option<NativeStyles>],
    content: &[ContentStyles],
    grids: &[Grid],
    page: scraper::ElementRef<'_>,
    rules: &PageRules,
) {
    if bounds.len() != words.len()
        || axes.len() != words.len()
        || styles.len() != words.len()
        || content.len() != words.len()
        || !words
            .iter()
            .any(|w| w.status == WordStatus::Empty && w.text.is_empty())
        || !units
            .iter()
            .any(|u| matches!(u,Unit::Plain(ls)if ls.iter().any(|l|l.table.is_some())))
    {
        return;
    }
    // Every successful plan requires this own-word certificate. Its absence
    // cannot become evidence through a neighboring value or legacy raw code.
    if !page
        .select(&scraper::Selector::parse("word[data-native-outline-rectangle]").unwrap())
        .any(|_| true)
    {
        return;
    }
    let terminals = native_terminal_glyph::for_words(page, words);
    let outlines = crate::native_word_outlines::for_words(page, words, &terminals);
    let mut owners = vec![0; words.len()];
    for u in units.iter() {
        let ls = match u {
            Unit::Plain(ls) => ls.iter().collect::<Vec<_>>(),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
                ..
            } => preamble
                .iter()
                .chain(after.iter().flatten())
                .chain(before.iter().flatten())
                .chain(outside.iter().flatten())
                .collect(),
        };
        for l in ls {
            for &id in l.words.iter().chain(
                l.table
                    .iter()
                    .flat_map(|t| t.cells.iter().flat_map(|c| &c.words)),
            ) {
                let Some(n) = owners.get_mut(id) else { return };
                *n += 1;
            }
        }
    }
    let s = Source {
        words,
        bounds,
        axes,
        styles,
        content,
        owners,
        terminals,
    };
    let mut plans = vec![];
    for (ui, u) in units.iter().enumerate() {
        let Unit::Plain(ls) = u else { continue };
        for (li, l) in ls.iter().enumerate() {
            let Some(t) = &l.table else { continue };
            for (id, w) in s.words.iter().enumerate() {
                if w.id != id
                    || w.status != WordStatus::Empty
                    || !w.text.is_empty()
                    || s.owners[id] != 0
                    || !outlines[id].is_some_and(|b| b[2] - b[0] > b[3] - b[1])
                {
                    continue;
                }
                for (gi, g) in grids.iter().enumerate() {
                    if let Some((row, col, physical, bounds, witnesses)) =
                        proposals(t, g, &s, id, outlines[id].unwrap())
                    {
                        plans.push(Plan {
                            unit: ui,
                            line: li,
                            id,
                            row,
                            col,
                            physical,
                            grid: gi,
                            bounds,
                            witnesses,
                        });
                    }
                }
            }
        }
    }
    if plans.is_empty() {
        return;
    }
    let Some(scene) =
        crate::native_source_scene::for_page(page, rules).filter(|scene| scene.matches(words))
    else {
        return;
    };
    let keep = plans
        .iter()
        .map(|p| {
            let check = |areas: [Rect; 3]| {
                scene
                    .interior_strip(areas[1], &grids[p.grid])
                    .is_some_and(|b| {
                        contains(b, areas[2])
                            && scene.clear(areas[0], &grids[p.grid])
                            && scene.clear(b, &grids[p.grid])
                    })
            };
            plans
                .iter()
                .filter(|q| {
                    p.id == q.id || (p.unit, p.line, p.row, p.col) == (q.unit, q.line, q.row, q.col)
                })
                .count()
                == 1
                && check([p.bounds[0], p.bounds[1], words[p.id].source_box])
                && p.witnesses.iter().copied().any(check)
        })
        .collect::<Vec<_>>();
    for (p, keep) in plans.into_iter().zip(keep) {
        if !keep {
            continue;
        }
        let Unit::Plain(ls) = &mut units[p.unit] else {
            continue;
        };
        let l = &mut ls[p.line];
        let t = l.table.as_mut().unwrap();
        if let Some(c) = t
            .cells
            .iter_mut()
            .find(|c| c.row == p.row && c.col == p.col)
        {
            c.text = "－".into();
            c.words.push(p.id)
        } else {
            t.cells.push(TableCell {
                row: p.row,
                col: p.col,
                rowspan: 1,
                colspan: 1,
                x0: p.physical[0],
                y0: p.physical[1],
                x1: p.physical[2],
                y1: p.physical[3],
                text: "－".into(),
                words: vec![p.id],
            });
        }
        words[p.id].text = "－".into();
        words[p.id].status = WordStatus::Used;
        l.text = t.render();
    }
}

#[cfg(test)]
#[path = "native_wrapped_amount_tests.rs"]
mod tests;
