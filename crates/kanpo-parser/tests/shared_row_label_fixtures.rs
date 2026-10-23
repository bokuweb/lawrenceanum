#[test]
fn a_shared_rate_label_precedes_both_braced_numeric_subrows() {
    let pdf = include_bytes!("archive/pages/20260724g00165/20260724g001650725.pdf");
    let out = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let lines: Vec<String> = out.text.lines().map(|s| s.chars().filter(|c| !c.is_whitespace()).collect()).collect();
    let expected = ["Ⅰの地域", "10種地", "ａ0.005100−4.1780", "ｂ0.005200−4.2800"];
    assert_eq!(lines.windows(4).filter(|w| w.iter().map(String::as_str).eq(expected)).count(), 2);
    let mut ids = std::collections::HashSet::new();
    for id in out.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .flat_map(|l| l.words.iter().chain(l.table.iter().flat_map(|t| &t.cells).flat_map(|c| &c.words)))
    {
        assert!(ids.insert(id));
    }
}
