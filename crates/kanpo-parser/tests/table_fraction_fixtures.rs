//! 国債の発行条件表で、本文の途中にある分子・分母を同じセルの式へ戻す。

const PDF: &[u8] = include_bytes!("archive/pages/20261014g00225/20261014g002250002.pdf");

#[test]
fn fractions_inside_cells_keep_the_surrounding_sentences_and_source_words() {
    let out = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(PDF).unwrap());
    for expected in ["額面金額×(1.7/100)×(１/２)", "額面金額の総額×(2.2/100)×(81/365)"] {
        assert!(out.text.contains(expected), "{expected}: {}", out.text);
    }
    assert!(out.text.contains("令和９年３月１日を支払期とし、次の算式により算出した金額を支払う。"));
    assert!(out.text.contains("第20号に規定する期日に払い込むものとする。"));
    let mut seen = std::collections::HashSet::new();
    for id in out.pages[0].regions.iter().flat_map(|r| &r.lines).flat_map(|l| l.words.iter().chain(l.table.iter().flat_map(|t| t.cells.iter().flat_map(|c| &c.words)))) {
        assert!(seen.insert(id), "同じ語 {id} を複数回組んだ");
    }
    for line in out.pages[0].regions.iter().flat_map(|r| &r.lines).filter(|l| l.table.is_some()) {
        assert_eq!(line.text, line.table.as_ref().unwrap().render());
    }
}

#[test]
fn cell_formulas_without_short_fraction_bars_are_not_restored() {
    let mut pages = kanpo_parser::page_inputs(PDF).unwrap();
    let rules = pages[0].rules.as_mut().unwrap();
    rules.horizontal.retain(|h| h.x1 - h.x0 >= 30.0);
    let out = kanpo_parser::analyze_item(&pages);
    assert!(!out.text.contains("(1.7/100)"));
    assert!(!out.text.contains("(2.2/100)"));
}
