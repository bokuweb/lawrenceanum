//! Independent textual and numeric fractions require two real native bars.
use crate::parallel_fraction_geometry::{self, Atom, Bar, Formula};
use crate::trace::{Line, Unit, Word, WordStatus};
use std::collections::HashMap;
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn count(lines: &[Line], counts: &mut HashMap<usize, usize>) {
    for l in lines {
        if let Some(t) = &l.table {
            for c in &t.cells {
                for id in &c.words {
                    *counts.entry(*id).or_default() += 1;
                }
            }
        } else {
            for id in &l.words {
                *counts.entry(*id).or_default() += 1;
            }
        }
    }
}
fn replace(text: &mut String, old: &str, new: &str) -> bool {
    let c = compact(text);
    let Some(a) = c.find(old) else { return false };
    if c.rfind(old) != Some(a) {
        return false;
    }
    let z = a + old.len();
    let (mut off, mut start, mut end) = (0, None, None);
    for (i, ch) in text.char_indices().filter(|(_, c)| !c.is_whitespace()) {
        if off == a {
            start = Some(i)
        }
        off += ch.len_utf8();
        if off == z {
            end = Some(i + ch.len_utf8());
            break;
        }
    }
    if let (Some(a), Some(z)) = (start, end) {
        text.replace_range(a..z, new);
        true
    } else {
        false
    }
}
fn rows(ids: &[usize], by: &HashMap<usize, &Word>) -> Option<Vec<Vec<usize>>> {
    let mut ws: Vec<_> = ids
        .iter()
        .map(|id| by.get(id).copied())
        .collect::<Option<_>>()?;
    ws.sort_by(|a, b| {
        ((a.y0 + a.y1) / 2.)
            .total_cmp(&((b.y0 + b.y1) / 2.))
            .then(a.x0.total_cmp(&b.x0))
    });
    let mut rs: Vec<(f32, f32, Vec<usize>)> = vec![];
    for w in ws {
        if w.text.trim().is_empty() {
            continue;
        }
        let y = (w.y0 + w.y1) / 2.;
        let em = w.y1 - w.y0;
        match rs.last_mut() {
            Some((cy, h, ids)) if (y - *cy).abs() <= em.min(*h) * 0.2 => {
                *cy = (*cy * ids.len() as f32 + y) / (ids.len() + 1) as f32;
                *h = h.min(em);
                ids.push(w.id);
            }
            _ => rs.push((y, em, vec![w.id])),
        }
    }
    Some(
        rs.into_iter()
            .map(|(_, _, mut ids)| {
                ids.sort_by(|a, b| by[a].x0.total_cmp(&by[b].x0));
                ids
            })
            .collect(),
    )
}
fn certify(
    ids: &[usize],
    by: &HashMap<usize, &Word>,
    counts: &HashMap<usize, usize>,
    bars: &[Bar],
) -> Option<Formula> {
    if ids.iter().any(|id| counts.get(id) != Some(&1)) {
        return None;
    }
    let atoms: Vec<_> = ids
        .iter()
        .map(|id| {
            let w = by.get(id)?;
            Some(Atom {
                id: *id,
                owner: Some(0),
                eligible: w.status == WordStatus::Used,
                raw: w.raw.clone(),
                text: w.text.clone(),
                x0: w.x0,
                y0: w.y0,
                x1: w.x1,
                y1: w.y1,
            })
        })
        .collect::<Option<_>>()?;
    let x0 = atoms.iter().map(|a| a.x0).fold(f32::INFINITY, f32::min);
    let x1 = atoms.iter().map(|a| a.x1).fold(f32::NEG_INFINITY, f32::max);
    let y0 = atoms.iter().map(|a| a.y0).fold(f32::INFINITY, f32::min);
    let y1 = atoms.iter().map(|a| a.y1).fold(f32::NEG_INFINITY, f32::max);
    if by.values().any(|w| {
        !ids.contains(&w.id)
            && !w.raw.is_empty()
            && (w.x0 + w.x1) / 2. >= x0
            && (w.x0 + w.x1) / 2. <= x1
            && (w.y0 + w.y1) / 2. >= y0
            && (w.y0 + w.y1) / 2. <= y1
    }) {
        return None;
    }
    parallel_fraction_geometry::parse(&atoms, bars)
}
pub(crate) fn restore(
    units: &mut [Unit],
    words: &[Word],
    long: &[(f32, f32, f32)],
    short: &[(f32, f32, f32)],
) {
    if short.is_empty() {
        return;
    }
    let bars: Vec<_> = long
        .iter()
        .chain(short)
        .map(|&(y, x0, x1)| Bar { y, x0, x1 })
        .collect();
    let by: HashMap<_, _> = words.iter().map(|w| (w.id, w)).collect();
    let mut counts = HashMap::new();
    for u in units.iter() {
        match u {
            Unit::Plain(ls) => count(ls, &mut counts),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            ..} => {
                count(preamble, &mut counts);
                for ls in after.iter().chain(before).chain(outside) {
                    count(ls, &mut counts)
                }
            }
        }
    }
    for u in units {
        let Unit::Plain(lines) = u else { continue };
        for l in lines.iter_mut() {
            let Some(t) = &mut l.table else { continue };
            let mut changed = false;
            for c in &mut t.cells {
                let Some(rs) = rows(&c.words, &by) else {
                    continue;
                };
                for group in rs.windows(3) {
                    let ids: Vec<_> = group.iter().flatten().copied().collect();
                    if let Some(f) = certify(&ids, &by, &counts, &bars) {
                        changed |= replace(&mut c.text, &f.original, &f.tree.render());
                    }
                }
            }
            if changed {
                l.text = t.render();
            }
        }
        let mut i = 0;
        while i + 2 < lines.len() {
            let group = &lines[i..i + 3];
            if group.iter().all(|l| l.table.is_none()) {
                let ids: Vec<_> = group.iter().flat_map(|l| l.words.iter()).copied().collect();
                if let Some(f) = certify(&ids, &by, &counts, &bars) {
                    let original: String = group.iter().map(|l| compact(&l.text)).collect();
                    if original == f.original {
                        lines.splice(i..i + 3, [Line::text(f.tree.render(), ids)]);
                        i += 1;
                        continue;
                    }
                }
            }
            i += 1;
        }
    }
}

#[cfg(test)]
#[path = "parallel_fraction_adapter_tests.rs"]
mod tests;
