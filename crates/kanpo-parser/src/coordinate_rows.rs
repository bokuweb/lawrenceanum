//! Recover printed angle suffixes in complete, unheaded, physically ruled rows.
//! No semantic headings are inferred; every numeric Word and six original glyphs
//! must have one demonstrable final row/cell owner before a grid is edited.
use crate::{
    grid::{find_grids, Grid},
    lines::PageRules,
    trace::{Line, TableCell, Unit, Word, WordStatus},
};
use std::collections::HashSet;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Owner {
    line: usize,
    cell: Option<usize>,
}
struct Change {
    owner: Owner,
    text: String,
    symbols: Vec<(usize, char)>,
}
fn visit(units: &[Unit], mut f: impl FnMut(usize, &Line)) {
    let mut k = 0;
    for u in units {
        match u {
            Unit::Plain(ls) => {
                for l in ls {
                    f(k, l);
                    k += 1;
                }
            }
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            } => {
                for l in preamble {
                    f(k, l);
                    k += 1;
                }
                for ls in after.iter().chain(before).chain(outside) {
                    for l in ls {
                        f(k, l);
                        k += 1;
                    }
                }
            }
        }
    }
}
fn visit_mut(units: &mut [Unit], mut f: impl FnMut(usize, &mut Line)) {
    let mut k = 0;
    for u in units {
        match u {
            Unit::Plain(ls) => {
                for l in ls {
                    f(k, l);
                    k += 1;
                }
            }
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            } => {
                for l in preamble {
                    f(k, l);
                    k += 1;
                }
                for ls in after.iter_mut().chain(before).chain(outside) {
                    for l in ls {
                        f(k, l);
                        k += 1;
                    }
                }
            }
        }
    }
}
fn spans(text: &str) -> Vec<(usize, usize)> {
    let mut out = vec![];
    let mut start = None;
    for (i, c) in text.char_indices() {
        if c.is_whitespace() {
            if let Some(s) = start.take() {
                out.push((s, i));
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        out.push((s, text.len()));
    }
    out
}
fn decimal(s: &str) -> Option<u32> {
    let s: String = s
        .chars()
        .map(|c| {
            if ('０'..='９').contains(&c) {
                char::from_u32(c as u32 - '０' as u32 + '0' as u32).unwrap()
            } else {
                c
            }
        })
        .collect();
    if s.is_empty() || s.len() > 4 || !s.chars().all(|c| c.is_ascii_digit()) {
        None
    } else {
        s.parse().ok()
    }
}
/// Final Tables keep native geometry even when the printed cells run downwards.
/// Verify a temporary clockwise-unturned frame, then transfer only text and IDs
/// to the same existing owners. Source metadata and prior repairs stay native.
pub(crate) fn restore_coordinate_rows(
    units: &mut [Unit],
    words: &mut [Word],
    rules: Option<&PageRules>,
    extent: f32,
) {
    restore_horizontal_coordinate_rows(units, words, rules, extent);
    if !extent.is_finite()
        || extent <= 0.
        || !words.iter().any(|w| {
            w.status == WordStatus::Empty
                && matches!(w.raw.as_str(), "\u{EA6F}" | "\u{EA6D}" | "\u{EA6E}")
        })
    {
        return;
    }
    let turn = |x0: f32, y0: f32, x1: f32, y1: f32| (y0, extent - x1, y1, extent - x0);
    let mut rotated_words = words.to_vec();
    for w in &mut rotated_words {
        (w.x0, w.y0, w.x1, w.y1) = turn(w.x0, w.y0, w.x1, w.y1);
    }
    let mut rotated: Vec<Unit> = units
        .iter()
        .map(|u| match u {
            Unit::Plain(ls) => Unit::Plain(ls.clone()),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            } => Unit::Shinkyu {
                preamble: preamble.clone(),
                after: after.clone(),
                before: before.clone(),
                outside: outside.clone(),
            },
        })
        .collect();
    visit_mut(&mut rotated, |_, l| {
        if let Some(t) = &mut l.table {
            (t.x0, t.y0, t.x1, t.y1) = turn(t.x0, t.y0, t.x1, t.y1);
            for c in &mut t.cells {
                (c.x0, c.y0, c.x1, c.y1) = turn(c.x0, c.y0, c.x1, c.y1);
            }
        }
    });
    let rotated_rules = rules.map(|r| PageRules {
        vertical: r
            .horizontal
            .iter()
            .map(|h| crate::lines::VRule {
                x: h.y,
                y0: extent - h.x1,
                y1: extent - h.x0,
            })
            .collect(),
        horizontal: r
            .vertical
            .iter()
            .map(|v| crate::lines::HRule {
                y: extent - v.x,
                x0: v.y0,
                x1: v.y1,
            })
            .collect(),
        enclosures: vec![], short_fraction_bars: vec![],
    });
    crate::coordinate_symbols::restore_coordinate_symbols(&mut rotated, &mut rotated_words);
    restore_horizontal_coordinate_rows(
        &mut rotated,
        &mut rotated_words,
        rotated_rules.as_ref(),
        extent,
    );
    let mut restored = HashSet::new();
    for (a, b) in words.iter().zip(&rotated_words) {
        if a.text != b.text || a.status != b.status {
            let expected = match a.raw.as_str() {
                "\u{EA6F}" => "°",
                "\u{EA6D}" => "′",
                "\u{EA6E}" => "″",
                _ => return,
            };
            if a.status != WordStatus::Empty
                || !a.text.is_empty()
                || b.status != WordStatus::Used
                || b.text != expected
            {
                return;
            }
            restored.insert(a.id);
        }
    }
    if restored.is_empty() {
        return;
    }
    // Reject a stale owner or text mismatch instead of rebuilding a prior repair.
    let insertion_only = |old: &str, new: &str| {
        let mut old = old.chars().peekable();
        for c in new.chars() {
            if old.peek() == Some(&c) {
                old.next();
            } else if !['°', '′', '″'].contains(&c) {
                return false;
            }
        }
        old.next().is_none()
    };
    let mut old_lines = vec![];
    visit(units, |_, l| old_lines.push(l.clone()));
    let mut valid = true;
    let mut attachments = HashSet::new();
    let mut new_lines = vec![];
    visit(&rotated, |i, l| {
        let old = &old_lines[i];
        let mut owner = |a: &[usize], b: &[usize]| {
            if b.len() < a.len() || b[..a.len()] != *a {
                return false;
            }
            for id in &b[a.len()..] {
                if !restored.contains(id) || !attachments.insert(*id) {
                    return false;
                }
            }
            true
        };
        valid &= owner(&old.words, &l.words);
        if !insertion_only(&old.text, &l.text) {
            valid = false;
        }
        match (&old.table, &l.table) {
            (Some(a), Some(b)) if a.cells.len() == b.cells.len() => {
                for (ac, bc) in a.cells.iter().zip(&b.cells) {
                    valid &= owner(&ac.words, &bc.words);
                    if !insertion_only(&ac.text, &bc.text) {
                        valid = false;
                    }
                }
            }
            (None, None) => {}
            _ => valid = false,
        }
        new_lines.push(l.clone());
    });
    if !valid || attachments != restored {
        return;
    }
    visit_mut(units, |i, l| {
        let n = &new_lines[i];
        l.text = n.text.clone();
        l.words = n.words.clone();
        if let (Some(a), Some(b)) = (&mut l.table, &n.table) {
            for (ac, bc) in a.cells.iter_mut().zip(&b.cells) {
                ac.text = bc.text.clone();
                ac.words = bc.words.clone();
            }
        }
    });
    for (a, b) in words.iter_mut().zip(rotated_words) {
        if restored.contains(&a.id) {
            a.text = b.text;
            a.status = b.status;
        }
    }
}
fn restore_horizontal_coordinate_rows(
    units: &mut [Unit],
    words: &mut [Word],
    rules: Option<&PageRules>,
    extent: f32,
) {
    if !words.iter().any(|w| {
        w.status == WordStatus::Empty
            && matches!(w.raw.as_str(), "\u{EA6F}" | "\u{EA6D}" | "\u{EA6E}")
    }) {
        return;
    }
    let Some(rules) = rules else { return };
    let mut counts = vec![0usize; words.len()];
    let mut owners = vec![None; words.len()];
    let mut ls = vec![];
    visit(units, |line, l| {
        ls.push(l.clone());
        let mut own = |ids: &[usize], cell| {
            for &id in ids {
                if id < words.len() {
                    counts[id] += 1;
                    owners[id] = Some(Owner { line, cell });
                }
            }
        };
        own(&l.words, None);
        if let Some(t) = &l.table {
            for (i, c) in t.cells.iter().enumerate() {
                own(&c.words, Some(i));
            }
        }
    });
    let mut changes = vec![];
    let mut planned = HashSet::new();
    for g in find_grids(rules, extent, extent) {
        let Some(edits) = plan_grid(&g, words, &ls, &counts, &owners) else {
            continue;
        };
        // Overlapping physical frames must never attach the same source or edit an owner twice.
        if edits.iter().any(|e| {
            changes.iter().any(|p: &Change| p.owner == e.owner)
                || e.symbols.iter().any(|(id, _)| planned.contains(id))
        }) {
            continue;
        }
        for e in edits {
            for (id, _) in &e.symbols {
                planned.insert(*id);
            }
            changes.push(e);
        }
    }
    visit_mut(units, |line, l| {
        let mut edited = false;
        for e in changes.iter().filter(|e| e.owner.line == line) {
            let ids = if let Some(i) = e.owner.cell {
                let c = &mut l.table.as_mut().unwrap().cells[i];
                c.text = e.text.clone();
                edited = true;
                &mut c.words
            } else {
                l.text = e.text.clone();
                &mut l.words
            };
            for &(id, ch) in &e.symbols {
                ids.push(id);
                words[id].text = ch.to_string();
                words[id].status = WordStatus::Used;
            }
        }
        if edited {
            l.text = l.table.as_ref().unwrap().render();
        }
    });
}
fn plan_grid(
    g: &Grid,
    words: &[Word],
    lines: &[Line],
    counts: &[usize],
    owners: &[Option<Owner>],
) -> Option<Vec<Change>> {
    let rows = g.ys.len().checked_sub(1)?;
    if g.xs.len() != 4
        || rows < 2
        || g.cells.len() != rows * 3
        || g.cells.iter().any(|c| c.x1 != c.x0 + 1 || c.y1 != c.y0 + 1)
    {
        return None;
    }
    let mut changes = vec![];
    let mut previous = None;
    let mut common_table = None;
    for row in 0..rows {
        let mut columns: Vec<Vec<&Word>> = vec![vec![], vec![], vec![]];
        for w in words {
            let (x, y) = ((w.x0 + w.x1) * 0.5, (w.y0 + w.y1) * 0.5);
            if g.ys[row] <= y && y < g.ys[row + 1] && g.left() <= x && x < g.right() {
                let col = (0..3).find(|&col| g.xs[col] <= x && x < g.xs[col + 1])?;
                if w.id >= words.len()
                    || !std::ptr::eq(w, &words[w.id])
                    || w.x0 < g.xs[col]
                    || w.x1 > g.xs[col + 1]
                    || w.y0 < g.ys[row]
                    || w.y1 > g.ys[row + 1]
                {
                    return None;
                }
                columns[col].push(w);
            }
        }
        for c in &mut columns {
            c.sort_by(|a, b| a.x0.total_cmp(&b.x0));
        }
        if columns[0].len() != 1 || columns[1].len() != 8 || columns[2].len() != 8 {
            return None;
        }
        let point = columns[0][0];
        let n = decimal(&point.text)?;
        if n == 0
            || point.status != WordStatus::Used
            || counts[point.id] != 1
            || previous.is_some_and(|p| n != p + 1)
        {
            return None;
        }
        previous = Some(n);
        let mut numeric = vec![point.id];
        let mut halves = vec![];
        for col in 1..=2 {
            let nums: Vec<_> = columns[col]
                .iter()
                .filter(|w| !w.text.is_empty())
                .copied()
                .collect();
            if nums.len() != 5
                || nums
                    .iter()
                    .any(|w| w.status != WordStatus::Used || counts[w.id] != 1)
            {
                return None;
            }
            let em = nums[0].y1 - nums[0].y0;
            if (point.y0 - nums[0].y0).abs() > em * 0.05
                || ((point.y1 - point.y0) - em).abs() > em * 0.05
            {
                return None;
            }
            let ids: Vec<_> = nums.iter().map(|w| w.id).collect();
            numeric.extend(&ids);
            let mut c = TableCell {
                row,
                col,
                rowspan: 1,
                colspan: 1,
                x0: g.xs[col],
                y0: g.ys[row],
                x1: g.xs[col + 1],
                y1: g.ys[row + 1],
                text: nums.iter().map(|w| w.text.as_str()).collect(),
                words: ids,
            };
            let own = owners[nums[0].id]?;
            if nums.iter().any(|w| owners[w.id] != Some(own)) {
                return None;
            }
            if let Some(ci) = own.cell {
                let t = lines[own.line].table.as_ref()?;
                let actual = &t.cells[ci];
                if t.cols != 3
                    || t.rows != rows
                    || actual.row != row
                    || actual.col != col
                    || actual.rowspan != 1
                    || actual.colspan != 1
                    || actual.words.len() != 5
                    || actual.words.iter().any(|id| !c.words.contains(id))
                    || [
                        actual.x0 - c.x0,
                        actual.y0 - c.y0,
                        actual.x1 - c.x1,
                        actual.y1 - c.y1,
                    ]
                    .iter()
                    .any(|d| d.abs() > 0.1)
                {
                    return None;
                }
                c.text = actual.text.clone();
                c.words = actual.words.clone();
            }
            let e = crate::coordinate_symbols::restore_cell(
                &c,
                words,
                if col == 1 { 90 } else { 180 },
                counts,
            )?;
            if e.symbols.iter().any(|(id, _)| counts[*id] != 0) {
                return None;
            }
            halves.push((own, e, c.text));
        }
        let pown = owners[point.id]?;
        if let Some(pi) = pown.cell {
            let t = lines[pown.line].table.as_ref()?;
            let pc = &t.cells[pi];
            if pc.row != row
                || pc.col != 0
                || pc.rowspan != 1
                || pc.colspan != 1
                || pc.words != [point.id]
                || pc.text.trim() != point.text
                || halves
                    .iter()
                    .any(|(o, _, _)| o.line != pown.line || o.cell.is_none())
            {
                return None;
            }
            if common_table.is_some_and(|v| v != pown.line) {
                return None;
            }
            common_table = Some(pown.line);
            for (own, e, _) in halves {
                changes.push(Change {
                    owner: own,
                    text: e.text,
                    symbols: e.symbols,
                });
            }
        } else {
            if common_table.is_some() || halves.iter().any(|(o, _, _)| *o != pown) {
                return None;
            }
            let l = &lines[pown.line];
            let fields = spans(&l.text);
            if l.table.is_some()
                || l.words != numeric
                || fields.len() != 3
                || l.text[fields[0].0..fields[0].1] != point.text
            {
                return None;
            }
            let mut insertions = vec![];
            let mut syms = vec![];
            for (col, (_, e, old)) in halves.into_iter().enumerate() {
                let (start, end) = fields[col + 1];
                if l.text[start..end] != old {
                    return None;
                }
                let nums: Vec<_> = columns[col + 1]
                    .iter()
                    .filter(|w| !w.text.is_empty())
                    .copied()
                    .collect();
                for (offset, ch) in [
                    (nums[0].text.len(), '°'),
                    (nums[0].text.len() + nums[1].text.len(), '′'),
                    (old.len(), '″'),
                ] {
                    insertions.push((start + offset, ch));
                }
                syms.extend(e.symbols);
            }
            let mut text = l.text.clone();
            insertions.sort_by_key(|v| v.0);
            for (at, ch) in insertions.into_iter().rev() {
                text.insert(at, ch);
            }
            changes.push(Change {
                owner: pown,
                text,
                symbols: syms,
            });
        }
    }
    if changes
        .iter()
        .enumerate()
        .any(|(i, e)| changes[i + 1..].iter().any(|f| f.owner == e.owner))
    {
        return None;
    }
    Some(changes)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        lines::{HRule, VRule},
        trace::Table,
    };
    fn fixture(table: bool) -> (Vec<Unit>, Vec<Word>, PageRules) {
        let xs = [0., 23., 91., 159.];
        let mut words = vec![];
        let mut cells = vec![];
        let mut plain = vec![];
        for row in 0..2 {
            let y = row as f32 * 18. + 5.;
            let mut numeric = vec![];
            let mut fields = vec![];
            let mut add = |text: &str, raw: &str, x: f32, width: f32| {
                let id = words.len();
                words.push(Word {
                    id,
                    x0: x,
                    y0: y,
                    x1: x + width,
                    y1: y + 8.,
                    text: text.into(),
                    raw: raw.into(),
                    status: if text.is_empty() {
                        WordStatus::Empty
                    } else {
                        WordStatus::Used
                    },
                });
                id
            };
            let point = (row + 1).to_string();
            let id = add(&point, &point, 6., 4.);
            numeric.push(id);
            fields.push(point.clone());
            cells.push(TableCell {
                row,
                col: 0,
                rowspan: 1,
                colspan: 1,
                x0: 0.,
                y0: row as f32 * 18.,
                x1: 23.,
                y1: (row + 1) as f32 * 18.,
                text: point,
                words: vec![id],
            });
            for (col, deg) in [(1, "34"), (2, "137")] {
                let x = xs[col] + 6.;
                let width = deg.len() as f32 * 4.;
                let mut ids = vec![];
                for (text, raw, dx, pw) in [
                    (deg, deg, 0., width),
                    ("", "\u{EA6F}", width - 2., 8.),
                    ("53", "53", width + 4., 8.),
                    ("", "\u{EA6D}", width + 10., 8.),
                    ("19", "19", width + 16., 8.),
                    (".", "\u{EA74}", width + 21., 8.),
                    ("8544", "8544", width + 26., 16.),
                    ("", "\u{EA6E}", width + 40., 8.),
                ] {
                    let id = add(text, raw, x + dx, pw);
                    if !text.is_empty() {
                        ids.push(id);
                    }
                }
                numeric.extend(&ids);
                let compact = format!("{deg}5319.8544");
                fields.push(compact.clone());
                cells.push(TableCell {
                    row,
                    col,
                    rowspan: 1,
                    colspan: 1,
                    x0: xs[col],
                    y0: row as f32 * 18.,
                    x1: xs[col + 1],
                    y1: (row + 1) as f32 * 18.,
                    text: if table {
                        format!("{deg} 53 19.8544")
                    } else {
                        compact
                    },
                    words: ids,
                });
            }
            plain.push(Line::text(
                format!(" \t{}　{}  {} ", fields[0], fields[1], fields[2]),
                numeric,
            ));
        }
        let units = if table {
            let t = Table {
                x0: 0.,
                y0: 0.,
                x1: 159.,
                y1: 36.,
                rows: 2,
                cols: 3,
                cells,
            };
            vec![Unit::Plain(vec![Line {
                text: t.render(),
                words: vec![],
                table: Some(t),
            }])]
        } else {
            vec![Unit::Plain(plain)]
        };
        let mut rules = PageRules::default();
        rules.vertical = xs
            .into_iter()
            .map(|x| VRule { x, y0: 0., y1: 36. })
            .collect();
        rules.horizontal = [0., 18., 36.]
            .into_iter()
            .map(|y| HRule {
                y,
                x0: 0.,
                x1: 159.,
            })
            .collect();
        (units, words, rules)
    }
    fn unit_snapshot(u: &[Unit]) -> String {
        let mut ls = vec![];
        visit(u, |_, l| ls.push(l.clone()));
        serde_json::to_string(&ls).unwrap()
    }
    fn snap(u: &[Unit], w: &[Word]) -> String {
        format!("{}{:?}", unit_snapshot(u), w)
    }
    #[test]
    fn restores_complete_unheaded_table_and_plain_rows_without_changing_existing_bytes() {
        for table in [true, false] {
            let (mut u, mut w, r) = fixture(table);
            let old_u = unit_snapshot(&u);
            let old_w = w.clone();
            restore_coordinate_rows(&mut u, &mut w, Some(&r), 200.);
            assert_eq!(
                w.iter()
                    .filter(|w| ["°", "′", "″"].contains(&w.text.as_str()))
                    .count(),
                12
            );
            let texts = u.iter().map(Unit::text).collect::<Vec<_>>().join("\n");
            assert!(texts.contains(if table {
                "34° 53′ 19.8544″"
            } else {
                "34°53′19.8544″"
            }));
            for (a, b) in w.iter().zip(&old_w) {
                assert_eq!(
                    (a.id, &a.raw, a.x0, a.y0, a.x1, a.y1),
                    (b.id, &b.raw, b.x0, b.y0, b.x1, b.y1)
                );
                if !b.text.is_empty() {
                    assert_eq!(
                        serde_json::to_string(a).unwrap(),
                        serde_json::to_string(b).unwrap()
                    );
                } else {
                    assert_eq!(a.status, WordStatus::Used);
                }
            }
            let once = snap(&u, &w);
            restore_coordinate_rows(&mut u, &mut w, Some(&r), 200.);
            assert_eq!(snap(&u, &w), once);
            visit_mut(&mut u, |_, l| {
                l.text = l.text.replace(['°', '′', '″'], "");
                l.words.retain(|&id| !old_w[id].text.is_empty());
                if let Some(t) = &mut l.table {
                    for c in &mut t.cells {
                        c.text = c.text.replace(['°', '′', '″'], "");
                        c.words.retain(|&id| !old_w[id].text.is_empty());
                    }
                }
            });
            assert_eq!(unit_snapshot(&u), old_u);
        }
    }
    #[test]
    fn rejects_nonconsecutive_points_and_price_or_other_pua_decoys() {
        for table in [true, false] {
            for mode in 0..4 {
                let (mut u, mut w, r) = fixture(table);
                match mode {
                    0 => {
                        w[17].text = "3".into();
                        visit_mut(&mut u, |_, l| {
                            if let Some(t) = &mut l.table {
                                t.cells[3].text = "3".into();
                                l.text = t.render();
                            } else {
                                l.text = l.text.replacen("2　", "3　", 1);
                            }
                        });
                    }
                    1 => w[2].raw = "\u{EA70}".into(),
                    2 => {
                        w[1].text = "価格".into();
                    }
                    _ => w[8].raw = "\u{E690}".into(),
                };
                let old = snap(&u, &w);
                restore_coordinate_rows(&mut u, &mut w, Some(&r), 200.);
                assert_eq!(snap(&u, &w), old, "table={table} mode={mode}");
            }
        }
    }
    #[test]
    fn rejects_misaligned_columns_baselines_vertical_advance_and_missing_real_rules() {
        for mode in 0..5 {
            let (mut u, mut w, mut r) = fixture(false);
            match mode {
                0 => r.vertical[2].x -= 12.,
                1 => {
                    w[2].y0 += 4.;
                    w[2].y1 += 4.;
                }
                2 => {
                    for a in &mut w {
                        a.x1 = a.x0 + 8.;
                        a.y1 = a.y0 + 20.;
                    }
                }
                3 => {
                    w[2].x0 += 4.;
                    w[2].x1 += 4.;
                }
                _ => r.vertical.pop().map(|_| ()).unwrap(),
            };
            let old = snap(&u, &w);
            restore_coordinate_rows(
                &mut u,
                &mut w,
                if mode == 4 { None } else { Some(&r) },
                200.,
            );
            assert_eq!(snap(&u, &w), old);
        }
    }
    #[test]
    fn rejects_other_article_owners_extra_prose_and_non_used_anchors() {
        for table in [true, false] {
            for mode in 0..7 {
                let (mut u, mut w, r) = fixture(table);
                match mode {
                    0 => u.push(Unit::Plain(vec![Line::text("別記事".into(), vec![1])])),
                    1 => {
                        w[1].status = WordStatus::Margin;
                    }
                    2 => {
                        w[1].status = WordStatus::Empty;
                    }
                    3 => {
                        w[2].text = "°".into();
                        w[2].status = WordStatus::Used;
                    }
                    4 => {
                        let mut a = w[2].clone();
                        a.id = w.len();
                        w.push(a);
                    }
                    5 => {
                        w[2].id = 999;
                    }
                    _ => {
                        let id = w.len();
                        w.push(Word {
                            id,
                            x0: 55.,
                            y0: 5.,
                            x1: 63.,
                            y1: 13.,
                            raw: "別".into(),
                            text: "別".into(),
                            status: WordStatus::Used,
                        });
                    }
                };
                let old = snap(&u, &w);
                restore_coordinate_rows(&mut u, &mut w, Some(&r), 200.);
                assert_eq!(snap(&u, &w), old, "table={table} mode={mode}");
            }
        }
    }
    #[test]
    fn rejects_split_plain_owners_reordered_ids_and_mismatched_existing_text() {
        for mode in 0..4 {
            let (mut u, mut w, r) = fixture(false);
            let Unit::Plain(ls) = &mut u[0] else { panic!() };
            match mode {
                0 => {
                    let id = ls[0].words.pop().unwrap();
                    ls.push(Line::text("別記事".into(), vec![id]));
                }
                1 => ls[0].words.swap(1, 2),
                2 => ls[0].text.push('円'),
                _ => ls[0].text = ls[0].text.replace("3453", "3454"),
            };
            let old = snap(&u, &w);
            restore_coordinate_rows(&mut u, &mut w, Some(&r), 200.);
            assert_eq!(snap(&u, &w), old);
        }
    }

    // A test-only inverse of the native sideways frame. Source metadata must stay in it.
    fn sideways_fixture(table: bool, headed: bool) -> (Vec<Unit>, Vec<Word>, PageRules) {
        let (mut u, mut w, mut r) = fixture(table);
        if headed {
            for word in &mut w {
                word.y0 += 18.;
                word.y1 += 18.;
            }
            for rule in &mut r.vertical {
                rule.y1 += 18.;
            }
            for rule in &mut r.horizontal {
                rule.y += 18.;
            }
            r.horizontal.insert(
                0,
                HRule {
                    y: 0.,
                    x0: 0.,
                    x1: 159.,
                },
            );
            visit_mut(&mut u, |_, l| {
                let t = l.table.as_mut().unwrap();
                t.y1 += 18.;
                t.rows += 1;
                for c in &mut t.cells {
                    c.y0 += 18.;
                    c.y1 += 18.;
                    c.row += 1;
                }
                for (col, text) in ["点", "北緯", "東経"].iter().enumerate() {
                    t.cells.push(TableCell {
                        row: 0,
                        col,
                        rowspan: 1,
                        colspan: 1,
                        x0: [0., 23., 91.][col],
                        x1: [23., 91., 159.][col],
                        y0: 0.,
                        y1: 18.,
                        text: text.to_string(),
                        words: vec![],
                    });
                }
                l.text = t.render();
            });
        }
        let turn = |x0: f32, y0: f32, x1: f32, y1: f32| (200. - y1, x0, 200. - y0, x1);
        for word in &mut w {
            (word.x0, word.y0, word.x1, word.y1) = turn(word.x0, word.y0, word.x1, word.y1);
        }
        visit_mut(&mut u, |_, l| {
            if let Some(t) = &mut l.table {
                (t.x0, t.y0, t.x1, t.y1) = turn(t.x0, t.y0, t.x1, t.y1);
                for c in &mut t.cells {
                    (c.x0, c.y0, c.x1, c.y1) = turn(c.x0, c.y0, c.x1, c.y1);
                }
            }
        });
        let old = r.clone();
        r.horizontal = old
            .vertical
            .iter()
            .map(|v| HRule {
                y: v.x,
                x0: 200. - v.y1,
                x1: 200. - v.y0,
            })
            .collect();
        r.vertical = old
            .horizontal
            .iter()
            .map(|h| VRule {
                x: 200. - h.y,
                y0: h.x0,
                y1: h.x1,
            })
            .collect();
        (u, w, r)
    }
    #[test]
    fn sideways_unheaded_table_keeps_original_metadata_and_prior_owner_ids() {
        let (mut u, mut w, r) = sideways_fixture(true, false);
        let mut old = vec![];
        visit(&u, |_, l| old.push(l.clone()));
        let oldw = w.clone();
        restore_coordinate_rows(&mut u, &mut w, Some(&r), 200.);
        assert_eq!(
            w.iter()
                .filter(|x| ["°", "′", "″"].contains(&x.text.as_str()))
                .count(),
            12
        );
        visit(&u, |li, l| {
            let ot = old[li].table.as_ref().unwrap();
            let t = l.table.as_ref().unwrap();
            assert_eq!(
                (t.x0, t.y0, t.x1, t.y1, t.rows, t.cols),
                (ot.x0, ot.y0, ot.x1, ot.y1, ot.rows, ot.cols)
            );
            for (c, oc) in t.cells.iter().zip(&ot.cells) {
                assert_eq!(
                    (c.row, c.col, c.rowspan, c.colspan, c.x0, c.y0, c.x1, c.y1),
                    (oc.row, oc.col, oc.rowspan, oc.colspan, oc.x0, oc.y0, oc.x1, oc.y1)
                );
                assert_eq!(&c.words[..oc.words.len()], oc.words);
                assert_eq!(c.text.replace(['°', '′', '″'], ""), oc.text);
            }
        });
        for (a, b) in w.iter().zip(oldw) {
            assert_eq!(
                (a.id, &a.raw, a.x0, a.y0, a.x1, a.y1),
                (b.id, &b.raw, b.x0, b.y0, b.x1, b.y1)
            );
            if !b.text.is_empty() {
                assert_eq!(
                    serde_json::to_string(a).unwrap(),
                    serde_json::to_string(&b).unwrap()
                );
            }
        }
        let once = snap(&u, &w);
        restore_coordinate_rows(&mut u, &mut w, Some(&r), 200.);
        assert_eq!(snap(&u, &w), once);
    }
    #[test]
    fn sideways_plain_rows_insert_suffixes_without_rebuilding_existing_spacing() {
        let (mut u, mut w, r) = sideways_fixture(false, false);
        let old = unit_snapshot(&u);
        restore_coordinate_rows(&mut u, &mut w, Some(&r), 200.);
        assert!(u[0].text().contains("34°53′19.8544″"));
        visit_mut(&mut u, |_, l| {
            l.text = l.text.replace(['°', '′', '″'], "");
            l.words
                .retain(|&id| !matches!(w[id].raw.as_str(), "\u{EA6F}" | "\u{EA6D}" | "\u{EA6E}"));
        });
        assert_eq!(unit_snapshot(&u), old);
    }
    #[test]
    fn sideways_formal_headers_reuse_cell_proof_without_fabricating_headings() {
        let (mut u, mut w, r) = sideways_fixture(true, true);
        restore_coordinate_rows(&mut u, &mut w, Some(&r), 200.);
        assert_eq!(
            w.iter()
                .filter(|x| ["°", "′", "″"].contains(&x.text.as_str()))
                .count(),
            12
        );
        assert!(u[0].text().contains("点　北緯　東経"));
        assert!(u[0].text().contains("34° 53′ 19.8544″"));
    }
    #[test]
    fn sideways_rows_reject_missing_rules_split_owners_noncoordinate_or_ambiguous_glyphs() {
        for table in [false, true] {
            for mode in 0..7 {
                let (mut u, mut w, mut r) = sideways_fixture(table, false);
                match mode {
                    0 => r.vertical.clear(),
                    1 => w[2].raw = "\u{EA70}".into(),
                    2 => {
                        let mut g = w[2].clone();
                        g.id = w.len();
                        w.push(g);
                    }
                    3 => w[1].text = "99".into(),
                    4 => w[0].status = WordStatus::Noise,
                    5 => {
                        let id = w.iter().position(|x| x.raw == "\u{EA6F}").unwrap();
                        u.push(Unit::Plain(vec![Line::text(
                            "other article".into(),
                            vec![id],
                        )]))
                    }
                    _ => w[0].text = "9".into(),
                };
                let before = snap(&u, &w);
                restore_coordinate_rows(&mut u, &mut w, Some(&r), 200.);
                assert_eq!(snap(&u, &w), before, "table={table} mode={mode}");
            }
        }
    }
}
