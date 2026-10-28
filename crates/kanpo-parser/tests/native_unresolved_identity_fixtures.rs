//! Expectations are constructed from baseline165, replacing only the source
//! occurrences whose unresolved CID was being cast to a Unicode codepoint.
//! Source words, positions, reading order, tables and article boundaries stay exact.
use kanpo_parser::{analyze_item, page_inputs};
use serde_json::Value;
use std::path::PathBuf;
#[test]
fn unmapped_native_cids_do_not_invent_literal_text_or_change_source_ownership() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
    let cases: Value = serde_json::from_slice(
        &std::fs::read(root.join("data/native-unresolved-identity167/index.json")).unwrap(),
    )
    .unwrap();
    let cases = cases.as_array().unwrap();
    assert_eq!(cases.len(), 12);
    assert_eq!(
        cases
            .iter()
            .map(|c| c["unresolved_words"].as_u64().unwrap())
            .sum::<u64>(),
        32
    );
    for case in cases {
        let key = case["key"].as_str().unwrap();
        let inputs =
            page_inputs(&std::fs::read(root.join(case["pdf"].as_str().unwrap())).unwrap()).unwrap();
        let actual =
            serde_json::from_str::<Value>(&serde_json::to_string(&analyze_item(&inputs)).unwrap())
                .unwrap();
        let expected: Value = serde_json::from_slice(
            &std::fs::read(root.join(case["gold"].as_str().unwrap())).unwrap(),
        )
        .unwrap();
        assert_eq!(actual, expected, "{key}");
    }
}
