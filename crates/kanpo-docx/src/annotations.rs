//! Source-certified ruby, baseline shifts and enclosures, by scalar identity.
use anyhow::{ensure, Result};
use docx_rs::{BuildXML, Run, RunFonts, TextBorder, VertAlignType};
use kanpo_parser::media::{Document, Field, MediaAnalysis};
use std::{cell::RefCell, collections::BTreeMap};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Mark {
    pub font: u16,
    pub family: u8,
    /// 0 unknown/inherited, 1 explicitly normal, 2 explicitly bold.
    pub weight: u8,
    pub ruby: usize,
    pub shift: i8,
    pub border: u8,
}
#[derive(Default)]
pub(crate) struct SourceAnnotations {
    fields: [Option<(String, Vec<Mark>)>; 3],
    rubies: Vec<Ruby>,
}
#[derive(Clone)]
struct Ruby {
    reading: String,
    base: u16,
    size: u16,
    raise: u16,
}
impl From<&str> for Ruby {
    fn from(s: &str) -> Self {
        Self {
            reading: s.into(),
            base: 21,
            size: 11,
            raise: 22,
        }
    }
}
pub(crate) fn half_points(points: f64) -> Option<u16> {
    let n = (points * 2.).round();
    (points.is_finite() && points > 0. && (1.0..=1638.0).contains(&n)).then_some(n as u16)
}
pub(crate) fn index(f: Field) -> usize {
    match f {
        Field::Paragraph => 0,
        Field::After => 1,
        Field::Before => 2,
    }
}
const FIELDS: [Field; 3] = [Field::Paragraph, Field::After, Field::Before];

