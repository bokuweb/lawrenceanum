//! 見出しが本文より長い二記録の表でも、最後の欄と署名を分離しない。

#[test]
fn two_type_changes_keep_the_fifth_column_and_follow_the_signature() {
    let pdf = include_bytes!("archive/pages/20260724h01754/20260724h017540006.pdf");
    let out = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let table = out.pages[0].regions.iter().flat_map(|r| &r.lines).filter_map(|l| l.table.as_ref())
        .find(|t| t.rows == 3 && t.cols == 5 && t.cells.iter().any(|c| c.text == "型式変更の内容")).unwrap();
    let value = |row, col| table.cells.iter().find(|c| c.row == row && c.col == col).unwrap().text.as_str();
    assert_eq!(value(1, 0), "第5433号");
    assert_eq!(value(1, 2), "ＴＫ－210Ｃ型");
    assert_eq!(value(1, 3), "高階救命器具株式会社");
    assert_eq!(value(1, 4), "布地の色の追加");
    assert_eq!(value(2, 0), "第5434号");
    assert_eq!(value(2, 2), "ＴＫ－210Ｙ型");
    assert_eq!(value(2, 4), "〃");
    assert!(out.text.find("国土交通大臣金子恭之").unwrap() < out.text.find("型式承認番号").unwrap());
    let mut ids = std::collections::HashSet::new();
    for id in table.cells.iter().flat_map(|c| &c.words) { assert!(ids.insert(id)); }
}
