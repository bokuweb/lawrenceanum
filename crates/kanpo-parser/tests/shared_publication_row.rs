//! Public-output regressions for independently signed publication boundaries.
//! The frozen JSON preserves surrounding output; it is not a full-page gold.
//! In particular, the pension example still has a later, separate publication
//! merged into it. That known residual is recorded in source-reference140.json.
fn check(pdf: &[u8], expected: &str, title: &str) {
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let actual: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    let reference: serde_json::Value = serde_json::from_str(expected).unwrap();
    assert_eq!(actual, reference);
    let next = &a.articles[1];
    assert!(next.text.starts_with(title));
    assert!(!a.articles[0].text.contains(title));
    assert_eq!(a.pages.len(), 1);
}
macro_rules! source {
    ($name:ident, $page:literal, $title:literal) => {
        #[test]
        fn $name() {
            check(
                include_bytes!(concat!("data/shared-publication-row/", $page, ".pdf")),
                include_str!(concat!(
                    "data/shared-publication-row/", $page, "-reviewed140.json"
                )),
                $title,
            );
        }
    };
}
source!(research_financial_publication, "20260803g001740089", "国立研究開発法人農業・食品産業技術総合研究機構令和７年度\n財務諸表に関する公告");
source!(pension_boundary_preserves_known_later_residual, "20260806g001770058", "企業年金基金変更公告");
source!(housing_financial_publication, "20260825g001890072", "令和７事業年度独立行政法人住宅金融支援機構の財務諸表に関\nする公告");
source!(trade_financial_publication, "20260831g001940057", "独立行政法人日本貿易振興機構令和７事業年度財務諸表に関す\nる公告");
source!(fisheries_financial_publication, "20260930g002160037", "国立研究開発法人水産研究・教育機構令和７事業年度財務諸表\nに関する公告");
source!(museum_financial_publication, "20261016g002270058", "独立行政法人国立美術館令和７事業年度財務諸表に関する公告");
source!(university_financial_publication, "20261016g002280645", "国立大学法人奈良先端科学技術大学院大学令和７事業年度財務\n諸表に関する公告");
