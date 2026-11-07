//! Table geometry from the parsed source, without changing the public model.
use kanpo_parser::{model::NestedTable, trace::Table};
#[path = "alignment.rs"]
mod alignment;
pub(crate) use alignment::Alignment;

pub(crate) fn header_rows(t: &NestedTable) -> usize {
    let end = t
        .merges
        .iter()
        .filter(|m| m.row == 0)
        .map(|m| m.rowspan)
        .max()
        .unwrap_or(0);
    if end > 1
        && end < t.rows.len()
        && t.merges
            .iter()
            .any(|m| m.row == 0 && m.colspan > 1 && m.rowspan < end)
    {
        end
    } else {
        0
    }
}

pub(crate) struct Layout<'a> {
    tables: Vec<&'a Table>,
    spacing: crate::spacing::Spacing,
    pages: Option<&'a [kanpo_parser::trace::PageTrace]>,
    derived: std::cell::RefCell<std::collections::BTreeMap<String, Option<Vec<f32>>>>,
}

pub(crate) struct PhysicalImageTable {
    pub widths: Vec<usize>,
    pub heights: Vec<f32>,
    pub scale: f32,
}

impl<'a> Layout<'a> {
    pub fn new(analysis: &'a kanpo_parser::Analysis) -> Self {
        Self {
            spacing: crate::spacing::Spacing::new(analysis),
            pages: Some(&analysis.pages),
            derived: Default::default(),
            tables: analysis
                .pages
                .iter()
                .flat_map(|p| &p.regions)
                .flat_map(|r| &r.lines)
                .filter_map(|l| l.table.as_ref())
                .collect(),
        }
    }

    pub fn label_gaps(&self, text: &str, prose: bool) -> Vec<(usize, usize)> {
        self.spacing.gaps(text, prose)
    }

    pub fn columns(&self, t: &NestedTable, width: usize) -> Vec<usize> {
        if let Some(Some(edges)) = self
            .derived
            .borrow()
            .get(&serde_json::to_string(t).unwrap())
        {
            if let Some(columns) = scale_columns(edges, width) {
                return columns;
            }
        }
        let cols = t.rows.iter().map(Vec::len).max().unwrap_or(0);
        let candidates = self
            .tables
            .iter()
            .filter_map(|source| {
                if !supports_prefix(source, t) {
                    return None;
                }
                physical_columns(source, width)
            })
            .collect::<Vec<_>>();
        if let Some(first) = candidates
            .first()
            .filter(|first| candidates.iter().all(|c| c == *first))
        {
            return first.clone();
        }
        // Explicitly declare even a geometry-free grid: an empty Word tblGrid
        // lets applications assign a horizontally merged header to wrong columns.
        (0..cols)
            .map(|c| width * (c + 1) / cols - width * c / cols)
            .collect()
    }

    pub fn alignments(
        &self,
        t: &NestedTable,
    ) -> std::collections::BTreeMap<(usize, usize), Alignment> {
        let mut candidates = vec![];
        let compact = |s: String| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
        for page in self.pages.unwrap_or_default() {
            for source in page
                .regions
                .iter()
                .flat_map(|r| &r.lines)
                .filter_map(|l| l.table.as_ref())
            {
                let own = source.to_nested();
                if own.rows.len() != t.rows.len()
                    || own.merges != t.merges
                    || own.rows.iter().zip(&t.rows).any(|(a, b)| {
                        a.len() != b.len()
                            || a.iter().zip(b).any(|(a, b)| {
                                b.iter().any(|r| r.table.is_some())
                                    || compact(a.iter().map(|r| r.text.as_str()).collect())
                                        != compact(b.iter().map(|r| r.text.as_str()).collect())
                            })
                    })
                {
                    continue;
                }
                let axis = if column_edges(source).is_some() {
                    0
                } else {
                    let mut transposed = source.clone();
                    for c in &mut transposed.cells {
                        c.x0 = c.y0;
                        c.x1 = c.y1;
                    }
                    if column_edges(&transposed).is_none() {
                        continue;
                    }
                    1
                };
                candidates.push(alignment::source(source, page, axis));
            }
        }
        let Some(first) = candidates.first() else {
            return Default::default();
        };
        first
            .iter()
            .filter(|(key, value)| candidates.iter().all(|c| c.get(key) == Some(value)))
            .map(|(key, value)| (*key, *value))
            .collect()
    }

