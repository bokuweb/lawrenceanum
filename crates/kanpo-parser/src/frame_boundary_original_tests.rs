use crate::{analyze_item, page_inputs, trace::Role};
#[test]
fn bounded_table_annotation_precedes_the_independent_notice() {
    let bytes = include_bytes!("../tests/data/enclosed-warichu/20260824g001880038.pdf");
    let a = analyze_item(&page_inputs(bytes).unwrap());
    let p = &a.pages[0];
    let mut emitted = vec![];
    let mut roles = std::collections::HashMap::new();
    for r in &p.regions {
        for l in &r.lines {
            for &id in l.words.iter().chain(
                l.table
                    .iter()
                    .flat_map(|t| t.cells.iter().flat_map(|c| &c.words)),
            ) {
                emitted.push(id);
                roles.insert(id, r.role);
            }
        }
    }
    let note = [100, 101, 102];
    let heading = [107, 104, 108];
    for id in note.iter().chain(&heading) {
        assert_eq!(
            emitted.iter().filter(|v| *v == id).count(),
            1,
            "source word{id} exactly once"
        );
    }
    let last_note = note
        .iter()
        .map(|id| emitted.iter().position(|v| v == id).unwrap())
        .max()
        .unwrap();
    let first_heading = heading
        .iter()
        .map(|id| emitted.iter().position(|v| v == id).unwrap())
        .min()
        .unwrap();
    assert!(
        last_note < first_heading,
        "table annotation must precede outside notice, text={}",
        a.text
    );
    for id in heading {
        assert!(
            !matches!(roles[&id], Role::After | Role::Before),
            "outside notice cannot become a table field"
        );
    }
}
