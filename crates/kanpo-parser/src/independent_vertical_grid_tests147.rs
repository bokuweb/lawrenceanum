use super::*;
use crate::grid::GridCell;
struct Source {
    units: Vec<Unit>,
    plans: Vec<(Grid, Vec<Col>)>,
    words: Vec<Word>,
    bounds: Vec<NativeBounds>,
    axes: Vec<NativeAxis>,
    styles: Vec<Option<crate::trace::NativeStyles>>,
    flows: Vec<Option<crate::native_column_flow::AxisFlow>>,
}
fn source() -> Source {
    let boxes = [
        [10., 10., 18., 26.],
        [10., 50., 18., 66.],
        [50., 10., 58., 26.],
        [50., 50., 58., 66.],
    ];
    let words: Vec<_> = boxes
        .into_iter()
        .enumerate()
        .map(|(id, b)| Word {
            id,
            x0: b[0],
            y0: b[1],
            x1: b[2],
            y1: b[3],
            source_box: b,
            raw: "甲乙".into(),
            text: "甲乙".into(),
            status: WordStatus::Used,
        })
        .collect();
    let cols = words
        .iter()
        .map(|w| Col {
            id: w.id,
            x: w.x0,
            x1: w.x1,
            y: w.y0,
            y1: w.y1,
            text: w.text.clone(),
        })
        .collect();
    let cells = (0..2)
        .flat_map(|x| {
            (0..2).map(move |y| GridCell {
                x0: x,
                x1: x + 1,
                y0: y,
                y1: y + 1,
            })
        })
        .collect();
    let grid = Grid {
        xs: vec![0., 40., 80.],
        ys: vec![0., 40., 80.],
        cells,
    };
    let units = words
        .iter()
        .map(|w| Unit::Plain(vec![Line::text(w.text.clone(), vec![w.id])]))
        .collect();
    Source {
        units,
        plans: vec![(grid, cols)],
        bounds: boxes.into_iter().map(NativeBounds::Glyphs).collect(),
        axes: vec![NativeAxis::Down; 4],
        styles: vec![Some([(0, 8f64.to_bits())].into_iter().collect()); 4],
        flows: boxes
            .into_iter()
            .enumerate()
            .map(|(id, b)| flow(b, id))
            .collect(),
        words,
    }
}
fn snapshot(u: &[Unit]) -> String {
    let mut flat = vec![];
    visit(u, |allowed, l| flat.push((allowed, l.clone())));
    serde_json::to_string(&flat).unwrap()
}
fn apply(s: &mut Source) {
    restore_independent_vertical(
        &mut s.units,
        &s.plans,
        &s.words,
        &s.bounds,
        &s.axes,
        &s.styles,
        &s.flows,
    )
}
fn reject(mut s: Source) {
    let old = snapshot(&s.units);
    apply(&mut s);
    assert_eq!(snapshot(&s.units), old);
}
#[test]
fn vertical147_complete_native_grid_rejoins_pure_source_fragments() {
    let mut s = source();
    apply(&mut s);
    let mut ts = vec![];
    visit(&s.units, |_, l| {
        if let Some(t) = &l.table {
            ts.push(t.clone())
        }
    });
    assert_eq!(ts.len(), 1);
    assert_eq!((ts[0].rows, ts[0].cols), (2, 2));
    let ids: BTreeSet<_> = ts[0]
        .cells
        .iter()
        .flat_map(|c| c.words.iter().copied())
        .collect();
    assert_eq!(ids, (0..4).collect());
    let once = snapshot(&s.units);
    apply(&mut s);
    assert_eq!(snapshot(&s.units), once);
}
#[test]
fn vertical147_last_native_glyph_center_may_stay_inside_while_its_em_box_crosses_the_rule() {
    let mut s = source();
    if let NativeBounds::Glyphs(b) = &mut s.bounds[0] {
        b[1] = 24.;
        b[3] = 40.5;
    }
    s.words[0].y0 = 24.;
    s.words[0].y1 = 40.5;
    s.plans[0].1[0].y = 24.;
    s.plans[0].1[0].y1 = 40.5;
    s.flows[0] = flow([10., 24., 18., 40.5], 0);
    apply(&mut s);
    let mut n = 0;
    visit(&s.units, |_, l| n += usize::from(l.table.is_some()));
    assert_eq!(n, 1);
}
#[test]
fn vertical147_glyph_centers_crossing_a_cell_boundary_decline() {
    let mut s = source();
    if let NativeBounds::Glyphs(b) = &mut s.bounds[0] {
        b[1] = 24.;
        b[3] = 48.5;
    }
    s.words[0].y0 = 24.;
    s.words[0].y1 = 48.5;
    s.plans[0].1[0].y = 24.;
    s.plans[0].1[0].y1 = 48.5;
    s.flows[0] = flow([10., 24., 18., 48.5], 0);
    reject(s);
}
#[test]
fn vertical147_unknown_native_direction_does_not_gain_a_table() {
    let mut s = source();
    s.axes[0] = NativeAxis::Unknown;
    reject(s);
}
#[test]
fn vertical147_unknown_native_geometry_does_not_gain_a_table() {
    let mut s = source();
    s.bounds[0] = NativeBounds::Unknown;
    reject(s);
}
#[test]
fn vertical147_duplicate_line_membership_declines() {
    let mut s = source();
    s.units
        .push(Unit::Plain(vec![Line::text("甲乙".into(), vec![0])]));
    reject(s);
}
#[test]
fn vertical147_a_native_member_missing_from_the_plan_is_not_silently_omitted() {
    let mut s = source();
    s.plans[0].1.pop();
    reject(s);
}
#[test]
fn vertical147_foreign_word_mixed_into_one_source_line_declines() {
    let mut s = source();
    if let Unit::Plain(ls) = &mut s.units[0] {
        ls[0].words.push(99);
    }
    reject(s);
}
#[test]
fn vertical147_real_comparison_roles_are_never_reassigned() {
    let mut s = source();
    s.units[0] = Unit::Shinkyu {
        labels: None,
        preamble: vec![],
        after: vec![vec![Line::text("甲乙".into(), vec![0])]],
        before: vec![],
        outside: vec![],
    };
    reject(s);
}
#[test]
fn vertical147_foreign_headings_between_fragments_remain_verbatim() {
    let mut s = source();
    s.units.insert(
        1,
        Unit::Plain(vec![Line::text("〇別省告示第一号".into(), vec![99])]),
    );
    apply(&mut s);
    assert_eq!(s.units[1].text(), "〇別省告示第一号");
    let mut n = 0;
    visit(&s.units, |_, l| n += usize::from(l.table.is_some()));
    assert_eq!(n, 1);
}
#[test]
fn vertical147_native_source_text_mismatch_declines() {
    let mut s = source();
    s.words[0].text = "違う".into();
    reject(s);
}
#[test]
fn vertical147_rendered_character_loss_declines() {
    let mut s = source();
    if let Unit::Plain(ls) = &mut s.units[0] {
        ls[0].text = "欠落".into();
    }
    reject(s);
}
#[test]
fn vertical147_a_source_grid_with_overlapping_cells_declines() {
    let mut s = source();
    let cell = s.plans[0].0.cells[0];
    s.plans[0].0.cells.push(cell);
    reject(s);
}
#[test]
fn vertical147_a_preexisting_typed_table_is_left_exact() {
    let mut s = source();
    let t = crate::shinkyu::grid_table(&s.plans[0].0, s.plans[0].1.clone());
    s.units = vec![Unit::Plain(vec![Line {
        text: t.render(),
        words: vec![],
        table: Some(t),
    }])];
    reject(s);
}

