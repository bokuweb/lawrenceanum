use kanpo_parser::{analyze_item, page_inputs};
fn fixture(path: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(path)
}
fn analyze(context: bool) -> serde_json::Value {
    let mut inputs = Vec::new();
    if context {
        inputs.extend(
            page_inputs(
                &std::fs::read(fixture("data/ruled-cell-flow/20260930g002150344.pdf")).unwrap(),
            )
            .unwrap(),
        );
    }
    inputs.extend(
        page_inputs(
            &std::fs::read(fixture(
                "data/source-ruling-sections/20260930g002150345.pdf",
            ))
            .unwrap(),
        )
        .unwrap(),
    );
    let a = analyze_item(&inputs);
    serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap()
}
fn check(context: bool, role: &str) {
    let a = analyze(context);
    let p = a["pages"].as_array().unwrap().last().unwrap();
    let refs: serde_json::Value =
        serde_json::from_str(include_str!("data/native-sentence-cells/references.json")).unwrap();
    let wanted = &refs["expected"][role];
    let anchor = wanted["anchor"].as_u64().unwrap();
    let tables: Vec<_> = p["regions"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| r["lines"].as_array().unwrap())
        .filter_map(|l| l.get("table"))
        .filter(|t| {
            t["cells"].as_array().unwrap().iter().any(|c| {
                c["words"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v.as_u64() == Some(anchor))
            })
        })
        .collect();
    assert_eq!(tables.len(), 1);
    let t = tables[0];
    assert_eq!(t["rows"],wanted["rows"],"Neighboring item lines cannot create a new record boundary through a wrapped source explanation");
    assert_eq!(t["cols"], wanted["cols"]);
    assert_eq!(t["cells"].as_array().unwrap().len(), 6);
    for e in wanted["cells"].as_array().unwrap() {
        let got: Vec<_> = t["cells"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["col"] == e["col"] && c["row"] == e["physical_row"])
            .collect();
        assert_eq!(got.len(), 1);
        assert_eq!(
            got[0]["words"], e["words"],
            "Finish each complete original ruled cell before moving to the next cell"
        );
    }
}
#[test]
fn standalone_left_explanation_stays_inside_one_original_source_cell() {
    check(false, "after");
}
#[test]
fn standalone_right_explanation_stays_inside_one_original_source_cell() {
    check(false, "before");
}
#[test]
fn prior_page_context_retains_the_complete_left_explanation_cell() {
    check(true, "after");
}
#[test]
fn prior_page_context_retains_the_complete_right_explanation_cell() {
    check(true, "before");
}
#[test]
fn complete_financial_records_are_not_collapsed_into_source_columns() {
    let inputs = page_inputs(
        &std::fs::read(fixture("data/ruled-cell-flow/20260716g001590046.pdf")).unwrap(),
    )
    .unwrap();
    let a = analyze_item(&inputs);
    let got: serde_json::Value = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    let expected: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(fixture(
            "data/ruled-cell-flow/20260716g001590046-baseline114.json",
        ))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(got, expected);
}

fn check_output(context: bool, role: &str) {
    let a = analyze(context);
    let p = a["pages"].as_array().unwrap().last().unwrap();
    let refs: serde_json::Value =
        serde_json::from_str(include_str!("data/native-sentence-cells/references.json")).unwrap();
    let anchor = refs["expected"][role]["anchor"].as_u64().unwrap();
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
    let t = l["table"].clone();
    let sentence = t["cells"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| {
            c["words"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i.as_u64() == Some(anchor))
        })
        .unwrap()["text"]
        .as_str()
        .unwrap()
        .to_string();
    for (name, text) in [
        ("page", p["text"].as_str().unwrap()),
        ("analysis", a["text"].as_str().unwrap()),
    ] {
        assert!(
            text.contains(l["text"].as_str().unwrap()),
            "{name} must reflect the complete rendered source table"
        );
        assert!(
            text.contains(&sentence.replace('\n', "　")),
            "{name} must contain the continuous source sentence"
        );
    }
}
#[test]
fn standalone_left_public_output_uses_repaired_cells() {
    check_output(false, "after");
}
#[test]
fn standalone_right_public_output_uses_repaired_cells() {
    check_output(false, "before");
}
#[test]
fn context_left_public_output_uses_repaired_cells() {
    check_output(true, "after");
}
#[test]
fn context_right_public_output_uses_repaired_cells() {
    check_output(true, "before");
}

fn nested_has(v: &serde_json::Value, wanted: &str) -> bool {
    match v {
        serde_json::Value::Object(m) => {
            if let Some(t) = m.get("table") {
                if let Some(rows) = t.get("rows").and_then(|v| v.as_array()) {
                    if rows.len() == 2 && rows[1][2][0]["text"].as_str() == Some(wanted) {
                        return true;
                    }
                }
            }
            m.values().any(|v| nested_has(v, wanted))
        }
        serde_json::Value::Array(v) => v.iter().any(|v| nested_has(v, wanted)),
        _ => false,
    }
}
fn check_document(context: bool, role: &str) {
    let a = analyze(context);
    let p = a["pages"].as_array().unwrap().last().unwrap();
    let refs: serde_json::Value =
        serde_json::from_str(include_str!("data/native-sentence-cells/references.json")).unwrap();
    let anchor = refs["expected"][role]["anchor"].as_u64().unwrap();
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
    let c = l["table"]["cells"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| {
            c["words"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i.as_u64() == Some(anchor))
        })
        .unwrap();
    assert!(
        a["articles"].as_array().unwrap().iter().any(|a| a["text"]
            .as_str()
            .unwrap()
            .contains(l["text"].as_str().unwrap())),
        "Article text must use the same repaired table"
    );
    assert!(
        nested_has(&a["articles"], c["text"].as_str().unwrap()),
        "Nested document table must retain the complete source cell in its true body row"
    );
}
#[test]
fn standalone_left_article_and_nested_table_use_repaired_cells() {
    check_document(false, "after");
}
#[test]
fn standalone_right_article_and_nested_table_use_repaired_cells() {
    check_document(false, "before");
}
#[test]
fn context_left_article_and_nested_table_use_repaired_cells() {
    check_document(true, "after");
}
#[test]
fn context_right_article_and_nested_table_use_repaired_cells() {
    check_document(true, "before");
}
