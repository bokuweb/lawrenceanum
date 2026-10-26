use crate::parallel_fractions;
use crate::trace::{Line, Table, TableCell, Unit, Word, WordStatus};
#[derive(Debug, PartialEq)]
enum Signature {
    Plain(Vec<Line>),
    Shinkyu {
        preamble: Vec<Line>,
        after: Vec<Vec<Line>>,
        before: Vec<Vec<Line>>,
        outside: Vec<Vec<Line>>,
    },
}
fn signature(units: &[Unit]) -> Vec<Signature> {
    units
        .iter()
        .map(|u| match u {
            Unit::Plain(ls) => Signature::Plain(ls.clone()),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            ..} => Signature::Shinkyu {
                preamble: preamble.clone(),
                after: after.clone(),
                before: before.clone(),
                outside: outside.clone(),
            },
        })
        .collect()
}
const EXPECTED: &str = "各発行対象国債の額面金額×(各発行対象国債の利率/100)×(１/２)";
fn word(id: usize, text: &str, b: [f32; 4]) -> Word {
    Word {
        source_box: [b[0], b[1], b[2], b[3]],
        id,
        raw: text.into(),
        text: text.into(),
        x0: b[0],
        y0: b[1],
        x1: b[2],
        y1: b[3],
        status: WordStatus::Used,
    }
}
fn words() -> Vec<Word> {
    vec![
        word(
            0,
            "各発行対象国債の利率",
            [631.008, 272.3357, 711.008, 280.5197],
        ),
        word(1, "１", [723., 272.3637, 731., 280.5477]),
        word(
            2,
            "各発行対象国債の額面金額×",
            [525.008, 277.8317, 629.008, 286.0157],
        ),
        word(3, "×", [713., 277.8597, 721., 286.0437]),
        word(4, "100", [665.008, 283.3357, 677.008, 291.5197]),
        word(5, "２", [723., 283.3637, 731., 291.5477]),
        word(6, "別の科目42", [790., 272., 830., 280.]),
    ]
}
fn source() -> String {
    "各発行対象国債の利率　１\n各発行対象国債の額面金額×　×\n100　２".into()
}
fn table() -> (Vec<Unit>, Vec<Word>) {
    let cells = vec![
        TableCell {
            row: 0,
            col: 0,
            rowspan: 1,
            colspan: 1,
            x0: 500.,
            y0: 260.,
            x1: 780.,
            y1: 300.,
            text: source(),
            words: vec![0, 1, 2, 3, 4, 5],
        },
        TableCell {
            row: 0,
            col: 1,
            rowspan: 1,
            colspan: 1,
            x0: 780.,
            y0: 260.,
            x1: 840.,
            y1: 300.,
            text: "別の科目42".into(),
            words: vec![6],
        },
    ];
    let t = Table {
        x0: 500.,
        y0: 260.,
        x1: 840.,
        y1: 300.,
        rows: 1,
        cols: 2,
        cells,
    };
    (
        vec![Unit::Plain(vec![Line {
            text: t.render(),
            words: (0..7).collect(),
            table: Some(t),
        }])],
        words(),
    )
}
fn lines(u: &mut [Unit]) -> &mut Vec<Line> {
    let Unit::Plain(ls) = &mut u[0] else { panic!() };
    ls
}
fn cell(u: &mut [Unit]) -> &mut TableCell {
    &mut lines(u)[0].table.as_mut().unwrap().cells[0]
}
fn run(u: &mut [Unit], w: &[Word]) {
    parallel_fractions::restore(u, w, &[(281., 629., 713.)], &[(281., 721., 733.)]);
}
fn unchanged(mut u: Vec<Unit>, w: Vec<Word>) {
    let before = signature(&u);
    run(&mut u, &w);
    assert_eq!(signature(&u), before);
}