    pub fn padding(&self, t: &NestedTable, width: usize) -> Option<usize> {
        let mut candidates = Vec::new();
        for page in self.pages? {
            for source in page
                .regions
                .iter()
                .flat_map(|r| &r.lines)
                .filter_map(|l| l.table.as_ref())
                .filter(|s| supports_prefix(s, t))
            {
                let axis = if column_edges(source).is_some() {
                    0
                } else {
                    let mut transposed = source.clone();
                    for c in &mut transposed.cells {
                        c.x0 = c.y0;
                        c.x1 = c.y1;
                    }
                    if column_edges(&transposed).is_none() {
                        continue;
                    }
                    1
                };
                let mut gaps = Vec::new();
                let mut valid = true;
                for c in &source.cells {
                    if c.text.trim().is_empty() {
                        continue;
                    }
                    let words = c
                        .words
                        .iter()
                        .filter_map(|id| page.words.get(*id))
                        .filter(|w| !w.text.trim().is_empty())
                        .collect::<Vec<_>>();
                    if words.is_empty() {
                        valid = false;
                        break;
                    }
                    let (left, right) = if axis == 0 {
                        (c.x0, c.x1)
                    } else {
                        (c.y0, c.y1)
                    };
                    let mut lo = right;
                    for w in words {
                        let Some(b) = page.original_glyph_bounds(w.id) else {
                            valid = false;
                            break;
                        };
                        // Hanging end punctuation can extend its em envelope
                        // into/past the trailing inset. It is not evidence of
                        // zero cell padding. Use the authored leading bearing
                        // for Word's shared symmetric inset instead.
                        if b[axis] < left || right <= b[axis] {
                            valid = false;
                            break;
                        }
                        lo = lo.min(b[axis]);
                    }
                    gaps.push(lo - left);
                }
                if !valid {
                    continue;
                }
                let Some(gap) = gaps.into_iter().reduce(f32::min) else {
                    continue;
                };
                let extent = if axis == 0 {
                    source.x1 - source.x0
                } else {
                    source.y1 - source.y0
                };
                if gap.is_finite() && gap > 0. && extent > 0. {
                    candidates.push((gap / extent * width as f32).round() as usize);
                }
            }
        }
        let first = *candidates.first()?;
        candidates.iter().all(|v| *v == first).then_some(first)
    }

    pub fn source_columns(
        &self,
        t: &NestedTable,
    ) -> Option<(usize, kanpo_parser::trace::Role, bool, Vec<f32>)> {
        let mut candidates = Vec::new();
        for (page, p) in self.pages?.iter().enumerate() {
            for r in &p.regions {
                for l in &r.lines {
                    let Some(source) = l.table.as_ref().filter(|s| s.to_nested() == *t) else {
                        continue;
                    };
                    let (normal, edges) = if let Some(e) = column_edges(source) {
                        (true, e)
                    } else {
                        let mut s = source.clone();
                        for c in &mut s.cells {
                            c.x0 = c.y0;
                            c.x1 = c.y1;
                        }
                        (false, column_edges(&s)?)
                    };
                    let origin = edges[0];
                    candidates.push((
                        page,
                        r.role,
                        normal,
                        edges.into_iter().map(|x| x - origin).collect(),
                    ));
                }
            }
        }
        (candidates.len() == 1).then(|| candidates.remove(0))
    }

    pub fn register_columns(&self, t: &NestedTable, edges: Vec<f32>) {
        let key = serde_json::to_string(t).unwrap();
        self.derived
            .borrow_mut()
            .entry(key)
            .and_modify(|old| {
                if old.as_ref() != Some(&edges) {
                    *old = None
                }
            })
            .or_insert(Some(edges));
    }

    /// Invert a source-proved vertical table only when both physical axes agree
    /// with every logical span. The image-bearing source layout then remains a
    /// single editable table, rather than placing its image beside subfields.
    pub fn physical_image_table(
        &self,
        t: &NestedTable,
        width: usize,
        max_height: f32,
    ) -> Option<PhysicalImageTable> {
        let mut candidates = Vec::new();
        for source in &self.tables {
            if source.to_nested() != *t || physical_columns(source, width).is_some() {
                continue;
            }
            let mut physical = (*source).clone();
            physical.cols = source.rows;
            physical.rows = source.cols;
            for c in &mut physical.cells {
                let (row, col, rowspan, colspan) = (c.row, c.col, c.rowspan, c.colspan);
                c.row = col;
                c.col = source.rows.checked_sub(row + rowspan)?;
                c.rowspan = colspan;
                c.colspan = rowspan;
            }
            let xs = column_edges(&physical)?;
            let mut ys_table = physical.clone();
            ys_table.cols = physical.rows;
            for c in &mut ys_table.cells {
                c.col = c.row;
                c.colspan = c.rowspan;
                c.x0 = c.y0;
                c.x1 = c.y1;
            }
            let ys = column_edges(&ys_table)?;
            let (w, h) = (xs.last()? - xs[0], ys.last()? - ys[0]);
            let scale = 1_f32.min(width as f32 / 20. / w).min(max_height / h);
            if !scale.is_finite() || scale <= 0. {
                continue;
            }
            let widths = xs
                .windows(2)
                .map(|p| ((p[1] - p[0]) * scale * 20.).round() as usize)
                .collect::<Vec<_>>();
            let heights = ys
                .windows(2)
                .map(|p| ((p[1] - p[0]) * scale * 20.).round())
                .collect::<Vec<_>>();
            candidates.push(PhysicalImageTable {
                widths,
                heights,
                scale,
            });
        }
        let first = candidates.pop()?;
        candidates
            .iter()
            .all(|c| c.widths == first.widths && c.heights == first.heights)
            .then_some(first)
    }
}

