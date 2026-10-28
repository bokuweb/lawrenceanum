//! Slice an existing plain line only at its complete, ordered Word boundaries.
//! Field geometry is proved by the caller; no text selects a field.
use crate::trace::{Line, Word, WordStatus};
use std::collections::BTreeSet;

pub(super) fn partition(line: &Line, words: &[Word], sides: &[usize]) -> Option<[Option<Line>; 2]> {
    if line.table.is_some()
        || sides.len() != line.words.len()
        || sides.is_empty()
        || sides.iter().any(|&s| s > 1)
    {
        return None;
    }
    let first = sides[0];
    let split = sides.iter().position(|&s| s != first)?;
    if sides[split..].iter().any(|&s| s == first) {
        return None;
    }
    let chars = line
        .text
        .char_indices()
        .filter(|(_, c)| !c.is_whitespace())
        .collect::<Vec<_>>();
    let mut spans = Vec::new();
    let mut offset: usize = 0;
    let mut seen = BTreeSet::new();
    for &id in &line.words {
        let word = words.get(id)?;
        if word.id != id
            || word.status != WordStatus::Used
            || !seen.insert(id)
            || word.text.trim() != word.text
        {
            return None;
        }
        let source = word
            .text
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<Vec<_>>();
        if source.is_empty() {
            return None;
        }
        let end = offset.checked_add(source.len())?;
        let own = chars.get(offset..end)?;
        if !own.iter().map(|(_, c)| *c).eq(source) {
            return None;
        }
        let &(start, _) = own.first()?;
        let &(last, c) = own.last()?;
        spans.push((start, last + c.len_utf8()));
        offset = end;
    }
    if offset != chars.len() {
        return None;
    }
    // Preserve both original fragments and all internal whitespace exactly.
    // Only the inter-field whitespace seam leaves the shared plain line.
    let mut out: [Option<Line>; 2] = Default::default();
    out[first] = Some(Line::text(
        line.text[..spans[split - 1].1].to_owned(),
        line.words[..split].to_vec(),
    ));
    out[1 - first] = Some(Line::text(
        line.text[spans[split].0..].to_owned(),
        line.words[split..].to_vec(),
    ));
    Some(out)
}

#[cfg(test)]
#[path = "native_line_field_span_tests.rs"]
mod tests;
