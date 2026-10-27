use super::*;
fn tag(cid: u32) -> String {
    char::from_u32(kanpo_pdf::bbox::IDENTITY_CID_BASE + cid)
        .unwrap()
        .to_string()
}
fn word(raw: &str, range: (usize, usize), extra: &str, x: i32) -> String {
    format!("<word xmin='{x}' ymin='100' xmax='{}' ymax='108' data-wmode='0' data-native-axis='right' data-native-flow-first-glyph='{}' data-native-flow-after-glyph='{}' {extra}>{raw}</word>",x+16,range.0,range.1)
}
fn html(target: &str, source: &str, attribute: &str) -> String {
    format!(
        "<page width='800' height='600' data-native-glyph-count='20'>{}{}{}</page>",
        word(target, (10, 12), attribute, 200),
        word(source, (2, 3), "", 300),
        ""
    )
}
fn candidates(html: &str) -> Vec<Option<String>> {
    let h = scraper::Html::parse_document(html);
    let p = h
        .select(&scraper::Selector::parse("page").unwrap())
        .next()
        .unwrap();
    let a = for_page(p);
    p.select(&scraper::Selector::parse("word").unwrap())
        .map(|w| a.for_word(w))
        .collect()
}
const ATTR: &str = "data-native-font-aliases='1 10 12 200 100 216 108 0:58495:2:23185'";
#[test]
fn alias157_consumer_reuses_only_the_existing_verified_identity_scalar() {
    assert_eq!(
        candidates(&html("\u{e47f}口", &tag(23185), ATTR)),
        [Some("樋口".into()), None]
    );
}
#[test]
fn alias157_consumer_legacy_without_relation_keeps_its_original_route() {
    assert_eq!(
        candidates(&html("\u{e47f}口", &tag(23185), "")),
        [None, None]
    );
}
#[test]
fn alias157_consumer_unknown_identity_cid_cannot_use_cid_as_unicode() {
    let x = html(
        "\u{e47f}口",
        &tag(0x661d),
        "data-native-font-aliases='1 10 12 200 100 216 108 0:58495:2:26141'",
    );
    assert!(candidates(&x).iter().all(Option::is_none));
}
#[test]
fn alias157_consumer_existing_unicode_and_known_pua_keep_priority() {
    for r in ["普通", "\u{e61c}口", "\u{ea75}口", "\u{e9f5}口", "樋口"] {
        assert!(candidates(&html(r, &tag(23185), ATTR))
            .iter()
            .all(Option::is_none));
    }
}
#[test]
fn alias157_consumer_rejects_stale_wrong_and_partial_relations() {
    for a in [
        "1 9 12 200 100 216 108 0:58495:2:23185",
        "1 10 13 200 100 216 108 0:58495:2:23185",
        "1 10 12 200 100 216 108 0:58495:3:23185",
        "1 10 12 200 100 216 108 0:58495:2:23186",
        "1 10 12 200 100 216 108 2:58495:2:23185",
        "2 10 12 200 100 216 108 0:58495:2:23185",
        "1 10 12 200 100 216 108 0:58495:2:23185 0:58495:2:23185",
        "1 10 12 200 100 216 108 1:58495:2:23185",
        "1 10 12",
        "1 10 12 200 100 216 108 0:58495:2:23185 garbage",
    ] {
        assert!(
            candidates(&html(
                "\u{e47f}口",
                &tag(23185),
                &format!("data-native-font-aliases='{a}'")
            ))
            .iter()
            .all(Option::is_none),
            "{a}"
        );
    }
}
#[test]
fn alias157_consumer_requires_every_unknown_pua_in_the_word() {
    assert!(candidates(&html("\u{e47f}\u{e480}", &tag(23185), ATTR))
        .iter()
        .all(Option::is_none));
}
#[test]
fn alias157_consumer_requires_unique_original_glyph_ownership() {
    let x = html("\u{e47f}口", &tag(23185), ATTR);
    for other in [
        word("別", (2, 3), "", 400),
        word("別", (11, 12), "", 400),
        word("別", (1, 4), "", 400),
        word("別", (9, 13), "", 400),
    ] {
        assert!(
            candidates(&x.replace("</page>", &format!("{other}</page>")))
                .iter()
                .all(Option::is_none)
        );
    }
}
#[test]
fn alias157_consumer_malformed_page_range_geometry_or_cardinality_declines() {
    let x = html("\u{e47f}口", &tag(23185), ATTR);
    for (old, new) in [
        ("data-native-glyph-count='20'", ""),
        (
            "data-native-glyph-count='20'",
            "data-native-glyph-count='11'",
        ),
        ("xmin='200'", "xmin='NaN'"),
        ("xmax='216'", "xmax='200'"),
        (
            "data-native-flow-first-glyph='2'",
            "data-native-flow-first-glyph='1'",
        ),
        ("data-wmode='0'", "data-wmode='unknown'"),
    ] {
        assert!(
            candidates(&x.replace(old, new)).iter().all(Option::is_none),
            "{old}"
        );
    }
}
#[test]
fn alias157_consumer_actual_source_must_be_tagged_not_a_proposed_text() {
    for r in ["樋", "媑", "\u{e47f}", &(tag(23185) + "口")] {
        assert!(candidates(&html("\u{e47f}口", r, ATTR))
            .iter()
            .all(Option::is_none));
    }
}

