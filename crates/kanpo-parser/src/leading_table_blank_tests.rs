use super::*;
fn cell(row: usize, col: usize, text: &str) -> TableCell {
    TableCell {
        row,
        col,
        rowspan: 1,
        colspan: 1,
        x0: col as f32 * 40.,
        x1: (col + 1) as f32 * 40.,
        y0: row as f32 * 20.,
        y1: (row + 1) as f32 * 20.,
        text: text.into(),
        words: vec![row * 3 + col],
    }
}
fn table() -> Table {
    Table {
        x0: 0.,
        y0: 0.,
        x1: 120.,
        y1: 60.,
        rows: 3,
        cols: 3,
        cells: vec![
            cell(0, 1, "10"),
            cell(0, 2, "20"),
            cell(1, 1, "30"),
            cell(1, 2, "40"),
            cell(2, 0, "50"),
            cell(2, 1, "60"),
            cell(2, 2, "70"),
        ],
    }
}
fn table_line(t: Table) -> Line {
    Line {
        text: t.render(),
        words: vec![],
        table: Some(t),
    }
}
fn page(lines: Vec<Line>) -> PageTrace {
    let text = join_lines(&lines);
    PageTrace {
        native_styles:vec![],
        native_bounds: vec![],
        width: 842.,
        height: 595.,
        layout: "horizontal",
        words: vec![],
        regions: vec![Region {
            role: Role::Body,
            unit: 0,
            part: 0,
            lines,
        }],
        text,
        doc_text: String::new(),
        tables: vec![],
    }
}
#[test]
fn omitted_first_cells_keep_exactly_their_columns() {
    let t = table();
    let expected = t.render();
    assert_eq!(join_lines(&[table_line(t)]), expected);
}
#[test]
fn explicit_empty_first_cell_has_the_same_column_evidence() {
    let mut t = table();
    t.cells.push(cell(0, 0, ""));
    assert!(join_lines(&[table_line(t)]).starts_with("\u{3000}10\u{3000}20"));
}
#[test]
fn empty_first_two_columns_restore_two_separators() {
    let t = Table {
        x0: 0.,
        y0: 0.,
        x1: 120.,
        y1: 20.,
        rows: 1,
        cols: 3,
        cells: vec![cell(0, 2, "10")],
    };
    assert_eq!(join_lines(&[table_line(t)]), "\u{3000}\u{3000}10");
}
#[test]
fn no_table_metadata_means_no_column_inference() {
    let l = Line::text("\u{3000}10\u{3000}20".into(), vec![]);
    assert_eq!(join_lines(&[l]), "10\u{3000}20");
}
#[test]
fn stale_table_metadata_is_not_used() {
    let mut l = table_line(table());
    l.text.push_str(" stale");
    assert!(!join_lines(&[l]).starts_with('\u{3000}'));
}
#[test]
fn ordinary_indented_prose_and_table_cell_indentation_still_trim() {
    assert_eq!(join_lines(&[Line::text("  Ordinary prose.\u{3000}".into(), vec![])]), "Ordinary prose.");
    let mut t = table();
    t.cells.push(cell(0, 0, "\u{3000}Label"));
    assert_eq!(join_lines(&[table_line(t)]).lines().next(), Some("Label\u{3000}10\u{3000}20"));
}
#[test]
fn empty_boundary_lines_are_removed_without_stripping_columns() {
    let ls = vec![Line::text(" \n".into(), vec![]), table_line(table()), Line::text(" \n".into(), vec![])];
    assert_eq!(join_lines(&ls), table().render());
}
#[test]
fn marker_serialization_does_not_add_column_separators() {
    let line = table_line(table());
    assert_eq!(join_lines_with(&[line], &mut |_| table_marker(0)), table_marker(0));
}
#[test]
fn second_and_later_rows_are_unchanged() {
    let text = join_lines(&[table_line(table())]);
    assert_eq!(text.lines().nth(1), Some("\u{3000}30\u{3000}40"));
    assert_eq!(text.lines().nth(2), Some("50\u{3000}60\u{3000}70"));
}
#[test]
fn page_join_keeps_first_table_metadata_and_later_page_columns() {
    let t = page(vec![table_line(table())]);
    assert!(join_page_texts(&[t.clone()], &|p| &p.text).starts_with('\u{3000}'));
    let prose = page(vec![Line::text("  Ordinary prose.  ".into(), vec![])]);
    let joined = join_page_texts(&[prose, t], &|p| &p.text);
    assert!(joined.starts_with("Ordinary prose.\n\u{3000}10"));
}
#[test]
fn missing_metadata_at_page_join_still_trims() {
    let mut p = page(vec![Line::text("Ordinary prose.".into(), vec![])]);
    p.text = "\u{3000}10\u{3000}20".into();
    assert_eq!(join_page_texts(&[p], &|p| &p.text), "10\u{3000}20");
}
#[test]
fn shinkyu_table_preamble_keeps_columns_but_not_before_label_indent() {
    let pre = table_line(table());
    let u = Unit::Shinkyu {
        preamble: vec![pre],
        after: vec![vec![Line::text("new".into(), vec![])]],
        before: vec![vec![Line::text("old".into(), vec![])]],
        outside: vec![],
    };
    assert!(u.text().starts_with("\u{3000}10"));
    let u = Unit::Shinkyu {
        preamble: vec![],
        after: vec![vec![table_line(table())]],
        before: vec![],
        outside: vec![],
    };
    assert!(u.text().starts_with("改正後\n\u{3000}10"));
}

#[test]
fn out_of_range_table_metadata_keeps_plain_trim_without_rendering_invalid_columns() {
    for (rows, cols, row, col) in [(1, 0, 0, 0), (1, 3, 0, 3), (0, 3, 0, 1), (3, 3, 3, 1)] {
        let mut t = table();
        t.rows = rows;
        t.cols = cols;
        t.cells = vec![cell(row, col, "10")];
        let l = Line { text: "\u{3000}10".into(), words: vec![], table: Some(t) };
        assert_eq!(join_lines(&[l.clone()]), "10");
        let p = page(vec![l]);
        assert_eq!(join_page_texts(&[p], &|p| &p.text), "10");
    }
}
