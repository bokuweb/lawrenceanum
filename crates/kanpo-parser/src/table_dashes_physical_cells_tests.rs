use super::*;
use crate::grid::{Grid, GridCell};

fn setup() -> (Vec<Unit>, Vec<Word>, Vec<Grid>) {
    let w = |id, x: f32, y: f32, raw: &str, status| Word {
        id,
        x0: x,
        y0: y,
        x1: x + 8.,
        y1: y + 8.,
        raw: raw.into(),
        text: if status == WordStatus::Empty { String::new() } else { raw.into() },
        status,
    };
    let words = vec![
        w(0, 5., 5., "区分", WordStatus::Used),
        w(1, 55., 46., "100円", WordStatus::Used),
        w(2, 65., 6., "\u{E61C}", WordStatus::Empty),
    ];
    let cell = |row, col, rowspan, x0, y0, x1, y1, text: &str, ids| TableCell {
        row,
        col,
        rowspan,
        colspan: 1,
        x0,
        y0,
        x1,
        y1,
        text: text.into(),
        words: ids,
    };
    let t = Table {
        x0: 0.,
        y0: 0.,
        x1: 80.,
        y1: 60.,
        rows: 3,
        cols: 2,
        cells: vec![
            cell(0, 0, 1, 0., 0., 40., 40., "区分", vec![0]),
            cell(2, 1, 1, 40., 40., 80., 60., "100円", vec![1]),
        ],
    };
    let g = Grid {
        xs: vec![0., 40., 80.],
        ys: vec![0., 20., 40., 60.],
        cells: vec![
            GridCell { x0: 0, x1: 1, y0: 0, y1: 2 },
            GridCell { x0: 0, x1: 1, y0: 2, y1: 3 },
            GridCell { x0: 1, x1: 2, y0: 0, y1: 1 },
            GridCell { x0: 1, x1: 2, y0: 1, y1: 2 },
            GridCell { x0: 1, x1: 2, y0: 2, y1: 3 },
        ],
    };
    (
        vec![Unit::Plain(vec![Line {
            text: t.render(),
            words: vec![],
            table: Some(t),
        }])],
        words,
        vec![g],
    )
}
fn t(u: &mut [Unit]) -> &mut Table {
    if let Unit::Plain(l) = &mut u[0] {
        l[0].table.as_mut().unwrap()
    } else {
        unreachable!()
    }
}
fn apply(u: &mut [Unit], w: &mut [Word], g: &[Grid]) {
    restore_existing_physical_table_dashes(u, w, g)
}
fn untouched(u: &mut [Unit], w: &mut [Word], g: &[Grid]) {
    let before = format!("{:?}", t(u));
    let old = w.to_vec();
    apply(u, w, g);
    assert_eq!(before, format!("{:?}", t(u)));
    for (a, b) in old.iter().zip(w) {
        assert_eq!(a.text, b.text);
        assert_eq!(a.status, b.status);
    }
}
#[test]
fn merged_label_is_preserved() {
    let (mut u, mut w, g) = setup();
    let old = t(&mut u).cells.clone();
    apply(&mut u, &mut w, &g);
    assert_eq!(&t(&mut u).cells[..2], old);
    let c = t(&mut u).cells.last().unwrap();
    assert_eq!((c.row, c.col, c.rowspan, c.colspan), (0, 1, 1, 1));
    assert_eq!(c.words, vec![2]);
    assert_eq!(w[2].status, WordStatus::Used);
    let after = t(&mut u).clone();
    apply(&mut u, &mut w, &g);
    assert_eq!(*t(&mut u), after);
}
#[test]
fn existing_empty_cell_is_filled() {
    let (mut u, mut w, g) = setup();
    t(&mut u).cells.push(TableCell {
        row: 0,
        col: 1,
        rowspan: 1,
        colspan: 1,
        x0: 40.,
        y0: 0.,
        x1: 80.,
        y1: 20.,
        text: String::new(),
        words: vec![],
    });
    apply(&mut u, &mut w, &g);
    assert_eq!(t(&mut u).cells.len(), 3);
    assert_eq!(t(&mut u).cells[2].words, vec![2]);
}
#[test]
fn original_merged_rowspan_is_accepted() {
    let (mut u, mut w, g) = setup();
    t(&mut u).cells[0].rowspan = 2;
    apply(&mut u, &mut w, &g);
    assert_eq!(w[2].status, WordStatus::Used);
    assert_eq!(t(&mut u).cells[0].rowspan, 2);
}
#[test]
fn unknown_pua() {
    let (mut u, mut w, g) = setup();
    w[2].raw = "\u{E61D}".into();
    untouched(&mut u, &mut w, &g);
}
#[test]
fn already_used() {
    let (mut u, mut w, g) = setup();
    w[2].status = WordStatus::Used;
    untouched(&mut u, &mut w, &g);
}
#[test]
fn glyph_crosses_edge() {
    let (mut u, mut w, g) = setup();
    w[2].x1 = 81.;
    untouched(&mut u, &mut w, &g);
}
#[test]
fn glyph_is_oversized() {
    let (mut u, mut w, g) = setup();
    w[2].y1 = 30.;
    untouched(&mut u, &mut w, &g);
}
#[test]
fn wrong_logical_rows() {
    let (mut u, mut w, g) = setup();
    t(&mut u).rows = 2;
    untouched(&mut u, &mut w, &g);
}
#[test]
fn wrong_logical_columns() {
    let (mut u, mut w, g) = setup();
    t(&mut u).cols = 3;
    untouched(&mut u, &mut w, &g);
}
#[test]
fn wrong_outer_bounds() {
    let (mut u, mut w, mut g) = setup();
    t(&mut u).x1 = 79.;
    g[0].xs[2] = 82.;
    untouched(&mut u, &mut w, &g);
}
#[test]
fn packed_row() {
    let (mut u, mut w, g) = setup();
    t(&mut u).cells[1].row = 1;
    untouched(&mut u, &mut w, &g);
}
#[test]
fn word_outside_original_cell() {
    let (mut u, mut w, g) = setup();
    w[0].x1 = 45.;
    untouched(&mut u, &mut w, &g);
}
#[test]
fn ambiguous_original_grids() {
    let (mut u, mut w, mut g) = setup();
    g.push(g[0].clone());
    untouched(&mut u, &mut w, &g);
}
#[test]
fn ambiguous_raw() {
    let (mut u, mut w, g) = setup();
    let mut d = w[2].clone();
    d.id = 3;
    w.push(d);
    untouched(&mut u, &mut w, &g);
}
#[test]
fn ambiguous_raw_including_used() {
    let (mut u, mut w, g) = setup();
    let mut d = w[2].clone();
    d.id = 3;
    d.status = WordStatus::Used;
    w.push(d);
    untouched(&mut u, &mut w, &g);
}
#[test]
fn merged_target() {
    let (mut u, mut w, mut g) = setup();
    g[0].cells.retain(|c| !(c.x0 == 1 && c.y0 < 2));
    g[0].cells.push(GridCell { x0: 1, x1: 2, y0: 0, y1: 2 });
    untouched(&mut u, &mut w, &g);
}
#[test]
fn overlapping_physical_bounds() {
    let (mut u, mut w, mut g) = setup();
    g[0].cells.push(GridCell { x0: 0, x1: 2, y0: 0, y1: 2 });
    untouched(&mut u, &mut w, &g);
}
#[test]
fn occupied_target() {
    let (mut u, mut w, g) = setup();
    let mut c = t(&mut u).cells[1].clone();
    c.row = 0;
    c.y0 = 0.;
    c.y1 = 20.;
    c.words = vec![];
    t(&mut u).cells.push(c);
    untouched(&mut u, &mut w, &g);
}
#[test]
fn empty_text_with_source_words() {
    let (mut u, mut w, g) = setup();
    let mut c = t(&mut u).cells[1].clone();
    c.row = 0;
    c.y0 = 0.;
    c.y1 = 20.;
    c.text = String::new();
    c.words = vec![2];
    t(&mut u).cells.push(c);
    untouched(&mut u, &mut w, &g);
}
#[test]
fn duplicate_empty_cells() {
    let (mut u, mut w, g) = setup();
    let mut c = t(&mut u).cells[1].clone();
    c.row = 0;
    c.y0 = 0.;
    c.y1 = 20.;
    c.text = String::new();
    c.words = vec![];
    t(&mut u).cells.push(c.clone());
    t(&mut u).cells.push(c);
    untouched(&mut u, &mut w, &g);
}
#[test]
fn raw_outside_table() {
    let (mut u, mut w, g) = setup();
    w[2].x0 = 85.;
    w[2].x1 = 93.;
    untouched(&mut u, &mut w, &g);
}
