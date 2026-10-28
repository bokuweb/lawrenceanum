//! Original native fields and item boundaries. These witnesses freeze reviewed
//! regressions, not an independent overall extraction score.
use kanpo_parser::{trace::Role, Analysis, Block, ShinkyuRow};
fn analyze(pdfs: &[&[u8]], golden: &str) -> Analysis {
    let pages = pdfs
        .iter()
        .flat_map(|p| kanpo_parser::page_inputs(p).unwrap())
        .collect::<Vec<_>>();
    let a = kanpo_parser::analyze_item(&pages);
    let actual: serde_json::Value = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    let expected: serde_json::Value = serde_json::from_str(golden).unwrap();
    assert_eq!(actual, expected, "original native field/row regression");
    a
}
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn rows(a: &Analysis) -> Vec<&ShinkyuRow> {
    a.articles
        .iter()
        .flat_map(|a| &a.document.blocks)
        .filter_map(|b| {
            if let Block::Shinkyu { rows ,..} = b {
                Some(rows)
            } else {
                None
            }
        })
        .flatten()
        .collect()
}
fn fields(a: &Analysis, page: usize) {
    let p = &a.pages[page];
    let field = |role| {
        compact(
            &p.regions
                .iter()
                .filter(|r| r.role == role)
                .flat_map(|r| r.lines.iter().map(|l| l.text.as_str()))
                .collect::<String>(),
        )
    };
    assert!(field(Role::After).starts_with("二十五株式の分割"));
    assert!(field(Role::Before).starts_with("二十一株式の分割"));
    assert_eq!(field(Role::Before).matches("〔号を加える。〕").count(), 3);
    assert!(field(Role::After).contains("三十二令第二条"));
}
const P52: &[u8] = include_bytes!("data/native-parallel-edits/20260916g002060052.pdf");
const P53: &[u8] = include_bytes!("data/native-parallel-edits/20260916g002060053.pdf");
const P55: &[u8] = include_bytes!("data/native-parallel-edits/20260916g002060055.pdf");
const P56: &[u8] = include_bytes!("data/native-parallel-edits/20260916g002060056.pdf");
const P57: &[u8] = include_bytes!("data/native-parallel-edits/20260916g002060057.pdf");
#[test]
fn original_explicit_item_edits_bind_native_fields_without_prior_page() {
    fields(
        &analyze(
            &[P56],
            include_str!("data/native-parallel-edits/isolated-reviewed135.json"),
        ),
        0,
    );
}
#[test]
fn original_continuation_keeps_preceding_and_current_fields() {
    fields(
        &analyze(
            &[P55, P56],
            include_str!("data/native-parallel-edits/preceding-reviewed135.json"),
        ),
        1,
    );
}
#[test]
fn original_page_between_neighbors_keeps_its_complete_five_rows() {
    let isolated = analyze(
        &[P56],
        include_str!("data/native-parallel-edits/isolated-reviewed135.json"),
    );
    let a = analyze(
        &[P55, P56, P57],
        include_str!("data/native-parallel-edits/both-neighbors-reviewed135.json"),
    );
    fields(&a, 1);
    assert_eq!(
        serde_json::to_value(&isolated.pages[0].words).unwrap(),
        serde_json::to_value(&a.pages[1].words).unwrap()
    );
    let expected = rows(&isolated);
    let actual = rows(&a);
    assert_eq!(expected.len(), 5);
    for row in expected {
        assert_eq!(
            actual
                .iter()
                .filter(|r| serde_json::to_value(r).unwrap() == serde_json::to_value(row).unwrap())
                .count(),
            1,
            "recovered row must not be absorbed by its neighbor"
        );
    }
}
#[test]
fn original_other_item_declarations_stay_separate_across_pages() {
    let a = analyze(
        &[P52, P53],
        include_str!("data/native-parallel-edits/other-numbered-boundary-reviewed135.json"),
    );
    let rows = rows(&a);
    let head = |r: &ShinkyuRow| r.after.iter().map(|r| r.text.as_str()).collect::<String>();
    let seven = rows
        .iter()
        .find(|r| compact(&head(r)) == "七〔略〕")
        .unwrap();
    assert_eq!(
        compact(
            &seven
                .before
                .iter()
                .map(|r| r.text.as_str())
                .collect::<String>()
        ),
        "五〔同上〕"
    );
    assert!(rows
        .iter()
        .any(|r| compact(&head(r)).starts_with("八法第二十六条第二項第五号")));
}
#[test]
fn ordinary_complete_two_bands_do_not_acquire_edit_roles() {
    let a = analyze(
        &[include_bytes!(
            "data/native-parallel-edits/20260724g001640062.pdf"
        )],
        include_str!("data/native-parallel-edits/ordinary-complete-reviewed135.json"),
    );
    assert!(a.pages[0].regions.iter().all(|r| r.role == Role::Body));
}
#[test]
fn ordinary_sparse_two_bands_do_not_acquire_edit_roles() {
    let a = analyze(
        &[include_bytes!(
            "data/native-parallel-edits/20260729g001690035.pdf"
        )],
        include_str!("data/native-parallel-edits/ordinary-sparse-reviewed135.json"),
    );
    assert!(a.pages[0].regions.iter().all(|r| r.role == Role::Body));
}
