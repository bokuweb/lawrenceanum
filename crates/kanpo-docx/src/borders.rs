//! Dashed source rulings decorate existing, completely matched table cells.
//! This never creates a grid from an underline or invents a missing frame side.
use anyhow::Result;
use docx_rs::{
    BorderType, TableCell, TableCellBorder, TableCellBorderPosition as Position, TableCellBorders,
};
use kanpo_parser::{
    media::Field,
    model::{NestedTable, Run},
    trace::{Role, Table, Word, WordStatus},
    Analysis,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}
type Edges = BTreeMap<Edge, u8>;
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub(crate) struct CellPlan(BTreeMap<(usize, usize), Edges>);
#[derive(Clone)]
struct SourceTable {
    table: NestedTable,
    field: Field,
    logical: CellPlan,
    physical: CellPlan,
}
#[derive(Clone, Default)]
pub(crate) struct Borders {
    tables: Vec<SourceTable>,
}
#[derive(Clone, Copy)]
struct Segment {
    a: [f64; 2],
    b: [f64; 2],
    size: u8,
}

fn canonical(t: &NestedTable) -> NestedTable {
    NestedTable {
        merges: t.merges.clone(),
        rows: t
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| {
                        let mut out: Vec<Run> = Vec::new();
                        for r in cell {
                            if !r.text.is_empty() {
                                if let Some(last) = out.last_mut().filter(|r| r.table.is_none()) {
                                    last.text.push_str(&r.text);
                                } else {
                                    out.push(Run::plain(&r.text));
                                }
                            }
                            if let Some(t) = &r.table {
                                out.push(Run {
                                    text: String::new(),
                                    underline: false,
                                    table: Some(canonical(t)),
                                });
                            }
                        }
                        out
                    })
                    .collect()
            })
            .collect(),
    }
}
fn direction(t: &Table) -> Option<bool> {
    fn edges(t: &Table, columns: bool, axis: usize, reverse: bool) -> Option<Vec<f32>> {
        let n = if columns { t.cols } else { t.rows };
        let mut slots = vec![None; n + 1];
        for c in &t.cells {
            let (i, span) = if columns {
                (c.col, c.colspan)
            } else {
                (c.row, c.rowspan)
            };
            let (lo, hi) = if axis == 0 {
                (c.x0, c.x1)
            } else {
                (c.y0, c.y1)
            };
            if !lo.is_finite() || !hi.is_finite() || hi <= lo {
                return None;
            }
            for (index, value) in [
                (i, if reverse { hi } else { lo }),
                (i + span, if reverse { lo } else { hi }),
            ] {
                let slot = slots.get_mut(index)?;
                if slot.is_some_and(|old: f32| (old - value).abs() > 0.1) {
                    return None;
                }
                *slot = Some(value);
            }
        }
        let values = slots.into_iter().collect::<Option<Vec<_>>>()?;
        values
            .windows(2)
            .all(|w| if reverse { w[0] > w[1] } else { w[0] < w[1] })
            .then_some(values)
    }
    if edges(t, true, 0, false).is_some() && edges(t, false, 1, false).is_some() {
        Some(true)
    } else if edges(t, true, 1, false).is_some() && edges(t, false, 0, true).is_some() {
        Some(false)
    } else {
        None
    }
}
fn tolerance(t: &Table, words: &[Word]) -> Option<f64> {
    let mut sizes = t
        .cells
        .iter()
        .flat_map(|c| &c.words)
        .filter_map(|&id| words.get(id))
        .filter(|w| w.status == WordStatus::Used)
        .filter_map(|w| {
            let [x, y, r, b] = w.source_box;
            let em = (r - x).min(b - y) as f64;
            (em.is_finite() && em > 0.).then_some(em)
        })
        .collect::<Vec<_>>();
    sizes.sort_by(f64::total_cmp);
    let em = *sizes.get(sizes.len() / 2)?;
    let minimum = t
        .cells
        .iter()
        .map(|c| (c.x1 - c.x0).min(c.y1 - c.y0) as f64)
        .reduce(f64::min)?;
    Some((em * 0.35).min(minimum * 0.25))
}
fn opaque(draw: &kanpo_pdf::PathDraw) -> bool {
    // This writer currently represents black table rules. Pattern or colored
    // strokes cannot silently acquire the default black cell border.
    if let Some(color) = draw.state.iter().rev().find(|s| {
        matches!(
            s.operator.as_slice(),
            b"G" | b"RG" | b"K" | b"CS" | b"SC" | b"SCN"
        )
    }) {
        let values = color
            .operands
            .iter()
            .map(kanpo_pdf::Object::as_f64)
            .collect::<Option<Vec<_>>>();
        let black = match (color.operator.as_slice(), values.as_deref()) {
            (b"G", Some([v])) => *v == 0.,
            (b"RG", Some([r, g, b])) => *r == 0. && *g == 0. && *b == 0.,
            (b"K", Some([c, m, y, k])) => *c == 0. && *m == 0. && *y == 0. && *k == 1.,
            _ => false,
        };
        if !black {
            return false;
        }
    }
    for op in &draw.state {
        if op.operator != b"gs" {
            continue;
        }
        let Some(d) = op.resource.as_ref().and_then(kanpo_pdf::Object::as_dict) else {
            return false;
        };
        for key in [b"CA".as_slice(), b"ca".as_slice()] {
            if d.get(key).is_some_and(|v| v.as_f64() != Some(1.)) {
                return false;
            }
        }
        if d.get(b"SMask".as_slice())
            .is_some_and(|v| v.as_name() != Some(b"None"))
        {
            return false;
        }
        if d.get(b"BM".as_slice())
            .is_some_and(|v| v.as_name() != Some(b"Normal") && v.as_name() != Some(b"Compatible"))
        {
            return false;
        }
    }
    true
}
fn segments(source: &kanpo_pdf::XObjectPageSources) -> Vec<Segment> {
    if !source.capture_gaps().is_empty() {
        return vec![];
    }
    let mut out = Vec::new();
    for draw in source.paths() {
        let Some(dash) = draw.state.iter().rev().find(|s| s.operator == b"d") else {
            continue;
        };
        let [array, phase] = dash.operands.as_slice() else {
            continue;
        };
        let Some(array) = array.as_array().filter(|a| {
            !a.is_empty()
                && a.iter()
                    .all(|v| v.as_f64().is_some_and(|n| n.is_finite() && n > 0.))
        }) else {
            continue;
        };
        if !phase.as_f64().is_some_and(f64::is_finite)
            || array.is_empty()
            || !draw.clips.is_empty()
            || !opaque(draw)
        {
            continue;
        }
        let Some(path) = source.page().paths.get(draw.path) else {
            continue;
        };
        if !path.stroke
            || path.fill
            || !path.line_width.is_finite()
            || path.line_width <= 0.
            || path.subpaths.iter().any(|s| s.curved)
        {
            continue;
        }
        let size = (path.line_width * 8.).round().clamp(1., 96.) as u8;
        let mut own = Vec::new();
        let mut valid = true;
        for sub in &path.subpaths {
            let mut pairs = sub
                .points
                .windows(2)
                .map(|p| (p[0], p[1]))
                .collect::<Vec<_>>();
            if sub.closed && sub.points.first() != sub.points.last() {
                if let (Some(a), Some(b)) = (sub.points.last(), sub.points.first()) {
                    pairs.push((*a, *b));
                }
            }
            for (a, b) in pairs {
                if !a.iter().chain(&b).all(|n| n.is_finite())
                    || ((a[0] - b[0]).abs() > 0.01 && (a[1] - b[1]).abs() > 0.01)
                {
                    valid = false;
                    break;
                }
                if (a[0] - b[0]).abs().max((a[1] - b[1]).abs()) > 0.01 {
                    own.push(Segment { a, b, size });
                }
            }
        }
        // A lone dashed text underline supplies no table enclosure evidence.
        if valid && own.len() >= 2 {
            out.extend(own);
        }
    }
    out
}
fn covers(s: Segment, edge: Edge, b: [f64; 4], tol: f64) -> bool {
    let (axis, pos, lo, hi) = match edge {
        Edge::Left => (0, b[0], b[1], b[3]),
        Edge::Right => (0, b[2], b[1], b[3]),
        Edge::Top => (1, b[1], b[0], b[2]),
        Edge::Bottom => (1, b[3], b[0], b[2]),
    };
    let flow = 1 - axis;
    (s.a[axis] - s.b[axis]).abs() <= 0.01
        && (s.a[axis] - pos).abs() <= tol
        && s.a[flow].min(s.b[flow]) <= lo + 0.01
        && s.a[flow].max(s.b[flow]) >= hi - 0.01
}
fn plans(t: &Table, words: &[Word], segments: &[Segment]) -> Option<(CellPlan, CellPlan)> {
    let normal = direction(t)?;
    let tol = tolerance(t, words)?;
    let mut logical = CellPlan::default();
    let mut physical = CellPlan::default();
    for c in &t.cells {
        let b = [c.x0 as f64, c.y0 as f64, c.x1 as f64, c.y1 as f64];
        for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
            let sizes = segments
                .iter()
                .copied()
                .filter(|&s| covers(s, edge, b, tol))
                .map(|s| s.size)
                .collect::<BTreeSet<_>>();
            let Some(size) = sizes.first().copied().filter(|_| sizes.len() == 1) else {
                continue;
            };
            let target = if normal {
                edge
            } else {
                match edge {
                    Edge::Left => Edge::Bottom,
                    Edge::Right => Edge::Top,
                    Edge::Top => Edge::Left,
                    Edge::Bottom => Edge::Right,
                }
            };
            logical
                .0
                .entry((c.row, c.col))
                .or_default()
                .insert(target, size);
            physical
                .0
                .entry((c.col, t.rows - c.row - c.rowspan))
                .or_default()
                .insert(edge, size);
        }
    }
    Some((logical, physical))
}
impl Borders {
    pub fn from_media(pdf: &[u8], source: &kanpo_parser::media::MediaAnalysis) -> Result<Self> {
        let mut out = Self::from_pdf(pdf, &source.source_layout)?;
        for group in &source.table_continuations {
            let mut by_target = BTreeMap::<(usize, usize), Vec<(usize, usize, usize)>>::new();
            for &(id, r, c, tr, tc) in &group.cells {
                by_target.entry((tr, tc)).or_default().push((id, r, c));
            }
            let fields = group
                .cells
                .iter()
                .map(|(id, _, _, _, _)| out.tables.get(*id).map(|s| s.field))
                .collect::<Option<Vec<_>>>();
            let Some(fields) = fields else {
                continue;
            };
            let Some(&field) = fields
                .first()
                .filter(|first| fields.iter().all(|f| f == *first))
            else {
                continue;
            };
            let mut logical = CellPlan::default();
            for (target, parts) in by_target {
                for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
                    // A vertical side must have original dashed support in
                    // every physical fragment of this logical owner. A short
                    // page-local dash cannot style an entire merged cell.
                    let fragments = match edge {
                        Edge::Top => &parts[..1],
                        Edge::Bottom => &parts[parts.len() - 1..],
                        _ => parts.as_slice(),
                    };
                    let sizes = fragments
                        .iter()
                        .map(|(id, r, c)| {
                            out.tables
                                .get(*id)?
                                .logical
                                .0
                                .get(&(*r, *c))?
                                .get(&edge)
                                .copied()
                        })
                        .collect::<Option<Vec<_>>>();
                    if let Some(sizes) = sizes {
                        if let Some(&size) = sizes.first().filter(|s| sizes.iter().all(|v| v == *s))
                        {
                            logical.0.entry(target).or_default().insert(edge, size);
                        }
                    }
                }
            }
            out.tables.push(SourceTable {
                table: canonical(&group.table),
                field,
                logical,
                physical: CellPlan::default(),
            });
        }
        Ok(out)
    }
    pub fn from_pdf(pdf: &[u8], analysis: &Analysis) -> Result<Self> {
        let sources = kanpo_pdf::extract_pages_with_xobject_sources(pdf)?;
        let mut out = Self::default();
        if sources.len() != analysis.pages.len() {
            return Ok(out);
        }
        for (source, page) in sources.iter().zip(&analysis.pages) {
            let segments = segments(source);
            for region in &page.regions {
                let field = match region.role {
                    Role::After => Field::After,
                    Role::Before => Field::Before,
                    _ => Field::Paragraph,
                };
                for table in region.lines.iter().filter_map(|l| l.table.as_ref()) {
                    let (logical, physical) =
                        plans(table, &page.words, &segments).unwrap_or_default();
                    out.tables.push(SourceTable {
                        table: canonical(&table.to_nested()),
                        field,
                        logical,
                        physical,
                    });
                }
            }
        }
        Ok(out)
    }
    pub fn table(&self, t: &NestedTable, field: Field, physical: bool) -> CellPlan {
        let t = canonical(t);
        let mut scopes = BTreeMap::<(usize, usize), Vec<CellPlan>>::new();
        for source in self
            .tables
            .iter()
            .filter(|s| s.field == field && s.table.rows.len() <= t.rows.len())
        {
            let count = source.table.rows.len();
            if count == 0 {
                continue;
            }
            let starts = (0..=t.rows.len() - count)
                .filter(|&start| {
                    if t.rows[start..start + count] != source.table.rows {
                        return false;
                    }
                    if t.merges.iter().any(|m| {
                        m.row < start && m.row + m.rowspan > start
                            || m.row < start + count && m.row + m.rowspan > start + count
                    }) {
                        return false;
                    }
                    let merges = t
                        .merges
                        .iter()
                        .filter(|m| m.row >= start && m.row < start + count)
                        .map(|m| kanpo_parser::model::CellMerge {
                            row: m.row - start,
                            ..*m
                        })
                        .collect::<Vec<_>>();
                    merges == source.table.merges
                })
                .collect::<Vec<_>>();
            if starts.len() != 1 {
                continue;
            }
            let start = starts[0];
            let mut plan = CellPlan::default();
            if physical {
                for (&(r, c), sides) in &source.physical.0 {
                    plan.0
                        .insert((r, c + t.rows.len() - count - start), sides.clone());
                }
            } else {
                for (&(r, c), sides) in &source.logical.0 {
                    plan.0.insert((r + start, c), sides.clone());
                }
            }
            scopes.entry((start, count)).or_default().push(plan);
        }
        let mut merged = BTreeMap::<((usize, usize), Edge), Option<u8>>::new();
        for variants in scopes.values() {
            let Some(first) = variants
                .first()
                .filter(|first| variants.iter().all(|v| v == *first))
            else {
                continue;
            };
            for (&cell, sides) in &first.0 {
                for (&edge, &size) in sides {
                    merged
                        .entry((cell, edge))
                        .and_modify(|old| {
                            if *old != Some(size) {
                                *old = None;
                            }
                        })
                        .or_insert(Some(size));
                }
            }
        }
        let mut out = CellPlan::default();
        for ((cell, edge), size) in merged {
            if let Some(size) = size {
                out.0.entry(cell).or_default().insert(edge, size);
            }
        }
        out
    }
}
impl CellPlan {
    pub fn apply(
        &self,
        cell: TableCell,
        row: usize,
        col: usize,
        top: bool,
        bottom: bool,
    ) -> TableCell {
        let Some(edges) = self.0.get(&(row, col)) else {
            return cell;
        };
        let mut borders = TableCellBorders::with_empty();
        let mut any = false;
        for (&edge, &size) in edges {
            let side = match edge {
                Edge::Left => Position::Left,
                Edge::Right => Position::Right,
                Edge::Top if top => Position::Top,
                Edge::Bottom if bottom => Position::Bottom,
                _ => continue,
            };
            borders = borders.set(
                TableCellBorder::new(side)
                    .border_type(BorderType::Dashed)
                    .size(size as usize),
            );
            any = true;
        }
        if any {
            cell.set_borders(borders)
        } else {
            cell
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_partial_stroke_or_interior_underline_cannot_become_a_cell_edge() {
        let b = [10., 20., 110., 80.];
        let left = Segment {
            a: [8., 20.],
            b: [8., 80.],
            size: 3,
        };
        assert!(covers(left, Edge::Left, b, 2.8));
        assert!(!covers(
            Segment {
                a: [8., 21.],
                ..left
            },
            Edge::Left,
            b,
            2.8
        ));
        assert!(!covers(
            Segment {
                a: [14., 20.],
                b: [14., 80.],
                ..left
            },
            Edge::Left,
            b,
            2.8
        ));
        for (scale, dx, dy) in [(0.7, 45., 29.), (1.8, 17., 63.)] {
            let at = |p: [f64; 2]| [p[0] * scale + dx, p[1] * scale + dy];
            let a = at([b[0], b[1]]);
            let z = at([b[2], b[3]]);
            assert!(covers(
                Segment {
                    a: at(left.a),
                    b: at(left.b),
                    ..left
                },
                Edge::Left,
                [a[0], a[1], z[0], z[1]],
                2.8 * scale
            ));
        }
    }
    #[test]
    fn original_dashed_frames_survive_styles_without_changing_the_grid() {
        let pdf = include_bytes!("../../kanpo-parser/tests/data/workbook/case-04.pdf");
        let source = kanpo_parser::media::analyze_pdf(pdf).unwrap();
        let borders = Borders::from_pdf(pdf, &source.source_layout).unwrap();
        let mut found = 0;
        for r in &source.source_layout.pages[0].regions {
            for l in &r.lines {
                if let Some(t) = &l.table {
                    let field = if r.role == Role::After {
                        Field::After
                    } else {
                        Field::Before
                    };
                    let mut target = t.to_nested();
                    for run in target.rows.iter_mut().flatten().flatten() {
                        run.underline = true;
                    }
                    let plan = borders.table(&target, field, false);
                    assert!(!plan.0.is_empty());
                    found += 1;
                    let original = plan.clone();
                    let mut ambiguous = borders.clone();
                    let source = ambiguous
                        .tables
                        .iter()
                        .find(|s| s.field == field && s.table == canonical(&target))
                        .unwrap()
                        .clone();
                    ambiguous.tables.push(SourceTable {
                        logical: Default::default(),
                        physical: Default::default(),
                        ..source
                    });
                    assert!(
                        ambiguous.table(&target, field, false).0.is_empty(),
                        "conflicting equal source tables cannot choose a border style"
                    );
                    target.rows[2][0].push(Run::plain("changed source cell"));
                    assert!(borders.table(&target, field, false).0.is_empty());
                    assert!(!original.0.is_empty());
                }
            }
        }
        assert_eq!(found, 2);
    }

    #[test]
    fn continued_refined_tables_keep_the_dashed_edges_of_the_complete_original_body() {
        let pdf = include_bytes!("../../kanpo-parser/tests/data/workbook/case-09.pdf");
        let source = kanpo_parser::media::analyze_pdf(pdf).unwrap();
        let borders = Borders::from_pdf(pdf, &source.source_layout).unwrap();
        let layout = crate::layout::Layout::new(&source.source_layout);
        let mut documents = source.documents.clone();
        crate::continuations::apply(&mut documents, &layout);
        let mut found = 0;
        for d in documents {
            for b in d.blocks {
                if let kanpo_parser::media::Block::Shinkyu { rows, .. } = b {
                    for row in rows {
                        for (field, items, n) in
                            [(Field::After, row.after, 9), (Field::Before, row.before, 7)]
                        {
                            for item in items {
                                if let kanpo_parser::media::Inline::Table { table } = item {
                                    if table.rows.len() == n {
                                        found += usize::from(
                                            !borders
                                                .table(
                                                    &crate::docx::plain_table(&table),
                                                    field,
                                                    false,
                                                )
                                                .0
                                                .is_empty(),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(found, 2);
    }
}
