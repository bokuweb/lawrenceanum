//! 金額の「－」を列の切り分け後に復元し、複数勘定の数値が左詰めにならないこと。

const PDF: &[u8] = include_bytes!("archive/pages/20260831g00194/20260831g001940045.pdf");

#[test]
fn known_dash_glyphs_keep_all_seven_amount_columns() {
    let pages = kanpo_parser::page_inputs(PDF).unwrap();
    let out = kanpo_parser::analyze_item(&pages);
    for expected in [
        "保険事業費　3,996　－　841　－　－　－　4,837",
        "保証事業費　－　721　－　－　－　－　721",
        "財務費用　－　－　－　－　7　－　7",
        "当期総利益　405　－　368　－　38　－　811",
    ] {
        assert!(out.text.lines().any(|line| line == expected), "{expected}: {}", out.text);
    }
    let page = &out.pages[0];
    let restored: Vec<_> = page.words.iter().filter(|w| w.raw == "\u{E61C}" && w.text == "－").collect();
    assert!(!restored.is_empty());
    assert!(restored.iter().all(|w| w.status == kanpo_parser::trace::WordStatus::Used));
    let mut seen = std::collections::HashSet::new();
    for id in page.regions.iter().flat_map(|r| &r.lines).flat_map(|l| l.words.iter().chain(l.table.iter().flat_map(|t| t.cells.iter().flat_map(|c| &c.words)))) {
        assert!(seen.insert(id), "同じ語 {id} を複数回組んだ");
    }
    assert!(restored.iter().all(|w| seen.contains(&w.id)));
}

#[test]
fn unknown_private_glyphs_are_not_invented_as_dash_amounts() {
    let mut pages = kanpo_parser::page_inputs(PDF).unwrap();
    pages[0].xhtml = pages[0].xhtml.replace('\u{E61C}', "\u{EFFF}");
    let out = kanpo_parser::analyze_item(&pages);
    assert!(!out.pages[0].words.iter().any(|w| w.raw == "\u{EFFF}" && w.text == "－"));
    assert!(!out.text.contains("財務費用　－"));
}

const NUMERIC_CONTINUATION: &[u8] = include_bytes!("archive/pages/20261016g00228/20261016g002280354.pdf");

#[test]
fn numeric_first_column_of_a_continued_table_keeps_its_dash() {
    let out = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(NUMERIC_CONTINUATION).unwrap());
    for row in [
        "－　－　273,140　273,140　273,140　273,140",
        "－　－　－　－　△2,002　△2,002",
    ] {
        assert!(out.text.lines().any(|line| line == row), "{row}: {}", out.text);
    }
}

#[test]
fn a_body_label_prevents_treating_the_first_column_as_numeric() {
    let mut pages = kanpo_parser::page_inputs(NUMERIC_CONTINUATION).unwrap();
    // 最初のデータ行に科目があれば、後続に数字が多くても先頭欄は金額専用とはみなさない。
    assert_eq!(pages[0].xhtml.matches(">613</word>").count(), 1);
    pages[0].xhtml = pages[0].xhtml.replace(">613</word>", ">科目欄</word>");
    let out = kanpo_parser::analyze_item(&pages);
    assert!(out.text.lines().any(|line| line == "　－　273,140　273,140　273,140　273,140"), "{}", out.text);
    assert!(!out.text.lines().any(|line| line == "－　－　273,140　273,140　273,140　273,140"));
}
