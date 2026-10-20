//! Restore verified numeric micro-unit suffixes within one unchanged existing owner.
use crate::trace::{Line, Unit, Word, WordStatus};
use std::collections::{BTreeMap, BTreeSet};
fn marker(c: char) -> Option<&'static str> { (c == '\u{E24F}').then_some("μ") }
fn is_private_use(c: char) -> bool {
    crate::normalize::is_private_use(c)
        || matches!(c as u32, 0xF0000..=0xFFFFD | 0x100000..=0x10FFFD)
}
fn numeric_measurement(raw: &str, pos: usize) -> bool {
    let prefix = &raw[..pos];
    let start = prefix.char_indices().rev()
        .find(|(_, c)| !c.is_ascii_digit() && *c != '.')
        .map_or(0, |(i,c)| i+c.len_utf8());
    let number = &prefix[start..];
    let mut parts = number.split('.');
    let Some(integer) = parts.next() else {return false};
    if integer.is_empty() || !integer.chars().all(|c|c.is_ascii_digit()) {return false}
    if let Some(fraction) = parts.next() {
        if fraction.is_empty() || !fraction.chars().all(|c|c.is_ascii_digit()) {return false}
    }
    if parts.next().is_some() || prefix[..start].chars().next_back().is_some_and(|c|c.is_ascii_alphanumeric()) {return false}
    let suffix = &raw[pos+'\u{E24F}'.len_utf8()..];
    let Some(unit) = suffix.chars().next() else {return false};
    if !matches!(unit,'L'|'m') {return false}
    let rest=&suffix[unit.len_utf8()..];
    !rest.chars().next().is_some_and(|c|c.is_ascii_alphanumeric())
}
fn mapped(w: &Word) -> Option<String> {
    // Only the CFF-verified numeric micro-unit suffix is covered by this pass.
    if w.raw.char_indices().any(|(i,c)| c=='\u{E24F}' && !numeric_measurement(&w.raw,i)) {return None}
    if w.raw.chars().any(|c| {
        if !is_private_use(c) || marker(c).is_some() {return false}
        if c as u32 >= 0xF0000 {return true}
        let normalized=crate::normalize::word_text(&c.to_string());
        normalized.is_empty() || normalized.chars().any(is_private_use)
    }) {return None}
    let (mut old, mut new, mut part)=(String::new(),String::new(),String::new());
    for c in w.raw.chars() {
        if let Some(m)=marker(c) {
            let n=crate::normalize::word_text(&part);old.push_str(&n);new.push_str(&n);new.push_str(m);part.clear();
        } else {part.push(c)}
    }
    let n=crate::normalize::word_text(&part);old.push_str(&n);new.push_str(&n);
    (!w.text.is_empty() && old==w.text && new!=w.text).then_some(new)
}

fn all_lines(units: &[Unit]) -> Vec<&Line> {
    let mut out = vec![];
    for u in units {
        match u {
            Unit::Plain(p) => out.extend(p),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            } => {
                out.extend(preamble);
                for p in after.iter().chain(before).chain(outside) {
                    out.extend(p);
                }
            }
        }
    }
    out
}
fn all_lines_mut(units: &mut [Unit]) -> Vec<&mut Line> {
    let mut out = vec![];
    for u in units {
        match u {
            Unit::Plain(p) => out.extend(p),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            } => {
                out.extend(preamble);
                for p in after.iter_mut().chain(before).chain(outside) {
                    out.extend(p);
                }
            }
        }
    }
    out
}
#[derive(Clone)]
struct Owner {
    line: usize,
    cell: Option<usize>,
    text: String,
    editable: bool,
}
struct Edit {
    owner: usize,
    start: usize,
    end: usize,
    text: String,
    id: usize,
}
fn unique(text: &str, part: &str) -> Option<(usize, usize)> {
    let mut matches = text.match_indices(part);
    let (s, _) = matches.next()?;
    matches.next().is_none().then_some((s, s + part.len()))
}
pub(crate) fn restore(units: &mut [Unit], words: &mut [Word]) {
    let lines = all_lines(units);
    let mut owners = vec![];
    let mut attached: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (li, l) in lines.iter().enumerate() {
        if !l.words.is_empty() {
            let oi = owners.len();
            for &id in &l.words {
                attached.entry(id).or_default().push(oi);
            }
            owners.push(Owner {
                line: li,
                cell: None,
                text: l.text.clone(),
                editable: l.table.is_none(),
            });
        }
        if let Some(t) = &l.table {
            for (ci, c) in t.cells.iter().enumerate() {
                let oi = owners.len();
                for &id in &c.words {
                    attached.entry(id).or_default().push(oi);
                }
                owners.push(Owner {
                    line: li,
                    cell: Some(ci),
                    text: c.text.clone(),
                    editable: true,
                });
            }
        }
    }
    let mut edits = vec![];
    for w in words
        .iter()
        .filter(|w| w.status == WordStatus::Used && w.raw.chars().any(|c| marker(c).is_some()))
    {
        let Some(new) = mapped(w) else { continue };
        let Some(os) = attached.get(&w.id) else {
            continue;
        };
        if os.len() != 1 {
            continue;
        };
        let oi = os[0];
        let o = &owners[oi];
        if !o.editable {
            continue;
        };
        if let Some((start, end)) = unique(&o.text, &w.text) {
            edits.push(Edit {
                owner: oi,
                start,
                end,
                text: new,
                id: w.id,
            });
        }
    }
    let mut conflicts = BTreeSet::new();
    for (i, a) in edits.iter().enumerate() {
        for b in &edits[i + 1..] {
            if a.owner == b.owner && a.start < b.end && b.start < a.end {
                conflicts.insert(a.owner);
            }
        }
    }
    drop(lines);
    let mut lines = all_lines_mut(units);
    let mut by_owner: BTreeMap<usize, Vec<Edit>> = BTreeMap::new();
    for e in edits {
        if !conflicts.contains(&e.owner) {
            by_owner.entry(e.owner).or_default().push(e);
        }
    }
    let mut tables = BTreeSet::new();
    for (oi, mut es) in by_owner {
        es.sort_by_key(|e| std::cmp::Reverse(e.start));
        let o = &owners[oi];
        let mut text = o.text.clone();
        for e in es {
            text.replace_range(e.start..e.end, &e.text);
            words[e.id].text = e.text;
        }
        let l = &mut lines[o.line];
        if let Some(ci) = o.cell {
            l.table.as_mut().unwrap().cells[ci].text = text;
            tables.insert(o.line);
        } else {
            l.text = text;
        }
    }
    for li in tables {
        lines[li].text = lines[li].table.as_ref().unwrap().render();
    }
}
#[cfg(test)]
#[path="micro_prefixes_tests.rs"] mod tests;
