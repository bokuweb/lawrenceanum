//! Source-proven hollow CFF frames enclose text; the drawing glyph is not a
//! second body character. Raw words/glyphs remain in the native audit.
use super::{Document, Field, PaintAudit};
use crate::trace::{PageTrace, Role, Unit, WordStatus};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
type Location = (usize, usize, Option<usize>);

#[derive(Debug, Clone, Serialize)]
pub struct Binding {
    pub source_document_sha256: String,
    pub document: usize,
    pub field: Field,
    pub offset: usize,
    pub length: usize,
    pub text: String,
    pub page: usize,
    pub frame_glyph: usize,
    pub frame_word: usize,
    pub parent_glyphs: Vec<usize>,
    pub parent_words: Vec<usize>,
    pub frame: kanpo_pdf::RectangleFrame,
    pub removed_text: String,
    pub removed_offset: usize,
    pub removed_length: usize,
}
#[derive(Debug, Clone, Serialize)]
pub struct Unresolved {
    pub page: usize,
    pub frame_glyph: usize,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct Audit {
    pub scope: &'static str,
    pub bindings: Vec<Binding>,
    pub unresolved: Vec<Unresolved>,
    pub coverage_verified: bool,
}
#[derive(Clone)]
pub(super) struct Enclosure {
    page: usize,
    glyph: usize,
    word: usize,
    location: Location,
    field: Field,
    remove_word: usize,
    remove_local: usize,
    remove_global: usize,
    remove_length: usize,
    remove_text: String,
    parent_start: usize,
    parent_end: usize,
    parent_text: String,
    parent_glyphs: Vec<usize>,
    parent_words: Vec<usize>,
    frame: kanpo_pdf::RectangleFrame,
}
#[derive(Clone)]
struct Occurrence {
    location: Location,
    field: Field,
    global: usize,
    local: usize,
}
struct Layout {
    streams: [String; 3],
    words: BTreeMap<(usize, usize), Occurrence>,
}
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn index(field: Field) -> usize {
    match field {
        Field::Paragraph => 0,
        Field::After => 1,
        Field::Before => 2,
    }
}
fn role(role: Role) -> Field {
    match role {
        Role::After => Field::After,
        Role::Before => Field::Before,
        _ => Field::Paragraph,
    }
}
fn ordered_cell(page: &PageTrace, cell: &crate::trace::TableCell) -> Option<Vec<usize>> {
    let columns = cell
        .words
        .iter()
        .map(|&id| {
            let w = page.words.get(id)?;
            (w.id == id && w.status == WordStatus::Used).then(|| crate::vertical::Col {
                id,
                x: w.x0,
                x1: w.x1,
                y: w.y0,
                y1: w.y1,
                text: w.text.clone(),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let ids = crate::shinkyu::cell_source_word_order(columns);
    (ids.len() == cell.words.len()
        && ids.iter().copied().collect::<BTreeSet<_>>() == cell.words.iter().copied().collect())
    .then_some(ids)
}
fn layout(base: &crate::Analysis, paint: Option<&PaintAudit>) -> Layout {
    let mut streams: [String; 3] = Default::default();
    let mut words = BTreeMap::new();
    let mut duplicates = BTreeSet::new();
    for (pi, page) in base.pages.iter().enumerate() {
        for (ri, region) in page.regions.iter().enumerate() {
            let field = role(region.role);
            for (li, line) in region.lines.iter().enumerate() {
                let base = streams[index(field)].chars().count();
                let mut add = |location: Location, parts: BTreeMap<usize, usize>, start: usize| {
                    for (word, local) in parts {
                        if words
                            .insert(
                                (pi, word),
                                Occurrence {
                                    location,
                                    field,
                                    global: base + start + local,
                                    local,
                                },
                            )
                            .is_some()
                        {
                            duplicates.insert((pi, word));
                        }
                    }
                };
                if let Some(table) = &line.table {
                    let mut cells = table.cells.iter().enumerate().collect::<Vec<_>>();
                    cells.sort_by_key(|(_, c)| (c.row, c.col));
                    if compact(&line.text)
                        == compact(
                            &cells
                                .iter()
                                .map(|(_, c)| c.text.as_str())
                                .collect::<String>(),
                        )
                    {
                        let mut start = 0;
                        for (ci, cell) in cells {
                            if let Some(ids) = ordered_cell(page, cell) {
                                let mut parts = BTreeMap::new();
                                let mut text = String::new();
                                for id in ids {
                                    parts.insert(id, text.chars().count());
                                    text.push_str(&compact(&page.words[id].text));
                                }
                                if text == compact(&cell.text) {
                                    add((ri, li, Some(ci)), parts, start);
                                }
                            }
                            start += compact(&cell.text).chars().count();
                        }
                    }
                } else if let Some(parts) =
                    super::native_glyphs::line_parts(page, pi, ri, li, paint)
                {
                    add((ri, li, None), parts, 0);
                }
                streams[index(field)].push_str(&compact(&line.text));
            }
        }
    }
    for key in duplicates {
        words.remove(&key);
    }
    Layout { streams, words }
}
#[derive(Deserialize)]
struct PaintScene {
    unknown: Vec<usize>,
    images: Vec<[f64; 4]>,
    fills: Vec<[f64; 4]>,
    lines: Vec<([f64; 4], [f64; 4])>,
}
fn scene(page: &PageTrace, source: &kanpo_pdf::XObjectPageSources) -> Option<PaintScene> {
    let html = kanpo_pdf::bbox::bbox_layout_with_native_styles(std::slice::from_ref(
        source.native_sources(),
    ));
    let parsed = scraper::Html::parse_document(&html);
    let selector = scraper::Selector::parse("page").ok()?;
    let node = parsed.select(&selector).next()?;
    let rules = kanpo_pdf::page_rules(source.page());
    let valid = crate::native_source_scene::for_page(node, &rules)?;
    if !valid.matches(&page.words) || !source.capture_gaps().is_empty() {
        return None;
    }
    serde_json::from_str(node.value().attr("data-native-scene")?).ok()
}
fn segment(
    word: &crate::trace::Word,
    ids: &[usize],
    selected: &BTreeSet<usize>,
    source: &kanpo_pdf::Aj1PageSources,
) -> Option<(usize, usize, String)> {
    let positions = ids
        .iter()
        .enumerate()
        .filter(|(_, id)| selected.contains(id))
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    let first = *positions.first()?;
    let last = *positions.last()?;
    if positions.len() != last - first + 1 {
        return None;
    }
    let raw = ids
        .iter()
        .map(|&id| source.glyph_bbox_text(id))
        .collect::<Option<Vec<_>>>()?;
    if raw.concat() != word.raw {
        return None;
    }
    if first == 0 && last + 1 == ids.len() {
        let text = compact(&word.text);
        return Some((0, text.chars().count(), text));
    }
    let prefix = crate::normalize::word_text(&raw[..first].concat());
    let text = crate::normalize::word_text(&raw[first..=last].concat());
    let suffix = crate::normalize::word_text(&raw[last + 1..].concat());
    if compact(&format!("{prefix}{text}{suffix}")) != compact(&word.text) {
        return None;
    }
    let text = compact(&text);
    Some((compact(&prefix).chars().count(), text.chars().count(), text))
}
fn candidate(
    pi: usize,
    glyph: usize,
    frame: kanpo_pdf::RectangleFrame,
    base: &crate::Analysis,
    source: &kanpo_pdf::XObjectPageSources,
    slots: &Layout,
    mapped: &BTreeMap<usize, Vec<usize>>,
    paint: &PaintScene,
) -> Result<Option<Enclosure>> {
    let page = &base.pages[pi];
    let native = source.page();
    let owner = mapped
        .iter()
        .flat_map(|(&word, ids)| ids.iter().map(move |&g| (g, word)))
        .collect::<BTreeMap<_, _>>();
    let other = native
        .glyphs
        .iter()
        .enumerate()
        .filter(|(id, g)| {
            *id != glyph
                && !g.invisible
                && super::positive(g.bbox)
                && super::overlaps(g.bbox, frame.outer)
        })
        .collect::<Vec<_>>();
    if other.is_empty() {
        return Ok(None);
    }
    ensure!(
        !native.glyphs[glyph].invisible,
        "frame drawing is invisible"
    );
    ensure!(
        paint.unknown.is_empty(),
        "original paint inventory has unknown glyphs"
    );
    ensure!(
        !paint
            .images
            .iter()
            .chain(&paint.fills)
            .chain(paint.lines.iter().map(|(_, b)| b))
            .any(|&b| super::overlaps(b, frame.outer)),
        "other source paint intersects the enclosure"
    );
    let word = *owner
        .get(&glyph)
        .ok_or_else(|| anyhow::anyhow!("frame Word occurrence is not unique"))?;
    let producer_is_text = page.words[word].status == WordStatus::Used;
    let mut at = slots.words.get(&(pi, word));
    ensure!(
        !producer_is_text || at.is_some(),
        "complete frame line/cell ownership is unavailable"
    );
    let mut parent_words = BTreeSet::new();
    let mut parent_glyphs = Vec::new();
    for (id, g) in other {
        ensure!(
            source.native_sources().glyph_rectangle_frame(id).is_none(),
            "nested frame ownership is ambiguous"
        );
        ensure!(
            super::contains(frame.inner, g.bbox),
            "another glyph crosses the painted border"
        );
        ensure!(
            source.native_sources().glyph_text_axes(id).is_some(),
            "enclosed text transform is unknown"
        );
        let own = *owner
            .get(&id)
            .ok_or_else(|| anyhow::anyhow!("enclosed glyph has no unique Word"))?;
        let part = slots
            .words
            .get(&(pi, own))
            .ok_or_else(|| anyhow::anyhow!("complete enclosed Word ownership is unavailable"))?;
        if let Some(frame_at) = at {
            ensure!(
                part.location == frame_at.location && part.field == frame_at.field,
                "enclosed words do not share the original line/cell"
            );
        } else {
            at = Some(part);
        }
        parent_words.insert(own);
        parent_glyphs.push(id);
    }
    let at = at.ok_or_else(|| anyhow::anyhow!("frame has no uniquely owned text content"))?;
    ensure!(
        slots.streams[index(at.field)] == crate::source_underlines::field_text(base, at.field),
        "complete original and document fields differ"
    );
    let (remove_word, remove_length, remove_text) = if producer_is_text {
        segment(
            &page.words[word],
            &mapped[&word],
            &[glyph].into_iter().collect(),
            source.native_sources(),
        )
        .ok_or_else(|| {
            anyhow::anyhow!("frame glyph does not have a complete canonical Word span")
        })?
    } else {
        (0, 0, String::new())
    };
    let selected = parent_glyphs.iter().copied().collect();
    let mut parts = Vec::new();
    for &parent in &parent_words {
        let (start, length, text) = segment(
            &page.words[parent],
            &mapped[&parent],
            &selected,
            source.native_sources(),
        )
        .ok_or_else(|| anyhow::anyhow!("enclosed text is not a contiguous canonical Word span"))?;
        ensure!(length > 0, "enclosed source Word has no body text");
        parts.push((slots.words[&(pi, parent)].global + start, length, text));
    }
    parts.sort_by_key(|p| p.0);
    let start = parts[0].0;
    let end = parts.last().unwrap().0 + parts.last().unwrap().1;
    let remove_global = at.global + remove_word;
    let mut ranges = parts.iter().map(|p| (p.0, p.0 + p.1)).collect::<Vec<_>>();
    if start <= remove_global && remove_global + remove_length <= end {
        ranges.push((remove_global, remove_global + remove_length));
    }
    ranges.sort_unstable();
    let mut until = start;
    for (a, b) in ranges {
        ensure!(
            a == until,
            "enclosed text has intervening or overlapping canonical material"
        );
        until = b;
    }
    ensure!(until == end, "enclosed source range is incomplete");
    Ok(Some(Enclosure {
        page: pi,
        glyph,
        word,
        location: at.location,
        field: at.field,
        remove_word,
        remove_local: at.local + remove_word,
        remove_global,
        remove_length,
        remove_text,
        parent_start: start,
        parent_end: end,
        parent_text: parts.into_iter().map(|p| p.2).collect(),
        parent_glyphs,
        parent_words: parent_words.into_iter().collect(),
        frame,
    }))
}
fn byte_span(text: &str, start: usize, length: usize) -> Option<std::ops::Range<usize>> {
    let positions = text
        .char_indices()
        .filter(|(_, c)| !c.is_whitespace())
        .collect::<Vec<_>>();
    if length == 0 {
        return Some(0..0);
    }
    let a = positions.get(start)?.0;
    let (b, c) = *positions.get(start.checked_add(length)?.checked_sub(1)?)?;
    Some(a..b + c.len_utf8())
}
fn remove(text: &mut String, ranges: &[(usize, usize)]) -> Result<()> {
    for &(start, length) in ranges.iter().rev() {
        let range = byte_span(text, start, length)
            .ok_or_else(|| anyhow::anyhow!("source scalar deletion range is unavailable"))?;
        text.replace_range(range, "");
    }
    Ok(())
}
fn project(position: usize, removed: &[(usize, usize)]) -> Option<usize> {
    let mut amount = 0;
    for &(start, length) in removed {
        let end = start.checked_add(length)?;
        if start < position && position < end {
            return None;
        }
        if end <= position {
            amount += length;
        }
    }
    position.checked_sub(amount)
}
fn one_document(base: &crate::Analysis, field: Field, start: usize, end: usize) -> bool {
    let mut at = 0;
    let mut count = 0;
    for article in &base.articles {
        let size = Document::from(&article.document)
            .field_text(field)
            .chars()
            .count();
        if at <= start && end <= at + size {
            count += 1;
        }
        at += size;
    }
    count == 1
}
pub(super) fn apply(
    base: &crate::Analysis,
    sources: &[kanpo_pdf::XObjectPageSources],
    paint: Option<&PaintAudit>,
) -> Result<(crate::Analysis, Vec<Enclosure>, Vec<Unresolved>)> {
    ensure!(
        base.pages.len() == sources.len(),
        "frame source page inventories differ"
    );
    let slots = layout(base, paint);
    let mut plans = Vec::new();
    let mut unresolved = Vec::new();
    for (pi, (page, source)) in base.pages.iter().zip(sources).enumerate() {
        let shapes = (0..source.page().glyphs.len())
            .filter_map(|id| {
                source
                    .native_sources()
                    .glyph_rectangle_frame(id)
                    .map(|f| (id, f))
            })
            .collect::<Vec<_>>();
        if shapes.is_empty() {
            continue;
        }
        let mapped = super::ruby::occurrences(page, source.page());
        let paint = scene(page, source);
        for (glyph, frame) in shapes {
            let result = paint
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("complete original frame scene is unavailable"))
                .and_then(|paint| {
                    candidate(pi, glyph, frame, base, source, &slots, &mapped, paint)
                });
            match result {
                Ok(Some(plan)) => plans.push(plan),
                Ok(None) => {}
                Err(error) => unresolved.push(Unresolved {
                    page: pi,
                    frame_glyph: glyph,
                    reason: error.to_string(),
                }),
            }
        }
    }
    let conflicts = plans
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            plans
                .iter()
                .enumerate()
                .any(|(j, q)| {
                    i != j
                        && p.field == q.field
                        && ((p.remove_global < q.remove_global + q.remove_length
                            && q.remove_global < p.remove_global + p.remove_length)
                            || (p.parent_start < q.parent_end && q.parent_start < p.parent_end))
                })
                .then_some(i)
        })
        .collect::<BTreeSet<_>>();
    plans = plans
        .into_iter()
        .enumerate()
        .filter_map(|(i, p)| {
            if conflicts.contains(&i) {
                unresolved.push(Unresolved {
                    page: p.page,
                    frame_glyph: p.glyph,
                    reason: "competing source enclosure ranges".into(),
                });
                None
            } else {
                Some(p)
            }
        })
        .collect();
    if plans.is_empty() {
        return Ok((base.clone(), plans, unresolved));
    }
    let attempt = (|| -> Result<crate::Analysis> {
        let mut observed = base.clone();
        let mut table_count = 0;
        for (pi, page) in observed.pages.iter_mut().enumerate() {
            let own = plans.iter().filter(|p| p.page == pi).collect::<Vec<_>>();
            if !own.is_empty() {
                let mut words = BTreeMap::<usize, Vec<(usize, usize)>>::new();
                let mut containers = BTreeMap::<Location, Vec<(usize, usize)>>::new();
                for p in own {
                    words
                        .entry(p.word)
                        .or_default()
                        .push((p.remove_word, p.remove_length));
                    containers
                        .entry(p.location)
                        .or_default()
                        .push((p.remove_local, p.remove_length));
                }
                for (word, mut ranges) in words {
                    ranges.sort_unstable();
                    remove(&mut page.words[word].text, &ranges)?;
                }
                for ((ri, li, ci), mut ranges) in containers {
                    ranges.sort_unstable();
                    let line = &mut page.regions[ri].lines[li];
                    if let Some(ci) = ci {
                        let table = line.table.as_mut().unwrap();
                        remove(&mut table.cells[ci].text, &ranges)?;
                        line.text = table.render();
                    } else {
                        remove(&mut line.text, &ranges)?;
                    }
                }
                let units = crate::native_role_continuation::frames::existing_units(page)
                    .ok_or_else(|| anyhow::anyhow!("frame page containers cannot round-trip"))?;
                let mut end = table_count;
                let (doc_text, tables) = crate::vertical::doc_text_of(&units, &mut end);
                ensure!(
                    tables.len() == page.tables.len(),
                    "frame repair changed table inventory"
                );
                page.text = units
                    .iter()
                    .map(Unit::text)
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n");
                page.doc_text = doc_text;
                page.tables = tables;
            }
            table_count += page.tables.len();
        }
        let inputs = sources
            .iter()
            .map(|s| crate::PageInput {
                rules: Some(kanpo_pdf::page_rules(s.page())),
                xhtml: kanpo_pdf::bbox::bbox_layout_with_native_styles(std::slice::from_ref(
                    s.native_sources(),
                )),
            })
            .collect::<Vec<_>>();
        let xhtml = crate::combine_pages(inputs.iter().map(|p| p.xhtml.as_str()));
        let rules = inputs.iter().map(|p| p.rules.clone()).collect::<Vec<_>>();
        let mut rebuilt = crate::analysis_from_pages(observed.pages, &xhtml, &rules);
        for field in [Field::Paragraph, Field::After, Field::Before] {
            let mut ranges = plans
                .iter()
                .filter(|p| p.field == field)
                .map(|p| (p.remove_global, p.remove_length))
                .collect::<Vec<_>>();
            ranges.sort_unstable();
            if !ranges.is_empty() {
                let mut expected = slots.streams[index(field)].clone();
                remove(&mut expected, &ranges)?;
                ensure!(
                    expected == crate::source_underlines::field_text(&rebuilt, field),
                    "frame repair changed other complete field material"
                );
                for p in plans.iter().filter(|p| p.field == field) {
                    let a = project(p.parent_start, &ranges)
                        .ok_or_else(|| anyhow::anyhow!("frame start overlaps removal"))?;
                    let b = project(p.parent_end, &ranges)
                        .ok_or_else(|| anyhow::anyhow!("frame end overlaps removal"))?;
                    ensure!(
                        a < b && one_document(&rebuilt, field, a, b),
                        "frame range crosses a final article boundary"
                    );
                    ensure!(
                        expected.chars().skip(a).take(b - a).collect::<String>() == p.parent_text,
                        "framed source text differs after repair"
                    );
                }
            }
            crate::source_underlines::transfer_field(base, &mut rebuilt, field);
        }
        Ok(rebuilt)
    })();
    match attempt {
        Ok(observed) => Ok((observed, plans, unresolved)),
        Err(error) => {
            unresolved.extend(plans.iter().map(|p| Unresolved {
                page: p.page,
                frame_glyph: p.glyph,
                reason: error.to_string(),
            }));
            Ok((base.clone(), Vec::new(), unresolved))
        }
    }
}
pub(super) fn nontext(plans: &[Enclosure]) -> BTreeSet<(usize, usize)> {
    plans.iter().map(|p| (p.page, p.glyph)).collect()
}
pub(super) fn bind(
    plans: &[Enclosure],
    mut unresolved: Vec<Unresolved>,
    base: &crate::Analysis,
    documents: &[Document],
    sources: &[kanpo_pdf::XObjectPageSources],
) -> Audit {
    let mut bindings = Vec::new();
    for p in plans {
        let mut removed = plans
            .iter()
            .filter(|q| q.field == p.field)
            .map(|q| (q.remove_global, q.remove_length))
            .collect::<Vec<_>>();
        removed.sort_unstable();
        let start = project(p.parent_start, &removed).unwrap();
        let end = project(p.parent_end, &removed).unwrap();
        let text = documents
            .iter()
            .map(|d| d.field_text(p.field))
            .collect::<String>();
        if text != crate::source_underlines::field_text(base, p.field) {
            unresolved.push(Unresolved {
                page: p.page,
                frame_glyph: p.glyph,
                reason: "complete final frame field differs".into(),
            });
            continue;
        }
        let mut offset = 0;
        for (document, d) in documents.iter().enumerate() {
            let length = d.field_text(p.field).chars().count();
            if offset <= start && end <= offset + length && start < end {
                bindings.push(Binding {
                    source_document_sha256: super::metadata::hex(
                        &sources[p.page].document_sha256(),
                    ),
                    document,
                    field: p.field,
                    offset: start - offset,
                    length: end - start,
                    text: p.parent_text.clone(),
                    page: p.page,
                    frame_glyph: p.glyph,
                    frame_word: p.word,
                    parent_glyphs: p.parent_glyphs.clone(),
                    parent_words: p.parent_words.clone(),
                    frame: p.frame,
                    removed_text: p.remove_text.clone(),
                    removed_offset: p.remove_global,
                    removed_length: p.remove_length,
                });
            }
            offset += length;
        }
    }
    Audit {
        scope: "strict_cff_text_enclosures",
        bindings,
        unresolved,
        coverage_verified: false,
    }
}

#[cfg(test)]
#[path = "media_frames_tests.rs"]
mod tests;
