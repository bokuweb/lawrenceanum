//! Full original PDFs with independently reviewed masthead/field ownership.
//! Expected changes copy the adopted original blocks, never re-render tables.
use kanpo_parser::{analyze_item, page_inputs, PageInput};
use serde_json::Value;
fn cases() -> [(Vec<PageInput>, Value, usize); 2] {
    [
        (
            page_inputs(include_bytes!(
                "data/section-context/20260730h017580006.pdf"
            ))
            .unwrap(),
            serde_json::from_str(include_str!(
                "data/section-context/20260730h017580006-baseline124.json"
            ))
            .unwrap(),
            4,
        ),
        (
            page_inputs(include_bytes!(
                "data/section-context/20260717g001600069.pdf"
            ))
            .unwrap(),
            serde_json::from_str(include_str!(
                "data/section-context/20260717g001600069-baseline124.json"
            ))
            .unwrap(),
            1,
        ),
    ]
}
fn value(input: &[PageInput]) -> Value {
    serde_json::from_str(&serde_json::to_string(&analyze_item(input)).unwrap()).unwrap()
}
fn joined(mut old: Value, index: usize) -> Value {
    let articles = old["articles"].as_array_mut().unwrap();
    let parent = articles.remove(index);
    let child = &mut articles[index];
    child["text"] = format!(
        "{}\n{}",
        parent["text"].as_str().unwrap(),
        child["text"].as_str().unwrap()
    )
    .into();
    let mut blocks = parent["document"]["blocks"].as_array().unwrap().clone();
    blocks.extend(child["document"]["blocks"].as_array().unwrap().clone());
    child["document"]["blocks"] = blocks.into();
    old
}
#[test]
fn original_vertical_masthead_attaches_to_its_field_and_keeps_all_other_material() {
    let (input, old, i) = cases().into_iter().next().unwrap();
    assert_eq!(value(&input), joined(old, i));
}
#[test]
fn original_horizontal_masthead_preserves_the_preceding_complete_shinkyu_article() {
    let (input, old, i) = cases().into_iter().nth(1).unwrap();
    assert_eq!(value(&input), joined(old, i));
}
fn words(input: &[PageInput]) -> Vec<String> {
    let mut rest = input[0].xhtml.as_str();
    let mut out = vec![];
    while let Some(at) = rest.find("<word ") {
        rest = &rest[at..];
        let end = rest.find("</word>").unwrap() + "</word>".len();
        out.push(rest[..end].to_owned());
        rest = &rest[end..];
    }
    assert!(!out.is_empty());
    out
}
fn wrap(input: &mut [PageInput], words: &[String]) {
    input[0].xhtml = format!(
        "<html><body><doc><page width=\"595\" height=\"842\">{}</page></doc></body></html>",
        words.join("\n")
    );
}
#[test]
fn native_word_emission_order_does_not_select_masthead_ownership() {
    for (mut input, old, i) in cases() {
        let mut words = words(&input);
        words.reverse();
        wrap(&mut input, &words);
        assert_eq!(value(&input), joined(old, i));
    }
}
#[test]
fn presentation_flow_wrappers_do_not_select_masthead_ownership() {
    for (mut input, old, i) in cases() {
        let words = words(&input);
        wrap(&mut input, &words);
        assert_eq!(value(&input), joined(old, i));
    }
}
fn remove_attr(input: &mut [PageInput], name: &str) {
    let mut words = words(input);
    let prefix = format!(" {name}=\"");
    for word in &mut words {
        if let Some(start) = word.find(&prefix) {
            let end = start + prefix.len() + word[start + prefix.len()..].find('"').unwrap() + 1;
            word.replace_range(start..end, "");
        }
    }
    wrap(input, &words);
}
fn separate(a: &Value) {
    assert!(
        a["articles"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["text"].as_str() == Some("官庁報告")),
        "Unknown evidence must leave the masthead unclaimed"
    );
}
#[test]
fn unknown_native_content_styles_do_not_lend_a_heading_hierarchy() {
    for (mut input, old, _) in cases() {
        remove_attr(&mut input, "data-native-content-styles");
        let a = value(&input);
        separate(&a);
        assert_eq!(a, old);
    }
}
#[test]
fn unknown_native_axes_do_not_lend_a_heading_hierarchy() {
    for (mut input, old, _) in cases() {
        input[0].xhtml = input[0]
            .xhtml
            .replace("data-native-axis=\"down\"", "data-native-axis=\"unknown\"")
            .replace("data-native-axis=\"right\"", "data-native-axis=\"unknown\"");
        let a = value(&input);
        separate(&a);
        assert_eq!(a, old);
    }
}
#[test]
fn compatibility_boxes_cannot_supply_native_masthead_extents() {
    for (mut input, old, _) in cases() {
        for name in [
            "data-native-xmin",
            "data-native-ymin",
            "data-native-xmax",
            "data-native-ymax",
        ] {
            remove_attr(&mut input, name);
        }
        let a = value(&input);
        separate(&a);
        assert_eq!(a, old);
    }
}
#[test]
fn unavailable_source_rules_do_not_authorize_the_relationship() {
    for (index, (mut input, _, _)) in cases().into_iter().enumerate() {
        input[0].rules = None;
        let old: Value = serde_json::from_str(
            [
                include_str!("data/section-context/20260730h017580006-no-rules-baseline124.json"),
                include_str!("data/section-context/20260717g001600069-no-rules-baseline124.json"),
            ][index],
        )
        .unwrap();
        assert_eq!(
            value(&input),
            old,
            "Retain the existing no-rule layout, including any earlier lexical joining"
        );
    }
}
