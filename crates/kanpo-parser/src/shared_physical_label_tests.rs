use super::*;
fn fixture() -> (Table, Vec<Word>) {
    let mut ws = Vec::new();
    let mut word = |x, y, width, text: &str| {
        let id = ws.len();
        ws.push(Word {
            id,
            x0: x,
            x1: x + width,
            y0: y,
            y1: y + 8.0,
            raw: text.into(),
            text: text.into(),
            status: WordStatus::Used,
        });
        id
    };
    let a = word(4., 4., 80., "コンビナシオネス・セ");
    let b = word(4., 12., 40., "レクシオン");
    let mut cells = Vec::new();
    for (row, id, text) in [(0, a, "コンビナシオネス・セ"), (1, b, "レクシオン")] {
        cells.push(TableCell {
            row,
            col: 0,
            rowspan: 1,
            colspan: 1,
            x0: 0.,
            y0: 0.,
            x1: 88.,
            y1: 32.,
            words: vec![id],
            text: text.into(),
        });
    }
    for row in 0..2 {
        for col in 1..3 {
            let x = 88. + (col - 1) as f32 * 52.;
            let y = row as f32 * 16.;
            let id = word(x + 12., y + 4., 32., "100円");
            cells.push(TableCell {
                row,
                col,
                rowspan: 1,
                colspan: 1,
                x0: x,
                y0: y,
                x1: x + 52.,
                y1: y + 16.,
                words: vec![id],
                text: "100円".into(),
            });
        }
    }
    (
        Table {
            x0: 0.,
            y0: 0.,
            x1: 192.,
            y1: 32.,
            rows: 2,
            cols: 3,
            cells,
        },
        ws,
    )
}
#[test]
fn one_wrapped_source_cell_keeps_all_words_and_both_numeric_records() {
    let (mut t, w) = fixture();
    let original = t.clone();
    let ids = |t: &Table| {
        let mut ids: Vec<_> = t.cells.iter().flat_map(|c| c.words.iter().copied()).collect();
        ids.sort();
        ids
    };
    assert_eq!(join_wrapped_physical_label(&mut t, &w), 1);
    assert_eq!(t.cells[0].text, "コンビナシオネス・セレクシオン");
    assert_eq!(t.cells[0].rowspan, 2);
    assert_eq!(ids(&original), ids(&t));
    assert_eq!(original.cells[2..], t.cells[1..]);
    assert_eq!(join_wrapped_physical_label(&mut t, &w), 0);
}
#[test]
fn separate_labels_cell_borders_and_row_anchors_are_not_merged() {
    for case in 0..15 {
        let (mut t, mut w) = fixture();
        match case {
            0 => t.cells[1].y0 += 16.,
            1 => w[0].x1 -= 24.,
            2 => {
                w[1].x0 += 8.;
                w[1].x1 += 8.;
            }
            3 => {
                w[1].y0 += 8.;
                w[1].y1 += 8.;
            }
            4 => {
                let mut c = t.cells[1].clone();
                c.row = 2;
                t.cells.push(c);
                t.rows = 3;
            }
            5 => t.cells[1].text = "令和３年".into(),
            6 => t.cells[0].text = "文章である。".into(),
            7 => t.cells[0].col = 1,
            8 => t.cells.retain(|c| c.col != 2),
            9 => {
                for x in &mut w[4..] {
                    x.y0 -= 8.;
                    x.y1 -= 8.;
                }
            }
            10 => t.cells[5].col = 3,
            11 => t.cells[1].words = t.cells[0].words.clone(),
            12 => t.cells[1].rowspan = 2,
            13 => {
                w[0].y0 -= 12.;
                w[0].y1 -= 12.;
            }
            14 => {
                w[5].y0 += 6.;
                w[5].y1 += 6.;
            }
            _ => unreachable!(),
        };
        let before = t.clone();
        assert_eq!(join_wrapped_physical_label(&mut t, &w), 0, "negative {case}");
        assert_eq!(t, before, "negative {case}");
    }
}
#[test]
fn the_physical_cell_can_cover_more_records_than_its_two_text_lines() {
    let (mut t, mut w) = fixture();
    t.rows = 4;
    t.y1 = 64.;
    t.cells[0].y1 = 64.;
    t.cells[1].y1 = 64.;
    for row in 2..4 {
        for col in 1..3 {
            let x = 88. + (col - 1) as f32 * 52.;
            let y = row as f32 * 16.;
            let id = w.len();
            w.push(Word {
                id,
                x0: x + 12.,
                x1: x + 44.,
                y0: y + 4.,
                y1: y + 12.,
                raw: "100円".into(),
                text: "100円".into(),
                status: WordStatus::Used,
            });
            t.cells.push(TableCell {
                row,
                col,
                rowspan: 1,
                colspan: 1,
                x0: x,
                y0: y,
                x1: x + 52.,
                y1: y + 16.,
                words: vec![id],
                text: "100円".into(),
            });
        }
    }
    assert_eq!(join_wrapped_physical_label(&mut t, &w), 1);
    assert_eq!(t.cells[0].rowspan, 4);
    assert_eq!(t.rows, 4);
}
