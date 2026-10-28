//! Original source caps close an earlier comparison before a later root.
fn inputs(name: &str) -> Vec<kanpo_parser::PageInput> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let index: serde_json::Value =
        serde_json::from_str(include_str!("data/native-role-prefix/index.json")).unwrap();
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
            kanpo_parser::page_inputs(
                &std::fs::read(root.join(p["path"].as_str().unwrap())).unwrap(),
            )
            .unwrap()
        })
        .collect()
}
fn actual(input: &[kanpo_parser::PageInput]) -> serde_json::Value {
    serde_json::from_str(&serde_json::to_string(&kanpo_parser::analyze_item(input)).unwrap())
        .unwrap()
}
fn stored(folder: &str, name: &str) -> serde_json::Value {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let index: serde_json::Value =
        serde_json::from_str(include_str!("data/native-role-prefix/index.json")).unwrap();
    let current = (folder == "expected")
        .then(|| {
            index["cases"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["name"] == name)
                .and_then(|c| c["current_expected_path"].as_str())
        })
        .flatten();
    let path = current.map(|p| root.join(p)).unwrap_or_else(|| {
        root.join(format!(
            "tests/data/native-role-prefix/{folder}/{name}.json"
        ))
    });
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}
#[test]
fn complete_original_radio_comparison_closes_before_the_next_notice() {
    assert!(actual(&inputs("171-three")) == stored("expected", "171-three"));
}
#[test]
fn complete_original_species_tables_keep_their_own_cells_and_enactment() {
    assert!(actual(&inputs("192-four")) == stored("expected", "192-four"));
}
#[test]
fn complete_original_index_formulas_keep_their_sides_and_outside_note() {
    assert!(actual(&inputs("192-three")) == stored("expected", "192-three"));
}
#[test]
fn complete_original_medical_table_tail_closes_before_the_next_article() {
    assert!(actual(&inputs("215-four")) == stored("expected", "215-four"));
}
#[test]
fn mixed_footer_uses_only_its_complete_source_owned_word_slices() {
    assert!(actual(&inputs("226-three")) == stored("expected", "226-three"));
}
#[test]
fn missing_current_original_scene_keeps_the_complete_original_output() {
    for name in ["171-three", "192-four", "192-three", "215-four"] {
        let mut input = inputs(name);
        let last = input.last_mut().unwrap();
        last.xhtml = last
            .xhtml
            .replace(" data-native-scene=", " data-missing-scene=");
        assert!(actual(&input) == stored("baseline", name), "{name}");
    }
}
#[test]
fn standalone_pages_cannot_borrow_a_previous_printed_scope() {
    for name in [
        "171-three",
        "192-four",
        "192-three",
        "215-four",
        "226-three",
    ] {
        let input = inputs(name);
        assert!(
            actual(&input[input.len() - 1..]) == stored("baseline", &format!("standalone-{name}")),
            "{name}"
        );
    }
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
fn complete_source_golds_survive_reversed_word_node_order() {
    for name in [
        "171-three",
        "192-four",
        "192-three",
        "215-four",
        "226-three",
    ] {
        let mut input = inputs(name);
        for page in &mut input {
            page.xhtml = reverse_words(&page.xhtml);
        }
        assert!(actual(&input) == stored("expected", name), "{name}");
    }
}

#[test]
fn a_mixed_footer_without_its_original_scene_keeps_the_entire_previous_abstention() {
    let mut input = inputs("226-three");
    let last = input.last_mut().unwrap();
    last.xhtml = last
        .xhtml
        .replace(" data-native-scene=", " data-missing-scene=");
    assert!(actual(&input) == stored("baseline", "226-three"));
}
