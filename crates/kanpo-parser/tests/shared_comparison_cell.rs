#[test]
fn source_shared_row_is_not_the_before_column() {
    let pdf=include_bytes!("data/shared-comparison-cell/20260813h017670003.pdf");
    let a=kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    // Freeze the complete public contract around the reviewed row correction;
    // this reference is not an independently correct full-page transcription.
    let actual: serde_json::Value = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    let expected: serde_json::Value = serde_json::from_str(include_str!("data/shared-comparison-cell/20260813h017670003-reviewed138.json")).unwrap();
    assert_eq!(actual, expected);
    assert_eq!(a.articles.len(),2);
    let page=&a.pages[0];
    let prefix=page.words.iter().find(|w|w.raw=="表中の﹇").unwrap();
    let close=page.words.iter().find(|w|w.raw.starts_with("﹈の記載")).unwrap();
    let role=|id|page.regions.iter().find(|r|r.lines.iter().any(|l|l.words.contains(&id))).unwrap().role;
    assert_eq!(role(prefix.id),kanpo_parser::trace::Role::Outside);
    assert_eq!(role(close.id),role(prefix.id),"source shared-row continuation must have the same owner as its prefix");
    let note="備考　表中の〔";
    let start=a.text.find(note).unwrap();
    let end=a.text.find("〕の記載").unwrap();
    assert!(start<end,"shared annotation must follow source reading order");
}
