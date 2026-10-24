use kanpo_parser::{analyze_item, page_inputs};
fn check(name: &str) {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/native-paragraph-cells");
    let inputs = page_inputs(&std::fs::read(root.join(format!("{name}.pdf"))).unwrap()).unwrap();
    let a: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&analyze_item(&inputs)).unwrap()).unwrap();
    let refs: serde_json::Value = serde_json::from_str(include_str!(
        "data/native-section-continuations/references.json"
    ))
    .unwrap();
    let target = &refs["expected"][name];
    let rect = |c: &serde_json::Value| {
        ["x0", "y0", "x1", "y1"]
            .iter()
            .map(|k| c[k].clone())
            .collect::<Vec<_>>()
    };
    let t = a["pages"][0]["regions"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| r["lines"].as_array().unwrap())
        .filter_map(|l| l.get("table"))
        .find(|t| serde_json::json!(rect(t)) == target["table_box"])
        .unwrap();
    assert_eq!(
        t["rows"], target["rows"],
        "All original ruled fields must finish before the next physical field"
    );
    assert_eq!(
        t["cells"].as_array().unwrap().len(),
        target["cells"].as_array().unwrap().len()
    );
    for wanted in target["cells"].as_array().unwrap() {
        let c: Vec<_> = t["cells"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["row"] == wanted["row"] && c["col"] == wanted["col"])
            .collect();
        assert_eq!(c.len(), 1);
        assert_eq!(c[0]["words"], wanted["words"]);
        assert_eq!(c[0]["colspan"], wanted["colspan"]);
        assert_eq!(serde_json::json!(rect(c[0])), wanted["source_box"]);
    }
    let line = a["pages"][0]["regions"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| r["lines"].as_array().unwrap())
        .find(|l| l.get("table") == Some(t))
        .unwrap();
    for text in [
        a["pages"][0]["text"].as_str().unwrap(),
        a["text"].as_str().unwrap(),
    ] {
        assert!(text.contains(line["text"].as_str().unwrap()));
    }
    assert!(a["articles"].as_array().unwrap().iter().any(|a| a["text"]
        .as_str()
        .unwrap()
        .contains(line["text"].as_str().unwrap())));
    let mut wanted = std::collections::BTreeMap::new();
    for c in t["cells"].as_array().unwrap() {
        let text = c["text"].as_str().unwrap();
        if !text.is_empty() {
            *wanted.entry(text.to_string()).or_insert(0usize) += 1;
        }
    }
    assert!(
        nested_table(&a["articles"], &wanted),
        "The corresponding document table must contain every complete native source cell"
    );
}
fn leaf_texts(v: &serde_json::Value, counts: &mut std::collections::BTreeMap<String, usize>) {
    match v {
        serde_json::Value::Object(m) => {
            for (k, v) in m {
                if k == "text" {
                    if let Some(s) = v.as_str() {
                        *counts.entry(s.to_string()).or_default() += 1;
                    }
                } else {
                    leaf_texts(v, counts);
                }
            }
        }
        serde_json::Value::Array(a) => {
            for v in a {
                leaf_texts(v, counts);
            }
        }
        _ => {}
    }
}
fn nested_table(v: &serde_json::Value, wanted: &std::collections::BTreeMap<String, usize>) -> bool {
    match v {
        serde_json::Value::Object(m) => {
            if m.get("rows")
                .and_then(|v| v.as_array())
                .is_some_and(|a| !a.is_empty() && a.iter().all(|v| v.is_array()))
            {
                let mut counts = std::collections::BTreeMap::new();
                leaf_texts(v, &mut counts);
                if wanted
                    .iter()
                    .all(|(k, n)| counts.get(k).copied().unwrap_or(0) >= *n)
                {
                    return true;
                }
            }
            m.values().any(|v| nested_table(v, wanted))
        }
        serde_json::Value::Array(a) => a.iter().any(|v| nested_table(v, wanted)),
        _ => false,
    }
}

#[test]
fn all_four_original_location_fields_are_complete() {
    check("20261009g002230077");
}
#[test]
fn a_separately_wrapped_parenthetical_value_stays_inside_its_location_section() {
    check("20261009g002230091");
}

#[test]
fn another_original_location_pair_keeps_one_complete_area_description() {
    check("20261009g002230084");
}
#[test]
fn a_second_source_location_pair_preserves_its_complete_ruled_row() {
    check("20261009g002230088");
}
