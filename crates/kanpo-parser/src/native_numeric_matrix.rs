//! A complete source frame may certify several amount rails. A local easy
//! fragment never supplies authority for a larger unsupported matrix.
use super::*;
#[derive(Clone)]
struct SourceRow {
    line: usize,
    label: String,
    label_ids: Vec<usize>,
    label_end: f32,
    amounts: Vec<(String, Vec<usize>, [f32; 4])>,
    top: f32,
    bottom: f32,
}
fn box_for(ws: &[&Word]) -> Option<[f32; 4]> {
    let first = ws.first()?;
    Some(
        ws.iter()
            .fold([first.x0, first.y0, first.x1, first.y1], |b, w| {
                [
                    b[0].min(w.x0),
                    b[1].min(w.y0),
                    b[2].max(w.x1),
                    b[3].max(w.y1),
                ]
            }),
    )
}
fn touches(w: &Word, f: Frame) -> bool {
    w.x0 < f.x1 && f.x0 < w.x1 && w.y0 < f.y1 && f.y0 < w.y1
}
fn table_touches(t: &Table, f: Frame) -> bool {
    t.x0 < f.x1 && f.x0 < t.x1 && t.y0 < f.y1 && f.y0 < t.y1
}
// A shorter path is not a parent frame when original collinear strokes cover
// beyond it. Union only overlapping/touching intervals at the identical x;
// actual gaps and distinct source coordinates are never rounded or bridged.
pub(super) fn covered_frames(rules: &PageRules) -> Vec<Frame> {
    let mut coverage: Vec<crate::lines::VRule> = vec![];
    for v in &rules.vertical {
        if ![v.x, v.y0, v.y1].iter().all(|x| x.is_finite()) || v.y0 >= v.y1 {
            continue;
        }
        let mut v = v.clone();
        while let Some(i) = coverage
            .iter()
            .position(|c| c.x == v.x && c.y0 <= v.y1 && v.y0 <= c.y1)
        {
            let c = coverage.remove(i);
            v.y0 = v.y0.min(c.y0);
            v.y1 = v.y1.max(c.y1);
        }
        coverage.push(v);
    }
    let mut complete = rules.clone();
    complete.vertical = coverage;
    frames(&complete)
}
fn row(
    l: &Line,
    index: usize,
    f: Frame,
    ws: &HashMap<usize, &Word>,
    baselines: &[Option<f64>],
    owners: &HashMap<usize, usize>,
) -> Option<SourceRow> {
    if l.table.is_some() || l.words.is_empty() {
        return None;
    }
    let words = l
        .words
        .iter()
        .map(|id| ws.get(id).copied())
        .collect::<Option<Vec<_>>>()?;
    let mut baseline = None;
    for w in &words {
        let y = baselines.get(w.id).copied().flatten()?;
        if !y.is_finite()
            || owners.get(&w.id) != Some(&1)
            || !matches!(w.status, WordStatus::Used)
            || w.x0 < f.x0
            || w.x1 > f.x1
            || y < f.y0 as f64
            || y > f.y1 as f64
            || (!f.open_top && w.y0 < f.y0)
            || (!f.open_bottom && w.y1 > f.y1)
            || y < w.y0 as f64
            || y > w.y1 as f64
            || baseline.is_some_and(|b| b != y)
        {
            return None;
        }
        baseline = Some(y);
    }
    // Original glyph boxes within a cell can overlap. Source origins must still
    // advance strictly; the complete cell envelopes must prove disjoint global
    // column gutters below. No glyph-specific overlap allowance is used.
    if words.windows(2).any(|p| p[0].x0 >= p[1].x0) {
        return None;
    }
    if compact(&words.iter().map(|w| w.text.as_str()).collect::<String>()) != compact(&l.text) {
        return None;
    }
    let fields = l.text.split('\u{3000}').collect::<Vec<_>>();
    let mut label_end = fields.len();
    while label_end > 0 && amount(fields[label_end - 1]) {
        label_end -= 1;
    }
    let label = fields[..label_end].join("\u{3000}");
    if !source_label(&label) {
        return None;
    }
    // A numeric source word in a purported heading is unsupported, rather than
    // evidence that can be ignored while promoting neighboring rows.
    if label_end == fields.len()
        && words
            .iter()
            .any(|w| amount(&w.text) || matches!(w.text.as_str(), "－" | "−" | "-"))
    {
        return None;
    }
    let k = (1..=words.len()).find(|&k| {
        compact(
            &words[..k]
                .iter()
                .map(|w| w.text.as_str())
                .collect::<String>(),
        ) == compact(&label)
    })?;
    let mut cursor = k;
    let mut amounts = vec![];
    for value in &fields[label_end..] {
        let start = cursor;
        let mut text = String::new();
        while cursor < words.len() {
            text.push_str(&words[cursor].text);
            cursor += 1;
            if text == *value {
                break;
            }
            if !value.starts_with(&text) {
                return None;
            }
        }
        if text != *value || start == cursor {
            return None;
        }
        let b = box_for(&words[start..cursor])?;
        amounts.push((
            (*value).into(),
            words[start..cursor].iter().map(|w| w.id).collect(),
            b,
        ));
    }
    if cursor != words.len() {
        return None;
    }
    let b = box_for(&words)?;
    Some(SourceRow {
        line: index,
        label,
        label_ids: words[..k].iter().map(|w| w.id).collect(),
        label_end: box_for(&words[..k])?[2],
        amounts,
        top: b[1],
        bottom: b[3],
    })
}
fn plan(
    units: &[Unit],
    f: Frame,
    ws: &HashMap<usize, &Word>,
    baselines: &[Option<f64>],
    owners: &HashMap<usize, usize>,
) -> Option<Plan> {
    let mut source = vec![];
    for (ui, u) in units.iter().enumerate() {
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
        for (i, l) in ls.iter().enumerate() {
            if l.table.as_ref().is_some_and(|t| table_touches(t, f)) {
                return None;
            }
            if !l
                .words
                .iter()
                .filter_map(|id| ws.get(id))
                .any(|w| touches(w, f))
            {
                continue;
            }
            if !plain {
                return None;
            }
            source.push((ui, row(l, i, f, ws, baselines, owners)?));
        }
    }
    let unit = source.first()?.0;
    if source.iter().any(|(u, _)| *u != unit)
        || source
            .windows(2)
            .any(|p| p[0].1.line + 1 != p[1].1.line || p[0].1.bottom > p[1].1.top)
    {
        return None;
    }
    let rows = source.into_iter().map(|(_, r)| r).collect::<Vec<_>>();
    let all = rows
        .iter()
        .flat_map(|r| {
            r.label_ids
                .iter()
                .copied()
                .chain(r.amounts.iter().flat_map(|(_, ids, _)| ids.iter().copied()))
        })
        .collect::<Vec<_>>();
    let ids = all.iter().copied().collect::<HashSet<_>>();
    if all.len() != ids.len()
        || ws
            .values()
            .any(|w| matches!(w.status, WordStatus::Used) && touches(w, f) && !ids.contains(&w.id))
    {
        return None;
    }
    let mut rails = rows
        .iter()
        .flat_map(|r| r.amounts.iter().map(|(_, _, b)| b[2]))
        .collect::<Vec<_>>();
    rails.sort_by(f32::total_cmp);
    rails.dedup();
    if rails.len() < 2 {
        return None;
    }
    let rail = |x: f32| rails.iter().position(|v| *v == x);
    // Every rail must participate in the same co-occurrence graph. Separate
    // singleton amounts at unrelated right edges do not prove extra columns.
    let mut graph = vec![HashSet::new(); rails.len()];
    let mut starts = vec![f32::INFINITY; rails.len()];
    for r in &rows {
        let cols = r
            .amounts
            .iter()
            .map(|(_, _, b)| rail(b[2]))
            .collect::<Option<Vec<_>>>()?;
        if cols.windows(2).any(|p| p[0] >= p[1]) {
            return None;
        }
        for (&c, (_, _, b)) in cols.iter().zip(&r.amounts) {
            starts[c] = starts[c].min(b[0]);
        }
        for p in cols.windows(2) {
            graph[p[0]].insert(p[1]);
            graph[p[1]].insert(p[0]);
        }
    }
    let mut seen = HashSet::new();
    let mut todo = vec![0];
    while let Some(c) = todo.pop() {
        if seen.insert(c) {
            todo.extend(graph[c].iter().copied());
        }
    }
    if seen.len() != rails.len() {
        return None;
    }
    let label_end = rows
        .iter()
        .map(|r| r.label_end)
        .fold(f32::NEG_INFINITY, f32::max);
    if label_end >= starts[0] || rails.iter().zip(starts.iter().skip(1)).any(|(a, b)| a >= b) {
        return None;
    }
    let mut edges = vec![f.x0, label_end + (starts[0] - label_end) * 0.5];
    edges.extend(
        rails
            .iter()
            .zip(starts.iter().skip(1))
            .map(|(a, b)| a + (b - a) * 0.5),
    );
    edges.push(f.x1);
    if edges.iter().any(|x| !x.is_finite()) || edges.windows(2).any(|p| p[0] >= p[1]) {
        return None;
    }
    let cols = rails.len() + 1;
    let mut cells = vec![];
    for (i, r) in rows.iter().enumerate() {
        let y0 = if i == 0 {
            r.top
        } else {
            rows[i - 1].bottom + (r.top - rows[i - 1].bottom) * 0.5
        };
        let y1 = if i + 1 == rows.len() {
            r.bottom
        } else {
            r.bottom + (rows[i + 1].top - r.bottom) * 0.5
        };
        for c in 0..cols {
            let (text, owned) = if c == 0 {
                (r.label.clone(), r.label_ids.clone())
            } else {
                r.amounts
                    .iter()
                    .find(|(_, _, b)| rail(b[2]) == Some(c - 1))
                    .map(|(t, ids, _)| (t.clone(), ids.clone()))
                    .unwrap_or_default()
            };
            cells.push(TableCell {
                row: i,
                col: c,
                rowspan: 1,
                colspan: 1,
                x0: edges[c],
                y0,
                x1: edges[c + 1],
                y1,
                text,
                words: owned,
            });
        }
    }
    let start = rows.first()?.line;
    let end = rows.last()?.line + 1;
    let table = Table {
        x0: f.x0,
        y0: rows.first()?.top,
        x1: f.x1,
        y1: rows.last()?.bottom,
        rows: rows.len(),
        cols,
        cells,
    };
    let Unit::Plain(ls) = &units[unit] else {
        return None;
    };
    // Cell payloads are exact source substrings. Only explicit blank physical
    // column slots can add separators; no visible word character is changed.
    if compact(&table.render())
        != compact(
            &ls[start..end]
                .iter()
                .map(|l| l.text.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
        )
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
pub(crate) fn restore(
    units: &mut [Unit],
    words: &[Word],
    rules: &PageRules,
    baselines: &[Option<f64>],
) {
    let ws = words.iter().map(|w| (w.id, w)).collect::<HashMap<_, _>>();
    if ws.len() != words.len()
        || words.iter().any(|w| {
            matches!(w.status, WordStatus::Used)
                && (![w.x0, w.x1, w.y0, w.y1].iter().all(|x| x.is_finite())
                    || w.x0 >= w.x1
                    || w.y0 >= w.y1)
        })
    {
        return;
    }
    let mut owners = HashMap::new();
    for u in units.iter() {
        let ls: Vec<&Line> = match u {
            Unit::Plain(ls) => ls.iter().collect(),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
                ..
            } => preamble
                .iter()
                .chain(after.iter().chain(before).chain(outside).flatten())
                .collect(),
        };
        for l in ls {
            for id in l.words.iter().chain(
                l.table
                    .iter()
                    .flat_map(|t| t.cells.iter().flat_map(|c| &c.words)),
            ) {
                *owners.entry(*id).or_insert(0) += 1;
            }
        }
    }
    // A page-local rail set cannot prove columns in an unseen continuation.
    // Only a complete original four-sided frame authorizes this overlay.
    let plans = covered_frames(rules)
        .into_iter()
        .filter(|f| !f.open_top && !f.open_bottom)
        .filter_map(|f| plan(units, f, &ws, baselines, &owners))
        .collect::<Vec<_>>();
    let conflicts = (0..plans.len())
        .filter(|&i| (0..plans.len()).any(|j| i != j && !plans[i].ids.is_disjoint(&plans[j].ids)))
        .collect::<HashSet<_>>();
    for (ui, u) in units.iter_mut().enumerate() {
        let Unit::Plain(ls) = u else {
            continue;
        };
        let selected = plans
            .iter()
            .enumerate()
            .filter_map(|(i, p)| (p.unit == ui && !conflicts.contains(&i)).then_some(p))
            .collect::<Vec<_>>();
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