pub(crate) fn range(
    documents: &[Document],
    document: usize,
    field: Field,
    offset: usize,
    length: usize,
) -> Option<std::ops::Range<usize>> {
    let own = documents.get(document)?.field_text(field);
    let end = offset.checked_add(length)?;
    if length == 0 || end > own.chars().count() {
        return None;
    }
    let start = documents[..document]
        .iter()
        .map(|d| d.field_text(field).chars().count())
        .sum::<usize>()
        + offset;
    Some(start..start.checked_add(length)?)
}
impl SourceAnnotations {
    pub fn new(source: &MediaAnalysis) -> Result<Self> {
        let mut out = Self {
            fields: FIELDS.map(|f| {
                let expected = source
                    .documents
                    .iter()
                    .map(|d| d.field_text(f))
                    .collect::<String>();
                let marks = vec![Mark::default(); expected.chars().count()];
                Some((expected, marks))
            }),
            rubies: Vec::new(),
        };
        let mut put = |document, field, offset, length, text: &str, mark: Mark| -> Result<()> {
            let r = range(&source.documents, document, field, offset, length)
                .ok_or_else(|| anyhow::anyhow!("annotation source range is unavailable"))?;
            let (expected, marks) = out.fields[index(field)].as_mut().unwrap();
            ensure!(
                expected
                    .chars()
                    .skip(r.start)
                    .take(length)
                    .collect::<String>()
                    == text,
                "annotation differs from its complete source field"
            );
            for own in &mut marks[r] {
                ensure!(
                    (mark.ruby == 0 || own.ruby == 0)
                        && (mark.shift == 0 || own.shift == 0)
                        && (mark.border == 0 || own.border == 0)
                        && (mark.font == 0
                            || own.font == 0
                            || own.font == mark.font
                            || mark.shift != 0)
                        && (mark.family == 0 || own.family == 0 || own.family == mark.family)
                        && (mark.weight == 0 || own.weight == 0 || own.weight == mark.weight),
                    "overlapping source annotations"
                );
                if mark.ruby != 0 {
                    own.ruby = mark.ruby;
                }
                if mark.font != 0 {
                    own.font = mark.font;
                }
                if mark.family != 0 {
                    own.family = mark.family;
                }
                if mark.weight != 0 {
                    own.weight = mark.weight;
                }
                if mark.shift != 0 {
                    own.shift = mark.shift;
                }
                if mark.border != 0 {
                    own.border = mark.border;
                }
            }
            Ok(())
        };
        for b in &source.fonts.bindings {
            if let Some(font) = half_points(b.points) {
                put(
                    b.document,
                    b.field,
                    b.offset,
                    b.length,
                    &b.text,
                    Mark {
                        font,
                        ..Default::default()
                    },
                )?;
            }
        }
        for b in &source.fonts.styles {
            put(
                b.document,
                b.field,
                b.offset,
                b.length,
                &b.text,
                style_mark(b.style),
            )?;
        }
        for (id, b) in source.ruby.bindings.iter().enumerate() {
            ensure!(!b.reading.is_empty(), "source ruby has no reading");
            put(
                b.document,
                b.field,
                b.offset,
                b.length,
                &b.text,
                Mark {
                    ruby: id + 1,
                    ..Default::default()
                },
            )?;
        }
        for b in &source.baseline_shifts.bindings {
            let shift = match b.source.shift {
                kanpo_parser::source_baseline_shifts::Shift::Raised => 1,
                kanpo_parser::source_baseline_shifts::Shift::Lowered => -1,
            };
            put(
                b.document,
                b.field,
                b.offset,
                b.length,
                &b.text,
                Mark {
                    shift,
                    // vertAlign scales a parent-size run. Applying the tiny
                    // native exponent font here would shrink it twice.
                    font: half_points(b.source.parent_size).unwrap_or(0),
                    ..Default::default()
                },
            )?;
        }
        for b in &source.frames.bindings {
            let a = b.frame.outer;
            let z = b.frame.inner;
            let thickness = ((z[0] - a[0]) + (z[1] - a[1]) + (a[2] - z[2]) + (a[3] - z[3])) / 4.;
            ensure!(
                thickness.is_finite() && thickness > 0.,
                "invalid source enclosure stroke"
            );
            let border = (thickness * 8.).round().clamp(1., 96.) as u8;
            put(
                b.document,
                b.field,
                b.offset,
                b.length,
                &b.text,
                Mark {
                    border,
                    ..Default::default()
                },
            )?;
        }
        out.rubies = source
            .ruby
            .bindings
            .iter()
            .map(|b| {
                let mut r = Ruby::from(b.reading.as_str());
                if let Some(m) = source
                    .fonts
                    .rubies
                    .iter()
                    .find(|m| m.page == b.page && m.word == b.ruby_word)
                {
                    if let (Some(base), Some(size), Some(raise)) = (
                        half_points(m.base_points),
                        half_points(m.reading_points),
                        half_points(m.raise_points),
                    ) {
                        r.base = base;
                        r.size = size;
                        r.raise = raise;
                    }
                }
                r
            })
            .collect();
        Ok(out)
    }
    /// For changed OCR fields, the final analysis supplies new scalar positions.
    /// Keep only newly certified native sizes; all other annotations retain the
    /// stricter original complete-field gate and cannot be borrowed by spelling.
    pub fn rebind_fonts(
        &mut self,
        documents: &[Document],
        fonts: &kanpo_parser::media::fonts::Audit,
    ) -> Result<()> {
        for field in FIELDS {
            let expected = documents
                .iter()
                .map(|d| d.field_text(field))
                .collect::<String>();
            if self.fields[index(field)]
                .as_ref()
                .is_some_and(|(text, _)| *text == expected)
            {
                continue;
            }
            let mut marks = vec![Mark::default(); expected.chars().count()];
            for b in fonts.bindings.iter().filter(|b| b.field == field) {
                let r = range(documents, b.document, field, b.offset, b.length)
                    .ok_or_else(|| anyhow::anyhow!("final native font range unavailable"))?;
                ensure!(
                    expected
                        .chars()
                        .skip(r.start)
                        .take(b.length)
                        .collect::<String>()
                        == b.text,
                    "final native font source differs"
                );
                let Some(font) = half_points(b.points) else {
                    continue;
                };
                for mark in &mut marks[r] {
                    ensure!(
                        mark.font == 0 || mark.font == font,
                        "conflicting final native font sizes"
                    );
                    mark.font = font;
                }
            }
            for b in fonts.styles.iter().filter(|b| b.field == field) {
                let r = range(documents, b.document, field, b.offset, b.length)
                    .ok_or_else(|| anyhow::anyhow!("final native font style range unavailable"))?;
                ensure!(
                    expected
                        .chars()
                        .skip(r.start)
                        .take(b.length)
                        .collect::<String>()
                        == b.text,
                    "final native font style differs"
                );
                let own = style_mark(b.style);
                for m in &mut marks[r] {
                    ensure!(
                        (m.family == 0 || own.family == 0 || m.family == own.family)
                            && (m.weight == 0 || own.weight == 0 || m.weight == own.weight),
                        "conflicting final native font styles"
                    );
                    if own.family != 0 {
                        m.family = own.family;
                    }
                    if own.weight != 0 {
                        m.weight = own.weight;
                    }
                }
            }
            self.fields[index(field)] = Some((expected, marks));
        }
        Ok(())
    }
    pub fn marks(&self, field: Field, text: &str) -> Vec<Mark> {
        self.fields[index(field)]
            .as_ref()
            .filter(|(expected, _)| expected == text)
            .map_or_else(
                || vec![Mark::default(); text.chars().count()],
                |(_, marks)| marks.clone(),
            )
    }
    pub fn writer(&self) -> Writer {
        Writer {
            rubies: self.rubies.clone(),
            patches: Default::default(),
        }
    }
}

