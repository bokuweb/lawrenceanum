use super::*;
fn compound153() -> Fixture {
    let mut s = source152(&[
        ("A", &[Some("11"), None, None]),
        ("B", &[Some("22"), Some("33"), None]),
        ("C", &[None, Some("44"), Some("55")]),
        ("D", &[None, None, Some("66")]),
    ]);
    for w in &mut s.1 {
        if w.id >= 5 {
            w.x0 += 400.;
            w.x1 += 400.;
            w.y0 -= 40.;
            w.y1 -= 40.;
            s.3[w.id] = s.3[w.id].map(|b| b - 40.);
        }
        w.source_box = [w.x0, w.y0, w.x1, w.y1];
    }
    s.2.vertical = vec![
        VRule {
            x: 0.,
            y0: 0.,
            y1: 100.,
        },
        VRule {
            x: 350.,
            y0: 0.,
            y1: 100.,
        },
        VRule {
            x: 400.,
            y0: 0.,
            y1: 80.,
        },
        VRule {
            x: 750.,
            y0: 0.,
            y1: 80.,
        },
        VRule {
            x: 375.,
            y0: 0.,
            y1: 100.,
        },
    ];
    s.2.horizontal = vec![
        HRule {
            y: 0.,
            x0: 0.,
            x1: 350.,
        },
        HRule {
            y: 80.,
            x0: 400.,
            x1: 750.,
        },
    ];
    s
}
#[test]
fn compound153_collects_later_rails_before_assigning_columns_to_either_printed_part() {
    let mut s = compound153();
    let old = serde_json::to_value(&s.1).unwrap();
    run152(&mut s);
    let ts = tables152(&s);
    assert_eq!(ts.len(), 2);
    assert!(ts.iter().all(|t| (t.rows, t.cols) == (2, 4)));
    assert_eq!(serde_json::to_value(&s.1).unwrap(), old);
    let first = ts[0];
    assert_eq!(
        first
            .cells
            .iter()
            .find(|c| c.row == 0 && c.col == 3)
            .unwrap()
            .text,
        ""
    );
    let next = ts[1];
    assert_eq!(
        next.cells
            .iter()
            .find(|c| c.row == 0 && c.col == 3)
            .unwrap()
            .text,
        "55"
    );
}
#[test]
fn compound153_missing_printed_column_divider_does_not_join_open_frames() {
    let mut s = compound153();
    s.2.vertical.pop();
    decline152(s);
}
#[test]
fn compound153_mismatched_source_frame_translation_declines() {
    let mut s = compound153();
    s.2.vertical[3].x += 1.;
    s.2.horizontal[1].x1 += 1.;
    decline152(s);
}
#[test]
fn compound153_incomplete_column_divider_does_not_supply_a_wrap() {
    let mut s = compound153();
    s.2.vertical[4].y1 -= 1.;
    decline152(s);
}
#[test]
fn compound153_multiple_possible_column_dividers_are_ambiguous() {
    let mut s = compound153();
    s.2.vertical.push(VRule {
        x: 380.,
        y0: 0.,
        y1: 100.,
    });
    decline152(s);
}
#[test]
fn compound153_existing_separate_unit_ownership_cannot_be_joined() {
    let mut s = compound153();
    if let Unit::Plain(ls) = &mut s.0[0] {
        let next = ls.split_off(2);
        s.0.push(Unit::Plain(next));
    }
    decline152(s);
}
#[test]
fn compound153_unknown_original_direction_cannot_be_borrowed_from_the_other_column() {
    let mut s = compound153();
    s.3[7] = None;
    decline152(s);
}
#[test]
fn compound153_an_original_used_word_below_the_last_frame_row_blocks_the_column_join() {
    let mut s = compound153();
    let id = s.1.len();
    s.1.push(Word {
        id,
        x0: 10.,
        y0: 105.,
        x1: 50.,
        y1: 115.,
        source_box: [10., 105., 50., 115.],
        raw: "unrelated".into(),
        text: "unrelated".into(),
        status: WordStatus::Used,
    });
    s.3.push(Some(112.));
    if let Unit::Plain(ls) = &mut s.0[0] {
        ls.push(Line::text("unrelated".into(), vec![id]));
    }
    decline152(s);
}
#[test]
fn compound153_an_original_used_word_before_the_next_frame_blocks_the_column_join() {
    let mut s = compound153();
    let id = s.1.len();
    s.1.push(Word {
        id,
        x0: 410.,
        y0: -20.,
        x1: 450.,
        y1: -10.,
        source_box: [410., -20., 450., -10.],
        raw: "other caption".into(),
        text: "other caption".into(),
        status: WordStatus::Used,
    });
    s.3.push(Some(-13.));
    if let Unit::Plain(ls) = &mut s.0[0] {
        ls.push(Line::text("other caption".into(), vec![id]));
    }
    decline152(s);
}
#[test]
fn compound153_a_source_word_crossing_the_column_divider_is_not_reassigned() {
    let mut s = compound153();
    s.1[4].x1 = 390.;
    decline152(s);
}

