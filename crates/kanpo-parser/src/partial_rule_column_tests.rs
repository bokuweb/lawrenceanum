use super::partial_rule_columns;
use crate::vertical::Col;
fn w(id: usize, x: f32, y: f32, h: f32) -> Col {
    Col {
        id,
        x,
        x1: x + 8.,
        y,
        y1: y + h,
        text: format!("任意{id}"),
    }
}
fn sample() -> Vec<Col> {
    vec![
        w(0, 80., 10., 120.),
        w(1, 20., 10., 40.),
        w(2, 8., 10., 40.),
        w(3, 20., 70., 60.),
        w(4, 8., 70., 60.),
    ]
}
fn ids(p: Option<Vec<Vec<Col>>>) -> Vec<Vec<usize>> {
    p.unwrap()
        .iter()
        .map(|g| g.iter().map(|w| w.id).collect())
        .collect()
}
#[test]
fn left_strip() {
    assert_eq!(
        ids(partial_rule_columns::parts(
            &sample(),
            &[],
            &[(60., 8., 28.)]
        )),
        vec![vec![0], vec![1, 2], vec![3, 4]]
    );
}
#[test]
fn right_strip() {
    let mut c = sample();
    for w in &mut c {
        let x = 100. - w.x1;
        w.x1 = 100. - w.x;
        w.x = x;
    }
    assert_eq!(
        ids(partial_rule_columns::parts(&c, &[], &[(60., 72., 92.)])),
        vec![vec![1, 2], vec![3, 4], vec![0]]
    );
}
#[test]
fn scale_translation_and_text_independent() {
    for scale in [0.01, 0.5, 1., 13., 1000.] {
        let mut c = sample();
        for w in &mut c {
            w.x = w.x * scale + 317.;
            w.x1 = w.x1 * scale + 317.;
            w.y = w.y * scale - 521.;
            w.y1 = w.y1 * scale - 521.;
            w.text = "別の長さの本文".into();
        }
        assert_eq!(
            ids(partial_rule_columns::parts(
                &c,
                &[],
                &[(60. * scale - 521., 8. * scale + 317., 28. * scale + 317.)]
            )),
            vec![vec![0], vec![1, 2], vec![3, 4]]
        );
    }
}
#[test]
fn no_ruling_holds() {
    assert!(partial_rule_columns::parts(&sample(), &[], &[]).is_none());
}
#[test]
fn repeated_rows_hold() {
    assert!(
        partial_rule_columns::parts(&sample(), &[], &[(60., 8., 28.), (80., 8., 28.)]).is_none()
    );
}
#[test]
fn vertical_grid_wall_holds() {
    assert!(partial_rule_columns::parts(&sample(), &[(8., 0., 150.)], &[(60., 8., 28.)]).is_none());
}
#[test]
fn crossing_word_holds() {
    let mut c = sample();
    c[1].y1 = 65.;
    assert!(partial_rule_columns::parts(&c, &[], &[(60., 8., 28.)]).is_none());
}
#[test]
fn divider_endpoint_in_word_holds() {
    assert!(partial_rule_columns::parts(&sample(), &[], &[(60., 8., 25.)]).is_none());
}
#[test]
fn interior_floating_rule_holds() {
    assert!(partial_rule_columns::parts(&sample(), &[], &[(60., 15., 28.)]).is_none());
}
#[test]
fn ordinary_full_width_tier_holds() {
    assert!(partial_rule_columns::parts(&sample(), &[], &[(60., 8., 88.)]).is_none());
}
#[test]
fn two_possible_strips_hold() {
    let mut c = sample();
    c.extend([w(5, 100., 10., 40.), w(6, 100., 70., 60.)]);
    assert!(partial_rule_columns::parts(&c, &[], &[(60., 8., 28.), (60., 100., 108.)]).is_none());
}
#[test]
fn missing_and_invalid_geometry_hold() {
    let mut c = sample();
    c[1].text.clear();
    assert!(partial_rule_columns::parts(&c, &[], &[(60., 8., 28.)]).is_none());
    let mut c = sample();
    c[1].x = f32::NAN;
    assert!(partial_rule_columns::parts(&c, &[], &[(60., 8., 28.)]).is_none());
    assert!(partial_rule_columns::parts(&sample(), &[], &[(f32::NAN, 8., 28.)]).is_none());
}

