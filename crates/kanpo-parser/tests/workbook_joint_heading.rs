//! All printed constituents of the joint heading must retain source ownership.
#[test]
fn joint_heading_owns_both_original_ministry_occurrences() {
    let inputs = kanpo_parser::page_inputs(include_bytes!("data/workbook/case-17.pdf")).unwrap();
    let result = kanpo_parser::analyze_item(&inputs[..1]);
    let page = &result.pages[0];
    let line = &page.regions[0].lines[0];
    assert_eq!(line.text, "〇内閣府・財務省令第六号");
    assert_eq!(
        line.words
            .iter()
            .map(|&id| page.words[id].raw.as_str())
            .collect::<Vec<_>>(),
        ["〇", "内閣府", "財務省", "令第六号"]
    );
}
