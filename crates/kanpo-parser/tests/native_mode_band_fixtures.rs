use kanpo_parser::{analyze_item, page_inputs, PageInput};
use serde_json::{json, Value};
use std::collections::BTreeMap;
fn material(a: &Value) -> (BTreeMap<(String, char), usize>, Vec<Value>) {
    let mut chars = BTreeMap::new();
    let mut tables = vec![];
    for article in a["articles"].as_array().unwrap() {
        for block in article["document"]["blocks"].as_array().unwrap() {
            assert_eq!(block["kind"], "paragraph");
            for run in block["runs"].as_array().unwrap() {
                if let Some(t) = run.get("table") {
                    tables.push(t.clone());
                } else {
                    let mut style = run.as_object().unwrap().clone();
                    style.remove("text");
                    let style = serde_json::to_string(&style).unwrap();
                    for c in run["text"]
                        .as_str()
                        .unwrap()
                        .chars()
                        .filter(|c| !c.is_whitespace())
                    {
                        *chars.entry((style.clone(), c)).or_default() += 1;
                    }
                }
            }
        }
    }
    (chars, tables)
}
fn value(input: &[PageInput]) -> Value {
    serde_json::from_str(&serde_json::to_string(&analyze_item(input)).unwrap()).unwrap()
}
fn reference() -> Value {
    serde_json::from_str(include_str!("data/native-mode-bands/references.json")).unwrap()
}
fn original() -> Vec<PageInput> {
    page_inputs(include_bytes!(
        "data/native-mode-bands/20260916g002060002.pdf"
    ))
    .unwrap()
}
fn check(input: &[PageInput], context: bool) {
    let r = reference();
    let b = &r[if context {
        "baseline_context"
    } else {
        "baseline_original"
    }];
    let a = value(input);
    let ap = &a["pages"][0];
    let bp = &b["pages"][0];
    for key in ["width", "height", "layout", "words", "tables"] {
        assert_eq!(ap[key],bp[key],"All original native source occurrences, raw characters, display/glyph boxes and tables are preserved: {key}");
    }
    let roles = |p: &Value| {
        p["regions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                let mut o = r.as_object().unwrap().clone();
                o.remove("lines");
                json!(o)
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(roles(ap), roles(bp));
    assert_eq!(ap["regions"].as_array().unwrap().len(), 1);
    assert_eq!(ap["regions"][0]["lines"],r["expected_lines"],"All original page lines match the three independently printed TOC parenthesis rows; no numeral is synthesized");
    let expected = r[if context {
        "expected_context_records"
    } else {
        "expected_records"
    }]
    .as_array()
    .unwrap();
    let actual = a["articles"].as_array().unwrap();
    assert_eq!(
        actual.len(),
        expected.len(),
        "Independently printed TOC entries and legal instruments own separate records"
    );
    for (article, text) in actual.iter().zip(expected) {
        assert_eq!(&article["text"], text);
    }
    assert_eq!(material(&a),material(b),"All document content characters/styles and ordered whole Tables are preserved; whitespace follows the existing formatter after source line correction");
    if context {
        assert_eq!(a["pages"].as_array().unwrap().len(), 2);
        assert_eq!(
            a["pages"][1], b["pages"][1],
            "The next page keeps its original complete PageTrace"
        );
    }
}
#[test]
fn original_page_numbers_own_their_toc_rows_and_five_orders_own_their_outlines() {
    check(&original(), false);
}
#[test]
fn original_next_page_continues_its_own_order_without_borrowing_a_new_number() {
    let mut input = original();
    input.extend(
        page_inputs(include_bytes!(
            "data/legal-summaries/20260916g002060003.pdf"
        ))
        .unwrap(),
    );
    check(&input, true);
}
#[test]
fn original_emission_reversal_keeps_all_occurrences_and_source_ownership() {
    let input: Vec<PageInput> = serde_json::from_str(include_str!(
        "data/native-mode-bands/reversed-native-emission-input.json"
    ))
    .unwrap();
    check(&input, false);
}
#[test]
fn absent_source_flow_keeps_the_complete_old_analysis_instead_of_guessing() {
    let input: Vec<PageInput> = serde_json::from_str(include_str!(
        "data/native-mode-bands/missing-native-flow-input.json"
    ))
    .unwrap();
    assert_eq!(value(&input), reference()["baseline_original"]);
}
#[test]
fn a_conflicting_native_baseline_keeps_the_complete_old_analysis() {
    let input: Vec<PageInput> = serde_json::from_str(include_str!(
        "data/native-mode-bands/conflicting-number-baseline-input.json"
    ))
    .unwrap();
    assert_eq!(value(&input), reference()["baseline_original"]);
}

#[test]
fn a_closed_native_frame_fences_a_new_source_line_affinity() {
    let input: Vec<PageInput> = serde_json::from_str(include_str!(
        "data/native-mode-bands/native-frame-fence-input.json"
    ))
    .unwrap();
    assert_eq!(value(&input), reference()["baseline_frame_control"]);
}
