use std::collections::BTreeSet;
#[test]
fn a_common_notice_precedes_its_referenced_statements_even_with_unresolved_source_ruling_glyphs() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/source-financial-native");
    let g: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("original-reading-gold.json")).unwrap())
            .unwrap();
    let page = g["page"].as_str().unwrap();
    let first = g["expected_first_source_Word_id_in_common_publication"]
        .as_u64()
        .unwrap() as usize;
    let pdf = root.join(format!("{page}.pdf"));
    let inputs = crate::page_inputs(&std::fs::read(&pdf).unwrap()).unwrap();
    let a = crate::analyze_item(&inputs);
    let groups = crate::notice_title_scope_tests::parts(&a, &inputs);
    let frames: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join(format!("{page}-frame-gold.json"))).unwrap(),
    )
    .unwrap();
    let wanted = frames["original_independent_frames"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            f["used_Word_ids"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_u64().unwrap() as usize)
                .collect::<BTreeSet<_>>()
        })
        .find(|s| s.contains(&first))
        .unwrap();
    let found = groups
        .iter()
        .filter(|s| s.iter().copied().collect::<BTreeSet<_>>() == wanted)
        .collect::<Vec<_>>();
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].first(),
        Some(&first),
        "the complete operative notice is the reading root, not a preceding financial annex"
    );
}
