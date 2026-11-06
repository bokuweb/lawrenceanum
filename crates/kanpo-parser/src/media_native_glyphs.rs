//! Explicit recognition of isolated, still-unmapped original Identity glyphs.
//! Geometry proves source identity and placement, never the recognized Unicode.
use super::{metadata, PaintAudit};
use crate::trace::{PageTrace, Unit, WordStatus};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Candidate {
    pub source_document_sha256: String,
    pub page: usize,
    pub word: usize,
    pub glyph: usize,
    pub raw_index: usize,
    pub cid: u32,
    pub font: usize,
    pub font_x: [i8; 2],
    pub font_y: [i8; 2],
    pub bounds: [f64; 4],
    pub source_word_box: [f32; 4],
}
#[derive(Debug, Clone, Serialize)]
pub struct Binding {
    pub observation: usize,
    pub source: Candidate,
    pub text: String,
    pub status: &'static str,
}
#[derive(Debug, Clone, Serialize)]
pub struct Unresolved {
    pub observation: usize,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct SourceIssue {
    pub page: usize,
    pub word: usize,
    pub remaining_scalars: usize,
    pub reason: &'static str,
}
#[derive(Debug, Clone, Serialize)]
pub struct Audit {
    pub candidates: Vec<Candidate>,
    pub bindings: Vec<Binding>,
    pub unresolved: Vec<Unresolved>,
    pub source_issues: Vec<SourceIssue>,
    pub coverage_verified: bool,
}
#[derive(Deserialize)]
struct Reading {
    candidate: Candidate,
    text: String,
    status: String,
}
#[derive(Clone)]
struct Target {
    candidate: Candidate,
    region: usize,
    line: usize,
    word_byte: usize,
    line_byte: usize,
}
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn scalar_position(s: &str, n: usize) -> Option<usize> {
    s.char_indices()
        .filter(|(_, c)| !c.is_whitespace())
        .nth(n)
        .map(|(i, _)| i)
}
pub(super) fn line_parts(
    page: &PageTrace,
    pi: usize,
    ri: usize,
    li: usize,
    paint: Option<&PaintAudit>,
) -> Option<BTreeMap<usize, usize>> {
    let line = page.regions.get(ri)?.lines.get(li)?;
    if line.table.is_some() {
        return None;
    }
    let mut offsets = BTreeMap::new();
    let mut all = String::new();
    let mut seen = BTreeSet::new();
    for (i, &id) in line.words.iter().enumerate() {
        let word = page.words.get(id)?;
        if word.status != WordStatus::Used || word.id != id || !seen.insert(id) {
            return None;
        }
        offsets.insert(id, all.chars().count());
        all.push_str(&compact(&word.text));
        if let Some(paint) = paint {
            for binding in paint
                .bindings
                .iter()
                .filter(|b| b.page == pi && b.region == ri && b.line == li && b.before_word == id)
            {
                if line.words.get(i + 1).copied() != Some(binding.after_word) {
                    return None;
                }
                let text = paint
                    .observation_report
                    .get("observations")?
                    .get(binding.observation)?
                    .get("text")?
                    .as_str()?;
                all.push_str(&compact(text));
            }
        }
    }
    (all == compact(&line.text)).then_some(offsets)
}
fn isolated(source: &kanpo_pdf::XObjectPageSources, id: usize) -> bool {
    let page = source.page();
    let Some(g) = page.glyphs.get(id) else {
        return false;
    };
    let b = g.bbox;
    if g.invisible
        || !super::positive(b)
        || b[0] < 0.
        || b[1] < 0.
        || b[2] > page.width
        || b[3] > page.height
        || source.native_sources().glyph_text_axes(id).is_none()
        || !source.capture_gaps().is_empty()
    {
        return false;
    }
    if page
        .glyphs
        .iter()
        .enumerate()
        .any(|(i, other)| i != id && !other.invisible && super::overlaps(b, other.bbox))
    {
        return false;
    }
    if source.images().iter().any(|draw| {
        super::quad(source, draw)
            .and_then(super::quad_bounds)
            .is_none_or(|other| super::overlaps(b, other))
    }) {
        return false;
    }
    !page
        .paths
        .iter()
        .filter(|p| p.stroke || p.fill)
        .any(|p| crate::painted_text::paint_bounds(p).is_none_or(|other| super::overlaps(b, other)))
}
fn targets(
    analysis: &crate::Analysis,
    sources: &[kanpo_pdf::XObjectPageSources],
    paint: Option<&PaintAudit>,
) -> Vec<Target> {
    let mut out = Vec::new();
    for (pi, (page, source)) in analysis.pages.iter().zip(sources).enumerate() {
        let words = kanpo_pdf::bbox::words(source.page());
        let mut locations = BTreeMap::<usize, Vec<(usize, usize, usize)>>::new();
        for (ri, region) in page.regions.iter().enumerate() {
            for (li, _) in region.lines.iter().enumerate() {
                if let Some(offsets) = line_parts(page, pi, ri, li, paint) {
                    for (id, start) in offsets {
                        locations.entry(id).or_default().push((ri, li, start));
                    }
                }
            }
        }
        for word in page
            .words
            .iter()
            .filter(|w| w.status == WordStatus::Used && w.text.contains('\u{fffd}'))
        {
            let Some(location) = locations.get(&word.id).filter(|v| v.len() == 1) else {
                continue;
            };
            let (ri, li, start) = location[0];
            if crate::normalize::word_text(&word.raw) != word.text {
                continue;
            }
            let found = words
                .iter()
                .filter(|s| {
                    s.text == word.raw
                        && [s.x_min, s.y_min, s.x_max, s.y_max]
                            .map(|n| format!("{n:.6}").parse::<f32>().unwrap())
                            == word.source_box
                })
                .collect::<Vec<_>>();
            if found.len() != 1 {
                continue;
            }
            let ids = &found[0].glyphs;
            let raw = word.raw.chars().collect::<Vec<_>>();
            if ids.len() != raw.len() {
                continue;
            }
            for (offset, (&id, &ch)) in ids.iter().zip(&raw).enumerate() {
                let Some(g) = source.page().glyphs.get(id) else {
                    continue;
                };
                let Some(font) = source.page().fonts.get(g.font) else {
                    continue;
                };
                if font.ordering.as_deref() != Some("Identity")
                    || font.has_to_unicode
                    || g.code != g.cid
                    || kanpo_pdf::bbox::IDENTITY_CID_BASE
                        .checked_add(g.cid)
                        .and_then(char::from_u32)
                        != Some(ch)
                    || !isolated(source, id)
                {
                    continue;
                }
                let prefix = crate::normalize::word_text(&raw[..offset].iter().collect::<String>());
                let suffix =
                    crate::normalize::word_text(&raw[offset + 1..].iter().collect::<String>());
                if format!("{prefix}�{suffix}") != word.text {
                    continue;
                }
                let Some(line_byte) = scalar_position(
                    &page.regions[ri].lines[li].text,
                    start + compact(&prefix).chars().count(),
                ) else {
                    continue;
                };
                if !page.regions[ri].lines[li].text[line_byte..].starts_with('�') {
                    continue;
                }
                out.push(Target {
                    candidate: Candidate {
                        source_document_sha256: metadata::hex(&source.document_sha256()),
                        page: pi,
                        word: word.id,
                        glyph: id,
                        raw_index: offset,
                        cid: g.cid,
                        font: g.font,
                        font_x: source.native_sources().glyph_text_axes(id).unwrap().x,
                        font_y: source.native_sources().glyph_text_axes(id).unwrap().y,
                        bounds: g.bbox,
                        source_word_box: word.source_box,
                    },
                    region: ri,
                    line: li,
                    word_byte: prefix.len(),
                    line_byte,
                });
            }
        }
    }
    let mut counts = BTreeMap::new();
    for t in &out {
        *counts
            .entry((t.candidate.page, t.candidate.glyph))
            .or_insert(0) += 1;
    }
    out.retain(|t| counts[&(t.candidate.page, t.candidate.glyph)] == 1);
    out
}

pub(super) fn apply(
    base: &crate::Analysis,
    sources: &[kanpo_pdf::XObjectPageSources],
    paint: Option<&PaintAudit>,
    observations: Option<&serde_json::Value>,
) -> Result<(crate::Analysis, Audit)> {
    ensure!(
        base.pages.len() == sources.len(),
        "native glyph source inventories differ"
    );
    let targets = targets(base, sources, paint);
    let candidates = targets.iter().map(|t| t.candidate.clone()).collect();
    let readings: Vec<Reading> = observations
        .map(|v| serde_json::from_value(v.clone()))
        .transpose()?
        .unwrap_or_default();
    let mut unresolved = Vec::new();
    let mut accepted = Vec::new();
    let mut duplicates = BTreeMap::new();
    for (index, r) in readings.iter().enumerate() {
        let found = targets
            .iter()
            .filter(|t| t.candidate == r.candidate)
            .collect::<Vec<_>>();
        if r.status != "candidate_unverified"
            || r.text.is_empty()
            || r.text.chars().any(|c| {
                c.is_whitespace()
                    || c.is_control()
                    || c == '�'
                    || matches!(c as u32,0xE000..=0xF8FF|0xF0000..=0xFFFFD|0x100000..=0x10FFFD)
            })
            || found.len() != 1
        {
            unresolved.push(Unresolved{observation:index,reason:"original unresolved glyph, complete line correspondence or recognition status differs".into()});
            continue;
        }
        let t = found[0].clone();
        *duplicates
            .entry((t.candidate.page, t.candidate.glyph))
            .or_insert(0) += 1;
        accepted.push((index, t));
    }
    accepted.retain(|(i, t)| {
        if duplicates[&(t.candidate.page, t.candidate.glyph)] != 1 {
            unresolved.push(Unresolved {
                observation: *i,
                reason: "competing original glyph observations".into(),
            });
            false
        } else {
            true
        }
    });
    let mut observed = base.clone();
    let mut bindings = Vec::new();
    let mut table_count = 0;
    for (pi, page) in observed.pages.iter_mut().enumerate() {
        let mut own = accepted
            .iter()
            .filter(|(_, t)| t.candidate.page == pi)
            .collect::<Vec<_>>();
        if !own.is_empty() {
            let original = page.clone();
            own.sort_by_key(|(_, t)| (t.candidate.word, std::cmp::Reverse(t.word_byte)));
            for (i, t) in &own {
                page.words[t.candidate.word].text.replace_range(
                    t.word_byte..t.word_byte + '�'.len_utf8(),
                    &readings[*i].text,
                );
            }
            own.sort_by_key(|(_, t)| (t.region, t.line, std::cmp::Reverse(t.line_byte)));
            for (i, t) in &own {
                page.regions[t.region].lines[t.line].text.replace_range(
                    t.line_byte..t.line_byte + '�'.len_utf8(),
                    &readings[*i].text,
                );
            }
            if let Some(units) = crate::native_role_continuation::frames::existing_units(page) {
                let mut end = table_count;
                let (doc_text, tables) = crate::vertical::doc_text_of(&units, &mut end);
                ensure!(
                    tables.len() == original.tables.len(),
                    "native glyph reading changed tables"
                );
                page.text = units
                    .iter()
                    .map(Unit::text)
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n");
                page.doc_text = doc_text;
                page.tables = tables;
                for (i, t) in own {
                    bindings.push(Binding {
                        observation: *i,
                        source: t.candidate.clone(),
                        text: readings[*i].text.clone(),
                        status: "candidate_unverified",
                    });
                }
            } else {
                *page = original;
                for (i, _) in own {
                    unresolved.push(Unresolved {
                        observation: *i,
                        reason: "original page containers cannot round-trip".into(),
                    });
                }
            }
        }
        table_count += page.tables.len();
    }
    if !bindings.is_empty() {
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
        observed = crate::analysis_from_pages(observed.pages, &xhtml, &rules);
        for field in [
            super::Field::Paragraph,
            super::Field::After,
            super::Field::Before,
        ] {
            crate::source_underlines::transfer_field(base, &mut observed, field);
        }
    }
    bindings.sort_by_key(|b| b.observation);
    unresolved.sort_by_key(|u| u.observation);
    let source_issues=observed.pages.iter().enumerate().flat_map(|(pi,page)|page.words.iter().filter(|w|w.status==WordStatus::Used&&w.text.contains('�')).map(move|word|SourceIssue{page:pi,word:word.id,remaining_scalars:word.text.chars().filter(|&c|c=='�').count(),reason:"native text still has unmapped scalars; isolated glyph recognition did not cover them"})).collect();
    Ok((
        observed,
        Audit {
            candidates,
            bindings,
            unresolved,
            source_issues,
            coverage_verified: false,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ineligible_placeholder_words_still_remain_in_source_issue_inventory() {
        let pdf = include_bytes!("../tests/data/workbook/case-06.pdf");
        let sources = kanpo_pdf::extract_pages_with_xobject_sources(pdf).unwrap();
        let mut base = crate::analyze_item(&crate::page_inputs(pdf).unwrap());
        for page in &mut base.pages {
            for word in &mut page.words {
                if word.text.contains('�') {
                    word.raw = "not the original raw glyphs".into();
                }
            }
        }
        let (_, audit) = apply(&base, &sources, None, None).unwrap();
        assert!(audit.candidates.is_empty());
        assert!(audit.bindings.is_empty());
        assert_eq!(audit.source_issues.len(), 2);
        assert_eq!(
            audit
                .source_issues
                .iter()
                .map(|i| i.remaining_scalars)
                .sum::<usize>(),
            2
        );
    }

    #[test]
    fn earlier_paint_insertions_shift_glyph_positions_only_through_complete_source_tokens() {
        let pdf = include_bytes!("../tests/data/workbook/case-06.pdf");
        let sources = kanpo_pdf::extract_pages_with_xobject_sources(pdf).unwrap();
        let mut base = crate::analyze_item(&crate::page_inputs(pdf).unwrap());
        let original = targets(&base, &sources, None)
            .into_iter()
            .find(|t| t.candidate.word == 178)
            .unwrap();
        let page = &base.pages[original.candidate.page];
        let line = &page.regions[original.region].lines[original.line];
        let index = line
            .words
            .iter()
            .position(|&id| id == original.candidate.word)
            .unwrap();
        assert!(index > 0);
        let before = line.words[index - 1];
        let offsets = line_parts(
            page,
            original.candidate.page,
            original.region,
            original.line,
            None,
        )
        .unwrap();
        let byte = scalar_position(&line.text, offsets[&original.candidate.word]).unwrap();
        let paint = PaintAudit {
            bindings: vec![crate::painted_text::Binding {
                observation: 0,
                page: original.candidate.page,
                region: original.region,
                line: original.line,
                byte_offset: byte,
                before_word: before,
                after_word: original.candidate.word,
            }],
            unresolved: vec![],
            observation_report: serde_json::json!({"observations":[{"text":"任意観測"}]}),
        };
        base.pages[original.candidate.page].regions[original.region].lines[original.line]
            .text
            .insert_str(byte, "任意観測");
        let shifted = targets(&base, &sources, Some(&paint))
            .into_iter()
            .find(|t| t.candidate == original.candidate)
            .unwrap();
        assert_eq!(shifted.word_byte, original.word_byte);
        assert_eq!(shifted.line_byte, original.line_byte + "任意観測".len());
        assert!(targets(&base, &sources, None)
            .iter()
            .all(|t| t.candidate != original.candidate));
        let mut duplicate = paint;
        duplicate.bindings.push(duplicate.bindings[0].clone());
        assert!(targets(&base, &sources, Some(&duplicate))
            .iter()
            .all(|t| t.candidate != original.candidate));
    }
}
