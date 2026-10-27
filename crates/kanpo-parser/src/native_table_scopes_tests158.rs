use super::*;
use crate::{
    trace::{table_marker, Word, WordStatus},
    Block, Document,
};
fn table(x: f32, y: f32) -> Table {
    Table {
        x0: x,
        y0: y,
        x1: x + 80.,
        y1: y + 40.,
        rows: 2,
        cols: 2,
        cells: vec![
            crate::trace::TableCell {
                row: 0,
                col: 0,
                rowspan: 1,
                colspan: 1,
                x0: x,
                y0: y,
                x1: x + 40.,
                y1: y + 20.,
                text: "same".into(),
                words: vec![],
            },
            crate::trace::TableCell {
                row: 0,
                col: 1,
                rowspan: 1,
                colspan: 1,
                x0: x + 40.,
                y0: y,
                x1: x + 80.,
                y1: y + 20.,
                text: "same".into(),
                words: vec![],
            },
            crate::trace::TableCell {
                row: 1,
                col: 0,
                rowspan: 1,
                colspan: 1,
                x0: x,
                y0: y + 20.,
                x1: x + 40.,
                y1: y + 40.,
                text: "tail".into(),
                words: vec![],
            },
        ],
    }
}
fn rules() -> PageRules {
    PageRules {
        vertical: [0., 100., 200.]
            .map(|x| crate::lines::VRule {
                x,
                y0: 0.,
                y1: if x == 100. { 80. } else { 100. },
            })
            .into(),
        horizontal: vec![crate::lines::HRule {
            y: 80.,
            x0: 0.,
            x1: 200.,
        }],
        ..Default::default()
    }
}
fn page() -> PageTrace {
    test_page(vec![table(10., 10.), table(110., 10.)], vec![], vec![])
}
fn scopes(p: PageTrace, r: PageRules) -> Scopes {
    Scopes::for_pages(&[p], &[Some(r)])
}
fn counts(d: &Document) -> Vec<usize> {
    d.blocks
        .iter()
        .flat_map(|b| match b {
            Block::Paragraph { runs } => runs
                .iter()
                .filter_map(|r| r.table.as_ref())
                .map(|t| t.rows.len())
                .collect::<Vec<_>>(),
            _ => vec![],
        })
        .collect()
}
#[test]
fn scope158_positive_outer_columns_keep_identical_data_rows() {
    let s = scopes(page(), rules());
    assert!(!s.may_join(0, 1));
    assert!(!s.may_join(1, 0));
    let text = format!("{}\n{}", table_marker(0), table_marker(1));
    for d in [
        Document::from_source_text_with_tables(&text, &s),
        Document::from_source_paragraph_text_with_tables(&text, &s),
    ] {
        assert_eq!(counts(&d), vec![2, 2]);
    }
    assert_eq!(
        counts(&Document::from_text_with_tables(&text, s.tables())),
        vec![3]
    );
}
#[test]
fn scope158_trace_tables_alone_do_not_prove_parent_independence() {
    let s = Scopes::for_pages(&[page()], &[]);
    assert!(s.may_join(0, 1));
    let mut r = rules();
    r.vertical.clear();
    assert!(scopes(page(), r).may_join(0, 1));
}
#[test]
fn scope158_a_page_border_at_a_different_frontier_is_not_an_outer_cell() {
    let mut r = rules();
    r.vertical[0].y0 = -5.;
    r.vertical[2].y0 = -5.;
    assert!(scopes(page(), r).may_join(0, 1));
}
#[test]
fn scope158_table_edges_are_not_strict_outer_parent_walls() {
    for side in [0, 1] {
        let mut p = page();
        if side == 0 {
            p.tables[0].x0 = 0.;
        } else {
            p.tables[1].x1 = 200.;
        }
        assert!(scopes(p, rules()).may_join(0, 1));
    }
}
#[test]
fn scope158_continuous_supports_can_be_segmented_but_gaps_never_bridge() {
    let mut r = rules();
    let v = r.vertical.remove(1);
    r.vertical.extend([
        crate::lines::VRule {
            x: v.x,
            y0: 0.,
            y1: 30.,
        },
        crate::lines::VRule {
            x: v.x,
            y0: 30.,
            y1: 80.,
        },
    ]);
    assert!(!scopes(page(), r.clone()).may_join(0, 1));
    r.vertical.last_mut().unwrap().y0 = 30.00001;
    assert!(scopes(page(), r).may_join(0, 1));
}
#[test]
fn scope158_parent_can_close_before_its_outer_walls_only_at_a_real_crossbar() {
    let mut r = rules();
    r.vertical[1].y1 = 60.;
    assert!(scopes(page(), r.clone()).may_join(0, 1));
    r.horizontal.push(crate::lines::HRule {
        y: 60.,
        x0: 0.,
        x1: 200.,
    });
    assert!(!scopes(page(), r.clone()).may_join(0, 1));
    r.horizontal.last_mut().unwrap().x1 = 199.999;
    assert!(scopes(page(), r).may_join(0, 1));
}
#[test]
fn scope158_crossing_original_used_glyph_blocks_a_column_certificate() {
    let mut p = page();
    p.words.push(Word {
        id: 0,
        x0: 99.,
        y0: 20.,
        x1: 101.,
        y1: 30.,
        source_box: [99., 20., 101., 30.],
        raw: "arbitrary".into(),
        text: "arbitrary".into(),
        status: WordStatus::Used,
    });
    assert!(scopes(p, rules()).may_join(0, 1));
}
#[test]
fn scope158_duplicate_rules_are_idempotent_and_invalid_geometry_is_not_evidence() {
    let mut r = rules();
    r.vertical.extend(r.vertical.clone());
    assert!(!scopes(page(), r.clone()).may_join(0, 1));
    r.vertical[0].x = f32::NAN;
    assert!(scopes(page(), r).may_join(0, 1));
}
#[test]
fn scope158_translation_scaling_and_content_do_not_change_source_parent_relation() {
    for (scale, dx, dy) in [(1., 0., 0.), (2., 17., 31.), (0.5, -30., 9.)] {
        let mut p = page();
        let mut r = rules();
        for t in &mut p.tables {
            for x in [&mut t.x0, &mut t.x1] {
                *x = *x * scale + dx;
            }
            for y in [&mut t.y0, &mut t.y1] {
                *y = *y * scale + dy;
            }
            for c in &mut t.cells {
                for x in [&mut c.x0, &mut c.x1] {
                    *x = *x * scale + dx;
                }
                for y in [&mut c.y0, &mut c.y1] {
                    *y = *y * scale + dy;
                }
                c.text = "完全に異なる任意の値".into();
            }
        }
        for v in &mut r.vertical {
            v.x = v.x * scale + dx;
            v.y0 = v.y0 * scale + dy;
            v.y1 = v.y1 * scale + dy;
        }
        for h in &mut r.horizontal {
            h.y = h.y * scale + dy;
            h.x0 = h.x0 * scale + dx;
            h.x1 = h.x1 * scale + dx;
        }
        assert!(!scopes(p, r).may_join(0, 1));
    }
}

