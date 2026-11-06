//! Bind complete source shifted Words to final document scalar positions.
use super::{Document, Field};
use crate::source_baseline_shifts::SourceRun;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Binding {
    pub source_document_sha256: String,
    pub page: usize,
    pub document: usize,
    pub field: Field,
    pub offset: usize,
    pub length: usize,
    pub text: String,
    pub source: SourceRun,
}
#[derive(Debug, Clone, Serialize)]
pub struct Unresolved {
    pub page: usize,
    pub source: SourceRun,
    pub reason: &'static str,
}
#[derive(Debug, Clone, Serialize)]
pub struct Audit {
    pub scope: &'static str,
    pub bindings: Vec<Binding>,
    pub unresolved: Vec<Unresolved>,
    pub coverage_verified: bool,
}
fn index(field: Field) -> usize {
    match field {
        Field::Paragraph => 0,
        Field::After => 1,
        Field::Before => 2,
    }
}
pub(super) fn analyze(
    base: &crate::Analysis,
    documents: &[Document],
    sources: &[kanpo_pdf::XObjectPageSources],
) -> Audit {
    let (streams, offsets) = super::ruby::offsets(base);
    let values = [Field::Paragraph, Field::After, Field::Before].map(|field| {
        documents
            .iter()
            .map(|d| d.field_text(field))
            .collect::<Vec<_>>()
    });
    let mut audit = Audit {
        scope: "strict_bracketed_source_baseline_shifts",
        bindings: vec![],
        unresolved: vec![],
        coverage_verified: false,
    };
    for (page, (trace, source)) in base.pages.iter().zip(sources).enumerate() {
        for run in crate::source_baseline_shifts::for_page(trace, source.native_sources()) {
            let target = (|| {
                let &(field, start) = offsets.get(&(page, run.word))?;
                let i = index(field);
                let text = trace.words[run.word]
                    .text
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect::<String>();
                let length = text.chars().count();
                let end = start.checked_add(length)?;
                if length == 0
                    || streams[i] != values[i].concat()
                    || streams[i] != crate::source_underlines::field_text(base, field)
                {
                    return None;
                }
                let mut cursor = 0;
                let mut targets = Vec::new();
                for (document, value) in values[i].iter().enumerate() {
                    let next = cursor + value.chars().count();
                    if cursor <= start && end <= next && start < next {
                        let offset = start - cursor;
                        if value.chars().skip(offset).take(length).collect::<String>() == text {
                            targets.push((document, offset));
                        }
                    }
                    cursor = next;
                }
                let [(document, offset)] = targets.as_slice() else {
                    return None;
                };
                Some((*document, field, *offset, length, text))
            })();
            if let Some((document, field, offset, length, text)) = target {
                audit.bindings.push(Binding {
                    source_document_sha256: super::metadata::hex(&source.document_sha256()),
                    page,
                    document,
                    field,
                    offset,
                    length,
                    text,
                    source: run,
                });
            } else {
                audit.unresolved.push(Unresolved{page,source:run,reason:"complete original and final fields or unique source Word/article range differ"});
            }
        }
    }
    audit
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::{Block, Inline, Row};
    #[test]
    fn whole_field_changes_and_article_boundaries_refuse_shifted_ranges() {
        let pdf = include_bytes!("../tests/data/workbook/case-09.pdf");
        let base = crate::analyze_item(&crate::page_inputs(pdf).unwrap());
        let sources = kanpo_pdf::extract_pages_with_xobject_sources(pdf).unwrap();
        let original = base
            .articles
            .iter()
            .map(|a| Document::from(&a.document))
            .collect::<Vec<_>>();
        let good = analyze(&base, &original, &sources);
        assert_eq!(good.bindings.len(), 3);
        let mut changed = original.clone();
        changed[0].blocks.push(Block::Shinkyu {
            labels: None,
            rows: vec![Row {
                after: vec![Inline::Text {
                    text: "unrelated tail mutation".into(),
                    underline: false,
                }],
                before: vec![],
                after_table: None,
                before_table: None,
            }],
        });
        let bad = analyze(&base, &changed, &sources);
        assert_eq!(bad.bindings.len(), 1);
        assert_eq!(bad.unresolved.len(), 2);
        assert_eq!(bad.bindings[0].field, Field::Before);
        let split = good
            .bindings
            .iter()
            .find(|b| b.field == Field::After)
            .unwrap()
            .offset
            + 1;
        let after = crate::source_underlines::field_text(&base, Field::After)
            .chars()
            .collect::<Vec<_>>();
        let before = crate::source_underlines::field_text(&base, Field::Before);
        let paragraph = crate::source_underlines::field_text(&base, Field::Paragraph);
        let text = |s: String| {
            vec![Inline::Text {
                text: s,
                underline: false,
            }]
        };
        let documents = [
            (after[..split].iter().collect::<String>(), before, paragraph),
            (
                after[split..].iter().collect::<String>(),
                String::new(),
                String::new(),
            ),
        ]
        .into_iter()
        .map(|(a, b, p)| Document {
            format: "shinkyu".into(),
            blocks: vec![
                Block::Paragraph { runs: text(p) },
                Block::Shinkyu {
                    labels: None,
                    rows: vec![Row {
                        after: text(a),
                        before: text(b),
                        after_table: None,
                        before_table: None,
                    }],
                },
            ],
        })
        .collect::<Vec<_>>();
        let bad = analyze(&base, &documents, &sources);
        assert_eq!(bad.bindings.len(), 2);
        assert_eq!(bad.unresolved.len(), 1);
        assert_eq!(bad.unresolved[0].source.glyphs, vec![1290, 1291]);
    }
}
