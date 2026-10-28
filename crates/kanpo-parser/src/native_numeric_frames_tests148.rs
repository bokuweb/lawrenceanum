use super::*;
use crate::lines::{HRule, VRule};
fn source() -> (Vec<Unit>, Vec<Word>, PageRules, Vec<Option<f64>>) {
    let mut ws = vec![];
    for (id, (x, y, t)) in [
        (10., 5., "任意の項目Ａ"),
        (200., 5., "12,345"),
        (10., 25., "別の項目Ｂ"),
        (200., 25., "23,456"),
    ]
    .into_iter()
    .enumerate()
    {
        ws.push(Word {
            id,
            x0: x,
            y0: y,
            x1: x + 40.,
            y1: y + 10.,
            source_box: [x, y, x + 40., y + 10.],
            raw: t.into(),
            text: t.into(),
            status: WordStatus::Used,
        });
    }
    let units = vec![Unit::Plain(vec![
        Line::text("任意の項目Ａ　12,345".into(), vec![0, 1]),
        Line::text("別の項目Ｂ　23,456".into(), vec![2, 3]),
    ])];
    let rules = PageRules {
        vertical: vec![
            VRule {
                x: 0.,
                y0: 0.,
                y1: 40.,
            },
            VRule {
                x: 250.,
                y0: 0.,
                y1: 40.,
            },
        ],
        horizontal: vec![HRule {
            y: 0.,
            x0: 0.,
            x1: 250.,
        }],
        ..Default::default()
    };
    (
        units,
        ws,
        rules,
        vec![Some(12.), Some(12.), Some(32.), Some(32.)],
    )
}
fn run(s: &mut (Vec<Unit>, Vec<Word>, PageRules, Vec<Option<f64>>)) {
    restore(&mut s.0, &s.1, &s.2, &s.3);
}
fn tables(us: &[Unit]) -> Vec<&Table> {
    us.iter()
        .flat_map(|u| match u {
            Unit::Plain(ls) => ls.iter().filter_map(|l| l.table.as_ref()).collect(),
            _ => vec![],
        })
        .collect()
}
fn text(us: &[Unit]) -> String {
    us.iter().map(Unit::text).collect::<Vec<_>>().join("\n")
}
#[test]
fn numeric148_preserves_all_existing_row_text_and_word_ownership() {
    let mut s = source();
    let old = text(&s.0);
    run(&mut s);
    let ts = tables(&s.0);
    assert_eq!(ts.len(), 1);
    assert_eq!((ts[0].rows, ts[0].cols), (2, 2));
    assert_eq!(text(&s.0), old);
    assert_eq!(
        ts[0]
            .cells
            .iter()
            .flat_map(|c| c.words.clone())
            .collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );
}
#[test]
fn numeric148_column_proof_is_invariant_to_translation_and_scale() {
    for (scale, dx, dy) in [(0.25, 80., 120.), (4., -40., -80.)] {
        let mut s = source();
        for w in &mut s.1 {
            w.x0 = w.x0 * scale + dx;
            w.x1 = w.x1 * scale + dx;
            w.y0 = w.y0 * scale + dy;
            w.y1 = w.y1 * scale + dy;
            w.source_box = [w.x0, w.y0, w.x1, w.y1];
        }
        for v in &mut s.2.vertical {
            v.x = v.x * scale + dx;
            v.y0 = v.y0 * scale + dy;
            v.y1 = v.y1 * scale + dy;
        }
        for h in &mut s.2.horizontal {
            h.y = h.y * scale + dy;
            h.x0 = h.x0 * scale + dx;
            h.x1 = h.x1 * scale + dx;
        }
        for b in s.3.iter_mut().flatten() {
            *b = *b * scale as f64 + dy as f64;
        }
        run(&mut s);
        assert_eq!(tables(&s.0).len(), 1);
    }
}
#[test]
fn numeric148_lexemes_are_not_financial_titles_or_known_values() {
    let mut s = source();
    for (i, t) in [
        (0, "Arbitrary alpha"),
        (1, "9.50"),
        (2, "任意かな"),
        (3, "0.01"),
    ] {
        s.1[i].text = t.into();
    }
    let Unit::Plain(ls) = &mut s.0[0] else {
        panic!()
    };
    ls[0].text = "Arbitrary alpha　9.50".into();
    ls[1].text = "任意かな　0.01".into();
    let old = text(&s.0);
    run(&mut s);
    assert_eq!(tables(&s.0).len(), 1);
    assert_eq!(text(&s.0), old);
}
#[test]
fn numeric148_number_grammar_is_balanced_and_grouped() {
    for t in ["0", "1234", "1,234", "123,456,789", "△1.25", "-0.1"] {
        assert!(amount(t), "{t}");
    }
    for t in [
        "", "1,23", "1234,000", "1,,234", "1.", ".5", "△", "1.2.3", "(123", "12%", "１２",
    ] {
        assert!(!amount(t), "{t}");
    }
}
#[test]
fn numeric148_unproven_native_baselines_decline() {
    for missing in 0..4 {
        let mut s = source();
        s.3[missing] = None;
        let old = text(&s.0);
        run(&mut s);
        assert!(tables(&s.0).is_empty());
        assert_eq!(text(&s.0), old);
    }
}
#[test]
fn numeric148_a_second_source_owner_declines() {
    let mut s = source();
    s.0.push(Unit::Plain(vec![Line::text("別の所有者".into(), vec![1])]));
    run(&mut s);
    assert!(tables(&s.0).is_empty());
}
#[test]
fn numeric148_duplicate_source_word_ids_decline() {
    let mut s = source();
    s.1.push(s.1[0].clone());
    run(&mut s);
    assert!(tables(&s.0).is_empty());
}
#[test]
fn numeric148_intervening_used_word_cannot_be_skipped() {
    let mut s = source();
    let mut w = s.1[0].clone();
    w.id = 4;
    w.x0 = 100.;
    w.x1 = 120.;
    w.y0 = 18.;
    w.y1 = 22.;
    s.1.push(w);
    run(&mut s);
    assert!(tables(&s.0).is_empty());
}
#[test]
fn numeric148_crossing_source_word_cannot_be_skipped() {
    let mut s = source();
    let mut w = s.1[0].clone();
    w.id = 4;
    w.x0 = -10.;
    w.x1 = 20.;
    w.y0 = 18.;
    w.y1 = 22.;
    s.1.push(w);
    run(&mut s);
    assert!(tables(&s.0).is_empty());
}
#[test]
fn numeric148_differently_aligned_amount_rails_decline() {
    let mut s = source();
    s.1[3].x1 -= 1.;
    run(&mut s);
    assert!(tables(&s.0).is_empty());
}
#[test]
fn numeric148_no_common_column_cut_declines() {
    let mut s = source();
    s.1[0].x1 = 220.;
    s.1[1].x0 = 230.;
    run(&mut s);
    assert!(tables(&s.0).is_empty());
}
#[test]
fn numeric148_multiple_numeric_lanes_are_not_folded_into_the_label() {
    let mut s = source();
    s.1[0].text = "任意123".into();
    let Unit::Plain(ls) = &mut s.0[0] else {
        panic!()
    };
    ls[0].text = "任意123　12,345".into();
    run(&mut s);
    assert!(tables(&s.0).is_empty());
}
#[test]
fn numeric148_a_prose_line_is_a_scope_boundary() {
    let mut s = source();
    let Unit::Plain(ls) = &mut s.0[0] else {
        panic!()
    };
    ls.insert(1, Line::text("任意の本文".into(), vec![]));
    run(&mut s);
    assert!(tables(&s.0).is_empty());
}
#[test]
fn numeric148_a_full_top_rule_and_two_rails_are_required() {
    for mode in 0..3 {
        let mut s = source();
        match mode {
            0 => s.2.horizontal.clear(),
            1 => {
                s.2.vertical.pop();
            }
            _ => s.2.horizontal[0].x1 = 249.,
        };
        run(&mut s);
        assert!(tables(&s.0).is_empty());
    }
}
#[test]
fn numeric148_an_interior_native_rail_keeps_existing_grid_authority() {
    let mut s = source();
    s.2.vertical.push(VRule {
        x: 150.,
        y0: 0.,
        y1: 40.,
    });
    run(&mut s);
    assert!(tables(&s.0).is_empty());
}
#[test]
fn numeric148_fragmented_native_rails_join_at_exact_source_endpoints() {
    let mut s = source();
    s.2.vertical = vec![
        VRule {
            x: 0.,
            y0: 0.,
            y1: 20.,
        },
        VRule {
            x: 0.,
            y0: 20.,
            y1: 40.,
        },
        VRule {
            x: 250.,
            y0: 0.,
            y1: 20.,
        },
        VRule {
            x: 250.,
            y0: 20.,
            y1: 40.,
        },
    ];
    run(&mut s);
    assert_eq!(tables(&s.0).len(), 1);
}
#[test]
fn numeric148_an_open_bottom_uses_the_proven_baseline_and_keeps_descenders() {
    let mut s = source();
    s.2.vertical.iter_mut().for_each(|v| v.y1 = 34.);
    run(&mut s);
    assert_eq!(tables(&s.0).len(), 1);
    assert_eq!(tables(&s.0)[0].y1, 35.);
}
#[test]
fn numeric148_a_closed_bottom_cannot_be_crossed() {
    let mut s = source();
    s.2.vertical.iter_mut().for_each(|v| v.y1 = 34.);
    s.2.horizontal.push(HRule {
        y: 34.,
        x0: 0.,
        x1: 250.,
    });
    run(&mut s);
    assert!(tables(&s.0).is_empty());
}
#[test]
fn numeric148_printed_word_material_must_match_each_whole_cell() {
    let mut s = source();
    s.1[1].text = "12,346".into();
    run(&mut s);
    assert!(tables(&s.0).is_empty());
}
#[test]
fn numeric148_a_nonhorizontal_row_baseline_declines() {
    let mut s = source();
    s.3[1] = Some(12.5);
    run(&mut s);
    assert!(tables(&s.0).is_empty());
}
#[test]
fn numeric148_existing_typed_tables_are_preserved() {
    let mut s = source();
    run(&mut s);
    let before = serde_json::to_value(tables(&s.0)).unwrap();
    let old = text(&s.0);
    run(&mut s);
    assert_eq!(serde_json::to_value(tables(&s.0)).unwrap(), before);
    assert_eq!(text(&s.0), old);
}

