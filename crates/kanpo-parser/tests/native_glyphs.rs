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
fn paragraph(result: &kanpo_parser::media::MediaAnalysis) -> String {
    result
        .documents
        .iter()
        .map(|d| d.field_text(Field::Paragraph))
        .collect()
}
#[test]
fn verified_static_readings_leave_no_candidates_for_recorded_native_observations() {
    // Both original glyphs are parenthesized ヲ (code 0x6570, the same number
    // as 数). The verified table now reads them, so the recorded observations
    // find no unknown glyph to bind and cannot change the text.
    let result = analyze_pdf_with_observations(PDF, report()).unwrap();
    assert!(result.native_glyphs.candidates.is_empty());
    assert!(result.native_glyphs.bindings.is_empty());
    assert_eq!(result.native_glyphs.unresolved.len(), 2);
    assert!(result.native_glyphs.source_issues.is_empty());
    let text = paragraph(&result);
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
fn an_observation_cannot_override_a_verified_static_reading() {
    let mut report = report();
    report["native_glyph_observations"][0]["text"] = serde_json::json!("（ア）");
    let result = analyze_pdf_with_observations(PDF, report).unwrap();
    assert!(result.native_glyphs.bindings.is_empty());
    let text = paragraph(&result);
    assert_eq!(text.matches("（１８）（ヲ）").count(), 2);
    assert!(!text.contains("（１８）（ア）"));
}
#[test]
fn foreign_pdf_identity_fails_before_any_native_or_painted_reading() {
    let mut report = report();
    report["source"]["document_sha256"] = serde_json::json!("foreign");
    assert!(analyze_pdf_with_observations(PDF, report).is_err());
}
