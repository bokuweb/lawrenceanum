#[test]
fn complete_wide_record_precedes_following_notice() {
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(include_bytes!("archive/pages/20261014h01807/20261014h018070005.pdf")).unwrap());
    let t = &a.text;
    let next = t.find("〇国土交通省告示第千二百十八号").unwrap();
    for field in [
        "事業場の名称",
        "事業場の所在地",
        "認定に係る物件の範囲",
        "有効期間",
        "本田技研工業株式会社",
        "静岡県浜松市浜名区細江",
        "船外機（連続最大出力が",
        "令和13年10月４日まで",
        "に限る。）",
    ] {
        assert!(t.find(field).unwrap() < next, "ship field {field} must remain before the next notice");
    }
}
#[test]
fn preserves_every_following_notice_line_and_all_42_coordinates() {
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(include_bytes!("archive/pages/20261014h01807/20261014h018070005.pdf")).unwrap());
    let before: serde_json::Value = serde_json::from_str(include_str!("archive/expected-ship-order/20261014h018070005.json")).unwrap();
    let b = before["text"].as_str().unwrap();
    let heading = "〇国土交通省告示第千二百十八号";
    let mut suffix = b[b.find(heading).unwrap()..]
        .lines()
        .filter(|l| {
            !l.starts_with("認定に係る物件の範囲") && !l.starts_with("船外機（連続最大出力が") && !l.starts_with("92キロワット以下のもの") && *l != "に限る。）"
        })
        .collect::<Vec<_>>()
        .join("\n");
    suffix.push('\n');
    assert_eq!(
        a.text[a.text.find(heading).unwrap()..].trim_end(),
        suffix.trim_end(),
        "all article body, signature and coordinate rows must retain their original complete order"
    );
    let boundary = "事業場の名称";
    assert_eq!(
        &a.text[..a.text.find(boundary).unwrap()],
        &b[..b.find(boundary).unwrap()],
        "preceding articles unchanged"
    );
}
#[test]
fn preserves_mixed_negative_20260928h017960004() {
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(include_bytes!("archive/pages/20260928h01796/20260928h017960004.pdf")).unwrap());
    let before: serde_json::Value = serde_json::from_str(include_str!("archive/expected-ship-order/20260928h017960004.json")).unwrap();
    assert_eq!(a.text, before["text"].as_str().unwrap());
    let trace: serde_json::Value = serde_json::from_str(&serde_json::to_string(&a.pages[0]).unwrap()).unwrap();
    assert!(
        trace == before,
        "mixed-layout negative must retain the entire trace, source IDs and original ownership"
    );
}
#[test]
fn pesticide_records_preserve_all_source_words_and_the_complete_composed_trace() {
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(include_bytes!("archive/pages/20260824h01774/20260824h017740004.pdf")).unwrap());
    let before: serde_json::Value = serde_json::from_str(include_str!("archive/expected-ship-order/20260824h017740004.json")).unwrap();
    assert_eq!(a.text, before["text"].as_str().unwrap());
    let trace: serde_json::Value = serde_json::from_str(&serde_json::to_string(&a.pages[0]).unwrap()).unwrap();
    assert!(
        trace == before,
        "six complete pesticide tables must retain the exact composed trace and original source metadata"
    );
    let tables: Vec<_> = a.pages[0].regions.iter().flat_map(|r| &r.lines).filter_map(|l| l.table.as_ref()).collect();
    assert_eq!(tables.len(), 6);
    assert_eq!(tables.iter().map(|t| t.rows - 1).sum::<usize>(), 36);
    assert!(tables.iter().all(|t| t.cols == 4));
}

#[test]
fn ship_record_has_four_exact_cells_and_preserves_all_source_words() {
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(include_bytes!("archive/pages/20261014h01807/20261014h018070005.pdf")).unwrap());
    let p = &a.pages[0];
    let before: serde_json::Value = serde_json::from_str(include_str!("archive/expected-ship-order/20261014h018070005.json")).unwrap();
    let word_json: serde_json::Value = serde_json::from_str(&serde_json::to_string(&p.words).unwrap()).unwrap();
    assert!(
        word_json == before["words"],
        "all IDs, raw text, normalized text, statuses and bboxes remain exact"
    );
    let tables: Vec<_> = p.regions.iter().flat_map(|r| &r.lines).filter_map(|l| l.table.as_ref()).collect();
    let t = tables.iter().find(|t| t.cells.iter().any(|c| c.text == "事業場の名称")).unwrap();
    assert_eq!((t.rows, t.cols), (2, 4));
    for (col, value) in [
        "本田技研工業株式会社細江船外機工場",
        "静岡県浜松市浜名区細江町気賀5794番地１",
        "船外機（連続最大出力が92キロワット以下のものに限る。）",
        "令和８年10月５日から令和13年10月４日まで",
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(t.cells.iter().find(|c| c.row == 1 && c.col == col).unwrap().text, value);
    }
    let source_tables: Vec<_> = before["regions"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| r["lines"].as_array().unwrap())
        .filter_map(|l| l.get("table"))
        .collect();
    let actual: Vec<_> = tables
        .iter()
        .map(|t| serde_json::from_str::<serde_json::Value>(&serde_json::to_string(t).unwrap()).unwrap())
        .collect();
    assert!(source_tables.iter().all(|t| actual.contains(t)), "all existing physical tables remain exact");
    assert_eq!(tables.len(), source_tables.len() + 1);
    assert_eq!(a.articles.len(), 3);
    assert!(a.articles[1].text.contains("船外機（連続最大出力が92キロワット以下のものに限る。）"));
    assert!(a.articles[1].text.contains("令和８年10月５日から令和13年10月４日まで"));
    assert!(!a.articles[2].text.contains("船外機"));
}
