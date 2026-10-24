use kanpo_parser::{analyze_item, page_inputs};
fn check(context: bool) {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/native-paragraph-cells");
    let mut inputs = Vec::new();
    let base = root.clone();
    if context {
        inputs.extend(
            page_inputs(&std::fs::read(base.join("20261009g002230085.pdf")).unwrap()).unwrap(),
        );
    }
    inputs
        .extend(page_inputs(&std::fs::read(base.join("20261009g002230086.pdf")).unwrap()).unwrap());
    let a = analyze_item(&inputs);
    let a: serde_json::Value = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    let p = a["pages"].as_array().unwrap().last().unwrap();
    let reference: serde_json::Value = serde_json::from_str(include_str!(
        "data/native-paragraph-cells/location-references.json"
    ))
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
                    .any(|i| i.as_u64() == Some(10))
            })
        })
        .unwrap();
    assert_eq!(t["rows"],reference["rows"],"Both enumerated location fields must preserve their complete shared description inside their original ruled row");
    assert_eq!(t["cols"], reference["cols"]);
    assert_eq!(t["cells"].as_array().unwrap().len(), 12);
    for wanted in reference["cells"].as_array().unwrap() {
        let c: Vec<_> = t["cells"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["row"] == wanted["row"] && c["col"] == wanted["col"])
            .collect();
        assert_eq!(c.len(), 1);
        assert_eq!(c[0]["colspan"], wanted["colspan"]);
        assert_eq!(c[0]["words"], wanted["words"]);
    }
}
#[test]
fn standalone_both_locations_keep_whole_area_descriptions() {
    check(false);
}
#[test]
fn context_both_locations_keep_whole_area_descriptions() {
    check(true);
}
