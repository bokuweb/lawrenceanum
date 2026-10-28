use super::native_style_tests::{glyph, page};
use super::*;
fn rotate(g: &mut Glyph, axis: usize) {
    let point = |x: f64, y: f64| match axis {
        0 => (x + 300., y + 300.),
        1 => (300. - y, 300. + x),
        2 => (300. - x, 300. - y),
        _ => (300. + y, 300. - x),
    };
    let (x, y) = point(g.x, g.y);
    let (ex, ey) = point(g.end_x, g.end_y);
    let b = g.bbox;
    let pts = [
        point(b[0], b[1]),
        point(b[2], b[1]),
        point(b[2], b[3]),
        point(b[0], b[3]),
    ];
    g.x = x;
    g.y = y;
    g.end_x = ex;
    g.end_y = ey;
    g.bbox = [
        pts.iter().map(|p| p.0).fold(f64::INFINITY, f64::min),
        pts.iter().map(|p| p.1).fold(f64::INFINITY, f64::min),
        pts.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max),
        pts.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max),
    ];
}
fn canonical(axis: usize, x: f64, y: f64) -> (f64, f64) {
    match axis {
        0 => (x, y),
        1 => (y, -x),
        2 => (-x, -y),
        _ => (-y, x),
    }
}
#[test]
fn all_source_axes_keep_exact_origins_and_true_advance_in_both_font_modes() {
    for axis in 0..4 {
        for mode in [false, true] {
            let mut gs = vec![glyph("甲", 10., 0, 8.), glyph("乙", 18., 0, 8.)];
            for g in &mut gs {
                g.vertical = mode;
                rotate(g, axis);
            }
            let mut p = page(gs);
            p.fonts[0].vertical = mode;
            for w in words(&p) {
                let first = &p.glyphs[w.glyphs[0]];
                let last = &p.glyphs[*w.glyphs.last().unwrap()];
                let a = canonical(axis, first.x, first.y);
                let z = canonical(axis, last.end_x, last.end_y);
                let b = canonical(axis, first.end_x, first.end_y);
                assert_eq!(
                    native_axis_flow_tuple(&p, &w),
                    Some([a.0, a.1, z.0, b.0 - a.0]),
                    "axis{axis} mode{mode}"
                );
            }
        }
    }
}
#[test]
fn projected_baselines_are_not_rounded_together() {
    for axis in 0..4 {
        let mut gs = vec![glyph("甲", 10., 0, 8.), glyph("乙", 18., 0, 8.)];
        gs[1].y += 1e-10;
        gs[1].end_y += 1e-10;
        for g in &mut gs {
            rotate(g, axis);
        }
        let p = page(gs);
        let w = Word {
            text: "甲乙".into(),
            x_min: p
                .glyphs
                .iter()
                .map(|g| poppler_box(g)[0])
                .fold(f64::INFINITY, f64::min),
            y_min: p
                .glyphs
                .iter()
                .map(|g| poppler_box(g)[1])
                .fold(f64::INFINITY, f64::min),
            x_max: p
                .glyphs
                .iter()
                .map(|g| poppler_box(g)[2])
                .fold(f64::NEG_INFINITY, f64::max),
            y_max: p
                .glyphs
                .iter()
                .map(|g| poppler_box(g)[3])
                .fold(f64::NEG_INFINITY, f64::max),
            glyphs: vec![0, 1],
        };
        assert_eq!(native_axis_flow_tuple(&p, &w), None);
    }
}
#[test]
fn changed_word_box_text_or_foreign_glyphs_never_borrow_flow() {
    let mut gs = vec![glyph("甲", 10., 0, 8.), glyph("乙", 18., 0, 8.)];
    for g in &mut gs {
        rotate(g, 1);
    }
    let p = page(gs);
    let good = words(&p).remove(0);
    assert!(native_axis_flow_tuple(&p, &good).is_some());
    for which in 0..7 {
        let mut w = good.clone();
        match which {
            0 => w.text.push('別'),
            1 => w.x_min -= 1.,
            2 => w.y_max += 1.,
            3 => w.glyphs = vec![0, 0],
            4 => w.glyphs = vec![1, 0],
            5 => w.glyphs = vec![],
            _ => w.glyphs.push(999),
        };
        assert!(native_axis_flow_tuple(&p, &w).is_none());
        assert!(native_glyph_range(&p, &w).is_none());
    }
}
#[test]
fn original_range_is_contiguous_exclusive_and_first_zero_is_valid() {
    let p = page(vec![
        glyph("甲", 10., 0, 8.),
        glyph("乙", 18., 0, 8.),
        glyph("丙", 26., 0, 8.),
    ]);
    let w = words(&p).remove(0);
    assert_eq!(native_glyph_range(&p, &w), Some((0, 3)));
    let mut skipped = w.clone();
    skipped.glyphs = vec![0, 2];
    skipped.text = "甲丙".into();
    assert_eq!(native_glyph_range(&p, &skipped), None);
}
#[test]
fn multi_unicode_glyphs_do_not_turn_range_identity_into_character_count() {
    let p = page(vec![glyph("任意の文字列", 10., 0, 8.)]);
    let w = words(&p).remove(0);
    assert_eq!(native_glyph_range(&p, &w), Some((0, 1)));
}
#[test]
fn only_native_source_output_adds_ranges_and_page_count() {
    let p = page(vec![glyph("甲", 10., 0, 8.)]);
    let legacy = bbox_layout(&[p.clone()]);
    assert!(!legacy.contains("data-native-glyph-count"));
    assert!(!legacy.contains("first-glyph"));
    let s = layout(std::iter::once((&p,None,None,None,None)), true);
    assert!(s.contains("data-native-glyph-count=\"1\""));
    assert!(s.contains("data-native-flow-first-glyph=\"0\""));
    assert!(s.contains("data-native-flow-after-glyph=\"1\""));
}
fn extracted(body: &str, vertical: bool) -> Page {
    let cid = crate::source_charcode_tests::cids()[0];
    let body = body.replace("CODE", &format!("00{cid:02x}"));
    let pdf = crate::source_charcode_tests::fixture(cid, vertical, 0, &body, true, "Adobe");
    crate::extract_pages(&pdf).unwrap().remove(0)
}
#[test]
fn actual_pdf_tracking_and_tj_use_the_advance_axis_under_nonuniform_transforms() {
    for tm in ["2 0 0 3", "0 -2 3 0", "-2 0 0 3", "0 2 3 0"] {
        let p = extracted(
            &format!("{tm} 20 150 Tm 2 Tc [100 <CODE> <CODE>] TJ"),
            false,
        );
        assert_eq!(p.glyphs.len(), 2);
        assert_eq!(p.glyphs[0].spacing, 4., "{tm}");
        assert_eq!(p.glyphs[0].kern, -(100f64 / 1000. * 12.) * 2., "{tm}");
    }
}
#[test]
fn mirrored_horizontal_scaling_preserves_the_signed_forward_step() {
    let p = extracted(
        "2 0 0 3 20 150 Tm -100 Tz 2 Tc [100 <CODE> <CODE>] TJ",
        false,
    );
    assert_eq!(p.glyphs[0].spacing, 4.);
    assert_eq!(p.glyphs[0].kern, -(100f64 / 1000. * 12.) * 2.);
}
#[test]
fn negative_vertical_font_size_reverses_tracking_relative_to_forward_motion() {
    let p = extracted(
        "/F1 -12 Tf 2 0 0 3 20 150 Tm 2 Tc [100 <CODE> <CODE>] TJ",
        true,
    );
    assert_eq!(p.glyphs[0].spacing, 6.);
    assert_eq!(p.glyphs[0].kern, (100f64 / 1000. * 12.) * 3.);
}