use crate::native_word_axes::NativeAxis;
use crate::trace::{Line, NativeBounds, Table, Unit, Word, WordStatus};
fn signature(u: &[Unit]) -> String {
    serde_json::to_string(&u.iter().map(|u|match u{Unit::Plain(l)=>serde_json::json!({"plain":l}),Unit::Shinkyu{preamble,after,before,outside,..}=>serde_json::json!({"preamble":preamble,"after":after,"before":before,"outside":outside})}).collect::<Vec<_>>()).unwrap()
}
fn fixture() -> (Vec<Unit>, Vec<Word>, Vec<NativeBounds>, Vec<NativeAxis>) {
    let c = sample();
    let w = c
        .iter()
        .map(|c| Word {
            id: c.id,
            text: c.text.clone(),
            raw: c.text.clone(),
            x0: c.x,
            y0: c.y,
            x1: c.x1,
            y1: c.y1,
            source_box: [c.x, c.y, c.x1, c.y1],
            status: WordStatus::Used,
        })
        .collect::<Vec<_>>();
    let b = c
        .iter()
        .map(|c| NativeBounds::Glyphs([c.x, c.y, c.x1, c.y1]))
        .collect();
    let l = vec![vec![0], vec![1, 3], vec![2, 4]]
        .into_iter()
        .map(|ids| Line {
            text: ids.iter().map(|&i| w[i].text.as_str()).collect(),
            words: ids,
            table: None,
        })
        .collect();
    (vec![Unit::Plain(l)], w, b, vec![NativeAxis::Down; 5])
}
fn render(c: Vec<Col>) -> Vec<Line> {
    c.into_iter()
        .map(|w| Line {
            text: w.text,
            words: vec![w.id],
            table: None,
        })
        .collect()
}
fn apply(u: &mut [Unit], w: &[Word], b: &[NativeBounds], a: &[NativeAxis]) {
    partial_rule_columns::restore(u, w, b, a, &[], &[(60., 8., 28.)], render)
}
fn allids(u: &[Unit]) -> Vec<usize> {
    let Unit::Plain(l) = &u[0] else { panic!() };
    l.iter().flat_map(|l| l.words.clone()).collect()
}
#[test]
fn native_source_reorders_complete_words() {
    let (mut u, w, b, a) = fixture();
    apply(&mut u, &w, &b, &a);
    assert_eq!(allids(&u), vec![0, 1, 2, 3, 4]);
}
#[test]
fn all_unverified_axes_hold() {
    for axis in [
        NativeAxis::Unknown,
        NativeAxis::Compatibility,
        NativeAxis::Right,
    ] {
        let (mut u, w, b, mut a) = fixture();
        a[3] = axis;
        let old = signature(&u);
        apply(&mut u, &w, &b, &a);
        assert_eq!(signature(&u), old);
    }
}
#[test]
fn unknown_native_bounds_hold() {
    let (mut u, w, mut b, a) = fixture();
    b[3] = NativeBounds::Unknown;
    let old = signature(&u);
    apply(&mut u, &w, &b, &a);
    assert_eq!(signature(&u), old);
}
#[test]
fn unowned_generated_text_holds() {
    let (mut u, w, b, a) = fixture();
    let Unit::Plain(l) = &mut u[0] else { panic!() };
    l[1].text.push_str("由来不明");
    let old = signature(&u);
    apply(&mut u, &w, &b, &a);
    assert_eq!(signature(&u), old);
}
#[test]
fn duplicate_word_id_holds() {
    let (mut u, w, b, a) = fixture();
    let Unit::Plain(l) = &mut u[0] else { panic!() };
    l[1].words.push(1);
    l[1].text.push_str(&w[1].text);
    let old = signature(&u);
    apply(&mut u, &w, &b, &a);
    assert_eq!(signature(&u), old);
}
#[test]
fn table_is_a_fence() {
    let (mut u, w, b, a) = fixture();
    let Unit::Plain(l) = &mut u[0] else { panic!() };
    l.insert(
        2,
        Line {
            text: "表".into(),
            words: vec![],
            table: Some(Table {
                x0: 0.,
                y0: 0.,
                x1: 100.,
                y1: 100.,
                rows: 1,
                cols: 1,
                cells: vec![],
            }),
        },
    );
    let old = signature(&u);
    apply(&mut u, &w, &b, &a);
    assert_eq!(signature(&u), old);
}
#[test]
fn explicit_blank_line_is_a_fence() {
    let (mut u, w, b, a) = fixture();
    let Unit::Plain(l) = &mut u[0] else { panic!() };
    l.insert(
        2,
        Line {
            text: String::new(),
            words: vec![],
            table: None,
        },
    );
    let old = signature(&u);
    apply(&mut u, &w, &b, &a);
    assert_eq!(signature(&u), old);
}
#[test]
fn shinkyu_sides_are_never_reassembled() {
    let (mut u, w, b, a) = fixture();
    let Unit::Plain(l) = u.remove(0) else {
        panic!()
    };
    u.push(Unit::Shinkyu {labels:None,
        preamble: l,
        after: vec![],
        before: vec![],
        outside: vec![],
    });
    let old = signature(&u);
    apply(&mut u, &w, &b, &a);
    assert_eq!(signature(&u), old);
}
#[test]
fn renderer_dropped_or_duplicated_material_holds() {
    for duplicate in [false, true] {
        let (mut u, w, b, a) = fixture();
        let old = signature(&u);
        partial_rule_columns::restore(&mut u, &w, &b, &a, &[], &[(60., 8., 28.)], |c| {
            let mut l = render(c);
            if duplicate {
                l.push(l[0].clone());
            } else {
                l.pop();
            }
            l
        });
        assert_eq!(signature(&u), old);
    }
}
#[test]
fn complete_full_width_divider_reorders_subbands() {
    let c = sample()[1..].to_vec();
    assert_eq!(
        ids(partial_rule_columns::parts(&c, &[], &[(60., 8., 28.)])),
        vec![vec![1, 2], vec![3, 4]]
    );
}

