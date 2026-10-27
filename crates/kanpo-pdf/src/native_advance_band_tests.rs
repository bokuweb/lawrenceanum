use super::*;
fn fixture(delta: f64) -> (Page, Word) {
    let glyphs = (0..2)
        .map(|i| {
            let x = 10. + i as f64 * 8.;
            let y = 100. + i as f64 * delta;
            Glyph {
                text: format!("{i}"),
                code: i,
                cid: i,
                font: 0,
                x,
                y,
                end_x: x + 8.,
                end_y: y,
                size: 8.,
                vertical: false,
                bbox: [x, y - 6., x + 8., y + 2.],
                invisible: false,
                spacing: 0.,
                kern: 0.,
                show: i,
            }
        })
        .collect();
    let p = Page {
        width: 595.,
        height: 842.,
        rotate: 0,
        glyphs,
        paths: vec![],
        fonts: vec![FontInfo {
            name: "arbitrary".into(),
            vertical: false,
            ordering: Some("Japan1".into()),
            has_to_unicode: false,
        }],
    };
    let w = Word {
        text: "01".into(),
        x_min: 10.,
        y_min: 94.,
        x_max: 26.,
        y_max: 102.,
        glyphs: vec![0, 1],
    };
    (p, w)
}
#[test]
fn band144_preserves_distinct_f64_origins_without_a_tolerance() {
    for d in [0., 0.008, -0.008, 1.5, 32.] {
        let (p, w) = fixture(d);
        let b = from_word(&p, &w).unwrap();
        assert_eq!(
            (b.min_baseline, b.max_baseline),
            (100_f64.min(100. + d), 100_f64.max(100. + d))
        );
        assert_eq!(b.projection, [w.x_min, w.y_min, w.x_max, w.y_max]);
        assert_eq!(
            b.glyph_bounds,
            [
                10.,
                100_f64.min(100. + d) - 6.,
                26.,
                100_f64.max(100. + d) + 2.
            ]
        );
        assert_eq!((b.first, b.after), (0, 2));
        assert_eq!((b.start, b.end, b.leading), (10., 26., 8.));
    }
}
#[test]
fn band144_does_not_relax_the_existing_exact_common_flow_contract() {
    let (p, w) = fixture(0.008);
    assert!(native_flow_tuple(&p, &w).is_none());
    assert!(from_word(&p, &w).is_some());
    let (p, w) = fixture(0.);
    assert!(native_flow_tuple(&p, &w).is_some());
}
#[test]
fn band144_rejects_missing_duplicate_reversed_and_gapped_glyph_ownership() {
    for ids in [vec![], vec![0, 0], vec![1, 0], vec![0, 2]] {
        let (mut p, mut w) = fixture(0.);
        p.glyphs.push(p.glyphs[1].clone());
        w.glyphs = ids;
        assert!(from_word(&p, &w).is_none());
    }
}
#[test]
fn band144_rejects_foreign_native_word_rectangles() {
    for i in 0..4 {
        let (p, mut w) = fixture(0.008);
        match i {
            0 => w.x_min += 1.,
            1 => w.y_min += 1.,
            2 => w.x_max += 1.,
            _ => w.y_max += 1.,
        };
        assert!(from_word(&p, &w).is_none());
    }
}
#[test]
fn band144_rejects_nonfinite_and_unowned_source_paints() {
    for i in 0..7 {
        let (mut p, w) = fixture(0.008);
        let g = &mut p.glyphs[1];
        match i {
            0 => g.x = f64::NAN,
            1 => g.y = f64::INFINITY,
            2 => g.end_x = f64::NAN,
            3 => g.spacing = f64::INFINITY,
            4 => g.kern = f64::NAN,
            5 => g.invisible = true,
            _ => g.font = 99,
        };
        assert!(from_word(&p, &w).is_none());
    }
}
#[test]
fn band144_rejects_nonmonotonic_or_different_advance_axes() {
    for i in 0..3 {
        let (mut p, w) = fixture(0.008);
        let g = &mut p.glyphs[1];
        match i {
            0 => {
                g.x = 9.;
                g.end_x = 17.;
            }
            1 => g.end_x = 10.,
            _ => {
                g.end_x = g.x;
                g.end_y = g.y + 8.;
            }
        };
        assert!(from_word(&p, &w).is_none());
    }
}
#[test]
fn band144_source_geometry_scales_and_translates_without_origin_rounding() {
    for s in [0.5, 2.] {
        let (mut p, mut w) = fixture(0.008);
        for g in &mut p.glyphs {
            g.x = g.x * s + 40.;
            g.end_x = g.end_x * s + 40.;
            g.y = g.y * s + 64.;
            g.end_y = g.end_y * s + 64.;
            g.size *= s;
            g.bbox = [
                g.bbox[0] * s + 40.,
                g.bbox[1] * s + 64.,
                g.bbox[2] * s + 40.,
                g.bbox[3] * s + 64.,
            ];
        }
        w.x_min = w.x_min * s + 40.;
        w.x_max = w.x_max * s + 40.;
        w.y_min = w.y_min * s + 64.;
        w.y_max = w.y_max * s + 64.;
        let b = from_word(&p, &w).unwrap();
        assert_eq!(
            (b.min_baseline, b.max_baseline),
            (p.glyphs[0].y, p.glyphs[1].y)
        );
    }
}
#[test]
fn band144_original_mixed_origin_code_has_a_band_but_no_common_baseline() {
    let p = crate::extract_pages(include_bytes!("../../kanpo-parser/tests/data/source-advance-band/20261020g002300021.pdf"))
        .unwrap()
        .remove(0);
    let words = words(&p);
    let w = words
        .iter()
        .find(|w| w.text.ends_with("6211\u{e61c}12"))
        .unwrap();
    assert!(native_flow_tuple(&p, w).is_none());
    let b = from_word(&p, w).unwrap();
    assert_eq!((b.first, b.after), (2095, 2103));
    assert_eq!(b.min_baseline, p.glyphs[2100].y);
    assert_eq!(b.max_baseline, p.glyphs[2095].y);
}

