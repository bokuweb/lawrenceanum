//! Preserve a source-certified terminal numeric column in an open ruled frame.
//! Existing row text and word order are authoritative. No characters are added.
use crate::{
    lines::PageRules,
    trace::{Line, Table, TableCell, Unit, Word, WordStatus},
};
use std::collections::{HashMap, HashSet};
#[derive(Clone, Copy)]
struct Frame {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    open_top: bool,
    open_bottom: bool,
}
#[derive(Clone)]
struct Row {
    line: usize,
    label: String,
    amount: String,
    left: Vec<usize>,
    right: Vec<usize>,
    left_end: f32,
    right_start: f32,
    right_end: f32,
    top: f32,
    bottom: f32,
}
struct Plan {
    unit: usize,
    start: usize,
    end: usize,
    table: Table,
    ids: HashSet<usize>,
}
fn unsigned(s: &str) -> &str {
    s.strip_prefix(['△', '▲', '−', '－', '+', '-']).unwrap_or(s)
}
fn amount(s: &str) -> bool {
    let s = unsigned(s);
    let mut parts = s.split('.');
    let whole = parts.next().unwrap_or("");
    let fraction = parts.next();
    if parts.next().is_some()
        || fraction.is_some_and(|f| f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit()))
    {
        return false;
    }
    let groups: Vec<_> = whole.split(',').collect();
    if groups
        .iter()
        .any(|g| g.is_empty() || !g.bytes().all(|b| b.is_ascii_digit()))
    {
        return false;
    }
    groups.len() == 1 || (groups[0].len() <= 3 && groups[1..].iter().all(|g| g.len() == 3))
}
fn frames(r: &PageRules) -> Vec<Frame> {
    let mut vs: Vec<(f32, f32, f32)> = vec![];
    for v in &r.vertical {
        if ![v.x, v.y0, v.y1].iter().all(|x| x.is_finite()) || v.y0 >= v.y1 {
            continue;
        }
        let mut a = (v.x, v.y0, v.y1);
        while let Some(i) = vs
            .iter()
            .position(|b| a.0 == b.0 && (a.1 == b.2 || a.2 == b.1))
        {
            let b = vs.remove(i);
            a.1 = a.1.min(b.1);
            a.2 = a.2.max(b.2);
        }
        vs.push(a);
    }
    let mut out = vec![];
    for a in &vs {
        for b in &vs {
            if a.0 >= b.0 || a.1 != b.1 || a.2 != b.2 {
                continue;
            }
            let wall = |y| {
                r.horizontal
                    .iter()
                    .any(|h| h.y == y && h.x0 == a.0 && h.x1 == b.0)
            };
            if (!wall(a.1) && !wall(a.2))
                || vs
                    .iter()
                    .any(|c| a.0 < c.0 && c.0 < b.0 && c.1 < a.2 && a.1 < c.2)
            {
                continue;
            }
            out.push(Frame {
                x0: a.0,
                y0: a.1,
                x1: b.0,
                y1: a.2,
                open_top: !wall(a.1),
                open_bottom: !wall(a.2),
            });
        }
    }
    out
}
// Whitespace is ignored only when binding the complete original label words.
// The cell retains the exact existing line substring, including every gap.
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn source_label(s: &str) -> bool {
    let mut fields = s.split_whitespace();
    let Some(first) = fields.next() else {
        return false;
    };
    // An amount is a whole numeric field. Digits embedded in a lexical label
    // (a year, identifier or footnote) do not create another amount rail.
    // A separate bare number remains ambiguous and cannot be hidden in a label.
    let literal = |field: &str| {
        let cs = field.chars().collect::<Vec<_>>();
        let alpha = |c: char| c.is_alphabetic() && !c.is_numeric();
        let mut at = 0;
        while at < cs.len() {
            if !cs[at].is_numeric() {
                at += 1;
                continue;
            }
            let start = at;
            while at < cs.len() && cs[at].is_numeric() {
                at += 1;
            }
            let embedded = start > 0 && at < cs.len() && alpha(cs[start - 1]) && alpha(cs[at]);
            // Dedicated reference/dagger marks introduce annotation numbers.
            // A bare terminal number can instead be an accidentally joined
            // amount; neither lexical text nor a neighboring row proves it.
            let note = at == cs.len()
                && start > 0
                && matches!(cs[start - 1], '※' | '†' | '‡')
                && cs[..start - 1].iter().copied().any(alpha);
            if !embedded && !note {
                return false;
            }
        }
        true
    };
    let roman = !first.is_empty()
        && first
            .chars()
            .all(|c| ('Ⅰ'..='Ⅿ').contains(&c) || ('ⅰ'..='ⅿ').contains(&c));
    let enclosed = first
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .or_else(|| first.strip_prefix('（').and_then(|s| s.strip_suffix('）')))
        .is_some_and(|n| {
            !n.is_empty()
                && n.chars()
                    .all(|c| c.is_ascii_digit() || ('０'..='９').contains(&c))
        });
    if roman || enclosed {
        let Some(body) = fields.next() else {
            return false;
        };
        literal(body) && fields.all(literal)
    } else {
        literal(first) && fields.all(literal)
    }
}