#[test]
fn scope158_unknown_or_repeated_source_id_cannot_borrow_authority() {
    let s = scopes(page(), rules());
    for (a, b) in [(0, 0), (2, 0), (0, 2), (usize::MAX, 0)] {
        assert!(!s.may_join(a, b));
    }
}

#[test]
fn scope158_prior_joins_keep_every_original_marker_scope() {
    let mut p = page();
    p.tables.insert(1, table(310., 10.));
    let s = scopes(p, rules());
    assert!(s.may_join(0, 1));
    assert!(s.may_join(1, 2));
    assert!(!s.may_join(0, 2));
    let text = (0..3).map(table_marker).collect::<Vec<_>>().join("\n");
    assert_eq!(
        counts(&Document::from_source_text_with_tables(&text, &s)),
        vec![3, 2]
    );
}
#[test]
fn scope158_original_rule_scope_does_not_leak_to_another_page() {
    let s = Scopes::for_pages(&[page(), page()], &[Some(rules())]);
    assert!(!s.may_join(0, 1));
    assert!(s.may_join(2, 3));
    assert!(s.may_join(0, 3));
}
#[test]
fn scope158_overlap_or_incomplete_parent_extent_is_unknown() {
    for dx in [50., 110.] {
        let mut p = page();
        p.tables[1] = table(dx, 60.);
        assert!(scopes(p, rules()).may_join(0, 1));
    }
}
#[test]
fn scope158_shared_child_word_cannot_supply_two_independent_parent_claims() {
    let mut p = page();
    for t in &mut p.tables {
        t.cells[0].words = vec![0];
    }
    assert!(scopes(p, rules()).may_join(0, 1));
}
#[test]
fn scope158_nearly_equal_parent_frontiers_are_not_equal() {
    let mut r = rules();
    r.vertical[0].y0 = 0.00001;
    assert!(scopes(page(), r).may_join(0, 1));
}
#[test]
fn scope158_table_vertical_order_and_matrix_values_do_not_supply_a_fence() {
    let mut p = page();
    p.tables[1] = table(10., 60.);
    assert!(scopes(p, rules()).may_join(0, 1));
}

