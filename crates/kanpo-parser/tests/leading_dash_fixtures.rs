fn assert_unique_source_ids(a: &kanpo_parser::Analysis) {
    for p in &a.pages {
        let mut seen = std::collections::HashSet::new();
        for l in p.regions.iter().flat_map(|r| &r.lines) {
            if let Some(t) = &l.table {
                assert_eq!(l.text, t.render());
            }
            for &id in l.words.iter().chain(l.table.iter().flat_map(|t| t.cells.iter().flat_map(|c| &c.words))) {
                assert!(seen.insert(id), "duplicate word {id}");
            }
        }
        for w in p.words.iter().filter(|w| w.status == kanpo_parser::trace::WordStatus::Used) {
            assert!(seen.contains(&w.id), "missing word {}", w.id);
        }
    }
}
#[test]
fn numeric_continuation_recovers_eight_leading_rows() {
    let pdf = include_bytes!("archive/pages/20261016g00228/20261016g002280016.pdf");
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    assert_unique_source_ids(&a);
    let ls: Vec<_> = a.text.lines().collect();
    assert!(ls[..8].iter().all(|s| *s == "－　－　－　－　－　－　－　－　－"));
    assert_eq!(ls[8], "－　48,602　55,857　△104,459　－　－　－　－　－");
    assert_eq!(a.text.matches('－').count(), 146);
    assert!(a.pages[0]
        .words
        .iter()
        .filter(|w| w.raw == "\u{E61C}")
        .all(|w| w.status == kanpo_parser::trace::WordStatus::Used));
}
#[test]
fn original_blank_rows_stay_blank_in_a_numeric_continuation() {
    let pdf = include_bytes!("archive/pages/20261016g00228/20261016g002280332.pdf");
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    assert_unique_source_ids(&a);
    let ls: Vec<_> = a.text.lines().collect();
    assert!(ls[..2].iter().all(|s| *s == "－　－　－　－　－　－　－　－　－"));
    assert_eq!(ls[2], "－　－　－　－　－　－　－　－　4,171,157");
    assert_eq!(a.text.matches('－').count(), 167);
    let t = a.pages[0].regions.iter().flat_map(|r| &r.lines).find_map(|l| l.table.as_ref()).unwrap();
    assert_eq!(t.rows, 24);
    assert_eq!(ls.len(), 20);
    for row in [0, 3, 18, 19] {
        assert!(
            !t.cells.iter().any(|c| c.row == row && !c.text.is_empty()),
            "a blank physical row was filled: {row}"
        );
    }
    assert!(t.cells.iter().any(|c| c.row == 4 && c.col == 8 && c.text == "4,171,157"));
}
