fn analyze() -> kanpo_parser::Analysis {
    let pdf=include_bytes!("data/shared-publication-row/20260806g001770058.pdf");
    kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap())
}
fn position(a: &kanpo_parser::Analysis,id:usize)->usize {
    let ids:Vec<_>=a.pages[0].regions.iter().flat_map(|r|&r.lines).flat_map(|l|l.words.iter().chain(l.table.iter().flat_map(|t|t.cells.iter().flat_map(|c|&c.words)))).copied().collect();
    let at:Vec<_>=ids.iter().enumerate().filter(|(_,x)|**x==id).map(|(i,_)|i).collect();
    assert_eq!(at.len(),1,"each original header Word must occur once");at[0]
}
#[test]
fn all_three_original_publications_keep_their_own_articles() {
    let a=analyze();assert_eq!(a.articles.len(),3,"prior financial continuation, pension change, independent federation financial announcement");
    assert!(a.articles[1].text.starts_with("企業年金基金変更公告"));
    assert!(a.articles[2].text.starts_with("令和７年度の決算に関する公告"));
    assert!(!a.articles[1].text.contains("全国社会保険労務士会連合会"));
}
#[test]
fn source_publication_title_is_read_before_its_own_date() {
    let a=analyze();assert!(position(&a,271)<position(&a,303),"source title printed above date must precede date");
}
#[test]
fn statement_caption_follows_its_parent_publication_header() {
    let a=analyze();assert!(position(&a,153)<position(&a,304),"source parent issuer is above the subordinate statement caption");
}

#[test]
fn the_other_original_header_preserves_the_prior_report_and_the_new_financial_notice() {
    let a=kanpo_parser::analyze_item(&kanpo_parser::page_inputs(include_bytes!("data/source-financial-headers/20260731g001730090.pdf")).unwrap());
    let actual:serde_json::Value=serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    let expected:serde_json::Value=serde_json::from_str(include_str!("data/source-financial-headers/20260731g001730090-reviewed141.json")).unwrap();
    assert_eq!(actual,expected,"Complete regression contract, not independent full-page gold");
    assert_eq!(a.articles.len(),2);
    assert!(a.articles[1].text.starts_with("農水産業協同組合貯金保険機構令和７事業年度決算に関する公告"));
    assert!(!a.articles[0].text.contains("農水産業協同組合貯金保険機構令和７事業年度決算に関する公告"));
}