#[test]
fn band144_horizontal_band_requires_horizontal_native_glyph_modes() {
    let (mut p, mut w) = fixture(0.);
    p.fonts.push(FontInfo {
        name: "arbitrary vertical".into(),
        vertical: true,
        ordering: Some("Japan1".into()),
        has_to_unicode: false,
    });
    p.glyphs[1].font = 1;
    p.glyphs[1].vertical = true;
    // The actual paint advances right, but this is not an all-mode0 source Word.
    w.x_max = 18.;
    assert_eq!(native_source_axis(&p, &w), Some("right"));
    assert!(from_word(&p, &w).is_none());
}
#[test]
fn band144_word_projection_and_full_glyph_envelope_remain_distinct() {
    let (p, w) = fixture(-0.008);
    let b = from_word(&p, &w).unwrap();
    assert_eq!(b.projection[1], 94.);
    assert_eq!(b.glyph_bounds[1], p.glyphs[1].bbox[1]);
    assert!(b.glyph_bounds[1] < b.projection[1]);
}
#[test]
fn band144_nonfinite_raw_glyph_boxes_are_not_partially_certified() {
    for field in 0..4 {
        let (mut p, w) = fixture(0.008);
        p.glyphs[1].bbox[field] = f64::NAN;
        assert!(from_word(&p, &w).is_none());
    }
}
fn attr<'a>(s: &'a str, name: &str) -> Option<&'a str> {
    let rest = s.split_once(&format!("{name}=\""))?.1;
    Some(rest.split_once('"')?.0)
}
fn native_html(p: &Page) -> String {
    layout(std::iter::once((p, None, None, None)), true)
}
#[test]
fn band144_legacy_xhtml_never_receives_source_band_metadata() {
    let (p, _) = fixture(0.008);
    assert!(!bbox_layout(&[p]).contains("data-native-advance-band"));
}
#[test]
fn band144_exact_flow_words_keep_the_existing_metadata_contract() {
    let (p, _) = fixture(0.);
    let s = native_html(&p);
    assert!(s.contains("data-native-flow-baseline=\"100\""));
    assert!(!s.contains("data-native-advance-band"));
}
#[test]
fn band144_emission_keeps_full_precision_range_and_canonical_projection_reference() {
    let (p, _) = fixture(-0.008);
    assert_eq!(words(&p).len(), 1);
    let s = native_html(&p);
    let tokens = attr(&s, "data-native-advance-band")
        .unwrap()
        .split_whitespace()
        .collect::<Vec<_>>();
    assert_eq!(tokens.len(), 18);
    assert_eq!(&tokens[..3], &["1", "0", "2"]);
    assert_eq!(tokens[4].parse::<f64>().unwrap(), p.glyphs[1].y);
    assert_eq!(tokens[5].parse::<f64>().unwrap(), p.glyphs[0].y);
    assert_eq!(tokens[11], "94.000000");
    assert_eq!(tokens[15].parse::<f64>().unwrap(), p.glyphs[1].bbox[1]);
    assert!(!s.contains("data-native-flow-baseline"));
}
#[test]
fn band144_original_mixed_origin_code_emits_band_instead_of_inventing_exact_flow() {
    let p = crate::extract_pages(include_bytes!("../../kanpo-parser/tests/data/source-advance-band/20261020g002300021.pdf"))
        .unwrap()
        .remove(0);
    let s = native_html(&p);
    let w = s
        .split("<word ")
        .find(|w| w.contains("Ｒ6211\u{e61c}12</word>"))
        .unwrap()
        .split("</word>")
        .next()
        .unwrap();
    let v = attr(w, "data-native-advance-band")
        .unwrap()
        .split_whitespace()
        .collect::<Vec<_>>();
    assert_eq!(v.len(), 18);
    assert_eq!(&v[..3], &["1", "2095", "2103"]);
    assert_eq!(v[4].parse::<f64>().unwrap(), p.glyphs[2100].y);
    assert_eq!(v[5].parse::<f64>().unwrap(), p.glyphs[2095].y);
    assert!(!w.contains("data-native-flow-baseline"));
}
