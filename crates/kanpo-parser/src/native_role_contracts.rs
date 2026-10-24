//! Source geometry contracts and parser integration regressions.
use crate::native_role_continuation::{source_plan, source_scene, Plan, SceneAtom, Source};
use crate::{
    grid::{Grid, GridCell},
    lines::{HRule, PageRules, VRule},
    native_word_axes::NativeAxis,
    trace::{NativeBounds, Word, WordStatus},
    vertical::Col,
};
struct F {
    inherited_explicit_roles: bool,
    words: Vec<Word>,
    cols: Vec<Col>,
    bounds: Vec<NativeBounds>,
    axes: Vec<NativeAxis>,
    grids: Vec<Grid>,
    rules: PageRules,
    expected: [Vec<usize>; 2],
}
// An implementation receives only source evidence. Test expectations are not
// exposed by this interface, including under test-only prototype development.
fn plan(f: &F) -> Option<Plan> {
    source_plan(Source {
        inherited_explicit_roles: f.inherited_explicit_roles,
        words: &f.words,
        cols: &f.cols,
        bounds: &f.bounds,
        axes: &f.axes,
        grids: &f.grids,
        rules: &f.rules,
    })
}

fn add_word(f: &mut F, b: [f32; 4], text: &str) -> usize {
    let id = f.words.len();
    f.words.push(Word {
        id,
        x0: b[0],
        y0: b[1],
        x1: b[2],
        y1: b[3],
        source_box: b,
        raw: text.into(),
        text: text.into(),
        status: WordStatus::Used,
    });
    f.cols.push(Col {
        id,
        x: b[0],
        y: b[1],
        x1: b[2],
        y1: b[3],
        text: text.into(),
    });
    f.bounds.push(NativeBounds::Glyphs(b));
    f.axes.push(NativeAxis::Right);
    id
}
fn source_pair(n: usize, label: &str) -> F {
    let bottom = 30. + 20. * n as f32;
    let mut f = F {
        inherited_explicit_roles: true,
        words: vec![],
        cols: vec![],
        bounds: vec![],
        axes: vec![],
        grids: vec![],
        rules: PageRules::default(),
        expected: [vec![], vec![]],
    };
    f.grids.push(Grid {
        xs: vec![0., 80., 160.],
        ys: vec![20., bottom],
        cells: vec![
            GridCell {
                x0: 0,
                x1: 1,
                y0: 0,
                y1: 1,
            },
            GridCell {
                x0: 1,
                x1: 2,
                y0: 0,
                y1: 1,
            },
        ],
    });
    for x in [0., 80., 160.] {
        f.rules.vertical.push(VRule {
            x,
            y0: 20.,
            y1: bottom,
        });
    }
    f.rules.horizontal.push(HRule {
        y: bottom,
        x0: 0.,
        x1: 160.,
    });
    for i in 0..n {
        let y = 25. + 20. * i as f32;
        let a = add_word(&mut f, [5., y, 65., y + 8.], label);
        f.expected[0].push(a);
        let b = add_word(&mut f, [85., y, 145., y + 8.], label);
        f.expected[1].push(b);
    }
    f
}
fn reject(f: F) {
    assert!(
        plan(&f).is_none(),
        "Incomplete or contradictory source evidence cannot authorize role inheritance"
    );
}
fn accept(f: F) {
    let p = plan(&f).expect("The complete local open-top pair is the continuation witness");
    assert_eq!(p.after, f.expected[0]);
    assert_eq!(p.before, f.expected[1]);
}
#[test]
fn complete_open_top_pair_with_inherited_source_roles_is_a_local_witness() {
    accept(source_pair(2, "arbitrary"));
}
#[test]
fn labels_and_numbers_are_not_a_vocabulary_classifier() {
    for s in ["x", "任意の長い記述", "123.45"] {
        accept(source_pair(3, s));
    }
}
#[test]
fn an_unbounded_number_of_printed_lines_has_no_source_specific_cap() {
    for n in [1, 4, 17] {
        accept(source_pair(n, "body"));
    }
}
#[test]
fn source_geometry_is_covariant_under_scale_and_translation() {
    for (sx, sy, tx, ty) in [(0.5, 2., 30., 7.), (3., 0.25, 5., 40.)] {
        let mut f = source_pair(2, "body");
        let tr = |b: &mut [f32; 4]| {
            b[0] = b[0] * sx + tx;
            b[2] = b[2] * sx + tx;
            b[1] = b[1] * sy + ty;
            b[3] = b[3] * sy + ty;
        };
        for w in &mut f.words {
            let mut b = [w.x0, w.y0, w.x1, w.y1];
            tr(&mut b);
            w.x0 = b[0];
            w.y0 = b[1];
            w.x1 = b[2];
            w.y1 = b[3];
            tr(&mut w.source_box);
        }
        for c in &mut f.cols {
            let mut b = [c.x, c.y, c.x1, c.y1];
            tr(&mut b);
            c.x = b[0];
            c.y = b[1];
            c.x1 = b[2];
            c.y1 = b[3];
        }
        for b in &mut f.bounds {
            if let NativeBounds::Glyphs(r) = b {
                tr(r);
            }
        }
        for g in &mut f.grids {
            for x in &mut g.xs {
                *x = *x * sx + tx;
            }
            for y in &mut g.ys {
                *y = *y * sy + ty;
            }
        }
        for v in &mut f.rules.vertical {
            v.x = v.x * sx + tx;
            v.y0 = v.y0 * sy + ty;
            v.y1 = v.y1 * sy + ty;
        }
        for h in &mut f.rules.horizontal {
            h.y = h.y * sy + ty;
            h.x0 = h.x0 * sx + tx;
            h.x1 = h.x1 * sx + tx;
        }
        accept(f);
    }
}
#[test]
fn a_previous_heuristic_or_absent_role_cannot_authorize_this_source_plan() {
    let mut f = source_pair(2, "x");
    f.inherited_explicit_roles = false;
    reject(f);
}
#[test]
fn a_closed_top_is_not_an_open_continuation_witness() {
    let mut f = source_pair(2, "x");
    f.rules.horizontal.push(HRule {
        y: 20.,
        x0: 0.,
        x1: 160.,
    });
    reject(f);
}
#[test]
fn even_a_partial_top_cap_is_contradictory() {
    let mut f = source_pair(2, "x");
    f.rules.horizontal.push(HRule {
        y: 20.,
        x0: 0.,
        x1: 80.,
    });
    reject(f);
}
#[test]
fn a_complete_bottom_cap_is_required() {
    let mut f = source_pair(2, "x");
    f.rules.horizontal.clear();
    reject(f);
}
#[test]
fn a_hole_in_a_wall_is_not_bridged_by_tolerance() {
    let mut f = source_pair(2, "x");
    let end = f.rules.vertical[1].y1;
    f.rules.vertical[1].y1 = 35.;
    f.rules.vertical.push(VRule {
        x: 80.,
        y0: 36.,
        y1: end,
    });
    reject(f);
}
#[test]
fn a_hole_in_the_bottom_cap_is_not_bridged() {
    let mut f = source_pair(2, "x");
    let end = f.rules.horizontal[0].y;
    f.rules.horizontal = vec![
        HRule {
            y: end,
            x0: 0.,
            x1: 79.,
        },
        HRule {
            y: end,
            x0: 80.,
            x1: 160.,
        },
    ];
    reject(f);
}
#[test]
fn all_source_geometry_must_be_complete_and_positive() {
    for mode in 0..4 {
        let mut f = source_pair(2, "x");
        f.bounds[0] = match mode {
            0 => NativeBounds::Unknown,
            1 => NativeBounds::Compatibility,
            2 => NativeBounds::Glyphs([5., 25., 5., 33.]),
            _ => NativeBounds::Glyphs([f32::NAN, 25., 65., 33.]),
        };
        reject(f);
    }
}
#[test]
fn no_axis_is_guessed_from_text_or_aspect_ratio() {
    for axis in [
        NativeAxis::Unknown,
        NativeAxis::Compatibility,
        NativeAxis::Down,
        NativeAxis::Left,
        NativeAxis::Up,
    ] {
        let mut f = source_pair(2, "x");
        f.axes[0] = axis;
        reject(f);
    }
}
#[test]
fn moved_merged_or_stale_projection_cannot_borrow_original_cells() {
    for mode in 0..3 {
        let mut f = source_pair(2, "x");
        match mode {
            0 => f.cols[0].text.push_str("new"),
            1 => f.cols[0].x += 1.,
            _ => f.words[0].source_box[2] += 1.,
        };
        reject(f);
    }
}
#[test]
fn missing_and_duplicate_occurrence_ownership_are_rejected() {
    for mode in 0..3 {
        let mut f = source_pair(2, "x");
        match mode {
            0 => {
                f.cols.pop();
            }
            1 => {
                let c = &f.cols[0];
                f.cols.push(Col {
                    id: c.id,
                    x: c.x,
                    y: c.y,
                    x1: c.x1,
                    y1: c.y1,
                    text: c.text.clone(),
                });
            }
            _ => f.words[0].id = 99,
        };
        reject(f);
    }
}
#[test]
fn a_cross_wall_occurrence_has_no_unique_role() {
    let mut f = source_pair(2, "x");
    add_word(&mut f, [75., 35., 85., 43.], "cross");
    reject(f);
}
#[test]
fn preceding_body_in_the_same_projection_denies_an_open_page_start() {
    let mut f = source_pair(2, "x");
    add_word(&mut f, [5., 5., 60., 13.], "body");
    reject(f);
}
#[test]
fn unknown_outside_geometry_cannot_prove_the_absence_of_preceding_body() {
    let mut f = source_pair(2, "x");
    let id = add_word(&mut f, [200., 5., 260., 13.], "outside");
    f.bounds[id] = NativeBounds::Unknown;
    reject(f);
}
#[test]
fn overlapping_or_duplicate_source_frames_are_ambiguous() {
    let mut f = source_pair(2, "x");
    f.grids.push(f.grids[0].clone());
    reject(f);
}
#[test]
fn independent_inherited_fields_keep_punctuation_without_page_closedness() {
    let mut f = source_pair(2, "x");
    f.cols[0].text = "（open".into();
    f.words[0].text = "（open".into();
    f.words[0].raw = "（open".into();
    f.cols[1].text = "close）".into();
    f.words[1].text = "close）".into();
    f.words[1].raw = "close）".into();
    accept(f);
}
#[test]
fn an_empty_comparison_field_cannot_supply_a_pair() {
    let mut f = source_pair(1, "x");
    f.words.pop();
    f.cols.pop();
    f.axes.pop();
    f.bounds.pop();
    reject(f);
}
#[test]
fn unrelated_closed_neighbor_tables_do_not_supply_or_expand_comparison_roles() {
    let mut f = source_pair(2, "body");
    for left in [200., 400.] {
        f.grids.push(Grid {
            xs: vec![left, left + 50., left + 100.],
            ys: vec![20., 40., 60.],
            cells: (0..2)
                .flat_map(|y| {
                    (0..2).map(move |x| GridCell {
                        x0: x,
                        x1: x + 1,
                        y0: y,
                        y1: y + 1,
                    })
                })
                .collect(),
        });
        for x in [left, left + 50., left + 100.] {
            f.rules.vertical.push(VRule {
                x,
                y0: 20.,
                y1: 60.,
            });
        }
        for y in [20., 40., 60.] {
            f.rules.horizontal.push(HRule {
                y,
                x0: left,
                x1: left + 100.,
            });
        }
        for row in 0..2 {
            for col in 0..2 {
                let x = left + 5. + 50. * col as f32;
                let y = 25. + 20. * row as f32;
                add_word(&mut f, [x, y, x + 30., y + 8.], "12345");
            }
        }
    }
    accept(f);
}

