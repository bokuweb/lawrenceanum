use super::*;
use crate::trace::WordStatus;
fn fixture() -> (
    Vec<Unit>,
    Vec<Word>,
    Vec<NativeBounds>,
    Vec<Option<NativeStyles>>,
    Vec<Option<AxisFlow>>,
) {
    // A coherent V,H,V sequence followed by a displaced H,V fragment. A third
    // physical column is adjacent in the source program but has another baseline.
    let mut words = vec![];
    let mut bounds = vec![];
    let mut styles = vec![];
    let mut flows = vec![];
    for (id, (start, end, baseline, mode, font)) in [
        (10., 30., -100., NativeMode::Vertical, 0),
        (30.25, 50., -98., NativeMode::Horizontal, 1),
        (50.25, 70., -100., NativeMode::Vertical, 0),
        (70.25, 90., -98., NativeMode::Horizontal, 1),
        (90.25, 110., -100., NativeMode::Vertical, 0),
        (10., 30., -80., NativeMode::Vertical, 0),
    ]
    .into_iter()
    .enumerate()
    {
        let text = char::from_u32('甲' as u32 + id as u32).unwrap().to_string();
        let b = [97., start as f32, 101., end as f32];
        let b = if id == 5 {
            [77., start as f32, 81., end as f32]
        } else {
            b
        };
        words.push(Word {
            id,
            x0: b[0],
            y0: b[1],
            x1: b[2],
            y1: b[3],
            source_box: b,
            raw: text.clone(),
            text,
            status: WordStatus::Used,
        });
        bounds.push(NativeBounds::Glyphs(b));
        styles.push(Some([(font, 8f64.to_bits())].into_iter().collect()));
        flows.push(Some(AxisFlow {
            axis: NativeAxis::Down,
            flow: NativeFlow {
                mode: Some(mode),
                start,
                baseline,
                end,
                leading: 8.,
                trailing: 0.,
                prefix: 0.,
            },
            first: id,
            after: id + 1,
            total: 6,
        }));
    }
    let line =
        |ids: Vec<usize>| Line::text(ids.iter().map(|&id| words[id].text.as_str()).collect(), ids);
    let units = vec![
        Unit::Plain(vec![line(vec![0, 1, 2]), line(vec![5])]),
        Unit::Plain(vec![line(vec![3, 4])]),
    ];
    (units, words, bounds, styles, flows)
}
fn lines(units: &[Unit]) -> Vec<Vec<usize>> {
    units
        .iter()
        .flat_map(|u| match u {
            Unit::Plain(l) => l.as_slice(),
            _ => &[],
        })
        .map(|l| l.words.clone())
        .collect()
}
fn check(
    mut u: Vec<Unit>,
    w: Vec<Word>,
    b: Vec<NativeBounds>,
    s: Vec<Option<NativeStyles>>,
    f: Vec<Option<AxisFlow>>,
    rules: Option<&crate::lines::PageRules>,
    want: Vec<Vec<usize>>,
) {
    let before = serde_json::to_value(&w).unwrap();
    restore(&mut u, &w, &b, &s, &f, rules);
    assert_eq!(lines(&u), want);
    assert_eq!(serde_json::to_value(w).unwrap(), before);
}
#[test]
fn witnessed_source_transition_restores_the_whole_fragment() {
    let (u, w, b, s, f) = fixture();
    check(u, w, b, s, f, None, vec![vec![0, 1, 2, 3, 4], vec![5]]);
}
#[test]
fn source_program_adjacency_alone_does_not_merge_the_next_physical_column() {
    let (mut u, w, b, s, f) = fixture();
    u = vec![Unit::Plain(vec![
        Line::text(
            w[..5].iter().map(|w| w.text.as_str()).collect(),
            vec![0, 1, 2, 3, 4],
        ),
        Line::text(w[5].text.clone(), vec![5]),
    ])];
    let old = lines(&u);
    check(u, w, b, s, f, None, old);
}
#[test]
fn missing_or_changed_source_evidence_holds() {
    for case in 0..10 {
        let (u, w, mut b, mut s, mut f) = fixture();
        match case {
            0 => f[3] = None,
            1 => f[3].as_mut().unwrap().first = 1,
            2 => f[3].as_mut().unwrap().flow.start += 0.01,
            3 => f[3].as_mut().unwrap().flow.baseline += 0.01,
            4 => f[3].as_mut().unwrap().flow.mode = Some(NativeMode::Vertical),
            5 => f[3].as_mut().unwrap().flow.prefix = 1.,
            6 => f[2].as_mut().unwrap().flow.trailing = 1.,
            7 => s[3] = None,
            8 => s[3] = Some([(15, 8f64.to_bits())].into_iter().collect()),
            _ => b[3] = NativeBounds::Unknown,
        };
        let old = lines(&u);
        check(u, w, b, s, f, None, old);
    }
}
#[test]
fn duplicate_ranges_and_duplicate_owners_hold() {
    for case in 0..2 {
        let (mut u, w, b, s, mut f) = fixture();
        if case == 0 {
            f[5] = f[3]
        } else {
            if let Unit::Plain(l) = &mut u[0] {
                l.push(Line::text(w[3].text.clone(), vec![3]))
            }
        }
        let old = lines(&u);
        check(u, w, b, s, f, None, old);
    }
}
#[test]
fn unknown_material_in_the_physical_corridor_holds() {
    let (u, mut w, mut b, mut s, mut f) = fixture();
    let mut extra = w[0].clone();
    extra.id = 6;
    extra.y0 = 70.;
    extra.y1 = 71.;
    extra.source_box = [98., 70., 100., 71.];
    w.push(extra);
    b.push(NativeBounds::Unknown);
    s.push(None);
    f.push(None);
    let old = lines(&u);
    check(u, w, b, s, f, None, old);
}
#[test]
fn intervening_blank_table_or_role_boundary_holds() {
    for case in 0..2 {
        let (mut u, w, b, s, f) = fixture();
        u.insert(
            1,
            if case == 0 {
                Unit::Plain(vec![Line {
                    text: String::new(),
                    words: vec![],
                    table: Some(crate::trace::Table {
                        x0: 0.,
                        y0: 0.,
                        x1: 1.,
                        y1: 1.,
                        rows: 1,
                        cols: 1,
                        cells: vec![],
                    }),
                }])
            } else {
                Unit::Shinkyu {labels:None,
                    preamble: vec![],
                    after: vec![],
                    before: vec![],
                    outside: vec![],
                }
            },
        );
        let old = lines(&u);
        check(u, w, b, s, f, None, old);
    }
}
#[test]
fn original_rule_or_frame_boundaries_hold() {
    for case in 0..2 {
        let (u, w, b, s, f) = fixture();
        let mut rules = crate::lines::PageRules::default();
        if case == 0 {
            rules.horizontal.push(crate::lines::HRule {
                y: 70.1,
                x0: 90.,
                x1: 110.,
            });
        } else {
            rules.enclosures.push([96., 5., 102., 70.1]);
        }
        let old = lines(&u);
        check(u, w, b, s, f, Some(&rules), old);
    }
}
#[test]
fn altered_line_text_and_reversed_source_order_hold() {
    for case in 0..2 {
        let (mut u, w, b, s, f) = fixture();
        if let Unit::Plain(l) = &mut u[1] {
            if case == 0 {
                l[0].text.push(' ');
            } else {
                l[0].words.reverse();
                l[0].text = l[0].words.iter().map(|&i| w[i].text.as_str()).collect();
            }
        }
        let old = lines(&u);
        check(u, w, b, s, f, None, old);
    }
}
#[test]
fn arbitrary_text_and_opaque_font_zero_are_not_classifiers() {
    let (mut u, mut w, b, mut s, f) = fixture();
    for word in &mut w {
        word.text = format!("arbitrary{}", word.id);
        word.raw = word.text.clone();
    }
    for st in s.iter_mut().flatten() {
        *st = st
            .iter()
            .map(|&(id, b)| (if id == 0 { 0 } else { 9999 }, b))
            .collect();
    }
    for unit in &mut u {
        if let Unit::Plain(l) = unit {
            for line in l {
                line.text = line.words.iter().map(|&id| w[id].text.as_str()).collect();
            }
        }
    }
    check(u, w, b, s, f, None, vec![vec![0, 1, 2, 3, 4], vec![5]]);
}

