use super::*;
fn fixture() -> (Vec<Col>, Vec<Grid>, Vec<(f32, f32, f32)>, Vec<(f32, f32, f32)>) {
    let c = vec![
        Col {
            id: 0,
            x: 5.,
            x1: 343.,
            y: 5.,
            y1: 13.,
            text: "左上の本文".into(),
        },
        Col {
            id: 1,
            x: 5.,
            x1: 343.,
            y: 490.,
            y1: 498.,
            text: "左下の本文".into(),
        },
        Col {
            id: 2,
            x: 400.,
            x1: 711.,
            y: 20.,
            y1: 28.,
            text: "右の本文".into(),
        },
    ];
    let g = vec![
        Grid {
            xs: vec![0., 150., 343.],
            ys: vec![100., 150., 200.],
            cells: vec![],
        },
        Grid {
            xs: vec![368., 520., 711.],
            ys: vec![0., 250., 510.],
            cells: vec![],
        },
    ];
    (c, g, vec![(355.5, 0., 510.)], vec![])
}
#[test]
fn original_full_height_divider_is_seen_before_tables_are_removed() {
    let (c, g, v, h) = fixture();
    assert_eq!(table_text_column_cut(&c, &g, &v, &h), Some(355.5));
}
#[test]
fn a_heading_crossing_both_columns_prevents_early_split() {
    let (mut c, g, v, h) = fixture();
    c.push(Col {
        id: 3,
        x: 100.,
        x1: 600.,
        y: 30.,
        y1: 38.,
        text: "左右の段にまたがる共通の見出し".into(),
    });
    assert_eq!(table_text_column_cut(&c, &g, &v, &h), None);
}
#[test]
fn a_horizontal_band_divider_takes_precedence() {
    let (c, g, v, mut h) = fixture();
    h.push((300., 0., 711.));
    assert_eq!(table_text_column_cut(&c, &g, &v, &h), None);
}
#[test]
fn no_table_may_cross_the_early_column_boundary() {
    let (c, mut g, v, h) = fixture();
    g.push(Grid {
        xs: vec![0., 355., 711.],
        ys: vec![250., 300., 350.],
        cells: vec![],
    });
    assert_eq!(table_text_column_cut(&c, &g, &v, &h), None);
}
#[test]
fn a_line_between_amount_columns_is_not_a_text_divider() {
    let (c, mut g, v, h) = fixture();
    g[0].xs[2] = 500.;
    assert_eq!(table_text_column_cut(&c, &g, &v, &h), None);
}
#[test]
fn only_one_side_with_a_table_is_insufficient() {
    let (c, mut g, v, h) = fixture();
    g.pop();
    assert_eq!(table_text_column_cut(&c, &g, &v, &h), None);
}
#[test]
fn short_divider_is_not_a_page_column() {
    let (c, g, mut v, h) = fixture();
    v[0].2 = 299.;
    assert_eq!(table_text_column_cut(&c, &g, &v, &h), None);
}
#[test]
fn a_divider_starting_below_the_top_content_is_rejected() {
    let (c, g, mut v, h) = fixture();
    v[0].1 = 10.;
    assert_eq!(table_text_column_cut(&c, &g, &v, &h), None);
}
#[test]
fn a_divider_ending_above_the_last_content_is_rejected() {
    let (c, g, mut v, h) = fixture();
    v[0].2 = 480.;
    assert_eq!(table_text_column_cut(&c, &g, &v, &h), None);
}
#[test]
fn ambiguous_full_height_boundaries_are_not_used() {
    let (c, g, mut v, h) = fixture();
    v.push((354., 0., 510.));
    assert_eq!(table_text_column_cut(&c, &g, &v, &h), None);
}
#[test]
fn narrow_paired_tables_are_not_changed() {
    let (c, mut g, v, h) = fixture();
    g[0].xs = vec![0., 70., 140.];
    assert_eq!(table_text_column_cut(&c, &g, &v, &h), None);
}
