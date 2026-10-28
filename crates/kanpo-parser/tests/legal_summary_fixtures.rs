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

fn pdf(name: &str) -> &'static [u8] {
    match name {
        "20260717g001600001" => include_bytes!("data/legal-summaries/20260717g001600001.pdf"),
        "20260723g001630002" => include_bytes!("data/legal-summaries/20260723g001630002.pdf"),
        "20260723g001630003" => include_bytes!("data/legal-summaries/20260723g001630003.pdf"),
        "20260723g001630004" => include_bytes!("data/legal-summaries/20260723g001630004.pdf"),
        "20260723g001630005" => include_bytes!("data/legal-summaries/20260723g001630005.pdf"),
        "20260724g001640004" => include_bytes!("data/legal-summaries/20260724g001640004.pdf"),
        "20260729g001690001" => include_bytes!("data/legal-summaries/20260729g001690001.pdf"),
        "20260807g001780004" => include_bytes!("data/legal-summaries/20260807g001780004.pdf"),
        "20260916g002060003" => include_bytes!("data/legal-summaries/20260916g002060003.pdf"),
        _ => panic!("unknown original fixture"),
    }
}
fn check(name: &str) {
    let refs: Value =
        serde_json::from_str(include_str!("data/legal-summaries/references.json")).unwrap();
    let r = &refs["expected"][name];
    let inputs: Vec<_> = r["names"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|n| page_inputs(pdf(n.as_str().unwrap())).unwrap())
        .collect();
    let a = analyze_item(&inputs);
    let v: Value = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    assert_eq!(
        v["pages"], r["baseline_pages"],
        "All original Word occurrences, source geometry, roles, cells and page text remain exact"
    );
    let docs: Vec<_> = v["articles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["document"].clone())
        .collect();
    assert_eq!(
        material(&docs),
        material(r["baseline_documents"].as_array().unwrap()),
        "All original document bytes, whitespace, styles and whole Tables remain once and in order"
    );
    let expected = r["expected_records"].as_array().unwrap();
    assert_eq!(a.articles.len(),expected.len(),"Independently printed numbered laws/orders own separate outlines; shared section headings and next-page continuations remain with their own law");
    for (article, text) in a.articles.iter().zip(expected) {
        assert_eq!(article.text, text.as_str().unwrap());
    }
}
#[test]
fn original_independent_law_summary_has_its_own_title_and_outline() {
    check("4");
}
#[test]
fn original_previous_page_outline_continues_its_law() {
    check("3");
}
#[test]
fn original_next_page_subordinate_numbering_continues_its_law() {
    check("5");
}
#[test]
fn original_new_law_owns_the_next_page_continuation() {
    check("4-5");
}
#[test]
fn original_three_pages_keep_two_independent_laws() {
    check("3-4-5");
}
#[test]
fn original_shared_field_heading_stays_with_first_of_two_laws() {
    check("20260723g001630002");
}
#[test]
fn original_prior_outline_and_two_new_laws_have_three_owners() {
    check("20260724g001640004");
}
#[test]
fn original_eight_orders_and_prior_continuation_have_nine_owners() {
    check("20260916g002060003");
}
#[test]
fn original_rotated_summary_stays_separate_from_its_toc() {
    check("20260717g001600001");
}
#[test]
fn original_rotated_three_orders_keep_the_shared_section_title() {
    check("20260729g001690001");
}
#[test]
fn original_complete_title_owns_its_next_source_column_body() {
    check("20260807g001780004");
}
