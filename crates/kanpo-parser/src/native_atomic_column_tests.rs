use super::*;
#[test]
fn atomic_lane_glyphs_crossing_a_fence_end_are_not_relocated() {
    for (y0, y1) in [(-10., 55.), (45., 155.)] {
        let mut f = fixture();
        let b = [20., y0, 60., y1];
        f.bounds[1] = NativeBounds::Glyphs(b);
        f.flows[1] = flow(b, 50.);
        unchanged(f);
    }
}
#[test]
fn preexisting_fenced_regions_do_not_gain_unproven_article_boundaries() {
    let mut f = fixture();
    f.units = vec![
        Unit::Plain(vec![Line::text("label".into(), vec![1])]),
        Unit::Plain(vec![
            Line::text("upper".into(), vec![0]),
            Line::text("middle".into(), vec![2]),
            Line::text("lower".into(), vec![3]),
        ]),
    ];
    assert!(crate::notice_boundaries::cuts(&page(&f), &f.rules).is_empty());
}
#[test]
fn a_column_certificate_cannot_be_rebound_to_other_original_words() {
    let mut f = fixture();
    apply(&mut f);
    let mut p = page(&f);
    p.native_atomic_columns[0].words = [0].into_iter().collect();
    assert!(crate::notice_boundaries::cuts(&p, &f.rules).is_empty());
}
use crate::{
    lines::{HRule, VRule},
    trace::{ContentStyles, PageTrace, Table},
};
struct Fixture {
    units: Vec<Unit>,
    words: Vec<Word>,
    bounds: Vec<NativeBounds>,
    axes: Vec<NativeAxis>,
    flows: Vec<Option<NativeFlow>>,
    rules: PageRules,
    certificates: Vec<Certificate>,
}
fn word(id: usize, b: [f32; 4], t: &str) -> Word {
    Word {
        id,
        x0: b[0],
        y0: b[1],
        x1: b[2],
        y1: b[3],
        source_box: b,
        raw: t.into(),
        text: t.into(),
        status: WordStatus::Used,
    }
}
fn flow(b: [f32; 4], baseline: f64) -> Option<NativeFlow> {
    Some(NativeFlow {
        mode: Some(NativeMode::Horizontal),
        start: b[0] as f64,
        end: b[2] as f64,
        baseline,
        leading: 8.,
        trailing: 0.,
        prefix: 0.,
    })
}
fn fixture() -> Fixture {
    let coords = [
        [110., 5., 150., 15.],
        [20., 45., 60., 55.],
        [110., 45., 150., 55.],
        [110., 95., 150., 105.],
    ];
    let words = coords
        .iter()
        .zip(["upper", "label", "middle", "lower"])
        .enumerate()
        .map(|(id, (&b, t))| word(id, b, t))
        .collect();
    Fixture {
        units: vec![Unit::Plain(vec![
            Line::text("upper".into(), vec![0]),
            Line::text("label middle".into(), vec![1, 2]),
            Line::text("lower".into(), vec![3]),
        ])],
        words,
        bounds: coords.into_iter().map(NativeBounds::Glyphs).collect(),
        axes: vec![NativeAxis::Right; 4],
        flows: coords
            .into_iter()
            .zip([12., 50., 50., 99.])
            .map(|(b, y)| flow(b, y))
            .collect(),
        certificates: vec![],
        rules: PageRules {
            vertical: [0., 100., 200.]
                .into_iter()
                .map(|x| VRule {
                    x,
                    y0: 0.,
                    y1: 100.,
                })
                .collect(),
            ..Default::default()
        },
    }
}
fn apply(f: &mut Fixture) {
    f.certificates = restore(
        &mut f.units,
        &f.words,
        &f.bounds,
        &f.axes,
        &f.flows,
        Some(&f.rules),
    )
}
fn copy(u: &Unit) -> Unit {
    match u {
        Unit::Plain(l) => Unit::Plain(l.clone()),
        Unit::Shinkyu {
            preamble,
            after,
            before,
            outside,
        } => Unit::Shinkyu {
            preamble: preamble.clone(),
            after: after.clone(),
            before: before.clone(),
            outside: outside.clone(),
        },
    }
}
fn snapshot(f: &Fixture) -> String {
    let mut rows = vec![];
    for (u, unit) in f.units.iter().enumerate() {
        rows.extend(copy(unit).regions(u))
    }
    serde_json::to_string(&rows).unwrap()
}
fn unchanged(mut f: Fixture) {
    let old = snapshot(&f);
    apply(&mut f);
    assert_eq!(snapshot(&f), old)
}
fn page(f: &Fixture) -> PageTrace {
    PageTrace {
        native_horizontal_baselines:vec![],
        native_atomic_columns: f.certificates.clone(),
        native_styles: vec![None; f.words.len()],
        native_bounds: f.bounds.clone(),
        native_content_styles: vec![ContentStyles::Compatibility; f.words.len()],
        native_axes: f.axes.clone(),
        native_rulings: vec![],
        width: 200.,
        height: 120.,
        layout: "horizontal",
        words: f.words.clone(),
        regions: f
            .units
            .iter()
            .enumerate()
            .flat_map(|(i, u)| copy(u).regions(i))
            .collect(),
        text: f
            .units
            .iter()
            .map(Unit::text)
            .collect::<Vec<_>>()
            .join("\n"),
        doc_text: f
            .units
            .iter()
            .map(Unit::text)
            .collect::<Vec<_>>()
            .join("\n"),
        tables: vec![],
    }
}
#[test]
fn original_baselines_prove_columns_even_when_font_descenders_extend_past_rule_end() {
    let mut f = fixture();
    apply(&mut f);
    assert_eq!(
        f.units.iter().map(Unit::text).collect::<Vec<_>>(),
        vec!["label", "upper\nmiddle\nlower"]
    )
}
#[test]
fn intact_body_line_spaces_are_preserved() {
    let mut f = fixture();
    if let Unit::Plain(ls) = &mut f.units[0] {
        ls[0].text = " upper  ".into()
    }
    apply(&mut f);
    assert!(f
        .units
        .iter()
        .any(|u| matches!(u,Unit::Plain(ls)if ls.iter().any(|l|l.text==" upper  "))))
}
#[test]
fn native_column_block_supplies_both_article_boundaries() {
    let mut f = fixture();
    apply(&mut f);
    assert_eq!(
        crate::notice_boundaries::cuts(&page(&f), &f.rules),
        vec![0, 1]
    )
}
#[test]
fn missing_original_flow_holds() {
    let mut f = fixture();
    f.flows[3] = None;
    unchanged(f)
}
#[test]
fn nonhorizontal_original_modes_hold() {
    let mut f = fixture();
    f.flows[1].as_mut().unwrap().mode = Some(NativeMode::Vertical);
    unchanged(f)
}
#[test]
fn original_flow_must_belong_to_the_glyph_bounds() {
    let mut f = fixture();
    f.flows[1].as_mut().unwrap().start += 1.;
    unchanged(f)
}
#[test]
fn a_rule_ending_before_a_native_baseline_holds() {
    let mut f = fixture();
    for r in &mut f.rules.vertical {
        r.y1 = 98.
    }
    unchanged(f)
}
#[test]
fn compatibility_and_unknown_bounds_cannot_supply_a_column() {
    for b in [NativeBounds::Compatibility, NativeBounds::Unknown] {
        let mut f = fixture();
        f.bounds[1] = b;
        unchanged(f)
    }
}
#[test]
fn all_original_axes_must_be_explicit_right() {
    for a in [
        NativeAxis::Down,
        NativeAxis::Left,
        NativeAxis::Up,
        NativeAxis::Unknown,
        NativeAxis::Compatibility,
    ] {
        let mut f = fixture();
        f.axes[1] = a;
        unchanged(f)
    }
}
#[test]
fn a_glyph_crossing_the_painted_wall_holds() {
    let mut f = fixture();
    let b = [90., 45., 110., 55.];
    f.bounds[1] = NativeBounds::Glyphs(b);
    f.flows[1] = flow(b, 50.);
    unchanged(f)
}
#[test]
fn a_second_original_row_in_the_lane_holds() {
    let mut f = fixture();
    let b = [20., 75., 60., 85.];
    let id = f.words.len();
    f.words.push(word(id, b, "another"));
    f.bounds.push(NativeBounds::Glyphs(b));
    f.axes.push(NativeAxis::Right);
    f.flows.push(flow(b, 80.));
    if let Unit::Plain(ls) = &mut f.units[0] {
        ls.insert(2, Line::text("another".into(), vec![id]));
    }
    unchanged(f)
}
#[test]
fn a_closing_horizontal_rule_holds() {
    let mut f = fixture();
    f.rules.horizontal.push(HRule {
        y: 70.,
        x0: 0.,
        x1: 200.,
    });
    unchanged(f)
}
#[test]
fn a_closed_original_enclosure_is_not_an_open_column() {
    let mut f = fixture();
    f.rules.enclosures.push([0., 0., 100., 100.]);
    unchanged(f)
}
#[test]
fn a_closed_enclosure_cannot_supply_article_roots() {
    let mut f = fixture();
    apply(&mut f);
    f.rules.enclosures.push([0., 0., 100., 100.]);
    assert!(crate::notice_boundaries::cuts(&page(&f), &f.rules).is_empty())
}
#[test]
fn comparative_roles_hold() {
    let mut f = fixture();
    let Unit::Plain(ls) = f.units.remove(0) else {
        panic!()
    };
    f.units.push(Unit::Shinkyu {
        preamble: vec![],
        after: vec![ls],
        before: vec![],
        outside: vec![],
    });
    unchanged(f)
}
#[test]
fn an_empty_source_line_is_a_boundary() {
    let mut f = fixture();
    if let Unit::Plain(ls) = &mut f.units[0] {
        ls.insert(1, Line::text("".into(), vec![]))
    }
    unchanged(f)
}
#[test]
fn a_duplicate_source_owner_inside_the_unit_holds() {
    let mut f = fixture();
    if let Unit::Plain(ls) = &mut f.units[0] {
        ls.push(Line::text("label".into(), vec![1]))
    }
    unchanged(f)
}
#[test]
fn a_duplicate_source_owner_in_another_unit_holds() {
    let mut f = fixture();
    f.units
        .push(Unit::Plain(vec![Line::text("label".into(), vec![1])]));
    unchanged(f)
}
#[test]
fn an_opaque_table_in_another_unit_holds() {
    let mut f = fixture();
    let t = Table {
        x0: 210.,
        y0: 0.,
        x1: 290.,
        y1: 100.,
        rows: 1,
        cols: 1,
        cells: vec![],
    };
    f.units.push(Unit::Plain(vec![Line {
        text: t.render(),
        words: vec![],
        table: Some(t),
    }]));
    unchanged(f)
}
#[test]
fn unknown_native_origin_in_another_unit_holds() {
    let mut f = fixture();
    let b = [210., 45., 250., 55.];
    let id = f.words.len();
    f.words.push(word(id, b, "foreign"));
    f.bounds.push(NativeBounds::Unknown);
    f.axes.push(NativeAxis::Unknown);
    f.flows.push(None);
    f.units
        .push(Unit::Plain(vec![Line::text("foreign".into(), vec![id])]));
    unchanged(f)
}
#[test]
fn a_single_untyped_table_row_has_ambiguous_atomic_lanes() {
    let mut f = fixture();
    f.units = vec![Unit::Plain(vec![Line::text(
        "upper label middle lower".into(),
        vec![0, 1, 2, 3],
    )])];
    for (id, b) in f.bounds.iter_mut().enumerate() {
        let NativeBounds::Glyphs(v) = b else { panic!() };
        v[1] = 45.;
        v[3] = 55.;
        f.flows[id] = flow(*v, 50.);
    }
    unchanged(f)
}
#[test]
fn metadata_text_disagreement_holds() {
    let mut f = fixture();
    if let Unit::Plain(ls) = &mut f.units[0] {
        ls[1].text.push_str("unknown")
    }
    unchanged(f)
}
#[test]
fn endcaps_touching_the_column_walls_are_still_a_closed_cell() {
    let mut f = fixture();
    f.rules.horizontal.extend([
        HRule {
            y: 0.,
            x0: 0.,
            x1: 100.,
        },
        HRule {
            y: 100.,
            x0: 0.,
            x1: 100.,
        },
    ]);
    unchanged(f)
}
#[test]
fn nonfinite_original_rule_endpoints_hold() {
    for end in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut f = fixture();
        for r in &mut f.rules.vertical {
            r.y1 = end;
        }
        unchanged(f)
    }
}
#[test]
fn pdf_word_wrappers_do_not_change_the_atomic_original_baseline() {
    let mut f = fixture();
    f.words[1].text = "l".into();
    f.words[1].raw = "l".into();
    f.bounds[1] = NativeBounds::Glyphs([20., 45., 30., 55.]);
    f.flows[1] = flow([20., 45., 30., 55.], 50.);
    for (x, t) in [(30., "a"), (40., "b"), (50., "el")] {
        let id = f.words.len();
        let b = [x, 45., x + 10., 55.];
        f.words.push(word(id, b, t));
        f.bounds.push(NativeBounds::Glyphs(b));
        f.flows.push(flow(b, 50.));
        f.axes.push(NativeAxis::Right);
    }
    if let Unit::Plain(ls) = &mut f.units[0] {
        ls[1].words = vec![1, 4, 5, 6, 2];
    }
    apply(&mut f);
    assert_eq!(
        f.units.iter().map(Unit::text).collect::<Vec<_>>(),
        vec!["label", "upper\nmiddle\nlower"]
    )
}
#[test]
fn restoration_is_idempotent() {
    let mut f = fixture();
    apply(&mut f);
    let old = snapshot(&f);
    apply(&mut f);
    assert_eq!(snapshot(&f), old)
}
#[test]
fn reflected_physical_columns_keep_their_physical_order() {
    let mut f = fixture();
    for (id, b) in f.bounds.iter_mut().enumerate() {
        let NativeBounds::Glyphs(v) = b else { panic!() };
        let old = *v;
        v[0] = 200. - old[2];
        v[2] = 200. - old[0];
        f.words[id] = word(id, *v, &f.words[id].text);
        f.flows[id] = flow(*v, f.flows[id].unwrap().baseline);
    }
    apply(&mut f);
    assert_eq!(
        f.units.iter().map(Unit::text).collect::<Vec<_>>(),
        vec!["upper\nmiddle\nlower", "label"]
    )
}
#[test]
fn translation_scale_and_opaque_text_do_not_select_the_lane() {
    for scale in [0.01, 0.5, 2., 25.] {
        for length in [1, 11, 1000] {
            let mut f = fixture();
            let label = "x".repeat(length);
            f.words[1].text = label.clone();
            f.words[1].raw = label.clone();
            if let Unit::Plain(ls) = &mut f.units[0] {
                ls[1].text = format!("{label} middle")
            }
            for (id, b) in f.bounds.iter_mut().enumerate() {
                let NativeBounds::Glyphs(v) = b else { panic!() };
                let baseline = f.flows[id].unwrap().baseline * scale as f64 - 217.;
                *v = [
                    v[0] * scale + 311.,
                    v[1] * scale - 217.,
                    v[2] * scale + 311.,
                    v[3] * scale - 217.,
                ];
                f.words[id] = word(id, *v, &f.words[id].text);
                let mut new = flow(*v, baseline).unwrap();
                new.leading = 8. * scale as f64;
                f.flows[id] = Some(new);
            }
            for v in &mut f.rules.vertical {
                v.x = v.x * scale + 311.;
                v.y0 = v.y0 * scale - 217.;
                v.y1 = v.y1 * scale - 217.;
            }
            apply(&mut f);
            assert_eq!(
                f.units.iter().map(Unit::text).collect::<Vec<_>>(),
                vec![label, "upper\nmiddle\nlower".into()]
            );
        }
    }
}
