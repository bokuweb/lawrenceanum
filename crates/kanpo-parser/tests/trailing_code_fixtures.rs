#[test]
fn jis_names_and_codes_stay_in_their_own_two_columns_for_all_ninety_records() {
    let pdf = include_bytes!("archive/pages/20261020g00230/20261020g002300023.pdf");
    let out = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let expected = include_str!("archive/expected-code-pairs/20261020g002300023.tsv");
    let compact = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    let actual: Vec<_> = out.text.lines().map(compact).collect();
    let gold: Vec<_> = expected.lines().map(compact).collect();
    assert_eq!(actual, gold);
    assert_eq!(actual[0], "防せい防食用語Ｚ0103");
    assert_eq!(
        actual[46],
        "耐候性鋼用のマグ溶接及びミグ溶接用ソリッドワイヤＺ3315"
    );
    let page = &out.pages[0];
    let tables: Vec<_> = page
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .collect();
    assert_eq!(
        tables.iter().map(|t| (t.rows, t.cols)).collect::<Vec<_>>(),
        vec![(46, 2), (44, 2)]
    );
    let mut seen = std::collections::HashSet::new();
    for id in tables.iter().flat_map(|t| &t.cells).flat_map(|c| &c.words) {
        assert!(seen.insert(*id), "duplicate source ID {id}");
    }
    let used: std::collections::HashSet<_> = page
        .words
        .iter()
        .filter(|w| w.status == kanpo_parser::trace::WordStatus::Used)
        .map(|w| w.id)
        .collect();
    assert_eq!(seen, used);
    assert_eq!(seen.len(), 184);
}

#[test]
fn type_codes_ship_codes_and_adjacent_notice_prose_keep_their_original_articles() {
    let pdf = include_bytes!("archive/pages/20260810h01765/20260810h017650004.pdf");
    let out = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let expected = include_str!("archive/expected-code-pairs/20260810h017650004.txt");
    assert_eq!(out.text.trim_end(), expected.trim_end());
}
#[test]
fn map_names_in_a_different_table_are_not_attached_to_codes_as_trailing_records() {
    let pdf = include_bytes!("archive/pages/20260831h01779/20260831h017790010.pdf");
    let out = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let expected = include_str!("archive/expected-code-pairs/20260831h017790010.txt");
    assert_eq!(out.text.trim_end(), expected.trim_end());
}

#[test]
fn mixed_letter_standard_codes_keep_all_ninety_four_records_in_their_original_columns() {
    let pdf = include_bytes!("archive/pages/20261020g00230/20261020g002300017.pdf");
    let out = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let expected = include_str!("archive/expected-code-pairs/20261020g002300017.txt");
    assert_eq!(out.text.trim_end(), expected.trim_end());
    assert!(
        out.text
            .starts_with("ステンレス鋼のアノード分極曲線測定方法　Ｇ0579")
    );
    assert!(out.text.contains("溶融亜鉛めっき試験方法　Ｈ0401"));
    assert_eq!(out.text.lines().count(), 94);
    let page = &out.pages[0];
    let mut seen = std::collections::HashSet::new();
    for id in page.regions.iter().flat_map(|r| &r.lines).flat_map(|l| {
        l.words
            .iter()
            .chain(l.table.iter().flat_map(|t| &t.cells).flat_map(|c| &c.words))
    }) {
        assert!(seen.insert(*id));
    }
    let used: std::collections::HashSet<_> = page
        .words
        .iter()
        .filter(|w| w.status == kanpo_parser::trace::WordStatus::Used)
        .map(|w| w.id)
        .collect();
    assert_eq!(seen, used);
    assert_eq!(seen.len(), 188);
}
