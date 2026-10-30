//! Earlier source-bounded literal edits cannot absorb an independent report.
fn inputs() -> Vec<kanpo_parser::PageInput> {
    let mut inputs = vec![];
    for bytes in [
        include_bytes!("data/native-source-paragraphs/20260731g001720011.pdf").as_slice(),
        include_bytes!("data/native-comparison-scope/20260731g001720298.pdf").as_slice(),
        include_bytes!("data/native-amount-marks/20260731g001720299.pdf").as_slice(),
    ] {
        inputs.extend(kanpo_parser::page_inputs(bytes).unwrap());
    }
    inputs
}
fn actual(inputs: &[kanpo_parser::PageInput]) -> serde_json::Value {
    serde_json::from_str(&serde_json::to_string(&kanpo_parser::analyze_item(inputs)).unwrap())
        .unwrap()
}
fn expected() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "data/native-source-paragraphs/expected-three187.json"
    ))
    .unwrap()
}
#[test]
fn earlier_original_literal_edits_leave_the_report_in_its_own_complete_article() {
    let a = actual(&inputs());
    assert_eq!(a["articles"].as_array().unwrap().len(), 4);
    assert!(a==expected(),"only the independently source-reviewed report article boundary may change; preserve all pages, Words, old article bytes and complete documents");
}
#[test]
fn removing_the_original_scene_keeps_the_complete_legacy_abstention() {
    let mut inputs = inputs();
    let h = &mut inputs[0].xhtml;
    let needle = " data-native-scene=\"";
    let at = h.find(needle).unwrap();
    let start = at + needle.len();
    let end = start + h[start..].find('"').unwrap() + 1;
    h.replace_range(at..end, "");
    // baseline-three183 cut at the spaced 「資　料」 heading (native263w), which starts the
    // report from the text alone (`data/spaced-materials263w/split.py`, the report `unknown`).
    let old: serde_json::Value = serde_json::from_str(include_str!(
        "data/native-source-paragraphs/baseline-three183-materials263w.json"
    ))
    .unwrap();
    assert!(actual(&inputs)==old,"missing source paint must not be replaced with inferred paragraph geometry or a forced quote reset");
}
#[test]
fn original_glyph_ranges_keep_the_complete_result_when_word_nodes_are_reversed() {
    let mut inputs = inputs();
    let h = &inputs[0].xhtml;
    let mut spans = vec![];
    let mut cursor = 0;
    while let Some(at) = h[cursor..].find("<word ") {
        let start = cursor + at;
        let end = start + h[start..].find("</word>").unwrap() + 7;
        spans.push((start, end));
        cursor = end;
    }
    let mut out = String::new();
    cursor = 0;
    for (&(start, end), &(a, b)) in spans.iter().zip(spans.iter().rev()) {
        out.push_str(&h[cursor..start]);
        out.push_str(&h[a..b]);
        cursor = end;
    }
    out.push_str(&h[cursor..]);
    inputs[0].xhtml = out;
    assert!(
        actual(&inputs) == expected(),
        "Word node order is independent of the original owned glyph ranges and source capacity"
    );
}
#[test]
fn an_original_heading_replacement_and_its_neutral_tail_do_not_reopen_the_report_scope() {
    let mut inputs = inputs();
    let page22 = kanpo_parser::page_inputs(include_bytes!(
        "data/native-source-paragraphs/20260731g001720022.pdf"
    ))
    .unwrap()
    .remove(0);
    inputs.insert(1, page22);
    let a = actual(&inputs);
    assert_eq!(a["articles"].as_array().unwrap().len(), 4);
    let e: serde_json::Value = serde_json::from_str(include_str!(
        "data/native-source-paragraphs/expected-four187.json"
    ))
    .unwrap();
    assert!(a==e,"both earlier complete original edit paragraphs stay exact; only the independently source-constructed report article split changes");
}

#[test]
fn heading_and_literal_scope_identity_survives_reversing_both_original_pages_word_nodes() {
    let mut inputs = inputs();
    inputs.insert(
        1,
        kanpo_parser::page_inputs(include_bytes!(
            "data/native-source-paragraphs/20260731g001720022.pdf"
        ))
        .unwrap()
        .remove(0),
    );
    for input in &mut inputs[..2] {
        let h = &input.xhtml;
        let mut spans = vec![];
        let mut cursor = 0;
        while let Some(at) = h[cursor..].find("<word ") {
            let start = cursor + at;
            let end = start + h[start..].find("</word>").unwrap() + 7;
            spans.push((start, end));
            cursor = end;
        }
        let mut out = String::new();
        cursor = 0;
        for (&(start, end), &(a, b)) in spans.iter().zip(spans.iter().rev()) {
            out.push_str(&h[cursor..start]);
            out.push_str(&h[a..b]);
            cursor = end;
        }
        out.push_str(&h[cursor..]);
        input.xhtml = out;
    }
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "data/native-source-paragraphs/expected-four187.json"
    ))
    .unwrap();
    assert!(actual(&inputs)==expected,"complete native glyph occurrence identity, including joined Word primitives, cannot depend on XHTML emission order");
}
