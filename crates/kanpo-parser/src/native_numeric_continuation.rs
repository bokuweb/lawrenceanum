//! Complete a source frame only across one proved printed-column transition.
//! The complete parent rail set is collected before either part gets cells.
use super::*;
fn inventory(
    units: &[Unit],
    f: Frame,
    ws: &HashMap<usize, &Word>,
    baselines: &[Option<f64>],
    owners: &HashMap<usize, usize>,
) -> Option<(usize, Vec<SourceRow>, HashSet<usize>)> {
    let mut unit = None;
    let mut rows = vec![];
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
        for (li, l) in ls.into_iter().enumerate() {
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
            if !plain || unit.is_some_and(|old| old != ui) {
                return None;
            }
            unit = Some(ui);
            rows.push(row(l, li, f, ws, baselines, owners)?);
        }
    }
    if rows.is_empty()
        || rows
            .windows(2)
            .any(|p| p[0].line + 1 != p[1].line || p[0].bottom > p[1].top)
    {
        return None;
    }
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
    if ids.len() != all.len()
        || ws
            .values()
            .any(|w| w.status == WordStatus::Used && touches(w, f) && !ids.contains(&w.id))
    {
        return None;
    }
    Some((unit?, rows, ids))
}
fn baseline_extreme(ids: &HashSet<usize>, baselines: &[Option<f64>], last: bool) -> Option<f64> {
    let ys = ids
        .iter()
        .map(|&id| baselines.get(id).copied().flatten())
        .collect::<Option<Vec<_>>>()?;
    ys.into_iter()
        .reduce(if last { f64::max } else { f64::min })
}
fn combine(
    units: &[Unit],
    rules: &PageRules,
    a: Frame,
    b: Frame,
    ws: &HashMap<usize, &Word>,
    baselines: &[Option<f64>],
    owners: &HashMap<usize, usize>,
) -> Option<Vec<Plan>> {
    if a.open_top
        || !a.open_bottom
        || !b.open_top
        || b.open_bottom
        || a.x1 >= b.x0
        || a.y0 < b.y0
        || b.y1 > a.y1
    {
        return None;
    }
    // Original equal frame translation supplies the common coordinate basis.
    // Never infer it from the desired numeric values or fitted amount rails.
    let dx = b.x0 - a.x0;
    if !dx.is_finite() || dx <= 0. || b.x1 - a.x1 != dx {
        return None;
    }
    let vertical = covered_vertical(rules);
    let dividers = vertical
        .iter()
        .filter(|v| a.x1 < v.x && v.x < b.x0 && v.y0 == b.y0 && v.y1 == a.y1)
        .collect::<Vec<_>>();
    let [divider] = dividers.as_slice() else {
        return None;
    };
    let (ua, ra, ia) = inventory(units, a, ws, baselines, owners)?;
    let (ub, rb, ib) = inventory(units, b, ws, baselines, owners)?;
    if ua != ub || !ia.is_disjoint(&ib) || ra.last()?.line + 1 != rb.first()?.line {
        return None;
    }
    let last = baseline_extreme(&ia, baselines, true)?;
    let first = baseline_extreme(&ib, baselines, false)?;
    // All original used glyphs independently fence the transition. An unseen
    // intervening caption/body, another owner or unknown direction blocks it.
    for w in ws.values().filter(|w| w.status == WordStatus::Used) {
        let y = baselines.get(w.id).copied().flatten()?;
        if !y.is_finite()
            || y < divider.y0 as f64
            || y > divider.y1 as f64
            || w.x0 < divider.x && divider.x < w.x1
        {
            return None;
        }
        if w.x1 <= divider.x && (y > last || y == last && !ia.contains(&w.id)) {
            return None;
        }
        if divider.x <= w.x0 && (y < first || y == first && !ib.contains(&w.id)) {
            return None;
        }
    }
    let mut common = ra.clone();
    for r in &rb {
        let mut r = r.clone();
        let shift = |x: f32| {
            let v = x - dx;
            (v.is_finite() && v + dx == x).then_some(v)
        };
        r.label_end = shift(r.label_end)?;
        for (_, _, v) in &mut r.amounts {
            v[0] = shift(v[0])?;
            v[2] = shift(v[2])?;
        }
        common.push(r);
    }
    let edges = columns(&common, a)?;
    let translated = edges.iter().map(|x| x + dx).collect::<Vec<_>>();
    if translated.iter().any(|x| !x.is_finite())
        || translated.windows(2).any(|p| p[0] >= p[1])
        || translated.first() != Some(&b.x0)
        || translated.last() != Some(&b.x1)
    {
        return None;
    }
    let mut plans = vec![];
    for (f, rows, ids, cuts) in [(a, ra, ia, edges), (b, rb, ib, translated)] {
        let table = source_table(&rows, f, &cuts)?;
        // Verify every translated cut against the original physical glyphs.
        let mut consumed = vec![];
        for c in &table.cells {
            for id in &c.words {
                let w = ws.get(id)?;
                if w.x0 < c.x0 || c.x1 < w.x1 || w.y0 < c.y0 || c.y1 < w.y1 {
                    return None;
                }
                consumed.push(*id);
            }
        }
        let ordered = rows
            .iter()
            .flat_map(|r| {
                r.label_ids
                    .iter()
                    .copied()
                    .chain(r.amounts.iter().flat_map(|(_, ids, _)| ids.iter().copied()))
            })
            .collect::<Vec<_>>();
        if consumed != ordered {
            return None;
        }
        let start = rows.first()?.line;
        let end = rows.last()?.line + 1;
        let Unit::Plain(ls) = &units[ua] else {
            return None;
        };
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
        plans.push(Plan {
            unit: ua,
            start,
            end,
            table,
            ids,
        });
    }
    Some(plans)
}
pub(super) fn plans(
    units: &[Unit],
    rules: &PageRules,
    ws: &HashMap<usize, &Word>,
    baselines: &[Option<f64>],
    owners: &HashMap<usize, usize>,
) -> Vec<Plan> {
    let fs = covered_frames(rules);
    let mut groups = vec![];
    for &a in &fs {
        for &b in &fs {
            if let Some(ps) = combine(units, rules, a, b, ws, baselines, owners) {
                groups.push(ps);
            }
        }
    }
    // A conflict in one part rejects the entire parent, never just its mate.
    let ids = groups
        .iter()
        .map(|g| {
            g.iter()
                .flat_map(|p| p.ids.iter().copied())
                .collect::<HashSet<_>>()
        })
        .collect::<Vec<_>>();
    groups
        .into_iter()
        .enumerate()
        .filter(|(i, _)| {
            !ids.iter()
                .enumerate()
                .any(|(j, other)| *i != j && !ids[*i].is_disjoint(other))
        })
        .flat_map(|(_, g)| g)
        .collect()
}
