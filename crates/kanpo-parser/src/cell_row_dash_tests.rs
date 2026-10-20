use super::*;
fn word(ws: &mut Vec<Word>, x: f32, x1: f32, y: f32, text: &str, empty: bool) -> usize {
    let id = ws.len();
    ws.push(Word {
        id,
        x0: x,
        x1,
        y0: y - 4.,
        y1: y + 4.,
        raw: text.into(),
        text: if empty { String::new() } else { text.into() },
        status: if empty {
            WordStatus::Empty
        } else {
            WordStatus::Used
        },
    });
    id
}
fn record(ws: &mut Vec<Word>, y: f32, label: &str, n: usize) -> (String, Vec<usize>) {
    let mut ids = vec![word(ws, 90., 150., y, label, false)];
    let mut txt = label.to_string();
    for i in 0..n {
        ids.push(word(
            ws,
            190. + i as f32 * 40.,
            210. + i as f32 * 40.,
            y,
            "0.100",
            false,
        ));
        txt.push_str("　0.100");
    }
    (txt, ids)
}
fn fixture() -> (Vec<Unit>, Vec<Word>, Vec<Grid>) {
    let mut w = vec![];
    let mut cells = vec![];
    for (row, y) in [(1, 35.), (2, 50.), (3, 65.), (4, 95.)] {
        let (text, mut ids) = record(&mut w, y, "甲地区", 3);
        let mut text = text;
        if row == 3 {
            let (s, sub) = record(&mut w, 80., "乙地区", 1);
            text.push('\n');
            text.push_str(&s);
            ids.extend(sub);
        }
        cells.push(TableCell {
            row,
            col: 2,
            rowspan: 1,
            colspan: 1,
            x0: 80.,
            x1: 300.,
            y0: 20.,
            y1: 120.,
            text,
            words: ids,
        });
    }
    word(&mut w, 242., 250., 80., "\u{E61C}", true);
    word(&mut w, 282., 290., 80., "\u{E61C}", true);
    let t = Table {
        x0: 0.,
        x1: 300.,
        y0: 0.,
        y1: 140.,
        rows: 6,
        cols: 3,
        cells,
    };
    let g = Grid {
        xs: vec![0., 40., 80., 300.],
        ys: vec![0., 20., 120., 140.],
        cells: (0..3)
            .flat_map(|y0| {
                (0..3).map(move |x0| crate::grid::GridCell {
                    x0,
                    x1: x0 + 1,
                    y0,
                    y1: y0 + 1,
                })
            })
            .collect(),
    };
    (
        vec![Unit::Plain(vec![Line {
            text: t.render(),
            words: vec![],
            table: Some(t),
        }])],
        w,
        vec![g],
    )
}
fn table(u: &mut [Unit]) -> &mut Table {
    let Unit::Plain(ls) = &mut u[0] else {
        unreachable!()
    };
    ls[0].table.as_mut().unwrap()
}
fn unchanged(mut u: Vec<Unit>, mut w: Vec<Word>, g: Vec<Grid>) {
    let old = table(&mut u).clone();
    let ws = serde_json::to_value(&w).unwrap();
    restore_cell_record_dashes(&mut u, &mut w, &g);
    assert_eq!(*table(&mut u), old);
    assert_eq!(serde_json::to_value(&w).unwrap(), ws);
}
fn dash(w: &[Word]) -> usize {
    w.iter().position(|w| w.raw == "\u{E61C}").unwrap()
}
#[test]
fn restores_and_splits_only_verified_records() {
    let (mut u, mut w, g) = fixture();
    let before: Vec<_> = table(&mut u)
        .cells
        .iter()
        .flat_map(|c| c.words.iter().copied())
        .collect();
    restore_cell_record_dashes(&mut u, &mut w, &g);
    let t = table(&mut u);
    assert_eq!(t.rows, 7);
    assert_eq!(
        t.cells.iter().find(|c| c.row == 3).unwrap().text,
        "甲地区　0.100　0.100　0.100"
    );
    assert_eq!(
        t.cells.iter().find(|c| c.row == 4).unwrap().text,
        "乙地区　0.100　－　－"
    );
    assert_eq!(
        t.cells
            .iter()
            .flat_map(|c| c.words.iter().copied())
            .filter(|&i| w[i].raw != "\u{E61C}")
            .collect::<Vec<_>>(),
        before
    );
    let old = t.clone();
    restore_cell_record_dashes(&mut u, &mut w, &g);
    assert_eq!(*table(&mut u), old);
}
#[test]
fn unknown_pua_is_not_a_dash() {
    let (u, mut w, g) = fixture();
    let i = dash(&w);
    w[i].raw = "\u{E61D}".into();
    unchanged(u, w, g);
}
#[test]
fn unprinted_empty_slot_stays_empty() {
    let (u, mut w, g) = fixture();
    let i = dash(&w);
    w[i].raw.clear();
    unchanged(u, w, g);
}
#[test]
fn duplicate_raw_dash_is_ambiguous() {
    let (u, mut w, g) = fixture();
    let mut q = w[dash(&w)].clone();
    q.id = w.len();
    w.push(q);
    unchanged(u, w, g);
}
#[test]
fn already_used_foreign_dash_is_not_reused() {
    let (u, mut w, g) = fixture();
    let i = dash(&w);
    w[i].status = WordStatus::Used;
    w[i].text = "－".into();
    unchanged(u, w, g);
}
#[test]
fn dash_on_a_different_baseline_stays_empty() {
    let (u, mut w, g) = fixture();
    let i = dash(&w);
    w[i].y0 += 4.;
    w[i].y1 += 4.;
    unchanged(u, w, g);
}
#[test]
fn dash_without_right_alignment_stays_empty() {
    let (u, mut w, g) = fixture();
    let i = dash(&w);
    w[i].x0 += 3.;
    w[i].x1 += 3.;
    unchanged(u, w, g);
}
#[test]
fn single_numeric_column_is_not_expanded() {
    let (mut u, mut w, g) = fixture();
    for c in &mut table(&mut u).cells {
        if !c.text.contains('\n') {
            c.words.truncate(2);
            c.text = "甲地区　0.100".into();
        }
    }
    let i = dash(&w);
    w[i].x0 += 40.;
    w[i].x1 += 40.;
    unchanged(u, w, g);
}
#[test]
fn needs_two_complete_rows_above() {
    let (mut u, w, g) = fixture();
    table(&mut u).cells.retain(|c| c.row != 1 && c.row != 2);
    unchanged(u, w, g);
}
#[test]
fn needs_a_complete_row_below() {
    let (mut u, w, g) = fixture();
    table(&mut u).cells.retain(|c| c.row != 4);
    unchanged(u, w, g);
}
#[test]
fn other_column_on_the_same_logical_row_is_preserved() {
    let (mut u, mut w, g) = fixture();
    let id = word(&mut w, 45., 65., 65., "別欄", false);
    table(&mut u).cells.push(TableCell {
        row: 3,
        col: 1,
        rowspan: 1,
        colspan: 1,
        x0: 40.,
        x1: 80.,
        y0: 20.,
        y1: 120.,
        text: "別欄".into(),
        words: vec![id],
    });
    unchanged(u, w, g);
}
#[test]
fn spanning_logical_row_is_not_shifted() {
    let (mut u, w, g) = fixture();
    table(&mut u).cells[0].rowspan = 2;
    unchanged(u, w, g);
}
#[test]
fn spanning_target_column_is_not_split() {
    let (mut u, w, g) = fixture();
    table(&mut u).cells[2].colspan = 2;
    unchanged(u, w, g);
}
#[test]
fn mismatched_outer_frame_is_rejected() {
    let (u, w, mut g) = fixture();
    g[0].xs[0] -= 3.;
    unchanged(u, w, g);
}
#[test]
fn ambiguous_grids_are_rejected() {
    let (u, w, mut g) = fixture();
    g.push(g[0].clone());
    unchanged(u, w, g);
}
#[test]
fn merged_physical_cell_is_rejected() {
    let (u, w, mut g) = fixture();
    for c in &mut g[0].cells {
        if c.x0 == 2 && c.y0 == 1 {
            c.y1 = 3;
        }
    }
    unchanged(u, w, g);
}
#[test]
fn unmatched_existing_text_is_preserved() {
    let (mut u, w, g) = fixture();
    table(&mut u).cells[2].text.push('注');
    unchanged(u, w, g);
}
#[test]
fn existing_source_order_is_preserved() {
    let (mut u, w, g) = fixture();
    table(&mut u).cells[2].words.swap(0, 1);
    unchanged(u, w, g);
}
#[test]
fn ordinary_sentence_is_not_split() {
    let (mut u, mut w, g) = fixture();
    let id = table(&mut u).cells[2].words[4];
    w[id].text = "通常の文章である。".into();
    table(&mut u).cells[2].text = table(&mut u).cells[2]
        .text
        .replace("乙地区", "通常の文章である。");
    unchanged(u, w, g);
}
#[test]
fn glyph_crossing_the_original_cell_edge_is_rejected() {
    let (u, mut w, g) = fixture();
    let i = dash(&w) + 1;
    w[i].x0 += 12.;
    w[i].x1 += 12.;
    unchanged(u, w, g);
}
#[test]
fn another_unmapped_glyph_in_the_record_is_rejected() {
    let (u, mut w, g) = fixture();
    word(&mut w, 215., 223., 80., "※", true);
    unchanged(u, w, g);
}
#[test]
fn three_existing_lines_are_not_rebuilt() {
    let (mut u, w, g) = fixture();
    table(&mut u).cells[2].text.push_str("\n注記");
    unchanged(u, w, g);
}
