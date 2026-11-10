//! Retain certified double underlines without changing native text or public Run.
use anyhow::{ensure, Result};
use kanpo_parser::{
    media::{Document, Inline, Table},
    source_underlines::Field,
    trace::WordStatus,
};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Underline {
    None,
    Single,
    Double,
}
impl Underline {
    pub fn value(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Single => Some("single"),
            Self::Double => Some("double"),
        }
    }
}
#[derive(Default)]
pub(crate) struct SourceStyles {
    base_font: u16,
    fields: [Option<SourceField>; 3],
    annotations: crate::annotations::SourceAnnotations,
    borders: std::rc::Rc<crate::borders::Borders>,
}
struct SourceField {
    expected: String,
    double: Vec<bool>,
}
struct FieldStyle {
    expected: String,
    double: Vec<bool>,
    observed: String,
    annotations: Vec<crate::annotations::Mark>,
    position: usize,
    invalid_transform: bool,
}
pub(crate) struct Styles {
    base_font: u16,
    fields: [Option<RefCell<FieldStyle>>; 3],
    annotations: crate::annotations::Writer,
    borders: std::rc::Rc<crate::borders::Borders>,
    /// Unverified readings of painted characters, keyed by (field, scalar
    /// position of the source character they precede).
    insertions: RefCell<std::collections::BTreeMap<(usize, usize), String>>,
    comments: std::cell::Cell<usize>,
}
const FIELDS: [Field; 3] = [Field::Paragraph, Field::After, Field::Before];
fn index(f: Field) -> usize {
    match f {
        Field::Paragraph => 0,
        Field::After => 1,
        Field::Before => 2,
    }
}
impl SourceStyles {
    pub fn default_font(&self) -> usize {
        if self.base_font == 0 {
            21
        } else {
            self.base_font as usize
        }
    }
    pub fn from_media(source: &kanpo_parser::media::MediaAnalysis, pdf: &[u8]) -> Result<Self> {
        let mut out = Self::default();
        out.annotations = crate::annotations::SourceAnnotations::new(source)?;
        let mut sizes = BTreeMap::<u16, usize>::new();
        for b in &source.fonts.bindings {
            if let Some(size) = crate::annotations::half_points(b.points) {
                *sizes.entry(size).or_default() += b.length;
            }
        }
        // Only a document fallback: every certified native scalar keeps its
        // own explicit size. Unknown/OCR text uses the predominant source size.
        out.base_font = sizes
            .into_iter()
            .max_by_key(|&(size, count)| (count, size))
            .map_or(0, |(size, _)| size);
        out.borders = std::rc::Rc::new(crate::borders::Borders::from_media(pdf, source)?);
        for field in FIELDS {
            if !source.underlines.applied_fields.contains(&field) {
                continue;
            }
            let spans = source
                .underlines
                .parallel_bindings
                .iter()
                .filter(|s| s.field == field && s.parallel_strokes == 2)
                .collect::<Vec<_>>();
            let expected = source
                .documents
                .iter()
                .map(|d| d.field_text(field))
                .collect::<String>();
            let mut double = vec![false; expected.chars().count()];
            let mut valid = true;
            for span in spans {
                let Some(range) = crate::annotations::range(
                    &source.documents,
                    span.document,
                    field,
                    span.offset,
                    span.length,
                )
                .filter(|r| r.end <= double.len()) else {
                    valid = false;
                    break;
                };
                let word = source
                    .source_layout
                    .pages
                    .get(span.source_word.page)
                    .and_then(|p| p.words.get(span.source_word.word));
                if span.length == 0
                    || word.is_none_or(|w| {
                        w.id != span.source_word.word || w.status != WordStatus::Used
                    })
                    || double[range.clone()].iter().any(|&b| b)
                {
                    valid = false;
                    break;
                }
                double[range].fill(true);
            }
            if valid {
                out.fields[index(field)] = Some(SourceField { expected, double });
            }
        }
        Ok(out)
    }
    pub fn rebind_fonts(
        &mut self,
        pdf: &[u8],
        analysis: &kanpo_parser::Analysis,
        documents: &[Document],
        source: &kanpo_parser::media::MediaAnalysis,
    ) -> Result<()> {
        if FIELDS.iter().any(|&f| {
            documents
                .iter()
                .map(|d| d.field_text(f))
                .collect::<String>()
                != source
                    .documents
                    .iter()
                    .map(|d| d.field_text(f))
                    .collect::<String>()
        }) {
            let fonts = kanpo_parser::media::fonts::analyze_documents(pdf, analysis, documents)?;
            self.annotations.rebind_fonts(documents, &fonts)?;
        }
        Ok(())
    }
    pub fn bind(&self, documents: &[Document]) -> Styles {
        Styles {
            base_font: self.base_font,
            insertions: RefCell::default(),
            comments: std::cell::Cell::new(0),
            borders: self.borders.clone(),
            annotations: self.annotations.writer(),
            fields: std::array::from_fn(|i| {
                let actual = documents
                    .iter()
                    .map(|d| d.field_text(FIELDS[i]))
                    .collect::<String>();
                let double = self.fields[i]
                    .as_ref()
                    .filter(|source| source.expected == actual)
                    .map_or_else(
                        || vec![false; actual.chars().count()],
                        |source| source.double.clone(),
                    );
                Some(RefCell::new(FieldStyle {
                    annotations: self.annotations.marks(FIELDS[i], &actual),
                    expected: actual,
                    double,
                    observed: String::new(),
                    position: 0,
                    invalid_transform: false,
                }))
            }),
        }
    }
}
impl Styles {
    pub fn default_font(&self) -> usize {
        if self.base_font == 0 {
            21
        } else {
            self.base_font as usize
        }
    }
    pub fn table_borders(
        &self,
        t: &kanpo_parser::model::NestedTable,
        field: Field,
        physical: bool,
    ) -> crate::borders::CellPlan {
        self.borders.table(t, field, physical)
    }
    /// Permute source scalars by the same certified cell map as an image table.
    /// This is positional identity, including repeated/identical cell strings.
    pub fn remap_table(
        &self,
        field: Field,
        original: &Table,
        physical: &Table,
        map: &[((usize, usize), (usize, usize))],
    ) {
        fn text(items: &[Inline]) -> String {
            items
                .iter()
                .map(|i| match i {
                    Inline::Text { text, .. } => text
                        .chars()
                        .filter(|c| !c.is_whitespace())
                        .collect::<String>(),
                    Inline::Image { .. } => String::new(),
                    Inline::Table { table } => {
                        table.rows.iter().flatten().map(|c| text(c)).collect()
                    }
                })
                .collect()
        }
        let Some(state) = &self.fields[index(field)] else {
            return;
        };
        let mut state = state.borrow_mut();
        let remapped = (|| {
            let expected = state.expected.chars().collect::<Vec<_>>();
            let start = state.position;
            let mut offset = start;
            let mut cells = BTreeMap::new();
            for (r, row) in original.rows.iter().enumerate() {
                for (c, cell) in row.iter().enumerate() {
                    let chars = text(cell).chars().collect::<Vec<_>>();
                    let end = offset.checked_add(chars.len())?;
                    if expected.get(offset..end)? != chars.as_slice() {
                        return None;
                    }
                    cells.insert(
                        (r, c),
                        chars
                            .into_iter()
                            .zip(
                                state
                                    .double
                                    .get(offset..end)?
                                    .iter()
                                    .copied()
                                    .zip(state.annotations.get(offset..end)?.iter().copied()),
                            )
                            .collect::<Vec<_>>(),
                    );
                    offset = end;
                }
            }
            let mut moved = BTreeMap::new();
            let mut seen = BTreeSet::new();
            for &(source, target) in map {
                if !seen.insert(source) || moved.contains_key(&target) {
                    return None;
                }
                moved.insert(target, cells.get(&source)?.clone());
            }
            if cells
                .iter()
                .any(|(key, chars)| !chars.is_empty() && !seen.contains(key))
            {
                return None;
            }
            let mut scalars = Vec::new();
            for (r, row) in physical.rows.iter().enumerate() {
                for (c, cell) in row.iter().enumerate() {
                    let own = moved.remove(&(r, c)).unwrap_or_default();
                    if own.iter().map(|(c, _)| *c).collect::<String>() != text(cell) {
                        return None;
                    }
                    scalars.extend(own);
                }
            }
            if !moved.is_empty() || scalars.len() != offset - start {
                return None;
            }
            Some((start, offset, expected, scalars))
        })();
        let Some((start, end, mut expected, scalars)) = remapped else {
            state.invalid_transform = true;
            return;
        };
        expected.splice(start..end, scalars.iter().map(|(c, _)| *c));
        state.expected = expected.into_iter().collect();
        state
            .double
            .splice(start..end, scalars.iter().map(|(_, (double, _))| *double));
        state.annotations.splice(
            start..end,
            scalars.into_iter().map(|(_, (_, annotation))| annotation),
        );
    }
    /// Readings to insert, keyed by field and source scalar position.
    pub fn set_insertions(&self, insertions: &[(Field, usize, String)]) {
        *self.insertions.borrow_mut() = insertions.iter().map(|(f, p, t)| ((index(*f), *p), t.clone())).collect();
    }
    /// Add the runs for `text` to `p`, placing any pending painted-character
    /// reading right before its source character: highlighted, inside a
    /// comment that says the reading is unverified. The reading is not part of
    /// the certified source field, so field positions are unaffected.
    pub fn add_word_runs(
        &self,
        mut p: docx_rs::Paragraph,
        field: Field,
        text: &str,
        underlined: bool,
        bold: bool,
        scale: f32,
    ) -> docx_rs::Paragraph {
        let mut position = self.fields[index(field)].as_ref().map_or(0, |s| s.borrow().position);
        let mut segment = String::new();
        for c in text.chars() {
            if !c.is_whitespace() {
                let reading = self.insertions.borrow_mut().remove(&(index(field), position));
                if let Some(reading) = reading {
                    // The parser renders the empty-looking seam where the
                    // shape is painted as a space; the reading fills it.
                    let kept = segment.trim_end().len();
                    segment.truncate(kept);
                    for r in self.word_runs_scaled(field, &segment, underlined, bold, scale) {
                        p = p.add_run(r);
                    }
                    segment.clear();
                    let id = self.comments.get() + 1;
                    self.comments.set(id);
                    let note = docx_rs::Comment::new(id).author("kanpo-docx").add_paragraph(
                        docx_rs::Paragraph::new().add_run(docx_rs::Run::new().add_text(
                            "原本では文字ではなく図形で描かれた字を、同じPDFの字形と照合して読み取った候補です（未確認）。原本で確認してください。",
                        )),
                    );
                    let mut run = docx_rs::Run::new().add_text(reading).highlight("yellow");
                    if bold {
                        run = run.bold();
                    }
                    if scale != 1. {
                        run = run.size((self.default_font() as f32 * scale).round().max(1.) as usize);
                    }
                    p = p.add_comment_start(note).add_run(run).add_comment_end(id);
                }
                position += 1;
            }
            segment.push(c);
        }
        for r in self.word_runs_scaled(field, &segment, underlined, bold, scale) {
            p = p.add_run(r);
        }
        p
    }
    pub fn word_runs_scaled(
        &self,
        field: Field,
        text: &str,
        underlined: bool,
        bold: bool,
        scale: f32,
    ) -> Vec<docx_rs::Run> {
        let start = self.fields[index(field)]
            .as_ref()
            .map_or(0, |s| s.borrow().position);
        let chunks = self.runs(field, text, underlined);
        let mut position = start;
        let mut parts: Vec<(String, Underline, crate::annotations::Mark)> = Vec::new();
        for (text, underline) in chunks {
            for c in text.chars() {
                let mark = if c.is_whitespace() {
                    // Spaces inherit neighboring native typography, never
                    // ruby, baseline shifts or the text enclosure.
                    let next = self.fields[index(field)]
                        .as_ref()
                        .and_then(|s| {
                            let s = s.borrow();
                            s.annotations
                                .get(position)
                                .or_else(|| {
                                    position.checked_sub(1).and_then(|i| s.annotations.get(i))
                                })
                                .copied()
                        })
                        .unwrap_or_default();
                    crate::annotations::Mark {
                        font: next.font,
                        family: next.family,
                        weight: next.weight,
                        ..Default::default()
                    }
                } else {
                    let m = self.fields[index(field)]
                        .as_ref()
                        .and_then(|s| s.borrow().annotations.get(position).copied())
                        .unwrap_or_default();
                    position += 1;
                    m
                };
                if let Some((text, _, _)) = parts
                    .last_mut()
                    .filter(|(_, u, m)| *u == underline && *m == mark)
                {
                    text.push(c);
                } else {
                    parts.push((c.to_string(), underline, mark));
                }
            }
        }
        parts
            .into_iter()
            .map(|(text, underline, mark)| {
                let mut r = docx_rs::Run::new().add_text(text);
                if mark.font == 0 && scale != 1. {
                    r = r.size((self.default_font() as f32 * scale).round().max(1.) as usize);
                }
                if bold {
                    r = r.bold();
                }
                if let Some(kind) = underline.value() {
                    r = r.underline(kind);
                }
                self.annotations.decorate_scaled(r, mark, scale)
            })
            .collect()
    }
    pub fn paragraph_font(&self, field: Field, text: &str) -> Option<usize> {
        let state = self.fields[index(field)].as_ref()?.borrow();
        let n = text.chars().filter(|c| !c.is_whitespace()).count();
        let fonts = state
            .annotations
            .get(state.position..state.position.checked_add(n)?)?;
        fonts
            .iter()
            .map(|m| m.font)
            .filter(|n| *n > 0)
            .max()
            .map(usize::from)
    }
    pub fn finish_xml(&self, xml: String) -> Result<String> {
        self.annotations.finish(xml)
    }
    pub fn runs(&self, field: Field, text: &str, underlined: bool) -> Vec<(String, Underline)> {
        let base = if underlined {
            Underline::Single
        } else {
            Underline::None
        };
        let Some(state) = &self.fields[index(field)] else {
            return vec![(text.into(), base)];
        };
        let mut state = state.borrow_mut();
        let mut out: Vec<(String, Underline)> = Vec::new();
        for c in text.chars() {
            let mut style = base;
            if !c.is_whitespace() {
                if state.double.get(state.position) == Some(&true) {
                    style = Underline::Double;
                }
                state.position += 1;
                state.observed.push(c);
            }
            if let Some((text, _)) = out.last_mut().filter(|(_, previous)| *previous == style) {
                text.push(c);
            } else {
                out.push((c.to_string(), style));
            }
        }
        out
    }
    pub fn finish(&self) -> Result<()> {
        for (field, state) in FIELDS.into_iter().zip(&self.fields) {
            if let Some(state) = state {
                let state = state.borrow();
                ensure!(
                    !state.invalid_transform
                        && state.observed == state.expected
                        && state.position == state.double.len(),
                    "certified decoration source field changed while writing {:?}",
                    field
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanpo_parser::media::{Block, Inline};
    fn document(text: &str) -> Document {
        Document {
            format: "prose".into(),
            blocks: vec![Block::Paragraph {
                runs: vec![Inline::Text {
                    text: text.into(),
                    underline: true,
                }],
            }],
        }
    }
    #[test]
    fn scalar_offsets_survive_run_boundaries_and_non_source_spaces() {
        let mut source = SourceStyles::default();
        source.fields[0] = Some(SourceField {
            expected: "甲𠀀乙丁".into(),
            double: vec![false, true, true, false],
        });
        let styles = source.bind(&[document("甲𠀀乙丁")]);
        assert_eq!(
            styles.runs(Field::Paragraph, "甲　𠀀", true),
            vec![
                ("甲　".into(), Underline::Single),
                ("𠀀".into(), Underline::Double)
            ]
        );
        assert_eq!(
            styles.runs(Field::Paragraph, "乙丁", true),
            vec![
                ("乙".into(), Underline::Double),
                ("丁".into(), Underline::Single)
            ]
        );
        styles.finish().unwrap();
        let wrong = source.bind(&[document("甲𠀀乙異")]);
        assert_eq!(
            wrong.runs(Field::Paragraph, "甲𠀀乙異", true),
            vec![("甲𠀀乙異".into(), Underline::Single)]
        );
        wrong.finish().unwrap();
    }
    #[test]
    fn a_writer_reorder_cannot_silently_move_the_original_decoration() {
        let mut source = SourceStyles::default();
        source.fields[0] = Some(SourceField {
            expected: "甲乙".into(),
            double: vec![false, true],
        });
        let styles = source.bind(&[document("甲乙")]);
        styles.runs(Field::Paragraph, "乙甲", true);
        assert!(styles.finish().is_err());
    }

    #[test]
    fn a_certified_cell_permutation_moves_glyph_identity_even_for_identical_strings() {
        let mut source = SourceStyles::default();
        source.fields[0] = Some(SourceField {
            expected: "甲甲".into(),
            double: vec![true, false],
        });
        let styles = source.bind(&[document("甲甲")]);
        let cell = || {
            vec![Inline::Text {
                text: "甲".into(),
                underline: true,
            }]
        };
        let original = Table {
            rows: vec![vec![cell(), cell()]],
            merges: Vec::new(),
        };
        styles.remap_table(
            Field::Paragraph,
            &original,
            &original,
            &[((0, 0), (0, 1)), ((0, 1), (0, 0))],
        );
        assert_eq!(
            styles.runs(Field::Paragraph, "甲甲", true),
            vec![
                ("甲".into(), Underline::Single),
                ("甲".into(), Underline::Double)
            ]
        );
        styles.finish().unwrap();
        let missing = source.bind(&[document("甲甲")]);
        missing.remap_table(Field::Paragraph, &original, &original, &[((0, 0), (0, 1))]);
        missing.runs(Field::Paragraph, "甲甲", true);
        assert!(
            missing.finish().is_err(),
            "incomplete cell maps cannot authorize a source permutation"
        );
    }
}
