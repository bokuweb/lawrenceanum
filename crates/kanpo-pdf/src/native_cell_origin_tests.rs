use super::super::native_style_tests::{glyph, page};
use super::*;
fn example() -> (Page, Word) {
    let a = glyph("ab", 10., 0, 8.);
    let mut mark = glyph("-", 18., 1, 8.);
    mark.y -= 0.16;
    mark.end_y = mark.y;
    mark.bbox[1] -= 0.16;
    mark.bbox[3] -= 0.16;
    let b = glyph("cd", 26., 0, 8.);
    let p = page(vec![a, mark, b]);
    let w = words(&p).remove(0);
    assert_eq!(w.text, "ab-cd");
    (p, w)
}
#[test]
fn offset_punctuation_keeps_the_original_content_baseline_and_full_glyph_bounds() {
    let (p, w) = example();
    assert!(native_flow_tuple(&p, &w).is_none());
    let o = prove(&p, &w).unwrap();
    assert_eq!((o.first, o.after), (0, 3));
    assert_eq!(o.flow, [10., 100., 34., 8., 0., 0.]);
    assert_eq!(o.bounds, [10., 92.84, 34., 102.]);
}
#[test]
fn actual_alphanumeric_baseline_drift_is_unknown() {
    let (mut p, w) = example();
    p.glyphs[2].y += 1e-10;
    p.glyphs[2].end_y = p.glyphs[2].y;
    assert!(prove(&p, &w).is_none());
}
#[test]
fn a_word_cannot_borrow_foreign_original_text_or_geometry() {
    let (p, w) = example();
    for k in 0..4 {
        let mut bad = w.clone();
        match k {
            0 => bad.text.push('別'),
            1 => bad.x_min -= 1.,
            2 => bad.y_max += 1.,
            _ => bad.glyphs.push(99),
        }
        assert!(prove(&p, &bad).is_none());
    }
}
#[test]
fn original_occurrence_ranges_must_be_contiguous_and_unique() {
    let (p, w) = example();
    for ids in [vec![0, 0, 2], vec![2, 1, 0], vec![0, 2], vec![]] {
        let mut bad = w.clone();
        bad.glyphs = ids;
        assert!(prove(&p, &bad).is_none());
    }
}
#[test]
fn invisible_or_nonhorizontal_source_is_unknown() {
    let (mut p, w) = example();
    p.glyphs[1].invisible = true;
    assert!(prove(&p, &w).is_none());
    let (mut p, w) = example();
    p.glyphs[1].vertical = true;
    assert!(prove(&p, &w).is_none());
}
#[test]
fn punctuation_only_words_need_a_complete_common_baseline() {
    let p = page(vec![glyph("(", 10., 0, 8.), glyph(")", 18., 0, 8.)]);
    let w = words(&p).remove(0);
    assert!(prove(&p, &w).is_some());
    let mut p = p;
    p.glyphs[1].y -= 1.;
    p.glyphs[1].end_y = p.glyphs[1].y;
    assert!(prove(&p, &w).is_none());
}
#[test]
fn source_font_identifiers_do_not_choose_the_content_baseline() {
    let (p, w) = example();
    let expected = prove(&p, &w).unwrap();
    let mut p = p;
    for g in &mut p.glyphs {
        g.font = 1 - g.font;
    }
    for f in &mut p.fonts {
        f.name = "unseen renamed face".into();
    }
    assert_eq!(prove(&p, &w), Some(expected));
}
#[test]
fn independently_scaled_and_translated_original_coordinates_remain_exact() {
    for (sx, sy, dx, dy) in [
        (1., 1., 0., 0.),
        (0.125, 64., 32., -64.),
        (64., 0.125, -32., 64.),
    ] {
        let (mut p, _) = example();
        for g in &mut p.glyphs {
            g.x = g.x * sx + dx;
            g.end_x = g.end_x * sx + dx;
            g.y = g.y * sy + dy;
            g.end_y = g.end_y * sy + dy;
            g.size *= sy;
            g.bbox = [
                g.bbox[0] * sx + dx,
                g.bbox[1] * sy + dy,
                g.bbox[2] * sx + dx,
                g.bbox[3] * sy + dy,
            ];
        }
        let w = words(&p).remove(0);
        let o = prove(&p, &w).unwrap();
        assert_eq!(o.flow[1], 100. * sy + dy);
        assert_eq!(o.bounds[0], 10. * sx + dx);
        assert_eq!(o.bounds[2], 34. * sx + dx);
    }
}
#[test]
fn source_renderer_records_the_envelope_without_inventing_strict_flow() {
    let (p, w) = example();
    assert!(native_flow_tuple(&p, &w).is_none());
    let legacy = bbox_layout(&[p.clone()]);
    assert!(!legacy.contains("data-native-cell-origin"));
    let source = layout(std::iter::once((&p,None,None,None,None)), true);
    assert!(
        source.contains("data-native-cell-origin=\"2 0 3 10 100 34 8 0 0 10 92.84 34 102 8 8\"")
    );
    assert!(!source.contains("data-native-flow-start"));
}

#[test]
fn original_endpoint_font_sizes_survive_a_same_baseline_style_change() {
    let p = page(vec![glyph("a", 10., 0, 16.), glyph("b", 26.125, 0, 8.)]);
    let ws = words(&p);
    assert_eq!(ws.len(), 2);
    assert_eq!(prove(&p, &ws[0]).unwrap().sizes, [16., 16.]);
    assert_eq!(prove(&p, &ws[1]).unwrap().sizes, [8., 8.]);
}
#[test]
fn unusable_original_font_sizes_are_not_cell_evidence() {
    for size in [0., -1., f64::INFINITY, f64::NAN] {
        let (mut p, w) = example();
        p.glyphs[0].size = size;
        assert!(prove(&p, &w).is_none());
    }
}
