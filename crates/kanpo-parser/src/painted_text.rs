//! Explicitly opt in to observed text from paint outside the native text layer.
//!
//! Source geometry and placement are checked here; recognition accuracy is not.
//! The original Analysis, all original Words and the complete observation report
//! remain available. The default native APIs never invoke an OCR engine.
use crate::trace::{Line, PageTrace, Unit, WordStatus};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize)]
struct Source {
    document_sha256: String,
    document_bytes: usize,
}
#[derive(Deserialize)]
struct Report {
    schema: usize,
    kind: String,
    source: Source,
    observations: Vec<Reading>,
}
#[derive(Deserialize)]
struct Reading {
    page: usize,
    before_glyph: usize,
    after_glyph: usize,
    gap: [f64; 4],
    path_ids: Vec<usize>,
    text: Option<String>,
    status: String,
}

/// A reading placed at a complete original Word boundary in one original line.
#[derive(Debug, Clone, Serialize)]
pub struct Binding {
    pub observation: usize,
    pub page: usize,
    pub region: usize,
    pub line: usize,
    /// UTF-8 byte offset in the original native Line.text, before any insertions.
    pub byte_offset: usize,
    pub before_word: usize,
    pub after_word: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Unresolved {
    pub observation: usize,
    pub reason: String,
}

/// The observed version is explicitly uncertified recognition, with a lossless
/// native version alongside it. No source glyph or native Word is fabricated.
#[derive(Debug, Clone, Serialize)]
pub struct PaintedAnalysis {
    pub native: crate::Analysis,
    pub observed: crate::Analysis,
    pub bindings: Vec<Binding>,
    pub unresolved: Vec<Unresolved>,
    pub observation_report: serde_json::Value,
}

type Rect = [f64; 4];
fn positive(b: Rect) -> bool {
    b.iter().all(|n| n.is_finite()) && b[0] < b[2] && b[1] < b[3]
}
fn contains(a: Rect, b: Rect) -> bool {
    a[0] <= b[0] && b[2] <= a[2] && a[1] <= b[1] && b[3] <= a[3]
}
fn overlaps(a: Rect, b: Rect) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}
pub(crate) fn paint_bounds(path: &kanpo_pdf::PathItem) -> Option<Rect> {
    if !path.fill && !path.stroke {
        return None;
    }
    if path
        .subpaths
        .iter()
        .any(|s| s.curved || path.stroke && s.points.len() > 2 && !s.rect)
    {
        return None;
    }
    let width = if path.stroke { path.line_width } else { 0. };
    if !width.is_finite() || width < 0. {
        return None;
    }
    let mut b = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for point in path.subpaths.iter().flat_map(|s| &s.points) {
        if !point.iter().all(|n| n.is_finite()) {
            return None;
        }
        for k in 0..2 {
            b[k] = b[k].min(point[k] - width / 2.);
            b[k + 2] = b[k + 2].max(point[k] + width / 2.);
        }
    }
    positive(b).then_some(b)
}

fn source_gap(reading: &Reading, source: &kanpo_pdf::XObjectPageSources) -> Result<()> {
    ensure!(
        source.images().is_empty() && source.capture_gaps().is_empty(),
        "opaque image source"
    );
    let page = source.page();
    let a = page
        .glyphs
        .get(reading.before_glyph)
        .ok_or_else(|| anyhow::anyhow!("missing preceding glyph"))?;
    let b = page
        .glyphs
        .get(reading.after_glyph)
        .ok_or_else(|| anyhow::anyhow!("missing following glyph"))?;
    ensure!(
        !a.invisible
            && !b.invisible
            && a.font == b.font
            && a.size == b.size
            && a.size.is_finite()
            && a.size > 0.
            && a.x == a.end_x
            && b.x == b.end_x
            && a.y < a.end_y
            && b.y < b.end_y
            && positive(a.bbox)
            && positive(b.bbox),
        "unsupported native advance"
    );
    let gap = [
        a.bbox[0].max(b.bbox[0]),
        a.end_y,
        a.bbox[2].min(b.bbox[2]),
        b.y,
    ];
    ensure!(
        positive(gap) && gap == reading.gap,
        "source gap differs from native advances"
    );
    ensure!(
        !page
            .glyphs
            .iter()
            .any(|g| !g.invisible && positive(g.bbox) && overlaps(gap, g.bbox)),
        "gap contains native glyphs"
    );
    // All contained paint is required, and unknown path geometry cannot be
    // treated as absent. These are source inventory checks, not visibility.
    let mut paths = Vec::new();
    for (id, path) in page.paths.iter().enumerate() {
        if !path.fill && !path.stroke {
            continue;
        }
        let bounds = paint_bounds(path).ok_or_else(|| anyhow::anyhow!("unbounded source paint"))?;
        if contains(gap, bounds) {
            paths.push(id);
        }
    }
    ensure!(
        !paths.is_empty() && paths == reading.path_ids,
        "paint inventory differs from source"
    );
    Ok(())
}

