//! Reassign existing paragraph Runs to proven article partitions. Every output
//! text byte and complete table comes from an original Run; no rerendering.
use crate::model::{Block, Document, NestedTable, Run};
#[derive(Clone)]
enum Wanted {
    Text(String),
    Table(NestedTable),
}
fn wanted(d: &Document) -> Option<Vec<Wanted>> {
    let mut out = Vec::new();
    for block in &d.blocks {
        let Block::Paragraph { runs } = block else {
            return None;
        };
        for r in runs {
            if let Some(t) = &r.table {
                if !r.text.is_empty() {
                    return None;
                }
                out.push(Wanted::Table(t.clone()));
            } else {
                let text: String = r.text.chars().filter(|c| *c != '\n').collect();
                if !text.is_empty() {
                    if let Some(Wanted::Text(last)) = out.last_mut() {
                        last.push_str(&text);
                    } else {
                        out.push(Wanted::Text(text));
                    }
                }
            }
        }
    }
    Some(out)
}
fn append(out: &mut Document, last: &mut Option<usize>, block: usize, run: Run) {
    if *last != Some(block) {
        out.blocks.push(Block::Paragraph { runs: vec![] });
        *last = Some(block);
    }
    let Some(Block::Paragraph { runs }) = out.blocks.last_mut() else {
        unreachable!()
    };
    runs.push(run);
}
struct Source {
    runs: Vec<(usize, Run)>,
    index: usize,
    offset: usize,
}
impl Source {
    fn text(&mut self, expected: &str, out: &mut Document, last: &mut Option<usize>) -> Option<()> {
        let mut chars = expected.chars().peekable();
        while let Some((bi, r)) = self.runs.get(self.index) {
            if r.table.is_some() {
                break;
            }
            let mut end = self.offset;
            for c in r.text[self.offset..].chars() {
                if c != '\n' {
                    let Some(want) = chars.peek() else {
                        break;
                    };
                    if *want != c {
                        return None;
                    }
                    chars.next();
                }
                end += c.len_utf8();
            }
            if end > self.offset || r.text.is_empty() {
                let mut piece = r.clone();
                piece.text = r.text[self.offset..end].to_string();
                append(out, last, *bi, piece);
            }
            self.offset = end;
            if self.offset == r.text.len() {
                self.index += 1;
                self.offset = 0;
            } else {
                break;
            }
        }
        if chars.next().is_none() {
            Some(())
        } else {
            None
        }
    }
    fn table(
        &mut self,
        expected: &NestedTable,
        out: &mut Document,
        last: &mut Option<usize>,
    ) -> Option<()> {
        self.text("", out, last)?;
        let (bi, r) = self.runs.get(self.index)?;
        if self.offset != 0 || !r.text.is_empty() || r.table.as_ref() != Some(expected) {
            return None;
        }
        append(out, last, *bi, r.clone());
        self.index += 1;
        Some(())
    }
}
/// Alignment may ignore renderer-added/removed LF for matching only. The output
/// always copies original Run slices, retaining every LF, ASCII space, U+3000,
/// style, paragraph transition, and table byte-for-byte in the original order.
pub(crate) fn repartition(original: &Document, desired: &[Document]) -> Option<Vec<Document>> {
    if desired.is_empty() {
        return None;
    }
    let mut source = Source {
        runs: vec![],
        index: 0,
        offset: 0,
    };
    for (bi, b) in original.blocks.iter().enumerate() {
        let Block::Paragraph { runs } = b else {
            return None;
        };
        if runs.is_empty() {
            return None;
        }
        for r in runs {
            if r.table.is_some() && !r.text.is_empty() {
                return None;
            }
            source.runs.push((bi, r.clone()));
        }
    }
    let mut docs = Vec::new();
    let mut last = None;
    for d in desired {
        let mut out = Document {
            format: original.format.clone(),
            blocks: vec![],
        };
        last = None;
        for token in wanted(d)? {
            match token {
                Wanted::Text(t) => source.text(&t, &mut out, &mut last)?,
                Wanted::Table(t) => source.table(&t, &mut out, &mut last)?,
            }
        }
        docs.push(out);
    }
    source.text("", docs.last_mut()?, &mut last)?;
    if source.index != source.runs.len() || source.offset != 0 {
        return None;
    }
    Some(docs)
}
#[cfg(test)]
#[path = "company_document_parts_tests.rs"]
mod tests;
