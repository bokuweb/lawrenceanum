//! 字下げ: each Word paragraph takes the indentation of its original lines.
//!
//! The original sets every line of a provision on an em grid below the top of
//! its column (or right of its left edge in horizontal text): 条 and 項 start
//! at the top and continue one em lower, 号 start one em lower and continue two
//! em lower, and so on. A paragraph's first line gives the first-line indent,
//! its second original line the indent of the following lines.
//!
//! Lines bind to Word paragraphs by scalar position in the field, as in
//! `paragraph_gaps`: when the original lines do not spell the start of the
//! field text, nothing in that field is indented.
use crate::paragraph_gaps::compact;
use kanpo_parser::{
    media::{Block, Document, Field, Inline},
    trace::{Line, PageTrace, Role, WordStatus},
    Analysis,
};
use std::collections::BTreeMap;

/// Indents deeper than this are layout (forms, signatures set at the foot),
/// not provision structure.
const MAX_LEVEL: i32 = 10;

#[derive(Default, Debug)]
pub(crate) struct Indents {
    /// Per field (Paragraph, After, Before): original line start position →
    /// indent level in em.
    lines: [BTreeMap<usize, i32>; 3],
    /// First-line level → continuation level → how many multi-line
    /// paragraphs, across all fields, continue so.
    usual: BTreeMap<i32, BTreeMap<i32, usize>>,
}

fn index(f: Field) -> usize {
    match f {
        Field::Paragraph => 0,
        Field::After => 1,
        Field::Before => 2,
    }
}

fn field_of(role: Role) -> Option<Field> {
    match role {
        Role::Body | Role::Preamble => Some(Field::Paragraph),
        Role::After => Some(Field::After),
        Role::Before => Some(Field::Before),
        Role::Outside => None,
    }
}

/// (start along the reading axis, em, axis) of a line made of bound original
/// glyphs: axis 1 for vertical lines, 0 for horizontal ones.
fn start(page: &PageTrace, line: &Line) -> Option<(f32, f32, usize)> {
    if line.table.is_some() || line.words.is_empty() {
        return None;
    }
    let mut u: Option<[f32; 4]> = None;
    let mut sizes = vec![];
    for &id in &line.words {
        let w = page.words.get(id)?;
        if w.status != WordStatus::Used {
            return None;
        }
        let b = page.original_glyph_bounds(id)?;
        sizes.push((b[2] - b[0], b[3] - b[1]));
        u = Some(u.map_or(b, |u| [u[0].min(b[0]), u[1].min(b[1]), u[2].max(b[2]), u[3].max(b[3])]));
    }
    let u = u?;
    let (w, h) = (u[2] - u[0], u[3] - u[1]);
    if h > 1.5 * w {
        let em = sizes.iter().map(|s| s.0).reduce(f32::min)?;
        (em > 0.).then_some((u[1], em, 1))
    } else if w > 1.5 * h {
        let em = sizes.iter().map(|s| s.1).reduce(f32::min)?;
        (em > 0.).then_some((u[0], em, 0))
    } else {
        None
    }
}

