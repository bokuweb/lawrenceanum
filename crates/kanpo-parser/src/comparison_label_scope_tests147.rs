use super::*;
use crate::grid::Grid;
use crate::trace::{Line, Unit};
fn cols() -> Vec<Col> {
    vec![
        Col {
            id: 1,
            x: 600.,
            x1: 608.,
            y: 100.,
            y1: 124.,
            text: "変更後".into(),
        },
        Col {
            id: 2,
            x: 600.,
            x1: 608.,
            y: 500.,
            y1: 524.,
            text: "変更前".into(),
        },
        Col {
            id: 3,
            x: 595.,
            x1: 599.,
            y: 110.,
            y1: 150.,
            text: "新しい本文".into(),
        },
        Col {
            id: 4,
            x: 595.,
            x1: 599.,
            y: 510.,
            y1: 550.,
            text: "元の本文".into(),
        },
    ]
}
fn frame(x0: f32, x1: f32, y0: f32, y1: f32) -> Grid {
    Grid {
        xs: vec![x0, x1],
        ys: vec![y0, y1],
        cells: vec![],
    }
}
#[test]
fn scope147_closed_frame_contains_the_complete_source_pair() {
    let c = cols();
    let gs = vec![frame(400., 620., 80., 560.)];
    assert_eq!(field_frames(&c, &gs).unwrap().len(), 1);
}
#[test]
fn scope147_one_label_does_not_certify_a_field() {
    let c = cols();
    let gs = vec![frame(400., 620., 80., 250.)];
    assert!(field_frames(&c, &gs).is_none());
}
#[test]
fn scope147_an_unframed_pair_keeps_independence_unproven() {
    assert!(field_frames(&cols(), &[]).is_none());
}
#[test]
fn scope147_crossing_competing_frames_have_no_unique_owner() {
    let c = cols();
    let gs = vec![frame(590., 610., 50., 600.), frame(580., 608., 80., 560.)];
    assert!(field_frames(&c, &gs).is_none());
}
#[test]
fn scope147_nested_native_frames_choose_the_innermost_owner() {
    let c = cols();
    let gs = vec![frame(300., 700., 50., 700.), frame(590., 610., 80., 560.)];
    assert_eq!(field_frames(&c, &gs).unwrap()[0], &gs[1]);
}
#[test]
fn scope147_a_parallel_independent_table_is_not_a_field_frame() {
    let c = cols();
    let gs = vec![frame(400., 620., 80., 560.), frame(100., 300., 80., 560.)];
    let f = field_frames(&c, &gs).unwrap();
    assert_eq!(f, vec![&gs[0]]);
}
#[test]
fn scope147_another_unframed_pair_prevents_partial_scope_authority() {
    let mut c = cols();
    let mut other = c.clone();
    for w in &mut other {
        w.id += 10;
        w.x -= 300.;
        w.x1 -= 300.;
    }
    c.extend(other);
    assert!(field_frames(&c, &[frame(400., 620., 80., 560.)]).is_none());
}
#[test]
fn scope147_frame_ownership_is_invariant_to_scale_and_translation() {
    for scale in [0.125, 4.] {
        let mut c = cols();
        for w in &mut c {
            w.x = w.x * scale + 10.;
            w.x1 = w.x1 * scale + 10.;
            w.y = w.y * scale - 20.;
            w.y1 = w.y1 * scale - 20.;
        }
        let gs = vec![frame(
            400. * scale + 10.,
            620. * scale + 10.,
            80. * scale - 20.,
            560. * scale - 20.,
        )];
        assert_eq!(field_frames(&c, &gs).unwrap().len(), 1);
    }
}
fn unit() -> Unit {
    Unit::Shinkyu {
        labels: None,
        preamble: vec![],
        after: vec![vec![Line::text("新しい本文".into(), vec![4])]],
        before: vec![vec![Line::text("元の本文".into(), vec![5])]],
        outside: vec![],
    }
}
#[test]
fn scope147_certified_initial_continuation_inherits_the_previous_family() {
    let mut us = vec![unit()];
    inherit(&mut us, Some(ComparisonLabels::CHANGE), false);
    assert!(us[0].text().starts_with("変更後\n"));
}
#[test]
fn scope147_printed_source_labels_stop_inheritance() {
    let mut us = vec![unit()];
    let old = us[0].text();
    inherit(&mut us, Some(ComparisonLabels::CHANGE), true);
    assert_eq!(us[0].text(), old);
}
#[test]
fn scope147_a_preamble_is_an_independent_scope_boundary() {
    let mut u = unit();
    if let Unit::Shinkyu { preamble, .. } = &mut u {
        preamble.push(Line::text("〇別省告示第一号".into(), vec![9]));
    }
    let mut us = vec![u];
    let old = us[0].text();
    inherit(&mut us, Some(ComparisonLabels::CHANGE), false);
    assert_eq!(us[0].text(), old);
}
#[test]
fn scope147_a_plain_unit_stops_later_comparison_inheritance() {
    let mut us = vec![
        Unit::Plain(vec![Line::text("別の記事".into(), vec![8])]),
        unit(),
    ];
    let old = us[1].text();
    inherit(&mut us, Some(ComparisonLabels::CHANGE), false);
    assert_eq!(us[1].text(), old);
}
#[test]
fn scope147_no_previous_family_does_not_invent_one() {
    let mut us = vec![unit()];
    let old = us[0].text();
    inherit(&mut us, None, false);
    assert_eq!(us[0].text(), old);
}
#[test]
fn scope147_a_declared_current_family_is_not_overwritten() {
    let mut u = unit();
    if let Unit::Shinkyu { labels, .. } = &mut u {
        *labels = Some(ComparisonLabels::AMENDMENT);
    }
    let mut us = vec![u];
    let old = us[0].text();
    inherit(&mut us, Some(ComparisonLabels::CHANGE), false);
    assert_eq!(us[0].text(), old);
}

#[test]
fn scope147_a_label_strip_does_not_claim_the_full_comparison_field() {
    let mut c = cols();
    c.truncate(2);
    c.push(Col {
        id: 3,
        x: 500.,
        x1: 508.,
        y: 110.,
        y1: 150.,
        text: "新しい本文".into(),
    });
    c.push(Col {
        id: 4,
        x: 500.,
        x1: 508.,
        y: 510.,
        y1: 550.,
        text: "元の本文".into(),
    });
    let gs = vec![frame(400., 620., 80., 560.), frame(590., 610., 80., 560.)];
    assert_eq!(field_frames(&c,&gs).unwrap(),vec![&gs[0]],"the field must enclose actual source body in both roles; its caption strip alone cannot own the comparison");
}

#[test]
fn scope147_label_only_frame_does_not_prove_field_ownership() {
    let mut c = cols();
    c.truncate(2);
    assert!(field_frames(&c, &[frame(400., 620., 80., 560.)]).is_none());
}
#[test]
fn scope147_one_body_role_is_not_a_complete_field() {
    let mut c = cols();
    c.pop();
    assert!(field_frames(&c, &[frame(400., 620., 80., 560.)]).is_none());
}
#[test]
fn scope147_a_publication_heading_inside_the_frame_prevents_field_authority() {
    let mut c = cols();
    c.push(Col {
        id: 99,
        x: 590.,
        x1: 598.,
        y: 300.,
        y1: 400.,
        text: "〇別省告示第一号".into(),
    });
    assert!(field_frames(&c, &[frame(400., 620., 80., 560.)]).is_none());
}
