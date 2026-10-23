fn original_frame(page: &str) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let inputs = crate::page_inputs(
        &std::fs::read(
            root.join("tests/archive/pages")
                .join(&page[..14])
                .join(format!("{page}.pdf")),
        )
        .unwrap(),
    )
    .unwrap();
    let a = crate::analyze_item(&inputs);
    let parts = crate::notice_title_scope_tests::parts(&a, &inputs);
    let g: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/expected-company-article-sources/director-common-financial-frames.json"
    ))
    .unwrap();
    let g = g["frames"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["page"] == page)
        .unwrap();
    let expected: std::collections::HashSet<usize> = g["source_word_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as usize)
        .collect();
    assert_eq!(
        parts
            .iter()
            .filter(|part| part.len() == expected.len()
                && part
                    .iter()
                    .copied()
                    .collect::<std::collections::HashSet<_>>()
                    == expected)
            .count(),
        1,
        "complete original parent/related-statement ownership: {page}"
    );
}
#[test]
fn original_director_merger_keeps_the_entire_related_statement() {
    original_frame("20260727g001670099");
}
#[test]
fn original_director_capital_reduction_keeps_both_related_statements() {
    original_frame("20260810g001790149");
}
#[test]
fn original_director_division_keeps_the_entire_related_statement() {
    original_frame("20260813g001810069");
}
