use super::*;
fn bottom(s: &mut (Vec<Unit>, Vec<Word>, PageRules, Vec<Option<f64>>)) {
    s.2.horizontal = vec![HRule {
        y: 40.,
        x0: 0.,
        x1: 250.,
    }];
}
fn label_words(
    s: &mut (Vec<Unit>, Vec<Word>, PageRules, Vec<Option<f64>>),
    row: usize,
    parts: &[&str],
) {
    let first = row * 2;
    let y = s.1[first].y0;
    let mut ids = vec![first];
    s.1[first].text = parts[0].into();
    s.1[first].raw = parts[0].into();
    s.1[first].x1 = 30.;
    for (i, text) in parts.iter().enumerate().skip(1) {
        let id = s.1.len();
        let x = 40. + (i - 1) as f32 * 40.;
        s.1.push(Word {
            id,
            x0: x,
            y0: y,
            x1: x + 30.,
            y1: y + 10.,
            source_box: [x, y, x + 30., y + 10.],
            text: (*text).into(),
            raw: (*text).into(),
            status: WordStatus::Used,
        });
        s.3.push(Some((y + 7.) as f64));
        ids.push(id);
    }
    ids.push(first + 1);
    let Unit::Plain(ls) = &mut s.0[0] else {
        panic!()
    };
    let amount = if row == 0 { "12,345" } else { "23,456" };
    ls[row] = Line::text(format!("{}　{amount}", parts.join("　")), ids);
}
#[test]
fn continuation149_bottom_rule_and_source_rails_certify_the_frame() {
    let mut s = source();
    bottom(&mut s);
    let old = text(&s.0);
    run(&mut s);
    assert_eq!(tables(&s.0).len(), 1);
    assert_eq!(text(&s.0), old);
}
#[test]
fn continuation149_compound_labels_keep_original_internal_gaps() {
    let mut s = source();
    label_words(&mut s, 0, &["Arbitrary", "alpha"]);
    label_words(&mut s, 1, &["任意", "名称"]);
    let old = text(&s.0);
    run(&mut s);
    assert_eq!(tables(&s.0).len(), 1);
    assert_eq!(text(&s.0), old);
    assert_eq!(tables(&s.0)[0].cells[0].text, "Arbitrary　alpha");
}
#[test]
fn continuation149_roman_enumerations_stay_inside_the_source_label() {
    for marker in ["Ⅰ", "Ⅳ", "Ⅵ", "Ⅻ", "ⅳ"] {
        let mut s = source();
        label_words(&mut s, 0, &[marker, "任意の名称"]);
        label_words(&mut s, 1, &["Ⅷ", "別の名称"]);
        let old = text(&s.0);
        run(&mut s);
        assert_eq!(tables(&s.0).len(), 1, "{marker}");
        assert_eq!(text(&s.0), old);
    }
}
#[test]
fn continuation149_closed_top_and_open_bottom_remain_symmetric_with_bottom_closed() {
    let mut a = source();
    let mut b = source();
    bottom(&mut b);
    run(&mut a);
    run(&mut b);
    assert_eq!(tables(&a.0), tables(&b.0));
}
#[test]
fn continuation149_a_second_numeric_lane_is_not_an_internal_label_gap() {
    let mut s = source();
    label_words(&mut s, 0, &["任意", "123"]);
    let old = text(&s.0);
    run(&mut s);
    assert!(tables(&s.0).is_empty());
    assert_eq!(text(&s.0), old);
}
#[test]
fn continuation149_numeric_content_after_a_roman_marker_is_not_a_label() {
    let mut s = source();
    label_words(&mut s, 0, &["Ⅳ", "123"]);
    run(&mut s);
    assert!(tables(&s.0).is_empty());
}
#[test]
fn continuation149_a_nonleading_roman_number_has_no_prefix_authority() {
    let mut s = source();
    label_words(&mut s, 0, &["任意", "Ⅳ"]);
    run(&mut s);
    assert!(tables(&s.0).is_empty());
}
#[test]
fn continuation149_internal_gap_never_permits_a_skipped_native_word() {
    let mut s = source();
    label_words(&mut s, 0, &["任意", "名称"]);
    let mut extra = s.1[0].clone();
    extra.id = s.1.len();
    extra.text = "別".into();
    extra.x0 = 110.;
    extra.x1 = 125.;
    s.1.push(extra);
    s.3.push(Some(12.));
    run(&mut s);
    assert!(tables(&s.0).is_empty());
}