struct Patch {
    ruby: usize,
    marked: String,
    base: String,
    scale: f32,
}
pub(crate) struct Writer {
    rubies: Vec<Ruby>,
    patches: RefCell<Vec<Patch>>,
}
impl Writer {
    pub fn decorate(&self, run: Run, mark: Mark) -> Run {
        self.decorate_scaled(run, mark, 1.)
    }
    pub fn decorate_scaled(&self, mut run: Run, mark: Mark, scale: f32) -> Run {
        assert!(scale.is_finite() && scale > 0.);
        if let Some(family) = match mark.family {
            1 => Some("ＭＳ 明朝"),
            2 => Some("ＭＳ ゴシック"),
            _ => None,
        } {
            run = run.fonts(
                RunFonts::new()
                    .ascii(family)
                    .east_asia(family)
                    .hi_ansi(family),
            );
        }
        if mark.weight == 1 {
            run = run.disable_bold();
        } else if mark.weight == 2 {
            run = run.bold();
        }
        if mark.font != 0 {
            run = run.size((mark.font as f32 * scale).round().max(1.) as usize);
        }
        if mark.shift != 0 {
            run.run_property = run.run_property.vert_align(if mark.shift > 0 {
                VertAlignType::SuperScript
            } else {
                VertAlignType::SubScript
            });
        }
        if mark.border != 0 {
            run = run.text_border(
                TextBorder::new()
                    .size((mark.border as f32 * scale).round().max(1.) as usize)
                    .space(0),
            );
        }
        if mark.ruby == 0 {
            return run;
        }
        let mut patches = self.patches.borrow_mut();
        let base = String::from_utf8(run.build()).expect("generated run XML is UTF-8");
        let marked = run.style(&format!("KanpoSourceRuby{}", patches.len()));
        patches.push(Patch {
            ruby: mark.ruby,
            marked: String::from_utf8(marked.build()).unwrap(),
            base,
            scale,
        });
        marked
    }
    /// docx-rs has no ruby node API. Replace only uniquely marked, complete
    /// generated runs; never locate a parent by searching for its body text.
    pub fn finish(&self, mut xml: String) -> Result<String> {
        let patches = self.patches.borrow();
        let mut groups = BTreeMap::<usize, Vec<(usize, usize, &str, f32)>>::new();
        for p in patches.iter() {
            let found = xml
                .match_indices(&p.marked)
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            ensure!(
                found.len() == 1,
                "ruby generated run has no unique XML position"
            );
            groups.entry(p.ruby).or_default().push((
                found[0],
                found[0] + p.marked.len(),
                &p.base,
                p.scale,
            ));
        }
        let mut replacements = Vec::new();
        for (ruby, mut parts) in groups {
            parts.sort_by_key(|p| p.0);
            ensure!(
                parts.windows(2).all(|p| p[0].1 == p[1].0),
                "source ruby crosses a paragraph, cell, or unrelated run"
            );
            let scale = parts[0].3;
            ensure!(
                parts.iter().all(|p| p.3 == scale),
                "source ruby crosses different physical canvas scales"
            );
            let scaled = |n: u16| (n as f32 * scale).round().max(1.) as u16;
            let reading = self
                .rubies
                .get(ruby - 1)
                .ok_or_else(|| anyhow::anyhow!("unknown source ruby identity"))?;
            let rt = Run::new()
                .add_text(&reading.reading)
                .size(scaled(reading.size) as usize)
                .fonts(
                    RunFonts::new()
                        .ascii("ＭＳ 明朝")
                        .east_asia("ＭＳ 明朝")
                        .hi_ansi("ＭＳ 明朝"),
                );
            let rt = String::from_utf8(rt.build())?;
            let bases = parts.iter().map(|p| p.2).collect::<String>();
            // ISO/IEC 29500 ruby: outer r/ruby contains rt and rubyBase runs.
            let wrapped=format!("<w:r><w:ruby><w:rubyPr><w:rubyAlign w:val=\"center\" /><w:hps w:val=\"{}\" /><w:hpsRaise w:val=\"{}\" /><w:hpsBaseText w:val=\"{}\" /><w:lid w:val=\"ja-JP\" /></w:rubyPr><w:rt>{rt}</w:rt><w:rubyBase>{bases}</w:rubyBase></w:ruby></w:r>",scaled(reading.size),scaled(reading.raise),scaled(reading.base));
            replacements.push((parts[0].0, parts.last().unwrap().1, wrapped));
        }
        replacements.sort_by_key(|p| p.0);
        ensure!(
            replacements.windows(2).all(|p| p[0].1 <= p[1].0),
            "overlapping ruby XML ranges"
        );
        for (start, end, wrapped) in replacements.into_iter().rev() {
            xml.replace_range(start..end, &wrapped);
        }
        Ok(xml)
    }
}