fn signed_source() -> (Vec<Unit>, Vec<Word>, PageRules, Vec<Option<f64>>) {
    let mut s = source();
    let Unit::Plain(ls) = &mut s.0[0] else {
        panic!()
    };
    ls[0].text = "任意の項目Ａ　△12,345".into();
    let mut sign = s.1[1].clone();
    sign.id = 4;
    sign.text = "△".into();
    sign.raw = "△".into();
    sign.x0 = 180.;
    sign.x1 = 190.;
    sign.source_box = [sign.x0, sign.y0, sign.x1, sign.y1];
    s.1.push(sign);
    s.3.push(Some(12.));
    s
}
#[test]
fn numeric148_detached_sign_gets_its_unique_printed_source_owner() {
    let mut s = signed_source();
    let old = text(&s.0);
    run(&mut s);
    let t = tables(&s.0);
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].cells[1].words, vec![4, 1]);
    assert_eq!(text(&s.0), old);
}
#[test]
fn numeric148_a_literal_sign_without_native_source_cannot_be_certified() {
    let mut s = signed_source();
    s.3[4] = None;
    run(&mut s);
    assert!(tables(&s.0).is_empty());
}
#[test]
fn numeric148_two_competing_printed_signs_are_ambiguous() {
    let mut s = signed_source();
    let mut sign = s.1[4].clone();
    sign.id = 5;
    s.1.push(sign);
    s.3.push(Some(12.));
    run(&mut s);
    assert!(tables(&s.0).is_empty());
}
#[test]
fn numeric148_a_detached_sign_cannot_be_borrowed_from_another_row() {
    let mut s = signed_source();
    s.3[4] = Some(32.);
    s.1[4].y0 = 25.;
    s.1[4].y1 = 35.;
    run(&mut s);
    assert!(tables(&s.0).is_empty());
}
fn extend_row(
    s: &mut (Vec<Unit>, Vec<Word>, PageRules, Vec<Option<f64>>),
    y: f32,
    fields: &[(f32, &str)],
) {
    let start = s.1.len();
    let mut ids = vec![];
    for (x, t) in fields {
        let id = s.1.len();
        let b = [*x, y, *x + 40., y + 10.];
        s.1.push(Word {
            id,
            x0: b[0],
            y0: b[1],
            x1: b[2],
            y1: b[3],
            source_box: b,
            raw: (*t).into(),
            text: (*t).into(),
            status: WordStatus::Used,
        });
        s.3.push(Some((y + 7.) as f64));
        ids.push(id);
    }
    let Unit::Plain(ls) = &mut s.0[0] else {
        panic!()
    };
    ls.push(Line::text(
        fields
            .iter()
            .map(|(_, t)| *t)
            .collect::<Vec<_>>()
            .join("\u{3000}"),
        ids,
    ));
    for v in &mut s.2.vertical {
        v.y1 = y + 15.;
    }
    assert!(s.1.len() > start);
}
#[test]
fn whole_frame148_multi_amount_row_prevents_two_column_fragments() {
    let mut s = source();
    extend_row(
        &mut s,
        45.,
        &[(10., "別の科目"), (100., "111"), (200., "222")],
    );
    run(&mut s);
    assert!(tables(&s.0).is_empty());
}
#[test]
fn whole_frame148_an_unpaired_numeric_row_prevents_partial_authority() {
    let mut s = source();
    extend_row(&mut s, 45., &[(200., "111")]);
    run(&mut s);
    assert!(tables(&s.0).is_empty());
}
#[test]
fn whole_frame148_an_unproven_later_numeric_row_prevents_partial_authority() {
    let mut s = source();
    extend_row(&mut s, 45., &[(10., "別の科目"), (200., "111")]);
    s.3[5] = None;
    run(&mut s);
    assert!(tables(&s.0).is_empty());
}
#[test]
fn whole_frame148_different_amount_rails_across_separate_groups_decline() {
    let mut s = source();
    let Unit::Plain(ls) = &mut s.0[0] else {
        panic!()
    };
    ls.push(Line::text("区切りの本文".into(), vec![]));
    extend_row(&mut s, 45., &[(10., "別の科目"), (180., "111")]);
    extend_row(&mut s, 65., &[(10., "次の科目"), (180., "222")]);
    run(&mut s);
    assert!(tables(&s.0).is_empty());
}
#[test]
fn whole_frame148_existing_typed_grid_prevents_parallel_partial_inference() {
    let mut s = source();
    extend_row(&mut s, 45., &[(10., "既存の科目"), (200., "111")]);
    let Unit::Plain(ls) = &mut s.0[0] else {
        panic!()
    };
    let table = Table {
        x0: 0.,
        y0: 45.,
        x1: 250.,
        y1: 55.,
        rows: 1,
        cols: 2,
        cells: vec![
            TableCell {
                row: 0,
                col: 0,
                rowspan: 1,
                colspan: 1,
                x0: 0.,
                y0: 45.,
                x1: 150.,
                y1: 55.,
                text: "既存の科目".into(),
                words: vec![4],
            },
            TableCell {
                row: 0,
                col: 1,
                rowspan: 1,
                colspan: 1,
                x0: 150.,
                y0: 45.,
                x1: 250.,
                y1: 55.,
                text: "111".into(),
                words: vec![5],
            },
        ],
    };
    ls[2] = Line {
        text: table.render(),
        words: vec![],
        table: Some(table.clone()),
    };
    run(&mut s);
    assert_eq!(tables(&s.0), vec![&table]);
}

