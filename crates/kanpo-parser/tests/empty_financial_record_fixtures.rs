use kanpo_parser::{analyze_item, page_inputs};
fn analysis() -> kanpo_parser::Analysis {
    analyze_item(
        &page_inputs(include_bytes!(
            "archive/pages/20260831g00194/20260831g001940049.pdf"
        ))
        .unwrap(),
    )
}
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
#[test]
fn independent_empty_amount_records_have_separate_rows() {
    let a = analysis();
    assert_eq!(
        a.text,
        include_str!("archive/expected-empty-financial-records/20260831g001940049.txt")
    );
    let tables: Vec<_> = a
        .pages
        .iter()
        .flat_map(|p| &p.regions)
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .filter(|t| t.cols == 13)
        .collect();
    assert_eq!(tables.len(), 1);
    let t = tables[0];
    let first = t
        .cells
        .iter()
        .find(|c| compact(&c.text) == "出資金の受入")
        .expect("first empty record must own one cell");
    let second = t
        .cells
        .iter()
        .find(|c| compact(&c.text) == "不要財産に係る国庫納付等による減資")
        .expect("second empty record must own one cell");
    assert_eq!(second.row, first.row + 1);
    assert_eq!(first.words, vec![296]);
    assert_eq!(second.words, vec![297, 298]);
    assert!(a.text.lines().any(|l| compact(l) == "出資金の受入"));
    assert!(a
        .text
        .lines()
        .any(|l| compact(l) == "不要財産に係る国庫納付等による減資"));
}
#[test]
fn actual_wrapped_labels_keep_their_single_cell_and_source_words() {
    let a = analysis();
    let cs: Vec<_> = a
        .pages
        .iter()
        .flat_map(|p| &p.regions)
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .flat_map(|t| &t.cells)
        .collect();
    for (text, ids) in [
        ("Ⅱ資本剰余金の当期変動額", vec![306, 282, 283]),
        ("時の経過による資産除去債務の増加", vec![288, 289]),
        ("不要財産に係る国庫納付等", vec![291, 292]),
        ("その他の資本剰余金の当期変動額（純額）", vec![293, 294]),
    ] {
        assert!(
            cs.iter()
                .any(|c| compact(&c.text) == text && c.words == ids),
            "actual wrapped label split: {text}"
        );
    }
}

#[test]
fn corresponding_empty_records_20260831g001940050_are_separate() {
    let a = analyze_item(
        &page_inputs(include_bytes!(
            "archive/pages/20260831g00194/20260831g001940050.pdf"
        ))
        .unwrap(),
    );
    assert_eq!(
        a.text,
        include_str!("archive/expected-empty-financial-records/20260831g001940050.txt")
    );
    let ls: Vec<_> = a.text.lines().map(compact).collect();
    assert!(ls
        .windows(2)
        .any(|p| p[0] == "出資金の受入" && p[1] == "不要財産に係る国庫納付等による減資"));
}

#[test]
fn corresponding_empty_records_20260831g001940055_are_separate() {
    let a = analyze_item(
        &page_inputs(include_bytes!(
            "archive/pages/20260831g00194/20260831g001940055.pdf"
        ))
        .unwrap(),
    );
    assert_eq!(
        a.text,
        include_str!("archive/expected-empty-financial-records/20260831g001940055.txt")
    );
    let ls: Vec<_> = a.text.lines().map(compact).collect();
    assert!(ls
        .windows(2)
        .any(|p| p[0] == "出資金の受入" && p[1] == "不要財産に係る国庫納付等による減資"));
}

#[test]
fn adjacent_20260831g001940048_is_unchanged() {
    let a = analyze_item(
        &page_inputs(include_bytes!(
            "archive/pages/20260831g00194/20260831g001940048.pdf"
        ))
        .unwrap(),
    );
    assert_eq!(
        a.text,
        include_str!("archive/expected-empty-financial-records/20260831g001940048.txt")
    );
}

#[test]
fn adjacent_20260831g001940051_is_unchanged() {
    let a = analyze_item(
        &page_inputs(include_bytes!(
            "archive/pages/20260831g00194/20260831g001940051.pdf"
        ))
        .unwrap(),
    );
    assert_eq!(
        a.text,
        include_str!("archive/expected-empty-financial-records/20260831g001940051.txt")
    );
}
