use kanpo_parser::{analyze_item, page_inputs};
fn build(context: bool) -> serde_json::Value {
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
    a
}

fn nested_field(v: &serde_json::Value, discriminator: &str, label: &str) -> bool {
    match v {
        serde_json::Value::Object(m) => {
            if let Some(rows) = m.get("rows").and_then(|v| v.as_array()) {
                let ts: Vec<_> = rows
                    .iter()
                    .filter_map(|r| r.as_array())
                    .flatten()
                    .filter_map(|c| c.as_array())
                    .flatten()
                    .filter_map(|run| run.get("text").and_then(|v| v.as_str()))
                    .collect();
                if ts.contains(&discriminator) && ts.contains(&label) {
                    return true;
                }
            }
            m.values().any(|v| nested_field(v, discriminator, label))
        }
        serde_json::Value::Array(v) => v.iter().any(|v| nested_field(v, discriminator, label)),
        _ => false,
    }
}
fn output(context: bool, anchor: u64) {
    let a = build(context);
    let p = a["pages"].as_array().unwrap().last().unwrap();
    let refs: serde_json::Value =
        serde_json::from_str(include_str!("data/native-paragraph-cells/references.json")).unwrap();
    let reference = refs["expected"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["anchor"].as_u64() == Some(anchor))
        .unwrap();
    let source_text = |row: u64, col: u64| -> String {
        let c = reference["cells"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["row"].as_u64() == Some(row) && c["col"].as_u64() == Some(col))
            .unwrap();
        c["words"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| {
                p["words"][id.as_u64().unwrap() as usize]["text"]
                    .as_str()
                    .unwrap()
            })
            .collect()
    };
    let label = source_text(5, 0);
    let discriminator = source_text(0, 1);
    let l = p["regions"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| r["lines"].as_array().unwrap())
        .find(|l| {
            l.get("table").is_some_and(|t| {
                t["cells"].as_array().unwrap().iter().any(|c| {
                    c["words"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|i| i.as_u64() == Some(anchor))
                })
            })
        })
        .unwrap();
    assert!(
        l["text"].as_str().unwrap().contains(&label),
        "Finish the whole source label before reading its description"
    );
    for text in [p["text"].as_str().unwrap(), a["text"].as_str().unwrap()] {
        assert!(
            text.contains(l["text"].as_str().unwrap()),
            "Public text must use the same restored table"
        );
    }
    assert!(a["articles"].as_array().unwrap().iter().any(|a| a["text"]
        .as_str()
        .unwrap()
        .contains(l["text"].as_str().unwrap())));
    assert!(
        nested_field(&a["articles"], &discriminator, &label),
        "The corresponding nested document table must retain the complete label in one cell"
    );
}
#[test]
fn standalone_first_label_is_complete_in_all_public_outputs() {
    output(false, 26);
}
#[test]
fn standalone_second_label_is_complete_in_all_public_outputs() {
    output(false, 33);
}
#[test]
fn context_first_label_is_complete_in_all_public_outputs() {
    output(true, 26);
}
#[test]
fn context_second_label_is_complete_in_all_public_outputs() {
    output(true, 33);
}
