#[test]
fn a_no_value_minus_stays_with_its_own_native_account_row() {
    let bytes = include_bytes!("../tests/data/numeric-sign/20260724g001660077.pdf");
    let a = crate::analyze_item(&crate::page_inputs(bytes).unwrap());
    let page = &a.pages[0];
    assert_eq!(page.words[1].raw, "51");
    assert_eq!(page.words[2].raw, "−");
    let tables: Vec<_> = page
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .collect();
    let t = tables
        .iter()
        .find(|t| t.cells.iter().any(|c| c.words.contains(&1)))
        .unwrap();
    let amount = t.cells.iter().find(|c| c.words.contains(&1)).unwrap();
    let dash = t.cells.iter().find(|c| c.words.contains(&2)).unwrap();
    assert_ne!(
        amount.row, dash.row,
        "two distinct printed account rows cannot share a numeric cell"
    );
    assert_eq!(amount.text, "51");
    assert_eq!(dash.text, "−");
    assert!(t
        .cells
        .iter()
        .any(|c| c.row == amount.row && c.text == "流動負債"));
    assert!(t
        .cells
        .iter()
        .any(|c| c.row == dash.row && c.text == "固定負債"));
    assert_eq!(
        t.cells
            .iter()
            .flat_map(|c| &c.words)
            .filter(|&&id| id == 1)
            .count(),
        1
    );
    assert_eq!(
        t.cells
            .iter()
            .flat_map(|c| &c.words)
            .filter(|&&id| id == 2)
            .count(),
        1
    );
}
