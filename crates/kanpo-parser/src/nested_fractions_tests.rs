use super::*;
use crate::trace::{Table, TableCell};
fn fixture() -> (Vec<Unit>, Vec<Word>, Vec<(f32, f32, f32)>) {
    let positions = [
        (10, [595., 458.002, 687., 466.186], "A＋B×C"),
        (20, [553., 469.002, 705., 477.186], "D＋E"),
        (30, [527., 474.503, 543., 482.687], "１＋"),
        (40, [623., 480.003, 635., 488.187], "100"),
        (50, [715., 474.503, 755., 482.687], "×F"),
        (
            60,
            [543.8597, 469.6429, 552.0437, 485.6429],
            "\u{eae9}\u{eaea}",
        ),
        (
            70,
            [707.8597, 469.6429, 716.0437, 485.6429],
            "\u{eaeb}\u{eaec}",
        ),
        (100, [525., 440., 700., 448.], "前文"),
        (200, [525., 500., 600., 508.], "別セル"),
    ];
    let words = positions
        .into_iter()
        .map(|(id, b, t)| Word {
            source_box: [b[0], b[1], b[2], b[3]],
            id,
            x0: b[0],
            y0: b[1],
            x1: b[2],
            y1: b[3],
            raw: t.into(),
            text: if id == 60 || id == 70 {
                String::new()
            } else {
                t.into()
            },
            status: if id == 60 || id == 70 {
                WordStatus::Empty
            } else {
                WordStatus::Used
            },
        })
        .collect();
    let table = Table {
        x0: 520.,
        y0: 435.,
        x1: 790.,
        y1: 550.,
        rows: 2,
        cols: 1,
        cells: vec![
            TableCell {
                row: 0,
                col: 0,
                rowspan: 1,
                colspan: 1,
                x0: 520.,
                y0: 435.,
                x1: 790.,
                y1: 550.,
                text: "前文\nA＋B×C\nD＋E\n１＋　×F\n100\n後文".into(),
                words: vec![100, 10, 20, 30, 50, 40],
            },
            TableCell {
                row: 1,
                col: 0,
                rowspan: 1,
                colspan: 1,
                x0: 520.,
                y0: 435.,
                x1: 790.,
                y1: 550.,
                text: "別セル".into(),
                words: vec![200],
            },
        ],
    };
    let line = Line {
        text: table.render(),
        words: vec![100, 10, 20, 30, 50, 40, 200],
        table: Some(table),
    };
    (
        vec![Unit::Plain(vec![line])],
        words,
        vec![(466.643, 525., 757.), (477.643, 551., 707.)],
    )
}
fn first(units: &[Unit]) -> &Line {
    let Unit::Plain(lines) = &units[0] else {
        panic!()
    };
    &lines[0]
}
#[test]
fn restore_only_verified_text_and_resync_table_render() {
    let (mut u, w, b) = fixture();
    let before = first(&u).table.clone().unwrap();
    let raw = format!("{w:?}");
    restore(&mut u, &w, &b);
    let l = first(&u);
    let t = l.table.as_ref().unwrap();
    assert_eq!(t.cells[0].text, "前文\n(A＋B×C)/(１＋((D＋E)/100)×F)\n後文");
    assert_eq!(t.cells[0].words, before.cells[0].words);
    assert_eq!(t.cells[1], before.cells[1]);
    assert_eq!(t.rows, before.rows);
    assert_eq!(t.cols, before.cols);
    assert_eq!(l.text, t.render());
    assert_eq!(raw, format!("{w:?}"));
}
#[test]
fn second_pass_is_idempotent() {
    let (mut u, w, b) = fixture();
    restore(&mut u, &w, &b);
    let old = first(&u).clone();
    restore(&mut u, &w, &b);
    assert_eq!(first(&u), &old);
}
#[test]
fn repeated_compact_formula_in_one_cell_is_not_replaced() {
    let (mut u, w, b) = fixture();
    let Unit::Plain(l) = &mut u[0] else { panic!() };
    let c = &mut l[0].table.as_mut().unwrap().cells[0];
    c.text.push_str("A＋B×CD＋E１＋×F100");
    let old = c.text.clone();
    restore(&mut u, &w, &b);
    assert_eq!(first(&u).table.as_ref().unwrap().cells[0].text, old);
}
#[test]
fn same_source_in_two_existing_cells_is_not_reassigned() {
    let (mut u, w, b) = fixture();
    let Unit::Plain(l) = &mut u[0] else { panic!() };
    l[0].table.as_mut().unwrap().cells[1].words.push(20);
    let old = l[0].clone();
    restore(&mut u, &w, &b);
    assert_eq!(first(&u), &old);
}
#[test]
fn one_plain_line_owner_is_not_promoted_to_a_table() {
    let (mut u, w, b) = fixture();
    let Unit::Plain(l) = &mut u[0] else { panic!() };
    l[0].table = None;
    let old = l[0].clone();
    restore(&mut u, &w, &b);
    assert_eq!(first(&u), &old);
}
#[test]
fn an_unrelated_shinkyu_owner_before_the_table_keeps_owner_enumeration() {
    let (mut u, w, b) = fixture();
    u.insert(
        0,
        Unit::Shinkyu {
            preamble: vec![Line::text("独立記事".into(), vec![999])],
            after: vec![],
            before: vec![],
            outside: vec![],
        },
    );
    restore(&mut u, &w, &b);
    let Unit::Plain(l) = &u[1] else { panic!() };
    assert!(l[0].text.contains("(A＋B×C)/(１＋((D＋E)/100)×F)"));
}
#[test]
fn source_also_owned_by_shinkyu_is_ambiguous() {
    let (mut u, w, b) = fixture();
    let old = first(&u).clone();
    u.push(Unit::Shinkyu {
        preamble: vec![],
        after: vec![vec![Line::text("D＋E".into(), vec![20])]],
        before: vec![],
        outside: vec![],
    });
    restore(&mut u, &w, &b);
    assert_eq!(first(&u), &old);
}
#[test]
fn a_shinkyu_table_itself_is_out_of_scope() {
    let (u, w, b) = fixture();
    let mut it = u.into_iter();
    let Unit::Plain(l) = it.next().unwrap() else {
        panic!()
    };
    let old = l[0].clone();
    let mut units = vec![Unit::Shinkyu {
        preamble: l,
        after: vec![],
        before: vec![],
        outside: vec![],
    }];
    restore(&mut units, &w, &b);
    let Unit::Shinkyu { preamble, .. } = &units[0] else {
        panic!()
    };
    assert_eq!(preamble[0], old);
}
