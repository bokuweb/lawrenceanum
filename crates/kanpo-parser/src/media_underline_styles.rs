//! Parallel underline annotations use complete source field streams. Article
//! boundaries come from final documents, never a substring/expected-text search.
use super::{Document, Field};
use crate::source_underlines::{ParallelUnderline, SourceWord};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Binding {
    pub document: usize,
    pub field: Field,
    pub offset: usize,
    pub length: usize,
    pub parallel_strokes: usize,
    pub source_word: SourceWord,
    pub source_document_sha256: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Unresolved {
    pub annotation: ParallelUnderline,
    pub reason: &'static str,
}

fn split(
    span: &ParallelUnderline,
    values: &[String],
    end: usize,
    source_document_sha256: &str,
) -> Vec<Binding> {
    let mut bindings = Vec::new();
    let mut base = 0;
    for (document, text) in values.iter().enumerate() {
        let next = base + text.chars().count();
        let from = span.offset.max(base);
        let until = end.min(next);
        if from < until {
            bindings.push(Binding {
                document,
                field: span.field,
                offset: from - base,
                length: until - from,
                parallel_strokes: span.parallel_strokes,
                source_word: span.source_word,
                source_document_sha256: source_document_sha256.into(),
            });
        }
        base = next;
    }
    bindings
}

pub(super) fn bind(
    spans: &[ParallelUnderline],
    native: &crate::Analysis,
    documents: &[Document],
    fields: &[Field],
    sources: &[kanpo_pdf::XObjectPageSources],
) -> (Vec<Binding>, Vec<Unresolved>) {
    let mut bindings = Vec::new();
    let mut unresolved = Vec::new();
    for field in [Field::Paragraph, Field::After, Field::Before] {
        let source_text = crate::source_underlines::field_text(native, field);
        let values = documents
            .iter()
            .map(|d| d.field_text(field))
            .collect::<Vec<_>>();
        let rendered = values.concat();
        let own = spans
            .iter()
            .filter(|s| s.field == field)
            .collect::<Vec<_>>();
        for span in &own {
            let end = span.offset.checked_add(span.length);
            let valid_word = native
                .pages
                .get(span.source_word.page)
                .and_then(|p| p.words.get(span.source_word.word))
                .is_some_and(|w| {
                    w.id == span.source_word.word && w.status == crate::trace::WordStatus::Used
                });
            let overlaps = own
                .iter()
                .filter(|other| {
                    other.offset.checked_add(other.length).zip(end).is_some_and(
                        |(other_end, end)| span.offset < other_end && other.offset < end,
                    )
                })
                .count()
                > 1;
            let reason = if !fields.contains(&field) || source_text != rendered {
                Some("complete original and final document fields differ or source style field was not applied")
            } else if span.length == 0
                || span.parallel_strokes < 2
                || end.is_none_or(|end| end > rendered.chars().count())
                || !valid_word
                || span.source_word.page >= sources.len()
            {
                Some("original source Word or complete field range is unavailable")
            } else if overlaps {
                Some("competing parallel underline source ranges")
            } else {
                None
            };
            if let Some(reason) = reason {
                unresolved.push(Unresolved {
                    annotation: (*span).clone(),
                    reason,
                });
                continue;
            }
            let end = end.unwrap();
            let source_document_sha256 =
                super::metadata::hex(&sources[span.source_word.page].document_sha256());
            bindings.extend(split(span, &values, end, &source_document_sha256));
        }
    }
    (bindings, unresolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn final_article_boundaries_and_non_bmp_scalars_do_not_reuse_old_article_offsets() {
        let span = ParallelUnderline {
            field: Field::Paragraph,
            offset: 1,
            length: 3,
            parallel_strokes: 2,
            source_word: SourceWord { page: 4, word: 17 },
        };
        let bound = split(
            &span,
            &["甲𠀀".into(), "乙丁戊".into()],
            4,
            "original source identity",
        );
        assert_eq!(
            bound
                .iter()
                .map(|b| (b.document, b.offset, b.length))
                .collect::<Vec<_>>(),
            vec![(0, 1, 1), (1, 0, 2)]
        );
        assert!(bound
            .iter()
            .all(|b| b.parallel_strokes == 2 && b.source_word == span.source_word));
    }
    #[test]
    fn whole_field_mutation_or_competing_ranges_rejects_annotations_without_text_search() {
        let pdf = include_bytes!("../tests/data/workbook/case-11.pdf");
        let styled = crate::source_underlines::analyze_pdf(pdf).unwrap();
        let sources = kanpo_pdf::extract_pages_with_xobject_sources(pdf).unwrap();
        let documents = styled
            .styled
            .articles
            .iter()
            .map(|a| Document::from(&a.document))
            .collect::<Vec<_>>();
        let (bound, pending) = bind(
            &styled.parallel_underlines,
            &styled.native,
            &documents,
            &styled.applied_fields,
            &sources,
        );
        assert_eq!(bound.len(), 5);
        assert!(pending.is_empty());
        let mut changed = documents.clone();
        let mut count = 0;
        super::super::model::visit(&changed[0], Field::After, &mut |_| count += 1);
        let mut index = 0;
        super::super::model::visit_mut(&mut changed[0], Field::After, &mut |runs| {
            index += 1;
            if index == count {
                runs.push(super::super::Inline::Text {
                    text: "foreign tail".into(),
                    underline: false,
                });
            }
        });
        let (bound, pending) = bind(
            &styled.parallel_underlines,
            &styled.native,
            &changed,
            &styled.applied_fields,
            &sources,
        );
        assert!(bound.iter().all(|b| b.field == Field::Before));
        assert_eq!(pending.len(), 3);
        let mut competing = styled.parallel_underlines.clone();
        let mut other = competing[0].clone();
        other.parallel_strokes = 3;
        competing.push(other);
        let (bound, pending) = bind(
            &competing,
            &styled.native,
            &documents,
            &styled.applied_fields,
            &sources,
        );
        assert_eq!(bound.len(), 4);
        assert_eq!(pending.len(), 2);
        let (bound, pending) = bind(
            &styled.parallel_underlines,
            &styled.native,
            &documents,
            &[Field::Before],
            &sources,
        );
        assert_eq!(bound.len(), 2);
        assert_eq!(pending.len(), 3);
    }
}
