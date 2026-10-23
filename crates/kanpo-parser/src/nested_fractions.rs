//! One conservatively certified nested fraction in an existing horizontal table
//! cell. Glyph components and real bars certify grouping; words and IDs remain
//! immutable. This is not a general mathematical expression parser.
use crate::nested_fraction_geometry::{self, Atom, Bar};
use crate::trace::{Line, Unit, Word, WordStatus};
use std::collections::HashMap;

pub(crate) fn restore(units: &mut [Unit], words: &[Word], hrules: &[(f32, f32, f32)]) {
    if hrules.len() < 2
        || !words.iter().any(|w| w.raw == "\u{eae9}\u{eaea}")
        || !words.iter().any(|w| w.raw == "\u{eaeb}\u{eaec}")
    {
        return;
    }
    let mut owners: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut next = 0;
    for u in units.iter() {
        match u {
            Unit::Plain(lines) => record_owners(lines, &mut owners, &mut next),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            } => {
                record_owners(preamble, &mut owners, &mut next);
                for lines in after.iter().chain(before.iter()).chain(outside.iter()) {
                    record_owners(lines, &mut owners, &mut next);
                }
            }
        }
    }
    let atoms: Vec<_> = words
        .iter()
        .map(|w| Atom {
            id: w.id,
            x0: w.x0,
            y0: w.y0,
            x1: w.x1,
            y1: w.y1,
            raw: w.raw.clone(),
            text: w.text.clone(),
            owner: owners
                .get(&w.id)
                .and_then(|o| (o.len() == 1).then_some(o[0])),
            eligible: matches!(w.status, WordStatus::Used | WordStatus::Empty),
        })
        .collect();
    let bars: Vec<_> = hrules
        .iter()
        .map(|&(y, x0, x1)| Bar { y, x0, x1 })
        .collect();
    let formulas = nested_fraction_geometry::recognize(&atoms, &bars);
    if formulas.is_empty() {
        return;
    }
    // The same stable owner enumeration is used for mutation; Shinkyu owners
    // participate in ambiguity detection but their text is never modified.
    next = 0;
    for u in units {
        match u {
            Unit::Plain(lines) => {
                for line in lines {
                    if let Some(table) = &mut line.table {
                        let mut changed = false;
                        for cell in &mut table.cells {
                            for f in formulas.iter().filter(|f| f.owner == next) {
                                if f.source_ids
                                    .iter()
                                    .all(|id| cell.words.iter().filter(|x| *x == id).count() == 1)
                                {
                                    changed |= replace_unique_compact(
                                        &mut cell.text,
                                        &f.original,
                                        &f.text,
                                    );
                                }
                            }
                            next += 1;
                        }
                        if changed {
                            line.text = table.render();
                        }
                    } else {
                        next += 1;
                    }
                }
            }
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            } => {
                next += owner_count(preamble);
                for lines in after.iter().chain(before.iter()).chain(outside.iter()) {
                    next += owner_count(lines);
                }
            }
        }
    }
}
fn record_owners(lines: &[Line], owners: &mut HashMap<usize, Vec<usize>>, next: &mut usize) {
    for line in lines {
        if let Some(t) = &line.table {
            for c in &t.cells {
                for &id in &c.words {
                    owners.entry(id).or_default().push(*next);
                }
                *next += 1;
            }
        } else {
            for &id in &line.words {
                owners.entry(id).or_default().push(*next);
            }
            *next += 1;
        }
    }
}
fn owner_count(lines: &[Line]) -> usize {
    lines
        .iter()
        .map(|l| l.table.as_ref().map_or(1, |t| t.cells.len()))
        .sum()
}
fn replace_unique_compact(text: &mut String, original: &str, replacement: &str) -> bool {
    if original.is_empty() {
        return false;
    }
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let Some(start) = compact.find(original) else {
        return false;
    };
    if compact.rfind(original) != Some(start) {
        return false;
    }
    let end = start + original.len();
    let (mut offset, mut from, mut to) = (0, None, None);
    for (at, ch) in text.char_indices().filter(|(_, c)| !c.is_whitespace()) {
        if offset == start {
            from = Some(at)
        }
        offset += ch.len_utf8();
        if offset == end {
            to = Some(at + ch.len_utf8());
            break;
        }
    }
    if let (Some(from), Some(to)) = (from, to) {
        text.replace_range(from..to, replacement);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unique_replacement_keeps_all_surrounding_prose() {
        let mut s = "前文\n甲　乙\n後文".to_string();
        assert!(replace_unique_compact(&mut s, "甲乙", "(甲/乙)"));
        assert_eq!(s, "前文\n(甲/乙)\n後文");
    }
    #[test]
    fn repeated_matching_text_is_ambiguous() {
        let mut s = "甲 乙\n甲乙".to_string();
        let old = s.clone();
        assert!(!replace_unique_compact(&mut s, "甲乙", "(甲/乙)"));
        assert_eq!(s, old);
    }
    #[test]
    fn noncontiguous_source_is_not_removed() {
        let mut s = "甲注乙".to_string();
        assert!(!replace_unique_compact(&mut s, "甲乙", "(甲/乙)"));
        assert_eq!(s, "甲注乙");
    }
}

#[cfg(test)]
#[path = "nested_fractions_tests.rs"]
mod integration_tests;
