use super::*;
fn body151_source() -> PageTrace {
    let mut p = valid();
    p.native_content_styles[6] = ContentStyles::Unknown;
    p.native_styles[6] = Some(BTreeSet::from([(0, 8f64.to_bits()), (3, 8f64.to_bits())]));
    p.native_horizontal_baselines = vec![None; p.words.len()];
    p.native_horizontal_baselines[6] = Some(165.);
    p
}
fn body151_two_words() -> PageTrace {
    let mut p = body151_source();
    p.words[6].text = "別の".into();
    p.words[6].raw = "別の".into();
    p.words[6].x1 = 100.;
    p.words[6].source_box[2] = 100.;
    p.native_bounds[6] = NativeBounds::Glyphs(p.words[6].source_box);
    add(&mut p, "任意の当事者", [100., 160., 240., 170.], 3);
    p.regions[0].lines.pop();
    p.regions[0].lines[6].words.push(7);
    p.native_content_styles[7] = ContentStyles::Unknown;
    p.native_horizontal_baselines.push(Some(165.));
    p
}
#[test]
fn body151_unknown_character_class_can_have_complete_original_body_typography() {
    let p = body151_source();
    assert_eq!(cuts(&p, &Default::default()), vec![5]);
    assert_eq!(p.native_content_styles[6], ContentStyles::Unknown);
}
#[test]
fn body151_several_source_words_need_the_same_original_horizontal_baseline() {
    let p = body151_two_words();
    assert_eq!(cuts(&p, &Default::default()), vec![5]);
}
#[test]
fn body151_complete_source_proof_is_invariant_to_scale_translation_and_font_ids() {
    for scale in [0.25f32, 4.] {
        let mut p = body151_source();
        for i in 0..p.words.len() {
            let old = p.words[i].source_box;
            let b = [
                old[0] * scale - 120.,
                old[1] * scale + 80.,
                old[2] * scale - 120.,
                old[3] * scale + 80.,
            ];
            p.words[i].source_box = b;
            p.words[i].x0 = b[0];
            p.words[i].y0 = b[1];
            p.words[i].x1 = b[2];
            p.words[i].y1 = b[3];
            p.native_bounds[i] = NativeBounds::Glyphs(b);
            if let Some(s) = &mut p.native_styles[i] {
                *s = s
                    .iter()
                    .map(|&(f, b)| (1000 - f, (f64::from_bits(b) * f64::from(scale)).to_bits()))
                    .collect();
            }
            if let ContentStyles::Glyphs(s) = &mut p.native_content_styles[i] {
                *s = s
                    .iter()
                    .map(|&(f, b)| (1000 - f, (f64::from_bits(b) * f64::from(scale)).to_bits()))
                    .collect();
            }
        }
        p.native_horizontal_baselines[6] = Some(165. * f64::from(scale) + 80.);
        assert_eq!(cuts(&p, &Default::default()), vec![5]);
    }
}
#[test]
fn body151_a_compatibility_or_missing_baseline_is_not_original_body_evidence() {
    let mut p = body151_source();
    p.native_horizontal_baselines.clear();
    assert!(cuts(&p, &Default::default()).is_empty());
}
#[test]
fn body151_invalid_or_outside_baselines_decline() {
    for y in [f64::NAN, f64::INFINITY, 159., 171.] {
        let mut p = body151_source();
        p.native_horizontal_baselines[6] = Some(y);
        assert!(cuts(&p, &Default::default()).is_empty());
    }
}
#[test]
fn body151_missing_all_glyph_font_evidence_cannot_be_reconstructed_from_body_text() {
    let mut p = body151_source();
    p.native_styles[6] = None;
    assert!(cuts(&p, &Default::default()).is_empty());
}
#[test]
fn body151_empty_or_nonfinite_source_font_sizes_decline() {
    for s in [
        BTreeSet::new(),
        BTreeSet::from([(0, f64::NAN.to_bits())]),
        BTreeSet::from([(0, 0f64.to_bits())]),
    ] {
        let mut p = body151_source();
        p.native_styles[6] = Some(s);
        assert!(cuts(&p, &Default::default()).is_empty());
    }
}
#[test]
fn body151_compatibility_bounds_or_nonhorizontal_axes_are_not_glyph_evidence() {
    for b in [NativeBounds::Unknown, NativeBounds::Compatibility] {
        let mut p = body151_source();
        p.native_bounds[6] = b;
        assert!(cuts(&p, &Default::default()).is_empty());
    }
    for a in [
        NativeAxis::Unknown,
        NativeAxis::Compatibility,
        NativeAxis::Down,
    ] {
        let mut p = body151_source();
        p.native_axes[6] = a;
        assert!(cuts(&p, &Default::default()).is_empty());
    }
}
#[test]
fn body151_baselines_from_different_source_rows_cannot_be_combined() {
    let mut p = body151_two_words();
    p.native_horizontal_baselines[7] = Some(166.);
    assert!(cuts(&p, &Default::default()).is_empty());
}
#[test]
fn body151_unknown_header_or_signature_classes_are_not_ordinary_body_evidence() {
    for i in [4, 5] {
        let mut p = body151_source();
        p.native_content_styles[i] = ContentStyles::Unknown;
        p.native_horizontal_baselines[i] = Some((p.words[i].y0 + p.words[i].y1) as f64 * 0.5);
        assert!(cuts(&p, &Default::default()).is_empty());
    }
}
#[test]
fn body151_body_typography_does_not_override_quotation_comparison_or_enclosure_authority() {
    let mut p = body151_source();
    p.regions[0].role = Role::Before;
    assert!(cuts(&p, &Default::default()).is_empty());
    let mut p = body151_source();
    p.regions[0].lines[3].text = "引用「".into();
    p.words[3].text = "引用「".into();
    assert!(cuts(&p, &Default::default()).is_empty());
    let p = body151_source();
    let mut r = PageRules::default();
    r.enclosures.push([50., 135., 410., 180.]);
    assert!(cuts(&p, &r).is_empty());
}
#[test]
fn body151_header_and_body_font_roles_still_need_distinct_source_authority() {
    for s in [
        BTreeSet::from([(2, 8f64.to_bits())]),
        BTreeSet::from([(3, 8f64.to_bits())]),
    ] {
        let mut p = body151_source();
        p.native_styles[6] = Some(s);
        assert!(cuts(&p, &Default::default()).is_empty());
    }
}
#[test]
fn body151_a_mismatched_rendered_line_or_repeated_source_owner_is_not_a_body_certificate() {
    let mut p = body151_source();
    p.regions[0].lines[6].text = "別の本文".into();
    assert!(cuts(&p, &Default::default()).is_empty());
    let mut p = body151_source();
    p.regions[0].lines[6].words.push(6);
    assert!(cuts(&p, &Default::default()).is_empty());
}

