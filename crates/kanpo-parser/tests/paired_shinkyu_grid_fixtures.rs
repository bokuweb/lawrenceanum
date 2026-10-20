//! 段切りで旧欄の備考が離れた植物表を、署名とともに元の新旧帯へ戻す。
#[test]
fn paired_ruled_tables_preserve_all_three_columns_and_the_preamble_signature() {
    let pdf = include_bytes!("archive/pages/20260731g00172/20260731g001720083.pdf");
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    assert!(
        a.text
            .contains("令和八年七月三十一日　農林水産大臣鈴木憲和\n種苗法施行規則の一部を改正する省令"),
        "{}",
        a.text
    );
    let tables: Vec<_> = a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .collect();
    let before = tables.iter().find(|t| t.cells.iter().any(|c| c.text == "（新設）")).unwrap();
    assert_eq!((before.rows, before.cols), (12, 3));
    let expected = [
        "区分　農林水産植物　備考",
        "（略）　（略）　（略）",
        "カキツバタ　Iris laevigata Fisch.　草花",
        "（新設）　（新設）　（新設）",
        "イソトマ　アキシラリス　Isotoma axillaris Lindl.　草花",
        "（略）　（略）　（略）",
        "パッションフルーツ　Passiflora edulis Sims　果樹",
        "（新設）　（新設）　（新設）",
        "サギソウ　Pecteilis radiata(Spreng.) Raf.　草花",
        "（略）　（略）　（略）",
        "コケモモ　Vaccinium vitis-idaea L.　観賞樹",
        "（新設）　（新設）　（新設）",
    ];
    assert_eq!(before.render(), expected.join("\n"));
    let after = tables.iter().find(|t| t.cells.iter().any(|c| c.text.contains("Isatis"))).unwrap();
    assert_eq!((after.rows, after.cols), (12, 3));
    assert!(after.render().contains("ホソバタイセイ　Isatis tinctoria L.　工芸作物"));
    assert!(after.render().contains("カノコソウ　Valeriana fauriei Briq.　工芸作物"));
    let mut ids = std::collections::HashSet::new();
    for id in a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .flat_map(|l| l.words.iter().chain(l.table.iter().flat_map(|t| t.cells.iter().flat_map(|c| &c.words))))
    {
        assert!(ids.insert(*id), "duplicate {id}");
    }
}
