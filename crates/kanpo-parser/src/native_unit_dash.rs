//! Restore an original no-value mark in a source-proven quantity slot, after layout.
use crate::lines::PageRules;
use crate::native_word_axes::NativeAxis;
use crate::trace::{Line, NativeBounds, NativeStyles, Unit, Word, WordStatus};
use std::collections::{BTreeSet, HashMap};

fn numeric_piece(s: &str) -> bool {
    !s.is_empty()
        && s.chars().all(|c| {
            c.is_ascii_digit() || ('０'..='９').contains(&c) || ",.，．△▲−－-()（）".contains(c)
        })
}

fn quantity_unit(s: &str) -> Option<&str> {
    let at = s
        .char_indices()
        .find(|(_, c)| !numeric_piece(&c.to_string()))?
        .0;
    let (number, unit) = s.split_at(at);
    (numeric_piece(number)
        && number
            .chars()
            .any(|c| c.is_ascii_digit() || ('０'..='９').contains(&c))
        && !unit.is_empty()
        && unit.chars().all(char::is_alphabetic))
    .then_some(unit)
}

fn bound(id: usize, native: &[NativeBounds]) -> Option<[f32; 4]> {
    match native.get(id)? {
        NativeBounds::Glyphs(b)
            if b.iter().all(|v| v.is_finite()) && b[0] < b[2] && b[1] < b[3] =>
        {
            Some(*b)
        }
        _ => None,
    }
}

fn style(id: usize, styles: &[Option<NativeStyles>]) -> Option<&NativeStyles> {
    styles.get(id)?.as_ref().filter(|s| {
        !s.is_empty()
            && s.iter()
                .all(|(_, size)| f64::from_bits(*size).is_finite() && f64::from_bits(*size) > 0.)
    })
}

fn unit_offset(line: &Line, words: &[Word]) -> Option<usize> {
    if line.table.is_some() {
        return None;
    }
    let text = &words.get(*line.words.last()?)?.text;
    if text.is_empty() {
        return None;
    }
    // Only the mapped source unit is the target. Keep corrected unrelated label
    // bytes and trailing whitespace; no Line is rebuilt from source Word text.
    line.text
        .trim_end_matches(char::is_whitespace)
        .strip_suffix(text.as_str())
        .map(str::len)
}

fn compact(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

fn crosses_rule(b: [f32; 4], rules: &PageRules) -> bool {
    rules
        .vertical
        .iter()
        .any(|v| b[0] < v.x && v.x < b[2] && b[1].max(v.y0) < b[3].min(v.y1))
        || rules
            .horizontal
            .iter()
            .any(|h| b[1] < h.y && h.y < b[3] && b[0].max(h.x0) < b[2].min(h.x1))
}

// A complete, already owned numeric tail supplies the unit, horizontal extent,
// and numeric source fonts. Unit names and the number of sibling rows are not
// hardcoded. A terminal source font can differ from the punctuation/sign font.
struct Witness<'a> {
    unit: &'a str,
    right: f32,
    left: f32,
    top: f32,
    bottom: f32,
    unit_style: &'a NativeStyles,
    numeric_styles: NativeStyles,
}

fn witness<'a>(
    line: &'a Line,
    words: &'a [Word],
    native: &[NativeBounds],
    axes: &[NativeAxis],
    styles: &'a [Option<NativeStyles>],
    owners: &[usize],
) -> Option<Witness<'a>> {
    if line.table.is_some() {
        return None;
    }
    let &last = line.words.last()?;
    let unit = quantity_unit(&words[last].text)?;
    let terminal = bound(last, native)?;
    let unit_style = style(last, styles)?;
    let mut left = terminal[0];
    let mut numeric_styles = BTreeSet::new();
    let mut tail_ids = BTreeSet::new();
    let mut tail_order = Vec::new();
    let mut previous_left = terminal[0];
    for (position, &id) in line.words.iter().enumerate().rev() {
        if position + 1 != line.words.len() && !numeric_piece(&words[id].text) {
            break;
        }
        let b = bound(id, native)?;
        if axes.get(id) != Some(&NativeAxis::Right)
            || words[id].status != WordStatus::Used
            || owners[id] != 1
            || b[1] != terminal[1]
            || b[3] != terminal[3]
            || b[2] > terminal[2]
            || b[0] > previous_left
        {
            return None;
        }
        left = left.min(b[0]);
        previous_left = b[0];
        tail_ids.insert(id);
        tail_order.push(id);
        numeric_styles.extend(style(id, styles)?.iter().copied());
    }
    tail_order.reverse();
    let expected: String = tail_order
        .iter()
        .flat_map(|&id| words[id].text.chars())
        .filter(|c| !c.is_whitespace())
        .collect();
    if !compact(&line.text).ends_with(&expected) {
        return None;
    }
    // A foreign source occurrence intersecting the numeric field is negative
    // evidence, even if legacy text reconstruction omitted or misassigned it.
    if words.iter().any(|other| {
        if tail_ids.contains(&other.id) {
            return false;
        }
        let Some(b) = bound(other.id, native) else {
            return true;
        };
        b[0].max(left) < b[2].min(terminal[2]) && b[1].max(terminal[1]) < b[3].min(terminal[3])
    }) {
        return None;
    }
    Some(Witness {
        unit,
        right: terminal[2],
        left,
        top: terminal[1],
        bottom: terminal[3],
        unit_style,
        numeric_styles,
    })
}

