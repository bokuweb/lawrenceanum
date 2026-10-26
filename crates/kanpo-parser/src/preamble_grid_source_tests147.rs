use super::*;
use crate::grid::GridCell;
fn fixture() -> (ShinkyuLayout, Vec<Grid>) {
    let cells = (0..2)
        .flat_map(|x| {
            (0..2).map(move |y| GridCell {
                x0: x,
                x1: x + 1,
                y0: y,
                y1: y + 1,
            })
        })
        .collect();
    let g = Grid {
        xs: vec![0., 40., 80.],
        ys: vec![0., 40., 80.],
        cells,
    };
    (
        ShinkyuLayout {
            labels: None,
            band_boxes: None,
            preamble: vec![Col {
                id: 1,
                x: 10.,
                x1: 18.,
                y: 10.,
                y1: 26.,
                text: "任意".into(),
            }],
            after: vec![],
            before: vec![],
            trailing: vec![],
        },
        vec![g],
    )
}
fn tables(u: &Unit) -> usize {
    if let Unit::Shinkyu { preamble, .. } = u {
        preamble.iter().filter(|l| l.table.is_some()).count()
    } else {
        panic!("not a comparison unit")
    }
}
#[test]
fn preamble147_a_partial_previous_frame_does_not_become_a_new_preamble_table() {
    let (l, g) = fixture();
    let u = l.into_unit_with_source(&g, None, &[vec![1, 2]]);
    assert_eq!(tables(&u), 0);
    assert_eq!(u.text(), "任意");
}
#[test]
fn preamble147_a_complete_single_occupied_cell_preserves_its_empty_source_columns() {
    let (l, g) = fixture();
    let u = l.into_unit_with_source(&g, None, &[vec![1]]);
    assert_eq!(tables(&u), 1);
    if let Unit::Shinkyu { preamble, .. } = u {
        let t = preamble[0].table.as_ref().unwrap();
        assert_eq!((t.rows, t.cols), (2, 2));
        assert_eq!(
            t.cells
                .iter()
                .flat_map(|c| c.words.iter().copied())
                .collect::<Vec<_>>(),
            vec![1]
        );
    }
}
#[test]
fn preamble147_missing_grid_source_membership_declines() {
    let (l, g) = fixture();
    assert_eq!(tables(&l.into_unit_with_source(&g, None, &[])), 0);
}
#[test]
fn preamble147_duplicate_grid_source_membership_declines() {
    let (l, g) = fixture();
    assert_eq!(tables(&l.into_unit_with_source(&g, None, &[vec![1, 1]])), 0);
}
#[test]
fn preamble147_unavailable_source_certificate_does_not_authorize_promotion() {
    let (l, g) = fixture();
    assert_eq!(tables(&l.into_unit(&g, None)), 0);
}
#[test]
fn preamble147_source_id_bijection_does_not_change_the_rule() {
    let (mut l, g) = fixture();
    l.preamble[0].id = 1001;
    assert_eq!(tables(&l.into_unit_with_source(&g, None, &[vec![1001]])), 1);
}
#[test]
fn preamble147_membership_is_exact_in_both_directions() {
    let (mut l, g) = fixture();
    let mut c = l.preamble[0].clone();
    c.id = 2;
    c.x = 20.;
    c.x1 = 28.;
    l.preamble.push(c);
    assert_eq!(tables(&l.into_unit_with_source(&g, None, &[vec![1]])), 0);
}
