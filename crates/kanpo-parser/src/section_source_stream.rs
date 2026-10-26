//! Recover source offsets through the *same* Unit renderer that inserted the
//! structural shinkyu labels. Native text with the same spelling is never
//! searched for. The complete reconstructed page and article streams must be
//! byte-identical, and no original line or whole table can cross article bounds.
use super::*;
use crate::trace::{Line, Region, Unit};

fn units(regions: &[Region]) -> Option<Vec<Unit>> {
    let mut out = vec![];
    let mut at = 0;
    let mut previous = None;
    while at < regions.len() {
        let id = regions[at].unit;
        if previous.is_some_and(|p| p >= id) {
            return None;
        }
        previous = Some(id);
        let start = at;
        while at < regions.len() && regions[at].unit == id {
            at += 1;
        }
        let rs = &regions[start..at];
        if rs[0].role == Role::Body {
            if rs.len() != 1 || rs[0].part != 0 {
                return None;
            }
            out.push(Unit::Plain(rs[0].lines.clone()));
        } else {
            let mut preamble = vec![];
            let mut after = vec![];
            let mut before = vec![];
            let mut outside = vec![];
            let mut last = None;
            for r in rs {
                let rank = match r.role {
                    Role::Preamble => 0,
                    Role::After => 1,
                    Role::Before => 2,
                    Role::Outside => 3,
                    _ => return None,
                };
                if last.is_some_and(|v| v >= (rank, r.part)) || (rank == 0 && r.part != 0) {
                    return None;
                }
                last = Some((rank, r.part));
                match r.role {
                    Role::Preamble => preamble = r.lines.clone(),
                    Role::After => after.push(r.lines.clone()),
                    Role::Before => before.push(r.lines.clone()),
                    Role::Outside => outside.push(r.lines.clone()),
                    _ => return None,
                }
            }
            out.push(Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            });
        }
    }
    Some(out)
}
struct Chunk {
    text: String,
    words: Vec<usize>,
}
struct Span {
    start: usize,
    end: usize,
    words: Vec<Source>,
}
fn line_chunk(
    line: &Line,
    page: &PageTrace,
    table: &mut usize,
    tables: &mut Vec<crate::trace::Table>,
    seen: &mut BTreeSet<usize>,
) -> Option<Chunk> {
    let (text, ids) = if let Some(t) = &line.table {
        if t.rows == 0
            || t.cols == 0
            || t.cells.iter().any(|c| c.row >= t.rows || c.col >= t.cols)
            || !line.words.is_empty()
            || t.render() != line.text
        {
            return None;
        }
        let text = crate::trace::table_marker(*table);
        *table += 1;
        tables.push(t.clone());
        (
            text,
            t.cells
                .iter()
                .flat_map(|c| c.words.iter().copied())
                .collect::<Vec<_>>(),
        )
    } else {
        let actual = line
            .words
            .iter()
            .map(|&id| page.words.get(id).map(|w| w.text.as_str()))
            .collect::<Option<Vec<_>>>()?
            .join("");
        if compact(&actual) != compact(&line.text) {
            return None;
        }
        (line.text.clone(), line.words.clone())
    };
    for &id in &ids {
        let w = page.words.get(id)?;
        if w.id != id || w.status != WordStatus::Used || !seen.insert(id) {
            return None;
        }
    }
    Some(Chunk { text, words: ids })
}
fn page_stream(page: &PageTrace, pi: usize, table: &mut usize) -> Option<(String, Vec<Span>)> {
    let units = units(&page.regions)?;
    let mut text = String::new();
    let mut spans = vec![];
    let mut seen = BTreeSet::new();
    let mut tables = vec![];
    for unit in units {
        let mut chunks = vec![];
        let mut valid_source = true;
        // Only this template is decoded. Native content is expanded once and
        // never interpreted as a template, even if it contains reserved PUA.
        let template = unit.text_with(
            &mut |line| {
                let Some(chunk) = line_chunk(line, page, table, &mut tables, &mut seen) else {
                    valid_source = false;
                    return String::new();
                };
                let core = chunk.text.trim();
                if core.is_empty() {
                    if !chunk.words.is_empty() {
                        valid_source = false;
                    }
                    return chunk.text;
                }
                let leading = chunk.text.len() - chunk.text.trim_start().len();
                let trailing = leading + core.len();
                let token = format!(
                    "{}\u{e003}{}\u{e004}{}",
                    &chunk.text[..leading],
                    chunks.len(),
                    &chunk.text[trailing..]
                );
                chunks.push(Chunk {
                    text: core.into(),
                    words: chunk.words,
                });
                token
            },
            true,
        );
        if !valid_source {
            return None;
        }
        if template.is_empty() {
            continue;
        }
        if !text.is_empty() {
            text.push('\n');
        }
        let mut rest = template.as_str();
        let mut used = BTreeSet::new();
        while let Some(at) = rest.find('\u{e003}') {
            text.push_str(&rest[..at]);
            rest = &rest[at + '\u{e003}'.len_utf8()..];
            let end = rest.find('\u{e004}')?;
            let id = rest[..end].parse::<usize>().ok()?;
            if !used.insert(id) {
                return None;
            }
            let c = chunks.get(id)?;
            let start = text.len();
            text.push_str(&c.text);
            let endtext = text.len();
            spans.push(Span {
                start,
                end: endtext,
                words: c.words.iter().map(|&id| (pi, id)).collect(),
            });
            rest = &rest[end + '\u{e004}'.len_utf8()..];
        }
        text.push_str(rest);
        if used.len() != chunks.len() {
            return None;
        }
    }
    if text != page.doc_text
        || tables != page.tables
        || page
            .words
            .iter()
            .any(|w| w.status == WordStatus::Used && !seen.contains(&w.id))
    {
        return None;
    }
    Some((text, spans))
}
pub(super) fn owners(sources: &[OwnedSources], pages: &[PageTrace]) -> Option<Vec<Vec<Source>>> {
    let mut text = String::new();
    let mut spans = vec![];
    let mut table = 0;
    for (pi, page) in pages.iter().enumerate() {
        let (s, mut rows) = page_stream(page, pi, &mut table)?;
        if s.is_empty() {
            continue;
        }
        if !text.is_empty() {
            text.push('\n');
        }
        let offset = text.len();
        for row in &mut rows {
            row.start += offset;
            row.end += offset;
        }
        text.push_str(&s);
        spans.extend(rows);
    }
    if sources
        .iter()
        .map(|s| s.marked_text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
        != text
    {
        return None;
    }
    let mut result = vec![];
    let mut start = 0;
    let mut rows = spans.into_iter().peekable();
    for source in sources {
        let end = start + source.marked_text.len();
        let mut words = vec![];
        while rows.peek().is_some_and(|row| row.start < end) {
            let row = rows.next()?;
            if row.start < start || row.end > end {
                return None;
            }
            words.extend(row.words);
        }
        if !source.words.is_empty()
            && source.words.iter().copied().collect::<BTreeSet<_>>()
                != words.iter().copied().collect::<BTreeSet<_>>()
        {
            return None;
        }
        result.push(words);
        start = end + 1;
    }
    rows.next().is_none().then_some(result)
}
pub(super) fn restore(sources: &mut [OwnedSources], pages: &[PageTrace]) {
    if let Some(bound) = owners(sources, pages) {
        for (source, words) in sources.iter_mut().zip(bound) {
            if source.words.is_empty() {
                source.words = words;
            }
        }
    }
}

/// Independently recover complete native units on one fully validated page.
/// This does not relax the global owners()/restore() contract. An invalid page
/// cannot certify any of its units, nor poison an unrelated valid source page.
pub(super) struct UnitSource {
    pub(super) text: String,
    pub(super) words: Vec<Source>,
    pub(super) tables: Vec<usize>,
    pub(super) has_structural_anchor: bool,
}
pub(super) fn page_units(
    page: &PageTrace,
    pi: usize,
    first_table: usize,
) -> Option<Vec<UnitSource>> {
    let mut after_table = first_table;
    page_stream(page, pi, &mut after_table)?;
    let mut out = vec![];
    let mut table = first_table;
    for unit in units(&page.regions)? {
        let mut tables = vec![];
        let text = unit.doc_text(&mut tables, table);
        let ids = (table..table + tables.len()).collect();
        table += tables.len();
        let regions = unit.regions(0);
        let has_structural_anchor = regions
            .iter()
            .filter(|r| matches!(r.role, Role::After | Role::Before))
            .flat_map(|r| &r.lines)
            .any(|l| {
                crate::model::outline_heading(&l.text)
                    || crate::model::article_number(&l.text).is_some()
            });
        let words = regions
            .iter()
            .flat_map(|r| &r.lines)
            .flat_map(|l| {
                if let Some(t) = &l.table {
                    t.cells
                        .iter()
                        .flat_map(|c| c.words.clone())
                        .collect::<Vec<_>>()
                } else {
                    l.words.clone()
                }
            })
            .map(|id| (pi, id))
            .collect();
        if !text.is_empty() {
            out.push(UnitSource {
                text,
                words,
                tables: ids,
                has_structural_anchor,
            });
        }
    }
    (table == after_table
        && out
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
            == page.doc_text)
        .then_some(out)
}
