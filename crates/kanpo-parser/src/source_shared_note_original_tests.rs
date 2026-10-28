// Original source: upper/lower comparison cells separated by y425 ruling; shared left note is outside both fields.
#[test]
fn original_shared_note_cannot_eject_the_comparison_columns() {
    let bytes = include_bytes!("../tests/data/enclosed-warichu/20260810g001790005.pdf");
    let a = crate::analyze_item(&crate::page_inputs(bytes).unwrap());
    let mut roles = std::collections::HashMap::new();
    for r in &a.pages[0].regions {
        for l in &r.lines {
            for id in l.words.iter().chain(
                l.table
                    .iter()
                    .flat_map(|t| t.cells.iter().flat_map(|c| &c.words)),
            ) {
                assert!(roles.insert(*id, r.role).is_none());
            }
        }
    }
    for id in [
        58, 59, 60, 62, 63, 65, 66, 69, 70, 73, 76, 79, 84, 86, 88, 89,
    ] {
        assert_eq!(
            roles[&id],
            crate::trace::Role::After,
            "upper source field Word{id}"
        );
    }
    for id in [61, 67, 72, 74, 77, 83, 90] {
        assert_eq!(
            roles[&id],
            crate::trace::Role::Before,
            "lower source field Word{id}"
        );
    }
    for id in [105, 106, 107] {
        assert!(
            !matches!(
                roles[&id],
                crate::trace::Role::After | crate::trace::Role::Before
            ),
            "shared note Word{id}"
        );
    }
}
