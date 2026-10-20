use super::*;
use crate::trace::{Table, TableCell};
fn fixture() -> (Vec<Word>, Vec<(f32, f32, f32)>) {
    let (a, b) = crate::numeric_fraction_geometry::tests::two();
    (
        a.into_iter()
            .map(|a| Word {
                id: a.id,
                raw: a.raw,
                text: a.text,
                x0: a.x0,
                y0: a.y0,
                x1: a.x1,
                y1: a.y1,
                status: WordStatus::Used,
            })
            .collect(),
        b.into_iter().map(|b| (b.y, b.x0, b.x1)).collect(),
    )
}
fn line() -> Line {
    Line::text(
        "前文。1.87１額面金額××100２後文。".into(),
        vec![28, 26, 24, 21, 61, 23, 27, 22],
    )
}
fn ls(u: &mut [Unit]) -> &mut Vec<Line> {
    let Unit::Plain(ls) = &mut u[0] else { panic!() };
    ls
}
#[test]
fn prose_prefix_suffix_and_ids_remain() {
    let (w, b) = fixture();
    let mut u = vec![Unit::Plain(vec![line()])];
    restore(&mut u, &w, &b, &[]);
    assert_eq!(
        ls(&mut u)[0].text,
        "前文。額面金額×(1.87/100)×(１/２)後文。"
    );
    assert_eq!(ls(&mut u)[0].words, line().words);
    let once = ls(&mut u).clone();
    restore(&mut u, &w, &b, &[]);
    assert_eq!(*ls(&mut u), once);
}
#[test]
fn repeated_old_substring_not_replaced() {
    let (w, b) = fixture();
    let mut l = line();
    l.text += &l.text.clone();
    let before = l.clone();
    let mut u = vec![Unit::Plain(vec![l])];
    restore(&mut u, &w, &b, &[]);
    assert_eq!(ls(&mut u)[0], before);
}
#[test]
fn duplicated_owner_rejects_formula() {
    let (w, b) = fixture();
    let l = line();
    let mut u = vec![
        Unit::Plain(vec![l.clone()]),
        Unit::Plain(vec![Line::text("1".into(), vec![28])]),
    ];
    restore(&mut u, &w, &b, &[]);
    assert_eq!(ls(&mut u)[0], l);
}
#[test]
fn separate_owners_not_combined() {
    let (w, b) = fixture();
    let mut l = line();
    l.words.pop();
    let before = l.clone();
    let mut u = vec![
        Unit::Plain(vec![l]),
        Unit::Plain(vec![Line::text("２".into(), vec![22])]),
    ];
    restore(&mut u, &w, &b, &[]);
    assert_eq!(ls(&mut u)[0], before);
}
#[test]
fn cell_geometry_word_ids_and_render_remain() {
    let (w, b) = fixture();
    let c = TableCell {
        row: 0,
        col: 0,
        rowspan: 1,
        colspan: 1,
        x0: 520.,
        y0: 128.,
        x1: 610.,
        y1: 151.,
        text: line().text,
        words: line().words,
    };
    let t = Table {
        x0: 520.,
        y0: 128.,
        x1: 610.,
        y1: 151.,
        rows: 1,
        cols: 1,
        cells: vec![c.clone()],
    };
    let mut u = vec![Unit::Plain(vec![Line {
        text: t.render(),
        table: Some(t),
        words: c.words.clone(),
    }])];
    restore(&mut u, &w, &b, &[]);
    let l = &ls(&mut u)[0];
    let t = l.table.as_ref().unwrap();
    assert_eq!(l.text, t.render());
    let mut got = t.cells[0].clone();
    got.text = c.text.clone();
    assert_eq!(got, c);
}
#[test]
fn shinkyu_text_not_rewritten() {
    let (w, b) = fixture();
    let l = line();
    let mut u = vec![Unit::Shinkyu {
        preamble: vec![],
        after: vec![vec![l.clone()]],
        before: vec![],
        outside: vec![],
    }];
    restore(&mut u, &w, &b, &[]);
    let Unit::Shinkyu { after, .. } = &u[0] else {
        panic!()
    };
    assert_eq!(after[0][0], l);
}
