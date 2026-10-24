use kanpo_parser::{analyze_item, page_inputs, PageInput};
use serde_json::{json, Value};
fn original() -> Vec<PageInput> {
    page_inputs(include_bytes!(
        "data/native-notice-relations/20260828g001920076.pdf"
    ))
    .unwrap()
}
fn value(input: &[PageInput]) -> Value {
    serde_json::from_str(&serde_json::to_string(&analyze_item(input)).unwrap()).unwrap()
}
fn notices(a: &Value) -> Vec<Value> {
    a["articles"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|a| {
            a.get("notices")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .cloned()
        })
        .collect()
}
fn expected() -> Value {
    serde_json::from_str(include_str!(
        "data/native-notice-relations/expected-notices.json"
    ))
    .unwrap()
}
fn baseline() -> Value {
    serde_json::from_str(include_str!(
        "data/native-notice-relations/baseline123.json"
    ))
    .unwrap()
}
fn without_notices(mut a: Value) -> Value {
    for article in a["articles"].as_array_mut().unwrap() {
        article.as_object_mut().unwrap().remove("notices");
    }
    a
}
fn table_count(a: &Value) -> usize {
    a["pages"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|p| p["regions"].as_array().unwrap())
        .flat_map(|r| r["lines"].as_array().unwrap())
        .filter(|l| l.get("table").is_some())
        .count()
}
fn native_words(input: &[PageInput]) -> Vec<String> {
    let mut out = vec![];
    let mut rest = input[0].xhtml.as_str();
    while let Some(at) = rest.find("<word ") {
        rest = &rest[at..];
        let end = rest.find("</word>").unwrap() + "</word>".len();
        out.push(rest[..end].to_owned());
        rest = &rest[end..];
    }
    assert!(!out.is_empty());
    out
}
fn only_words(input: &mut [PageInput], words: &[String]) {
    input[0].xhtml = format!(
        "<html><body><doc><page width=\"595\" height=\"842\">{}</page></doc></body></html>",
        words.join("\n")
    );
}
#[test]
fn original_signed_procedures_share_references_without_copying_financial_content() {
    let a = value(&original());
    assert_eq!(json!(notices(&a)), expected());
    assert_eq!(table_count(&a), 5);
    assert_eq!(
        without_notices(a),
        baseline(),
        "The complete adopted Analysis remains exact after removing only the additive source graph"
    );
}
#[test]
fn original_external_gazette_citation_does_not_create_a_sixth_local_statement() {
    let a = value(&original());
    let ns = notices(&a);
    assert_eq!(ns.len(), 3);
    assert_eq!(ns[2]["financial_statements"].as_array().unwrap().len(), 1);
    assert_eq!(
        ns[1]["financial_statements"][1],
        ns[2]["financial_statements"][0]
    );
    assert_eq!(table_count(&a), 5);
}
#[test]
fn native_word_emission_order_does_not_select_the_source_graph() {
    let mut input = original();
    let mut words = native_words(&input);
    words.reverse();
    only_words(&mut input, &words);
    let a = value(&input);
    assert_eq!(json!(notices(&a)), expected());
    assert_eq!(without_notices(a), baseline());
}
#[test]
fn presentation_flow_and_line_wrappers_do_not_define_the_source_scope() {
    let mut input = original();
    let words = native_words(&input);
    only_words(&mut input, &words);
    let a = value(&input);
    assert_eq!(json!(notices(&a)), expected());
    assert_eq!(without_notices(a), baseline());
}
#[test]
fn unknown_native_flow_keeps_statements_without_claiming_procedure_links() {
    let mut input = original();
    input[0].xhtml = input[0]
        .xhtml
        .replace("data-native-axis=\"down\"", "data-native-axis=\"unknown\"")
        .replace("data-native-axis=\"right\"", "data-native-axis=\"unknown\"");
    let a = value(&input);
    assert!(notices(&a).is_empty());
    assert_eq!(table_count(&a), 5);
    assert_eq!(a["pages"][0]["words"], baseline()["pages"][0]["words"]);
}
#[test]
fn open_source_frames_do_not_authorize_a_notice_graph() {
    let mut input = original();
    input[0].rules.as_mut().unwrap().enclosures.clear();
    let a = value(&input);
    assert!(notices(&a).is_empty());
    assert_eq!(table_count(&a), 5);
    assert_eq!(a["pages"][0]["words"], baseline()["pages"][0]["words"]);
}
fn prior(text: &str) -> PageInput {
    PageInput{xhtml:format!("<html><body><doc><page width=\"595\" height=\"842\"><word xMin=\"100\" yMin=\"100\" xMax=\"200\" yMax=\"110\">{text}</word></page></doc></body></html>"),rules:None}
}
#[test]
fn page_offsets_remain_original_source_indices_in_a_multi_page_item() {
    let mut input = vec![prior("先行する独立した本文")];
    input.extend(original());
    let a = value(&input);
    let mut e = expected();
    for n in e.as_array_mut().unwrap() {
        for span in n["source"].as_array_mut().unwrap() {
            span["page"] = json!(1);
        }
        for s in n["financial_statements"].as_array_mut().unwrap() {
            for span in s["source"].as_array_mut().unwrap() {
                span["page"] = json!(1);
            }
        }
    }
    assert_eq!(json!(notices(&a)), e);
    assert_eq!(table_count(&a), 5);
}
#[test]
fn an_external_open_quote_cannot_lend_announcement_authority() {
    let mut input = vec![prior("先行する本文が引用を開始「")];
    input.extend(original());
    let a = value(&input);
    assert!(notices(&a).is_empty());
    assert_eq!(table_count(&a), 5);
}
