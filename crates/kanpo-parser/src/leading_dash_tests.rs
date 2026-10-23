use super::*;
use crate::grid::{Grid, GridCell};
fn fixture() -> (Vec<Unit>, Vec<Word>, Vec<Grid>) {
    let mut words = vec![];
    let mut cells = vec![];
    for row in 0..3 {
        for col in 0..2 {
            let id = words.len();
            let (x, y) = (col as f32 * 40. + 8., (row + 2) as f32 * 20. + 5.);
            let text = format!("{}", 100 + row + col * 100);
            words.push(Word {
                source_box: [x, y, x + 8., y + 8.],
                id,
                x0: x,
                y0: y,
                x1: x + 8.,
                y1: y + 8.,
                raw: text.clone(),
                text: text.clone(),
                status: WordStatus::Used,
            });
            cells.push(TableCell {
                row,
                col,
                rowspan: 1,
                colspan: 1,
                x0: col as f32 * 40.,
                x1: (col + 1) as f32 * 40.,
                y0: (row + 2) as f32 * 20.,
                y1: (row + 3) as f32 * 20.,
                text,
                words: vec![id],
            });
        }
    }
    for row in 0..2 {
        for col in 0..2 {
            let id = words.len();
            let (x, y) = (col as f32 * 40. + 8., row as f32 * 20. + 5.);
            words.push(Word {
                source_box: [x, y, x + 8., y + 8.],
                id,
                x0: x,
                y0: y,
                x1: x + 8.,
                y1: y + 8.,
                raw: "\u{E61C}".into(),
                text: String::new(),
                status: WordStatus::Empty,
            });
        }
    }
    let t = Table {
        x0: 0.,
        y0: 0.,
        x1: 80.,
        y1: 100.,
        rows: 3,
        cols: 2,
        cells,
    };
    let units = vec![Unit::Plain(vec![Line {
        text: t.render(),
        words: vec![],
        table: Some(t),
    }])];
    let grid = Grid {
        xs: vec![0., 40., 80.],
        ys: vec![0., 20., 40., 60., 80., 100.],
        cells: (0..5)
            .flat_map(|y| {
                (0..2).map(move |x| GridCell {
                    x0: x,
                    x1: x + 1,
                    y0: y,
                    y1: y + 1,
                })
            })
            .collect(),
    };
    (units, words, vec![grid])
}
fn table(u: &mut [Unit]) -> &mut Table {
    let Unit::Plain(l) = &mut u[0] else { unreachable!() };
    l[0].table.as_mut().unwrap()
}
fn run(u: &mut [Unit], w: &mut [Word], g: &[Grid]) {
    restore_partitioned_table_dashes(u, w, g);
}
fn unchanged(mut u: Vec<Unit>, mut w: Vec<Word>, g: Vec<Grid>) {
    let Unit::Plain(l) = &mut u[0] else { unreachable!() };
    for line in l {
        if let Some(t) = &line.table {
            line.text = t.render();
        }
    }
    let before = u[0].text();
    run(&mut u, &mut w, &g);
    assert_eq!(u[0].text(), before);
    for word in &w[6..] {
        assert_eq!(word.status, WordStatus::Empty);
    }
}
#[test]
fn leading_numeric_data_rows_restore_only_raw_bars_and_are_idempotent() {
    let (mut u, mut w, g) = fixture();
    run(&mut u, &mut w, &g);
    assert_eq!(u[0].text(), "－　－\n－　－\n100　200\n101　201\n102　202");
    let ids = table(&mut u)
        .cells
        .iter()
        .flat_map(|c| c.words.iter().copied())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(ids.len(), 10);
    let once = u[0].text();
    run(&mut u, &mut w, &g);
    assert_eq!(u[0].text(), once);
    assert_eq!(table(&mut u).cells.iter().map(|c| c.words.len()).sum::<usize>(), 10);
}
#[test]
fn blank_physical_and_logical_rows_are_not_filled() {
    let (mut u, mut w, mut g) = fixture();
    let t = table(&mut u);
    t.rows = 4;
    t.y1 = 120.;
    for c in &mut t.cells {
        if c.row == 2 {
            c.row = 3;
            c.y0 = 100.;
            c.y1 = 120.;
            for id in &c.words {
                w[*id].y0 = 105.;
                w[*id].y1 = 113.;
            }
        }
    }
    g[0].ys.push(120.);
    for x in 0..2 {
        g[0].cells.push(GridCell {
            x0: x,
            x1: x + 1,
            y0: 5,
            y1: 6,
        });
    }
    run(&mut u, &mut w, &g);
    let t = table(&mut u);
    assert_eq!(t.rows, 6);
    assert!(!t.cells.iter().any(|c| c.row == 4));
    assert!(t.cells.iter().any(|c| c.row == 5 && c.text == "102"));
    assert_eq!(u[0].text().lines().count(), 5);
}
#[test]
fn a_subject_label_keeps_the_original_first_numeric_guard() {
    let (mut u, w, g) = fixture();
    table(&mut u).cells[0].text = "資産".into();
    unchanged(u, w, g);
}
#[test]
fn a_header_keeps_the_original_guard() {
    let (mut u, w, g) = fixture();
    table(&mut u).cells[0].text = "令和８年度".into();
    unchanged(u, w, g);
}
#[test]
fn merged_top_cells_are_not_assumed_to_be_data_rows() {
    let (u, w, mut g) = fixture();
    g[0].cells.retain(|c| c.y0 != 0);
    g[0].cells.push(GridCell { x0: 0, x1: 2, y0: 0, y1: 1 });
    unchanged(u, w, g);
}
#[test]
fn missing_column_boundaries_do_not_trigger_recovery() {
    let (u, w, mut g) = fixture();
    g[0].xs = vec![0., 80.];
    unchanged(u, w, g);
}
#[test]
fn two_matching_physical_grids_are_ambiguous() {
    let (u, w, mut g) = fixture();
    g.push(g[0].clone());
    unchanged(u, w, g);
}
#[test]
fn unknown_private_glyphs_are_not_filled() {
    let (u, mut w, g) = fixture();
    for x in &mut w[6..] {
        x.raw = "\u{EFFF}".into();
    }
    unchanged(u, w, g);
}
#[test]
fn duplicate_raw_bars_in_one_physical_cell_are_not_assigned() {
    let (mut u, mut w, g) = fixture();
    let mut dup = w[6].clone();
    dup.id = w.len();
    dup.x0 += 0.1;
    dup.x1 += 0.1;
    w.push(dup);
    run(&mut u, &mut w, &g);
    assert_eq!(w[6].status, WordStatus::Empty);
    assert_eq!(w[10].status, WordStatus::Empty);
    assert_eq!(w[7..10].iter().filter(|x| x.status == WordStatus::Used).count(), 3);
}
#[test]
fn a_bar_crossing_a_cell_edge_is_not_assigned() {
    let (mut u, mut w, g) = fixture();
    w[6].x0 = 37.;
    w[6].x1 = 45.;
    run(&mut u, &mut w, &g);
    assert_eq!(w[6].status, WordStatus::Empty);
}
#[test]
fn a_glyph_already_used_in_another_article_is_not_duplicated() {
    let (mut u, mut w, g) = fixture();
    w[6].status = WordStatus::Used;
    w[6].text = "－".into();
    let Unit::Plain(l) = &mut u[0] else { unreachable!() };
    l.push(Line::text("－".into(), vec![6]));
    run(&mut u, &mut w, &g);
    assert!(!table(&mut u).cells.iter().any(|c| c.words.contains(&6)));
}