#[test]
fn continuation149_a_kerned_sign_precedes_both_edges_of_its_numeric_word() {
    let mut s = signed_source();
    s.1[4].x0 = 190.;
    s.1[4].x1 = 210.;
    s.1[4].source_box = [190., 5., 210., 15.];
    let old = text(&s.0);
    run(&mut s);
    assert_eq!(tables(&s.0).len(), 1);
    assert_eq!(tables(&s.0)[0].cells[1].words, vec![4, 1]);
    assert_eq!(text(&s.0), old);
}
#[test]
fn continuation149_a_sign_covering_the_whole_numeric_word_is_ambiguous() {
    let mut s = signed_source();
    s.1[4].x0 = 190.;
    s.1[4].x1 = 250.;
    run(&mut s);
    assert!(tables(&s.0).is_empty());
}
#[test]
fn continuation149_a_sign_starting_inside_the_numeric_word_is_not_a_prefix() {
    let mut s = signed_source();
    s.1[4].x0 = 210.;
    s.1[4].x1 = 220.;
    run(&mut s);
    assert!(tables(&s.0).is_empty());
}

#[test]
fn continuation149_open_top_keeps_ascenders_when_the_baseline_is_inside() {
    let mut s = source();
    bottom(&mut s);
    s.2.vertical.iter_mut().for_each(|v| v.y0 = 10.);
    run(&mut s);
    assert_eq!(tables(&s.0).len(), 1);
    assert_eq!(tables(&s.0)[0].y0, 5.);
}

#[test]
fn continuation149_an_open_top_cannot_admit_a_baseline_outside_the_frame() {
    let mut s = source();
    bottom(&mut s);
    s.2.vertical.iter_mut().for_each(|v| v.y0 = 13.);
    run(&mut s);
    assert!(tables(&s.0).is_empty());
}

#[test]
fn continuation149_a_closed_top_cannot_be_crossed() {
    let mut s = source();
    bottom(&mut s);
    s.2.vertical.iter_mut().for_each(|v| v.y0 = 10.);
    s.2.horizontal.push(HRule {
        y: 10.,
        x0: 0.,
        x1: 250.,
    });
    run(&mut s);
    assert!(tables(&s.0).is_empty());
}

#[test]
fn continuation149_compound_labels_keep_ascii_and_fullwidth_spaces_exact() {
    let mut s = source();
    label_words(&mut s, 0, &["Arbitrary", "alpha"]);
    let Unit::Plain(ls) = &mut s.0[0] else {
        panic!()
    };
    ls[0].text = "Arbitrary  alpha　12,345".into();
    let old = text(&s.0);
    run(&mut s);
    assert_eq!(tables(&s.0)[0].cells[0].text, "Arbitrary  alpha");
    assert_eq!(text(&s.0), old);
}

#[test]
fn continuation149_kerned_source_proof_is_invariant_to_translation_and_scale() {
    for (scale, dx, dy) in [(0.25, 80., 120.), (4., -40., -80.)] {
        let mut s = signed_source();
        bottom(&mut s);
        s.1[4].x0 = 190.;
        s.1[4].x1 = 210.;
        for w in &mut s.1 {
            w.x0 = w.x0 * scale + dx;
            w.x1 = w.x1 * scale + dx;
            w.y0 = w.y0 * scale + dy;
            w.y1 = w.y1 * scale + dy;
            w.source_box = [w.x0, w.y0, w.x1, w.y1];
        }
        for v in &mut s.2.vertical {
            v.x = v.x * scale + dx;
            v.y0 = v.y0 * scale + dy;
            v.y1 = v.y1 * scale + dy;
        }
        for h in &mut s.2.horizontal {
            h.y = h.y * scale + dy;
            h.x0 = h.x0 * scale + dx;
            h.x1 = h.x1 * scale + dx;
        }
        for b in s.3.iter_mut().flatten() {
            *b = *b * scale as f64 + dy as f64;
        }
        let old = text(&s.0);
        run(&mut s);
        assert_eq!(tables(&s.0).len(), 1);
        assert_eq!(tables(&s.0)[0].cells[1].words, vec![4, 1]);
        assert_eq!(text(&s.0), old);
    }
}
