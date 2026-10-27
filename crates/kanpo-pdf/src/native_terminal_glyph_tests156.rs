use super::*;
fn source() -> (Page, Word) {
    let glyph = |text: &str, x: f64, end: f64| Glyph {
        text: text.into(),
        code: text.chars().next().unwrap() as u32,
        cid: text.chars().next().unwrap() as u32,
        font: 0,
        x,
        y: 100.,
        end_x: end,
        end_y: 100.,
        size: 10.,
        vertical: false,
        bbox: [x, 90., end, 102.],
        invisible: false,
        spacing: 0.,
        kern: 0.,
        show: 0,
    };
    let page = Page {
        width: 200.,
        height: 200.,
        rotate: 0,
        glyphs: vec![glyph("a", 10., 20.), glyph("b", 20., 23.)],
        paths: vec![],
        fonts: vec![FontInfo {
            name: "Arbitrary font identity".into(),
            vertical: false,
            ordering: None,
            has_to_unicode: true,
        }],
    };
    let word = Word {
        text: "ab".into(),
        x_min: 10.,
        y_min: 90.,
        x_max: 23.,
        y_max: 102.,
        glyphs: vec![0, 1],
    };
    (page, word)
}
#[test]
fn terminal156_last_glyph_advance_is_not_the_first_glyph_width() {
    let (p, w) = source();
    let t = from_word(&p, &w).expect("original last glyph");
    assert_eq!(
        t,
        Terminal {
            first: 0,
            after: 2,
            start: 20.,
            end: 23.,
            baseline: 100.
        }
    );
    assert_ne!(t.end - t.start, p.glyphs[0].end_x - p.glyphs[0].x);
}
#[test]
fn terminal156_single_glyph_keeps_its_own_complete_advance() {
    let (mut p, mut w) = source();
    p.glyphs.pop();
    w.text = "a".into();
    w.x_max = 20.;
    w.glyphs = vec![0];
    assert_eq!(
        from_word(&p, &w),
        Some(Terminal {
            first: 0,
            after: 1,
            start: 10.,
            end: 20.,
            baseline: 100.
        })
    );
}
#[test]
fn terminal156_source_coordinates_are_affine_without_a_fixed_tolerance() {
    for scale in [0.25, 4.] {
        let (mut p, mut w) = source();
        for g in &mut p.glyphs {
            g.size *= scale;
            g.x = g.x * scale + 17.;
            g.end_x = g.end_x * scale + 17.;
            g.y = g.y * scale + 31.;
            g.end_y = g.end_y * scale + 31.;
            g.bbox = [
                g.bbox[0] * scale + 17.,
                g.bbox[1] * scale + 31.,
                g.bbox[2] * scale + 17.,
                g.bbox[3] * scale + 31.,
            ];
        }
        w.x_min = w.x_min * scale + 17.;
        w.x_max = w.x_max * scale + 17.;
        w.y_min = w.y_min * scale + 31.;
        w.y_max = w.y_max * scale + 31.;
        let t = from_word(&p, &w).unwrap();
        assert_eq!(
            [t.start, t.end, t.baseline],
            [20. * scale + 17., 23. * scale + 17., 100. * scale + 31.]
        );
    }
}
#[test]
fn terminal156_source_font_names_supply_no_dispatch() {
    let (mut p, w) = source();
    p.fonts[0].name = "Unrelated name".into();
    assert!(from_word(&p, &w).is_some());
}
#[test]
fn terminal156_metadata_version_serializes_original_range_and_endpoints() {
    assert_eq!(
        Terminal {
            first: 8,
            after: 11,
            start: 20.,
            end: 23.,
            baseline: 100.
        }
        .attribute(),
        "1 8 11 20 23 100"
    );
}
#[test]
fn terminal156_source_only_metadata_is_emitted_only_with_native_evidence() {
    let (p, _) = source();
    let native = layout([(&p,None,None,None,None)], true);
    assert!(native.contains("data-native-terminal-glyph=\"1 0 2 20 23 100\""));
    assert!(!bbox_layout(&[p]).contains("data-native-terminal-glyph"));
}
#[test]
fn terminal156_unknown_or_mismatched_word_payload_declines() {
    let (p, mut w) = source();
    w.text = "changed".into();
    assert!(from_word(&p, &w).is_none());
}
#[test]
fn terminal156_a_missing_source_font_declines() {
    let (mut p, w) = source();
    p.fonts.clear();
    assert!(from_word(&p, &w).is_none());
}
#[test]
fn terminal156_a_missing_original_glyph_declines() {
    let (mut p, w) = source();
    p.glyphs.pop();
    assert!(from_word(&p, &w).is_none());
}
#[test]
fn terminal156_a_skipped_original_glyph_is_not_a_contiguous_word() {
    let (mut p, mut w) = source();
    let mut g = p.glyphs[1].clone();
    g.text = "c".into();
    g.x = 23.;
    g.end_x = 26.;
    g.bbox = [23., 90., 26., 102.];
    p.glyphs.push(g);
    w.text = "ac".into();
    w.x_max = 26.;
    w.glyphs = vec![0, 2];
    assert!(from_word(&p, &w).is_none());
}
#[test]
fn terminal156_duplicate_or_reversed_original_ranges_decline() {
    for ids in [vec![0, 0], vec![1, 0], vec![usize::MAX]] {
        let (p, mut w) = source();
        w.glyphs = ids;
        assert!(from_word(&p, &w).is_none());
    }
}
#[test]
fn terminal156_mixed_vertical_mode_cannot_borrow_the_first_glyph_mode() {
    let (mut p, w) = source();
    p.glyphs[1].vertical = true;
    assert!(from_word(&p, &w).is_none());
}
#[test]
fn terminal156_diagonal_source_advance_is_not_horizontal() {
    let (mut p, w) = source();
    p.glyphs[1].end_y += 1.;
    assert!(from_word(&p, &w).is_none());
}
#[test]
fn terminal156_different_exact_source_baselines_are_not_rounded() {
    let (mut p, w) = source();
    p.glyphs[1].y += 0.0001;
    p.glyphs[1].end_y += 0.0001;
    assert!(from_word(&p, &w).is_none());
}
#[test]
fn terminal156_an_invisible_last_glyph_supplies_no_visible_tail() {
    let (mut p, w) = source();
    p.glyphs[1].invisible = true;
    assert!(from_word(&p, &w).is_none());
}
#[test]
fn terminal156_source_projection_is_bound_to_its_own_word() {
    let (p, mut w) = source();
    w.x_max += 1.;
    assert!(from_word(&p, &w).is_none());
}
#[test]
fn terminal156_nonfinite_source_coordinates_decline() {
    let (mut p, w) = source();
    p.glyphs[1].end_x = f64::NAN;
    assert!(from_word(&p, &w).is_none());
}
#[test]
fn terminal156_empty_source_word_supplies_no_range() {
    let (p, mut w) = source();
    w.glyphs.clear();
    assert!(from_word(&p, &w).is_none());
}

#[test]
fn terminal156_a_single_source_glyph_can_have_several_unicode_scalars() {
    let (mut p, mut w) = source();
    p.glyphs.truncate(1);
    p.glyphs[0].text = "ffi".into();
    w.text = "ffi".into();
    w.x_max = 20.;
    w.glyphs = vec![0];
    assert_eq!(
        from_word(&p, &w),
        Some(Terminal {
            first: 0,
            after: 1,
            start: 10.,
            end: 20.,
            baseline: 100.
        })
    );
}
