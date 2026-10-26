use kanpo_parser::{Analysis, Block, Run, ShinkyuRow};
const PDF: &[u8] = include_bytes!("data/source-outline-partial-claims/20260729g001690092.pdf");
const HEADER: &str = "第五節　登録サンプリング補助者講習機関";
fn flat(rs: &[Run]) -> String {
    rs.iter().map(|r| r.text.as_str()).collect()
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
fn analyze() -> Analysis {
    kanpo_parser::analyze_item(&kanpo_parser::page_inputs(PDF).unwrap())
}
fn owns_header(a: &Analysis) {
    assert_eq!(
        rows(a)
            .iter()
            .filter(|r| flat(&r.after) == HEADER && flat(&r.before) == "（新設）")
            .count(),
        1
    );
    let previous = rows(a)
        .into_iter()
        .find(|r| flat(&r.after).contains("第四十五条の二　"))
        .unwrap();
    assert!(!flat(&previous.before).contains("（新設）"));
}
#[test]
fn original_outline_owns_its_marker_without_changing_its_neighbor() {
    let a = analyze();
    assert_eq!(rows(&a).len(), 6);
    owns_header(&a);
}
#[test]
fn plain_page_claims_are_partial_until_the_original_unit_is_certified() {
    let mut inputs = kanpo_parser::page_inputs(include_bytes!(
        "data/source-outline-partial-claims/plain-preface.pdf"
    ))
    .unwrap();
    inputs.extend(kanpo_parser::page_inputs(PDF).unwrap());
    let a = kanpo_parser::analyze_item(&inputs);
    assert_eq!(a.pages.len(), 2);
    assert_eq!(a.pages[0].text, "Source preface.");
    owns_header(&a);
    let single = analyze();
    assert_eq!(
        serde_json::to_string(&a.pages[1].words).unwrap(),
        serde_json::to_string(&single.pages[0].words).unwrap()
    );
    assert_eq!(
        serde_json::to_string(&a.pages[1].regions).unwrap(),
        serde_json::to_string(&single.pages[0].regions).unwrap()
    );
}
#[test]
fn complete_original_keeps_the_reviewed_opaque_table_and_word_contract() {
    // Only the native row association was corrected. This nonblind reference
    // retains every pre-existing native Word, opaque table, and other row.
    let actual: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&analyze()).unwrap()).unwrap();
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "data/source-outline-partial-claims/20260729g001690092-reviewed145.json"
    ))
    .unwrap();
    assert_eq!(actual, expected);
}
