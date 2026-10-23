use crate::notice_boundaries::cuts;
fn input(content: &str) -> crate::PageInput {
    let xhtml=format!("<doc><page width='600' height='800'><word xmin='100' ymin='50' xmax='240' ymax='60' data-native-styles='1:4020000000000000 2:4020000000000000' data-native-content-styles='{content}'>任意（追記）公告</word><word xmin='100' ymin='70' xmax='240' ymax='80' data-native-styles='2:4020000000000000'>2026年7月1日</word><word xmin='100' ymin='90' xmax='240' ymax='100' data-native-styles='2:4020000000000000'>清算人任意</word></page></doc>");
    let mut rules = crate::lines::PageRules::default();
    rules.enclosures = vec![[90., 40., 250., 110.]];
    crate::PageInput {
        xhtml,
        rules: Some(rules),
    }
}
#[test]
fn content_typography_within_one_original_word_is_distinct_from_its_punctuation() {
    let p = input("1:4020000000000000");
    let a = crate::analyze_item(&[p.clone()]);
    assert_eq!(cuts(&a.pages[0], p.rules.as_ref().unwrap()), vec![0]);
}
#[test]
fn real_body_style_content_within_the_same_word_cannot_be_discarded() {
    let p = input("1:4020000000000000 2:4020000000000000");
    let a = crate::analyze_item(&[p.clone()]);
    assert!(cuts(&a.pages[0], p.rules.as_ref().unwrap()).is_empty());
}
#[test]
fn unknown_incomplete_or_foreign_content_styles_do_not_fall_back_to_all_styles() {
    for bad in [
        "unknown",
        "",
        "1",
        "3:4020000000000000",
        "1:0000000000000000",
        "1:7ff0000000000000",
        "1:7ff8000000000000",
        "1:c020000000000000",
    ] {
        let p = input(bad);
        let a = crate::analyze_item(&[p.clone()]);
        assert!(
            cuts(&a.pages[0], p.rules.as_ref().unwrap()).is_empty(),
            "{bad}"
        );
    }
}
#[test]
fn a_legacy_mixed_word_without_glyph_class_evidence_is_still_ambiguous() {
    let mut p = input("1:4020000000000000");
    p.xhtml = p
        .xhtml
        .replace("data-native-content-styles='1:4020000000000000'", "");
    let a = crate::analyze_item(&[p.clone()]);
    assert!(cuts(&a.pages[0], p.rules.as_ref().unwrap()).is_empty());
}
#[test]
fn an_arbitrary_font_bijection_preserves_all_evidence_channels() {
    let p = input("1:4020000000000000");
    let mut q = p.clone();
    q.xhtml = q
        .xhtml
        .replace("1:4020000000000000", "171:4020000000000000")
        .replace("2:4020000000000000", "202:4020000000000000");
    assert_eq!(
        serde_json::to_value(crate::analyze_item(&[p])).unwrap(),
        serde_json::to_value(crate::analyze_item(&[q])).unwrap()
    );
}
#[test]
fn conflicting_content_styles_for_indistinguishable_occurrences_abstain_in_both_orders() {
    let p = input("1:4020000000000000");
    let start = p.xhtml.find("<word").unwrap();
    let end = p.xhtml.find("</word>").unwrap() + 7;
    let first = &p.xhtml[start..end];
    let other = first.replace(
        "data-native-content-styles='1:4020000000000000'",
        "data-native-content-styles='2:4020000000000000'",
    );
    let mut analyses = vec![];
    for both in [format!("{first}{other}"), format!("{other}{first}")] {
        let mut q = p.clone();
        q.xhtml.replace_range(start..end, &both);
        let a = crate::analyze_item(&[q.clone()]);
        assert!(cuts(&a.pages[0], q.rules.as_ref().unwrap()).is_empty());
        analyses.push(serde_json::to_value(a).unwrap());
    }
    assert_eq!(analyses[0], analyses[1]);
}
#[test]
fn complete_symbol_only_titles_keep_their_original_typography() {
    let mut p = input("");
    p.xhtml = p.xhtml.replace("任意（追記）公告", "★").replace(
        "data-native-styles='1:4020000000000000 2:4020000000000000'",
        "data-native-styles='1:4020000000000000'",
    );
    let a = crate::analyze_item(&[p.clone()]);
    assert_eq!(cuts(&a.pages[0], p.rules.as_ref().unwrap()), vec![0]);
}
