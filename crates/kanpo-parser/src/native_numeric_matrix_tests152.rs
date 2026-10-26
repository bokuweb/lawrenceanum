use super::*;
use crate::lines::{HRule, VRule};
type Fixture = (Vec<Unit>, Vec<Word>, PageRules, Vec<Option<f64>>);
fn source152(rows: &[(&str, &[Option<&str>])]) -> Fixture {
    let mut words = vec![];
    let mut lines = vec![];
    let mut baselines = vec![];
    let columns = rows.iter().map(|r| r.1.len()).max().unwrap();
    for (row, (label, values)) in rows.iter().enumerate() {
        let y = 5. + 20. * row as f32;
        let mut ids = vec![];
        let mut fields = vec![*label];
        for (x, text) in std::iter::once((10., *label)).chain(
            values
                .iter()
                .enumerate()
                .filter_map(|(i, s)| s.map(|s| (100. + 100. * i as f32, s))),
        ) {
            let id = words.len();
            words.push(Word {
                id,
                x0: x,
                y0: y,
                x1: x + 40.,
                y1: y + 10.,
                source_box: [x, y, x + 40., y + 10.],
                raw: text.into(),
                text: text.into(),
                status: WordStatus::Used,
            });
            baselines.push(Some((y + 7.) as f64));
            ids.push(id);
        }
        fields.extend(values.iter().filter_map(|s| *s));
        lines.push(Line::text(fields.join("\u{3000}"), ids));
    }
    let right = 100. * columns as f32 + 50.;
    let bottom = rows.len() as f32 * 20.;
    let rules = PageRules {
        vertical: vec![
            VRule {
                x: 0.,
                y0: 0.,
                y1: bottom,
            },
            VRule {
                x: right,
                y0: 0.,
                y1: bottom,
            },
        ],
        horizontal: vec![
            HRule {
                y: 0.,
                x0: 0.,
                x1: right,
            },
            HRule {
                y: bottom,
                x0: 0.,
                x1: right,
            },
        ],
        ..Default::default()
    };
    (vec![Unit::Plain(lines)], words, rules, baselines)
}
fn valid152() -> Fixture {
    source152(&[
        ("arbitrary A", &[Some("11"), None]),
        ("arbitrary B", &[Some("22"), Some("33")]),
        ("arbitrary C", &[None, Some("44")]),
    ])
}
fn run152(s: &mut Fixture) {
    restore(&mut s.0, &s.1, &s.2, &s.3);
    restore_matrix(&mut s.0, &s.1, &s.2, &s.3);
}
fn tables152(s: &Fixture) -> Vec<&Table> {
    s.0.iter()
        .flat_map(|u| match u {
            Unit::Plain(ls) => ls.iter().filter_map(|l| l.table.as_ref()).collect(),
            _ => vec![],
        })
        .collect()
}
fn units152(us: &[Unit]) -> serde_json::Value {
    serde_json::Value::Array(us.iter().map(|u| match u { Unit::Plain(ls)=>serde_json::json!({"plain":ls}), Unit::Shinkyu{labels,preamble,after,before,outside}=>serde_json::json!({"labels":labels,"preamble":preamble,"after":after,"before":before,"outside":outside}) }).collect())
}
fn decline152(mut s: Fixture) {
    let old = units152(&s.0);
    run152(&mut s);
    assert_eq!(units152(&s.0), old);
}
fn positive152(mut s: Fixture, cols: usize) {
    let ids: Vec<_> =
        s.0.iter()
            .flat_map(|u| match u {
                Unit::Plain(ls) => ls.iter().flat_map(|l| l.words.clone()).collect(),
                _ => vec![],
            })
            .collect();
    let words = serde_json::to_value(&s.1).unwrap();
    run152(&mut s);
    let ts = tables152(&s);
    assert_eq!(ts.len(), 1);
    assert_eq!(ts[0].cols, cols);
    assert_eq!(
        ts[0]
            .cells
            .iter()
            .flat_map(|c| c.words.iter().copied())
            .collect::<Vec<_>>(),
        ids
    );
    assert_eq!(serde_json::to_value(&s.1).unwrap(), words);
}
#[test]
fn matrix152_complete_two_rail_frame_keeps_blank_physical_amount_columns() {
    let mut s = valid152();
    run152(&mut s);
    let ts = tables152(&s);
    assert_eq!(ts.len(), 1);
    assert_eq!((ts[0].rows, ts[0].cols), (3, 3));
    assert_eq!(
        ts[0]
            .cells
            .iter()
            .find(|c| c.row == 2 && c.col == 1)
            .unwrap()
            .text,
        ""
    );
    assert_eq!(
        ts[0]
            .cells
            .iter()
            .find(|c| c.row == 2 && c.col == 2)
            .unwrap()
            .text,
        "44"
    );
}
#[test]
fn matrix152_three_connected_amount_rails_need_no_fixed_column_budget() {
    positive152(
        source152(&[
            ("A", &[Some("11"), Some("22"), None]),
            ("B", &[None, Some("33"), Some("44")]),
            ("C", &[Some("55"), None, None]),
        ]),
        4,
    );
}
#[test]
fn matrix152_nonnumeric_heading_rows_keep_their_original_order_and_words() {
    positive152(
        source152(&[
            ("first heading", &[None, None]),
            ("A", &[Some("11"), None]),
            ("other heading", &[None, None]),
            ("B", &[Some("22"), Some("33")]),
            ("C", &[None, Some("44")]),
        ]),
        3,
    );
}
#[test]
fn matrix152_scale_translation_and_word_id_renaming_preserve_the_same_matrix() {
    for (scale, dx, dy) in [(0.25, 80., -30.), (4., -120., 100.)] {
        let mut s = valid152();
        let n = s.1.len();
        let old = s.3.clone();
        for w in &mut s.1 {
            let before = w.id;
            w.id = n - 1 - before;
            w.x0 = w.x0 * scale + dx;
            w.x1 = w.x1 * scale + dx;
            w.y0 = w.y0 * scale + dy;
            w.y1 = w.y1 * scale + dy;
            w.source_box = [w.x0, w.y0, w.x1, w.y1];
            s.3[w.id] = old[before].map(|y| y * scale as f64 + dy as f64);
        }
        for u in &mut s.0 {
            let Unit::Plain(ls) = u else { panic!() };
            for l in ls {
                for id in &mut l.words {
                    *id = n - 1 - *id;
                }
            }
        }
        for v in &mut s.2.vertical {
            v.x = v.x * scale + dx;
            v.y0 = v.y0 * scale + dy;
            v.y1 = v.y1 * scale + dy;
        }
        for h in &mut s.2.horizontal {
            h.x0 = h.x0 * scale + dx;
            h.x1 = h.x1 * scale + dx;
            h.y = h.y * scale + dy;
        }
        positive152(s, 3);
    }
}
#[test]
fn matrix152_disconnected_numeric_rails_do_not_prove_one_column_matrix() {
    decline152(source152(&[
        ("A", &[Some("11"), None]),
        ("B", &[None, Some("22")]),
    ]));
}
#[test]
fn matrix152_a_disconnected_later_rail_blocks_all_easy_two_column_fragments() {
    decline152(source152(&[
        ("A", &[Some("11"), Some("22"), None]),
        ("B", &[Some("33"), None, None]),
        ("C", &[None, None, Some("44")]),
    ]));
}
#[test]
fn matrix152_any_missing_or_invalid_native_baseline_blocks_the_entire_frame() {
    for y in [None, Some(f64::NAN), Some(f64::INFINITY), Some(200.)] {
        let mut s = valid152();
        s.3[4] = y;
        decline152(s);
    }
}
#[test]
fn matrix152_one_unsupported_later_row_blocks_partial_frame_promotion() {
    let mut s = valid152();
    let Unit::Plain(ls) = &mut s.0[0] else {
        panic!()
    };
    ls[2].text = "arbitrary C\u{3000}4x".into();
    decline152(s);
}
#[test]
fn matrix152_repeated_original_owners_are_not_available_to_another_cell() {
    let mut s = valid152();
    let Unit::Plain(ls) = &mut s.0[0] else {
        panic!()
    };
    ls[2].words.push(0);
    decline152(s);
}
#[test]
fn matrix152_an_orphan_native_word_anywhere_in_the_parent_frame_blocks_it() {
    let mut s = valid152();
    let id = s.1.len();
    s.1.push(Word {
        id,
        x0: 60.,
        y0: 42.,
        x1: 80.,
        y1: 48.,
        source_box: [60., 42., 80., 48.],
        text: "orphan".into(),
        raw: "orphan".into(),
        status: WordStatus::Used,
    });
    s.3.push(Some(45.));
    decline152(s);
}
#[test]
fn matrix152_missing_original_frame_walls_or_both_open_ends_decline() {
    for id in 0..2 {
        let mut s = valid152();
        s.2.vertical.remove(id);
        decline152(s);
    }
    let mut s = valid152();
    s.2.horizontal.clear();
    decline152(s);
}
#[test]
fn matrix152_a_crossing_native_word_does_not_belong_to_the_frame() {
    let mut s = valid152();
    s.1[0].x0 = -5.;
    s.1[0].source_box[0] = -5.;
    decline152(s);
}
#[test]
fn matrix152_inconsistent_source_text_cannot_be_recovered_from_rendered_numbers() {
    let mut s = valid152();
    s.1[4].text = "999".into();
    decline152(s);
}
#[test]
fn matrix152_a_value_without_a_label_cannot_supply_parent_frame_authority() {
    let mut s = valid152();
    let Unit::Plain(ls) = &mut s.0[0] else {
        panic!()
    };
    ls[2].words.remove(0);
    ls[2].text = "44".into();
    s.1[5].status = WordStatus::Noise;
    decline152(s);
}
#[test]
fn matrix152_distinct_lines_with_overlapping_native_rows_decline() {
    let mut s = valid152();
    for id in [5, 6] {
        s.1[id].y0 = 24.;
        s.1[id].y1 = 34.;
        s.1[id].source_box[1] = 24.;
        s.1[id].source_box[3] = 34.;
        s.3[id] = Some(31.);
    }
    decline152(s);
}
#[test]
fn matrix152_native_columns_need_a_common_nonoverlapping_numeric_gap() {
    let mut s = valid152();
    s.1[6].x0 = 120.;
    s.1[6].source_box[0] = 120.;
    decline152(s);
}
#[test]
fn matrix152_no_single_source_unit_can_borrow_numeric_rows_from_another() {
    let mut s = valid152();
    let Unit::Plain(ls) = &mut s.0[0] else {
        panic!()
    };
    let rest = ls.split_off(1);
    s.0.push(Unit::Plain(rest));
    decline152(s);
}
#[test]
fn matrix152_an_existing_typed_grid_retains_its_own_authority() {
    let mut s = valid152();
    let Unit::Plain(ls) = &mut s.0[0] else {
        panic!()
    };
    ls[0].table = Some(Table {
        x0: 0.,
        y0: 5.,
        x1: 250.,
        y1: 15.,
        rows: 1,
        cols: 1,
        cells: vec![TableCell {
            row: 0,
            col: 0,
            rowspan: 1,
            colspan: 1,
            x0: 0.,
            y0: 5.,
            x1: 250.,
            y1: 15.,
            text: ls[0].text.clone(),
            words: ls[0].words.clone(),
        }],
    });
    ls[0].words.clear();
    decline152(s);
}
#[test]
fn matrix152_numbered_or_signed_unproved_labels_are_not_special_cased() {
    let mut s = valid152();
    let Unit::Plain(ls) = &mut s.0[0] else {
        panic!()
    };
    ls[0].text = "A 9\u{3000}11".into();
    s.1[0].text = "A 9".into();
    decline152(s);
    let mut s = valid152();
    let Unit::Plain(ls) = &mut s.0[0] else {
        panic!()
    };
    ls[0].text = "arbitrary A\u{3000}△11".into();
    decline152(s);
}

