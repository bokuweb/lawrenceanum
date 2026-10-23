use super::*;
fn raw() -> String {
    char::from_u32(0xF0000 + 17518).unwrap().to_string()
}
fn word(x: i32, raw: &str, extra: &str) -> String {
    format!(
        "<word xMin=\"{x}\" yMin=\"100\" xMax=\"{}\" yMax=\"108\" {extra}>{raw}</word>",
        x + 8
    )
}
fn read(body: &str) -> Vec<Word> {
    let html =
        scraper::Html::parse_document(&format!("<page width=\"800\" height=\"600\">{body}</page>"));
    let p = html
        .select(&scraper::Selector::parse("page").unwrap())
        .next()
        .unwrap();
    page_words_traced(p, 800., 600., None).1
}
#[test]
fn source_candidate_changes_only_text_not_raw_ID_or_bbox() {
    let r = raw();
    let a = read(&word(200, &r, "data-source-aj1-text=\"嬴\""));
    let b = read(&word(200, &r, ""));
    assert_eq!(a[0].text, "嬴");
    assert_eq!(b[0].text, "䑮");
    assert_eq!(a[0].raw, b[0].raw);
    assert_eq!(a[0].id, b[0].id);
    assert_eq!(
        [a[0].x0, a[0].y0, a[0].x1, a[0].y1],
        [b[0].x0, b[0].y0, b[0].x1, b[0].y1]
    );
}
#[test]
fn wrong_and_partial_candidate_do_not_gain_authority() {
    let r = raw();
    for s in ["瀀", "嬴外", ""] {
        assert_eq!(
            read(&word(200, &r, &format!("data-source-aj1-text=\"{s}\"")))[0].text,
            "䑮"
        );
    }
}
#[test]
fn raw_candidate_evidence_sorts_as_one_occurrence() {
    let r = raw();
    let a = read(&(word(100, &r, "data-source-aj1-text=\"嬴\"") + &word(200, &r, "")));
    assert_eq!(
        a.iter().map(|w| w.text.as_str()).collect::<Vec<_>>(),
        ["䑮", "嬴"]
    );
    assert_eq!(a.iter().map(|w| w.id).collect::<Vec<_>>(), [0, 1]);
}
#[test]
fn exact_bbox_text_ties_do_not_transfer_resolution() {
    let r = raw();
    let a = read(&(word(200, &r, "data-source-aj1-text=\"嬴\"") + &word(200, &r, "")));
    assert_eq!(
        a.iter().map(|w| w.text.as_str()).collect::<Vec<_>>(),
        ["嬴", "䑮"]
    );
    let a = read(&(word(200, &r, "") + &word(200, &r, "data-source-aj1-text=\"嬴\"")));
    assert_eq!(
        a.iter().map(|w| w.text.as_str()).collect::<Vec<_>>(),
        ["䑮", "嬴"]
    );
}
#[test]
fn ordinary_unicode_and_existing_trusted_AJ1_keep_their_route() {
    let a = read(&word(
        200,
        "救",
        "data-unicode-aj1=\"1\" data-source-aj1-text=\"（チ）\"",
    ));
    assert_eq!(a[0].text, "救");
    let a = read(&word(200, "嬴", "data-source-aj1-text=\"瀀\""));
    assert_eq!(a[0].text, "嬴");
}
#[test]
fn real_ToUnicode_PUA_and_out_of_range_CIDs_are_not_standard_candidates() {
    for r in [
        "\u{E9F5}".to_string(),
        char::from_u32(0xF0000 + 25937).unwrap().to_string(),
    ] {
        assert_eq!(
            read(&word(200, &r, "data-source-aj1-text=\"嬴\""))[0].text,
            read(&word(200, &r, ""))[0].text
        );
    }
}

