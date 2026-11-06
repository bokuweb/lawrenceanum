//! Ruby annotations bind original small glyphs to source parent characters.
//! Offsets are non-whitespace character offsets in one complete document field.
use super::{model, Document, Field};
use crate::source_underlines::Basis;
use crate::trace::WordStatus;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
type Rect = [f64; 4];

#[derive(Debug, Clone, Serialize)]
pub struct Binding {
    pub source_document_sha256: String,
    pub document: usize,
    pub field: Field,
    pub offset: usize,
    pub length: usize,
    pub text: String,
    pub reading: String,
    pub page: usize,
    pub ruby_word: usize,
    pub parent_word: usize,
    pub ruby_glyphs: Vec<usize>,
    pub parent_glyphs: Vec<usize>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Unresolved {
    pub page: usize,
    pub word: usize,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct Audit {
    pub document_sha256: Option<String>,
    pub bindings: Vec<Binding>,
    pub unresolved: Vec<Unresolved>,
    pub coverage_verified: bool,
}
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn positive(b: Rect) -> bool {
    b.iter().all(|n| n.is_finite()) && b[0] < b[2] && b[1] < b[3]
}
fn union(a: Rect, b: Rect) -> Rect {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}
fn index(f: Field) -> usize {
    match f {
        Field::Paragraph => 0,
        Field::After => 1,
        Field::Before => 2,
    }
}
fn role(r: crate::trace::Role) -> Field {
    match r {
        crate::trace::Role::After => Field::After,
        crate::trace::Role::Before => Field::Before,
        _ => Field::Paragraph,
    }
}
pub(super) use crate::source_baseline_shifts::occurrences;
// This map is built from source line/cell order, never by locating parent text
// in the result. An interleaved or synthesized line is deliberately unavailable.
pub(super) fn offsets(analysis: &crate::Analysis) -> ([String; 3], BTreeMap<(usize, usize), (Field, usize)>) {
    let mut streams: [String; 3] = Default::default();
    let mut out = BTreeMap::new();
    let mut duplicates = BTreeSet::new();
    for (pi, page) in analysis.pages.iter().enumerate() {
        for region in &page.regions {
            let field = role(region.role);
            for line in &region.lines {
                let mut part = |text: &str, ids: &[usize], start: usize| {
                    let mut own = String::new();
                    let mut starts = Vec::new();
                    let mut seen = BTreeSet::new();
                    for &id in ids {
                        let Some(w) = page.words.get(id).filter(|w| {
                            w.id == id && w.status == WordStatus::Used && seen.insert(id)
                        }) else {
                            return;
                        };
                        starts.push((id, start + own.chars().count()));
                        own.push_str(&compact(&w.text));
                    }
                    if own != compact(text) {
                        return;
                    }
                    for (id, start) in starts {
                        if out.insert((pi, id), (field, start)).is_some() {
                            duplicates.insert((pi, id));
                        }
                    }
                };
                let start = streams[index(field)].chars().count();
                if let Some(table) = &line.table {
                    let mut cells = table.cells.iter().collect::<Vec<_>>();
                    cells.sort_by_key(|c| (c.row, c.col));
                    if compact(&line.text)
                        == compact(&cells.iter().map(|c| c.text.as_str()).collect::<String>())
                    {
                        let mut position = start;
                        for cell in cells {
                            part(&cell.text, &cell.words, position);
                            position += compact(&cell.text).chars().count();
                        }
                    }
                } else {
                    part(&line.text, &line.words, start);
                }
                streams[index(field)].push_str(&compact(&line.text));
            }
        }
    }
    for key in duplicates {
        out.remove(&key);
    }
    (streams, out)
}

#[derive(Debug, Clone)]
struct Part {
    id: usize,
    start: usize,
    text: String,
    basis: Basis,
    bounds: Rect,
    baseline: f64,
    center: f64,
}
fn parts(
    word: &crate::trace::Word,
    ids: &[usize],
    source: &kanpo_pdf::Aj1PageSources,
) -> Option<Vec<Part>> {
    let raw = word.raw.chars().collect::<Vec<_>>();
    if raw.len() != ids.len() || ids.iter().collect::<BTreeSet<_>>().len() != ids.len() {
        return None;
    }
    let mut out = Vec::new();
    let mut text = String::new();
    for (&id, c) in ids.iter().zip(raw) {
        let g = source.page().glyphs.get(id)?;
        if g.invisible
            || !positive(g.bbox)
            || g.text.chars().ne(std::iter::once(c))
            || c == '\u{fffd}'
        {
            return None;
        }
        let basis = Basis::glyph(g, source.glyph_text_axes(id)?)?;
        let value = compact(&crate::normalize::word_text(&c.to_string()));
        if value.is_empty() {
            return None;
        }
        let origin = basis.point([g.x, g.y]);
        let end = basis.point([g.end_x, g.end_y]);
        out.push(Part {
            id,
            start: text.chars().count(),
            text: value.clone(),
            basis,
            bounds: basis.rect(g.bbox),
            baseline: origin[0],
            center: (origin[1] + end[1]) / 2.,
        });
        text.push_str(&value);
    }
    (text == compact(&word.text)).then_some(out)
}
fn parent(ruby: &[Part], body: &[Part]) -> Option<(usize, usize, Vec<usize>, String)> {
    let first = ruby.first()?;
    if ruby
        .iter()
        .any(|r| r.basis != first.basis || r.baseline != first.baseline)
    {
        return None;
    }
    let rb = ruby.iter().map(|r| r.bounds).reduce(union)?;
    let cross = rb[2] - rb[0];
    let selected = body
        .iter()
        .enumerate()
        .filter(|(_, b)| {
            b.basis == first.basis
                && rb[1] <= b.center
                && b.center < rb[3]
                && cross < b.bounds[2] - b.bounds[0]
                && b.bounds[2] < first.baseline
                && b.baseline <= rb[0]
                && first.baseline - b.bounds[2] < b.bounds[2] - b.bounds[0]
        })
        .collect::<Vec<_>>();
    let (start, first_body) = *selected.first()?;
    let (end, _) = *selected.last()?;
    if selected.len() != end - start + 1
        || selected
            .iter()
            .any(|(_, b)| b.baseline != first_body.baseline)
    {
        return None;
    }
    Some((
        first_body.start,
        selected.iter().map(|(_, b)| b.text.chars().count()).sum(),
        selected.iter().map(|(_, b)| b.id).collect(),
        selected.iter().map(|(_, b)| b.text.as_str()).collect(),
    ))
}

pub(super) fn analyze(
    analysis: &crate::Analysis,
    documents: &[Document],
    sources: &[kanpo_pdf::XObjectPageSources],
) -> Audit {
    let (streams, offsets) = offsets(analysis);
    let mut starts = Vec::new();
    let mut rendered: [String; 3] = Default::default();
    for document in documents {
        starts.push(rendered.each_ref().map(|s| s.chars().count()));
        for field in [Field::Paragraph, Field::After, Field::Before] {
            model::visit(document, field, &mut |runs| {
                rendered[index(field)].push_str(&model::text(runs))
            });
        }
    }
    let mut bindings = Vec::new();
    let mut unresolved = Vec::new();
    for (pi, (page, source)) in analysis.pages.iter().zip(sources).enumerate() {
        let mapped = occurrences(page, source.page());
        let candidates = page
            .words
            .iter()
            .filter(|w| w.status == WordStatus::Used)
            .filter_map(|w| Some((w.id, parts(w, mapped.get(&w.id)?, source.native_sources())?)))
            .collect::<Vec<_>>();
        for word in page.words.iter().filter(|w| w.status == WordStatus::Ruby) {
            let result = (|| -> Option<Binding> {
                let ids = mapped.get(&word.id)?;
                let reading = parts(word, ids, source.native_sources())?;
                let parents = candidates
                    .iter()
                    .filter_map(|(id, body)| parent(&reading, body).map(|p| (*id, p)))
                    .collect::<Vec<_>>();
                if parents.len() != 1 {
                    return None;
                }
                let (parent_word, (start, length, parent_glyphs, text)) =
                    parents.into_iter().next()?;
                let &(field, word_offset) = offsets.get(&(pi, parent_word))?;
                if streams[index(field)] != rendered[index(field)] {
                    return None;
                }
                let offset = word_offset + start;
                let targets = documents
                    .iter()
                    .enumerate()
                    .filter_map(|(di, document)| {
                        let mut text = String::new();
                        model::visit(document, field, &mut |runs| {
                            text.push_str(&model::text(runs))
                        });
                        let base = starts[di][index(field)];
                        (base <= offset
                            && offset + length <= base + text.chars().count()
                            && length > 0)
                            .then(|| (di, offset - base))
                    })
                    .collect::<Vec<_>>();
                if targets.len() != 1 {
                    return None;
                }
                let (document, offset) = targets[0];
                Some(Binding {
                    source_document_sha256: source
                        .document_sha256()
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect(),
                    document,
                    field,
                    offset,
                    length,
                    text,
                    reading: compact(&word.text),
                    page: pi,
                    ruby_word: word.id,
                    parent_word,
                    ruby_glyphs: ids.clone(),
                    parent_glyphs,
                })
            })();
            if let Some(binding) = result {
                bindings.push(binding);
            } else {
                unresolved.push(Unresolved{page:pi,word:word.id,reason:"source ruby/parent geometry, unique character ownership or complete field correspondence unavailable".into()});
            }
        }
    }
    let conflicts = bindings
        .iter()
        .enumerate()
        .filter_map(|(i, a)| {
            bindings
                .iter()
                .enumerate()
                .any(|(j, b)| {
                    i != j
                        && a.document == b.document
                        && a.field == b.field
                        && a.offset < b.offset + b.length
                        && b.offset < a.offset + a.length
                })
                .then_some(i)
        })
        .collect::<BTreeSet<_>>();
    let mut i = 0;
    bindings.retain(|b| {
        let reject = conflicts.contains(&i);
        i += 1;
        if reject {
            unresolved.push(Unresolved {
                page: b.page,
                word: b.ruby_word,
                reason: "competing ruby annotations overlap the same original parent characters"
                    .into(),
            });
        }
        !reject
    });
    Audit {
        document_sha256: sources
            .first()
            .filter(|first| {
                sources
                    .iter()
                    .all(|p| p.document_sha256() == first.document_sha256())
            })
            .map(|p| {
                p.document_sha256()
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect()
            }),
        bindings,
        unresolved,
        coverage_verified: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn basis() -> Basis {
        let g = kanpo_pdf::Glyph {
            text: "x".into(),
            code: 0,
            cid: 0,
            font: 0,
            x: 0.,
            y: 0.,
            end_x: 0.,
            end_y: 8.,
            size: 8.,
            vertical: true,
            bbox: [-4., 0., 4., 8.],
            invisible: false,
            spacing: 0.,
            kern: 0.,
            show: 0,
        };
        Basis::glyph(
            &g,
            kanpo_pdf::SourceTextAxes {
                x: [1, 0],
                y: [0, -1],
            },
        )
        .unwrap()
    }
    fn part(id: usize, start: usize, text: &str, bounds: Rect, baseline: f64) -> Part {
        Part {
            id,
            start,
            text: text.into(),
            basis: basis(),
            bounds,
            baseline,
            center: (bounds[1] + bounds[3]) / 2.,
        }
    }
    #[test]
    fn geometry_supports_arbitrary_parent_text_and_reading_lengths_scales_and_origins() {
        for count in [1, 2, 7] {
            for scale in [0.125, 1., 64.] {
                for origin in [-128., 0., 256.] {
                    let b = |v: Rect| v.map(|n| n * scale + origin);
                    let parent_parts = (0..count)
                        .map(|i| {
                            part(
                                i,
                                i,
                                "任",
                                b([6., 20. + 8. * i as f64, 14., 28. + 8. * i as f64]),
                                10. * scale + origin,
                            )
                        })
                        .collect::<Vec<_>>();
                    let ruby_parts = (0..count * 2)
                        .map(|i| {
                            part(
                                100 + i,
                                i,
                                "ら",
                                b([14., 20. + 4. * i as f64, 18., 24. + 4. * i as f64]),
                                16. * scale + origin,
                            )
                        })
                        .collect::<Vec<_>>();
                    let actual = parent(&ruby_parts, &parent_parts).unwrap();
                    assert_eq!(actual, (0, count, (0..count).collect(), "任".repeat(count)));
                }
            }
        }
    }
    #[test]
    fn wrong_side_same_sized_disjoint_and_noncontiguous_candidates_are_rejected() {
        let body = vec![part(1, 0, "任", [6., 20., 14., 28.], 10.)];
        let r = part(2, 0, "ら", [14., 20., 18., 28.], 16.);
        assert!(parent(&[r.clone()], &body).is_some());
        for bounds in [
            [-2., 20., 2., 28.],
            [14., 20., 22., 28.],
            [14., 40., 18., 48.],
            [30., 20., 34., 28.],
        ] {
            let mut wrong = r.clone();
            wrong.bounds = bounds;
            wrong.baseline = (bounds[0] + bounds[2]) / 2.;
            assert!(parent(&[wrong], &body).is_none());
        }
        let mut middle = body[0].clone();
        middle.center = 100.;
        let mut last = body[0].clone();
        last.id = 8;
        assert!(parent(&[r], &[body[0].clone(), middle, last]).is_none());
    }

    #[test]
    fn complete_field_and_original_occurrence_are_required_even_when_parent_context_still_matches()
    {
        let pdf = include_bytes!("../tests/data/workbook/case-11.pdf");
        let mut analysis = crate::analyze_item(&crate::page_inputs(pdf).unwrap());
        let sources = kanpo_pdf::extract_pages_with_xobject_sources(pdf).unwrap();
        let mut documents = analysis
            .articles
            .iter()
            .map(|a| Document::from(&a.document))
            .collect::<Vec<_>>();
        let good = analyze(&analysis, &documents, &sources);
        assert_eq!(good.bindings.len(), 2);
        let binding = good
            .bindings
            .iter()
            .find(|b| b.field == Field::After)
            .unwrap();
        // The real parent's text and all source coordinates are still present.
        // Changing a distant part of the field must prevent attachment too.
        if let super::super::Block::Shinkyu { rows, .. } = &mut documents[binding.document]
            .blocks
            .iter_mut()
            .find(|b| matches!(b, super::super::Block::Shinkyu { .. }))
            .unwrap()
        {
            rows.last_mut()
                .unwrap()
                .after
                .push(super::super::Inline::Text {
                    text: "追加の文字".into(),
                    underline: false,
                });
        }
        let changed = analyze(&analysis, &documents, &sources);
        assert_eq!(changed.bindings.len(), 1);
        assert_eq!(changed.bindings[0].field, Field::Before);
        assert_eq!(changed.unresolved.len(), 1);
        let before = good
            .bindings
            .iter()
            .find(|b| b.field == Field::Before)
            .unwrap();
        analysis.pages[before.page].words[before.ruby_word].raw = "別の原文".into();
        let changed = analyze(&analysis, &documents, &sources);
        assert!(changed.bindings.is_empty());
        assert_eq!(changed.unresolved.len(), 2);
    }

    #[test]
    fn separate_original_pages_keep_ruby_in_their_own_article_across_later_field_boundaries() {
        let mut inputs = Vec::new();
        let mut sources = Vec::new();
        for pdf in [
            include_bytes!("../tests/data/workbook/case-10-listed-1.pdf").as_slice(),
            include_bytes!("../tests/data/workbook/case-10-listed-2.pdf").as_slice(),
            include_bytes!("../tests/data/workbook/case-10-listed-3.pdf").as_slice(),
        ] {
            inputs.extend(crate::page_inputs(pdf).unwrap());
            sources.extend(kanpo_pdf::extract_pages_with_xobject_sources(pdf).unwrap());
        }
        let analysis = crate::analyze_item(&inputs);
        let documents = analysis
            .articles
            .iter()
            .map(|a| Document::from(&a.document))
            .collect::<Vec<_>>();
        let audit = analyze(&analysis, &documents, &sources);
        assert!(audit.unresolved.is_empty(), "{:?}", audit.unresolved);
        assert!(audit.document_sha256.is_none());
        assert_eq!(audit.bindings.len(), 10);
        for binding in &audit.bindings {
            let stream = documents[binding.document].field_text(binding.field);
            assert_eq!(
                stream
                    .chars()
                    .skip(binding.offset)
                    .take(binding.length)
                    .collect::<String>(),
                binding.text
            );
            assert_eq!(binding.page, 2);
            assert_eq!(
                binding.source_document_sha256,
                sources[2]
                    .document_sha256()
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            );
        }
        let selected = crate::pick_article(
            "使用薬剤の薬価（薬価基準）等の一部を改正する告示",
            &analysis.articles,
        )
        .unwrap();
        let own = audit
            .bindings
            .iter()
            .filter(|b| b.document == selected)
            .collect::<Vec<_>>();
        assert_eq!(own.len(), 2);
        assert!(own.iter().all(|b| b.text == "歇" && b.reading == "けつ"));
        assert_eq!(
            audit
                .bindings
                .iter()
                .filter(|b| b.document != selected)
                .count(),
            8
        );
    }
}