#[test]
fn whole_frame148_unowned_native_word_outside_the_local_fragment_declines() {
    let mut s = source();
    let mut w = s.1[0].clone();
    w.id = 4;
    w.text = "999".into();
    w.raw = "999".into();
    w.x0 = 100.;
    w.x1 = 140.;
    w.y0 = 45.;
    w.y1 = 55.;
    w.source_box = [w.x0, w.y0, w.x1, w.y1];
    s.1.push(w);
    s.3.push(Some(52.));
    for v in &mut s.2.vertical {
        v.y1 = 60.;
    }
    run(&mut s);
    assert!(tables(&s.0).is_empty());
}
#[test]
fn whole_frame148_an_opaque_existing_grid_retains_its_authority() {
    let mut s = source();
    for v in &mut s.2.vertical {
        v.y1 = 60.;
    }
    let table = Table {
        x0: 0.,
        y0: 45.,
        x1: 250.,
        y1: 55.,
        rows: 1,
        cols: 2,
        cells: vec![],
    };
    let Unit::Plain(ls) = &mut s.0[0] else {
        panic!()
    };
    ls.push(Line {
        text: "既存の未追跡表".into(),
        words: vec![],
        table: Some(table.clone()),
    });
    run(&mut s);
    assert_eq!(tables(&s.0), vec![&table]);
}

#[path = "native_numeric_frame_continuations_tests149.rs"]
mod continuations149;
