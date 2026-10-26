use serde_json::{json, Value};
fn role_material(document: &Value, side: &str) -> Vec<Value> {
    let mut out = vec![];
    for block in document["blocks"].as_array().unwrap() {
        if block["kind"] != "shinkyu" {
            continue;
        }
        for row in block["rows"].as_array().unwrap() {
            for run in row[side].as_array().unwrap() {
                let underline = run
                    .get("underline")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if let Some(table) = run.get("table") {
                    out.push(json!({"table":table,"underline":underline}));
                } else {
                    for c in run["text"].as_str().unwrap().chars().filter(|c| *c != '\n') {
                        out.push(json!([c.to_string(), underline]));
                    }
                }
            }
            if let Some(table) = row.get(format!("{side}_table").as_str()) {
                out.push(json!({"table":table,"underline":false}));
            }
        }
    }
    out
}
#[test]
fn previously_recognized_comparisons_keep_native_payload_and_reviewed_rows() {
    let cases: [(&[u8], &str, &str); 2] = [
        (
            include_bytes!("../tests/data/ruled-marker-bands/20260724g001650782.pdf"),
            include_str!("../tests/data/ruled-marker-bands/20260724g001650782-baseline99.json"),
            include_str!(
                "../tests/data/source-inserted-structure/20260724g001650782-reviewed146.json"
            ),
        ),
        (
            include_bytes!("../tests/data/ruled-marker-bands/20260729g001690094.pdf"),
            include_str!("../tests/data/ruled-marker-bands/20260729g001690094-baseline99.json"),
            include_str!(
                "../tests/data/source-inserted-structure/20260729g001690094-reviewed146.json"
            ),
        ),
    ];
    for (pdf, old, reviewed) in cases {
        let a = crate::analyze_item(&crate::page_inputs(pdf).unwrap());
        let actual: Value = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
        let original: Value = serde_json::from_str(old).unwrap();
        // Retain the old99 reference as an independent source-payload guard.
        // Only row boundaries changed: native PageTrace/text/Article ownership,
        // styles, spaces and complete opaque tables must remain identical.
        let mut before = original.clone();
        let mut now = actual.clone();
        for value in [&mut before, &mut now] {
            for article in value["articles"].as_array_mut().unwrap() {
                article.as_object_mut().unwrap().remove("document");
            }
        }
        assert_eq!(
            before, now,
            "all native and Article fields from99 remain exact"
        );
        for (old, new) in original["articles"]
            .as_array()
            .unwrap()
            .iter()
            .zip(actual["articles"].as_array().unwrap())
        {
            for side in ["after", "before"] {
                assert_eq!(
                    role_material(&old["document"], side),
                    role_material(&new["document"], side),
                    "role material except cell-edge LF"
                );
            }
        }
        let expected: Value = serde_json::from_str(reviewed).unwrap();
        assert_eq!(
            actual, expected,
            "original-verified individual insertion rows"
        );
    }
}