fn column_edges(t: &Table) -> Option<Vec<f32>> {
    let mut edges: Vec<Option<f32>> = vec![None; t.cols + 1];
    for c in &t.cells {
        for (i, x) in [(c.col, c.x0), (c.col + c.colspan, c.x1)] {
            let slot = edges.get_mut(i)?;
            if !x.is_finite() || slot.is_some_and(|old| (old - x).abs() > 1.) {
                return None;
            }
            *slot = Some(x);
        }
    }
    let edges = edges.into_iter().collect::<Option<Vec<_>>>()?;
    let span = edges.last()? - edges[0];
    if span <= 0. || edges.windows(2).any(|p| p[1] <= p[0]) {
        return None;
    }
    Some(edges)
}

fn physical_columns(t: &Table, width: usize) -> Option<Vec<usize>> {
    let edges = column_edges(t)?;
    scale_columns(&edges, width)
}
fn scale_columns(edges: &[f32], width: usize) -> Option<Vec<usize>> {
    let span = edges.last()? - edges[0];
    let scaled = edges
        .iter()
        .map(|x| (((x - edges[0]) / span) as f64 * width as f64).round() as usize)
        .collect::<Vec<_>>();
    let result = scaled.windows(2).map(|p| p[1] - p[0]).collect::<Vec<_>>();
    (!result.contains(&0)).then_some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanpo_parser::trace::TableCell;

    fn source() -> Table {
        let xs = [0., 10., 30., 50., 100.];
        let ys = [0., 20., 60., 100.];
        let shapes = [
            (0, 0, 1, 2),
            (0, 2, 2, 1),
            (0, 3, 2, 1),
            (1, 0, 1, 1),
            (1, 1, 1, 1),
            (2, 0, 1, 1),
            (2, 1, 1, 1),
            (2, 2, 1, 1),
            (2, 3, 1, 1),
        ];
        Table {
            x0: 0.,
            y0: 0.,
            x1: 100.,
            y1: 100.,
            rows: 3,
            cols: 4,
            cells: shapes
                .into_iter()
                .map(|(row, col, rowspan, colspan)| TableCell {
                    row,
                    col,
                    rowspan,
                    colspan,
                    x0: xs[col],
                    x1: xs[col + colspan],
                    y0: ys[row],
                    y1: ys[row + rowspan],
                    text: format!("field-{row}-{col}"),
                    words: vec![],
                })
                .collect(),
        }
    }

    #[test]
    fn source_widths_are_translation_scale_and_continuation_independent() {
        let original = source();
        let mut nested = original.to_nested();
        nested.rows[2][0][0].text.push_str(" continuation");
        for (scale, dx, dy) in [(1., 0., 0.), (1.5, 100., 40.), (0.7, 31., 55.)] {
            let mut s = original.clone();
            for c in &mut s.cells {
                c.x0 = c.x0 * scale + dx;
                c.x1 = c.x1 * scale + dx;
                c.y0 = c.y0 * scale + dy;
                c.y1 = c.y1 * scale + dy;
            }
            assert_eq!(
                Layout {
                    tables: vec![&s],
                    spacing: Default::default(),
                    pages: None,
                    derived: Default::default()
                }
                .columns(&nested, 1000),
                vec![100, 200, 200, 500]
            );
        }
        assert_eq!(header_rows(&nested), 2);
    }

    #[test]
    fn original_cell_anchors_need_complete_tables_and_original_glyph_geometry() {
        let mut counts = [0usize; 3];
        for pdf in [
            include_bytes!("../../kanpo-parser/tests/data/workbook/case-09.pdf").as_slice(),
            include_bytes!("../../kanpo-parser/tests/data/workbook/case-17.pdf").as_slice(),
        ] {
            let source = kanpo_parser::media::analyze_pdf(pdf).unwrap();
            let a = &source.source_layout;
            let layout = Layout::new(a);
            for t in a
                .pages
                .iter()
                .flat_map(|p| &p.regions)
                .flat_map(|r| &r.lines)
                .filter_map(|l| l.table.as_ref())
            {
                let target = t.to_nested();
                let map = layout.alignments(&target);
                for value in map.values() {
                    counts[match value {
                        Alignment::Left => 0,
                        Alignment::Center => 1,
                        Alignment::Right => 2,
                    }] += 1;
                }
                if map.is_empty() {
                    continue;
                }
                let mut wrong = target.clone();
                wrong.rows[0][0].push(kanpo_parser::model::Run::plain("異なるセル"));
                assert!(
                    layout.alignments(&wrong).is_empty(),
                    "complete table identity is required"
                );
                let mut unknown = a.clone();
                for p in &mut unknown.pages {
                    for w in &mut p.words {
                        w.id += 100000;
                    }
                }
                assert!(
                    Layout::new(&unknown).alignments(&target).is_empty(),
                    "guessed rectangles cannot supply alignment"
                );
            }
        }
        eprintln!(
            "SOURCE-CELL-ALIGNMENTS left={} center={} right={}",
            counts[0], counts[1], counts[2]
        );
        assert!(
            counts.iter().all(|&n| n > 0),
            "originals exercise every alignment: {counts:?}"
        );
    }

    #[test]
    fn table_insets_use_original_leading_bearings_despite_hanging_end_punctuation() {
        let source = kanpo_parser::media::analyze_pdf(include_bytes!(
            "../../kanpo-parser/tests/data/workbook/case-04.pdf"
        ))
        .unwrap();
        let a = &source.source_layout;
        let t = a.pages[0]
            .regions
            .iter()
            .flat_map(|r| &r.lines)
            .filter_map(|l| l.table.as_ref())
            .find(|t| t.cols == 4)
            .unwrap()
            .to_nested();
        assert_eq!(Layout::new(a).padding(&t, 6560), Some(80));
        assert_eq!(Layout::new(a).padding(&t, 13120), Some(160));
        let mut wrong = t.clone();
        for row in &mut wrong.rows {
            for cell in row {
                for run in cell {
                    run.text.push('異');
                }
            }
        }
        assert_eq!(Layout::new(a).padding(&wrong, 6560), None);
        let mut unknown = a.clone();
        for p in &mut unknown.pages {
            for w in &mut p.words {
                w.id += 100000;
            }
        }
        assert_eq!(Layout::new(&unknown).padding(&t, 6560), None);
    }

    #[test]
    fn conflicting_geometry_or_mismatched_header_uses_explicit_fallback_grid() {
        let a = source();
        let mut b = a.clone();
        for c in &mut b.cells {
            if c.x0 == 50. {
                c.x0 = 60.;
            }
            if c.x1 == 50. {
                c.x1 = 60.;
            }
        }
        let mut model = a.to_nested();
        assert_eq!(
            Layout {
                tables: vec![&a, &b],
                spacing: Default::default(),
                pages: None,
                derived: Default::default(),
            }
            .columns(&model, 1000),
            vec![250; 4]
        );
        model.rows[0][0][0].text.push_str(" extra");
        assert_eq!(
            Layout {
                tables: vec![&a],
                spacing: Default::default(),
                pages: None,
                derived: Default::default()
            }
            .columns(&model, 1000),
            vec![250; 4]
        );
        let mut vertical = a.clone();
        for c in &mut vertical.cells {
            std::mem::swap(&mut c.x0, &mut c.y0);
            std::mem::swap(&mut c.x1, &mut c.y1);
        }
        assert!(physical_columns(&vertical, 1000).is_none());
    }
}

fn supports_prefix(source: &Table, t: &NestedTable) -> bool {
    if source.cols != t.rows.iter().map(Vec::len).max().unwrap_or(0)
        || source.rows > t.rows.len()
        || !source.cells.iter().any(|c| !c.text.trim().is_empty())
    {
        return false;
    }
    let nested = source.to_nested();
    if !nested.merges.iter().all(|m| t.merges.contains(m)) {
        return false;
    }
    // A joined continuation may extend the final row's text. All
    // nonempty source fields must still match at their own positions.
    if !nested.rows.iter().enumerate().all(|(r, row)| {
        row.iter().enumerate().all(|(c, runs)| {
            let text: String = runs.iter().map(|r| r.text.as_str()).collect();
            text.is_empty()
                || t.rows
                    .get(r)
                    .and_then(|row| row.get(c))
                    .is_some_and(|target| {
                        let target_text =
                            target.iter().map(|r| r.text.as_str()).collect::<String>();
                        target.iter().all(|r| r.table.is_none())
                            && (target_text == text
                                || (r + 1 == source.rows && target_text.starts_with(&text)))
                    })
        })
    }) {
        return false;
    }

    true
}