#[test]
fn adapter_table_retains_owner_ids_geometry_other_cell_and_idempotence() {
    let (mut u, w) = table();
    let old = lines(&mut u)[0].clone();
    let before_words = format!("{w:?}");
    run(&mut u, &w);
    let new = &lines(&mut u)[0];
    let nt = new.table.as_ref().unwrap();
    let ot = old.table.as_ref().unwrap();
    assert_eq!(nt.cells[0].text, EXPECTED);
    assert_eq!(new.words, old.words);
    let mut same = nt.clone();
    same.cells[0].text = ot.cells[0].text.clone();
    assert_eq!(&same, ot);
    assert_eq!(nt.cells[1], ot.cells[1]);
    assert_eq!(new.text, nt.render());
    assert_eq!(format!("{w:?}"), before_words);
    let once = signature(&u);
    run(&mut u, &w);
    assert_eq!(signature(&u), once);
}
#[test]
fn adapter_plain_retains_concatenated_ids_and_following_line() {
    let mut u = vec![Unit::Plain(vec![
        Line::text("各発行対象国債の利率　１".into(), vec![0, 1]),
        Line::text("各発行対象国債の額面金額×　×".into(), vec![2, 3]),
        Line::text("100　２".into(), vec![4, 5]),
        Line::text("別の科目42".into(), vec![6]),
    ])];
    let w = words();
    let following = lines(&mut u)[3].clone();
    run(&mut u, &w);
    assert_eq!(lines(&mut u).len(), 2);
    assert_eq!(
        lines(&mut u)[0],
        Line::text(EXPECTED.into(), vec![0, 1, 2, 3, 4, 5])
    );
    assert_eq!(lines(&mut u)[1], following);
    let once = signature(&u);
    run(&mut u, &w);
    assert_eq!(signature(&u), once);
}
#[test]
fn adapter_missing_short_real_bar_is_noop() {
    let (mut u, w) = table();
    let b = signature(&u);
    parallel_fractions::restore(&mut u, &w, &[(281., 629., 713.)], &[]);
    assert_eq!(signature(&u), b);
}
#[test]
fn adapter_missing_long_real_bar_is_noop() {
    let (mut u, w) = table();
    let b = signature(&u);
    parallel_fractions::restore(&mut u, &w, &[], &[(281., 721., 733.)]);
    assert_eq!(signature(&u), b);
}
#[test]
fn adapter_unknown_raw_in_operand_is_noop() {
    let (u, mut w) = table();
    w[0].raw.push('\u{e0aa}');
    unchanged(u, w);
}
#[test]
fn adapter_empty_unknown_inside_formula_is_noop() {
    let (u, mut w) = table();
    let mut extra = word(7, "\u{e0aa}", [650., 278., 658., 286.]);
    extra.text.clear();
    extra.status = WordStatus::Empty;
    w.push(extra);
    unchanged(u, w);
}
#[test]
fn adapter_global_duplicate_owner_is_noop() {
    let (mut u, w) = table();
    u.push(Unit::Plain(vec![Line::text("duplicate".into(), vec![0])]));
    unchanged(u, w);
}
#[test]
fn adapter_cell_without_exact_source_substring_is_noop() {
    let (mut u, w) = table();
    cell(&mut u).text = "原字との対応がないセル".into();
    unchanged(u, w);
}
#[test]
fn adapter_repeated_source_substring_is_ambiguous() {
    let (mut u, w) = table();
    cell(&mut u).text = format!("{}\n{}", source(), source());
    unchanged(u, w);
}
#[test]
fn adapter_operand_in_different_cell_is_noop() {
    let (mut u, w) = table();
    cell(&mut u).words.retain(|id| *id != 1);
    lines(&mut u)[0].table.as_mut().unwrap().cells[1]
        .words
        .push(1);
    unchanged(u, w);
}
#[test]
fn adapter_shinkyu_is_never_mutated() {
    let (mut u, w) = table();
    let ls = lines(&mut u).clone();
    let u = vec![Unit::Shinkyu {labels:None,
        preamble: vec![],
        after: vec![ls],
        before: vec![],
        outside: vec![],
    }];
    unchanged(u, w);
}
#[test]
fn adapter_duplicate_id_in_same_cell_is_noop() {
    let (mut u, w) = table();
    cell(&mut u).words.push(0);
    unchanged(u, w);
}
#[test]
fn adapter_unowned_extra_raw_inside_formula_is_noop() {
    let (u, mut w) = table();
    w.push(word(7, "余分な語", [650., 278., 658., 286.]));
    unchanged(u, w);
}
