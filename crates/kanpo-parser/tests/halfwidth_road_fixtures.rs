#[test]
fn halfwidth_decimal_marks_keep_both_road_states_and_their_measurements() {
    let pdf = include_bytes!("archive/pages/20260731h01759/20260731h017590006.pdf");
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let p = &a.pages[0];
    let t = p
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .find(|t| t.rows == 5 && t.cols == 6)
        .expect("54 road table has four measurement records");
    let cell = |r, c| t.cells.iter().find(|x| x.row == r && x.col == c).unwrap();
    for row in [1, 3] {
        assert_eq!(cell(row, 2).text, "Ａ");
        assert_eq!(cell(row, 3).text, "九･七七〜四二･三六");
        assert_eq!(cell(row, 4).text, "九・〇九七");
    }
    assert_eq!(cell(2, 2).text, "Ｂ･Ｃ");
    assert_eq!(cell(2, 3).text, "一二･六四〜一五七･二五");
    assert_eq!(cell(2, 4).text, "三・四一三");
    assert_eq!(cell(4, 2).text, "Ｂ･Ｃ･Ｄ");
    assert_eq!(cell(4, 3).text, "一二･六四〜一五七･二五");
    assert_eq!(cell(4, 4).text, "四・六五六");
    assert_eq!(cell(1, 0).rowspan, 4);
    assert_eq!(cell(1, 5).rowspan, 4);
    assert_eq!(cell(1, 1).text, "前");
    assert_eq!(cell(1, 1).rowspan, 2);
    assert_eq!(cell(3, 1).text, "後");
    assert_eq!(cell(3, 1).rowspan, 2);
    let mut pos = 0;
    for text in [
        "四国地方整備局告示第五十五号",
        "四国地方整備局告示第五十六号",
    ] {
        pos += a.text[pos..].find(text).unwrap() + text.len();
    }
    let neighbor = p
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .find(|t| t.rows == 3 && t.cols == 4)
        .expect("neighbor55 remains four columns");
    let ncell = |r, c| {
        neighbor
            .cells
            .iter()
            .find(|x| x.row == r && x.col == c)
            .unwrap()
    };
    assert!(ncell(1, 0).text.starts_with("東かがわ市伊座"));
    assert_eq!(ncell(1, 2).text, "二二・五二〜二九・三六");
    assert_eq!(ncell(2, 2).text, "二三・七〇〜二九・三六");
    assert_eq!(ncell(1, 3).text, "〇・〇〇五");
    assert_eq!(ncell(2, 3).text, "〇・〇〇五");
    let ids = t
        .cells
        .iter()
        .flat_map(|c| &c.words)
        .copied()
        .collect::<Vec<_>>();
    for id in &ids {
        assert_eq!(ids.iter().filter(|i| *i == id).count(), 1);
    }
    assert!(ids.iter().all(|&id| id < p.words.len()));
}
