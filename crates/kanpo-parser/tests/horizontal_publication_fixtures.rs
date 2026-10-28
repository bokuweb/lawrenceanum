use kanpo_parser::{analyze_item, page_inputs};
use serde_json::{json, Value};

fn material(documents: &[Value]) -> Vec<Value> {
    let mut out: Vec<Value> = vec![];
    for d in documents {
        for block in d["blocks"].as_array().unwrap() {
            assert_eq!(block["kind"], "paragraph");
            for run in block["runs"].as_array().unwrap() {
                if let Some(table) = run.get("table") {
                    out.push(json!({"table":table}));
                } else {
                    let text = run["text"].as_str().unwrap();
                    let mut style = run.as_object().unwrap().clone();
                    style.remove("text");
                    if let Some(last) = out
                        .last_mut()
                        .filter(|v| v.get("text").is_some() && v["style"] == json!(style))
                    {
                        let joined = last["text"].as_str().unwrap().to_owned() + text;
                        last["text"] = json!(joined);
                    } else {
                        out.push(json!({"text":text,"style":style}));
                    }
                }
            }
        }
    }
    out
}

fn tables(document: &Value) -> Vec<Value> {
    document["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|b| b["runs"].as_array().unwrap())
        .filter_map(|r| r.get("table").cloned())
        .collect()
}

fn check(name: &str, pdf: &[u8]) {
    let refs: Value =
        serde_json::from_str(include_str!("data/horizontal-publications/references.json")).unwrap();
    let expected = &refs["expected"][name];
    let a = analyze_item(&page_inputs(pdf).unwrap());
    // Serialize through the public JSON representation to retain f32 fidelity.
    let actual: Value = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    assert_eq!(
        actual["pages"][0], expected["page"],
        "Native Words, source ownership, full cells, roles and text must be unchanged"
    );
    assert_eq!(actual["text"], expected["page"]["text"]);
    let documents: Vec<_> = actual["articles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["document"].clone())
        .collect();
    assert_eq!(material(&documents), material(expected["baseline_documents"].as_array().unwrap()), "All original document bytes, whitespace, styles and complete tables must occur once in source order");
    if let Some(records) = expected.get("records").and_then(Value::as_array) {
        assert_eq!(a.articles.len(), records.len());
        let lines: Vec<_> = a.pages[0].regions.iter().flat_map(|r| &r.lines).collect();
        for (article, record) in actual["articles"].as_array().unwrap().iter().zip(records) {
            assert_eq!(article["text"], record["text"]);
            assert_eq!(
                json!(tables(&article["document"])),
                record["document_tables"],
                "A complete table belongs to its own original publication"
            );
            let start = record["line_range"][0].as_u64().unwrap() as usize;
            let end = record["line_range"][1].as_u64().unwrap() as usize;
            let ids: Vec<_> = lines[start..end]
                .iter()
                .flat_map(|l| {
                    l.table.as_ref().map_or_else(
                        || l.words.clone(),
                        |t| {
                            t.cells
                                .iter()
                                .flat_map(|c| c.words.iter().copied())
                                .collect()
                        },
                    )
                })
                .collect();
            assert_eq!(json!(ids), record["source_word_ids"]);
        }
    } else {
        let context = expected["category_and_publication_text"].as_str().unwrap();
        assert_eq!(a.articles.iter().filter(|a| a.text.contains(context)).count(), 1, "The field label stays with its first publication; it must not become a new orphan article");
    }
}

#[test]
fn original_wrapped_financial_notice_and_previous_statement_are_separate() {
    check(
        "20260722g001620053",
        include_bytes!("data/horizontal-publications/20260722g001620053.pdf"),
    );
}
#[test]
fn original_previous_statement_new_statement_and_later_numbered_notice_are_three_scopes() {
    check(
        "20260722g001620054",
        include_bytes!("data/horizontal-publications/20260722g001620054.pdf"),
    );
}
#[test]
fn original_road_amendment_and_signed_financial_publication_have_separate_scopes() {
    check(
        "20260731g001730072",
        include_bytes!("data/horizontal-publications/20260731g001730072.pdf"),
    );
}
#[test]
fn original_comparison_and_two_financial_publications_keep_their_complete_tables() {
    check(
        "20260806g001770049",
        include_bytes!("data/horizontal-publications/20260806g001770049.pdf"),
    );
}
#[test]
fn original_field_label_remains_with_the_first_financial_publication() {
    check(
        "20261016g002280002",
        include_bytes!("data/horizontal-publications/20261016g002280002.pdf"),
    );
}
