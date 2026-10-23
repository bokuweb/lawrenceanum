#[test]
fn before_after_road_sections_keep_codes_widths_lengths_and_merged_labels_together() {
    let pdf = include_bytes!("archive/pages/20260831h01779/20260831h017790008.pdf");
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let tables = a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .filter(|t| t.rows == 5 && t.cols == 6)
        .collect::<Vec<_>>();
    assert_eq!(tables.len(), 2, "two unruled before/after road tables");
    for (i, rows) in [
        [
            ("前", "Ａ", "九・七〇〜一〇八・八〇", "一八・二三一"),
            (
                "",
                "Ｂ・Ｃ・Ｄ・Ｅ・Ｆ・Ｇ",
                "一二・七〇〜三三三・七〇",
                "八・五四八",
            ),
            ("後", "Ａ", "九・七〇〜一〇八・八〇", "一八・二三一"),
            (
                "",
                "Ｂ・Ｃ・Ｄ・Ｅ・Ｆ・Ｇ・Ｈ",
                "一二・七〇〜三三三・七〇",
                "一一・二八八",
            ),
        ],
        [
            ("前", "Ａ", "八・七三〜一〇八・四〇", "八・三〇六"),
            ("", "Ｂ・Ｃ", "一二・六四〜二四六・四三", "三・六六〇"),
            ("後", "Ａ", "八・七三〜一〇八・四〇", "八・三〇六"),
            ("", "Ｂ・Ｃ・Ｄ", "一二・六四〜二四六・四三", "五・二九三"),
        ],
    ]
    .iter()
    .enumerate()
    {
        let t = tables[i];
        let cell = |r, c| t.cells.iter().find(|x| x.row == r && x.col == c);
        for (r, (change, code, width, length)) in rows.iter().enumerate() {
            let row = r + 1;
            if !change.is_empty() {
                let c = cell(row, 1).unwrap();
                assert_eq!(c.text, *change);
                assert_eq!(c.rowspan, 2);
            } else {
                assert!(cell(row, 1).is_none());
            }
            assert_eq!(cell(row, 2).unwrap().text, *code);
            assert_eq!(cell(row, 3).unwrap().text, *width);
            assert_eq!(cell(row, 4).unwrap().text, *length);
        }
        assert_eq!(cell(1, 0).unwrap().rowspan, 4);
        assert_eq!(cell(1, 5).unwrap().rowspan, 4);
        assert_eq!(cell(0, 1).unwrap().colspan, 2);
        assert!(cell(0, 3).unwrap().text.contains("敷地の幅員"));
        assert!(cell(0, 3).unwrap().text.contains("メートル"));
        assert!(cell(0, 4).unwrap().text.contains("キロメートル"));
    }
    let mut pos = 0;
    for s in [
        "第六十三号",
        "第六十四号",
        "第六十五号",
        "人事異動",
        "最高裁判所",
        "官庁報告",
    ] {
        pos += a.text[pos..].find(s).expect(s) + s.len();
    }
}
#[test]
fn a_two_record_road_table_without_section_codes_keeps_its_common_interval() {
    let pdf = include_bytes!("archive/pages/20261014h01807/20261014h018070006.pdf");
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let t = a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .find(|t| t.rows == 3 && t.cols == 4)
        .expect("before/after table without section code column");
    let cell = |r, c| t.cells.iter().find(|x| x.row == r && x.col == c).unwrap();
    assert_eq!(cell(0, 1).text, "変更前後別");
    assert_eq!(cell(0, 1).colspan, 1);
    assert_eq!(cell(1,0).text,"岩手県九戸郡洋野町種市第二四地割字横手三五番一から同町種市第二五地割字サンニヤ五三番一八まで");
    assert_eq!(cell(1, 0).rowspan, 2);
    assert_eq!(cell(1, 1).text, "前");
    assert_eq!(cell(2, 1).text, "後");
    assert_eq!(cell(1, 2).text, "六六・八六〜二六〇・〇五");
    assert_eq!(cell(2, 2).text, "六八・一九〜二六一・七〇");
    assert_eq!(cell(1, 3).text, "〇・一四六");
    assert_eq!(cell(2, 3).text, "〇・一四六");
    let mut pos = 0;
    for s in [
        "第二百五十一号",
        "第二百五十二号",
        "第二百五十三号",
        "第二百五十四号",
        "第二百五十五号",
        "東北地方整備局告示第百一号",
        "九州地方整備局告示第九十八号",
    ] {
        pos += a.text[pos..].find(s).expect(s) + s.len();
    }
}

#[test]
fn both_states_keep_a_code_width_native_word_fragment_once() {
    let pdf = include_bytes!("archive/pages/20260820h01772/20260820h017720007.pdf");
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let page = &a.pages[0];
    let t = page
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .find(|t| t.rows == 5 && t.cols == 6)
        .unwrap();
    for (row, id) in [(2, 54), (4, 83)] {
        let cell = |col| {
            t.cells
                .iter()
                .find(|c| c.row == row && c.col == col)
                .unwrap()
        };
        assert_eq!(cell(2).text, "Ｂ・Ｃ");
        assert_eq!(cell(3).text, "二三・〇〇〜一七九・五〇");
        assert_eq!(cell(4).text, "一九・三〇七");
        assert_eq!(page.words[id].raw, "Ｃ二三");
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
    assert_eq!(
        page.regions
            .iter()
            .flat_map(|r| &r.lines)
            .filter(|l| l.table.is_some())
            .count(),
        3
    );
}
