//! 確定した物理セル内の、数値を横に並べた記録へ原字の横棒を戻す。
//! 二つの記録が一つの論理セルへ結合された場合だけ、既存の改行と語の高さを照合して分離する。
use crate::{
    grid::Grid,
    trace::{Line, Table, TableCell, Unit, Word, WordStatus},
};
#[derive(Clone)]
struct Row {
    cy: f32,
    ids: Vec<usize>,
}
#[derive(Clone)]
struct Number {
    x0: f32,
    x1: f32,
}
struct Proposal {
    index: usize,
    texts: Vec<String>,
    ids: Vec<Vec<usize>>,
    dashes: Vec<usize>,
}
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn japanese(c: char) -> bool {
    ('\u{3040}'..='\u{30ff}').contains(&c) || ('\u{3400}'..='\u{9fff}').contains(&c)
}
fn center(w: &Word) -> f32 {
    (w.y0 + w.y1) / 2.
}
fn groups(ids: &[usize], words: &[Word], em: f32) -> Option<Vec<Row>> {
    if ids.iter().any(|&id| words.get(id).is_none()) {
        return None;
    }
    let mut sorted = ids.to_vec();
    sorted.sort_by(|&a, &b| {
        center(&words[a])
            .total_cmp(&center(&words[b]))
            .then(words[a].x0.total_cmp(&words[b].x0))
    });
    let mut rows: Vec<Row> = vec![];
    for id in sorted {
        let w = words.get(id)?;
        if w.status != WordStatus::Used
            || w.text.trim().is_empty()
            || !(em * 0.75..=em * 1.25).contains(&(w.y1 - w.y0))
        {
            return None;
        }
        if let Some(r) = rows
            .last_mut()
            .filter(|r| (r.cy - center(w)).abs() <= em * 0.35)
        {
            r.ids.push(id);
        } else {
            rows.push(Row {
                cy: center(w),
                ids: vec![id],
            });
        }
    }
    for r in &mut rows {
        r.ids.sort_by(|&a, &b| words[a].x0.total_cmp(&words[b].x0));
    }
    Some(rows)
}
fn numbers(r: &Row, ws: &[Word], em: f32) -> Vec<Number> {
    let numeric = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_digit() || ",.△▲－−-()（）".contains(c))
    };
    let mut out = vec![];
    let mut run: Vec<&Word> = vec![];
    let finish = |run: &mut Vec<&Word>, out: &mut Vec<Number>| {
        let s: String = run.iter().map(|w| w.text.as_str()).collect();
        if s.chars().filter(|c| c.is_ascii_digit()).count() >= 2 {
            out.push(Number {
                x0: run[0].x0,
                x1: run.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max),
            });
        }
        run.clear();
    };
    for &id in &r.ids {
        let w = &ws[id];
        if !numeric(&w.text) {
            finish(&mut run, &mut out);
            continue;
        }
        if run.last().is_some_and(|p| w.x0 > p.x1 + em * 0.75) {
            finish(&mut run, &mut out);
        }
        run.push(w);
    }
    finish(&mut run, &mut out);
    out
}
fn named(r: &Row, ns: &[Number], ws: &[Word]) -> bool {
    ns.first().is_some_and(|n| {
        let label: String = r
            .ids
            .iter()
            .filter(|&&id| ws[id].x1 < n.x0)
            .map(|&id| ws[id].text.as_str())
            .collect();
        label.chars().filter(|&c| japanese(c)).count() >= 2
            && compact(&label).chars().count() <= 40
            && !label.contains(['。', '、', ':', '：'])
    })
}

