use super::*;
use crate::{native_word_axes::NativeAxis, trace::NativeBounds};
fn sample() -> (Vec<Col>, Vec<NativeBounds>, Vec<NativeAxis>) {
    let boxes = [
        [70., 80., 400., 88.],
        [430., 80., 760., 88.],
        [70., 280., 400., 288.],
        [430., 280., 760., 288.],
        [80., 302., 88., 310.],
        [70., 330., 760., 338.],
    ];
    let mut words: Vec<_> = boxes
        .iter()
        .enumerate()
        .map(|(id, b)| Col {
            id,
            x: b[0],
            y: b[1],
            x1: b[2],
            y1: b[3],
            text: format!("arbitrary{id}"),
        })
        .collect();
    words[4].y = 297.;
    words[4].y1 = 305.;
    (
        words,
        boxes.into_iter().map(NativeBounds::Glyphs).collect(),
        vec![NativeAxis::Right; 6],
    )
}
fn cut(w: &[Col], b: &[NativeBounds], a: &[NativeAxis]) -> Option<f32> {
    source_table_bottom_cut(w, &[], &[(300., 69., 781.)], &[(425., 60., 300.)], b, a)
}
#[test]
fn source_boxes_can_certify_a_seam_crossed_only_by_compatibility_boxes() {
    let (w, b, a) = sample();
    assert_eq!(
        table_bottom_cut(&w, &[], &[(300., 69., 781.)], &[(425., 60., 300.)]),
        None
    );
    assert_eq!(cut(&w, &b, &a), Some(300.));
}
#[test]
fn source_certificate_is_independent_of_text_and_word_count() {
    let (mut w, mut b, mut a) = sample();
    for t in ["任意の文章", "章見出し", "x+y", "００", "（表）"] {
        for q in &mut w {
            q.text = t.into();
        }
        assert_eq!(cut(&w, &b, &a), Some(300.));
    }
    w.push(Col {
        id: 6,
        x: 100.,
        y: 200.,
        x1: 300.,
        y1: 208.,
        text: String::new(),
    });
    b.push(NativeBounds::Glyphs([100., 200., 300., 208.]));
    a.push(NativeAxis::Right);
    assert_eq!(cut(&w, &b, &a), Some(300.));
}
#[test]
fn source_certificate_survives_affine_page_scale_and_translation() {
    for (s, dx, dy) in [(0.5, 31., 29.), (1., 0., 0.), (2., 100., 80.)] {
        let (mut w, mut b, a) = sample();
        for q in &mut w {
            q.x = q.x * s + dx;
            q.x1 = q.x1 * s + dx;
            q.y = q.y * s + dy;
            q.y1 = q.y1 * s + dy;
        }
        for q in &mut b {
            if let NativeBounds::Glyphs(r) = q {
                r[0] = r[0] * s + dx;
                r[2] = r[2] * s + dx;
                r[1] = r[1] * s + dy;
                r[3] = r[3] * s + dy;
            }
        }
        let y = 300. * s + dy;
        assert_eq!(
            source_table_bottom_cut(
                &w,
                &[],
                &[(y, 69. * s + dx, 781. * s + dx)],
                &[(425. * s + dx, 60. * s + dy, y)],
                &b,
                &a
            ),
            Some(y)
        );
    }
}
#[test]
fn permuting_sources_does_not_rebind_metadata() {
    let (mut w, b, a) = sample();
    w.reverse();
    assert_eq!(cut(&w, &b, &a), Some(300.));
}
#[test]
fn source_crossing_is_not_rescued_by_a_plausible_compatibility_box() {
    let (mut w, mut b, a) = sample();
    w[4].y = 302.;
    w[4].y1 = 310.;
    b[4] = NativeBounds::Glyphs([80., 299., 88., 307.]);
    assert_eq!(cut(&w, &b, &a), None);
}
#[test]
fn unknown_or_compatibility_bounds_cannot_certify_source_separation() {
    for v in [NativeBounds::Unknown, NativeBounds::Compatibility] {
        let (w, mut b, a) = sample();
        b[4] = v;
        assert_eq!(cut(&w, &b, &a), None);
    }
}
#[test]
fn absent_bounds_and_axes_are_not_inferred() {
    let (w, b, a) = sample();
    assert_eq!(cut(&w, &b[..4], &a), None);
    assert_eq!(cut(&w, &b, &a[..4]), None);
}
#[test]
fn each_source_occurrence_must_have_a_known_rightward_advance() {
    for axis in [
        NativeAxis::Compatibility,
        NativeAxis::Unknown,
        NativeAxis::Down,
        NativeAxis::Up,
        NativeAxis::Left,
    ] {
        let (w, b, mut a) = sample();
        a[4] = axis;
        assert_eq!(cut(&w, &b, &a), None);
    }
}
#[test]
fn invalid_explicit_geometry_is_not_a_compatibility_fallback() {
    for r in [
        [80., f32::NAN, 88., 310.],
        [80., 302., 88., f32::INFINITY],
        [88., 302., 80., 310.],
        [80., 310., 88., 302.],
        [80., 302., 80., 310.],
    ] {
        let (w, mut b, a) = sample();
        b[4] = NativeBounds::Glyphs(r);
        assert_eq!(cut(&w, &b, &a), None);
    }
}
#[test]
fn different_source_occurrences_cannot_share_a_certificate_id() {
    let (mut w, b, a) = sample();
    w[4].id = 3;
    assert_eq!(cut(&w, &b, &a), None);
}
#[test]
fn a_native_rectangle_that_changes_side_does_not_authorize_legacy_partitioning() {
    let (w, mut b, a) = sample();
    b[4] = NativeBounds::Glyphs([80., 282., 88., 290.]);
    assert_eq!(cut(&w, &b, &a), None);
}
#[test]
fn a_horizontal_rule_without_a_terminating_panel_wall_is_insufficient() {
    let (w, b, a) = sample();
    assert_eq!(
        source_table_bottom_cut(&w, &[], &[(300., 69., 781.)], &[], &b, &a),
        None
    );
    assert_eq!(
        source_table_bottom_cut(&w, &[], &[(300., 69., 781.)], &[(425., 60., 450.)], &b, &a),
        None
    );
}
#[test]
fn extracted_tables_block_the_native_seam() {
    let (w, b, a) = sample();
    assert_eq!(
        source_table_bottom_cut(
            &w,
            &[(69., 250., 781., 350.)],
            &[(300., 69., 781.)],
            &[(425., 60., 300.)],
            &b,
            &a
        ),
        None
    );
}
#[test]
fn horizontal_extent_is_still_required() {
    let (w, b, a) = sample();
    assert_eq!(
        source_table_bottom_cut(&w, &[], &[(300., 69., 200.)], &[(425., 60., 300.)], &b, &a),
        None
    );
}
