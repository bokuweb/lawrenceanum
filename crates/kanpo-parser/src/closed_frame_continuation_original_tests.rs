#[test]
fn all_six_printed_financial_frames_and_upper_roster_keep_complete_source_ownership() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let inputs = crate::page_inputs(
        &std::fs::read(root.join("tests/archive/pages/20260804g00175/20260804g001750060.pdf"))
            .unwrap(),
    )
    .unwrap();
    let a = crate::analyze_item(&inputs);
    let parts = crate::notice_title_scope_tests::parts(&a, &inputs);
    let gold: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/expected-company-article-sources/20260804g001750060.json"
    ))
    .unwrap();
    let got: Vec<std::collections::HashSet<usize>> =
        parts.iter().map(|p| p.iter().copied().collect()).collect();
    let expected: Vec<std::collections::HashSet<usize>> = gold["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| {
            g["source_word_ids"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as usize)
                .collect()
        })
        .collect();
    assert_eq!(got.len(), expected.len());
    for group in expected {
        assert_eq!(
            got.iter().filter(|v| **v == group).count(),
            1,
            "original complete source membership"
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