#[test]
fn explicit_source_decoder_has_priority_over_standard_CFF_fallback() {
    let raw = char::from_u32(0xF0000 + 21319).unwrap().to_string();
    let baseline = read(&word(200, &raw, ""));
    let with_fallback = read(&word(200, &raw, "data-source-aj1-text=\"圑\""));
    assert_eq!(baseline[0].text, "團");
    assert_eq!(with_fallback[0].text, baseline[0].text);
    assert_eq!(
        serde_json::to_string(&with_fallback).unwrap(),
        serde_json::to_string(&baseline).unwrap()
    );
}
#[test]
fn all_explicit_decoder_CIDs_are_not_unresolved_standard_fallback() {
    let mut witnessed = 0;
    for cid in 1..23060 {
        let Some(c) = kanpo_pdf::font::aj1(cid) else {
            continue;
        };
        let raw = char::from_u32(0xF0000 + cid).unwrap().to_string();
        if crate::normalize::private_cid_glyph(&raw, 8., 8.).is_some() {
            witnessed += 1;
            assert!(
                crate::source_aj1::candidate(&raw, &c.to_string()).is_none(),
                "already explicitly decoded CID {cid}"
            );
        }
    }
    assert!(witnessed > 0);
}
#[test]
fn one_resolved_component_refuses_whole_word_source_fallback() {
    let raw = char::from_u32(0xF0000 + 21319).unwrap().to_string()
        + &char::from_u32(0xF0000 + 17518).unwrap().to_string();
    assert!(crate::source_aj1::candidate(&raw, "圑嬴").is_none());
    let a = read(&word(200, &raw, "data-source-aj1-text=\"圑嬴\""));
    let b = read(&word(200, &raw, ""));
    assert_eq!(
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&b).unwrap()
    );
}

// Whole-Word priority is distinct from the singleton decoder inventory. The
// currently supported two-component decoder lies outside the standard table;
// these are contract/protection tests, not a claim of a newly failing input.
#[test]
fn compound_parenthesized_numbers_keep_the_whole_word_route() {
    let tagged = |cid: u32| char::from_u32(0xF0000 + cid).unwrap().to_string();
    let mut n = 0;
    for tens in 0x661D..=0x6625 {
        for ones in 0x6627..=0x6630 {
            let raw = tagged(tens) + &tagged(ones);
            assert!(crate::normalize::private_cid_glyph(&tagged(tens), 8., 8.).is_none());
            assert!(crate::normalize::private_cid_glyph(&tagged(ones), 8., 8.).is_none());
            let legacy = crate::normalize::private_cid_glyph(&raw, 8., 8.).unwrap();
            assert!(legacy.text.starts_with('（') && legacy.text.ends_with('）'));
            assert!(kanpo_pdf::font::aj1(tens).is_none() && kanpo_pdf::font::aj1(ones).is_none());
            assert!(crate::source_aj1::candidate(&raw, "外外").is_none());
            let a = read(&word(200, &raw, "data-source-aj1-text=\"外外\""));
            let b = read(&word(200, &raw, ""));
            assert_eq!(
                serde_json::to_string(&a).unwrap(),
                serde_json::to_string(&b).unwrap()
            );
            n += 1;
        }
    }
    assert_eq!(n, 90);
}
#[test]
fn compound_and_singleton_recursive_sequences_keep_the_legacy_route() {
    let tag = |cid: u32| char::from_u32(0xF0000 + cid).unwrap().to_string();
    let pair = tag(0x661D) + &tag(0x6627);
    let known = tag(21319);
    for raw in [
        pair.clone() + &pair,
        pair.clone() + &known,
        known.clone() + &pair,
        known.clone() + &known + &known,
    ] {
        assert!(crate::normalize::private_cid_glyph(&raw, 8., 8.).is_some());
        let value = "外".repeat(raw.chars().count());
        assert!(crate::source_aj1::candidate(&raw, &value).is_none());
        let a = read(&word(
            200,
            &raw,
            &format!("data-source-aj1-text=\"{value}\""),
        ));
        let b = read(&word(200, &raw, ""));
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
    }
}
#[test]
fn compound_mixed_unresolved_and_untagged_words_never_gain_partial_authority() {
    let tag = |cid: u32| char::from_u32(0xF0000 + cid).unwrap().to_string();
    let pair = tag(0x661D) + &tag(0x6627);
    for raw in [
        pair.clone() + &tag(17518),
        tag(17518) + &pair,
        pair.clone() + "嬴",
        format!("嬴{pair}"),
        "\u{661D}\u{6627}".into(),
    ] {
        let value = "外".repeat(raw.chars().count());
        assert!(crate::source_aj1::candidate(&raw, &value).is_none());
        let a = read(&word(
            200,
            &raw,
            &format!("data-source-aj1-text=\"{value}\""),
        ));
        let b = read(&word(200, &raw, ""));
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
    }
}