fn source_word(
    page: &PageTrace,
    source: &kanpo_pdf::Page,
    glyph: usize,
    last: bool,
) -> Result<usize> {
    let candidates = kanpo_pdf::bbox::words(source)
        .into_iter()
        .filter(|w| {
            if last {
                w.glyphs.last() == Some(&glyph)
            } else {
                w.glyphs.first() == Some(&glyph)
            }
        })
        .collect::<Vec<_>>();
    ensure!(
        candidates.len() == 1,
        "glyph is not one complete native Word boundary"
    );
    let native = &candidates[0];
    // Match the documented six-decimal XHTML projection, not a proximity search.
    let bounds = [native.x_min, native.y_min, native.x_max, native.y_max]
        .map(|n| format!("{n:.6}").parse::<f32>().unwrap());
    let words = page
        .words
        .iter()
        .filter(|w| w.raw == native.text && w.source_box == bounds)
        .collect::<Vec<_>>();
    ensure!(
        words.len() == 1 && words[0].status == WordStatus::Used,
        "native Word is absent, ambiguous or unused"
    );
    Ok(words[0].id)
}

fn seam(line: &Line, page: &PageTrace, before: usize, after: usize) -> Option<usize> {
    if line.table.is_some() {
        return None;
    }
    let slot = line
        .words
        .windows(2)
        .position(|ids| ids == [before, after])?;
    let chars = line
        .text
        .char_indices()
        .filter(|(_, c)| !c.is_whitespace())
        .collect::<Vec<_>>();
    let mut offset = 0usize;
    let mut boundary = None;
    let mut seen = BTreeSet::new();
    for (i, &id) in line.words.iter().enumerate() {
        let w = page.words.get(id)?;
        if w.id != id || w.status != WordStatus::Used || !seen.insert(id) {
            return None;
        }
        let text = w
            .text
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<Vec<_>>();
        if text.is_empty() {
            return None;
        }
        let end = offset.checked_add(text.len())?;
        let own = chars.get(offset..end)?;
        if !own.iter().map(|(_, c)| *c).eq(text) {
            return None;
        }
        if i == slot {
            let &(start, c) = own.last()?;
            boundary = Some(start + c.len_utf8());
        }
        offset = end;
    }
    (offset == chars.len()).then_some(boundary?)
}

fn bind(
    index: usize,
    reading: &Reading,
    pages: &[PageTrace],
    sources: &[kanpo_pdf::XObjectPageSources],
) -> Result<Binding> {
    ensure!(
        reading.status == "candidate_unverified",
        "observation is not a recognized candidate"
    );
    let text = reading
        .text
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("missing recognized text"))?;
    ensure!(
        !text.trim().is_empty() && !text.chars().any(char::is_control),
        "empty or control-bearing recognized text"
    );
    let page = pages
        .get(reading.page)
        .ok_or_else(|| anyhow::anyhow!("missing page"))?;
    let source = sources
        .get(reading.page)
        .ok_or_else(|| anyhow::anyhow!("missing native page"))?;
    source_gap(reading, source)?;
    let before = source_word(page, source.page(), reading.before_glyph, true)?;
    let after = source_word(page, source.page(), reading.after_glyph, false)?;
    ensure!(before != after, "a native Word is not a source gap");
    let mut locations = Vec::new();
    let mut counts = [0usize; 2];
    for (ri, region) in page.regions.iter().enumerate() {
        for (li, line) in region.lines.iter().enumerate() {
            for id in line.words.iter().chain(
                line.table
                    .iter()
                    .flat_map(|t| t.cells.iter().flat_map(|c| &c.words)),
            ) {
                for (k, expected) in [before, after].iter().enumerate() {
                    if id == expected {
                        counts[k] += 1;
                    }
                }
            }
            if let Some(byte_offset) = seam(line, page, before, after) {
                locations.push(Binding {
                    observation: index,
                    page: reading.page,
                    region: ri,
                    line: li,
                    byte_offset,
                    before_word: before,
                    after_word: after,
                });
            }
        }
    }
    ensure!(
        counts == [1, 1] && locations.len() == 1,
        "source Words do not have one adjacent plain-line seam"
    );
    Ok(locations.pop().unwrap())
}