#[test]
fn scope158_without_crossbar_common_page_frontiers_are_unknown() {
    let mut r = rules();
    r.horizontal.clear();
    assert!(scopes(page(), r).may_join(0, 1));
}
#[test]
fn scope158_open_sibling_frames_need_coextensive_original_child_frontiers() {
    let mut p = test_page(vec![table(10., 0.), table(115., 0.)], vec![], vec![]);
    let r = PageRules {
        vertical: [0., 100., 105., 205.]
            .map(|x| crate::lines::VRule { x, y0: 0., y1: 40. })
            .into(),
        ..Default::default()
    };
    assert!(!scopes(p.clone(), r.clone()).may_join(0, 1));
    p.tables[0].y0 = 1.;
    assert!(scopes(p, r).may_join(0, 1));
}

#[test]
fn scope158_newspaper_bottom_border_is_not_an_internal_parent_row() {
    let mut r = rules();
    for v in &mut r.vertical {
        v.y1 = 80.;
    }
    assert!(scopes(page(), r).may_join(0, 1));
}

#[test]
fn scope158_full_analysis_wire_gold_keeps_exact_serialized_f32_geometry() {
    let point = 393.036_f32;
    let actual =
        serde_json::from_str::<serde_json::Value>(&serde_json::to_string(&point).unwrap()).unwrap();
    let expected: serde_json::Value = serde_json::from_str("393.036").unwrap();
    assert_eq!(actual, expected);
    assert_ne!(serde_json::to_value(point).unwrap(),expected,"to_value widens f32 to f64 before Number serialization; full Analysis baselines are actual emitted JSON and must be compared in that same representation, with no rounding tolerance or geometry projection");
}
#[test]
fn scope158_duplicate_inner_walls_do_not_create_disjoint_open_frames() {
    let mut p = test_page(vec![table(10., 0.), table(110., 0.)], vec![], vec![]);
    let mut r = PageRules {
        vertical: [0., 100., 100., 200.]
            .map(|x| crate::lines::VRule { x, y0: 0., y1: 40. })
            .into(),
        ..Default::default()
    };
    assert!(scopes(p.clone(), r.clone()).may_join(0, 1));
    r.vertical[2].x = 105.;
    p.tables[1] = table(115., 0.);
    r.vertical[3].x = 205.;
    assert!(!scopes(p.clone(), r.clone()).may_join(0, 1));
    p.tables[0].y1 = 39.;
    assert!(scopes(p, r).may_join(0, 1));
}
