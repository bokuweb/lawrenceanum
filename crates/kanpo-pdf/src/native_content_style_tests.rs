use super::native_style_tests::{glyph, page};
use super::*;
fn render(p: Page) -> String {
    let n = p.glyphs.len();
    bbox_layout_with_native_styles(&[crate::Aj1PageSources {
        original: p,
        resolved: vec![None; n],
    }])
}
#[test]
fn mixed_word_preserves_punctuation_fonts_separately_from_content() {
    let p = page(
        "任（意）公告"
            .chars()
            .enumerate()
            .map(|(i, c)| {
                glyph(
                    &c.to_string(),
                    10. + i as f64 * 8.,
                    if c.is_alphanumeric() { 0 } else { 1 },
                    8.,
                )
            })
            .collect(),
    );
    assert_eq!(words(&p).len(), 1);
    let s = render(p);
    assert!(s.contains("data-native-styles=\"0:4020000000000000 1:4020000000000000\""));
    assert!(s.contains("data-native-content-styles=\"0:4020000000000000\""));
}
#[test]
fn a_symbol_only_word_has_complete_empty_content_evidence() {
    let s = render(page(vec![glyph("★", 10., 0, 8.)]));
    assert!(s.contains("data-native-content-styles=\"\""));
}
#[test]
fn unknown_private_character_class_is_explicitly_unknown() {
    let s = render(page(vec![glyph("\u{e000}", 10., 0, 8.)]));
    assert!(s.contains("data-native-content-styles=\"unknown\""));
}
#[test]
fn adding_source_class_evidence_does_not_change_legacy_xhtml_or_native_page() {
    let p = page(vec![glyph("任", 10., 0, 8.), glyph("（", 18., 1, 8.)]);
    let before = serde_json::to_value(&p).unwrap();
    let sources = vec![crate::Aj1PageSources {
        original: p.clone(),
        resolved: vec![None; 2],
    }];
    let old = bbox_layout_with_aj1_sources(&sources);
    let mut new = bbox_layout_with_native_styles(&sources);
    for attr in ["data-native-styles", "data-native-content-styles"] {
        let needle = format!(" {attr}=\"");
        while let Some(start) = new.find(&needle) {
            let from = start + needle.len();
            let end = from + new[from..].find('"').unwrap() + 1;
            new.replace_range(start..end, "");
        }
    }
    assert_eq!(new, old);
    assert_eq!(old, bbox_layout(&[p]));
    assert_eq!(before, serde_json::to_value(sources[0].page()).unwrap());
}

#[test]
fn content_styles_belong_to_the_source_occurrence_even_for_identical_text() {
    let p = page(vec![glyph("同", 10., 0, 8.), glyph("同", 40., 1, 9.)]);
    let words = words(&p);
    assert_eq!(
        native_content_styles(&p, &words[0], None).as_deref(),
        Some("0:4020000000000000")
    );
    assert_eq!(
        native_content_styles(&p, &words[1], None).as_deref(),
        Some("1:4022000000000000")
    );
}

#[test]
fn incomplete_reordered_or_mismatching_source_word_evidence_is_unknown() {
    let p = page(vec![glyph("甲", 10., 0, 8.), glyph("乙", 18., 1, 8.)]);
    let original = words(&p).remove(0);
    for ids in [vec![], vec![0, 9], vec![1, 0], vec![0, 0]] {
        let mut w = original.clone();
        w.glyphs = ids;
        assert_eq!(native_content_styles(&p, &w, None), None);
    }
    let mut w = original;
    w.text = "別の文字".into();
    assert_eq!(native_content_styles(&p, &w, None), None);
}

#[test]
fn source_proved_identity_cids_use_their_actual_unicode_class() {
    for ch in ['甲', '・'] {
        let cid = (0..65536)
            .find(|&id| crate::font::aj1(id) == Some(ch))
            .unwrap();
        let mut g = glyph(&char::from_u32(cid).unwrap().to_string(), 10., 0, 8.);
        g.code = cid;
        g.cid = cid;
        let mut p = page(vec![g]);
        p.fonts[0].ordering = Some("Identity".into());
        let w = words(&p).remove(0);
        assert_eq!(
            native_content_styles(&p, &w, Some(&[Some(ch)])).as_deref(),
            Some(if ch.is_alphanumeric() {
                "0:4020000000000000"
            } else {
                ""
            })
        );
        assert_eq!(native_content_styles(&p, &w, None), None);
        assert_eq!(native_content_styles(&p, &w, Some(&[Some('異')])), None);
    }
}

#[test]
fn an_alphanumeric_glyph_in_another_font_is_not_treated_as_punctuation() {
    for text in ["甲", "Q", "7", "あ"] {
        let p = page(vec![
            glyph("任", 10., 0, 8.),
            glyph(text, 18., 1, 8.),
            glyph("意", 26., 0, 8.),
        ]);
        let w = words(&p).remove(0);
        assert_eq!(
            native_content_styles(&p, &w, None).as_deref(),
            Some("0:4020000000000000 1:4020000000000000")
        );
    }
}