#[test]
fn compound153_repeated_numeric_right_edges_can_prove_separate_total_columns() {
    positive152(
        source152(&[
            ("A", &[Some("11"), None]),
            ("B", &[Some("22"), None]),
            ("C", &[None, Some("33")]),
            ("D", &[None, Some("44")]),
        ]),
        3,
    );
}
#[test]
fn compound153_distinct_source_occurrences_can_repeat_the_same_literal_amount() {
    positive152(
        source152(&[
            ("A", &[Some("11"), None]),
            ("B", &[Some("11"), None]),
            ("C", &[None, Some("11")]),
            ("D", &[None, Some("11")]),
        ]),
        3,
    );
}
#[test]
fn compound153_an_independent_total_rail_needs_original_repetition() {
    decline152(source152(&[
        ("A", &[Some("11"), Some("22"), None]),
        ("B", &[None, None, Some("33")]),
    ]));
}
#[test]
fn compound153_repeated_totals_do_not_need_to_cooccur_with_component_amounts() {
    positive152(
        source152(&[
            ("A", &[Some("11"), Some("22"), None]),
            ("B", &[None, None, Some("33")]),
            ("C", &[None, None, Some("44")]),
        ]),
        4,
    );
}
#[test]
fn compound153_nearby_different_amount_right_edges_do_not_count_as_repetition() {
    let mut s = source152(&[
        ("A", &[Some("11"), None]),
        ("B", &[Some("22"), None]),
        ("C", &[None, Some("33")]),
        ("D", &[None, Some("44")]),
    ]);
    s.1.last_mut().unwrap().x1 += 0.125;
    decline152(s);
}

#[test]
fn compound153_each_cell_can_keep_its_own_original_baseline_in_one_shared_row_band() {
    let mut s = valid152();
    s.3[4] = Some(33.);
    positive152(s, 3);
}
#[test]
fn compound153_every_cell_baseline_must_lie_in_the_whole_original_row_band() {
    let mut s = valid152();
    s.1[4].y0 = 33.;
    s.1[4].y1 = 43.;
    s.3[4] = Some(40.);
    decline152(s);
}
#[test]
fn compound153_words_within_one_numeric_cell_still_need_one_exact_baseline() {
    let mut s = overlapping_amount152();
    s.3[7] = Some(33.);
    decline152(s);
}
#[test]
fn compound153_a_label_cannot_join_two_independent_native_baselines() {
    let mut s = valid152();
    s.1[2].text = "arbitrary".into();
    s.1[2].raw = "arbitrary".into();
    let id = s.1.len();
    s.1.push(Word {
        id,
        x0: 55.,
        y0: 25.,
        x1: 65.,
        y1: 35.,
        source_box: [55., 25., 65., 35.],
        raw: "B".into(),
        text: "B".into(),
        status: WordStatus::Used,
    });
    s.3.push(Some(33.));
    if let Unit::Plain(ls) = &mut s.0[0] {
        ls[1].words.insert(1, id);
    }
    decline152(s);
}
#[test]
fn compound153_different_cell_baselines_do_not_need_a_fixed_point_tolerance() {
    for scale in [0.25, 4.] {
        let mut s = valid152();
        s.3[4] = Some(33.);
        for w in &mut s.1 {
            w.x0 *= scale;
            w.x1 *= scale;
            w.y0 *= scale;
            w.y1 *= scale;
            w.source_box = [w.x0, w.y0, w.x1, w.y1];
        }
        for b in s.3.iter_mut().flatten() {
            *b *= scale as f64;
        }
        for v in &mut s.2.vertical {
            v.x *= scale;
            v.y0 *= scale;
            v.y1 *= scale;
        }
        for h in &mut s.2.horizontal {
            h.x0 *= scale;
            h.x1 *= scale;
            h.y *= scale;
        }
        positive152(s, 3);
    }
}

