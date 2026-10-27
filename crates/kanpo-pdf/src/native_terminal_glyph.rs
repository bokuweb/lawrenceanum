//! Exact source last-glyph advances; no inferred equal-width or point tolerance.
use super::*;
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Terminal {
    pub first: usize,
    pub after: usize,
    pub start: f64,
    pub end: f64,
    pub baseline: f64,
}
impl Terminal {
    pub(super) fn attribute(self) -> String {
        format!(
            "1 {} {} {} {} {}",
            self.first, self.after, self.start, self.end, self.baseline
        )
    }
}
pub(super) fn from_word(page: &Page, word: &Word) -> Option<Terminal> {
    let first = *word.glyphs.first()?;
    let after = word.glyphs.last()?.checked_add(1)?;
    if first >= after
        || after > page.glyphs.len()
        || !word
            .glyphs
            .iter()
            .enumerate()
            .all(|(i, &id)| first.checked_add(i) == Some(id))
        || page.glyphs[first..after].iter().any(|g| g.vertical)
    {
        return None;
    }
    let [start, baseline, end, _] = native_flow_tuple(page, word)?;
    let last = page.glyphs.get(after - 1)?;
    if ![last.x, last.end_x, last.y, last.end_y]
        .iter()
        .all(|x| x.is_finite())
        || last.x < start
        || last.x >= last.end_x
        || last.end_x != end
        || last.y != baseline
        || last.end_y != baseline
    {
        return None;
    }
    Some(Terminal {
        first,
        after,
        start: last.x,
        end: last.end_x,
        baseline: last.y,
    })
}
#[cfg(test)]
#[path = "native_terminal_glyph_tests156.rs"]
mod tests;