impl Indents {
    pub(crate) fn new(analysis: &Analysis, documents: &[Document]) -> Self {
        let mut out = Indents::default();
        // Pass 1: the line starts of every region, and the column tops
        // (region minima) per field and axis across the document.
        struct R {
            field: usize,
            axis: usize,
            lines: Vec<(usize, Option<(f32, f32)>)>,
        }
        let mut streams = [String::new(), String::new(), String::new()];
        let mut regions = vec![];
        let mut tops: BTreeMap<(usize, usize), Vec<f32>> = BTreeMap::new();
        for page in &analysis.pages {
            for region in &page.regions {
                let Some(field) = field_of(region.role).map(index) else { continue };
                let starts = region.lines.iter().map(|l| start(page, l)).collect::<Vec<_>>();
                let mut axes = [0, 0];
                for s in starts.iter().flatten() {
                    axes[s.2] += 1;
                }
                let axis = if axes[1] >= axes[0] { 1 } else { 0 };
                let mut lines = vec![];
                for (line, s) in region.lines.iter().zip(&starts) {
                    let text = compact(&line.text);
                    lines.push((
                        streams[field].chars().count(),
                        s.filter(|s| s.2 == axis).map(|s| (s.0, s.1)),
                    ));
                    streams[field].push_str(&text);
                }
                if let Some(top) = lines.iter().filter_map(|l| l.1).map(|s| s.0).reduce(f32::min) {
                    tops.entry((field, axis)).or_default().push(top);
                }
                regions.push(R { field, axis, lines });
            }
        }
        let fields = [Field::Paragraph, Field::After, Field::Before];
        // The lines must spell the start of the field (material the lines do
        // not reach, such as a closing 附則 on a page read another way, is
        // left unindented), and the Word paragraphs the whole field.
        let certified = std::array::from_fn::<bool, 3, _>(|i| {
            let field = kanpo_parser::source_underlines::field_text(analysis, fields[i]);
            !streams[i].is_empty()
                && field.starts_with(&streams[i])
                && field
                    == documents
                        .iter()
                        .map(|d| compact(&d.field_text(fields[i])))
                        .collect::<String>()
        });
        // Pass 2: levels from the column top. A region holding no line at the
        // top (a page of 号 only) takes the highest top of the same field
        // within four em above its own first line.
        for r in &regions {
            if !certified[r.field] {
                continue;
            }
            let Some((min, em)) = r
                .lines
                .iter()
                .filter_map(|l| l.1)
                .reduce(|a, b| (a.0.min(b.0), a.1.min(b.1)))
            else {
                continue;
            };
            let top = tops[&(r.field, r.axis)]
                .iter()
                .copied()
                .filter(|t| *t <= min + 0.25 * em && min - *t <= 4. * em)
                .reduce(f32::min)
                .unwrap_or(min);
            for &(position, s) in &r.lines {
                let Some((s, e)) = s else { continue };
                let level = (s - top) / e;
                if (level - level.round()).abs() <= 0.2 && (0..=MAX_LEVEL).contains(&(level.round() as i32)) {
                    out.lines[r.field].insert(position, level.round() as i32);
                }
            }
        }
        // The continuations multi-line paragraphs use for each first level.
        // Word wraps narrower than the original, so one-line paragraphs need
        // one too (see `indent`).
        for (fi, field) in fields.into_iter().enumerate() {
            if !certified[fi] {
                continue;
            }
            for (position, n) in paragraphs(documents, field) {
                if let Some((first, Some(rest))) = out.paragraph_levels(fi, position, n) {
                    if first - rest < 2 {
                        *out.usual.entry(first).or_default().entry(rest).or_default() += 1;
                    }
                }
            }
        }
        out
    }

    /// Word indentation (left indent in twips, first-line adjustment) for the
    /// paragraph `text` starting at scalar `position`, set in `size`
    /// half-points. One-line paragraphs continue as `usual_rest` says: Word
    /// wraps them in a narrower measure than the original.
    pub(crate) fn indent(
        &self,
        field: Field,
        position: usize,
        text: &str,
        size: usize,
    ) -> Option<(i32, Option<docx_rs::SpecialIndentType>)> {
        let fi = index(field);
        let n = compact(text).chars().count();
        let (first, rest) = self.paragraph_levels(fi, position, n)?;
        let rest = rest.unwrap_or_else(|| self.usual_rest(first, label(text)));
        // A provision continues at most one em above its first line (a
        // wrapped heading). A next line two em or more shallower belongs to
        // other material joined into this paragraph: its indent is unknown.
        if first == 0 && rest == 0 || first - rest >= 2 {
            return None;
        }
        let em = size as i32 * 10;
        let special = match first.cmp(&rest) {
            std::cmp::Ordering::Greater => Some(docx_rs::SpecialIndentType::FirstLine((first - rest) * em)),
            std::cmp::Ordering::Less => Some(docx_rs::SpecialIndentType::Hanging((rest - first) * em)),
            std::cmp::Ordering::Equal => None,
        };
        Some((rest * em, special))
    }

