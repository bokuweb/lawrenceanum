fn assert_source_ids(a: &kanpo_parser::Analysis) {
    for p in &a.pages {
        let mut ids = std::collections::HashSet::new();
        for l in p.regions.iter().flat_map(|r| &r.lines) {
            if let Some(t) = &l.table {
                assert_eq!(l.text, t.render());
            }
            for &id in l.words.iter().chain(l.table.iter().flat_map(|t| t.cells.iter().flat_map(|c| &c.words))) {
                assert!(ids.insert(id), "duplicate source word {id}");
            }
        }
        for w in p.words.iter().filter(|w| w.status == kanpo_parser::trace::WordStatus::Used) {
            assert!(ids.contains(&w.id), "missing source word {}", w.id);
        }
    }
}
#[test]
fn asset_records_continue_from_left_bottom_to_right_top() {
    let pdf = include_bytes!("archive/pages/20260930g00216/20260930g002160055.pdf");
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    assert_source_ids(&a);
    let pos = |s: &str| a.text.find(s).unwrap();
    assert!(pos("（２）　認められた減損の兆候の概要") < pos("４．不要財産に係る国庫納付関係"));
    assert!(pos("４．不要財産に係る国庫納付関係") < pos("医療研究開発革新基盤創成事業における不要金銭"));
    assert!(pos("医療研究開発革新基盤創成事業における不要金銭") < pos("本事業において、開発終了に伴い企業"));
    assert!(pos("本事業において、開発終了に伴い企業") < pos("固定資産の売却に係る収入金における不要金銭"));
    assert!(a.text.contains("62,562,500"));
    assert!(a.text.contains("142,596,224"));
}
#[test]
fn the_next_asset_page_finishes_its_left_column_before_financial_notes() {
    let pdf = include_bytes!("archive/pages/20260930g00216/20260930g002160056.pdf");
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    assert_source_ids(&a);
    let record = a.text.find("第２期中長期目標期間終了に伴う積立金").unwrap();
    let chapter = a.text.find("５．金融商品の時価等について").unwrap();
    assert!(record < chapter);
    let date = a.text.find("令和８年３月10日").unwrap();
    assert!(record < date && date < chapter);
    assert!(a.text.contains("17,044,447,084"));
    assert!(a.text.contains("14,374,633,174"));
}
