fn source_ids_unchanged_in_rendered_page(a: &kanpo_parser::Analysis) {
    for p in &a.pages {
        let mut ids = std::collections::HashSet::new();
        for l in p.regions.iter().flat_map(|r| &r.lines) {
            if let Some(t) = &l.table {
                assert_eq!(l.text, t.render());
            }
            for &id in l.words.iter().chain(l.table.iter().flat_map(|t| t.cells.iter().flat_map(|c| &c.words))) {
                assert!(ids.insert(id), "duplicate word {id}");
            }
        }
        for w in p.words.iter().filter(|w| w.status == kanpo_parser::trace::WordStatus::Used) {
            assert!(ids.contains(&w.id), "missing word {}", w.id);
        }
    }
}
fn analyzed(pdf: &[u8]) -> kanpo_parser::Analysis {
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    source_ids_unchanged_in_rendered_page(&a);
    a
}
#[test]
fn a_wide_address_corporation_and_president_precede_the_balance_sheet() {
    let a = analyzed(include_bytes!("archive/pages/20261016g00228/20261016g002280051.pdf"));
    assert!(a
        .text
        .contains("令和８年10月16日\n岩手県盛岡市上田三丁目18番８号\n国立大学法人岩手大学\n学長　山本　欣郎\n貸借対照表\n（令和８年３月31日現在）"));
    assert!(a.text.contains("土地　38,322,829,783"));
    assert!(a.text.contains("総合臨床検査システム　992,098,800　956,666,700"));
}
#[test]
fn floating_point_rounding_keeps_a_same_baseline_date_with_the_signer() {
    let a = analyzed(include_bytes!("archive/pages/20261016g00228/20261016g002280157.pdf"));
    assert!(a
        .text
        .contains("令和８年10月16日\n東京都文京区本郷７－３－１\n国立大学法人東京大学\n総長　藤井　輝夫\n貸借対照表"));
    assert!(a.text.contains("資産合計　1,521,677"));
}
#[test]
fn an_existing_correct_name_block_is_not_reordered_into_the_adjacent_financial_table() {
    let a = analyzed(include_bytes!("archive/pages/20261016g00228/20261016g002280372.pdf"));
    assert!(a
        .text
        .contains("令和８年10月16日\n滋賀県大津市瀬田月輪町\n国立大学法人滋賀医科大学長\n遠山　育夫\n貸借対照表"));
    assert!(a.text.contains("期末残高　9,394"));
}
