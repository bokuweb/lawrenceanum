use kanpo_parser::{analyze_item, page_inputs};
fn check(context: bool, anchor: u64) {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/native-paragraph-cells");
    let mut inputs = Vec::new();
    if context {
        inputs.extend(
            page_inputs(&std::fs::read(root.join("20261016g002280624.pdf")).unwrap()).unwrap(),
        );
    }
    inputs
        .extend(page_inputs(&std::fs::read(root.join("20261016g002280625.pdf")).unwrap()).unwrap());
    let a = analyze_item(&inputs);
    let a: serde_json::Value = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    let p = a["pages"].as_array().unwrap().last().unwrap();
    let refs: serde_json::Value =
        serde_json::from_str(include_str!("data/native-paragraph-cells/references.json")).unwrap();
    let reference = refs["expected"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["anchor"].as_u64() == Some(anchor))
        .unwrap();
    let t = p["regions"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| r["lines"].as_array().unwrap())
        .filter_map(|l| l.get("table"))
        .find(|t| {
            t["cells"].as_array().unwrap().iter().any(|c| {
                c["words"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|i| i.as_u64() == Some(anchor))
            })
        })
        .unwrap();
    assert_eq!(
        t["rows"], reference["rows"],
        "The wrapped source label must not create a record boundary"
    );
    assert_eq!(t["cols"], reference["cols"]);
    assert_eq!(t["cells"].as_array().unwrap().len(), 14);
    for wanted in reference["cells"].as_array().unwrap() {
        let c: Vec<_> = t["cells"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["row"] == wanted["row"] && c["col"] == wanted["col"])
            .collect();
        assert_eq!(c.len(), 1);
        assert_eq!(c[0]["words"], wanted["words"]);
    }
}
#[test]
fn standalone_first_complete_label_precedes_its_summary_and_reason() {
    check(false, 26);
}
#[test]
fn standalone_second_complete_label_precedes_its_summary_and_reason() {
    check(false, 33);
}
#[test]
fn context_first_complete_label_precedes_its_summary_and_reason() {
    check(true, 26);
}
#[test]
fn context_second_complete_label_precedes_its_summary_and_reason() {
    check(true, 33);
}
