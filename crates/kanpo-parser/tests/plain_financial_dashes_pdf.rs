//! 外枠だけ、又は格子のない財務表でも原字の横棒を元の科目行へ戻す。
use std::collections::HashSet;
fn check(pdf: &[u8], expected: &[&str]) {
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let page = &a.pages[0];
    for text in expected {
        let line = page
            .regions
            .iter()
            .flat_map(|r| &r.lines)
            .find(|l| l.text == *text)
            .unwrap_or_else(|| panic!("missing {text}: {}", a.text));
        assert!(line.table.is_none());
        assert!(line
            .words
            .iter()
            .any(|&id| page.words[id].raw == "\u{E61C}" && page.words[id].text == "－"));
    }
    let mut seen = HashSet::new();
    for id in page
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .flat_map(|l| l.words.iter().chain(l.table.iter().flat_map(|t| t.cells.iter().flat_map(|c| &c.words))))
    {
        assert!(seen.insert(*id), "duplicate word {id}");
    }
}
#[test]
fn plain_financial_rows_restore_their_existing_bar_at_one_numeric_edge() {
    check(
        include_bytes!("archive/pages/20261016g00228/20261016g002280278.pdf"),
        &["徴収不能引当金　△1,041,096　－"],
    );
    check(
        include_bytes!("archive/pages/20261016g00228/20261016g002280379.pdf"),
        &["資産除去債務の履行等による減少額　－千円"],
    );
    check(
        include_bytes!("archive/pages/20261016g00228/20261016g002280516.pdf"),
        &["国庫納付金の支払額　－", "Ⅳ　資金に係る換算差額　－"],
    );
    check(
        include_bytes!("archive/pages/20260914g00204/20260914g002040066.pdf"),
        &["Ⅲ　次期繰越利益　－"],
    );
    check(
        include_bytes!("archive/pages/20261016g00228/20261016g002280370.pdf"),
        &["Ⅳ　資金に係る換算差額　－"],
    );
}
