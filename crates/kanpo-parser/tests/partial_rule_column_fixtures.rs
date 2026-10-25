//! Source-order witnesses are manually checked against original PDF pixels.
//! Complete Analysis JSON freezes reviewed behavior for regression; it does not
//! establish that every existing omission or table-role issue is resolved.
fn check(id: &str, pdf: &[u8], reviewed: &str) {
    let analysis = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let witnesses: serde_json::Value = serde_json::from_str(include_str!(
        "data/partial-rule-columns/source-order-witnesses130.json"
    )).unwrap();
    let witness = witnesses["rows"].as_array().unwrap().iter()
        .find(|row| row["page"].as_str() == Some(id)).unwrap();
    if let Some(fragment) = witness["fragment"].as_str() {
        assert!(analysis.text.contains(fragment), "source-order witness: {id}");
    }
    if let Some(ordered) = witness["ordered"].as_array() {
        let mut at = 0;
        for fragment in ordered {
            let fragment = fragment.as_str().unwrap();
            let offset = analysis.text[at..].find(fragment)
                .unwrap_or_else(|| panic!("source-order witness: {id}, missing {fragment:?}"));
            at += offset + fragment.len();
        }
    }
    // Compare serialized JSON on both sides, preserving f32 serialization.
    let actual: serde_json::Value = serde_json::from_str(
        &serde_json::to_string(&analysis).unwrap()
    ).unwrap();
    let expected: serde_json::Value = serde_json::from_str(reviewed).unwrap();
    assert_eq!(actual, expected, "complete reviewed Analysis regression: {id}");
}
macro_rules! fixture {
    ($name:ident, $id:literal) => {
        #[test] fn $name() {
            check($id,
                include_bytes!(concat!("data/partial-rule-columns/",$id,".pdf")),
                include_str!(concat!("data/partial-rule-columns/",$id,"-reviewed130.json")));
        }
    };
}
fixture!(preface_finishes_before_lower_record, "20260819h017710004");
fixture!(personnel_procedures_finish_within_each_ruled_band, "20260805h017620008");
fixture!(toc_page_number_stays_above_the_original_divider, "20260817h017690001");
fixture!(long_correction_cells_do_not_alternate_wrapped_fragments, "20260813h017670032");
fixture!(appendix_order_and_intact_signature_spaces, "20261015g002260005");
