const PDF: &[u8] = include_bytes!("archive/pages/20260930g00215/20260930g002150475.pdf");

#[test]
fn sideways_prose_and_native_statistics_keep_each_amount_in_its_row() {
    let out = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(PDF).unwrap());
    let table = out.pages[0].regions.iter().flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref()).find(|t| t.cols == 7 && t.rows == 20).expect("7欄20行の統計表");
    let text = |row,col| table.cells.iter().find(|c| c.row==row && c.col==col).unwrap().text.as_str();
    assert_eq!(text(0,0),"租税及印紙収入");
    assert_eq!(text(0,1),"80,698,000,000");
    assert_eq!(text(0,2),"7,996,977,361\n14,552,913,703");
    assert_eq!(text(0,3),"61,672,715,366");
    assert_eq!(text(0,4),"84,222,606,431");
    assert_eq!(text(0,5),"3,524,606,431");
    assert_eq!(text(0,6),"104.3");
    assert_eq!(text(10,0),"航空機燃料税");
    assert_eq!(text(10,2),"3,272,131\n1,218");
    assert_eq!(text(19,0),"印紙収入");
    assert!(text(19,2).contains("4,988,353\n△44,061"));
    let mut seen=std::collections::HashSet::new();
    for id in out.pages[0].regions.iter().flat_map(|r| &r.lines)
        .flat_map(|l|l.words.iter().chain(l.table.iter().flat_map(|t|t.cells.iter().flat_map(|c|&c.words)))) {
        assert!(seen.insert(id),"同じ語 {id} の重複");
    }
}

#[test]
fn numeric_alignment_without_the_long_dividing_rule_does_not_change_orientation() {
    let mut pages=kanpo_parser::page_inputs(PDF).unwrap();
    pages[0].rules.as_mut().unwrap().vertical.retain(|r|(r.x-425.0).abs()>0.5);
    let out=kanpo_parser::analyze_item(&pages);
    assert!(!out.pages[0].regions.iter().flat_map(|r|&r.lines).filter_map(|l|l.table.as_ref())
        .any(|t|t.cols==7&&t.rows==20));
}

#[test]
fn a_short_rule_does_not_claim_a_full_statistics_region() {
    let mut pages=kanpo_parser::page_inputs(PDF).unwrap();
    for r in &mut pages[0].rules.as_mut().unwrap().vertical {
        if (r.x-425.0).abs()<0.5 {r.y1=r.y0+150.0;}
    }
    let out=kanpo_parser::analyze_item(&pages);
    assert!(!out.pages[0].regions.iter().flat_map(|r|&r.lines).filter_map(|l|l.table.as_ref())
        .any(|t|t.cols==7&&t.rows==20));
}
