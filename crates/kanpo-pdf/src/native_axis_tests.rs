use super::native_style_tests::{glyph, page};
use super::*;
fn html(p: &Page) -> String {
    layout(std::iter::once((p, None, None, None)), true)
}
#[test]
fn complete_original_advances_define_source_axis_independently_of_font_mode() {
    for (x, y, axis) in [
        (8., 0., "right"),
        (0., 8., "down"),
        (-8., 0., "left"),
        (0., -8., "up"),
    ] {
        for vertical in [false, true] {
            let mut g = glyph("甲", 10., 0, 8.);
            g.vertical = vertical;
            g.end_x = g.x + x;
            g.end_y = g.y + y;
            let p = page(vec![g]);
            let s = html(&p);
            assert!(
                s.contains(&format!("data-native-axis=\"{}\"", axis)),
                "source axis absent: {s}"
            );
        }
    }
}
#[test]
fn incomplete_stationary_mixed_or_nonfinite_source_never_proves_an_axis() {
    for (x, y) in [(0., 0.), (8., 8.), (f64::NAN, 0.), (f64::INFINITY, 0.)] {
        let mut g = glyph("甲", 10., 0, 8.);
        g.end_x = g.x + x;
        g.end_y = g.y + y;
        let p = page(vec![g]);
        assert!(html(&p).contains("data-native-axis=\"unknown\""));
    }
    let mut p = page(vec![glyph("甲", 10., 0, 8.), glyph("乙", 18., 0, 8.)]);
    p.glyphs[1].end_y += 1.;
    let word = Word {
        text: "甲乙".into(),
        x_min: 10.,
        y_min: 93.,
        x_max: 26.,
        y_max: 102.,
        glyphs: vec![0, 1],
    };
    assert_eq!(native_source_axis(&p, &word), None);
    let p = page(vec![glyph("甲", 10., 0, 8.)]);
    for ids in [vec![], vec![0, 0], vec![1]] {
        let word = Word {
            text: "甲".into(),
            x_min: 10.,
            y_min: 93.,
            x_max: 18.,
            y_max: 102.,
            glyphs: ids,
        };
        assert_eq!(native_source_axis(&p, &word), None);
    }
}
#[test]
fn axis_is_unchanged_by_independent_positive_scaling_translation_and_font_labels() {
    for (sx, sy, dx, dy) in [
        (1., 1., 0., 0.),
        (0.125, 64., 32., -64.),
        (64., 0.125, -32., 64.),
    ] {
        let mut g = glyph("甲", 10., 0, 8.);
        g.end_x = g.x;
        g.end_y = g.y + 8.;
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
        let mut p = page(vec![g]);
        p.fonts[0].name = "unseen arbitrary font".into();
        assert!(html(&p).contains("data-native-axis=\"down\""));
    }
}
