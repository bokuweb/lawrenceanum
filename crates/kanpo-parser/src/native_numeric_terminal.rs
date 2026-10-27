//! A complete closed source parent may prove one terminal glyph band.
//! Full amount envelopes and coordinate proximity are not that evidence.
use super::*;
use crate::native_terminal_glyph::Terminals;
fn internal_crosscut(rules: &PageRules, f: Frame) -> bool {
    let mut ys = rules
        .horizontal
        .iter()
        .filter(|h| h.y.is_finite() && f.y0 < h.y && h.y < f.y1 && h.x0 <= f.x0 && f.x0 < h.x1)
        .map(|h| h.y)
        .collect::<Vec<_>>();
    ys.sort_by(f32::total_cmp);
    ys.dedup();
    for y in ys {
        let mut xs = rules
            .horizontal
            .iter()
            .filter(|h| h.y == y && h.x0.is_finite() && h.x1.is_finite() && h.x0 < h.x1)
            .map(|h| (h.x0, h.x1))
            .collect::<Vec<_>>();
        xs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        let mut end = f.x0;
        for (a, b) in xs {
            if a > end {
                break;
            }
            if end < b {
                end = b;
            }
            if f.x1 <= end {
                return true;
            }
        }
    }
    false
}
fn band_columns(
    rows: &[SourceRow],
    f: Frame,
    ws: &HashMap<usize, &Word>,
    baselines: &[Option<f64>],
    tails: &Terminals,
) -> Option<Vec<f32>> {
    let mut left = f64::NEG_INFINITY;
    let mut right = f64::INFINITY;
    let mut label = f32::NEG_INFINITY;
    let mut start = f32::INFINITY;
    let mut witnessed = 0;
    for row in rows {
        label = label.max(row.label_end);
        if row.amounts.is_empty() {
            continue;
        }
        let [(_, ids, b)] = row.amounts.as_slice() else {
            return None;
        };
        let word = ws.get(ids.last()?)?;
        let tail = tails.get(word)?;
        let span = tail.advance_for(word)?;
        if Some(tail.origins_for(word)?[2]) != baselines.get(word.id).copied().flatten() {
            return None;
        }
        left = left.max(span[0]);
        right = right.min(span[1]);
        start = start.min(b[0]);
        witnessed += 1;
    }
    if witnessed < 2 || left >= right || label >= start {
        return None;
    }
    let cut = label + (start - label) * 0.5;
    let cuts = vec![f.x0, cut, f.x1];
    (cuts.iter().all(|x| x.is_finite()) && cuts.windows(2).all(|p| p[0] < p[1])).then_some(cuts)
}
pub(super) fn candidate_frames(units: &[Unit], rules: &PageRules) -> Vec<Frame> {
    covered_frames(rules)
        .into_iter()
        .filter(|f| !f.open_top && !f.open_bottom && !internal_crosscut(rules, *f))
        .filter(|f| {
            !units.iter().any(|u| {
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
                ls.iter()
                    .any(|l| l.table.as_ref().is_some_and(|t| table_touches(t, *f)))
            })
        })
        .collect()
}
pub(super) fn plans(
    units: &[Unit],
    frames: &[Frame],
    ws: &HashMap<usize, &Word>,
    baselines: &[Option<f64>],
    owners: &HashMap<usize, usize>,
    tails: &Terminals,
) -> Vec<Plan> {
    frames
        .iter()
        .copied()
        .filter_map(|f| {
            let p = plan_with_rows(
                units,
                f,
                ws,
                |l, i| row_with_terminal_labels(l, i, f, ws, baselines, owners, Some(tails)),
                |rows| band_columns(rows, f, ws, baselines, tails),
            )?;
            let Unit::Plain(ls) = &units[p.unit] else {
                return None;
            };
            // A sole amount column adds no leading/interior blank slots. Every original
            // line byte, including ASCII/fullwidth spaces, must therefore render exactly.
            (p.table.render()
                == ls[p.start..p.end]
                    .iter()
                    .map(|l| l.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n"))
            .then_some(p)
        })
        .collect()
}