fn flow(b: Rect, id: usize) -> Option<crate::native_column_flow::AxisFlow> {
    let raw=format!("<word xmin='{}' ymin='{}' xmax='{}' ymax='{}' data-wmode='1' data-native-axis='down' data-native-xmin='{}' data-native-ymin='{}' data-native-xmax='{}' data-native-ymax='{}' data-native-flow-start='{}' data-native-flow-baseline='{}' data-native-flow-end='{}' data-native-flow-leading='8' data-native-flow-trailing='0' data-native-flow-prefix='0' data-native-flow-first-glyph='{}' data-native-flow-after-glyph='{}'>甲乙</word>",b[0],b[1],b[2],b[3],b[0],b[1],b[2],b[3],b[1],-(b[0]+b[2])*0.5,b[3],id*2,id*2+2);
    let h = scraper::Html::parse_fragment(&raw);
    crate::native_column_flow::from_word(
        h.select(&scraper::Selector::parse("word").unwrap())
            .next()
            .unwrap(),
        Some(8),
    )
}
#[test]
fn vertical147_missing_native_font_evidence_does_not_authorize_glyph_centers() {
    let mut s = source();
    s.styles[0] = None;
    reject(s);
}
#[test]
fn vertical147_a_smaller_native_last_glyph_must_not_cross_the_cell_boundary() {
    let mut s = source();
    let b = [10., 24.01, 18., 44.];
    s.bounds[0] = NativeBounds::Glyphs(b);
    s.words[0].y0 = b[1];
    s.words[0].y1 = b[3];
    s.plans[0].1[0].y = b[1];
    s.plans[0].1[0].y1 = b[3];
    s.flows[0] = flow(b, 0);
    s.styles[0].as_mut().unwrap().insert((1, 7.99f64.to_bits()));
    reject(s);
}
#[test]
fn vertical147_missing_native_column_evidence_does_not_authorize_glyph_centers() {
    let mut s = source();
    s.flows[0] = None;
    reject(s);
}
#[test]
fn vertical147_stale_native_column_coordinates_are_not_borrowed() {
    let mut s = source();
    if let NativeBounds::Glyphs(b) = &mut s.bounds[0] {
        b[1] += 1.;
        b[3] += 1.;
    }
    s.words[0].y0 += 1.;
    s.words[0].y1 += 1.;
    s.plans[0].1[0].y += 1.;
    s.plans[0].1[0].y1 += 1.;
    reject(s);
}
#[test]
fn vertical147_different_font_identities_with_the_same_source_em_are_supported() {
    let mut s = source();
    s.styles[0].as_mut().unwrap().insert((71, 8f64.to_bits()));
    apply(&mut s);
    let mut n = 0;
    visit(&s.units, |_, l| n += usize::from(l.table.is_some()));
    assert_eq!(n, 1);
}

#[test]
fn vertical147_mixed_em_sizes_are_supported_when_all_possible_centers_stay_in_one_cell() {
    let mut s = source();
    s.styles[0].as_mut().unwrap().insert((1, 7.99f64.to_bits()));
    apply(&mut s);
    let mut n = 0;
    visit(&s.units, |_, l| n += usize::from(l.table.is_some()));
    assert_eq!(n, 1);
}
#[test]
fn vertical147_font_em_and_native_envelope_must_describe_the_same_column() {
    let mut s = source();
    s.styles[0] = Some([(9, 4f64.to_bits())].into_iter().collect());
    reject(s);
}
