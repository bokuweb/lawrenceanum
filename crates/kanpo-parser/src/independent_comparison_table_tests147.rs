use super::*;
use crate::grid::Grid;
fn frame() -> Grid {
    Grid {
        xs: vec![10., 20.],
        ys: vec![30., 40.],
        cells: vec![],
    }
}
#[test]
fn independent147_a_parent_is_not_an_independent_source_table() {
    let f = frame();
    assert!(!independent_table((0., 25., 20., 50.), &[&f]));
}
#[test]
fn independent147_a_descendant_is_not_an_independent_source_table() {
    let f = frame();
    assert!(!independent_table((12., 18., 32., 38.), &[&f]));
}
#[test]
fn independent147_a_crossing_frame_is_not_independent() {
    let f = frame();
    assert!(!independent_table((5., 15., 25., 35.), &[&f]));
}
#[test]
fn independent147_an_identical_frame_is_not_independent() {
    let f = frame();
    assert!(!independent_table((10., 20., 30., 40.), &[&f]));
}
#[test]
fn independent147_disjoint_source_tables_are_independent_on_either_axis() {
    let f = frame();
    for b in [
        (0., 9., 30., 40.),
        (21., 30., 30., 40.),
        (10., 20., 10., 29.),
        (10., 20., 41., 50.),
    ] {
        assert!(independent_table(b, &[&f]));
    }
}
#[test]
fn independent147_adjacent_cells_may_share_a_printed_border() {
    let f = frame();
    assert!(independent_table((0., 10., 30., 40.), &[&f]));
    assert!(!independent_table((0., 10.01, 30., 40.), &[&f]));
}
#[test]
fn independent147_all_confirmed_fields_must_be_disjoint() {
    let f = frame();
    let g = Grid {
        xs: vec![0., 5.],
        ys: vec![30., 40.],
        cells: vec![],
    };
    assert!(!independent_table((0., 9., 30., 40.), &[&f, &g]));
}
#[test]
fn independent147_invalid_or_missing_source_frames_do_not_certify_independence() {
    let f = frame();
    for b in [
        (0., 0., 30., 40.),
        (0., f32::NAN, 30., 40.),
        (0., 9., 40., 30.),
    ] {
        assert!(!independent_table(b, &[&f]));
    }
    assert!(!independent_table((0., 9., 30., 40.), &[]));
}
#[test]
fn independent147_scale_and_translation_keep_the_source_relation() {
    for (scale, dx, dy) in [(0.125, 100., -7.), (4., -50., 60.)] {
        let f = Grid {
            xs: vec![10. * scale + dx, 20. * scale + dx],
            ys: vec![30. * scale + dy, 40. * scale + dy],
            cells: vec![],
        };
        assert!(independent_table(
            (dx, 9. * scale + dx, 30. * scale + dy, 40. * scale + dy),
            &[&f]
        ));
        assert!(!independent_table(
            (dx, 25. * scale + dx, 20. * scale + dy, 50. * scale + dy),
            &[&f]
        ));
    }
}
