fn fixture() -> kanpo_parser::Analysis {
    kanpo_parser::analyze_item(
        &kanpo_parser::page_inputs(include_bytes!(
            "archive/pages/20260724g00165/20260724g001650693.pdf"
        ))
        .unwrap(),
    )
}
#[test]
fn inner_coefficient_records_keep_missing_values_in_their_own_row() {
    let a = fixture();
    let ts: Vec<_> = a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .collect();
    assert_eq!(ts.len(), 2);
    for (t, value, prior) in [
        (ts[0], "0.002", "１級地　0.007　0.164　0.077"),
        (ts[1], "0.003", "１級地　0.007　0.168　0.079"),
    ] {
        assert_eq!(t.rows, 39);
        let c = t
            .cells
            .iter()
            .find(|c| c.text.starts_with("無級地（旧４級地であつた地域）"))
            .unwrap();
        assert_eq!(
            c.text,
            format!("無級地（旧４級地であつた地域）　{value}　－　－")
        );
        assert_eq!(c.row, 30);
        assert_eq!(
            t.cells
                .iter()
                .find(|p| p.row == 29 && p.col == c.col)
                .unwrap()
                .text,
            prior
        );
        assert_eq!(
            c.words
                .iter()
                .filter(|&&id| a.pages[0].words[id].raw == "\u{E61C}")
                .count(),
            2
        );
    }
}
#[test]
fn four_original_dashes_are_used_once_without_deleting_other_words() {
    let a = fixture();
    let p = &a.pages[0];
    let ds: Vec<_> = p.words.iter().filter(|w| w.raw == "\u{E61C}").collect();
    assert_eq!(ds.len(), 4);
    assert!(
        ds.iter()
            .all(|w| w.status == kanpo_parser::trace::WordStatus::Used && w.text == "－")
    );
    let mut ids = std::collections::HashSet::new();
    for l in p.regions.iter().flat_map(|r| &r.lines) {
        for &id in l.words.iter().chain(
            l.table
                .iter()
                .flat_map(|t| t.cells.iter().flat_map(|c| &c.words)),
        ) {
            assert!(ids.insert(id));
        }
    }
    for w in &p.words {
        assert_eq!(
            ids.contains(&w.id),
            w.status == kanpo_parser::trace::WordStatus::Used
        );
    }
}

#[test]
fn the_following_school_coefficients_page_uses_the_same_record_geometry() {
    let a = kanpo_parser::analyze_item(
        &kanpo_parser::page_inputs(include_bytes!(
            "archive/pages/20260724g00165/20260724g001650694.pdf"
        ))
        .unwrap(),
    );
    let ts: Vec<_> = a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .collect();
    assert_eq!(ts.len(), 2);
    for (t, value, prev) in [
        (ts[0], "0.002", "１級地　0.007　0.167　0.080"),
        (ts[1], "0.003", "１級地　0.007　0.171　0.082"),
    ] {
        assert_eq!(t.rows, 36);
        let c = t
            .cells
            .iter()
            .find(|c| c.text.starts_with("無級地（旧４級地であつた地域）"))
            .unwrap();
        assert_eq!(c.row, 22);
        assert_eq!(
            c.text,
            format!("無級地（旧４級地であつた地域）　{value}　－　－")
        );
        assert_eq!(
            t.cells
                .iter()
                .find(|c| c.row == 21 && c.col == 2)
                .unwrap()
                .text,
            prev
        );
        assert_eq!(
            c.words
                .iter()
                .filter(|&&id| a.pages[0].words[id].raw == "\u{E61C}")
                .count(),
            2
        );
    }
}
