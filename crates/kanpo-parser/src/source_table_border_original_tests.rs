#[test]
fn complete_inner_table_borders_stay_on_source_rulings() {
    let bytes = include_bytes!("../tests/archive/pages/20260727g00167/20260727g001670020.pdf");
    let rules = crate::page_rules(bytes).unwrap();
    let xs: Vec<_> = rules.vertical.iter().map(|r| r.x).collect();
    let a = crate::analyze_item(&crate::page_inputs(bytes).unwrap());
    let mut seen = 0;
    for p in &a.pages {
        for r in &p.regions {
            for l in &r.lines {
                if let Some(t) = &l.table {
                    seen += 1;
                    assert!(
                        xs.contains(&t.x0),
                        "left table edge{} must retain an original vertical ruling",
                        t.x0
                    );
                }
            }
        }
    }
    assert!(seen > 0, "source inner tables must remain tables");
}
