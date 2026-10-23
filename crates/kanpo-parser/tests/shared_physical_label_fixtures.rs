#[test]
fn a_wrapped_common_brand_remains_one_physical_cell_above_its_two_variants() {
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(include_bytes!("archive/pages/20260722g00162/20260722g001620026.pdf")).unwrap());
    let t = a.pages[0].regions.iter().flat_map(|r| &r.lines).find_map(|l| l.table.as_ref()).unwrap();
    let common = t.cells.iter().find(|c| c.row == 12 && c.col == 0).unwrap();
    assert_eq!(common.text, "コンビナシオネス・セレクシオン");
    assert_eq!((common.rowspan, common.colspan), (2, 1));
    assert!(!t.cells.iter().any(|c| c.row == 13 && c.col == 0));
    let cell = |r, c| t.cells.iter().find(|x| x.row == r && x.col == c).unwrap().text.as_str();
    for (row, variant, length, old, new) in [
        (12, "ピラミデス", "156㎜", "61,600円", "66,000円"),
        (13, "ロブストス", "124㎜", "54,450円", "58,300円"),
    ] {
        assert_eq!(cell(row, 1), variant);
        assert_eq!(cell(row, 3), format!("{length}　６本"));
        assert_eq!(cell(row, 4), old);
        assert_eq!(cell(row, 5), new);
    }
    assert!(a.text.contains("コンビナシオネス・セレクシオン　ピラミデス"));
    assert!(a.text.contains("\n　ロブストス"));
    assert_eq!(cell(14, 0), "クアバ");
    assert!(cell(14, 1).contains("ディスティングイドス"));
    assert_eq!(cell(14, 4), "7,700円");
    assert_eq!(cell(23, 4), "8,800円");
    assert!(!t.cells.iter().any(|c| c.row == 24 && c.col == 4));
    let mut ids = std::collections::HashSet::new();
    for id in a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .flat_map(|l| l.words.iter().chain(l.table.iter().flat_map(|t| t.cells.iter().flat_map(|c| &c.words))))
    {
        assert!(ids.insert(*id), "duplicate {id}");
    }
    assert_eq!(
        common.words.iter().map(|&id| a.pages[0].words[id].text.as_str()).collect::<String>(),
        "コンビナシオネス・セレクシオン"
    );
}
#[test]
fn independent_amount_and_payment_date_labels_stay_separate() {
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(include_bytes!("archive/pages/20260930g00216/20260930g002160055.pdf")).unwrap());
    let t = a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .find(|t| t.cells.iter().any(|c| c.text == "⑧　国庫納付等の額"))
        .unwrap();
    assert!(t.cells.iter().any(|c| c.text == "納付等の年月日"));
    assert!(!t.cells.iter().any(|c| c.text.contains("国庫納付等の額納付等の年月日")));
}
#[test]
fn a_brand_shared_by_twenty_one_variants_uses_the_whole_physical_span() {
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(include_bytes!("archive/pages/20260722g00162/20260722g001620030.pdf")).unwrap());
    let t = a.pages[0].regions.iter().flat_map(|r| &r.lines).find_map(|l| l.table.as_ref()).unwrap();
    let common = t.cells.iter().find(|c| c.text == "ロメオ・Y・ジュリエッタ").unwrap();
    assert_eq!((common.row, common.rowspan), (11, 21));
    assert_eq!(t.cells.iter().find(|c| c.row == 11 && c.col == 4).unwrap().text, "6,270円");
    assert_eq!(t.cells.iter().find(|c| c.row == 31 && c.col == 5).unwrap().text, "3,080円");
}
#[test]
fn a_shared_plan_keeps_four_separate_price_records() {
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(include_bytes!("archive/pages/20260731g00173/20260731g001730071.pdf")).unwrap());
    let t = a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .find(|t| t.cells.iter().any(|c| c.text == "往復料金セットプラン"))
        .unwrap();
    let label = t.cells.iter().find(|c| c.text == "往復料金セットプラン").unwrap();
    assert_eq!(label.rowspan, 4);
    assert!(t.render().contains("滋賀・京都　島根県東部　２日間　9,300　7,500"));
    assert!(t.render().contains("大阪・兵庫　島根県東部　３日間　8,500　6,900"));
}