#[test]
fn moved_intact_line_retains_its_original_spaces() {
    let (mut u, w, b, a) = fixture();
    let Unit::Plain(l) = &mut u[0] else { panic!() };
    l[0].text = "任　意0".into();
    apply(&mut u, &w, &b, &a);
    assert_eq!(allids(&u), vec![0, 1, 2, 3, 4]);
    let Unit::Plain(l) = &u[0] else { panic!() };
    assert_eq!(l[0].text, "任　意0");
}

#[test]
fn duplicate_source_occurrence_in_another_unit_holds() {
    let (mut u, w, b, a) = fixture();
    u.push(Unit::Plain(vec![Line {
        text: w[1].text.clone(),
        words: vec![1],
        table: None,
    }]));
    let old = signature(&u);
    apply(&mut u, &w, &b, &a);
    assert_eq!(signature(&u), old);
}
#[test]
fn missing_source_word_holds() {
    let (mut u, w, b, a) = fixture();
    let Unit::Plain(l) = &mut u[0] else { panic!() };
    l[1].words.push(99);
    let old = signature(&u);
    apply(&mut u, &w, &b, &a);
    assert_eq!(signature(&u), old);
}
#[test]
fn compatibility_bounds_do_not_substitute_for_original_glyphs() {
    let (mut u, w, mut b, a) = fixture();
    b[3] = NativeBounds::Compatibility;
    let old = signature(&u);
    apply(&mut u, &w, &b, &a);
    assert_eq!(signature(&u), old);
}

#[test]
fn non_body_word_status_cannot_establish_a_boundary() {
    for status in [
        WordStatus::Margin,
        WordStatus::Empty,
        WordStatus::HeaderLabel,
        WordStatus::Noise,
        WordStatus::Ruby,
    ] {
        let (mut u, mut w, b, a) = fixture();
        w[3].status = status;
        let old = signature(&u);
        apply(&mut u, &w, &b, &a);
        assert_eq!(signature(&u), old);
    }
}
