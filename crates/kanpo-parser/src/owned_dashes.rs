//! 確定した行の科目と数値の右端が一致するときだけ、既知の原字の横棒を戻す。
use crate::{
    grid::Grid,
    trace::{Line, Table, TableCell, Unit, Word, WordStatus},
};
fn cy(w: &Word) -> f32 {
    (w.y0 + w.y1) / 2.
}
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn label(s: &str) -> bool {
    let s = compact(s);
    (2..=40).contains(&s.chars().count())
        && s.chars().all(|c| {
            ('\u{3040}'..='\u{30ff}').contains(&c)
                || ('\u{3400}'..='\u{9fff}').contains(&c)
                || "（）()・".contains(c)
        })
}
fn amount(s: &str) -> Option<&str> {
    let unit = if s.ends_with("百万円") {
        "百万円"
    } else if s.ends_with("千円") {
        "千円"
    } else if s.ends_with('円') {
        "円"
    } else {
        ""
    };
    let n = s.strip_suffix(unit)?;
    (n.chars().filter(|c| c.is_ascii_digit()).count() >= 2
        && n.chars()
            .all(|c| c.is_ascii_digit() || ",.△▲−－-()（）".contains(c)))
    .then_some(unit)
}
#[derive(Clone)]
struct Run {
    edge: f32,
    y: f32,
    unit: String,
}
fn runs(ids: &[usize], ws: &[Word], em: f32) -> Vec<Run> {
    let mut ids = ids.to_vec();
    ids.sort_by(|a, b| ws[*a].x0.total_cmp(&ws[*b].x0));
    let mut out = vec![];
    let mut text = String::new();
    let mut edge = 0.;
    let mut y = 0.;
    let finish = |text: &mut String, edge: f32, y: f32, out: &mut Vec<Run>| {
        if let Some(unit) = amount(text) {
            out.push(Run {
                edge,
                y,
                unit: unit.into(),
            });
        }
        text.clear();
    };
    for id in ids {
        let w = &ws[id];
        if w.text.is_empty() {
            continue;
        }
        if !text.is_empty() && ((cy(w) - y).abs() > em * 0.35 || w.x0 > edge + em * 0.75) {
            finish(&mut text, edge, y, &mut out);
        }
        if w.text
            .chars()
            .all(|c| c.is_ascii_digit() || ",.△▲−－-()（）千百万円".contains(c))
        {
            edge = if text.is_empty() { w.x1 } else { edge.max(w.x1) };
            text.push_str(&w.text);
            y = cy(w);
        } else {
            finish(&mut text, edge, y, &mut out);
        }
    }
    finish(&mut text, edge, y, &mut out);
    out
}
fn candidate(w: &Word) -> bool {
    let em = w.y1 - w.y0;
    w.status == WordStatus::Empty
        && w.raw == "\u{E61C}"
        && (3.0..=20.0).contains(&em)
        && (em * 0.7..=em * 1.3).contains(&(w.x1 - w.x0))
}
fn row(ids: &[usize], ws: &[Word], d: &Word) -> bool {
    let em = d.y1 - d.y0;
    !ids.is_empty()
        && ids.iter().all(|&id| {
            ws.get(id).is_some_and(|w| {
                w.status == WordStatus::Used
                    && !w.text.is_empty()
                    && w.x1 < d.x0
                    && (cy(w) - cy(d)).abs() <= em * 0.25
                    && (w.y1 - w.y0 - em).abs() <= 1.
            })
        })
        && label(
            &ids.iter()
                .map(|id| ws[*id].text.as_str())
                .collect::<String>(),
        )
}
fn plain_with_context(
    lines: &mut [Line],
    ws: &mut [Word],
    vrules: &[(f32, f32, f32)],
    all_lines: &[Line],
) {
    let mut ps = vec![];
    for d in ws.iter().filter(|w| candidate(w)) {
        let em = d.y1 - d.y0;
        let x = (d.x0 + d.x1) / 2.;
        let rails: Vec<_> = vrules
            .iter()
            .filter(|r| r.1 <= d.y0 + 1.25 && r.2 + 1.25 >= d.y1)
            .collect();
        let Some(l) = rails
            .iter()
            .filter(|r| r.0 < x)
            .max_by(|a, b| a.0.total_cmp(&b.0))
        else {
            continue;
        };
        let Some(r) = rails
            .iter()
            .filter(|r| r.0 > x)
            .min_by(|a, b| a.0.total_cmp(&b.0))
        else {
            continue;
        };
        if !(150.0..=450.0).contains(&(r.0 - l.0)) {
            continue;
        }
        let (top, bottom) = (l.1.max(r.1), l.2.min(r.2));
        let inside = |ids: &[usize]| {
            ids.iter().all(|&id| {
                ws[id].x0 >= l.0 - 1.
                    && ws[id].x1 <= r.0 + 1.
                    && ws[id].y0 >= top - 1.25
                    && ws[id].y1 <= bottom + 1.25
            })
        };
        if all_lines
            .iter()
            .filter(|a| a.table.is_none() && inside(&a.words) && row(&a.words, ws, d))
            .count()
            != 1
        {
            continue;
        }
        let targets: Vec<_> = lines
            .iter()
            .enumerate()
            .filter(|(_, a)| a.table.is_none() && inside(&a.words) && row(&a.words, ws, d))
            .map(|(i, _)| i)
            .collect();
        if targets.len() != 1 {
            continue;
        }
        let mut anchors = vec![];
        let mut multi = false;
        for a in lines.iter().filter(|a| {
            a.table.is_none() && inside(&a.words) && !a.text.contains(['。', '、', ':', '：'])
        }) {
            let ns = runs(&a.words, ws, em);
            let matched: Vec<_> = ns
                .iter()
                .filter(|n| (n.edge - d.x1).abs() <= 1. && (n.y - cy(d)).abs() <= em * 10.)
                .collect();
            if matched.len() != 1 {
                continue;
            }
            let n = matched[0];
            let labels: Vec<_> = a
                .words
                .iter()
                .copied()
                .filter(|id| {
                    ws[*id].x1 < d.x0 - em * 2.
                        && label(&ws[*id].text)
                        && (cy(&ws[*id]) - n.y).abs() <= em * 0.25
                })
                .collect();
            if labels.is_empty()
                || !label(
                    &labels
                        .iter()
                        .map(|id| ws[*id].text.as_str())
                        .collect::<String>(),
                )
            {
                continue;
            }
            multi |= ns.len() >= 2;
            anchors.push(n.clone());
        }
        anchors.sort_by(|a, b| a.y.total_cmp(&b.y));
        anchors.dedup_by(|a, b| (a.y - b.y).abs() < em);
        // 一つの金額欄なら単位付き2行、複数欄なら同じ端に2行（うち1行は複数値）。
        if anchors.len() < 2
            || (!multi && anchors.iter().filter(|n| !n.unit.is_empty()).count() < 2)
        {
            continue;
        }
        if ws
            .iter()
            .filter(|w| {
                w.raw == "\u{E61C}"
                    && (cy(w) - cy(d)).abs() <= em * 0.4
                    && (w.x1 - d.x1).abs() <= 1.
            })
            .count()
            != 1
        {
            continue;
        }
        ps.push((d.id, targets[0]));
    }
    for (id, i) in ps {
        if lines.iter().any(|l| {
            l.words.contains(&id)
                || l.table
                    .as_ref()
                    .is_some_and(|t| t.cells.iter().any(|c| c.words.contains(&id)))
        }) {
            continue;
        }
        lines[i].text.push_str("　－");
        lines[i].words.push(id);
        ws[id].text = "－".into();
        ws[id].status = WordStatus::Used;
    }
}
fn table(t: &mut Table, ws: &mut [Word], grids: &[Grid]) {
    if t.cols < 2 || t.rows < 3 {
        return;
    }
    let near = |a: f32, b: f32| (a - b).abs() <= 1.;
    let gs: Vec<_> = grids
        .iter()
        .filter(|g| {
            g.xs.len() == t.cols + 1
                && near(g.left(), t.x0)
                && near(g.right(), t.x1)
                && near(g.top(), t.y0)
                && near(g.bottom(), t.y1)
        })
        .collect();
    if gs.len() != 1 {
        return;
    }
    let g = gs[0];
    let mut ps = vec![];
    for d in ws.iter().filter(|w| candidate(w)) {
        let em = d.y1 - d.y0;
        let x = (d.x0 + d.x1) / 2.;
        let y = cy(d);
        let pc: Vec<_> = g
            .cells
            .iter()
            .filter(|p| g.xs[p.x0] < x && x < g.xs[p.x1] && g.ys[p.y0] < y && y < g.ys[p.y1])
            .collect();
        if pc.len() != 1 {
            continue;
        }
        let p = pc[0];
        if p.x1 - p.x0 != 1
            || p.y1 - p.y0 != 1
            || p.x0 == 0
            || d.x0 < g.xs[p.x0]
            || d.x1 > g.xs[p.x1]
            || d.y0 < g.ys[p.y0] - 1.
            || d.y1 > g.ys[p.y1] + 1.
        {
            continue;
        }
        let labels: Vec<_> = t
            .cells
            .iter()
            .filter(|c| {
                c.col + c.colspan == p.x0
                    && c.rowspan == 1
                    && near(c.x1, g.xs[p.x0])
                    && near(c.y0, g.ys[p.y0])
                    && near(c.y1, g.ys[p.y1])
            })
            .collect();
        // 同じ物理科目升目の2行を論理分割した場合だけ。多段文章や全横棒列は推定しない。
        if labels.len() != 2 || labels.iter().any(|c| !label(&c.text)) {
            continue;
        }
        let owners: Vec<_> = labels.iter().filter(|c| row(&c.words, ws, d)).collect();
        if owners.len() != 1 {
            continue;
        }
        let rr = owners[0].row;
        if t.cells.iter().any(|c| {
            c.row <= rr
                && rr < c.row + c.rowspan
                && c.col <= p.x0
                && p.x0 < c.col + c.colspan
                && (!c.text.trim().is_empty() || !c.words.is_empty())
        }) {
            continue;
        }
        let mut anchors = vec![];
        for c in t
            .cells
            .iter()
            .filter(|c| c.col == p.x0 && c.colspan == 1 && c.rowspan == 1)
        {
            let ns = runs(&c.words, ws, em);
            if ns.len() == 1 && (ns[0].edge - d.x1).abs() <= 1. && (ns[0].y - y).abs() <= em * 4. {
                anchors.push(ns[0].y);
            }
        }
        anchors.sort_by(f32::total_cmp);
        anchors.dedup_by(|a, b| (*a - *b).abs() < em * 0.75);
        if anchors.len() < 2 {
            continue;
        }
        let other = labels.iter().find(|c| c.row != rr).unwrap();
        if !t
            .cells
            .iter()
            .any(|c| c.row == other.row && c.col == p.x0 && runs(&c.words, ws, em).len() == 1)
        {
            continue;
        }
        if ws
            .iter()
            .filter(|w| {
                w.raw == "\u{E61C}"
                    && w.x0 >= g.xs[p.x0]
                    && w.x1 <= g.xs[p.x1]
                    && (cy(w) - y).abs() < em * 0.5
            })
            .count()
            != 1
        {
            continue;
        }
        ps.push((
            d.id, rr, p.x0, g.xs[p.x0], g.xs[p.x1], g.ys[p.y0], g.ys[p.y1],
        ));
    }
    for (id, row, col, x0, x1, y0, y1) in ps {
        t.cells.push(TableCell {
            row,
            col,
            rowspan: 1,
            colspan: 1,
            x0,
            x1,
            y0,
            y1,
            text: "－".into(),
            words: vec![id],
        });
        ws[id].text = "－".into();
        ws[id].status = WordStatus::Used;
    }
}
pub(crate) fn restore_owned_dashes(
    units: &mut [Unit],
    ws: &mut [Word],
    grids: &[Grid],
    vrules: &[(f32, f32, f32)],
) {
    if !ws.iter().any(candidate) {
        return;
    }
    let all_lines: Vec<_> = units
        .iter()
        .filter_map(|u| {
            if let Unit::Plain(ls) = u {
                Some(ls)
            } else {
                None
            }
        })
        .flatten()
        .cloned()
        .collect();
    for u in units {
        let Unit::Plain(ls) = u else { continue };
        plain_with_context(ls, ws, vrules, &all_lines);
        for l in ls {
            if let Some(t) = &mut l.table {
                table(t, ws, grids);
                l.text = t.render();
            }
        }
    }
}
#[cfg(test)]
fn plain(lines: &mut [Line], ws: &mut [Word], vrules: &[(f32, f32, f32)]) {
    let context = lines.to_vec();
    plain_with_context(lines, ws, vrules, &context);
}

#[cfg(test)]
#[path = "owned_dash_tests.rs"]
mod tests;
