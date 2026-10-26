//! A source-owned opaque glyph need not erase independent physical body typography.
//! Case/court semantics and every original source/material field stay strict.
#[path = "support/native_court_article_contracts.rs"]
mod court_contract;
use serde_json::Value;
use std::path::PathBuf;
fn data(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/native-court-body-fonts")
        .join(name)
}
fn baseline(name: &str) -> Value {
    serde_json::from_slice(&std::fs::read(data(name)).unwrap()).unwrap()
}
fn analyze(keys: &[&str]) -> Value {
    let inputs: Vec<_> = keys
        .iter()
        .flat_map(|key| {
            kanpo_parser::page_inputs(&std::fs::read(data(&format!("{key}.pdf"))).unwrap()).unwrap()
        })
        .collect();
    serde_json::from_str(&serde_json::to_string(&kanpo_parser::analyze_item(&inputs)).unwrap())
        .unwrap()
}
fn compact(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            '０'..='９' => char::from_u32(c as u32 - 0xfee0).unwrap(),
            _ => c,
        })
        .collect()
}
fn independent_cases(a: &Value) {
    let owners: Vec<_> = ["令和8年（ヒ）第26号", "令和7年（ヒ）第1002号"]
        .iter()
        .map(|id| {
            let owners: Vec<_> = a["articles"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .filter_map(|(i, a)| {
                    compact(a["text"].as_str().unwrap())
                        .contains(id)
                        .then_some(i)
                })
                .collect();
            assert_eq!(owners.len(), 1, "unique owner for printed case {id}");
            owners[0]
        })
        .collect();
    assert_ne!(
        owners[0], owners[1],
        "independent printed Kyoto and Tottori proceedings"
    );
}
#[test]
fn unknown_body_character_class_keeps_two_independent_printed_cases() {
    let old = baseline("20260916h017910021-baseline150.json");
    let new = analyze(&["20260916h017910021"]);
    court_contract::preserved(&old, &new);
    independent_cases(&new);
    assert_eq!(new["articles"].as_array().unwrap().len(), 8);
    let words = new["pages"][0]["words"].as_array().unwrap();
    let opaque: Vec<_> = words
        .iter()
        .filter(|w| w["raw"].as_str().unwrap().contains('\u{e40d}'))
        .collect();
    assert_eq!(opaque.len(), 1);
    assert!(!opaque[0]["text"].as_str().unwrap().contains('\u{e40d}'));
    // The unknown original scalar remains in raw source; no name restoration.
    assert_eq!(old["pages"][0]["words"], new["pages"][0]["words"]);
}
#[test]
fn the_original_consecutive_pages_keep_all_material_and_case_owners() {
    let old = baseline("consecutive-3pages-baseline150.json");
    let new = analyze(&[
        "20260916h017910020",
        "20260916h017910021",
        "20260916h017910022",
    ]);
    court_contract::preserved(&old, &new);
    independent_cases(&new);
    assert_eq!(
        new["articles"].as_array().unwrap().len(),
        old["articles"].as_array().unwrap().len() + 1
    );
}
