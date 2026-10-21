use super::*;
fn val<T: serde::Serialize>(v: &T) -> serde_json::Value {
    serde_json::from_str(&serde_json::to_string(v).unwrap()).unwrap()
}
fn positive(pdf: &[u8], want: &str) {
    let old = extract_pages(pdf).unwrap();
    let ext = extract_pages_with_aj1_sources(pdf).unwrap();
    let sources = ext;
    let pages: Vec<_> = sources.iter().map(|s| s.page().clone()).collect();
    assert_eq!(
        val(&old),
        val(&pages),
        "all public native fields must stay exact"
    );
    assert_eq!(bbox::bbox_layout(&old), bbox::bbox_layout(&pages));
    let html = bbox::bbox_layout_with_aj1_sources(&sources);
    assert!(
        html.contains(&format!("data-source-aj1-text=\"{want}\"")),
        "actual source CFF standard CID should supply the candidate"
    );
}
#[test]
fn actual_20260817g001830048_standard_source_candidate() {
    positive(
        include_bytes!(
            "../../kanpo-parser/tests/archive/pages/20260817g00183/20260817g001830048.pdf"
        ),
        "瀀",
    )
}
#[test]
fn actual_20260818g001840038_standard_source_candidate() {
    positive(
        include_bytes!(
            "../../kanpo-parser/tests/archive/pages/20260818g00184/20260818g001840038.pdf"
        ),
        "嬴",
    )
}
#[test]
fn actual_20260819g001850040_standard_source_candidate() {
    positive(
        include_bytes!(
            "../../kanpo-parser/tests/archive/pages/20260819g00185/20260819g001850040.pdf"
        ),
        "釬",
    )
}
#[test]
fn true_extended_chi_ToUnicode_stays_without_AJ1_candidate() {
    let pdf = include_bytes!(
        "../../kanpo-parser/tests/archive/pages/20260723g00163/20260723g001630002.pdf"
    );
    let old = extract_pages(pdf).unwrap();
    let ext = extract_pages_with_aj1_sources(pdf).unwrap();
    let s = ext;
    let p: Vec<_> = s.iter().map(|s| s.page().clone()).collect();
    assert_eq!(val(&old), val(&p));
    assert!(!bbox::bbox_layout_with_aj1_sources(&s).contains("data-source-aj1-text=\"（チ）\""));
}

#[test]
fn malformed_FDSelect_actual_PDF_has_no_source_authority() {
    let source = extract_pages_with_aj1_sources(include_bytes!(
        "source-structure-fixtures/bad-fd-index.pdf"
    ))
    .unwrap();
    assert_eq!(source[0].page().glyphs[0].cid, 17518);
    assert!(!bbox::bbox_layout_with_aj1_sources(&source).contains("data-source-aj1-text"));
}
#[test]
fn owned_bundle_cannot_render_a_foreign_Page_or_mutated_readonly_clone() {
    let good =
        extract_pages_with_aj1_sources(include_bytes!("source-structure-fixtures/standard.pdf"))
            .unwrap();
    let foreign =
        extract_pages_with_aj1_sources(include_bytes!("source-structure-fixtures/foreign-ros.pdf"))
            .unwrap();
    assert_eq!(val(good[0].page()), val(foreign[0].page()));
    assert!(bbox::bbox_layout_with_aj1_sources(&good).contains("data-source-aj1-text=\"嬴\""));
    assert!(!bbox::bbox_layout_with_aj1_sources(&foreign).contains("data-source-aj1-text"));
    let before = bbox::bbox_layout_with_aj1_sources(&good);
    let mut clone = good[0].page().clone();
    clone.glyphs[0].font = 999;
    clone.glyphs[0].text = "other".into();
    assert_eq!(bbox::bbox_layout_with_aj1_sources(&good), before);
    assert!(!bbox::bbox_layout(&[clone]).contains("data-source-aj1-text"));
}