    /// The continuation of a one-line paragraph: the most frequent one for its
    /// first level, among deeper ones when it opens with a label (号, 項 and
    /// 条 hang), among the others when it does not (改正文 and headings).
    /// Without such a paragraph in the document, a label hangs one em and
    /// other text continues at its first line.
    fn usual_rest(&self, first: i32, labelled: bool) -> i32 {
        self.usual
            .get(&first)
            .and_then(|rests| {
                rests
                    .iter()
                    .filter(|(r, _)| (**r > first) == labelled)
                    .max_by_key(|(r, n)| (**n, -**r))
                    .map(|(r, _)| *r)
            })
            .unwrap_or(if labelled { first + 1 } else { first })
    }

    /// Levels of the paragraph of `n` scalars at `position`: its first
    /// original line, and the next original line inside it.
    fn paragraph_levels(&self, field: usize, position: usize, n: usize) -> Option<(i32, Option<i32>)> {
        let lines = &self.lines[field];
        let first = *lines.get(&position)?;
        let rest = lines.range(position + 1..position + n).next().map(|(_, l)| *l);
        Some((first, rest))
    }
}

/// A paragraph opening with a label set off by a space (「一　」「第二条の二　」
/// 「イ　」「（１）　」): the shape the original sets with a hanging indent.
fn label(text: &str) -> bool {
    let t = text.trim_start();
    let Some(k) = t.chars().position(char::is_whitespace).filter(|k| (1..=12).contains(k)) else {
        return false;
    };
    // Numbering only: a date or a name before a space (「令和八年九月十五日　環境大臣」)
    // is not a label.
    t.chars().take(k).all(|c| {
        "〇一二三四五六七八九十百千第条項号のイロハニホヘトチリヌルヲワカヨタレソツネナラムウヰノオクヤマケフコエテアサキユメミシヱヒモセス（）()ⅰⅱⅲⅳⅴⅵⅶⅷⅸⅹivx"
            .contains(c)
            || c.is_ascii_digit()
            || ('０'..='９').contains(&c)
    })
}

