//! 実官報の縦組み名簿で、所在地の番地だけが次の列に折り返す場合の回帰テスト。

const PAGE: &str = include_str!("corpus/2026-09-01-gougai195-p16/page.bbox.html");

#[test]
fn address_number_fragments_stay_in_their_records() {
    let pdf = include_bytes!("corpus/2026-09-01-gougai195-p16/page.pdf");
    let pages = kanpo_parser::page_inputs(pdf).unwrap();
    let out = kanpo_parser::analyze_item(&pages).text;
    // 原本ではそれぞれ同じ所在地欄の第 2 列。名称が 4 字の行も含む。
    for address in [
        "茨城県稲敷郡阿見町中郷二－七－一二",
        "埼玉県富士見市東みずほ台二－一四－四",
        "東京都三鷹市下連雀三－三四－一三",
    ] {
        assert!(out.contains(address), "所在地 {address} の折り返し: {out}");
    }
    for orphan in ["七－一二", "一四－四", "一三"] {
        assert!(!out.lines().any(|line| line == orphan), "番地 {orphan} が独立した: {out}");
    }
}

#[test]
fn ditto_is_not_an_address_number_fragment() {
    // 数字に見える位置でも、同上記号を 1 字断片として前の所在地に足さない。
    // 任意のフォント属性が追加されても、対象語は座標と元の本文で一意に選ぶ。
    let bounds = "<word xMin=\"480.256100\" yMin=\"228.839900\" xMax=\"488.256100\" yMax=\"244.839900\"";
    let matching: Vec<_> = PAGE.lines().filter(|line| line.contains(bounds) && line.contains(">一四</word>")).collect();
    assert_eq!(matching.len(), 1);
    let numeric_word = matching[0];
    let page = PAGE.replace(numeric_word, &numeric_word.replace(">一四</word>", ">〃</word>"));
    let out = kanpo_parser::reconstruct_vertical(&page);
    assert!(out.contains("〃"), "同上記号が失われた: {out}");
    assert!(!out.contains("埼玉県富士見市東みずほ台二－〃"), "同上記号を番地として連結した: {out}");
}
