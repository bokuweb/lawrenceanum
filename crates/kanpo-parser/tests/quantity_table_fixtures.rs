//! 縦罫の無い縦組みの用具一覧で、名称・数量・備考の対応を保つ。

const PDF: &[u8] = include_bytes!("archive/pages/20261009g00223/20261009g002230035.pdf");

#[test]
fn inferred_quantity_columns_keep_names_counts_and_wrapped_notes_together() {
    let out = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(PDF).unwrap());
    for expected in [
        "安全帯　五",
        "投光器一式　一　発電機は三百ワット／百ボルト以上のもの",
        "車両移動器具※　一　耐荷重二トン以上",
    ] {
        assert!(out.text.lines().any(|line| line.trim_end() == expected), "{expected}: {}", out.text);
    }
    let repeated = "放射線防護服（個人用線量計を含む。）※※　二　個人用線量計は、フィルムバッジで代替することができる。";
    assert_eq!(out.text.lines().filter(|line| line == &repeated).count(), 2, "{}", out.text);
    let tables: Vec<_> = out.pages[0].regions.iter().flat_map(|r| &r.lines).filter_map(|l| l.table.as_ref()).collect();
    assert!(tables.iter().any(|t| t.rows == 38 && t.cols == 3));
    let mut seen = std::collections::HashSet::new();
    for id in out.pages[0].regions.iter().flat_map(|r| &r.lines).flat_map(|l| l.words.iter().chain(l.table.iter().flat_map(|t| t.cells.iter().flat_map(|c| &c.words)))) {
        assert!(seen.insert(id), "同じ語 {id} を複数回組んだ");
    }
}

#[test]
fn a_band_with_non_numeric_entries_is_not_inferred_as_a_quantity_table() {
    let mut pages = kanpo_parser::page_inputs(PDF).unwrap();
    // 同じ列位置に短い語が並ぶだけでは数量表にしない。
    assert!(pages[0].xhtml.contains(">一</word>"));
    pages[0].xhtml = pages[0].xhtml.replace(">一</word>", ">甲</word>");
    let out = kanpo_parser::analyze_item(&pages);
    assert!(!out.pages[0].regions.iter().flat_map(|r| &r.lines).filter_map(|l| l.table.as_ref()).any(|t| t.rows == 38 && t.cols == 3));
}
