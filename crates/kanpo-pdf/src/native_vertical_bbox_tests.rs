use super::*;
fn vglyph(text: &str, x: f64, y: f64, horizontal: bool) -> Glyph {
    Glyph {
        text: text.into(),
        code: 0,
        cid: 0,
        font: 0,
        x,
        y,
        end_x: x + if horizontal { 8. } else { 0. },
        end_y: y + if horizontal { 0. } else { 8. },
        size: 8.,
        vertical: true,
        bbox: if horizontal {
            [x, y - 4., x + 8., y + 4.]
        } else {
            [x - 4., y, x + 4., y + 8.]
        },
        invisible: false,
        spacing: 0.,
        kern: 0.,
        show: 0,
    }
}
fn vpage(g: Vec<Glyph>) -> Page {
    Page {
        width: 842.,
        height: 595.,
        rotate: 270,
        glyphs: g,
        paths: vec![],
        fonts: vec![FontInfo {
            name: "Vertical".into(),
            vertical: true,
            ordering: Some("Japan1".into()),
            has_to_unicode: false,
        }],
    }
}
#[test]
fn horizontal_wmode_one_exports_native_bounds_without_changing_compatibility() {
    let p = vpage(vec![vglyph("別", 68., 486., true), vglyph("表", 76., 486., true)]);
    let ws = words(&p);
    assert_eq!((ws[0].x_min, ws[0].y_min, ws[0].x_max, ws[0].y_max), (60., 478., 76., 486.));
    let s = bbox_layout(&[p]);
    assert!(s.contains("data-wmode=\"1\" data-advance-x=\"8.000000\" data-advance-y=\"0.000000\""));
    assert!(
        s.contains("data-native-xmin=\"68.000000\" data-native-ymin=\"482.000000\" data-native-xmax=\"84.000000\" data-native-ymax=\"490.000000\"")
    );
    assert!(s.contains("xMin=\"60.000000\" yMin=\"478.000000\""));
}
#[test]
fn vertical_advance_remains_explicit_in_metadata() {
    let s = bbox_layout(&[vpage(vec![vglyph("別", 68., 486., false)])]);
    assert!(s.contains("data-advance-x=\"0.000000\" data-advance-y=\"8.000000\""));
}
#[test]
fn horizontal_font_does_not_export_vertical_metadata() {
    let mut g = vglyph("横", 68., 486., true);
    g.vertical = false;
    let s = bbox_layout(&[vpage(vec![g])]);
    assert!(s.contains("data-wmode=\"0\""));
    assert!(!s.contains("data-native-"));
}
