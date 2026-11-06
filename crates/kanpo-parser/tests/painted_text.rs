use kanpo_parser::painted_text::analyze_pdf;
use serde_json::{json, Value};

const PDF: &[u8] = include_bytes!("data/workbook/case-06.pdf");
fn report() -> Value {
    serde_json::from_str(include_str!(
        "data/workbook/case-06.paint-observations.json"
    ))
    .unwrap()
}

#[test]
fn all_original_paint_readings_are_placed_without_changing_native_words_or_ownership() {
    let input = report();
    let result = analyze_pdf(PDF, input.clone()).unwrap();
    assert!(result.unresolved.is_empty(), "{:?}", result.unresolved);
    assert_eq!(result.bindings.len(), 8);
    assert_eq!(result.observation_report, input);
    let normal = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(PDF).unwrap());
    assert_eq!(
        serde_json::to_value(&result.native).unwrap(),
        serde_json::to_value(normal).unwrap()
    );
    let review: Value =
        serde_json::from_str(include_str!("data/workbook/painted-compound-review.json")).unwrap();
    for (binding, source) in result
        .bindings
        .iter()
        .zip(review["observations"].as_array().unwrap())
    {
        let row = &result.observation_report["observations"][binding.observation];
        for key in ["page", "before_glyph", "after_glyph", "text"] {
            assert_eq!(row[key], source[key], "{key}");
        }
        assert_eq!(binding.page, source["page"].as_u64().unwrap() as usize);
    }
    for (pi, (original, observed)) in result
        .native
        .pages
        .iter()
        .zip(&result.observed.pages)
        .enumerate()
    {
        assert_eq!(
            serde_json::to_value(&original.words).unwrap(),
            serde_json::to_value(&observed.words).unwrap()
        );
        assert_eq!(original.regions.len(), observed.regions.len());
        for (ri, (a, b)) in original.regions.iter().zip(&observed.regions).enumerate() {
            assert_eq!((a.role, a.unit, a.part), (b.role, b.unit, b.part));
            assert_eq!(a.lines.len(), b.lines.len());
            for (li, (a, b)) in a.lines.iter().zip(&b.lines).enumerate() {
                assert_eq!(a.words, b.words);
                assert_eq!(a.table, b.table);
                let mut expected = a.text.clone();
                let mut own = result
                    .bindings
                    .iter()
                    .filter(|s| (s.page, s.region, s.line) == (pi, ri, li))
                    .collect::<Vec<_>>();
                own.sort_by_key(|s| std::cmp::Reverse(s.byte_offset));
                for seam in own {
                    expected.insert_str(
                        seam.byte_offset,
                        input["observations"][seam.observation]["text"]
                            .as_str()
                            .unwrap(),
                    );
                }
                assert_eq!(b.text, expected);
            }
        }
    }
    // Independently reviewed missing reference numbers, in the rebuilt article
    // and structured document as well as the page lines.
    let original = result.native.text.matches("別表第五（二十八）").count();
    assert_eq!(
        result.observed.text.matches("別表第五（二十八）").count(),
        original + 4
    );
    let original = result.native.text.matches("別表第五（三十一）").count();
    assert_eq!(
        result.observed.text.matches("別表第五（三十一）").count(),
        original + 4
    );
    let text = result
        .observed
        .articles
        .iter()
        .map(|a| a.document.to_text())
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(text.matches("別表第五（二十八）").count(), 4);
    assert_eq!(text.matches("別表第五（三十一）").count(), 4);
}

#[test]
fn wrong_pdf_identity_fails_before_any_reading_is_applied() {
    for (key, value) in [
        ("document_sha256", json!("0".repeat(64))),
        ("document_bytes", json!(0)),
    ] {
        let mut input = report();
        input["source"][key] = value;
        assert!(analyze_pdf(PDF, input)
            .unwrap_err()
            .to_string()
            .contains("another PDF"));
    }
}

#[test]
fn competing_observations_do_not_pick_a_winner() {
    let mut input = report();
    let mut duplicate = input["observations"][0].clone();
    duplicate["text"] = json!("different unverified reading");
    input["observations"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    let result = analyze_pdf(PDF, input).unwrap();
    assert_eq!(result.bindings.len(), 7);
    assert_eq!(result.unresolved.len(), 2);
    assert!(result
        .unresolved
        .iter()
        .all(|u| u.reason.contains("competing")));
}

#[test]
fn source_changes_and_non_candidate_observations_remain_unresolved() {
    let mut changes = Vec::new();
    let original = report()["observations"][0].clone();
    for (key, value) in [
        ("gap", json!([0, 0, 1, 1])),
        ("before_glyph", json!(0)),
        ("path_ids", json!([0])),
        ("text", Value::Null),
        ("status", json!("unclassified_geometry")),
    ] {
        let mut row = original.clone();
        row[key] = value;
        changes.push(row);
    }
    let mut duplicate_path = original;
    let id = duplicate_path["path_ids"][0].clone();
    duplicate_path["path_ids"].as_array_mut().unwrap().push(id);
    changes.push(duplicate_path);
    let mut input = report();
    input["observations"] = json!(changes);
    let result = analyze_pdf(PDF, input).unwrap();
    assert!(result.bindings.is_empty());
    assert_eq!(result.unresolved.len(), 6);
    assert_eq!(
        serde_json::to_value(&result.native).unwrap(),
        serde_json::to_value(&result.observed).unwrap()
    );
}
