use super::*;
fn court150_box(p: &mut PageTrace, id: usize, b: Rect) {
    let w = &mut p.words[id];
    w.x0 = b[0];
    w.y0 = b[1];
    w.x1 = b[2];
    w.y1 = b[3];
    w.source_box = b;
    p.native_bounds[id] = NativeBounds::Glyphs(b);
}
fn court150_native(p: &mut PageTrace) {
    p.native_horizontal_baselines = p
        .words
        .iter()
        .map(|w| Some(f64::from((w.y0 + w.y1) * 0.5)))
        .collect();
}
fn court150_source() -> (PageTrace, PageRules) {
    let mut p = valid();
    court150_box(&mut p, 4, [180., 120., 260., 130.]);
    court150_box(&mut p, 6, [68., 160., 400., 170.]);
    court150_native(&mut p);
    let mut r = PageRules::default();
    r.vertical = vec![
        crate::lines::VRule {
            x: 50.,
            y0: 0.,
            y1: 800.,
        },
        crate::lines::VRule {
            x: 410.,
            y0: 0.,
            y1: 800.,
        },
    ];
    (p, r)
}
fn court150_transition() -> (PageTrace, PageRules) {
    let (mut p, mut r) = court150_source();
    court150_box(&mut p, 5, [420., 40., 580., 50.]);
    court150_box(&mut p, 6, [420., 60., 760., 70.]);
    court150_native(&mut p);
    r.vertical.push(crate::lines::VRule {
        x: 770.,
        y0: 0.,
        y1: 800.,
    });
    (p, r)
}
#[test]
fn court150_source_column_allows_a_centered_signature_before_a_wider_address() {
    let (p, r) = court150_source();
    assert_eq!(cuts(&p, &r), vec![5]);
}
#[test]
fn court150_source_column_allows_a_flush_body_but_not_a_body_before_the_header_margin() {
    let (mut p, r) = court150_source();
    court150_box(&mut p, 6, [60., 160., 400., 170.]);
    assert_eq!(cuts(&p, &r), vec![5]);
    court150_box(&mut p, 6, [59., 160., 400., 170.]);
    assert!(cuts(&p, &r).is_empty());
}
#[test]
fn court150_source_column_cannot_be_inferred_from_an_incomplete_wall() {
    let (p, mut r) = court150_source();
    r.vertical[0].y1 = 100.;
    assert!(cuts(&p, &r).is_empty());
}
#[test]
fn court150_source_column_does_not_admit_a_word_crossing_its_wall() {
    let (mut p, r) = court150_source();
    court150_box(&mut p, 6, [68., 160., 411., 170.]);
    assert!(cuts(&p, &r).is_empty());
}
#[test]
fn court150_source_column_does_not_replace_missing_native_baselines() {
    let (mut p, r) = court150_source();
    p.native_horizontal_baselines[5] = None;
    assert!(cuts(&p, &r).is_empty());
}
#[test]
fn court150_source_column_keeps_font_role_authority() {
    let (mut p, r) = court150_source();
    p.native_content_styles[5] = p.native_content_styles[6].clone();
    assert!(cuts(&p, &r).is_empty());
}
#[test]
fn court150_adjacent_native_columns_do_not_require_a_repeated_category_caption() {
    let (p, r) = court150_transition();
    assert_eq!(cuts(&p, &r), vec![5]);
}
#[test]
fn court150_column_transition_requires_the_previous_signature_to_be_terminal() {
    let (mut p, r) = court150_transition();
    add(&mut p, "別の本文。", [68., 180., 220., 190.], 0);
    court150_native(&mut p);
    assert!(cuts(&p, &r).is_empty());
}
#[test]
fn court150_column_transition_requires_the_next_header_to_be_first() {
    let (mut p, r) = court150_transition();
    add(&mut p, "別の本文。", [420., 20., 580., 30.], 0);
    court150_native(&mut p);
    assert!(cuts(&p, &r).is_empty());
}
#[test]
fn court150_native_column_proof_is_translation_and_scale_invariant() {
    for scale in [0.25, 4.] {
        let (mut p, mut r) = court150_source();
        for i in 0..p.words.len() {
            let w = &p.words[i];
            let b = [
                w.x0 * scale - 120.,
                w.y0 * scale + 80.,
                w.x1 * scale - 120.,
                w.y1 * scale + 80.,
            ];
            court150_box(&mut p, i, b);
        }
        for v in &mut r.vertical {
            v.x = v.x * scale - 120.;
            v.y0 = v.y0 * scale + 80.;
            v.y1 = v.y1 * scale + 80.;
        }
        court150_native(&mut p);
        assert_eq!(cuts(&p, &r), vec![5]);
    }
}

#[test]
fn court150_proven_columns_keep_existing_enclosures_authoritative() {
    let (p, mut r) = court150_source();
    r.enclosures.push([50., 135., 410., 180.]);
    assert!(cuts(&p, &r).is_empty());
}
#[test]
fn court150_proven_columns_do_not_promote_a_quoted_case() {
    let (mut p, r) = court150_source();
    p.regions[0].lines[3].text = "引用「".into();
    p.words[3].text = "引用「".into();
    p.words[3].raw = "引用「".into();
    assert!(cuts(&p, &r).is_empty());
}
#[test]
fn court150_proven_columns_keep_comparison_role_authority() {
    let (mut p, r) = court150_source();
    p.regions[0].role = Role::Before;
    assert!(cuts(&p, &r).is_empty());
}
#[test]
fn court150_column_inventory_does_not_guess_an_unrelated_unknown_native_bound() {
    let (mut p, r) = court150_source();
    p.native_bounds[0] = NativeBounds::Unknown;
    assert!(cuts(&p, &r).is_empty());
}
#[test]
fn court150_column_inventory_cannot_skip_another_used_word_crossing_the_wall() {
    let (mut p, r) = court150_source();
    add(&mut p, "別の原字", [405., 40., 420., 50.], 0);
    court150_native(&mut p);
    assert!(cuts(&p, &r).is_empty());
}