#[test]
fn alias157_consumer_disjoint_touching_claims_and_word_order_do_not_change_the_relation() {
    let x = html("\u{e47f}口", &tag(23185), ATTR);
    let extra = word("任意", (12, 14), "", 400);
    assert_eq!(
        candidates(&x.replace("</page>", &format!("{extra}</page>")))[0],
        Some("樋口".into())
    );
    let target = word("\u{e47f}口", (10, 12), ATTR, 200);
    let source = word(&tag(23185), (2, 3), "", 300);
    assert_eq!(
        candidates(&format!(
            "<page data-native-glyph-count='20'>{source}{target}</page>"
        )),
        [None, Some("樋口".into())]
    );
}
#[test]
fn alias157_consumer_duplicate_source_boxes_and_out_of_page_overlap_cannot_claim_an_alias() {
    let x = html("\u{e47f}口", &tag(23185), ATTR);
    for duplicate in [
        word(&tag(23185), (4, 5), "", 300),
        word("別", (1, 100), "", 400),
    ] {
        assert!(
            candidates(&x.replace("</page>", &format!("{duplicate}</page>")))
                .iter()
                .all(Option::is_none)
        );
    }
}
fn original_xml() -> String {
    let b = std::fs::read(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/data/native-font-aliases/20260917g002080090.pdf"),
    )
    .unwrap();
    kanpo_pdf::bbox::bbox_layout_with_native_styles(
        &kanpo_pdf::extract_pages_with_aj1_sources(&b).unwrap(),
    )
}
fn without_aliases(mut xml: String) -> String {
    while let Some(start) = xml.find(" data-native-font-aliases=\"") {
        let end = start + " data-native-font-aliases=\"".len();
        let after = end + xml[end..].find('"').unwrap() + 1;
        xml.replace_range(start..after, "");
    }
    xml
}
#[test]
fn alias157_original_native_page_recovers_both_printed_occurrences_and_preserves_source_words() {
    let b = std::fs::read(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/data/native-font-aliases/20260917g002080090.pdf"),
    )
    .unwrap();
    let mut input = crate::page_inputs(&b).unwrap();
    let a = crate::analyze_item(&input);
    input[0].xhtml = without_aliases(input[0].xhtml.clone());
    let old = crate::analyze_item(&input);
    assert_eq!(a.text.matches("樋口").count(), 2);
    assert_eq!(old.text.matches("樋口").count(), 1);
    let mut changes = 0;
    for (n, o) in a.pages[0].words.iter().zip(&old.pages[0].words) {
        assert_eq!(n.raw, o.raw);
        assert_eq!(n.id, o.id);
        assert_eq!(n.source_box, o.source_box);
        assert_eq!([n.x0, n.y0, n.x1, n.y1], [o.x0, o.y0, o.x1, o.y1]);
        assert_eq!(n.status, o.status);
        if n.text != o.text {
            changes += 1;
            assert_eq!(n.raw, "\u{e47f}口");
            assert_eq!(o.text, "口");
            assert_eq!(n.text, "樋口");
        }
    }
    assert_eq!(changes, 1);
    assert_eq!(a.pages[0].words.len(), old.pages[0].words.len());
}

#[test]
fn alias157_consumer_a_relation_cannot_be_transferred_to_another_private_raw_scalar_or_source_box()
{
    let x = html("\u{e47f}口", &tag(23185), ATTR);
    for (a, b) in [
        ("\u{e47f}", "\u{e480}"),
        ("xmin='200'", "xmin='201'"),
        ("ymin='100'", "ymin='101'"),
    ] {
        assert!(
            candidates(&x.replacen(a, b, 1)).iter().all(Option::is_none),
            "{a}"
        );
    }
}