#[test]
fn matrix152_comparison_roles_cannot_supply_a_plain_numeric_matrix() {
    let mut s = valid152();
    let Unit::Plain(ls) = s.0.remove(0) else {
        panic!()
    };
    s.0.push(Unit::Shinkyu {
        labels: None,
        preamble: ls,
        after: vec![],
        before: vec![],
        outside: vec![],
    });
    decline152(s);
}
fn overlapping_amount152() -> Fixture {
    let mut s = valid152();
    s.1[3].text = "1".into();
    s.1[3].raw = "1".into();
    s.1[3].x1 = 110.;
    s.1[3].source_box[2] = 110.;
    let mut added = vec![];
    for (x0, x1, t) in [(103., 123., ","), (116., 140., "234")] {
        let id = s.1.len();
        s.1.push(Word {
            id,
            x0,
            y0: 25.,
            x1,
            y1: 35.,
            source_box: [x0, 25., x1, 35.],
            text: t.into(),
            raw: t.into(),
            status: WordStatus::Used,
        });
        s.3.push(Some(32.));
        added.push(id);
    }
    let Unit::Plain(ls) = &mut s.0[0] else {
        panic!()
    };
    ls[1].text = "arbitrary B\u{3000}1,234\u{3000}33".into();
    ls[1].words.splice(2..2, added);
    s
}
#[test]
fn matrix152_same_cell_glyph_boxes_can_overlap_while_source_origins_advance() {
    positive152(overlapping_amount152(), 3);
}
#[test]
fn matrix152_reversed_or_identical_glyph_origins_do_not_prove_source_word_order() {
    for x in [99., 100.] {
        let mut s = overlapping_amount152();
        s.1[7].x0 = x;
        s.1[7].source_box[0] = x;
        decline152(s);
    }
}
fn split_rails152(s: &mut Fixture, cut: f32, overlap: f32) {
    let old = std::mem::take(&mut s.2.vertical);
    for v in old {
        s.2.vertical.push(VRule {
            x: v.x,
            y0: v.y0,
            y1: cut + overlap * 0.5,
        });
        s.2.vertical.push(VRule {
            x: v.x,
            y0: cut - overlap * 0.5,
            y1: v.y1,
        });
    }
}
#[test]
fn matrix152_overlapping_original_rail_segments_cannot_hide_later_parent_columns() {
    for overlap in [0.0002, 0.5, 5.] {
        let mut s = source152(&[
            ("A", &[Some("11"), None, None]),
            ("B", &[Some("22"), Some("33"), None]),
            ("C", &[None, Some("44"), None]),
            ("D", &[None, None, Some("55")]),
        ]);
        split_rails152(&mut s, 60., overlap);
        decline152(s);
    }
}
#[test]
fn matrix152_exact_continuous_rail_coverage_preserves_every_row_in_the_frame() {
    for overlap in [0.0002, 0.5, 5.] {
        let mut s = valid152();
        split_rails152(&mut s, 40., overlap);
        positive152(s, 3);
    }
}
#[test]
fn matrix152_redundant_nested_source_strokes_do_not_create_conflicting_partial_frames() {
    let mut s = valid152();
    for v in s.2.vertical.clone() {
        s.2.vertical.push(VRule {
            x: v.x,
            y0: v.y0,
            y1: 40.,
        });
    }
    positive152(s, 3);
}
#[test]
fn matrix152_overlapping_stroke_order_and_affine_coordinates_cannot_change_scope() {
    for scale in [0.25, 4.] {
        let mut s = valid152();
        split_rails152(&mut s, 40., 0.5);
        s.2.vertical.reverse();
        for w in &mut s.1 {
            w.x0 = w.x0 * scale - 90.;
            w.x1 = w.x1 * scale - 90.;
            w.y0 = w.y0 * scale + 70.;
            w.y1 = w.y1 * scale + 70.;
            w.source_box = [w.x0, w.y0, w.x1, w.y1];
        }
        for y in &mut s.3 {
            *y = y.map(|y| y * scale as f64 + 70.);
        }
        for v in &mut s.2.vertical {
            v.x = v.x * scale - 90.;
            v.y0 = v.y0 * scale + 70.;
            v.y1 = v.y1 * scale + 70.;
        }
        for h in &mut s.2.horizontal {
            h.x0 = h.x0 * scale - 90.;
            h.x1 = h.x1 * scale - 90.;
            h.y = h.y * scale + 70.;
        }
        positive152(s, 3);
    }
}
#[test]
fn matrix152_real_gaps_are_not_completed_by_a_point_tolerance() {
    for gap in [0.0002, 0.5, 5.] {
        let mut s = valid152();
        split_rails152(&mut s, 40., -gap);
        let frames = super::matrix::covered_frames(&s.2);
        assert_eq!(frames.len(), 2);
        assert!(frames.iter().any(|f| f.y1 < 40.));
        assert!(frames.iter().any(|f| f.y0 > 40.));
    }
}
#[test]
fn matrix152_nearby_noncollinear_strokes_cannot_be_snapped_into_a_frame() {
    let mut s = valid152();
    split_rails152(&mut s, 40., 0.5);
    s.2.vertical[1].x += 0.0002;
    decline152(s);
}
fn continued_parent152() -> Fixture {
    source152(&[
        ("A", &[Some("11"), None, None]),
        ("B", &[Some("22"), Some("33"), None]),
        ("C", &[None, Some("44"), None]),
        ("D", &[None, None, Some("55")]),
    ])
}
#[test]
fn matrix152_a_gap_in_source_strokes_cannot_hide_a_later_parent_amount_rail() {
    for gap in [0.0002, 0.5, 5.] {
        let mut s = continued_parent152();
        split_rails152(&mut s, 60., -gap);
        decline152(s);
    }
}
#[test]
fn matrix152_unknown_continuation_evidence_cannot_be_ignored_across_a_stroke_gap() {
    let mut s = source152(&[
        ("A", &[Some("11"), None]),
        ("B", &[Some("22"), Some("33")]),
        ("C", &[None, Some("44")]),
        ("D", &[Some("55"), None]),
    ]);
    split_rails152(&mut s, 60., -0.5);
    let last = s.3.len() - 1;
    s.3[last] = None;
    decline152(s);
}
#[test]
fn matrix152_an_original_closed_wall_keeps_an_independent_later_frame_outside_scope() {
    let mut s = continued_parent152();
    split_rails152(&mut s, 60., -0.5);
    s.2.horizontal.push(HRule {
        y: 59.75,
        x0: 0.,
        x1: 350.,
    });
    run152(&mut s);
    let ts = tables152(&s);
    assert_eq!(ts.len(), 1);
    assert_eq!((ts[0].rows, ts[0].cols), (3, 3));
    let Unit::Plain(ls) = &s.0[0] else { panic!() };
    assert_eq!(ls.last().unwrap().text, "D\u{3000}55");
    assert!(ls.last().unwrap().table.is_none());
}
#[test]
fn matrix152_matching_visible_rails_do_not_supply_a_missing_parent_boundary() {
    let mut s = valid152();
    split_rails152(&mut s, 40., -0.5);
    decline152(s);
}
#[test]
fn matrix152_an_open_end_does_not_prove_columns_in_an_unseen_continuation() {
    for cap in 0..2 {
        let mut s = valid152();
        s.2.horizontal.remove(cap);
        decline152(s);
    }
}
