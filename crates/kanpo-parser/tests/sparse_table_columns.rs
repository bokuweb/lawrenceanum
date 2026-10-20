//! 疎な財務表でも、列見出しと金額の論理上の対応を出力に残す。

#[test]
fn net_asset_changes_keep_empty_leading_and_internal_columns() {
    let pdf = include_bytes!("archive/pages/20261016g00228/20261016g002280031.pdf");
    let pages = kanpo_parser::page_inputs(pdf).unwrap();
    let analysis = kanpo_parser::analyze_item(&pages);
    let text = &analysis.text;
    let profit = text.lines().find(|line| line.contains("585,995")).unwrap();
    assert_eq!(
        profit.split('　').collect::<Vec<_>>(),
        vec!["", "", "", "585,995", "585,995", "585,995", "585,995"],
        "{text}"
    );
    let transfer = text.lines().find(|line| line.contains("299,194")).unwrap();
    assert_eq!(transfer.split('　').count(), 7, "{text}");
    assert!(text.contains("キャッシュ・フロー計算書"));
    assert!(text.contains("△3,533,906"));
}
