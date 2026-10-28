//! Reconstruct cell whitespace from the original horizontal glyph flow. Cell
//! membership and public Word records are unchanged. Source paint order must
//! agree with physical baseline order; uncertain cells keep their old text.
use crate::{
    native_cell_origin::Origin,
    trace::{Line, NativeBounds, TableCell, Unit, Word, WordStatus},
};
use std::collections::{BTreeMap, BTreeSet};
fn material(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
// Recover native same-baseline gaps for Unicode line-break classes AL (ordinary
// alphabetic/symbol characters), HL (Hebrew letters), and NU (numbers).
// Physical line wrapping supplies no proof of a word boundary. Keep all old
// whitespace and all old nonwhitespace order, including logical CJK wrapping.
fn source_gaps(old: &str, native: &str) -> Option<String> {
    use icu_properties::{props::LineBreak, CodePointMapData};
    if material(old) != material(native) {
        return None;
    }
    let letters = CodePointMapData::<LineBreak>::new();
    let eligible = |c: char| {
        matches!(
            letters.get(c),
            LineBreak::Alphabetic | LineBreak::HebrewLetter | LineBreak::Numeric
        )
    };
    let chars = material(old).chars().collect::<Vec<_>>();
    let mut gaps = BTreeSet::new();
    let mut offset = 0;
    for c in native.chars() {
        if c.is_whitespace() {
            if c == ' '
                && offset > 0
                && offset < chars.len()
                && chars[..offset]
                    .iter()
                    .rev()
                    .find(|&&c| letters.get(c) != LineBreak::CombiningMark)
                    .is_some_and(|&c| eligible(c))
                && eligible(chars[offset])
            {
                gaps.insert(offset);
            }
        } else {
            offset += 1;
        }
    }
    let mut text = String::new();
    let mut offset = 0;
    let mut whitespace = false;
    for c in old.chars() {
        if c.is_whitespace() {
            whitespace = true;
        } else {
            if gaps.contains(&offset) && !whitespace {
                text.push(' ');
            }
            offset += 1;
            whitespace = false;
        }
        text.push(c);
    }
    Some(text)
}
fn valid(b: [f32; 4]) -> bool {
    b.iter().all(|n| n.is_finite()) && b[0] < b[2] && b[1] < b[3]
}
fn visit(units: &[Unit], mut f: impl FnMut(&[Line])) {
    for unit in units {
        match unit {
            Unit::Plain(lines) => f(lines),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            ..} => {
                f(preamble);
                for lines in after.iter().chain(before).chain(outside) {
                    f(lines)
                }
            }
        }
    }
}
fn repair(
    cell: &TableCell,
    words: &[Word],
    bounds: &[NativeBounds],
    flows: &[Option<Origin>],
    counts: &BTreeMap<usize, usize>,
) -> Option<String> {
    let outer = [cell.x0, cell.y0, cell.x1, cell.y1];
    if !valid(outer) || cell.words.is_empty() {
        return None;
    }
    let mut seen = BTreeSet::new();
    let mut source = vec![];
    for &id in &cell.words {
        if !seen.insert(id) || counts.get(&id) != Some(&1) {
            return None;
        }
        let w = words.get(id)?;
        if w.id != id || w.status != WordStatus::Used || w.text.trim().is_empty() {
            return None;
        }
        let NativeBounds::Glyphs(b) = *bounds.get(id)? else {
            return None;
        };
        if !valid(b) || outer[0] > b[0] || outer[1] > b[1] || b[2] > outer[2] || b[3] > outer[3] {
            return None;
        }
        let origin = flows.get(id)?.as_ref()?;
        if origin.legacy != w.source_box {
            return None;
        }
        let (flow, range) = origin.horizontal_source(b)?;
        let actual = origin.bounds;
        if outer[0] > actual[0]
            || outer[1] > actual[1]
            || actual[2] > outer[2]
            || actual[3] > outer[3]
        {
            return None;
        }
        source.push((id, flow, range, origin.sizes));
    }
    source.sort_by(|a, b| {
        a.1.baseline
            .total_cmp(&b.1.baseline)
            .then(a.1.start.total_cmp(&b.1.start))
    });
    let mut text = String::new();
    for (index, &(id, flow, range, sizes)) in source.iter().enumerate() {
        if index > 0 {
            let (_, prev, pr, previous_sizes) = source[index - 1];
            if pr.2 != range.2 || pr.1 != range.0 {
                return None;
            }
            if prev.baseline == flow.baseline {
                if prev.start >= flow.start
                    || prev.end >= flow.end
                    || (prev.end + prev.trailing + flow.prefix) as f32 != flow.start as f32
                {
                    return None;
                }
                // A native Word can also split solely because its font size changes.
                // Compare the original adjoining glyph sizes exactly; their
                // first-glyph advances need not match (letters have different widths).
                if sizes[0] == previous_sizes[1] && flow.prefix > 0. && flow.start > prev.end {
                    text.push(' ');
                }
            } else {
                text.push('\n');
            }
        }
        text.push_str(&words[id].text);
    }
    source_gaps(&cell.text, &text)
}
pub(crate) fn restore(
    units: &mut [Unit],
    words: &[Word],
    bounds: &[NativeBounds],
    flows: &[Option<Origin>],
) {
    if bounds.len() != words.len() || flows.len() != words.len() {
        return;
    }
    let mut counts = BTreeMap::new();
    visit(units, |lines| {
        for line in lines {
            for id in &line.words {
                *counts.entry(*id).or_default() += 1;
            }
            if let Some(t) = &line.table {
                for cell in &t.cells {
                    for id in &cell.words {
                        *counts.entry(*id).or_default() += 1;
                    }
                }
            }
        }
    });
    let rewrite = |lines: &mut [Line]| {
        for line in lines {
            let Some(table) = &line.table else {
                continue;
            };
            if table.rows == 0
                || table.cols == 0
                || table
                    .cells
                    .iter()
                    .any(|c| c.row >= table.rows || c.col >= table.cols)
                || line.text != table.render()
            {
                continue;
            }
            let replacements = table
                .cells
                .iter()
                .enumerate()
                .filter_map(|(i, c)| {
                    repair(c, words, bounds, flows, &counts)
                        .filter(|t| *t != c.text)
                        .map(|text| (i, text))
                })
                .collect::<Vec<_>>();
            if replacements.is_empty() {
                continue;
            }
            let table = line.table.as_mut().unwrap();
            for (i, text) in replacements {
                table.cells[i].text = text;
            }
            line.text = table.render();
        }
    };
    for unit in units {
        match unit {
            Unit::Plain(lines) => rewrite(lines),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            ..} => {
                rewrite(preamble);
                for lines in after.iter_mut().chain(before).chain(outside) {
                    rewrite(lines)
                }
            }
        }
    }
}
#[cfg(test)]
#[path = "native_cell_text_tests.rs"]
mod tests;
