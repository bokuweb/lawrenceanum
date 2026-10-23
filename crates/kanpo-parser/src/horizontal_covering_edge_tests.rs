use super::*;
fn setup() -> (Vec<f32>, Vec<Col>, Vec<(f32, f32, f32)>, Vec<(f32, f32, f32)>) {
    let xs = vec![40., 70., 100., 130.];
    let v = vec![
        (10., 0., 180.),
        (40., 0., 100.),
        (70., 0., 100.),
        (100., 0., 100.),
        (130., 0., 100.),
        (160., 0., 180.),
    ];
    let h = vec![(20., 40., 160.), (40., 40., 160.), (60., 40., 160.), (80., 40., 160.)];
    let mut words = vec![];
    for r in 0..5 {
        for c in 0..4 {
            words.push(Col {
                id: words.len(),
                x: 45. + c as f32 * 30.,
                x1: 56. + c as f32 * 30.,
                y: 5. + r as f32 * 20.,
                y1: 13. + r as f32 * 20.,
                text: "1.5".into(),
            });
        }
    }
    (xs, words, v, h)
}
#[test]
fn enclosing_edges_add_only_the_last_printed_numeric_column() {
    let (xs, w, v, h) = setup();
    assert_eq!(
        covering_numeric_table_edges(&xs, &w, &v, &h, 0., 100.),
        Some(vec![10., 40., 70., 100., 130., 160.])
    );
}
#[test]
fn full_assembly_keeps_source_ids_and_drops_the_empty_left_column() {
    let (_, w, v, h) = setup();
    let original = w.clone();
    let (ts, rest) = rule_column_tables(w, &v, &h);
    assert!(rest.is_empty());
    assert_eq!(ts.len(), 1);
    assert_eq!((ts[0].x0, ts[0].x1, ts[0].rows, ts[0].cols), (40., 160., 5, 4));
    let mut ids: Vec<_> = ts[0].cells.iter().flat_map(|c| &c.words).copied().collect();
    ids.sort();
    assert_eq!(ids, original.iter().map(|w| w.id).collect::<Vec<_>>());
}
#[test]
fn lacking_horizontal_extension_is_not_expanded() {
    let (xs, w, v, mut h) = setup();
    for r in &mut h {
        r.2 = 130.;
    }
    assert!(covering_numeric_table_edges(&xs, &w, &v, &h, 0., 100.).is_none());
}
#[test]
fn one_horizontal_boundary_is_not_a_row_series() {
    let (xs, w, v, mut h) = setup();
    h.truncate(1);
    assert!(covering_numeric_table_edges(&xs, &w, &v, &h, 0., 100.).is_none());
}
#[test]
fn repeated_horizontal_fragments_are_not_distinct_rows() {
    let (xs, w, v, h) = setup();
    let h = vec![h[0]; 4];
    assert!(covering_numeric_table_edges(&xs, &w, &v, &h, 0., 100.).is_none());
}
#[test]
fn both_enclosing_edges_are_required() {
    let (xs, w, mut v, h) = setup();
    v.remove(0);
    assert!(covering_numeric_table_edges(&xs, &w, &v, &h, 0., 100.).is_none());
}
#[test]
fn shorter_edge_does_not_cover_seed() {
    let (xs, w, mut v, h) = setup();
    v[5].2 = 90.;
    assert!(covering_numeric_table_edges(&xs, &w, &v, &h, 0., 100.).is_none());
}
#[test]
fn farther_margin_is_not_a_missing_column() {
    let (xs, w, mut v, h) = setup();
    v[5].0 = 210.;
    assert!(covering_numeric_table_edges(&xs, &w, &v, &h, 0., 100.).is_none());
}
#[test]
fn a_partial_intervening_fence_is_ambiguous() {
    let (xs, w, mut v, h) = setup();
    v.push((145., 0., 70.));
    assert!(covering_numeric_table_edges(&xs, &w, &v, &h, 0., 100.).is_none());
}
#[test]
fn a_later_article_in_the_added_band_is_not_taken() {
    let (xs, mut w, v, h) = setup();
    w[3].text = "説明文。".into();
    assert!(covering_numeric_table_edges(&xs, &w, &v, &h, 0., 100.).is_none());
}
#[test]
fn an_added_label_band_is_not_taken() {
    let (xs, mut w, v, h) = setup();
    w.push(Col {
        id: 100,
        x: 15.,
        x1: 25.,
        y: 5.,
        y1: 13.,
        text: "備考".into(),
    });
    assert!(covering_numeric_table_edges(&xs, &w, &v, &h, 0., 100.).is_none());
}
#[test]
fn values_in_different_rows_are_not_taken() {
    let (xs, mut w, v, h) = setup();
    for c in w.iter_mut().skip(3).step_by(4) {
        c.y += 2.;
        c.y1 += 2.;
    }
    assert!(covering_numeric_table_edges(&xs, &w, &v, &h, 0., 100.).is_none());
}
#[test]
fn crossing_text_cannot_be_absorbed() {
    let (xs, mut w, v, h) = setup();
    w[3].x = 125.;
    assert!(covering_numeric_table_edges(&xs, &w, &v, &h, 0., 100.).is_none());
}
#[test]
fn just_two_source_rows_are_insufficient() {
    let (xs, mut w, v, h) = setup();
    w.truncate(8);
    assert!(covering_numeric_table_edges(&xs, &w, &v, &h, 0., 100.).is_none());
}
#[test]
fn three_original_numeric_columns_are_required() {
    let (xs, mut w, v, h) = setup();
    for c in w.iter_mut().skip(1).step_by(4) {
        c.text = "記事名".into();
    }
    assert!(covering_numeric_table_edges(&xs, &w, &v, &h, 0., 100.).is_none());
}
#[test]
fn two_unchanged_existing_columns_are_not_enough() {
    let (mut xs, w, v, h) = setup();
    xs.pop();
    assert!(covering_numeric_table_edges(&xs, &w, &v, &h, 0., 100.).is_none());
}
#[test]
fn an_empty_added_band_does_not_expand_the_table() {
    let (xs, mut w, v, h) = setup();
    w.retain(|c| c.x < 130.);
    assert!(covering_numeric_table_edges(&xs, &w, &v, &h, 0., 100.).is_none());
}
#[test]
fn adjacent_article_stays_outside_the_existing_numeric_table() {
    let (_, mut w, v, h) = setup();
    for c in w.iter_mut().skip(3).step_by(4) {
        c.text = "隣接する公告の本文。".into();
        c.x1 = c.x + 45.;
    }
    let (ts, rest) = rule_column_tables(w, &v, &h);
    assert_eq!(ts.len(), 1);
    assert_eq!((ts[0].x0, ts[0].x1, ts[0].cols), (40., 130., 3));
    assert_eq!(rest.len(), 5);
    assert!(rest.iter().all(|w| w.text == "隣接する公告の本文。"));
}
#[test]
fn the_note_below_a_continuing_frame_remains_outside() {
    let (_, mut w, v, h) = setup();
    w.push(Col {
        id: 99,
        x: 40.,
        x1: 150.,
        y: 120.,
        y1: 128.,
        text: "備考。この表の下には説明文がある。".into(),
    });
    let (ts, rest) = rule_column_tables(w, &v, &h);
    assert_eq!(ts.len(), 1);
    assert_eq!((ts[0].y0, ts[0].y1, ts[0].cols), (0., 100., 4));
    assert_eq!(rest.len(), 1);
    assert_eq!(rest[0].id, 99);
}
