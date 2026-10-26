use kanpo_parser::{Analysis, Block, Run, ShinkyuRow};
const PDF: &[u8] = include_bytes!("data/source-outline-columns/20260918g002090059.pdf");
const HEADER: &str = "第七節　自動車地域旅客運送サービス再構築事業";
fn analyze() -> Analysis {
    kanpo_parser::analyze_item(&kanpo_parser::page_inputs(PDF).unwrap())
}
fn flat(runs: &[Run]) -> String {
    runs.iter().map(|r| r.text.as_str()).collect()
}
fn rows(a: &Analysis) -> Vec<&ShinkyuRow> {
    a.articles
        .iter()
        .flat_map(|a| &a.document.blocks)
        .flat_map(|b| match b {
            Block::Shinkyu { rows } => rows.as_slice(),
            _ => &[],
        })
        .collect()
}
#[test]
fn original_outline_keeps_its_own_comparison_row_and_source_marker() {
    let a = analyze();
    let found: Vec<_> = rows(&a)
        .into_iter()
        .filter(|r| flat(&r.after).starts_with(HEADER))
        .collect();
    assert_eq!(found.len(), 1);
    assert_eq!(flat(&found[0].after), HEADER);
    assert_eq!(flat(&found[0].before), "（新設）");
}
#[test]
fn original_previous_article_does_not_claim_the_outlines_marker() {
    let a = analyze();
    let row = rows(&a)
        .into_iter()
        .find(|r| flat(&r.after).contains("第三十二条　"))
        .unwrap();
    assert!(!flat(&row.before).contains("（新設）"));
}
#[test]
fn original_outline_and_its_first_article_are_separate_structural_rows() {
    let a = analyze();
    let rs = rows(&a);
    assert_eq!(rs.len(), 7);
    let first = rs
        .iter()
        .find(|r| flat(&r.after).contains("第三十二条の二　"))
        .unwrap();
    assert!(!flat(&first.after).contains(HEADER));
    assert_eq!(flat(&first.before), "（新設）");
}
#[test]
fn every_used_word_keeps_one_original_page_line_owner() {
    let a = analyze();
    let page = &a.pages[0];
    let mut ids = std::collections::HashSet::new();
    for id in page
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .flat_map(|l| &l.words)
    {
        assert!(ids.insert(*id), "source Word {id} must occur once");
    }
    let used: std::collections::HashSet<_> = page
        .words
        .iter()
        .filter(|w| w.status == kanpo_parser::trace::WordStatus::Used)
        .map(|w| w.id)
        .collect();
    assert_eq!(page.words.len(), 107);
    assert_eq!(ids, used);
    assert_eq!(ids.len(), 93);
}
#[test]
fn unrelated_page_cannot_disable_a_complete_source_owned_unit() {
    // These non-contiguous originals reproduce the global source-proof failure.
    // The target unit must still bind its own complete source and Article scope.
    let mut inputs = kanpo_parser::page_inputs(include_bytes!(
        "data/source-outline-columns/20260918g002090002.pdf"
    ))
    .unwrap();
    inputs.extend(kanpo_parser::page_inputs(PDF).unwrap());
    let a = kanpo_parser::analyze_item(&inputs);
    assert_eq!(a.pages.len(), 2);
    assert_eq!(
        rows(&a)
            .iter()
            .filter(|r| flat(&r.after) == HEADER && flat(&r.before) == "（新設）")
            .count(),
        1
    );
    let single = analyze();
    // Layout classification may use preceding-page context. Every source Word,
    // native region/line assignment and physical-page text must stay identical.
    assert_eq!(
        serde_json::to_string(&a.pages[1].words).unwrap(),
        serde_json::to_string(&single.pages[0].words).unwrap()
    );
    assert_eq!(
        serde_json::to_string(&a.pages[1].regions).unwrap(),
        serde_json::to_string(&single.pages[0].regions).unwrap()
    );
    assert_eq!(a.pages[1].text, single.pages[0].text);
}
#[test]
fn original_complete_analysis_keeps_the_reviewed_source_contract() {
    // Nonblind seven-row source correspondence review; not independent gold.
    let actual: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&analyze()).unwrap()).unwrap();
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "data/source-outline-columns/20260918g002090059-reviewed145.json"
    ))
    .unwrap();
    assert_eq!(actual, expected);
}
