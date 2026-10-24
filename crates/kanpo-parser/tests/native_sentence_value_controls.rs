use kanpo_parser::{analyze_item, page_inputs};
fn analyze(name: &str) -> serde_json::Value {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/native-sentence-cells")
        .join(format!("{name}.pdf"));
    let inputs = page_inputs(&std::fs::read(p).unwrap()).unwrap();
    serde_json::from_str(&serde_json::to_string(&analyze_item(&inputs)).unwrap()).unwrap()
}
fn unchanged(name: &str) {
    let a = analyze(name);
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/native-sentence-cells")
        .join(format!("{name}-baseline116.json"));
    let expected: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    assert_eq!(
        a, expected,
        "Source field/value rows must not be turned into prose by a later note's terminal"
    );
}
#[test]
fn mixed_scalar_and_text_field_values_keep_their_entire_analysis() {
    unchanged("20261013g002240055");
}
#[test]
fn parenthetical_summary_and_reason_do_not_rewrite_neighboring_field_flow() {
    unchanged("20261016g002280625");
}
#[test]
fn frequency_explanations_span_records_without_inserting_footnotes() {
    let a = analyze("20261001g002170049");
    let expected = [vec![44, 45, 46], vec![49, 50, 47], vec![51, 52, 48]];
    for words in expected {
        let c = a["pages"][0]["regions"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|r| r["lines"].as_array().unwrap())
            .filter_map(|l| l.get("table"))
            .flat_map(|t| t["cells"].as_array().unwrap())
            .find(|c| c["words"] == serde_json::json!(words))
            .expect("Complete original source sentence cell");
        assert_eq!(c["rowspan"], 2);
        assert!(a["text"]
            .as_str()
            .unwrap()
            .contains(c["text"].as_str().unwrap()));
    }
}
