//! include_bytes の4原本は tests/archive/pages/{issue}/{page}.pdf へ置く。
fn check(pdf:&[u8],expected:usize,excerpt:&str) {
    let a=kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());let p=&a.pages[0];
    assert!(a.text.contains(excerpt),"{}",a.text);
    let restored:Vec<_>=p.words.iter().filter(|w|w.raw=="\u{E61C}"&&w.text=="－").collect();assert_eq!(restored.len(),expected);
    let mut seen=std::collections::HashSet::new();
    for l in p.regions.iter().flat_map(|r|&r.lines) {
        if let Some(t)=&l.table {assert_eq!(l.text,t.render());}
        for &id in l.words.iter().chain(l.table.iter().flat_map(|t|t.cells.iter().flat_map(|c|&c.words))) {assert!(seen.insert(id),"重複した語 {id}");}
    }
    for w in restored {assert_eq!(w.status,kanpo_parser::trace::WordStatus::Used);assert!(seen.contains(&w.id));}
}
#[test]fn continuation_first_column_and_leading_bar_only_slots(){check(include_bytes!("archive/pages/20261016g00228/20261016g002280370.pdf"),29,"－　－　－　－　－　－　−432,811\n－　－　－　－　－　－　8");}
#[test]fn merged_header_preserves_nine_column_all_bar_rows(){check(include_bytes!("archive/pages/20261016g00228/20261016g002280015.pdf"),170,"－　－　－　－　－　－　－　－　－\n－　－　－　－　－　－　－　－　－");}
#[test]fn long_total_strip_uses_original_logical_row_positions(){check(include_bytes!("archive/pages/20261016g00228/20261016g002280678.pdf"),19,"固定資産の減損　　　　　　　　　　　－");}
#[test]fn two_column_profit_tables_keep_their_missing_amount_rows(){check(include_bytes!("archive/pages/20261001g00217/20261001g002170155.pdf"),3,"営業外費用　－");}
