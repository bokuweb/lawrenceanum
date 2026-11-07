//! Effective native glyph sizes, attached through complete source field tapes.
//! Point sizes include the original text/page transform; neither an em bbox
//! estimate nor the desired rendered output supplies a font size.
use super::{Document, Field};
use crate::{source_underlines::Basis, trace::WordStatus};
use serde::Serialize;
use std::collections::BTreeSet;
#[path = "media_font_styles.rs"]
mod styles;
pub use styles::{Family, Style, StyleBinding};
#[derive(Debug, Clone, Serialize)]
pub struct Binding {
    pub page: usize,
    pub words: Vec<usize>,
    pub glyphs: Vec<usize>,
    pub document: usize,
    pub field: Field,
    pub offset: usize,
    pub length: usize,
    pub text: String,
    pub points: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct Ruby {
    pub page: usize,
    pub word: usize,
    pub base_points: f64,
    pub reading_points: f64,
    pub raise_points: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct Unresolved {
    pub page: usize,
    pub word: usize,
    pub reason: &'static str,
}
#[derive(Debug, Clone, Serialize)]
pub struct Audit {
    pub bindings: Vec<Binding>,
    pub styles: Vec<StyleBinding>,
    pub rubies: Vec<Ruby>,
    pub unresolved: Vec<Unresolved>,
}
/// Rebind only original native glyph sizes to a final (possibly OCR-extended)
/// analysis. OCR words have no certified native occurrence and remain unknown.
/// Unchanged fields should retain the richer original forward-projection audit.
pub fn analyze_documents(
    pdf: &[u8],
    analysis: &crate::Analysis,
    documents: &[Document],
) -> anyhow::Result<Audit> {
    let sources = kanpo_pdf::extract_pages_with_xobject_sources(pdf)?;
    let ruby = super::ruby::analyze(analysis, documents, &sources);
    Ok(analyze_inner(
        analysis,
        documents,
        &sources,
        &ruby,
        &[],
        true,
    ))
}
fn size(page: &kanpo_pdf::Page, ids: &[usize]) -> Option<f64> {
    if ids.is_empty() {
        return None;
    }
    let sizes = ids
        .iter()
        .map(|&id| {
            let g = page.glyphs.get(id)?;
            (!g.invisible && g.size.is_finite() && g.size > 0.).then_some(g.size.to_bits())
        })
        .collect::<Option<BTreeSet<_>>>()?;
    (sizes.len() == 1).then(|| f64::from_bits(*sizes.first().unwrap()))
}
fn baseline(source: &kanpo_pdf::Aj1PageSources, ids: &[usize]) -> Option<f64> {
    let mut values = vec![];
    for &id in ids {
        let g = source.page().glyphs.get(id)?;
        let basis = Basis::glyph(g, source.glyph_text_axes(id)?)?;
        values.push(basis.point([g.x, g.y])[0]);
    }
    let first = *values.first()?;
    values
        .iter()
        .all(|v| v.is_finite() && (*v as f32) == (first as f32))
        .then_some(first)
}
pub(super) fn analyze(
    base: &crate::Analysis,
    documents: &[Document],
    sources: &[kanpo_pdf::XObjectPageSources],
    ruby: &super::ruby::Audit,
    projections: &[crate::source_line_projection::Record],
) -> Audit {
    analyze_inner(base, documents, sources, ruby, projections, false)
}

fn analyze_inner(
    base: &crate::Analysis,
    documents: &[Document],
    sources: &[kanpo_pdf::XObjectPageSources],
    ruby: &super::ruby::Audit,
    projections: &[crate::source_line_projection::Record],
    allow_native_prefix: bool,
) -> Audit {
    let (streams, offsets) = super::ruby::offsets(base);
    let fields = [Field::Paragraph, Field::After, Field::Before];
    let values = fields.map(|f| {
        documents
            .iter()
            .map(|d| d.field_text(f))
            .collect::<Vec<_>>()
    });
    let mut out = Audit {
        bindings: vec![],
        styles: vec![],
        rubies: vec![],
        unresolved: vec![],
    };
    let target = |field: Field, start: usize, text: &str| -> Option<(usize, usize)> {
        let fi = fields.iter().position(|f| *f == field)?;
        let length = text.chars().count();
        let end = start.checked_add(length)?;
        let final_field = values[fi].concat();
        // OCR appends articles without changing the native PageTrace. In the
        // explicit final-document API, a complete native field may be an exact
        // leading prefix of the final model. This is not a substring search or
        // a match on an individual heading. Added OCR scalars receive no claims.
        let native_agrees = streams[fi] == final_field
            || (allow_native_prefix
                && !streams[fi].is_empty()
                && final_field.starts_with(&streams[fi]));
        if length == 0
            || !native_agrees
            || final_field != crate::source_underlines::field_text(base, field)
        {
            return None;
        }

        let mut cursor = 0;
        let mut found = vec![];
        for (document, value) in values[fi].iter().enumerate() {
            let next = cursor + value.chars().count();
            if cursor <= start
                && end <= next
                && start < next
                && value
                    .chars()
                    .skip(start - cursor)
                    .take(length)
                    .collect::<String>()
                    == text
            {
                found.push((document, start - cursor));
            }
            cursor = next;
        }
        match found.as_slice() {
            [t] => Some(*t),
            _ => None,
        }
    };
    let mut positions = [0usize; 3];
    for (pi, (trace, source)) in base.pages.iter().zip(sources).enumerate() {
        let occurrences = super::ruby::occurrences(trace, source.page());
        let mut covered = BTreeSet::new();
        for region in &trace.regions {
            let field = super::field(region.role);
            let fi = fields.iter().position(|f| *f == field).unwrap();
            for line in &region.lines {
                let mut parts = if let Some(t) = &line.table {
                    let mut cells = t.cells.iter().collect::<Vec<_>>();
                    cells.sort_by_key(|c| (c.row, c.col));
                    cells
                        .iter()
                        .map(|c| (c.text.as_str(), c.words.as_slice()))
                        .collect::<Vec<_>>()
                } else {
                    vec![(line.text.as_str(), line.words.as_slice())]
                };
                let compact =
                    |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
                if compact(&parts.iter().map(|p| p.0).collect::<String>()) != compact(&line.text) {
                    parts.clear();
                }
                let mut start = positions[fi];
                for (text, words) in parts {
                    let text = compact(text);
                    let length = text.chars().count();
                    let uniform = (|| {
                        let unique = words.iter().copied().collect::<BTreeSet<_>>();
                        if unique.len() != words.len() || words.iter().any(|w| covered.contains(w))
                        {
                            return None;
                        }
                        let original = words
                            .iter()
                            .map(|id| trace.words.get(*id))
                            .collect::<Option<Vec<_>>>()?;
                        if original.iter().any(|w| w.status != WordStatus::Used) {
                            return None;
                        }
                        let bag = |s: String| {
                            let mut c = std::collections::BTreeMap::new();
                            for ch in s.chars().filter(|c| !c.is_whitespace()) {
                                *c.entry(ch).or_insert(0usize) += 1;
                            }
                            c
                        };
                        if bag(original.iter().map(|w| w.text.as_str()).collect())
                            != bag(text.clone())
                        {
                            return None;
                        }
                        let glyphs = words
                            .iter()
                            .map(|id| occurrences.get(id).cloned())
                            .collect::<Option<Vec<_>>>()?
                            .into_iter()
                            .flatten()
                            .collect::<Vec<_>>();
                        let points = size(source.page(), &glyphs)?;
                        let (document, offset) = target(field, start, &text)?;
                        Some(Binding {
                            page: pi,
                            words: words.to_vec(),
                            glyphs,
                            document,
                            field,
                            offset,
                            length,
                            text: text.clone(),
                            points,
                        })
                    })();
                    if let Some(b) = uniform {
                        covered.extend(words.iter().copied());
                        out.bindings.push(b);
                    }
                    start += length;
                }
                positions[fi] += compact(&line.text).chars().count();
            }
        }
        for w in trace
            .words
            .iter()
            .filter(|w| w.status == WordStatus::Used && !w.text.trim().is_empty())
        {
            if covered.contains(&w.id) {
                continue;
            }
            let binding = (|| {
                let ids = occurrences.get(&w.id)?;
                let points = size(source.page(), ids)?;
                let &(field, start) = offsets.get(&(pi, w.id))?;
                let text = w
                    .text
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect::<String>();
                let length = text.chars().count();
                let (document, offset) = target(field, start, &text)?;
                Some(Binding {
                    page: pi,
                    words: vec![w.id],
                    glyphs: ids.clone(),
                    document,
                    field,
                    offset,
                    length,
                    text,
                    points,
                })
            })();
            if let Some(b) = binding {
                out.bindings.push(b);
            } else {
                out.unresolved.push(Unresolved{page:pi,word:w.id,reason:"uniform original glyph size or complete source field occurrence unavailable"});
            }
        }
    }
    for b in &ruby.bindings {
        let measured = (|| {
            let source = sources.get(b.page)?;
            let p = source.page();
            let base_points = size(p, &b.parent_glyphs)?;
            let reading_points = size(p, &b.ruby_glyphs)?;
            let raise_points = (baseline(source.native_sources(), &b.parent_glyphs)?
                - baseline(source.native_sources(), &b.ruby_glyphs)?)
            .abs();
            (raise_points.is_finite() && raise_points > 0.).then_some(Ruby {
                page: b.page,
                word: b.ruby_word,
                base_points,
                reading_points,
                raise_points,
            })
        })();
        if let Some(r) = measured {
            out.rubies.push(r);
        }
    }
    // Certified forward edits can interleave scalars from one original Word
    // across legal quote operations. Reuse those original scalar identities;
    // never search the resulting text for an equal spelling.
    let mut claimed = BTreeSet::new();
    for b in &out.bindings {
        for scalar in b.offset..b.offset + b.length {
            claimed.insert((
                b.document,
                match b.field {
                    Field::Paragraph => 0,
                    Field::After => 1,
                    Field::Before => 2,
                },
                scalar,
            ));
        }
    }
    let mut source_scalars = std::collections::BTreeMap::<(usize, usize), BTreeSet<usize>>::new();
    let occurrences = base
        .pages
        .iter()
        .zip(sources)
        .map(|(p, s)| super::ruby::occurrences(p, s.page()))
        .collect::<Vec<_>>();
    for record in projections {
        if crate::source_line_projection::value(&record.tokens) != record.text {
            continue;
        }
        for (i, token) in record.tokens.iter().enumerate() {
            use crate::source_line_projection::Origin;
            let (words, scalar) = match &token.origin {
                Origin::Whitespace => continue,
                Origin::Word { word, scalar } | Origin::RelocatedWord { word, scalar, .. } => {
                    let Some(w) = base.pages.get(record.page).and_then(|p| p.words.get(*word))
                    else {
                        continue;
                    };
                    // A certified relocation consumes a real native word
                    // before ordinary line ownership is assigned (e.g. an
                    // amount sign). Its trace may therefore be Noise. Only
                    // the forward relocation permits that status; ordinary
                    // words still need a used source occurrence.
                    let status_ok = w.status == WordStatus::Used
                        || (matches!(&token.origin, Origin::RelocatedWord { .. })
                            && w.status == WordStatus::Noise);
                    if !status_ok
                        || w.text.chars().filter(|c| !c.is_whitespace()).nth(*scalar)
                            != Some(token.character)
                    {
                        continue;
                    }
                    (vec![*word], Some(*scalar))
                }
                Origin::ParallelJoin { before, after } => {
                    let words = before
                        .iter()
                        .chain(after)
                        .copied()
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect::<Vec<_>>();
                    (words, None)
                }
            };
            let Some(source) = sources.get(record.page) else {
                continue;
            };
            let Some(glyphs) = words
                .iter()
                .map(|w| occurrences[record.page].get(w).cloned())
                .collect::<Option<Vec<_>>>()
                .map(|ids| ids.into_iter().flatten().collect::<Vec<_>>())
            else {
                continue;
            };
            let Some(points) = size(source.page(), &glyphs) else {
                continue;
            };
            let text = token.character.to_string();
            let Some((document, offset)) = target(record.field, record.offset + i, &text) else {
                continue;
            };
            let fi = match record.field {
                Field::Paragraph => 0,
                Field::After => 1,
                Field::Before => 2,
            };
            if !claimed.insert((document, fi, offset)) {
                continue;
            }
            if let Some(scalar) = scalar {
                source_scalars
                    .entry((record.page, words[0]))
                    .or_default()
                    .insert(scalar);
            }
            out.bindings.push(Binding {
                page: record.page,
                words,
                glyphs,
                document,
                field: record.field,
                offset,
                length: 1,
                text,
                points,
            });
        }
    }
    out.unresolved.retain(|u| {
        source_scalars.get(&(u.page, u.word)).is_none_or(|scalars| {
            let n = base.pages[u.page].words[u.word]
                .text
                .chars()
                .filter(|c| !c.is_whitespace())
                .count();
            scalars.len() != n || !scalars.iter().copied().eq(0..n)
        })
    });
    out.styles = styles::bind(base, documents, sources, &out.bindings);
    out
}