fn style_mark(style: kanpo_parser::media::fonts::Style) -> Mark {
    use kanpo_parser::media::fonts::Family;
    Mark {
        family: match style.family {
            Some(Family::Mincho) => 1,
            Some(Family::Gothic) => 2,
            None => 0,
        },
        weight: match style.bold {
            Some(false) => 1,
            Some(true) => 2,
            None => 0,
        },
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_changed_complete_field_cannot_borrow_ruby_from_identical_parent_text() {
        let source = kanpo_parser::media::analyze_pdf(include_bytes!(
            "../../kanpo-parser/tests/data/workbook/case-11.pdf"
        ))
        .unwrap();
        let plan = SourceAnnotations::new(&source).unwrap();
        let after = source
            .documents
            .iter()
            .map(|d| d.field_text(Field::After))
            .collect::<String>();
        let before = source
            .documents
            .iter()
            .map(|d| d.field_text(Field::Before))
            .collect::<String>();
        assert!(plan.marks(Field::After, &after).iter().any(|m| m.ruby != 0));
        assert!(plan
            .marks(Field::After, &format!("{after}別の本文"))
            .iter()
            .all(|m| *m == Mark::default()));
        assert!(plan
            .marks(Field::Before, &before)
            .iter()
            .any(|m| m.ruby != 0));
    }
    #[test]
    fn ruby_joins_only_its_own_runs_and_preserves_formats_and_escaped_readings() {
        let w = Writer {
            rubies: vec!["よみ<&".into()],
            patches: Default::default(),
        };
        let a = w.decorate(
            Run::new().add_text("甲").bold(),
            Mark {
                ruby: 1,
                ..Default::default()
            },
        );
        let b = w.decorate(
            Run::new().add_text("𠀀").underline("double"),
            Mark {
                ruby: 1,
                ..Default::default()
            },
        );
        let xml = format!(
            "<w:p>{}{}</w:p>",
            String::from_utf8(a.build()).unwrap(),
            String::from_utf8(b.build()).unwrap()
        );
        let out = w.finish(xml.clone()).unwrap();
        assert_eq!(out.matches("<w:ruby>").count(), 1);
        assert!(out.contains("よみ&lt;&amp;"));
        assert!(out.contains("甲"));
        assert!(out.contains("𠀀"));
        assert!(out.contains("<w:b "));
        assert!(out.contains("w:val=\"double\""));
        assert!(!out.contains("KanpoSourceRuby"));
        let broken = xml.replacen("</w:r><w:r>", "</w:r></w:p><w:p><w:r>", 1);
        assert!(w.finish(broken).is_err());
    }
    #[test]
    fn final_ocr_field_rebinds_native_fonts_without_borrowing_other_annotations() {
        let source = kanpo_parser::media::analyze_pdf(include_bytes!(
            "../../kanpo-parser/tests/data/workbook/case-11.pdf"
        ))
        .unwrap();
        let mut plan = SourceAnnotations::new(&source).unwrap();
        let f = Field::After;
        let original = source
            .documents
            .iter()
            .map(|d| d.field_text(f))
            .collect::<String>();
        assert!(plan.marks(f, &original).iter().any(|m| m.ruby != 0));
        let mut docs = source.documents.clone();
        if let Some(kanpo_parser::media::Block::Shinkyu { rows, .. }) = docs
            .last_mut()
            .unwrap()
            .blocks
            .iter_mut()
            .rev()
            .find(|b| matches!(b, kanpo_parser::media::Block::Shinkyu { .. }))
        {
            rows.last_mut()
                .unwrap()
                .after
                .push(kanpo_parser::media::Inline::Text {
                    text: "認識追記".into(),
                    underline: false,
                });
        } else {
            panic!("fixture has no comparison field");
        }
        // The final analysis would supply these certified original positions.
        // New OCR scalars are intentionally absent from this source audit.
        plan.rebind_fonts(&docs, &source.fonts).unwrap();
        let actual = docs.iter().map(|d| d.field_text(f)).collect::<String>();
        assert_eq!(actual, format!("{original}認識追記"));
        let marks = plan.marks(f, &actual);
        assert!(marks[..original.chars().count()]
            .iter()
            .all(|m| m.font > 0 && m.ruby == 0 && m.border == 0 && m.shift == 0));
        assert!(marks[original.chars().count()..]
            .iter()
            .all(|m| *m == Mark::default()));
        assert!(plan
            .marks(f, &format!("{actual}異"))
            .iter()
            .all(|m| *m == Mark::default()));
    }

    #[test]
    fn native_ruby_fonts_and_raise_scale_together_with_the_physical_canvas() {
        let source = kanpo_parser::media::analyze_pdf(include_bytes!(
            "../../kanpo-parser/tests/data/workbook/case-11.pdf"
        ))
        .unwrap();
        let plan = SourceAnnotations::new(&source).unwrap();
        for scale in [0.5, 1., 1.5] {
            let w = plan.writer();
            let b = &source.ruby.bindings[0];
            let m = source
                .fonts
                .rubies
                .iter()
                .find(|m| m.page == b.page && m.word == b.ruby_word)
                .unwrap();
            let r = w.decorate_scaled(
                Run::new().add_text(&b.text),
                Mark {
                    font: half_points(m.base_points).unwrap(),
                    ruby: 1,
                    ..Default::default()
                },
                scale,
            );
            let xml = w.finish(String::from_utf8(r.build()).unwrap()).unwrap();
            for (tag, points) in [
                ("hps", m.reading_points),
                ("hpsBaseText", m.base_points),
                ("hpsRaise", m.raise_points),
            ] {
                let expected = (half_points(points).unwrap() as f32 * scale)
                    .round()
                    .max(1.) as usize;
                assert!(
                    xml.contains(&format!("<w:{tag} w:val=\"{expected}\"")),
                    "{xml}"
                );
            }
            assert!(!xml.contains("KanpoSourceRuby"));
        }
    }

    #[test]
    fn explicit_font_weight_and_family_are_word_properties_without_invented_bold() {
        use kanpo_parser::media::fonts::{Family, Style};
        let w = SourceAnnotations::default().writer();
        let normal = w.decorate(
            Run::new().add_text("source normal").bold(),
            style_mark(Style {
                family: Some(Family::Gothic),
                bold: Some(false),
            }),
        );
        let normal = String::from_utf8(normal.build()).unwrap();
        assert!(normal.contains("ＭＳ ゴシック"));
        assert!(!normal.contains("<w:b />"));
        let bold = w.decorate(
            Run::new().add_text("source bold"),
            style_mark(Style {
                family: None,
                bold: Some(true),
            }),
        );
        assert!(String::from_utf8(bold.build()).unwrap().contains("<w:b />"));
        let unknown = w.decorate(
            Run::new().add_text("family only"),
            style_mark(Style {
                family: Some(Family::Gothic),
                bold: None,
            }),
        );
        assert!(!String::from_utf8(unknown.build()).unwrap().contains("<w:b"));
    }

    #[test]
    fn shifts_and_frames_are_real_run_properties() {
        let w = SourceAnnotations::default().writer();
        let r = w.decorate(
            Run::new().add_text("−６"),
            Mark {
                shift: 1,
                border: 2,
                ..Default::default()
            },
        );
        let xml = String::from_utf8(r.build()).unwrap();
        assert!(xml.contains("w:val=\"superscript\""));
        assert!(xml.contains("<w:bdr "));
    }
}