fn compound_positive153(mut s: Fixture) {
    let words = serde_json::to_value(&s.1).unwrap();
    let owned =
        s.0.iter()
            .flat_map(|u| match u {
                Unit::Plain(ls) => ls.iter().flat_map(|l| l.words.clone()).collect::<Vec<_>>(),
                _ => vec![],
            })
            .collect::<Vec<_>>();
    run152(&mut s);
    let ts = tables152(&s);
    assert_eq!(ts.len(), 2);
    assert!(ts.iter().all(|t| (t.rows, t.cols) == (2, 4)));
    assert_eq!(
        ts.iter()
            .flat_map(|t| t.cells.iter().flat_map(|c| c.words.clone()))
            .collect::<Vec<_>>(),
        owned
    );
    assert_eq!(serde_json::to_value(&s.1).unwrap(), words);
}
#[test]
fn compound153_redundant_original_divider_strokes_are_not_ambiguous_ownership() {
    let mut s = compound153();
    s.2.vertical.push(s.2.vertical[4].clone());
    compound_positive153(s);
}
#[test]
fn compound153_touching_or_overlapping_original_divider_intervals_have_one_exact_union() {
    for overlap in [0., 0.25] {
        let mut s = compound153();
        s.2.vertical[4].y1 = 50. + overlap;
        s.2.vertical.push(VRule {
            x: 375.,
            y0: 50.,
            y1: 100.,
        });
        compound_positive153(s);
    }
}
#[test]
fn compound153_an_actual_original_divider_gap_is_not_filled() {
    let mut s = compound153();
    s.2.vertical[4].y1 = 49.875;
    s.2.vertical.push(VRule {
        x: 375.,
        y0: 50.,
        y1: 100.,
    });
    decline152(s);
}
#[test]
fn compound153_stroke_order_cannot_change_the_source_parent() {
    let mut s = compound153();
    s.2.vertical.reverse();
    s.2.horizontal.reverse();
    compound_positive153(s);
}
#[test]
fn compound153_unproved_last_part_does_not_leave_a_promoted_first_fragment() {
    let mut s = compound153();
    s.2.horizontal.pop();
    decline152(s);
}
#[test]
fn compound153_unproved_first_cap_does_not_promote_the_following_part() {
    let mut s = compound153();
    s.2.horizontal.remove(0);
    decline152(s);
}
#[test]
fn compound153_source_scale_translation_cannot_change_the_common_columns() {
    for (scale, dx, dy) in [(0.25, 80., -30.), (4., -120., 100.)] {
        let mut s = compound153();
        for w in &mut s.1 {
            w.x0 = w.x0 * scale + dx;
            w.x1 = w.x1 * scale + dx;
            w.y0 = w.y0 * scale + dy;
            w.y1 = w.y1 * scale + dy;
            w.source_box = [w.x0, w.y0, w.x1, w.y1];
        }
        for b in s.3.iter_mut().flatten() {
            *b = *b * scale as f64 + dy as f64;
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
        compound_positive153(s);
    }
}
#[test]
fn compound153_native_word_id_renaming_does_not_change_parent_ownership() {
    let mut s = compound153();
    let n = s.1.len();
    let old = s.3.clone();
    for w in &mut s.1 {
        let before = w.id;
        w.id = n - 1 - before;
        s.3[w.id] = old[before];
    }
    for u in &mut s.0 {
        if let Unit::Plain(ls) = u {
            for l in ls {
                for id in &mut l.words {
                    *id = n - 1 - *id;
                }
            }
        }
    }
    compound_positive153(s);
}
#[test]
fn compound153_an_unsupported_later_source_row_blocks_both_parts() {
    let mut s = compound153();
    s.1[7].text = "5x".into();
    if let Unit::Plain(ls) = &mut s.0[0] {
        ls[2].text = "C\u{3000}44\u{3000}5x".into();
    }
    decline152(s);
}