/// Analyze a PDF with explicitly requested, still-unverified paint recognition.
/// A wrong PDF identity fails the request. Unplaceable/ambiguous observations
/// remain unresolved, with their raw evidence retained. Unicode accuracy and
/// final paint visibility are not established by these geometry checks.
pub fn analyze_pdf(pdf: &[u8], observation_report: serde_json::Value) -> Result<PaintedAnalysis> {
    let report: Report = serde_json::from_value(observation_report.clone())?;
    ensure!(
        report.schema == 1 && report.kind == "source_font_compound_observations",
        "unsupported paint observation schema"
    );
    let sources = kanpo_pdf::extract_pages_with_xobject_sources(pdf)?;
    let source = sources
        .first()
        .ok_or_else(|| anyhow::anyhow!("PDF has no pages"))?;
    let digest = source
        .document_sha256()
        .iter()
        .map(|n| format!("{n:02x}"))
        .collect::<String>();
    ensure!(
        report.source.document_bytes == pdf.len() && report.source.document_sha256 == digest,
        "paint observations belong to another PDF"
    );
    let inputs = crate::page_inputs(pdf)?;
    let native = crate::analyze_item(&inputs);
    let mut pages = native.pages.clone();
    let mut bindings = Vec::new();
    let mut unresolved = Vec::new();
    for (index, reading) in report.observations.iter().enumerate() {
        match bind(index, reading, &pages, &sources) {
            Ok(binding) => bindings.push(binding),
            Err(error) => unresolved.push(Unresolved {
                observation: index,
                reason: error.to_string(),
            }),
        }
    }
    // Competing observations at one original seam must not choose a winner by
    // text, confidence, report order, or the expected manuscript.
    let mut duplicates = BTreeMap::new();
    for b in &bindings {
        *duplicates
            .entry((b.page, b.region, b.line, b.byte_offset))
            .or_insert(0usize) += 1;
    }
    bindings.retain(|b| {
        if duplicates[&(b.page, b.region, b.line, b.byte_offset)] == 1 {
            return true;
        }
        unresolved.push(Unresolved {
            observation: b.observation,
            reason: "competing source seam observations".into(),
        });
        false
    });
    let mut table_count = 0;
    for (pi, page) in pages.iter_mut().enumerate() {
        let mut own = bindings.iter().filter(|b| b.page == pi).collect::<Vec<_>>();
        own.sort_by_key(|b| (b.region, b.line, std::cmp::Reverse(b.byte_offset)));
        if !own.is_empty() {
            // Patch original UTF-8 seams from right to left. Word/raw/geometry,
            // table cells, role ownership and all native glyph IDs stay intact.
            let original_regions = page.regions.clone();
            for b in &own {
                page.regions[b.region].lines[b.line].text.insert_str(
                    b.byte_offset,
                    report.observations[b.observation].text.as_deref().unwrap(),
                );
            }
            if let Some(units) = crate::native_role_continuation::frames::existing_units(page) {
                let mut end = table_count;
                let (doc_text, tables) = crate::vertical::doc_text_of(&units, &mut end);
                ensure!(
                    tables.len() == page.tables.len(),
                    "paint reading changed native tables"
                );
                page.text = units
                    .iter()
                    .map(Unit::text)
                    .filter(|t| !t.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n");
                page.doc_text = doc_text;
                page.tables = tables;
            } else {
                page.regions = original_regions;
                for b in own {
                    unresolved.push(Unresolved {
                        observation: b.observation,
                        reason: "original page containers cannot round-trip".into(),
                    });
                }
                bindings.retain(|b| b.page != pi);
            }
        }
        table_count += page.tables.len();
    }
    let xhtml = crate::combine_pages(inputs.iter().map(|p| p.xhtml.as_str()));
    let rules = inputs.iter().map(|p| p.rules.clone()).collect::<Vec<_>>();
    let observed = crate::analysis_from_pages(pages, &xhtml, &rules);
    unresolved.sort_by_key(|u| u.observation);
    Ok(PaintedAnalysis {
        native,
        observed,
        bindings,
        unresolved,
        observation_report,
    })
}