fn same_frame(c: &TableCell, g: &Grid, p: &crate::grid::GridCell) -> bool {
    let near = |a: f32, b: f32| (a - b).abs() <= 1.;
    near(c.x0, g.xs[p.x0])
        && near(c.x1, g.xs[p.x1])
        && near(c.y0, g.ys[p.y0])
        && near(c.y1, g.ys[p.y1])
}
fn restore_table(t: &mut Table, words: &mut [Word], grids: &[Grid]) {
    if t.cells.iter().any(|c| c.rowspan != 1) {
        return;
    }
    let near = |a: f32, b: f32| (a - b).abs() <= 1.;
    let gs: Vec<_> = grids
        .iter()
        .filter(|g| {
            near(t.x0, g.left())
                && near(t.x1, g.right())
                && near(t.y0, g.top())
                && near(t.y1, g.bottom())
                && g.xs.len() == t.cols + 1
        })
        .collect();
    if gs.len() != 1 {
        return;
    }
    let g = gs[0];
    let mut proposals = vec![];
    for p in &g.cells {
        if p.x1 - p.x0 != 1 || p.y1 - p.y0 != 1 || p.y0 == 0 || p.y1 + 1 >= g.ys.len() {
            continue;
        }
        let inside = |w: &Word| {
            w.x0 >= g.xs[p.x0] && w.x1 <= g.xs[p.x1] && w.y0 >= g.ys[p.y0] && w.y1 <= g.ys[p.y1]
        };
        let ds: Vec<_> = words
            .iter()
            .filter(|w| w.status == WordStatus::Empty && w.raw == "\u{E61C}" && inside(w))
            .map(|w| w.id)
            .collect();
        if ds.is_empty() {
            continue;
        }
        let em = words[ds[0]].y1 - words[ds[0]].y0;
        if !(3.0..=20.0).contains(&em)
            || ds.iter().any(|&id| {
                let w = &words[id];
                (w.y1 - w.y0 - em).abs() > 1. || !(em * 0.7..=em * 1.3).contains(&(w.x1 - w.x0))
            })
        {
            continue;
        }
        if !(em * 4.0..=em * 25.0).contains(&(g.ys[p.y1] - g.ys[p.y0])) {
            continue;
        }
        let cells: Vec<_> = t
            .cells
            .iter()
            .enumerate()
            .filter(|(_, c)| same_frame(c, g, p))
            .collect();
        if cells.len() < 3
            || cells.iter().any(|(_, c)| {
                c.col != p.x0
                    || c.colspan != 1
                    || c.words
                        .iter()
                        .any(|&id| words.get(id).is_none_or(|w| !inside(w)))
            })
        {
            continue;
        }
        let ids: Vec<_> = cells
            .iter()
            .flat_map(|(_, c)| c.words.iter().copied())
            .collect();
        let mut unique = ids.clone();
        unique.sort();
        unique.dedup();
        if unique.len() != ids.len() {
            continue;
        }
        let Some(rows) = groups(&ids, words, em) else {
            continue;
        };
        for &(index, c) in &cells {
            let texts: Vec<_> = c.text.split('\n').map(str::to_string).collect();
            if texts.len() != 2
                || t.cells
                    .iter()
                    .enumerate()
                    .any(|(i, v)| i != index && v.row == c.row)
            {
                continue;
            }
            let Some(cr) = groups(&c.words, words, em) else {
                continue;
            };
            if cr.len() != 2 || cr[1].cy - cr[0].cy < em || cr[1].cy - cr[0].cy > em * 2.1 {
                continue;
            }
            if cr
                .iter()
                .flat_map(|r| r.ids.iter().copied())
                .collect::<Vec<_>>()
                != c.words
                || cr.iter().zip(&texts).any(|(r, s)| {
                    compact(s)
                        != compact(
                            &r.ids
                                .iter()
                                .map(|&id| words[id].text.as_str())
                                .collect::<String>(),
                        )
                })
            {
                continue;
            }
            let target = &cr[1];
            let ns = numbers(target, words, em);
            if ns.is_empty() || !named(target, &ns, words) {
                continue;
            }
            let mut dashes: Vec<_> = ds
                .iter()
                .copied()
                .filter(|&id| (center(&words[id]) - target.cy).abs() <= em * 0.35)
                .collect();
            if dashes.is_empty() {
                continue;
            }
            dashes.sort_by(|&a, &b| words[a].x0.total_cmp(&words[b].x0));
            // この高さの元字は、既存の対象記録か今回戻す既知横棒に限る。別のセル・未知PUAを巻き込まない。
            if words.iter().any(|w| {
                inside(w)
                    && (center(w) - target.cy).abs() <= em * 0.35
                    && !w.raw.trim().is_empty()
                    && !target.ids.contains(&w.id)
                    && !dashes.contains(&w.id)
            }) {
                continue;
            }
            let n = ns.len() + dashes.len();
            if !(3..=6).contains(&n) {
                continue;
            }
            let mut patterns: Vec<Vec<f32>> = vec![];
            for r in &rows {
                let rs = numbers(r, words, em);
                if rs.len() != n || !named(r, &rs, words) {
                    continue;
                }
                let xs: Vec<_> = rs.iter().map(|n| n.x1).collect();
                if xs.windows(2).any(|a| a[1] - a[0] < em * 1.5)
                    || g.xs[p.x1] - xs[n - 1] > em * 2.0
                {
                    continue;
                }
                if !patterns
                    .iter()
                    .any(|p| p.iter().zip(&xs).all(|(a, b)| (a - b).abs() <= 1.))
                {
                    patterns.push(xs);
                }
            }
            let matches: Vec<_> = patterns
                .iter()
                .filter(|xs| {
                    if ns.iter().zip(*xs).any(|(a, b)| (a.x1 - b).abs() > 1.)
                        || dashes
                            .iter()
                            .zip(xs.iter().skip(ns.len()))
                            .any(|(&id, x)| (words[id].x1 - x).abs() > 1.)
                    {
                        return false;
                    }
                    let good = |r: &&Row| {
                        let rs = numbers(r, words, em);
                        rs.len() == n
                            && named(r, &rs, words)
                            && rs.iter().zip(*xs).all(|(a, b)| (a.x1 - b).abs() <= 1.)
                            && (r.cy - target.cy).abs() <= em * 10.
                    };
                    rows.iter()
                        .filter(good)
                        .filter(|r| r.cy < target.cy - em * 0.75)
                        .count()
                        >= 2
                        && rows
                            .iter()
                            .filter(good)
                            .any(|r| r.cy > target.cy + em * 0.75)
                })
                .collect();
            if matches.len() != 1 {
                continue;
            }
            // 分離前の先行記録も同じ全数値欄を持つ。通常の折返し文章を数値記録へ変えない。
            let prev = numbers(&cr[0], words, em);
            if prev.len() != n
                || !named(&cr[0], &prev, words)
                || prev
                    .iter()
                    .zip(matches[0])
                    .any(|(a, b)| (a.x1 - b).abs() > 1.)
            {
                continue;
            }
            proposals.push(Proposal {
                index,
                texts,
                ids: cr.into_iter().map(|r| r.ids).collect(),
                dashes,
            });
        }
    }
    proposals.sort_by_key(|p| std::cmp::Reverse(p.index));
    let mut all = std::collections::HashSet::new();
    if proposals
        .iter()
        .any(|p| p.dashes.iter().any(|id| !all.insert(*id)))
    {
        return;
    }
    for p in proposals {
        let at = t.cells[p.index].row + 1;
        for c in &mut t.cells {
            if c.row >= at {
                c.row += 1;
            }
        }
        t.rows += 1;
        let mut added = t.cells[p.index].clone();
        added.row = at;
        added.text = p.texts[1].clone();
        added.words = p.ids[1].clone();
        t.cells[p.index].text = p.texts[0].clone();
        t.cells[p.index].words = p.ids[0].clone();
        for id in p.dashes {
            added.text.push_str("　－");
            added.words.push(id);
            words[id].text = "－".into();
            words[id].status = WordStatus::Used;
        }
        t.cells.insert(p.index + 1, added);
    }
}
pub(crate) fn restore_cell_record_dashes(units: &mut [Unit], words: &mut [Word], grids: &[Grid]) {
    let restore = |lines: &mut [Line], words: &mut [Word]| {
        for line in lines {
            if let Some(t) = &mut line.table {
                restore_table(t, words, grids);
                line.text = t.render();
            }
        }
    };
    for unit in units {
        match unit {
            Unit::Plain(lines) => restore(lines, words),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            ..} => {
                restore(preamble, words);
                for lines in after.iter_mut().chain(before).chain(outside) {
                    restore(lines, words);
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "cell_row_dash_tests.rs"]
mod tests;
