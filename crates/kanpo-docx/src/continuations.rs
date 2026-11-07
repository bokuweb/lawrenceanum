//! Reconcile a source header with a refined grid on the next source page.
use crate::{
    docx::plain_table,
    layout::{header_rows, Layout},
};
use kanpo_parser::{
    media::{Block, Document, Inline, Table},
    model::CellMerge,
};

fn omitted_header(t: &Table) -> bool {
    let plain = plain_table(t);
    let n = header_rows(&plain);
    n > 0
        && n + 1 == t.rows.len()
        && plain.rows[n].iter().all(|c| {
            let text = c.iter().map(|r| r.text.as_str()).collect::<String>();
            let text = text.trim();
            text.is_empty()
                || ((text.starts_with('〔') && text.ends_with('〕')
                    || text.starts_with('［') && text.ends_with('］'))
                    && text.chars().count() <= 8)
        })
}

fn refine(t: &Table, old: &[f32], fine: &[f32]) -> Option<Table> {
    let find = |x: f32| {
        let p = fine
            .iter()
            .enumerate()
            .filter(|(_, v)| (**v - x).abs() <= 1.)
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        (p.len() == 1).then(|| p[0])
    };
    let mut out = Table {
        rows: vec![vec![vec![]; fine.len() - 1]; t.rows.len()],
        merges: Vec::new(),
    };
    for (r, row) in t.rows.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            let m = t.merges.iter().find(|m| {
                m.row <= r && r < m.row + m.rowspan && m.col <= c && c < m.col + m.colspan
            });
            if m.is_some_and(|m| m.row != r || m.col != c) {
                continue;
            }
            let (rs, cs) = m.map_or((1, 1), |m| (m.rowspan, m.colspan));
            let a = find(*old.get(c)?)?;
            let b = find(*old.get(c + cs)?)?;
            if b <= a {
                return None;
            }
            out.rows[r][a] = cell.clone();
            if rs > 1 || b - a > 1 {
                out.merges.push(CellMerge {
                    row: r,
                    col: a,
                    rowspan: rs,
                    colspan: b - a,
                });
            }
        }
    }
    Some(out)
}

fn joined(a: &Table, b: &Table, layout: &Layout<'_>) -> Option<Table> {
    if !omitted_header(a) {
        return None;
    }
    let ap = plain_table(a);
    let bp = plain_table(b);
    let (page, role, axis, old) = layout.source_columns(&ap)?;
    let (next, other_role, other_axis, fine) = layout.source_columns(&bp)?;
    if next != page + 1
        || role != other_role
        || axis != other_axis
        || fine.len() <= old.len()
        || (fine.last()? - old.last()?).abs() > 1.
    {
        return None;
    }
    let mut out = refine(a, &old, &fine)?;
    let offset = out.rows.len();
    out.rows.extend(b.rows.clone());
    out.merges.extend(b.merges.iter().map(|m| CellMerge {
        row: m.row + offset,
        ..*m
    }));
    layout.register_columns(&plain_table(&out), fine);
    Some(out)
}

fn runs(items: &mut Vec<Inline>, layout: &Layout<'_>) {
    for item in items.iter_mut() {
        if let Inline::Table { table } = item {
            for row in &mut table.rows {
                for c in row {
                    runs(c, layout)
                }
            }
        }
    }
    let mut i = 0;
    while i + 1 < items.len() {
        let combined = match (&items[i], &items[i + 1]) {
            (Inline::Table { table: a }, Inline::Table { table: b }) => joined(a, b, layout),
            _ => None,
        };
        if let Some(table) = combined {
            items.splice(i..i + 2, [Inline::Table { table }]);
        } else {
            i += 1;
        }
    }
}

pub(crate) fn apply(documents: &mut [Document], layout: &Layout<'_>) {
    for doc in documents {
        for block in &mut doc.blocks {
            match block {
                Block::Paragraph { runs: r } => runs(r, layout),
                Block::Shinkyu { rows, .. } => {
                    for row in rows {
                        runs(&mut row.after, layout);
                        runs(&mut row.before, layout);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn next_page_refinements_expand_the_original_header_without_losing_text() {
        let pdf = include_bytes!("../../kanpo-parser/tests/data/workbook/case-09.pdf");
        for (variant, a) in [
            ("native", crate::analyze(pdf, &[]).unwrap()),
            (
                "source-underlined",
                kanpo_parser::source_underlines::analyze_pdf(pdf)
                    .unwrap()
                    .styled,
            ),
        ] {
            let layout = Layout::new(&a);
            let mut docs = a
                .articles
                .iter()
                .map(|a| Document::from(&a.document))
                .collect::<Vec<_>>();
            let fields = [
                kanpo_parser::media::Field::Paragraph,
                kanpo_parser::media::Field::After,
                kanpo_parser::media::Field::Before,
            ];
            let before = fields.map(|f| docs.iter().map(|d| d.field_text(f)).collect::<String>());
            apply(&mut docs, &layout);
            assert_eq!(
                before,
                fields.map(|f| docs.iter().map(|d| d.field_text(f)).collect::<String>())
            );
            let mut count = 0;
            for doc in docs {
                for block in doc.blocks {
                    if let Block::Shinkyu { rows, .. } = block {
                        for row in rows {
                            for (side, runs, cols, rows, span) in
                                [(true, row.after, 5, 9, 3), (false, row.before, 4, 7, 2)]
                            {
                                for item in runs {
                                    if let Inline::Table { table } = item {
                                        if table.rows.len() != rows {
                                            continue;
                                        }
                                        count += 1;
                                        assert_eq!(table.rows[0].len(), cols);
                                        assert!(table.merges.contains(&CellMerge {
                                            row: 0,
                                            col: 0,
                                            rowspan: 2,
                                            colspan: span
                                        }));
                                        let widths = layout.columns(&plain_table(&table), 3280);
                                        assert_eq!(
                                            widths,
                                            if side {
                                                vec![1040, 640, 640, 480, 480]
                                            } else {
                                                vec![1040, 1280, 480, 480]
                                            }
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
            assert_eq!(
                count, 2,
                "{variant}: text decoration must not disable the source geometry proof"
            );
        }
    }

    #[test]
    fn a_different_source_role_cannot_supply_the_continuation_grid() {
        let pdf = include_bytes!("../../kanpo-parser/tests/data/workbook/case-09.pdf");
        let mut a = crate::analyze(pdf, &[]).unwrap();
        for r in &mut a.pages[1].regions {
            if r.role == kanpo_parser::trace::Role::After {
                r.role = kanpo_parser::trace::Role::Body;
            }
        }
        let layout = Layout::new(&a);
        let mut docs = a
            .articles
            .iter()
            .map(|a| Document::from(&a.document))
            .collect::<Vec<_>>();
        apply(&mut docs, &layout);
        let mut joined_after = 0;
        for d in docs {
            for b in d.blocks {
                if let Block::Shinkyu { rows, .. } = b {
                    for r in rows {
                        for item in r.after {
                            if let Inline::Table { table } = item {
                                if table.rows.len() == 9 {
                                    joined_after += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(joined_after, 0);
    }
}