#[test]
fn body151_a_symbol_only_source_row_is_not_known_body_material() {
    let mut p = body151_source();
    p.words[6].text = "★".into();
    p.words[6].raw = "★".into();
    p.regions[0].lines[6].text = "★".into();
    assert!(cuts(&p, &Default::default()).is_empty());
}

#[test]
fn body151_known_character_classes_cannot_borrow_unclassified_glyph_fonts() {
    let mut p = body151_source();
    p.native_content_styles[6] = ContentStyles::Glyphs(BTreeSet::new());
    assert!(cuts(&p, &Default::default()).is_empty());
}

#[test]
fn body151_one_word_cannot_borrow_another_words_missing_glyph_font_evidence() {
    let mut p = body151_two_words();
    p.native_styles[7] = Some(BTreeSet::new());
    assert!(cuts(&p, &Default::default()).is_empty());
}

#[test]
fn body151_a_known_contradiction_in_one_word_is_not_rescued_by_another_opaque_word() {
    for content in [
        ContentStyles::Compatibility,
        ContentStyles::Glyphs(BTreeSet::new()),
        ContentStyles::Glyphs(BTreeSet::from([(123, 8f64.to_bits())])),
    ] {
        let mut p = body151_two_words();
        p.native_content_styles[7] = content;
        assert!(cuts(&p, &Default::default()).is_empty());
    }
}
