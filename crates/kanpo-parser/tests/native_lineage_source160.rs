use std::path::PathBuf;
fn check(key: &str) {
    let gold: serde_json::Value =
        serde_json::from_str(include_str!("expected-native-lineage160.json")).unwrap();
    let case = gold["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["key"] == key)
        .unwrap();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(case["input"].as_str().unwrap());
    let a = kanpo_parser::analyze_item(
        &kanpo_parser::page_inputs(&std::fs::read(path).unwrap()).unwrap(),
    );
    let actual: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    assert_eq!(
        actual, case["expected"],
        "complete source-constructed Analysis: {key}"
    );
}
#[test]
fn two_original_header_labels_keep_their_own_TD_and_Tstar_wraps() {
    check("20260724g001650702")
}
#[test]
fn original_wrapped_supply_label_stays_before_its_symbol() {
    check("20260818g001840006")
}
#[test]
fn original_estimated_demand_label_stays_before_its_symbol() {
    check("20260818g001840007")
}
