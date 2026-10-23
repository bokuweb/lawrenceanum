const PDF: &[u8] = include_bytes!("archive/pages/20261014g00225/20261014g002250011.pdf");
const FORMULA: &str="額面金額＋経過利子に相当する金額−（初期利子に相当する金額×(79.685/100)＋第２期利子に相当する金額×(79.685/100)）";

#[test]
fn real_wrapped_fraction_keeps_the_formula_and_all_source_words() {
    let out=kanpo_parser::analyze_item(&kanpo_parser::page_inputs(PDF).unwrap());
    assert_eq!(out.text.matches(FORMULA).count(),2,"{}",out.text);
    assert!(out.text.contains("（１）　令和９年９月15日から令和10年３月15日前までの間の場合"));
    assert!(out.text.contains("令和10年３月15日以後の場合"));
    let mut seen=std::collections::HashSet::new();
    for id in out.pages[0].regions.iter().flat_map(|r|&r.lines)
        .flat_map(|l|l.words.iter().chain(l.table.iter().flat_map(|t|t.cells.iter().flat_map(|c|&c.words)))) {
        assert!(seen.insert(id),"語 {id} を複数回使用");
    }
    for line in out.pages[0].regions.iter().flat_map(|r|&r.lines).filter(|l|l.table.is_some()) {
        assert_eq!(line.text,line.table.as_ref().unwrap().render());
    }
}

#[test]
fn the_wrapped_fraction_does_not_use_only_its_siblings_bar() {
    let mut pages=kanpo_parser::page_inputs(PDF).unwrap();
    // 折返し後の左端の分数線を除き、同じ式の後半の線は残す。
    pages[0].rules.as_mut().unwrap().horizontal.retain(|h|
        !((h.x0-165.0).abs()<0.5&&(h.y-98.5).abs()<0.5)
        && !((h.x0-533.0).abs()<0.5&&(h.y-283.679).abs()<0.5));
    let out=kanpo_parser::analyze_item(&pages);
    assert!(!out.text.contains(FORMULA),"{}",out.text);
}

#[test]
fn an_unrelated_line_ending_does_not_supply_a_multiplication_sign() {
    let mut pages=kanpo_parser::page_inputs(PDF).unwrap();
    pages[0].xhtml=pages[0].xhtml.replace("初期利子に相当する金額×</word>","初期利子に相当する金額＋</word>");
    let out=kanpo_parser::analyze_item(&pages);
    assert!(!out.text.contains(FORMULA));
}
