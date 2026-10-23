//! 大きな区分見出しが欄見出しと標題の右下に置かれたときの段またぎ。
#[test]
fn primary_section_precedes_its_field_and_notice_title() {
    let pdf = include_bytes!("archive/pages/20260909h01786/20260909h017860009.pdf");
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    assert!(a.text.contains("官庁報告\n官庁事項\n関東地方整備局公示\n道路法"), "{}", a.text);
    assert!(a.text.find("九月七日同国大統領").unwrap() < a.text.find("官庁報告").unwrap());
    assert!(a.text.find("関東地方整備局公示").unwrap() < a.text.find("九州地方整備局公示").unwrap());
}
