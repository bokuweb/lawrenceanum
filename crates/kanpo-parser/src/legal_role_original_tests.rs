#[test]
fn original_dividend_and_environment_notices_preserve_all_five_source_groups() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let inputs = crate::page_inputs(
        &std::fs::read(root.join("tests/archive/pages/20260717g00160/20260717g001600122.pdf"))
            .unwrap(),
    )
    .unwrap();
    let a = crate::analyze_item(&inputs);
    let parts = crate::notice_title_scope_tests::parts(&a, &inputs);
    let g: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/expected-company-article-sources/20260717g001600122.json"
    ))
    .unwrap();
    let expected: Vec<std::collections::HashSet<usize>> = g["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            v["source_word_ids"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as usize)
                .collect()
        })
        .collect();
    let got: Vec<std::collections::HashSet<usize>> =
        parts.iter().map(|p| p.iter().copied().collect()).collect();
    assert_eq!(got.len(), expected.len());
    for group in expected {
        assert_eq!(
            got.iter().filter(|p| **p == group).count(),
            1,
            "printed complete source membership"
        );
    }
    let all: std::collections::HashSet<_> = parts.iter().flatten().copied().collect();
    assert_eq!(all.len(), parts.iter().map(Vec::len).sum::<usize>());
    assert_eq!(
        all,
        a.pages[0]
            .words
            .iter()
            .filter(|w| matches!(w.status, crate::trace::WordStatus::Used))
            .map(|w| w.id)
            .collect()
    );
}
