use kanpo_parser::{analyze_item, page_inputs};
fn check(name: &str) {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/native-paragraph-cells");
    let inputs = page_inputs(&std::fs::read(root.join(format!("{name}.pdf"))).unwrap()).unwrap();
    let a: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&analyze_item(&inputs)).unwrap()).unwrap();
    let p = &a["pages"][0];
    let refs: serde_json::Value = serde_json::from_str(include_str!(
        "data/native-paragraph-cells/expanded-references.json"
    ))
    .unwrap();
    let rect = |v: &serde_json::Value| -> Vec<serde_json::Value> {
        ["x0", "y0", "x1", "y1"]
            .iter()
            .map(|k| v[k].clone())
            .collect()
    };
    for wanted in refs["pages"][name]["target_cells"].as_array().unwrap() {
        let line = p["regions"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|r| r["lines"].as_array().unwrap())
            .find(|l| {
                l.get("table")
                    .is_some_and(|t| serde_json::json!(rect(t)) == wanted["table_box"])
            })
            .unwrap();
        let cells: Vec<_> = line["table"]["cells"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| {
                serde_json::json!(rect(c)) == wanted["source_box"] && c["col"] == wanted["col"]
            })
            .collect();
        assert_eq!(
            cells.len(),
            1,
            "Finish the original physical cell before visiting a parallel item"
        );
        assert_eq!(cells[0]["words"], wanted["words"]);
        assert!(p["text"]
            .as_str()
            .unwrap()
            .contains(line["text"].as_str().unwrap()));
        assert!(a["text"]
            .as_str()
            .unwrap()
            .contains(line["text"].as_str().unwrap()));
        assert!(a["articles"].as_array().unwrap().iter().any(|a| a["text"]
            .as_str()
            .unwrap()
            .contains(line["text"].as_str().unwrap())));
    }
}
#[test]
fn original_20261009g002230077_keeps_complete_source_fields() {
    check("20261009g002230077");
}
#[test]
fn original_20261009g002230084_keeps_complete_source_fields() {
    check("20261009g002230084");
}
#[test]
fn original_20261009g002230085_keeps_complete_source_fields() {
    check("20261009g002230085");
}
#[test]
fn original_20261009g002230087_keeps_complete_source_fields() {
    check("20261009g002230087");
}
#[test]
fn original_20261009g002230088_keeps_complete_source_fields() {
    check("20261009g002230088");
}
#[test]
fn original_20261009g002230090_keeps_complete_source_fields() {
    check("20261009g002230090");
}
#[test]
fn original_20261009g002230091_keeps_complete_source_fields() {
    check("20261009g002230091");
}
