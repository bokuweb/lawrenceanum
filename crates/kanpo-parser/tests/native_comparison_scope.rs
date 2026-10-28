//! A completed comparison must not relabel an independent following report.
use serde_json::Value;
fn pair() -> Value {
    let mut pages = kanpo_parser::page_inputs(include_bytes!(
        "data/native-comparison-scope/20260731g001720298.pdf"
    ))
    .unwrap();
    pages.extend(
        kanpo_parser::page_inputs(include_bytes!(
            "data/native-amount-marks/20260731g001720299.pdf"
        ))
        .unwrap(),
    );
    serde_json::from_str(&serde_json::to_string(&kanpo_parser::analyze_item(&pages)).unwrap())
        .unwrap()
}
#[test]
fn the_following_original_financial_report_has_its_own_complete_body_page() {
    let a = pair();
    let b: Value = serde_json::from_str(include_str!(
        "data/native-amount-marks/expected/20260731g001720299.json"
    ))
    .unwrap();
    assert!(a["pages"][1]==b["pages"][0],"the whole original financial page must keep its source-reviewed standalone text, roles and ownership");
}
#[test]
fn the_preceding_original_comparisons_and_their_outside_provisions_stay_exact() {
    let a = pair();
    let b: Value = serde_json::from_str(include_str!(
        "data/native-comparison-scope/baseline-pair180.json"
    ))
    .unwrap();
    assert_eq!(a["pages"][0], b["pages"][0]);
}

#[test]
fn current_printed_comparison_headers_preserve_their_complete_existing_handling() {
    let mut pages = vec![];
    for bytes in [
        include_bytes!("data/enclosed-warichu/20260813h017670002.pdf").as_slice(),
        include_bytes!("data/shared-comparison-cell/20260813h017670003.pdf").as_slice(),
    ] {
        pages.extend(kanpo_parser::page_inputs(bytes).unwrap());
    }
    let actual: Value =
        serde_json::from_str(&serde_json::to_string(&kanpo_parser::analyze_item(&pages)).unwrap())
            .unwrap();
    let expected: Value = serde_json::from_str(include_str!(
        "data/native-comparison-scope/explicit-current-control180.json"
    ))
    .unwrap();
    assert!(actual==expected,"current printed headers are their own evidence; existing plant table word boundaries and every other field must stay exact");
}

#[test]
fn the_complete_original_pair_matches_the_independent_source_ast_restoration() {
    let expected: Value = serde_json::from_str(include_str!(
        "data/native-comparison-scope/expected-pair181.json"
    ))
    .unwrap();
    assert_eq!(pair(), expected);
}
