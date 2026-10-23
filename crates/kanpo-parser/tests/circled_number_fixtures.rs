// Nearby full text is an unchanged baseline preservation check, not a claim that every unrelated glyph is decoded.
use kanpo_parser::analyze_item;
fn analyze(page: &str) -> kanpo_parser::Analysis {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/archive/pages")
        .join(&page[..14])
        .join(format!("{page}.pdf"));
    analyze_item(&kanpo_parser::page_inputs(&std::fs::read(path).unwrap()).unwrap())
}
#[test]
fn native_circled_eleven_keeps_the_complete_range() {
    let a = analyze("20260827g001910011");
    assert!(a.text.contains("⑨から⑪までの事項"), "{}", a.text);
    assert!(a.text.contains("⑦から⑩までの事項"));
}
#[test]
fn overlaid_twenty_one_keeps_each_original_reference() {
    let a = analyze("20260827g001910011");
    assert_eq!(a.text.matches("６㉑に掲げる事項").count(), 2, "{}", a.text);
    assert_eq!(a.text.matches("５㉑に掲げる事項").count(), 1, "{}", a.text);
    assert!(!a.text.contains("６に掲げる事項"));
}
#[test]
fn nearby_normal_reference_ranges_remain_exact() {
    let a = analyze("20260827g001910011");
    assert!(a.text.contains("⑦から⑩までの事項"));
    assert!(a.text.contains("７①から③まで、⑦及び⑧"));
    for (page, expected) in [
        (
            "20260827g001910008",
            include_str!("expected/circled_numbers/20260827g001910008.txt"),
        ),
        (
            "20260827g001910010",
            include_str!("expected/circled_numbers/20260827g001910010.txt"),
        ),
    ] {
        assert_eq!(analyze(page).text, expected, "{page}")
    }
}
