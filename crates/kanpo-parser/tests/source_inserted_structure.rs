use kanpo_parser::{Analysis, Block, Run, ShinkyuRow};
fn flat(rs: &[Run]) -> String {
    rs.iter().map(|r| r.text.as_str()).collect()
}
fn rows(a: &Analysis) -> Vec<&ShinkyuRow> {
    a.articles
        .iter()
        .flat_map(|a| &a.document.blocks)
        .flat_map(|b| match b {
            Block::Shinkyu { rows ,..} => rows.as_slice(),
            _ => &[],
        })
        .collect()
}
fn analyze(pdfs: &[&[u8]]) -> Analysis {
    let mut pages = vec![];
    for p in pdfs {
        pages.extend(kanpo_parser::page_inputs(p).unwrap());
    }
    kanpo_parser::analyze_item(&pages)
}
fn owns(a: &Analysis, key: &str, caption: &str) {
    let rows = rows(a);
    let rs = rows
        .iter()
        .filter(|r| flat(&r.after).starts_with(&format!("{}\n{}　", caption, key)))
        .collect::<Vec<_>>();
    assert_eq!(rs.len(), 1, "independent source row for {key}");
    assert_eq!(flat(&rs[0].before), "（新設）");
}
macro_rules! original {
    ($name:ident,$stem:literal) => {
        #[test]
        fn $name() {
            let a = analyze(&[include_bytes!(concat!(
                "data/source-inserted-structure/",
                $stem,
                ".pdf"
            ))]);
            let actual: serde_json::Value =
                serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
            let expected: serde_json::Value = serde_json::from_str(include_str!(concat!(
                "data/source-inserted-structure/",
                $stem,
                "-reviewed146.json"
            )))
            .unwrap();
            assert_eq!(
                actual, expected,
                "nonblind reviewed native row contract: {}",
                $stem
            );
        }
    };
}
original!(original_20260724g001650782, "20260724g001650782");
original!(original_20260729g001690093, "20260729g001690093");
original!(original_20260729g001690094, "20260729g001690094");
original!(original_20260729g001690096, "20260729g001690096");
original!(original_20260729g001690098, "20260729g001690098");
original!(original_20260729g001690174, "20260729g001690174");
original!(original_20260729g001690175, "20260729g001690175");
original!(original_20260731g001720088, "20260731g001720088");
original!(original_20260731g001720089, "20260731g001720089");
original!(original_20260814h017680003, "20260814h017680003");
original!(original_20260918g002090061, "20260918g002090061");
original!(original_20260930g002150051, "20260930g002150051");
original!(original_20260930g002150090, "20260930g002150090");
original!(original_20260930g002150091, "20260930g002150091");
original!(original_20260930g002150092, "20260930g002150092");
original!(original_20260930g002150100, "20260930g002150100");
original!(original_20260930g002150153, "20260930g002150153");
original!(original_20260930g002150154, "20260930g002150154");
original!(original_20260930g002150155, "20260930g002150155");
original!(original_20260930g002150361, "20260930g002150361");
original!(original_20260930g002150398, "20260930g002150398");
const P92: &[u8] = include_bytes!("data/source-outline-partial-claims/20260729g001690092.pdf");
const P93: &[u8] = include_bytes!("data/source-inserted-structure/20260729g001690093.pdf");
const P94: &[u8] = include_bytes!("data/source-inserted-structure/20260729g001690094.pdf");
#[test]
fn inserted_articles_have_individual_native_markers() {
    let a = analyze(&[P93]);
    assert_eq!(rows(&a).len(), 3);
    owns(&a, "第五十一条の十一", "（欠格条項）");
    owns(&a, "第五十一条の十二", "（登録基準）");
    assert!(rows(&a)[0].before.is_empty());
}
#[test]
fn unrelated_invalid_page_does_not_poison_contiguous_native_units() {
    let a = analyze(&[
        include_bytes!("data/source-outline-columns/20260918g002090002.pdf"),
        P92,
        P93,
        P94,
    ]);
    assert_eq!(a.pages.len(), 4);
    owns(&a, "第五十一条の十一", "（欠格条項）");
    owns(&a, "第五十一条の十二", "（登録基準）");
    owns(&a, "第五十一条の十三", "（登録の更新）");
}
#[test]
fn complete_opaque_table_stays_with_the_continuing_article() {
    let a = analyze(&[P94]);
    let rs = rows(&a);
    assert_eq!(rs.len(), 2);
    assert!(rs[0].before.is_empty());
    assert!(rs[0].after.iter().any(|r| r.table.is_some()));
    assert!(rs[1].after.iter().all(|r| r.table.is_none()) && rs[1].after_table.is_none());
    owns(&a, "第五十一条の十三", "（登録の更新）");
}
#[test]
fn internal_outline_keeps_its_own_marker_and_all_adjacent_article_markers() {
    let a = analyze(&[include_bytes!(
        "data/source-inserted-structure/20260918g002090061.pdf"
    )]);
    let rs = rows(&a);
    assert_eq!(rs.len(), 5);
    assert_eq!(
        rs.iter()
            .filter(|r| flat(&r.after) == "第八節　海上運送利便確保事業")
            .count(),
        1
    );
    for r in rs {
        assert_eq!(flat(&r.before), "（新設）");
    }
}
