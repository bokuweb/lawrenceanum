//! Original prefix reproduces continuation state; the golden freezes behavior,
//! while physical record/column witnesses check the two tables independently.
#[test]
fn preceding_original_pages_do_not_leave_either_registration_grid_fragmented() {
    let mut pages = vec![];
    for bytes in [
        include_bytes!("data/native-grid-fragments/20260813h017670001.pdf").as_slice(),
        include_bytes!("data/native-grid-fragments/20260813h017670002.pdf").as_slice(),
        include_bytes!("data/native-grid-fragments/20260813h017670003.pdf").as_slice(),
    ] {
        pages.extend(kanpo_parser::page_inputs(bytes).unwrap());
    }
    let a = kanpo_parser::analyze_item(&pages);
    let page = &a.pages[2];
    let tables = page
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .collect::<Vec<_>>();
    assert_eq!(tables.len(), 2);
    assert_eq!((tables[0].rows, tables[0].cols), (6, 6));
    assert_eq!((tables[1].rows, tables[1].cols), (10, 6));
    let witness: serde_json::Value = serde_json::from_str(include_str!(
        "data/native-grid-fragments/source-left-cell-witnesses131.json"
    ))
    .unwrap();
    let compact = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    for (row, cells) in witness["rows"].as_array().unwrap().iter().enumerate() {
        for (col, value) in cells.as_array().unwrap().iter().enumerate() {
            let cell = tables[0]
                .cells
                .iter()
                .find(|c| (c.row, c.col) == (row, col))
                .unwrap();
            assert_eq!(compact(&cell.text), compact(value.as_str().unwrap()));
        }
    }
    for row in 0..10 {
        let cell = tables[1]
            .cells
            .iter()
            .find(|c| (c.row, c.col) == (row, 0))
            .unwrap();
        assert_eq!(
            compact(&cell.text),
            format!("第{}号令和８年８月13日", 31781 + row)
        );
    }
    let actual: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(page).unwrap()).unwrap();
    let reviewed: serde_json::Value = serde_json::from_str(include_str!(
        "data/native-grid-fragments/20260813h017670003-whole-page-reviewed131.json"
    ))
    .unwrap();
    assert_eq!(actual, reviewed);
}
