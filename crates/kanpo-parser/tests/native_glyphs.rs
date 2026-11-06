use kanpo_parser::media::{analyze_pdf_with_observations, Field};
fn report() -> serde_json::Value {
    let mut paint: serde_json::Value = serde_json::from_str(include_str!(
        "data/workbook/case-06.paint-observations.json"
    ))
    .unwrap();
    let glyphs: serde_json::Value = serde_json::from_str(include_str!(
        "data/workbook/case-06.native-glyph-observations.json"
    ))
    .unwrap();
    paint["native_glyph_observations"] = glyphs["observations"].clone();
    paint["native_glyph_report"] = glyphs;
    paint
}
const PDF: &[u8] = include_bytes!("data/workbook/case-06.pdf");
#[test]
fn isolated_native_readings_and_painted_numbers_compose_without_guessing_unknown_cids() {
    let result = analyze_pdf_with_observations(PDF, report()).unwrap();
    assert_eq!(result.native_glyphs.candidates.len(), 2);
    assert_eq!(result.native_glyphs.bindings.len(), 2);
    assert!(
        result.native_glyphs.unresolved.is_empty(),
        "{:?}",
        result.native_glyphs.unresolved
    );
    assert!(!result.native_glyphs.coverage_verified);
    assert!(result.native_glyphs.source_issues.is_empty());
    for binding in &result.native_glyphs.bindings {
        // Independent source review: both original glyphs are parenthesized ヲ,
        // not the unrelated Unicode character with the same numeric CID.
        assert_eq!(binding.text, "（ヲ）");
        assert_eq!(binding.status, "candidate_unverified");
        let word = &result.native.pages[binding.source.page].words[binding.source.word];
        assert_eq!(word.text, "（１８）�");
        assert_eq!(word.raw.chars().count(), 3);
    }
    let text = result
        .documents
        .iter()
        .map(|d| d.field_text(Field::Paragraph))
        .collect::<String>();
    assert!(!text.contains('�'));
    assert_eq!(text.matches("（１８）（ヲ）").count(), 2);
    assert_eq!(result.painted_text.as_ref().unwrap().bindings.len(), 8);
    assert!(result.painted_text.as_ref().unwrap().unresolved.is_empty());
    let native = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(PDF).unwrap());
    assert_eq!(
        serde_json::to_value(result.native).unwrap(),
        serde_json::to_value(native).unwrap()
    );
}
#[test]
fn wrong_occurrences_and_known_number_halves_cannot_take_native_readings() {
    for mutation in 0..4 {
        let mut report = report();
        let source = &mut report["native_glyph_observations"][0]["candidate"];
        match mutation {
            0 => {
                source["bounds"][0] = serde_json::json!(source["bounds"][0].as_f64().unwrap() + 1.)
            }
            1 => source["source_document_sha256"] = serde_json::json!("foreign PDF"),
            2 => source["source_word_box"][1] = serde_json::json!(0.),
            _ => {
                source["glyph"] = serde_json::json!(2805);
                source["raw_index"] = serde_json::json!(0);
                source["cid"] = serde_json::json!(26141);
            }
        }
        let result = analyze_pdf_with_observations(PDF, report).unwrap();
        assert_eq!(result.native_glyphs.bindings.len(), 1, "{mutation}");
        assert_eq!(result.native_glyphs.unresolved.len(), 1);
        assert_eq!(result.native_glyphs.source_issues.len(), 1);
        let text = result
            .documents
            .iter()
            .map(|d| d.field_text(Field::Paragraph))
            .collect::<String>();
        assert_eq!(text.matches('�').count(), 1);
        assert!(text.contains("（１８）�"));
    }
}
#[test]
fn competing_observations_reject_both_without_confidence_or_order_selection() {
    let mut report = report();
    let mut other = report["native_glyph_observations"][0].clone();
    other["text"] = serde_json::json!("（ア）");
    report["native_glyph_observations"]
        .as_array_mut()
        .unwrap()
        .push(other);
    let result = analyze_pdf_with_observations(PDF, report).unwrap();
    assert_eq!(result.native_glyphs.bindings.len(), 1);
    assert_eq!(result.native_glyphs.unresolved.len(), 2);
    let text = result
        .documents
        .iter()
        .map(|d| d.field_text(Field::Paragraph))
        .collect::<String>();
    assert_eq!(text.matches('�').count(), 1);
}
#[test]
fn foreign_pdf_identity_fails_before_any_native_or_painted_reading() {
    let mut report = report();
    report["source"]["document_sha256"] = serde_json::json!("foreign");
    assert!(analyze_pdf_with_observations(PDF, report).is_err());
}