pub(crate) fn restore(
    units: &mut [Unit],
    words: &mut [Word],
    native: &[NativeBounds],
    axes: &[NativeAxis],
    styles: &[Option<NativeStyles>],
    rules: Option<&PageRules>,
) {
    let Some(rules) = rules else { return };
    if rules
        .horizontal
        .iter()
        .any(|h| ![h.y, h.x0, h.x1].iter().all(|v| v.is_finite()) || h.x0 >= h.x1)
        || rules
            .vertical
            .iter()
            .any(|v| ![v.x, v.y0, v.y1].iter().all(|v| v.is_finite()) || v.y0 >= v.y1)
    {
        return;
    }
    if !words
        .iter()
        .any(|w| w.status == WordStatus::Empty && w.raw == "\u{E61C}")
    {
        return;
    }
    if words.len() != native.len()
        || words.len() != axes.len()
        || words.len() != styles.len()
        || words
            .iter()
            .enumerate()
            .any(|(id, w)| w.id != id || bound(id, native).is_none())
    {
        return;
    }
    let mut owners = vec![0; words.len()];
    let mut valid = true;
    let mut count = |line: &Line| {
        for id in line.words.iter().copied().chain(
            line.table
                .iter()
                .flat_map(|t| t.cells.iter().flat_map(|c| c.words.iter().copied())),
        ) {
            if let Some(value) = owners.get_mut(id) {
                *value += 1;
            } else {
                valid = false;
            }
        }
    };
    for unit in units.iter() {
        match unit {
            Unit::Plain(ls) => ls.iter().for_each(&mut count),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            } => preamble
                .iter()
                .chain(after.iter().flatten())
                .chain(before.iter().flatten())
                .chain(outside.iter().flatten())
                .for_each(&mut count),
        }
    }
    if !valid {
        return;
    }
    let mut proposals = Vec::new();
    for (u, unit) in units.iter().enumerate() {
        let Unit::Plain(lines) = unit else { continue };
        let witnesses: Vec<_> = lines
            .iter()
            .filter_map(|line| witness(line, words, native, axes, styles, &owners))
            .filter(|w| !crosses_rule([w.left, w.top, w.right, w.bottom], rules))
            .collect();
        for (id, dash) in words.iter().enumerate() {
            if dash.raw != "\u{E61C}"
                || dash.status != WordStatus::Empty
                || !dash.text.is_empty()
                || owners[id] != 0
                || axes[id] != NativeAxis::Right
            {
                continue;
            }
            let d = bound(id, native).unwrap();
            let Some(dash_style) = style(id, styles) else {
                continue;
            };
            let mut hits = Vec::new();
            for (i, line) in lines.iter().enumerate() {
                let Some(offset) = unit_offset(line, words) else {
                    continue;
                };
                let Some(&unit_id) = line.words.last() else {
                    continue;
                };
                let w = &words[unit_id];
                let b = bound(unit_id, native).unwrap();
                if w.text.is_empty()
                    || !w.text.chars().all(char::is_alphabetic)
                    || w.raw != w.text
                    || w.status != WordStatus::Used
                    || owners[unit_id] != 1
                    || axes[unit_id] != NativeAxis::Right
                    || d[1] != b[1]
                    || d[3] != b[3]
                    || d[2] > b[0]
                {
                    continue;
                }
                let Some(unit_style) = style(unit_id, styles) else {
                    continue;
                };
                let corridor = [d[0], d[1], b[2], b[3]];
                if crosses_rule(corridor, rules)
                    || words.iter().any(|other| {
                        if other.id == id || other.id == unit_id {
                            return false;
                        }
                        let o = bound(other.id, native).unwrap();
                        o[0].max(d[0]) < o[2].min(b[0]) && o[1].max(d[1]) < o[3].min(d[3])
                    })
                {
                    continue;
                }
                if !witnesses.iter().any(|source| {
                    source.unit == w.text
                        && source.right == b[2]
                        && source.unit_style == unit_style
                        && source.left <= d[0]
                        && dash_style.is_subset(&source.numeric_styles)
                        && !rules.horizontal.iter().any(|h| {
                            let (a, z) = if source.bottom <= d[1] {
                                (source.bottom, d[1])
                            } else if d[3] <= source.top {
                                (d[3], source.top)
                            } else {
                                return false;
                            };
                            a < h.y && h.y < z && source.left.max(h.x0) < b[2].min(h.x1)
                        })
                }) {
                    continue;
                }
                hits.push((i, unit_id, offset));
            }
            if let [hit] = hits.as_slice() {
                proposals.push((id, u, hit.0, hit.1, hit.2));
            }
        }
    }
    let counts: HashMap<_, usize> = proposals.iter().fold(HashMap::new(), |mut out, p| {
        *out.entry((p.1, p.2, p.3)).or_default() += 1;
        out
    });
    let source_counts: HashMap<usize, usize> =
        proposals.iter().fold(HashMap::new(), |mut out, p| {
            *out.entry(p.0).or_default() += 1;
            out
        });
    for (id, u, line, unit_id, offset) in proposals {
        if counts[&(u, line, unit_id)] != 1 || source_counts[&id] != 1 {
            continue;
        }
        let Unit::Plain(lines) = &mut units[u] else {
            unreachable!()
        };
        let l = &mut lines[line];
        let position = l.words.iter().position(|&i| i == unit_id).unwrap();
        l.text.insert(offset, '－');
        l.words.insert(position, id);
        words[id].text = "－".into();
        words[id].status = WordStatus::Used;
    }
}

#[cfg(test)]
#[path = "native_unit_dash_tests.rs"]
mod tests;
