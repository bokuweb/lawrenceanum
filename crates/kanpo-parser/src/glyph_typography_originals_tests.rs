use std::collections::HashSet;
#[test]
fn original_eight_frames_including_multiple_period_finance_own_all_656_words_once() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let inputs = crate::page_inputs(
        &std::fs::read(root.join("tests/archive/pages/20260721g00161/20260721g001610161.pdf"))
            .unwrap(),
    )
    .unwrap();
    let a = crate::analyze_item(&inputs);
    let parts = crate::notice_title_scope_tests::parts(&a, &inputs);
    let gold: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/expected-company-article-sources/20260721g001610161-eight-enclosures.json"
    ))
    .unwrap();
    assert_eq!(parts.len(), 8);
    let mut seen = HashSet::new();
    for (part, g) in parts
        .iter()
        .zip(gold["expected_source_groups"].as_array().unwrap())
    {
        let actual: HashSet<_> = part.iter().copied().collect();
        let wanted: HashSet<_> = g["source_word_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i.as_u64().unwrap() as usize)
            .collect();
        assert_eq!(actual, wanted);
        assert_eq!(actual.len(), part.len());
        for id in actual {
            assert!(seen.insert(id));
        }
    }
    assert_eq!(seen.len(), 656);
    assert_eq!(
        seen,
        a.pages[0]
            .words
            .iter()
            .filter(|w| matches!(w.status, crate::trace::WordStatus::Used))
            .map(|w| w.id)
            .collect()
    );
}
