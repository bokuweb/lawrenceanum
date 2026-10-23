#[test]
fn a_whole_width_range_native_word_keeps_its_code_and_long_shared_interval() {
    let pdf = include_bytes!("archive/pages/20261001h01799/20261001h017990008.pdf");
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let page = &a.pages[0];
    let tables = page
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .filter(|t| t.rows == 5 && t.cols == 6)
        .collect::<Vec<_>>();
    assert_eq!(
        tables.len(),
        3,
        "all three before/after road tables, including70"
    );
    let t = tables
        .iter()
        .find(|t| {
            t.cells
                .iter()
                .any(|c| c.text.starts_with("北海道上川郡上川町"))
        })
        .unwrap();
    let cell = |r, c| t.cells.iter().find(|x| x.row == r && x.col == c).unwrap();
    assert_eq!(cell(1,0).text,"北海道上川郡上川町字清川三一三番一から同町字層雲峡国有林上川南部森林計画区上川中部森林管理署上川森林事務所二一五九林班イ小班まで");
    assert_eq!(cell(1, 0).rowspan, 4);
    for (row, code, width) in [
        (1, "Ａ", "三〇・七〇〜五〇・五〇"),
        (2, "Ｂ・Ｄ", "一五・六一〜八四・一二"),
        (3, "Ａ", "二三・〇〇〜五八・五〇"),
        (4, "Ｂ・Ｄ", "一五・六一〜八四・一二"),
    ] {
        assert_eq!(cell(row, 2).text, code);
        assert_eq!(cell(row, 3).text, width);
        assert_eq!(cell(row, 4).text, "二・五五〇");
    }
    assert_eq!(
        cell(1, 5).text,
        "上記Ａ、Ｂ及びＤは、関係図面に表示する敷地の区分をいう。"
    );
    assert_eq!(cell(1, 5).rowspan, 4);
    for id in [40, 59] {
        assert_eq!(page.words[id].raw, "Ｄ一五・六一〜八四・一二");
        assert_eq!(
            t.cells
                .iter()
                .flat_map(|c| &c.words)
                .filter(|&&i| i == id)
                .count(),
            1
        );
    }
    assert!(t
        .cells
        .iter()
        .flat_map(|c| &c.words)
        .all(|&id| id < page.words.len()));
    let mut pos = 0;
    for text in [
        "北海道開発局告示第六十九号",
        "北海道開発局告示第七十号",
        "北海道開発局告示第七十一号",
        "沖縄総合事務局告示第二十三号",
    ] {
        pos += a.text[pos..].find(text).unwrap() + text.len();
    }
}
