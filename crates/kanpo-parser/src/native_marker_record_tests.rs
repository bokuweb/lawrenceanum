use super::*;
use crate::{
    lines::{HRule, VRule},
    trace::Line,
};
struct F {
    units: Vec<Unit>,
    words: Vec<Word>,
    bounds: Vec<NativeBounds>,
    axes: Vec<NativeAxis>,
    styles: Vec<Option<NativeStyles>>,
    rules: PageRules,
    width: f32,
    height: f32,
}
fn fixture(records: usize, fields: usize, label: &str) -> F {
    let (left, top) = (30., 30.);
    let right = left + 16. * records as f32 + 24.;
    let bottom = top + 80. * fields as f32;
    let dy = bottom - top + 20.;
    let mut rules = PageRules::default();
    let mut frame = |xs: &[f32], ys: &[f32]| {
        for &x in xs {
            rules.vertical.push(VRule {
                x,
                y0: ys[0],
                y1: *ys.last().unwrap(),
            });
        }
        for &y in ys {
            rules.horizontal.push(HRule {
                y,
                x0: xs[0],
                x1: *xs.last().unwrap(),
            });
        }
    };
    frame(
        &[20., right + 10., right + 30.],
        &[20., bottom + 10., bottom + dy + 10.],
    );
    let ys = (0..=fields)
        .map(|i| top + 80. * i as f32)
        .collect::<Vec<_>>();
    let pys = ys.iter().map(|y| y + dy).collect::<Vec<_>>();
    frame(&[left, right - 20., right], &ys);
    frame(&[left, right - 20., right], &pys);
    let mut words = vec![];
    let mut cells = [vec![], vec![]];
    for side in 0..2 {
        for record in 0..records {
            for field in 0..fields {
                let id = words.len();
                let x = left + 4. + 16. * (records - 1 - record) as f32;
                let y = top + 10. + 80. * field as f32 + side as f32 * dy;
                let text = if side == 0 {
                    format!("{label}{record}欄{field}")
                } else {
                    "（新設）".into()
                };
                words.push(Word {
                    id,
                    x0: x,
                    y0: y,
                    x1: x + 8.,
                    y1: y + 16.,
                    source_box: [x, y, x + 8., y + 16.],
                    raw: text.clone(),
                    text,
                    status: WordStatus::Used,
                });
            }
        }
    }
    for side in 0..2 {
        for row in 0..if side == 0 { 1 } else { records } {
            for col in 0..fields {
                let ids = if side == 0 {
                    (0..records).map(|r| r * fields + col).collect::<Vec<_>>()
                } else {
                    vec![records * fields + row * fields + col]
                };
                let text = ids
                    .iter()
                    .map(|&id| words[id].text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                cells[side].push(TableCell {
                    row,
                    col,
                    rowspan: 1,
                    colspan: 1,
                    x0: left,
                    y0: top + 80. * col as f32 + side as f32 * dy,
                    x1: right,
                    y1: top + 80. * (col + 1) as f32 + side as f32 * dy,
                    text,
                    words: ids,
                });
            }
        }
    }
    let tables = (0..2)
        .map(|side| Table {
            x0: left,
            y0: top + side as f32 * dy,
            x1: right,
            y1: bottom + side as f32 * dy,
            rows: if side == 0 { 1 } else { records },
            cols: fields,
            cells: cells[side].clone(),
        })
        .collect::<Vec<_>>();
    let line = |t: &Table| Line {
        text: t.render(),
        words: vec![],
        table: Some(t.clone()),
    };
    let units = vec![Unit::Shinkyu {
        preamble: vec![],
        after: vec![vec![line(&tables[0])]],
        before: vec![vec![line(&tables[1])]],
        outside: vec![],
        labels: None,
    }];
    let bounds = words
        .iter()
        .map(|w| NativeBounds::Glyphs(w.source_box))
        .collect();
    let axes = vec![NativeAxis::Down; words.len()];
    let styles = vec![Some(NativeStyles::from([(37, 8f64.to_bits())])); words.len()];
    F {
        units,
        words,
        bounds,
        axes,
        styles,
        rules,
        width: right + 50.,
        height: bottom + dy + 30.,
    }
}
fn tables(f: &F) -> Vec<Table> {
    let Unit::Shinkyu { after, before, .. } = &f.units[0] else {
        panic!()
    };
    after
        .iter()
        .flatten()
        .chain(before.iter().flatten())
        .map(|l| l.table.clone().unwrap())
        .collect()
}
fn run(f: &mut F) {
    let scene = crate::native_source_scene::Scene::for_test(&f.words);
    restore(
        &mut f.units,
        Inputs {
            words: &f.words,
            bounds: &f.bounds,
            axes: &f.axes,
            styles: &f.styles,
            rules: &f.rules,
            size: [f.width, f.height],
            scene: &scene,
        },
    );
}

fn unchanged(mut f: F) {
    let before = tables(&f);
    run(&mut f);
    assert_eq!(tables(&f), before);
}
fn rewrite_peer(f: &mut F, row: usize, col: usize, text: &str) {
    let Unit::Shinkyu { before, .. } = &mut f.units[0] else {
        panic!()
    };
    let t = before[0][0].table.as_mut().unwrap();
    let c = t
        .cells
        .iter_mut()
        .find(|c| c.row == row && c.col == col)
        .unwrap();
    c.text = text.into();
    let id = c.words[0];
    f.words[id].text = text.into();
    f.words[id].raw = text.into();
    before[0][0].text = t.render();
}
#[test]
fn arbitrary_record_counts_fields_and_text_use_existing_changed_marker_witnesses() {
    for n in [2, 4, 9] {
        for fields in [2, 3, 5] {
            for label in ["field", "名称", "値"] {
                let mut f = fixture(n, fields, label);
                let old = tables(&f);
                run(&mut f);
                let new = tables(&f);
                assert_eq!(new[0].rows, n);
                assert_eq!(new[1], old[1]);
                for c in &new[0].cells {
                    assert_eq!(c.words, vec![c.row * fields + c.col]);
                }
            }
        }
    }
}
#[test]
fn parallel_wrapping_without_changed_marker_rows_does_not_gain_records() {
    let mut f = fixture(3, 2, "途中");
    for row in 0..3 {
        for col in 0..2 {
            rewrite_peer(&mut f, row, col, "文章の続き");
        }
    }
    unchanged(f);
}
#[test]
fn one_marker_cannot_certify_a_multicolumn_wrapped_remainder() {
    let mut f = fixture(3, 2, "途中");
    for row in 1..3 {
        for col in 0..2 {
            rewrite_peer(&mut f, row, col, "続く文");
        }
    }
    unchanged(f);
}
#[test]
fn incomplete_marker_fields_do_not_authorize_a_record() {
    let mut f = fixture(2, 2, "field");
    rewrite_peer(&mut f, 1, 1, "違う欄");
    unchanged(f);
}
#[test]
fn omission_rows_cannot_supply_a_new_record_boundary() {
    let mut f = fixture(2, 2, "field");
    for row in 0..2 {
        for col in 0..2 {
            rewrite_peer(&mut f, row, col, "（略）");
        }
    }
    unchanged(f);
}
#[test]
fn marker_witnesses_may_refine_only_a_terminal_single_axis_suffix() {
    let mut f = fixture(3, 2, "field");
    for col in 0..2 {
        rewrite_peer(&mut f, 0, col, "前の欄");
    }
    run(&mut f);
    let ts = tables(&f);
    assert_eq!(ts[0].rows, 3);
    assert_eq!(ts[0].cells[0].words, vec![0]);
}
#[test]
fn unknown_axes_or_original_geometry_do_not_borrow_display_boxes() {
    let mut f = fixture(2, 2, "field");
    f.axes[0] = NativeAxis::Unknown;
    unchanged(f);
    let mut f = fixture(2, 2, "field");
    f.bounds[0] = NativeBounds::Unknown;
    unchanged(f);
}
#[test]
fn missing_or_empty_styles_cannot_supply_original_word_evidence() {
    let mut f = fixture(2, 2, "field");
    f.styles[0] = None;
    unchanged(f);
    let mut f = fixture(2, 2, "field");
    f.styles[0] = Some(NativeStyles::new());
    unchanged(f);
}
#[test]
fn stale_table_text_or_original_word_text_is_not_repaired() {
    let mut f = fixture(2, 2, "field");
    let Unit::Shinkyu { after, .. } = &mut f.units[0] else {
        panic!()
    };
    after[0][0].text.push('外');
    unchanged(f);
    let mut f = fixture(2, 2, "field");
    f.words[0].raw = "別の原字".into();
    unchanged(f);
}
#[test]
fn original_unknown_foreign_ink_inside_a_frame_blocks_the_entire_transfer() {
    let mut f = fixture(2, 2, "field");
    let id = f.words.len();
    f.words.push(Word {
        id,
        x0: 42.,
        y0: 80.,
        x1: 44.,
        y1: 84.,
        source_box: [42., 80., 44., 84.],
        raw: "\u{e500}".into(),
        text: String::new(),
        status: WordStatus::Empty,
    });
    f.bounds.push(NativeBounds::Glyphs([42., 80., 44., 84.]));
    f.axes.push(NativeAxis::Down);
    f.styles
        .push(Some(NativeStyles::from([(37, 8f64.to_bits())])));
    unchanged(f);
}
#[test]
fn duplicate_source_ownership_blocks_the_entire_table() {
    let mut f = fixture(2, 2, "field");
    let Unit::Shinkyu { preamble, .. } = &mut f.units[0] else {
        panic!()
    };
    preamble.push(Line::text(f.words[0].text.clone(), vec![0]));
    unchanged(f);
}
#[test]
fn shared_or_partial_logical_cells_keep_the_previous_table() {
    let mut f = fixture(2, 2, "field");
    let Unit::Shinkyu { after, .. } = &mut f.units[0] else {
        panic!()
    };
    let t = after[0][0].table.as_mut().unwrap();
    t.cells[0].rowspan = 2;
    after[0][0].text = t.render();
    unchanged(f);
}
#[test]
fn separate_original_frames_cannot_borrow_correspondence() {
    let mut f = fixture(2, 2, "field");
    f.rules.vertical.retain(|v| v.y1 - v.y0 < 300.);
    f.rules.horizontal.retain(|h| h.x0 != 20.);
    unchanged(f);
}
#[test]
fn record_plans_are_invariant_under_source_scale_translation_and_font_id_renaming() {
    for scale in [0.5, 1., 4.] {
        let mut f = fixture(3, 3, "任意");
        let (x, y) = (64., 32.);
        for w in &mut f.words {
            w.x0 = w.x0 * scale + x;
            w.x1 = w.x1 * scale + x;
            w.y0 = w.y0 * scale + y;
            w.y1 = w.y1 * scale + y;
            w.source_box = [w.x0, w.y0, w.x1, w.y1];
        }
        f.bounds = f
            .words
            .iter()
            .map(|w| NativeBounds::Glyphs(w.source_box))
            .collect();
        for st in &mut f.styles {
            *st = Some(NativeStyles::from([(999, (8. * scale as f64).to_bits())]));
        }
        for v in &mut f.rules.vertical {
            v.x = v.x * scale + x;
            v.y0 = v.y0 * scale + y;
            v.y1 = v.y1 * scale + y;
        }
        for h in &mut f.rules.horizontal {
            h.y = h.y * scale + y;
            h.x0 = h.x0 * scale + x;
            h.x1 = h.x1 * scale + x;
        }
        let Unit::Shinkyu { after, before, .. } = &mut f.units[0] else {
            panic!()
        };
        for l in after
            .iter_mut()
            .flatten()
            .chain(before.iter_mut().flatten())
        {
            let t = l.table.as_mut().unwrap();
            t.x0 = t.x0 * scale + x;
            t.x1 = t.x1 * scale + x;
            t.y0 = t.y0 * scale + y;
            t.y1 = t.y1 * scale + y;
            for c in &mut t.cells {
                c.x0 = c.x0 * scale + x;
                c.x1 = c.x1 * scale + x;
                c.y0 = c.y0 * scale + y;
                c.y1 = c.y1 * scale + y;
            }
        }
        f.width = f.width * scale + x;
        f.height = f.height * scale + y;
        run(&mut f);
        assert_eq!(tables(&f)[0].rows, 3);
    }
}
#[test]
fn a_derived_table_frame_cannot_move_away_from_its_own_original_glyphs() {
    let mut f = fixture(2, 2, "field");
    let Unit::Shinkyu { after, .. } = &mut f.units[0] else {
        panic!()
    };
    let t = after[0][0].table.as_mut().unwrap();
    t.x0 = 0.;
    t.y0 = 0.;
    t.x1 = 1.;
    t.y1 = 1.;
    unchanged(f);
}
#[test]
fn a_derived_field_rectangle_cannot_borrow_a_different_original_band() {
    let mut f = fixture(2, 2, "field");
    let Unit::Shinkyu { after, .. } = &mut f.units[0] else {
        panic!()
    };
    let t = after[0][0].table.as_mut().unwrap();
    t.cells[0].y0 = 0.;
    t.cells[0].y1 = 1.;
    unchanged(f);
}
#[test]
fn native_record_witnesses_survive_arbitrary_word_tokenization() {
    let mut f = fixture(3, 3, "項目");
    let mut ids = BTreeMap::new();
    let mut words = vec![];
    for old in &f.words {
        let chars = old.text.chars().collect::<Vec<_>>();
        let mut own = vec![];
        for (i, ch) in chars.iter().enumerate() {
            let mut w = old.clone();
            w.id = words.len();
            w.text = ch.to_string();
            w.raw = w.text.clone();
            w.y0 = old.y0 + (old.y1 - old.y0) * i as f32 / chars.len() as f32;
            w.y1 = old.y0 + (old.y1 - old.y0) * (i + 1) as f32 / chars.len() as f32;
            w.source_box = [w.x0, w.y0, w.x1, w.y1];
            own.push(w.id);
            words.push(w);
        }
        ids.insert(old.id, own);
    }
    f.words = words;
    f.bounds = f
        .words
        .iter()
        .map(|w| NativeBounds::Glyphs(w.source_box))
        .collect();
    f.axes = vec![NativeAxis::Down; f.words.len()];
    f.styles = vec![Some(NativeStyles::from([(88, 8f64.to_bits())])); f.words.len()];
    let Unit::Shinkyu { after, before, .. } = &mut f.units[0] else {
        panic!()
    };
    for l in after
        .iter_mut()
        .flatten()
        .chain(before.iter_mut().flatten())
    {
        for c in &mut l.table.as_mut().unwrap().cells {
            c.words = c
                .words
                .iter()
                .flat_map(|id| ids[id].iter().copied())
                .collect();
        }
    }
    run(&mut f);
    assert_eq!(tables(&f)[0].rows, 3);
}
#[test]
fn one_broad_marker_axis_cannot_certify_multiple_original_record_columns() {
    let mut f = fixture(2, 2, "field");
    let Unit::Shinkyu { before, .. } = &mut f.units[0] else {
        panic!()
    };
    let t = before[0][0].table.as_mut().unwrap();
    t.rows = 1;
    t.cells.retain(|c| c.row == 0);
    before[0][0].text = t.render();
    f.words.truncate(6);
    f.bounds.truncate(6);
    f.axes.truncate(6);
    f.styles.truncate(6);
    for id in [4, 5] {
        let w = &mut f.words[id];
        w.x0 = 34.;
        w.x1 = 58.;
        w.source_box = [w.x0, w.y0, w.x1, w.y1];
        f.bounds[id] = NativeBounds::Glyphs(w.source_box);
    }
    unchanged(f);
}
