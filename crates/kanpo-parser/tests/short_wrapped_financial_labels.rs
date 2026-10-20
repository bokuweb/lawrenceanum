#[test]
fn short_numbered_account_labels_are_joined_before_their_amount() {
    let pdf = include_bytes!("archive/pages/20260925g00212/20260925g002120051.pdf");
    let out = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    assert!(out.text.contains("１年以内償還予定機構債　2,299,072,500,000"));
    assert!(out.text.contains("１年以内返済予定長期借入金　211,112,266,666"));
    let mut seen = std::collections::HashSet::new();
    for id in out.pages[0].regions.iter().flat_map(|r| &r.lines).flat_map(|l| l.words.iter().chain(l.table.iter().flat_map(|t| &t.cells).flat_map(|c| &c.words))) { assert!(seen.insert(id)); }
}
