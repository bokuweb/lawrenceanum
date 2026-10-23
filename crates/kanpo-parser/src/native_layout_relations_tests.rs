//! Original-image source relations that the rejected partial-coordinate
//! prototype broke despite preserving every character and passing lib tests.
use std::collections::HashSet;
fn gold() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../tests/expected-native-layout-source-relations.json"
    ))
    .unwrap()
}
fn actual(page: &str) -> (crate::Analysis, Vec<crate::PageInput>) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/archive/pages")
        .join(&page[..14])
        .join(format!("{page}.pdf"));
    let inputs = crate::page_inputs(&std::fs::read(path).unwrap()).unwrap();
    (crate::analyze_item(&inputs), inputs)
}
fn ids(v: &serde_json::Value) -> Vec<usize> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as usize)
        .collect()
}
#[test]
fn original_name_fragments_remain_in_their_original_column_and_order() {
    let g = gold();
    let g = &g["same_column"];
    let (a, _) = actual(g["page"].as_str().unwrap());
    let wanted = ids(&g["ids"]);
    assert!(
        a.pages[0].regions.iter().flat_map(|r| &r.lines).any(|l| {
            let found: Vec<_> = l
                .words
                .iter()
                .copied()
                .filter(|id| wanted.contains(id))
                .collect();
            found == wanted
        }),
        "a name fragment moved behind the rest of its printed column"
    );
}
#[test]
fn original_before_after_cells_stay_in_the_same_printed_row() {
    let g = gold();
    let g = &g["same_printed_row"];
    let (a, _) = actual(g["page"].as_str().unwrap());
    let wanted: HashSet<_> = ids(&g["ids"]).into_iter().collect();
    assert!(
        a.pages[0].regions.iter().flat_map(|r| &r.lines).any(|l| {
            wanted.is_subset(&l.words.iter().copied().collect())
                || l.table.as_ref().is_some_and(|t| {
                    (0..t.rows).any(|row| {
                        wanted.is_subset(
                            &t.cells
                                .iter()
                                .filter(|c| c.row == row)
                                .flat_map(|c| c.words.iter().copied())
                                .collect(),
                        )
                    })
                })
        }),
        "the two cells of one original row were split into unrelated lines"
    );
}
#[test]
fn a_related_balance_sheet_is_not_reassigned_to_an_independent_notice() {
    let g = gold();
    let g = &g["common_notice_versus_independent_notice"];
    let (a, inputs) = actual(g["page"].as_str().unwrap());
    let parts = crate::notice_title_scope_tests::parts(&a, &inputs);
    let owner = |key: &str| {
        let wanted: HashSet<_> = ids(&g[key]).into_iter().collect();
        let indices: Vec<_> = parts
            .iter()
            .enumerate()
            .filter(|(_, p)| wanted.is_subset(&p.iter().copied().collect()))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(indices.len(), 1);
        indices[0]
    };
    assert_eq!(
        owner("parent_ids"),
        owner("related_financial_ids"),
        "related statement left its parent publication"
    );
    assert_ne!(owner("parent_ids"), owner("independent_financial_ids"));
}
