use kanpo_parser::trace::WordStatus;

#[test]
fn split_period_parentheses_keep_each_company_and_all_existing_source_ids() {
    let pdf = include_bytes!("archive/pages/20260810g00179/20260810g001790150.pdf");
    let inputs = kanpo_parser::page_inputs(pdf).unwrap();
    let out = kanpo_parser::analyze_item(&inputs);
    let expected = [
        "（自　令和７年５月１日\n至　令和８年４月30日）(単位：百万円)",
        "（自　令和７年１月１日\n至　令和７年12月31日）(単位：百万円)",
    ];
    for period in expected {
        assert_eq!(out.text.matches(period).count(), 1, "{period}");
    }
    let miyano = out.text.find("宮野医療器株式会社").unwrap();
    let enoteca = out.text.find("エノテカ株式会社").unwrap();
    let first = out.text.find(expected[0]).unwrap();
    let second = out.text.find(expected[1]).unwrap();
    assert!(miyano < first && first < enoteca && enoteca < second);
    assert_eq!(out.text.lines().count(), 149);
    assert!(out.text.contains("営業損失　76　法人税、住民税及び事業税　△13"));
    assert!(out.text.contains("営業外費用　221　当期純損失　457"));
    let page = &out.pages[0];
    // These IDs locate each company's original four endpoint-bearing words.
    for ids in [[552, 522, 553, 523], [241, 212, 242, 213]] {
        for (id, code) in ids.into_iter().zip(['\u{EAED}', '\u{EAEF}', '\u{EAEE}', '\u{EAF0}']) {
            assert!(page.words[id].raw.contains(code));
            assert_eq!(page.words[id].status, WordStatus::Used);
        }
    }
    let mut rendered = std::collections::HashSet::new();
    for id in page
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .flat_map(|l| l.words.iter().chain(l.table.iter().flat_map(|t| &t.cells).flat_map(|c| &c.words)))
    {
        assert!(rendered.insert(*id), "repeated source id {id}");
    }
    let used: std::collections::HashSet<_> = page.words.iter().filter(|w| w.status == WordStatus::Used).map(|w| w.id).collect();
    assert_eq!(rendered, used);
}

#[test]
fn blank_cashflow_period_is_not_inferred_from_account_names_or_placeholders() {
    let pdf = include_bytes!("archive/pages/20260731g00172/20260731g001720101.pdf");
    let out = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let expected = include_str!("archive/expected-period-boundaries/20260731g001720101.txt");
    assert_eq!(out.text.trim_end(), expected.trim_end());
}
#[test]
fn existing_two_line_account_parentheses_are_not_treated_as_periods() {
    let pdf = include_bytes!("archive/pages/20260727g00167/20260727g001670087.pdf");
    let out = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let expected = include_str!("archive/expected-period-boundaries/20260727g001670087.txt");
    assert_eq!(out.text.trim_end(), expected.trim_end());
    assert_eq!(
        out.pages[0]
            .words
            .iter()
            .filter(|w| w.raw == "\u{EAF0}" && w.status == WordStatus::Empty)
            .count(),
        3
    );
}
