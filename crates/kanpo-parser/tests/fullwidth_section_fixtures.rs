#[test]
fn finish_both_columns_before_the_spanning_section_heading_and_table() {
    let pdf = include_bytes!("archive/pages/20260730g00171/20260730g001710130.pdf");
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    assert_eq!(a.articles.len(), 1);
    assert_eq!(a.articles[0].text, a.text);
    let ordered = [
        "Ⅳ",
        "※１",
        "※２",
        "※３",
        "※４",
        "※５",
        "※６",
        "※７",
        "Ⅴ",
        "Ⅵ",
        "１　職員宿舎",
        "名称　用途　種類　場所　面積",
        "当該土地は",
        "２　電話加入権",
        "使用中の回線は",
    ];
    let mut pos = 0;
    for token in ordered {
        let n = a.text[pos..]
            .find(token)
            .unwrap_or_else(|| panic!("missing/out of order {token} after byte {pos}\n{}", a.text));
        pos += n + token.len();
    }
    let tables = a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .collect::<Vec<_>>();
    assert_eq!(tables.len(), 3);
    let dorm = tables.iter().find(|t| t.cols == 8).unwrap();
    assert_eq!((dorm.rows, dorm.cols), (2, 8));
    for (col, want) in [
        (0, "都内宿舎（一部）"),
        (1, "住宅用のもの"),
        (2, "土地"),
        (3, "東京都世田谷区上用賀外"),
        (4, "21,140.60"),
        (5, "7,513,212,000円"),
        (6, "－"),
        (7, "7,513,212,000円"),
    ] {
        assert_eq!(
            dorm.cells
                .iter()
                .find(|c| c.row == 1 && c.col == col)
                .unwrap()
                .text,
            want
        );
    }
    let mut ids = std::collections::HashSet::new();
    for id in a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .flat_map(|l| {
            l.words.iter().chain(
                l.table
                    .iter()
                    .flat_map(|t| t.cells.iter().flat_map(|c| &c.words)),
            )
        })
    {
        assert!(ids.insert(*id));
    }
}

#[test]
fn ordinary_financial_columns_keep_profit_before_disposal_and_notes() {
    let pdf = include_bytes!("archive/pages/20261008g00222/20261008g002220074.pdf");
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let ordered = [
        "損益計算書",
        "業務費　73,177",
        "経常費用合計　74,978",
        "経常収益合計　75,864",
        "当期総利益　540",
        "利益の処分に関する書類",
        "積立金　540",
        "注記事項",
        "10．消費税等の会計処理",
    ];
    let mut pos = 0;
    for t in ordered {
        pos += a.text[pos..].find(t).expect(t) + t.len();
    }
}
#[test]
fn the_next_financial_article_stays_after_the_previous_revenue_notes() {
    let pdf = include_bytes!("archive/pages/20260831g00194/20260831g001940064.pdf");
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let ordered = [
        "（収益認識）",
        "受託契約に係る収益　1,423　1,406　381　1,326　4,536　－　4,536",
        "（３）　当該事業年度",
        "独立行政法人国際観光振興機構令和７事業年度財務諸表に関す",
        "令和８年８月31日",
        "貸借対照表",
        "現金及び預金　3,679,174,849　5,894,205,030　26,108,948　9,599,488,827",
        "無形固定資産合計　138,980,883　111,343,738　　250,324,621",
    ];
    let mut pos = 0;
    for t in ordered {
        pos += a.text[pos..].find(t).expect(t) + t.len();
    }
}
