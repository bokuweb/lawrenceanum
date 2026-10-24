use crate::{analyze_item, page_inputs, trace::Role};
#[test]
fn supplementary_heading_and_body_follow_both_comparison_fields() {
    let pdf = include_bytes!("../tests/archive/pages/20261001t00055/20261001t000550002.pdf");
    let a = analyze_item(&page_inputs(pdf).unwrap());
    let page = &a.pages[0];
    let aux = [31, 32, 33, 34];
    assert_eq!(page.words[31].text, "附");
    assert_eq!(page.words[32].text, "則");
    assert!(page.words[33].text.starts_with("この省令は"));
    assert!(page.words[34].text.ends_with("施行する。"));
    let mut ids = vec![];
    let mut fields = vec![];
    let mut roles = std::collections::HashMap::new();
    for r in &page.regions {
        for l in &r.lines {
            for &id in l.words.iter().chain(
                l.table
                    .iter()
                    .flat_map(|t| t.cells.iter().flat_map(|c| &c.words)),
            ) {
                ids.push(id);
                roles.insert(id, r.role);
                if matches!(r.role, Role::After | Role::Before) && !aux.contains(&id) {
                    fields.push(id);
                }
            }
        }
    }
    assert!(!fields.is_empty());
    for id in aux {
        assert_eq!(ids.iter().filter(|v| **v == id).count(), 1);
        assert!(!matches!(roles[&id], Role::After | Role::Before));
    }
    let last_field = fields
        .iter()
        .map(|i| ids.iter().position(|id| id == i).unwrap())
        .max()
        .unwrap();
    let first_aux = aux
        .iter()
        .map(|i| ids.iter().position(|id| id == i).unwrap())
        .min()
        .unwrap();
    assert!(last_field < first_aux);
}
