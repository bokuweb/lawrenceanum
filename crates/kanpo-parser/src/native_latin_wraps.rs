//! Restore Latin word spaces at physical line wraps inside table cells.
//! A wrap alone does not prove a word boundary: narrow cells may break a word
//! anywhere. A table whose own original glyphs break words with a hyphen
//! ("Diffu‑" + "sion") proves its composition hyphenates divided words, so its
//! unhyphenated wraps between two Latin letters are word boundaries.
use crate::{
    native_word_axes::NativeAxis,
    trace::{Line, NativeBounds, Table, TableCell, Unit, Word, WordStatus},
};
use std::collections::BTreeMap;
type Rect = [f32; 4];
fn valid(b: Rect) -> bool {
    b.iter().all(|n| n.is_finite()) && b[0] < b[2] && b[1] < b[3]
}
fn material(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
struct Source<'a> {
    words: &'a [Word],
    bounds: &'a [NativeBounds],
    axes: &'a [NativeAxis],
    counts: BTreeMap<usize, usize>,
}
impl Source<'_> {
    fn word(&self, id: usize) -> Option<(&str, Rect, NativeAxis)> {
        let w = self.words.get(id)?;
        if w.id != id || w.status != WordStatus::Used || self.counts.get(&id) != Some(&1) {
            return None;
        }
        let NativeBounds::Glyphs(b) = *self.bounds.get(id)? else {
            return None;
        };
        let axis = *self.axes.get(id)?;
        (valid(b) && matches!(axis, NativeAxis::Right | NativeAxis::Down))
            .then_some((w.text.as_str(), b, axis))
    }
    /// The next original Word starts the following physical line of the cell:
    /// its centre lies beyond the far edge of a horizontal Word and it starts
    /// no further right, or beyond the left edge of a vertical Word and it
    /// starts no lower. Tight leading may overlap the glyph boxes of adjacent lines.
    fn wraps(&self, a: usize, b: usize) -> Option<bool> {
        let (_, p, pa) = self.word(a)?;
        let (_, n, na) = self.word(b)?;
        if pa != na {
            return Some(false);
        }
        Some(match pa {
            NativeAxis::Right => (n[1] + n[3]) / 2. > p[3] && n[0] <= p[0],
            _ => (n[0] + n[2]) / 2. < p[0] && n[1] <= p[1],
        })
    }
    /// Junctions (byte offsets into the cell text) between consecutive Words.
    fn junctions(&self, cell: &TableCell) -> Option<Vec<usize>> {
        let mut at = 0;
        let mut out = vec![];
        for (i, &id) in cell.words.iter().enumerate() {
            let (text, _, _) = self.word(id)?;
            for ch in material(text).chars() {
                let rest = &cell.text[at..];
                let skip = rest.len() - rest.trim_start().len();
                at += skip;
                if !cell.text[at..].starts_with(ch) {
                    return None;
                }
                at += ch.len_utf8();
            }
            if i + 1 < cell.words.len() {
                out.push(at);
            }
        }
        cell.text[at..].trim().is_empty().then_some(out)
    }
}
fn latin(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_ascii_alphabetic())
}
fn lower(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_ascii_lowercase())
}
fn hyphen(c: char) -> bool {
    matches!(c, '-' | '‐' | '‑')
}
/// A divided word: a lowercase letter and a hyphen end the line, and the next
/// line continues with a lowercase letter.
fn hyphenated(s: &Source, table: &Table) -> bool {
    table.cells.iter().any(|cell| {
        cell.words.windows(2).any(|w| {
            let (Some((a, _, _)), Some((b, _, _))) = (s.word(w[0]), s.word(w[1])) else {
                return false;
            };
            let mut tail = a.chars().rev();
            tail.next().is_some_and(hyphen)
                && lower(tail.next())
                && lower(b.chars().next())
                && s.wraps(w[0], w[1]) == Some(true)
        })
    })
}
fn repair(s: &Source, cell: &TableCell) -> Option<String> {
    let junctions = s.junctions(cell)?;
    let mut text = cell.text.clone();
    for (i, &at) in junctions.iter().enumerate().rev() {
        let (a, b) = (cell.words[i], cell.words[i + 1]);
        let before = cell.text[..at].chars().last();
        let after = cell.text[at..].chars().next();
        // As within a printed line, punctuation that ends a Latin word also
        // precedes a word space ("S.A." + "Camino", "Co.," + "Ltd.").
        let ended = latin(before) || before.is_some_and(|c| ".,;:".contains(c));
        if ended && latin(after) && s.wraps(a, b)? {
            text.insert(at, ' ');
        }
    }
    (text != cell.text).then_some(text)
}
pub(crate) fn restore(units: &mut [Unit], words: &[Word], bounds: &[NativeBounds], axes: &[NativeAxis]) {
    if bounds.len() != words.len() || axes.len() != words.len() {
        return;
    }
    let mut counts = BTreeMap::new();
    let mut count = |lines: &[Line]| {
        for line in lines {
            for id in &line.words {
                *counts.entry(*id).or_default() += 1;
            }
            if let Some(t) = &line.table {
                for id in t.cells.iter().flat_map(|c| &c.words) {
                    *counts.entry(*id).or_default() += 1;
                }
            }
        }
    };
    for unit in units.iter() {
        match unit {
            Unit::Plain(lines) => count(lines),
            Unit::Shinkyu { preamble, after, before, outside, .. } => {
                count(preamble);
                for lines in after.iter().chain(before).chain(outside) {
                    count(lines);
                }
            }
        }
    }
    let s = Source { words, bounds, axes, counts };
    let rewrite = |lines: &mut [Line]| {
        for line in lines {
            let Some(table) = &line.table else { continue };
            if line.text != table.render() || !hyphenated(&s, table) {
                continue;
            }
            let replacements: Vec<_> = table
                .cells
                .iter()
                .enumerate()
                .filter_map(|(i, c)| repair(&s, c).map(|t| (i, t)))
                .collect();
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
            Unit::Shinkyu { preamble, after, before, outside, .. } => {
                rewrite(preamble);
                for lines in after.iter_mut().chain(before).chain(outside) {
                    rewrite(lines);
                }
            }
        }
    }
}
#[cfg(test)]
#[path = "native_latin_wrap_tests.rs"]
mod tests;