include!("native_role_scene_contracts.rs");

#[test]
fn punctuation_cannot_choose_an_independently_proven_native_role() {
    for (a, b) in [
        ("（open", "body"),
        ("body", "close）"),
        ("「open", "close」"),
        ("left(", "right]"),
    ] {
        let mut f = source_pair(2, "x");
        for (id, text) in [(0, a), (1, b)] {
            f.cols[id].text = text.into();
            f.words[id].text = text.into();
            f.words[id].raw = text.into();
        }
        accept(f);
    }
}

fn evidence(f: &F) -> Source<'_> {
    Source {
        inherited_explicit_roles: f.inherited_explicit_roles,
        words: &f.words,
        cols: &f.cols,
        bounds: &f.bounds,
        axes: &f.axes,
        grids: &f.grids,
        rules: &f.rules,
    }
}
fn marker() -> Vec<crate::trace::Unit> {
    vec![crate::trace::Unit::Plain(vec![crate::trace::Line::text(
        "unchanged".into(),
        vec![999],
    )])]
}
fn assert_marker(units: &[crate::trace::Unit]) {
    assert!(
        matches!(units,[crate::trace::Unit::Plain(ls)]if ls==&vec![crate::trace::Line::text("unchanged".into(),vec![999])])
    )
}
#[test]
fn restoration_of_a_complete_pair_can_carry_only_its_certified_roles() {
    let f = source_pair(3, "arbitrary");
    let mut units = marker();
    let result = crate::native_role_continuation::restore(evidence(&f), &mut units, |_| {
        panic!("No ordinary body in this scene")
    });
    assert!(result.applied && result.continues);
    let [crate::trace::Unit::Shinkyu { after, before, .. }] = units.as_slice() else {
        panic!()
    };
    assert_eq!(
        after
            .iter()
            .flatten()
            .flat_map(|l| l.words.clone())
            .collect::<Vec<_>>(),
        f.expected[0]
    );
    assert_eq!(
        before
            .iter()
            .flatten()
            .flat_map(|l| l.words.clone())
            .collect::<Vec<_>>(),
        f.expected[1]
    );
}
#[test]
fn restoration_with_an_ordinary_tail_ends_the_role_chain() {
    let s = scene_fixture(2, 2, true, "ordinary");
    let mut units = marker();
    let result = crate::native_role_continuation::restore(
        evidence(&s.f),
        &mut units,
        crate::horizontal::row_lines,
    );
    assert!(result.applied && !result.continues);
    assert!(matches!(units.last(), Some(crate::trace::Unit::Plain(_))));
}
#[test]
fn incomplete_or_duplicated_renderer_ownership_aborts_without_partial_mutation() {
    for mode in 0..3 {
        let s = scene_fixture(1, 2, true, "ordinary");
        let mut units = marker();
        let result = crate::native_role_continuation::restore(evidence(&s.f), &mut units, |cols| {
            let mut ls = crate::horizontal::row_lines(cols);
            match mode {
                0 => {
                    ls[0].words.pop();
                }
                1 => {
                    let id = ls[0].words[0];
                    ls[0].words.push(id);
                }
                _ => ls[0].words.push(999),
            }
            ls
        });
        assert!(!result.applied && !result.continues);
        assert_marker(&units);
    }
}
#[test]
fn missing_or_invented_material_characters_abort_without_partial_mutation() {
    for mode in 0..2 {
        let s = scene_fixture(1, 2, true, "ordinary");
        let mut units = marker();
        let result = crate::native_role_continuation::restore(evidence(&s.f), &mut units, |cols| {
            let mut ls = crate::horizontal::row_lines(cols);
            if mode == 0 {
                ls[0].text.pop();
            } else {
                ls[0].text.push('X');
            }
            ls
        });
        assert!(!result.applied && !result.continues);
        assert_marker(&units);
    }
}
#[test]
fn formatting_whitespace_has_no_power_to_change_source_ownership() {
    let s = scene_fixture(1, 2, true, "ordinary");
    let mut units = marker();
    let result = crate::native_role_continuation::restore(evidence(&s.f), &mut units, |cols| {
        let mut ls = crate::horizontal::row_lines(cols);
        for l in &mut ls {
            l.text = format!(" {}　\n", l.text);
        }
        ls
    });
    assert!(result.applied && !result.continues);
}
#[test]
fn missing_provenance_never_calls_the_renderer_or_mutates_units() {
    let mut f = source_pair(3, "arbitrary");
    f.inherited_explicit_roles = false;
    let mut units = marker();
    let result = crate::native_role_continuation::restore(evidence(&f), &mut units, |_| panic!());
    assert!(!result.applied);
    assert_marker(&units);
}
fn synthetic_pair_page(tail: bool) -> crate::PageInput {
    let mut xhtml = String::from("<doc><page width='842' height='595'>");
    for y in [85., 105.] {
        for x in [75., 155.] {
            let (x1, y1) = (x + 60., y + 8.);
            xhtml.push_str(&format!("<word xmin='{x}' ymin='{y}' xmax='{x1}' ymax='{y1}' data-wmode='0' data-native-axis='right' data-native-xmin='{x}' data-native-ymin='{y}' data-native-xmax='{x1}' data-native-ymax='{y1}'>arbitrary</word>"));
        }
    }
    if tail {
        xhtml.push_str("<word xmin='75' ymin='140' xmax='135' ymax='148' data-wmode='0' data-native-axis='right' data-native-xmin='75' data-native-ymin='140' data-native-xmax='135' data-native-ymax='148'>ordinary</word>");
    }
    xhtml.push_str("</page></doc>");
    let rules = PageRules {
        vertical: [70., 150., 230.]
            .into_iter()
            .map(|x| VRule {
                x,
                y0: 80.,
                y1: 120.,
            })
            .collect(),
        horizontal: vec![HRule {
            y: 120.,
            x0: 70.,
            x1: 230.,
        }],
        ..Default::default()
    };
    crate::PageInput {
        xhtml,
        rules: Some(rules),
    }
}
fn assert_synthetic_roles(p: &crate::trace::PageTrace) {
    for (role, wanted) in [
        (crate::trace::Role::After, vec![2, 3]),
        (crate::trace::Role::Before, vec![0, 1]),
    ] {
        let ids: Vec<_> = p
            .regions
            .iter()
            .filter(|r| r.role == role)
            .flat_map(|r| &r.lines)
            .flat_map(|l| &l.words)
            .copied()
            .collect();
        assert_eq!(ids, wanted);
    }
}
#[test]
fn actual_parser_can_continue_a_source_certified_pair_across_multiple_unheaded_pages() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut pages = crate::page_inputs(
        &std::fs::read(root.join("tests/data/ruled-cell-flow/20261019h018100001.pdf")).unwrap(),
    )
    .unwrap();
    for _ in 0..3 {
        pages.push(synthetic_pair_page(false));
    }
    let a = crate::analyze_item(&pages);
    assert_eq!(a.pages.len(), 4);
    for p in &a.pages[1..] {
        assert_synthetic_roles(p);
    }
}
#[test]
fn an_ordinary_tail_ends_inheritance_before_a_later_open_frame() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut pages = crate::page_inputs(
        &std::fs::read(root.join("tests/data/ruled-cell-flow/20261019h018100001.pdf")).unwrap(),
    )
    .unwrap();
    pages.push(synthetic_pair_page(true));
    pages.push(synthetic_pair_page(false));
    let a = crate::analyze_item(&pages);
    assert_synthetic_roles(&a.pages[1]);
    assert!(
        a.pages[2].regions.iter().all(|r| !matches!(
            r.role,
            crate::trace::Role::After | crate::trace::Role::Before
        )),
        "A later ordinary open frame cannot borrow an expired source caption"
    );
}
