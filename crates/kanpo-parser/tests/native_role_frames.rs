//! Printed comparison roots and original parent fences survive page rotation.
fn inputs(name: &str) -> Vec<kanpo_parser::PageInput> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let index: serde_json::Value =
        serde_json::from_str(include_str!("data/native-role-frames/index.json")).unwrap();
    let entry = index["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap();
    entry["pdfs"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|p| {
            kanpo_parser::page_inputs(&std::fs::read(root.join(p.as_str().unwrap())).unwrap())
                .unwrap()
        })
        .collect()
}
fn actual(input: &[kanpo_parser::PageInput]) -> serde_json::Value {
    serde_json::from_str(&serde_json::to_string(&kanpo_parser::analyze_item(input)).unwrap())
        .unwrap()
}
fn expected(name: &str) -> serde_json::Value {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(
        &std::fs::read_to_string(root.join(format!(
            "tests/data/native-role-frames/expected/{name}.json"
        )))
        .unwrap(),
    )
    .unwrap()
}
fn baseline(name: &str) -> serde_json::Value {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(
        &std::fs::read_to_string(root.join(format!(
            "tests/data/native-role-frames/baseline/{name}.json"
        )))
        .unwrap(),
    )
    .unwrap()
}
#[test]
fn complete_original_vertical_forms_keep_their_printed_source_roles() {
    assert!(
        actual(&inputs("172-four")) == expected("172-four"),
        "source form notes must keep the full original AST and Word material"
    );
}
#[test]
fn complete_original_landscape_tables_keep_each_side_and_all_nested_cells() {
    assert!(
        actual(&inputs("192-three")) == expected("192-three"),
        "original table cells and merges must survive the field continuation"
    );
}
#[test]
fn complete_original_alternating_page_axes_keep_one_typed_source_scope() {
    assert!(
        actual(&inputs("213-five")) == expected("213-five"),
        "rotation and image-only forms do not manufacture new field roots"
    );
}
#[test]
fn missing_intermediate_original_scene_keeps_the_entire_legacy_abstention() {
    let mut input = inputs("192-three");
    input[1].xhtml = input[1]
        .xhtml
        .replace(" data-native-scene=", " data-missing-scene=");
    assert!(
        actual(&input) == baseline("192-three"),
        "inferred current roles must not carry scope through missing source evidence"
    );
}
#[test]
fn standalone_pages_keep_the_complete_original_word_and_field_assignments() {
    let input = inputs("192-three");
    let root = actual(&input[..1]);
    assert!(root["pages"][0] == baseline("192-three")["pages"][0]);
    let a = actual(&input[2..]);
    assert!(a["pages"][0] == baseline("192-three")["pages"][2]);
}
fn reverse_words(html: &str) -> String {
    let mut spans = vec![];
    let mut cursor = 0;
    while let Some(at) = html[cursor..].find("<word ") {
        let start = cursor + at;
        let end = start + html[start..].find("</word>").unwrap() + 7;
        spans.push((start, end));
        cursor = end;
    }
    let mut out = String::new();
    cursor = 0;
    for (&(start, end), &(a, b)) in spans.iter().zip(spans.iter().rev()) {
        out.push_str(&html[cursor..start]);
        out.push_str(&html[a..b]);
        cursor = end;
    }
    out.push_str(&html[cursor..]);
    out
}
#[test]
fn source_occurrences_and_full_output_are_independent_of_word_node_order() {
    for name in ["172-four", "192-three", "213-five"] {
        let mut input = inputs(name);
        for page in &mut input {
            page.xhtml = reverse_words(&page.xhtml);
        }
        assert!(
            actual(&input) == expected(name),
            "source glyph occurrences, not XHTML node order: {name}"
        );
    }
}