fn unproject(axis: NativeAxis, a: f32, c: f32) -> (f32, f32) {
    match axis {
        NativeAxis::Right => (a, c),
        NativeAxis::Down => (-c, a),
        NativeAxis::Left => (-a, -c),
        NativeAxis::Up => (c, -a),
        _ => unreachable!(),
    }
}
fn box_for(axis: NativeAxis, b: [f32; 4]) -> [f32; 4] {
    let p = [
        unproject(axis, b[0], b[1]),
        unproject(axis, b[0], b[3]),
        unproject(axis, b[2], b[1]),
        unproject(axis, b[2], b[3]),
    ];
    [
        p.iter().map(|p| p.0).fold(f32::INFINITY, f32::min),
        p.iter().map(|p| p.1).fold(f32::INFINITY, f32::min),
        p.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max),
        p.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max),
    ]
}
#[test]
fn all_four_physical_axes_use_the_same_source_transition_proof() {
    for axis in [
        NativeAxis::Right,
        NativeAxis::Down,
        NativeAxis::Left,
        NativeAxis::Up,
    ] {
        let (u, mut w, mut b, s, mut f) = fixture();
        for id in 0..w.len() {
            let NativeBounds::Glyphs(old) = b[id] else {
                unreachable!()
            };
            let canonical = project(NativeAxis::Down, old).unwrap();
            let next = box_for(axis, canonical);
            b[id] = NativeBounds::Glyphs(next);
            w[id].source_box = next;
            w[id].x0 = next[0];
            w[id].y0 = next[1];
            w[id].x1 = next[2];
            w[id].y1 = next[3];
            f[id].as_mut().unwrap().axis = axis;
        }
        check(u, w, b, s, f, None, vec![vec![0, 1, 2, 3, 4], vec![5]]);
    }
}
fn metadata(axis: NativeAxis, mode: usize) -> String {
    let name = match axis {
        NativeAxis::Right => "right",
        NativeAxis::Down => "down",
        NativeAxis::Left => "left",
        NativeAxis::Up => "up",
        _ => unreachable!(),
    };
    let b = box_for(axis, [10., -101., 30., -97.]);
    format!("xmin='{}' ymin='{}' xmax='{}' ymax='{}' data-native-xmin='{}' data-native-ymin='{}' data-native-xmax='{}' data-native-ymax='{}' data-wmode='{mode}' data-native-axis='{name}' data-native-flow-start='10' data-native-flow-baseline='-100' data-native-flow-end='30' data-native-flow-leading='8' data-native-flow-trailing='0' data-native-flow-prefix='0' data-native-flow-first-glyph='0' data-native-flow-after-glyph='2'",b[0],b[1],b[2],b[3],b[0],b[1],b[2],b[3])
}
fn parse(attrs: &str, total: Option<usize>) -> Option<AxisFlow> {
    let dom =
        scraper::Html::parse_fragment(&format!("<word {attrs}>unrelated arbitrary string</word>"));
    from_word(
        dom.select(&scraper::Selector::parse("word").unwrap())
            .next()
            .unwrap(),
        total,
    )
}
#[test]
fn exact_native_projection_is_explicit_in_each_font_mode_and_axis() {
    for axis in [
        NativeAxis::Right,
        NativeAxis::Down,
        NativeAxis::Left,
        NativeAxis::Up,
    ] {
        for mode in 0..2 {
            let f = parse(&metadata(axis, mode), Some(2)).unwrap();
            assert_eq!(f.axis, axis);
            assert_eq!((f.first, f.after), (0, 2));
            assert_eq!(f.flow.baseline, -100.);
            assert_eq!(
                f.flow.mode,
                Some(if mode == 0 {
                    NativeMode::Horizontal
                } else {
                    NativeMode::Vertical
                })
            );
        }
    }
}
#[test]
fn incomplete_nonfinite_invalid_and_out_of_page_metadata_never_borrows_geometry() {
    let good = metadata(NativeAxis::Down, 0);
    for key in [
        "start",
        "baseline",
        "end",
        "leading",
        "trailing",
        "prefix",
        "first-glyph",
        "after-glyph",
    ] {
        let bad = good
            .split_whitespace()
            .filter(|p| !p.starts_with(&format!("data-native-flow-{key}=")))
            .collect::<Vec<_>>()
            .join(" ");
        assert!(parse(&bad, Some(2)).is_none(), "missing{key}");
    }
    for total in [None, Some(0), Some(1)] {
        assert!(parse(&good, total).is_none());
    }
    for (from, to) in [
        ("start='10'", "start='NaN'"),
        ("end='30'", "end='inf'"),
        ("leading='8'", "leading='0'"),
        ("baseline='-100'", "baseline='0'"),
        ("first-glyph='0'", "first-glyph='-1'"),
        ("first-glyph='0'", "first-glyph='2'"),
        ("after-glyph='2'", "after-glyph='3'"),
        ("data-wmode='0'", "data-wmode='unknown'"),
        ("data-native-axis='down'", "data-native-axis='unknown'"),
    ] {
        assert!(parse(&good.replace(from, to), Some(2)).is_none(), "{from}");
    }
}

#[test]
fn no_candidate_preserves_preexisting_empty_unit_indices() {
    let (_, w, b, s, f) = fixture();
    let mut units = vec![
        Unit::Plain(vec![]),
        Unit::Plain(vec![Line::text(
            w.iter().map(|w| w.text.as_str()).collect(),
            (0..6).collect(),
        )]),
    ];
    let before = units.iter().map(Unit::text).collect::<Vec<_>>();
    restore(&mut units, &w, &b, &s, &f, None);
    assert_eq!(units.iter().map(Unit::text).collect::<Vec<_>>(), before);
}
#[test]
fn restoring_one_fragment_preserves_unrelated_preexisting_empty_units() {
    let (mut units, w, b, s, f) = fixture();
    units.insert(0, Unit::Plain(vec![]));
    restore(&mut units, &w, &b, &s, &f, None);
    assert_eq!(units.len(), 3);
    assert!(matches!(&units[0],Unit::Plain(l)if l.is_empty()));
    assert_eq!(lines(&units), vec![vec![0, 1, 2, 3, 4], vec![5]]);
}
