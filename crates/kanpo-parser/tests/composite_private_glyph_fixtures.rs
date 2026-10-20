fn ids_once(p: &kanpo_parser::trace::PageTrace) {
    let mut ids = std::collections::HashSet::new();
    for id in p
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .flat_map(|l| l.words.iter().chain(l.table.iter().flat_map(|t| &t.cells).flat_map(|c| &c.words)))
    {
        assert!(ids.insert(*id), "repeated {id}");
    }
    let used: std::collections::HashSet<_> = p
        .words
        .iter()
        .filter(|w| matches!(w.status, kanpo_parser::trace::WordStatus::Used))
        .map(|w| w.id)
        .collect();
    assert_eq!(ids, used);
}
#[test]
fn overlaid_item_numbers_restore_both_versions_of_the_insurance_form() {
    let pdf = include_bytes!("archive/pages/20260828g00192/20260828g001920015.pdf");
    let out = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    assert_eq!(out.text.matches("（１１）　比較・推奨販売の方法").count(), 2);
    assert!(out.text.contains("［（１２）〜（２３）　略］"));
    assert!(out.text.contains("［（１２）〜（２３）　同左］"));
    let p = &out.pages[0];
    let components: Vec<_> = p
        .words
        .iter()
        .filter(|w| w.raw.chars().any(|c| matches!(c as u32, 0xEAC1 | 0xEAC2 | 0xEACB | 0xEACC | 0xEACD)))
        .collect();
    assert_eq!(components.len(), 12);
    assert!(components.iter().all(|w| matches!(w.status, kanpo_parser::trace::WordStatus::Used)));
    ids_once(p);
}
#[test]
fn two_line_parentheses_enclose_only_their_own_account_names() {
    let pdf = include_bytes!("archive/pages/20260727g00167/20260727g001670087.pdf");
    let out = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let compact: String = out.text.chars().filter(|c| !c.is_whitespace()).collect();
    for s in [
        "（社員退職給付引当金）(119,971)",
        "（役員退任給与引当金）(7,299)",
        "（うち役員退職慰労引当金）(43)",
    ] {
        assert!(compact.contains(s), "missing {s}");
    }
    let p = &out.pages[0];
    assert_eq!(
        p.words
            .iter()
            .filter(|w| w.raw == "\u{EAF0}" && matches!(w.status, kanpo_parser::trace::WordStatus::Empty))
            .count(),
        3
    );
    ids_once(p);
}
#[test]
fn an_unanchored_marker_after_a_continued_table_cannot_prefix_the_previous_record() {
    let pdf = include_bytes!("archive/pages/20260930g00216/20260930g002160004.pdf");
    let out = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    assert!(out.text.starts_with("変更内容\n変更年月日"));
    assert!(!out.text.contains("（１２）"));
    assert!(out.text.contains("2026年８月10日　役員の氏名"));
    assert!(out.text.contains("届出日　2026年８月25日"));
    ids_once(&out.pages[0]);
}