fn row(
    l: &Line,
    index: usize,
    f: Frame,
    ws: &HashMap<usize, &Word>,
    baselines: &[Option<f64>],
    owners: &HashMap<usize, usize>,
) -> Option<Row> {
    if l.table.is_some() || l.words.is_empty() {
        return None;
    }
    let (label, number) = l.text.rsplit_once('\u{3000}')?;
    if !source_label(label) || !amount(number) {
        return None;
    }
    let words = l
        .words
        .iter()
        .map(|id| ws.get(id).copied())
        .collect::<Option<Vec<_>>>()?;
    let mut baseline = None;
    for w in &words {
        let y = *baselines.get(w.id)?.as_ref()? as f32;
        if ![w.x0, w.x1, w.y0, w.y1, y].iter().all(|v| v.is_finite())
            || w.x0 >= w.x1
            || w.y0 >= w.y1
            || owners.get(&w.id) != Some(&1)
            || !matches!(w.status, WordStatus::Used)
            || w.x0 < f.x0
            || w.x1 > f.x1
            || (!f.open_top && w.y0 < f.y0)
            || y < f.y0
            || y > f.y1
            || (!f.open_bottom && w.y1 > f.y1)
            || y < w.y0
            || y > w.y1
        {
            return None;
        }
        if baseline.is_some_and(|b| b != y) {
            return None;
        }
        baseline = Some(y);
    }
    let top = words.iter().map(|w| w.y0).fold(f32::INFINITY, f32::min);
    let bottom = words.iter().map(|w| w.y1).fold(f32::NEG_INFINITY, f32::max);
    for k in 1..words.len() {
        let left = &words[..k];
        let right = &words[k..];
        if compact(&left.iter().map(|w| w.text.as_str()).collect::<String>()) != compact(label)
            || right.iter().map(|w| w.text.as_str()).collect::<String>() != unsigned(number)
        {
            continue;
        }
        let left_end = left.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max);
        let right_start = right.iter().map(|w| w.x0).fold(f32::INFINITY, f32::min);
        let right_end = right.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max);
        if left_end >= right_start {
            continue;
        }
        let mut right_ids = right.iter().map(|w| w.id).collect::<Vec<_>>();
        let prefix = &number[..number.len() - unsigned(number).len()];
        let mut source_right_start = right_start;
        let (mut source_top, mut source_bottom) = (top, bottom);
        if !prefix.is_empty() {
            // A detached printed sign must have its own unique original word and
            // horizontal baseline. Bind that source occurrence instead of leaving
            // its existing literal text without a source-cell owner.
            let candidates = ws
                .values()
                .copied()
                .filter(|w| {
                    w.text == prefix
                        && !owners.contains_key(&w.id)
                        && matches!(w.status, WordStatus::Used)
                        && [w.x0, w.x1, w.y0, w.y1].iter().all(|v| v.is_finite())
                        && w.x0 < w.x1
                        && w.y0 < w.y1
                        && w.x0 >= left_end
                        && w.x0 < right_start
                        && w.x1 <= right[0].x1
                        && (f.open_top || w.y0 >= f.y0)
                        && (f.open_bottom || w.y1 <= f.y1)
                        && baselines.get(w.id).and_then(|b| *b).is_some_and(|y| {
                            y as f32 == baseline.unwrap() && w.y0 <= y as f32 && y as f32 <= w.y1
                        })
                })
                .collect::<Vec<_>>();
            let [sign] = candidates.as_slice() else {
                return None;
            };
            right_ids.insert(0, sign.id);
            source_right_start = sign.x0;
            source_top = source_top.min(sign.y0);
            source_bottom = source_bottom.max(sign.y1);
        }
        return Some(Row {
            line: index,
            label: label.into(),
            amount: number.into(),
            left: left.iter().map(|w| w.id).collect(),
            right: right_ids,
            left_end,
            right_start: source_right_start,
            right_end,
            top: source_top,
            bottom: source_bottom,
        });
    }
    None
}
fn plan(unit: usize, ls: &[Line], rows: &[Row], f: Frame, words: &[Word]) -> Option<Plan> {
    if rows.len() < 2 {
        return None;
    }
    let left_end = rows
        .iter()
        .map(|r| r.left_end)
        .fold(f32::NEG_INFINITY, f32::max);
    let right_start = rows
        .iter()
        .map(|r| r.right_start)
        .fold(f32::INFINITY, f32::min);
    if left_end >= right_start
        || rows.iter().any(|r| r.right_end != rows[0].right_end)
        || rows
            .windows(2)
            .any(|r| r[0].bottom > r[1].top || r[0].line + 1 != r[1].line)
    {
        return None;
    }
    let all = rows
        .iter()
        .flat_map(|r| r.left.iter().chain(&r.right))
        .copied()
        .collect::<Vec<_>>();
    let ids = all.iter().copied().collect::<HashSet<_>>();
    if all.len() != ids.len() {
        return None;
    }
    let top = rows[0].top;
    let bottom = rows.last()?.bottom;
    // Complete used-word coverage of the physical source band. A crossing or
    // intervening word cannot be ignored to manufacture a numeric table.
    if words
        .iter()
        .filter(|w| {
            matches!(w.status, WordStatus::Used)
                && w.x0 < f.x1
                && f.x0 < w.x1
                && w.y0 < bottom
                && top < w.y1
        })
        .any(|w| !ids.contains(&w.id))
    {
        return None;
    }
    let cut = (left_end + right_start) * 0.5;
    let mut cells = vec![];
    for (i, r) in rows.iter().enumerate() {
        let y0 = if i == 0 {
            top
        } else {
            (rows[i - 1].bottom + r.top) * 0.5
        };
        let y1 = if i + 1 == rows.len() {
            bottom
        } else {
            (r.bottom + rows[i + 1].top) * 0.5
        };
        for (col, x0, x1, text, owned) in [
            (0, f.x0, cut, &r.label, &r.left),
            (1, cut, f.x1, &r.amount, &r.right),
        ] {
            cells.push(TableCell {
                row: i,
                col,
                rowspan: 1,
                colspan: 1,
                x0,
                y0,
                x1,
                y1,
                text: text.clone(),
                words: owned.clone(),
            });
        }
    }
    let table = Table {
        x0: f.x0,
        y0: top,
        x1: f.x1,
        y1: bottom,
        rows: rows.len(),
        cols: 2,
        cells,
    };
    let start = rows[0].line;
    let end = rows.last()?.line + 1;
    if table.render()
        != ls[start..end]
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    {
        return None;
    }
    Some(Plan {
        unit,
        start,
        end,
        table,
        ids,
    })
}
// A local fragment cannot certify the column structure of its parent frame.
// Every numeric source line in the frame must prove the same sole terminal
// amount rail and a common empty column gap. Existing grids/roles retain their
// authority; unsupported numeric rows decline the entire frame.
fn certifies_frame(
    units: &[Unit],
    f: Frame,
    ws: &HashMap<usize, &Word>,
    baselines: &[Option<f64>],
    owners: &HashMap<usize, usize>,
) -> bool {
    let mut numeric = vec![];
    let mut claimed = HashSet::new();
    for u in units {
        let (plain, ls): (bool, Vec<&Line>) = match u {
            Unit::Plain(ls) => (true, ls.iter().collect()),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
                ..
            } => (
                false,
                preamble
                    .iter()
                    .chain(after.iter().chain(before).chain(outside).flatten())
                    .collect(),
            ),
        };
        for (i, l) in ls.into_iter().enumerate() {
            if l.table
                .as_ref()
                .is_some_and(|t| t.x0 < f.x1 && f.x0 < t.x1 && t.y0 < f.y1 && f.y0 < t.y1)
            {
                return false;
            }
            let ids = l
                .words
                .iter()
                .chain(
                    l.table
                        .iter()
                        .flat_map(|t| t.cells.iter().flat_map(|c| &c.words)),
                )
                .copied()
                .collect::<Vec<_>>();
            if !ids
                .iter()
                .filter_map(|i| ws.get(i))
                .any(|w| w.x0 < f.x1 && f.x0 < w.x1 && w.y0 < f.y1 && f.y0 < w.y1)
            {
                continue;
            }
            if !plain || l.table.is_some() {
                return false;
            }
            claimed.extend(ids.iter().copied());
            let value = |s: &str| amount(s) || matches!(s, "－" | "−" | "-");
            if !l.text.split('\u{3000}').any(value)
                && !ids.iter().filter_map(|i| ws.get(i)).any(|w| value(&w.text))
            {
                continue;
            }
            let Some(r) = row(l, i, f, ws, baselines, owners) else {
                return false;
            };
            claimed.extend(r.left.iter().chain(&r.right).copied());
            numeric.push(r);
        }
    }
    // Every used native source word in the entire frame already has a line
    // owner or the unique printed-sign certificate above. An orphan outside
    // a local numeric fragment cannot silently disappear from frame authority.
    if ws.values().any(|w| {
        matches!(w.status, WordStatus::Used)
            && w.x0 < f.x1
            && f.x0 < w.x1
            && w.y0 < f.y1
            && f.y0 < w.y1
            && !claimed.contains(&w.id)
    }) {
        return false;
    }
    numeric.len() >= 2
        && numeric.iter().all(|r| r.right_end == numeric[0].right_end)
        && numeric
            .iter()
            .map(|r| r.left_end)
            .fold(f32::NEG_INFINITY, f32::max)
            < numeric
                .iter()
                .map(|r| r.right_start)
                .fold(f32::INFINITY, f32::min)
}
pub(crate) fn restore(
    units: &mut [Unit],
    words: &[Word],
    rules: &PageRules,
    baselines: &[Option<f64>],
) {
    let ws: HashMap<_, _> = words.iter().map(|w| (w.id, w)).collect();
    if ws.len() != words.len() {
        return;
    }
    let mut owners = HashMap::new();
    let mut count = |l: &Line| {
        for id in l.words.iter().chain(
            l.table
                .iter()
                .flat_map(|t| t.cells.iter().flat_map(|c| &c.words)),
        ) {
            *owners.entry(*id).or_insert(0) += 1;
        }
    };
    for u in units.iter() {
        match u {
            Unit::Plain(ls) => {
                for l in ls {
                    count(l);
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
                    .chain(after.iter().chain(before).chain(outside).flatten())
                {
                    count(l);
                }
            }
        }
    }
    let mut plans = vec![];
    for f in frames(rules) {
        if !certifies_frame(units, f, &ws, baselines, &owners) {
            continue;
        }
        for (ui, u) in units.iter().enumerate() {
            let Unit::Plain(ls) = u else { continue };
            let mut group = vec![];
            for i in 0..=ls.len() {
                let next = ls
                    .get(i)
                    .and_then(|l| row(l, i, f, &ws, baselines, &owners));
                if next
                    .as_ref()
                    .is_none_or(|r| group.last().is_some_and(|last: &Row| last.bottom > r.top))
                {
                    if let Some(p) = plan(ui, ls, &group, f, words) {
                        plans.push(p);
                    }
                    group.clear();
                }
                if let Some(r) = next {
                    group.push(r);
                }
            }
        }
    }
    let conflicts = (0..plans.len())
        .filter(|&i| (0..plans.len()).any(|j| i != j && !plans[i].ids.is_disjoint(&plans[j].ids)))
        .collect::<HashSet<_>>();
    for (ui, u) in units.iter_mut().enumerate() {
        let Unit::Plain(ls) = u else { continue };
        let selected: Vec<_> = plans
            .iter()
            .enumerate()
            .filter_map(|(i, p)| (p.unit == ui && !conflicts.contains(&i)).then_some(p))
            .collect();
        if selected.is_empty() {
            continue;
        }
        let old = std::mem::take(ls);
        for (i, l) in old.into_iter().enumerate() {
            if let Some(p) = selected.iter().find(|p| p.start == i) {
                ls.push(Line {
                    text: p.table.render(),
                    words: vec![],
                    table: Some(p.table.clone()),
                });
            }
            if !selected.iter().any(|p| p.start <= i && i < p.end) {
                ls.push(l);
            }
        }
    }
}
#[cfg(test)]
#[path = "native_numeric_frames_tests148.rs"]
mod tests;

#[cfg(test)]
#[path = "native_numeric_matrix_tests152.rs"]
mod matrix152_tests;

#[path = "native_numeric_matrix.rs"]
mod matrix;
pub(crate) use matrix::restore as restore_matrix;

pub(crate) use matrix::restore_with_terminal as restore_matrix_with_terminal;