/// (start position, scalar count) of every Word paragraph of `field`, in the
/// order the writer emits them: text split at newlines, with tables (and
/// their cells) as separate paragraphs.
fn paragraphs(documents: &[Document], field: Field) -> Vec<(usize, usize)> {
    fn runs(r: &[Inline], position: &mut usize, out: &mut Vec<(usize, usize)>) {
        let mut start = *position;
        let flush = |start: usize, position: usize, out: &mut Vec<(usize, usize)>| {
            if position > start {
                out.push((start, position - start));
            }
        };
        for run in r {
            match run {
                Inline::Text { text, .. } => {
                    for c in text.chars() {
                        if c == '\n' {
                            flush(start, *position, out);
                            start = *position;
                        } else if !c.is_whitespace() {
                            *position += 1;
                        }
                    }
                }
                Inline::Table { table } => {
                    flush(start, *position, out);
                    for row in &table.rows {
                        for cell in row {
                            runs(cell, position, out);
                        }
                    }
                    start = *position;
                }
                Inline::Image { .. } => {
                    flush(start, *position, out);
                    start = *position;
                }
            }
        }
        flush(start, *position, out);
    }
    let mut position = 0;
    let mut out = vec![];
    for doc in documents {
        for block in &doc.blocks {
            match block {
                Block::Paragraph { runs: r } if field == Field::Paragraph => runs(r, &mut position, &mut out),
                Block::Shinkyu { rows, .. } if field != Field::Paragraph => {
                    for row in rows {
                        let (r, table) = if field == Field::After {
                            (&row.after, &row.after_table)
                        } else {
                            (&row.before, &row.before_table)
                        };
                        runs(r, &mut position, &mut out);
                        if let Some(table) = table {
                            for tr in &table.rows {
                                for cell in tr {
                                    runs(cell, &mut position, &mut out);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use docx_rs::SpecialIndentType::{FirstLine, Hanging};

    fn indents(lines: &[(usize, i32)], usual: &[(i32, i32, usize)]) -> Indents {
        let mut out = Indents::default();
        out.lines[1] = lines.iter().copied().collect();
        for &(first, rest, n) in usual {
            out.usual.entry(first).or_default().insert(rest, n);
        }
        out
    }
    /// A paragraph of `n` scalars: a label and a space, or plain text.
    fn para(n: usize, labelled: bool) -> String {
        if labelled { format!("イ　{}", "字".repeat(n - 1)) } else { "字".repeat(n) }
    }

    #[test]
    fn levels_become_twips_of_the_paragraph_size() {
        let i = indents(&[(0, 0), (10, 1), (30, 1), (40, 2), (60, 3), (70, 3)], &[(2, 3, 1), (2, 2, 5)]);
        let ind = |p, n| i.indent(Field::After, p, &para(n, true), 16).map(|(l, s)| (l, s.map(|s| format!("{s:?}"))));
        // 条: first line at the top, continuing one em lower.
        assert_eq!(ind(0, 40), Some((160, Some(format!("{:?}", Hanging(160))))));
        // A one-line 号細分 continues at the usual level for its shape.
        assert_eq!(ind(40, 5), Some((480, Some(format!("{:?}", Hanging(160))))));
        assert_eq!(i.indent(Field::After, 40, &para(5, false), 16).map(|v| v.0), Some(320));
        // Without a paragraph of its shape: a label hangs one em, other text
        // continues at its first line.
        let only = indents(&[(0, 1)], &[(1, 2, 3)]);
        assert_eq!(only.indent(Field::After, 0, &para(5, false), 16).map(|v| v.0), Some(160));
        let none = indents(&[(0, 2)], &[]);
        assert_eq!(none.indent(Field::After, 0, &para(5, true), 16).map(|v| v.0), Some(480));
        // A wrapped heading continues one em above its first line.
        let h = indents(&[(0, 2), (10, 1)], &[]);
        assert_eq!(
            h.indent(Field::After, 0, &para(20, false), 16).map(|(l, s)| (l, s.map(|s| format!("{s:?}")))),
            Some((160, Some(format!("{:?}", FirstLine(160)))))
        );
        // Same level throughout: a plain left indent.
        assert_eq!(ind(60, 20), Some((480, None)));
        // No original line starts at the paragraph: unknown.
        assert_eq!(ind(5, 5), None);
        // Another field has no lines.
        assert!(i.indent(Field::Before, 0, &para(40, true), 16).is_none());
    }

    #[test]
    fn labels_are_numbering_set_off_by_a_space() {
        for l in ["一　協定", "イ　非常", "２　法第", "第一条の二の三　法第", "（１）　当該", "（ii）　法第", "12　法第"] {
            assert!(label(l), "{l}");
        }
        for l in ["令和八年九月十五日　環境大臣", "第二条の次に次の一条を加える。", "（新設）", "附則", "環境大臣　石原"] {
            assert!(!label(l), "{l}");
        }
    }

    #[test]
    fn a_much_shallower_next_line_is_other_material() {
        let i = indents(&[(0, 6), (10, 2), (20, 0), (30, 0)], &[]);
        assert!(i.indent(Field::After, 0, &para(20, true), 16).is_none(), "6 em then 2 em");
        assert!(i.indent(Field::After, 20, &para(20, true), 16).is_none(), "flush with the column top");
    }

    #[test]
    fn changed_field_text_indents_nothing() {
        let pdf = include_bytes!("../../kanpo-parser/tests/data/workbook/case-01.pdf");
        let a = crate::analyze(pdf, &[]).unwrap();
        let mut docs = a.articles.iter().map(|a| Document::from(&a.document)).collect::<Vec<_>>();
        assert!(!Indents::new(&a, &docs).lines[1].is_empty());
        for doc in &mut docs {
            for block in &mut doc.blocks {
                if let Block::Shinkyu { rows, .. } = block {
                    rows[0].after.push(Inline::Text { text: "異なる原文".into(), underline: false });
                }
            }
        }
        let i = Indents::new(&a, &docs);
        assert!(i.lines[1].is_empty());
        assert!(!i.lines[2].is_empty(), "the unchanged field keeps its indents");
    }
}
