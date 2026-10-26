//! Preserve original words/owners; rewrite only an unambiguous formula substring.
use crate::numeric_fraction_geometry;
use crate::parallel_fraction_geometry::{Atom, Bar};
use crate::trace::{Line, Unit, Word, WordStatus};
use std::collections::{HashMap, HashSet};
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn count(ls: &[Line], m: &mut HashMap<usize, usize>) {
    for l in ls {
        if let Some(t) = &l.table {
            for c in &t.cells {
                for id in &c.words {
                    *m.entry(*id).or_default() += 1
                }
            }
        } else {
            for id in &l.words {
                *m.entry(*id).or_default() += 1
            }
        }
    }
}
fn formulas(
    ids: &[usize],
    words: &[Word],
    counts: &HashMap<usize, usize>,
    bars: &[Bar],
) -> Vec<numeric_fraction_geometry::Formula> {
    if ids.len() < 6
        || !words
            .iter()
            .any(|w| ids.contains(&w.id) && w.text.ends_with('×'))
    {
        return vec![];
    }
    let own: HashSet<_> = ids.iter().copied().collect();
    if own.len() != ids.len() {
        return vec![];
    }
    let atoms: Vec<_> = words
        .iter()
        .map(|w| Atom {
            id: w.id,
            owner: if own.contains(&w.id) && counts.get(&w.id) == Some(&1) {
                Some(0)
            } else {
                None
            },
            eligible: if own.contains(&w.id) {
                w.status == WordStatus::Used && counts.get(&w.id) == Some(&1)
            } else {
                w.status == WordStatus::Empty && counts.get(&w.id).is_none()
            },
            raw: w.raw.clone(),
            text: w.text.clone(),
            x0: w.x0,
            y0: w.y0,
            x1: w.x1,
            y1: w.y1,
        })
        .collect();
    numeric_fraction_geometry::parse(&atoms, bars)
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
pub(crate) fn restore(
    units: &mut [Unit],
    words: &[Word],
    long: &[(f32, f32, f32)],
    short: &[(f32, f32, f32)],
) {
    let bars: Vec<_> = long
        .iter()
        .chain(short)
        .map(|&(y, x0, x1)| Bar { y, x0, x1 })
        .collect();
    if bars.len() < 2 {
        return;
    }
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
            if let Some(t) = &mut l.table {
                let mut changed = false;
                for c in &mut t.cells {
                    for f in formulas(&c.words, words, &counts, &bars) {
                        changed |= replace(&mut c.text, &f.original, &f.tree.render());
                    }
                }
                if changed {
                    l.text = t.render()
                }
            } else {
                for f in formulas(&l.words, words, &counts, &bars) {
                    replace(&mut l.text, &f.original, &f.tree.render());
                }
            }
        }
        let mut i = 0;
        while i + 2 < lines.len() {
            let group = &lines[i..i + 3];
            if group.iter().all(|l| l.table.is_none()) {
                let ids: Vec<_> = group.iter().flat_map(|l| l.words.iter().copied()).collect();
                let original: String = group.iter().map(|l| compact(&l.text)).collect();
                let fs: Vec<_> = formulas(&ids, words, &counts, &bars)
                    .into_iter()
                    .filter(|f| f.original == original && f.source_ids.len() == ids.len())
                    .collect();
                if fs.len() == 1 {
                    lines.splice(i..i + 3, [Line::text(fs[0].tree.render(), ids)]);
                    i += 1;
                    continue;
                }
            }
            i += 1;
        }
    }
}

#[cfg(test)]
#[path = "numeric_fraction_adapter_tests.rs"]
mod tests;
