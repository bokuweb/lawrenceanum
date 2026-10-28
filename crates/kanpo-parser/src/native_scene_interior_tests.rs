use super::*;
fn grid() -> crate::grid::Grid {
    crate::grid::Grid {
        xs: vec![0., 40., 100.],
        ys: vec![0., 100.],
        cells: vec![],
    }
}
fn scene() -> Scene {
    Scene {
        words: vec![],
        obstacles: vec![],
        lines: vec![
            ([40., 0., 40., 100.], [39.82, -0.18, 40.18, 100.18], true),
            (
                [100., 0., 100., 100.],
                [99.46, -0.54, 100.54, 100.54],
                false,
            ),
        ],
    }
}
const STRIP: [f32; 4] = [40., 20., 100., 35.];
#[test]
fn complete_original_boundary_paint_defines_an_interior_without_changing_clear() {
    let g = grid();
    let s = scene();
    assert!(!s.clear(STRIP, &g));
    let b = s.interior_strip(STRIP, &g).unwrap();
    assert!((b[0] as f64) >= 40.18 && (b[2] as f64) <= 99.46);
    assert!(s.clear(b, &g));
    assert!(contains(b.map(f64::from), [80., 22., 98., 33.]));
}
#[test]
fn an_unknown_image_or_fill_in_the_removed_border_footprint_is_still_an_obstacle() {
    let g = grid();
    let mut s = scene();
    s.obstacles.push([99.6, 22., 100.1, 25.]);
    assert!(s.interior_strip(STRIP, &g).is_none());
}
#[test]
fn a_nonflow_word_envelope_remains_opaque_even_outside_its_legacy_projection() {
    let g = grid();
    let mut s = scene();
    s.obstacles.push([90., 22., 110., 25.]);
    assert!(s.interior_strip(STRIP, &g).is_none());
}
#[test]
fn a_partial_boundary_cannot_supply_a_complete_source_frame() {
    let g = grid();
    let mut s = scene();
    s.lines[1].0 = [100., 10., 100., 50.];
    let b = s.interior_strip(STRIP, &g).unwrap();
    assert_eq!(b[2], 100.);
    assert!(!s.clear(b, &g));
}
#[test]
fn an_interior_line_is_not_ignored_merely_because_it_is_in_the_page_inventory() {
    let g = grid();
    let mut s = scene();
    s.lines
        .push(([70., 0., 70., 100.], [69.5, -0.5, 70.5, 100.5], true));
    let b = s.interior_strip(STRIP, &g).unwrap();
    assert!(!s.clear(b, &g));
}
#[test]
fn an_off_grid_complete_line_does_not_define_a_new_field_boundary() {
    let g = grid();
    let mut s = scene();
    s.lines[1] = ([99., 0., 99., 100.], [98.46, -0.54, 99.54, 100.54], false);
    let b = s.interior_strip(STRIP, &g).unwrap();
    assert_eq!(b[2], 100.);
    assert!(!s.clear(b, &g));
}
#[test]
fn source_boundary_width_can_exclude_a_glyph_and_never_counts_as_tolerance() {
    let g = grid();
    let mut s = scene();
    s.lines[1].1 = [97., -3., 103., 103.];
    let b = s.interior_strip(STRIP, &g).unwrap();
    assert!(!contains(b.map(f64::from), [80., 22., 98., 33.]));
}
#[test]
fn overlapping_source_borders_cannot_certify_an_empty_interior() {
    let g = grid();
    let mut s = scene();
    s.lines[0].1 = [0., -40., 80., 140.];
    s.lines[1].1 = [20., -80., 180., 180.];
    assert!(s.interior_strip(STRIP, &g).is_none());
}
#[test]
fn translation_and_scale_do_not_supply_a_fixed_pt_allowance() {
    for k in [0.25, 1., 3.] {
        let mut g = grid();
        let mut s = scene();
        let x = |v: f64| v * k + 17.125;
        let y = |v: f64| v * k - 2.5;
        g.xs.iter_mut().for_each(|v| *v = x(*v as f64) as f32);
        g.ys.iter_mut().for_each(|v| *v = y(*v as f64) as f32);
        for (c, p, _) in &mut s.lines {
            *c = [x(c[0]), y(c[1]), x(c[2]), y(c[3])];
            *p = [x(p[0]), y(p[1]), x(p[2]), y(p[3])];
        }
        let strip = [x(40.) as f32, y(20.) as f32, x(100.) as f32, y(35.) as f32];
        let b = s.interior_strip(strip, &g).unwrap();
        assert!(s.clear(b, &g));
    }
}
#[test]
fn invalid_source_grid_has_no_certified_interior() {
    for xs in [
        vec![],
        vec![0.],
        vec![100., 40., 0.],
        vec![0., f32::NAN, 100.],
    ] {
        let mut g = grid();
        g.xs = xs;
        assert!(scene().interior_strip(STRIP, &g).is_none());
    }
}
