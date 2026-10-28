//! Native source sentence scope contracts, frozen before the rewrite implementation.
use crate::native_sentence_cells::{source_plan, Source};
use crate::{
    grid::{Grid, GridCell},
    lines::{HRule, PageRules, VRule},
    native_word_axes::NativeAxis,
    native_word_rulings::NativeRuling,
    trace::{NativeBounds, Table, TableCell, Word, WordStatus},
    vertical::Col,
};
struct F {
    flows: Vec<Option<crate::native_word_flow::NativeFlow>>,
    table: Table,
    words: Vec<Word>,
    originals: Vec<Col>,
    bounds: Vec<NativeBounds>,
    axes: Vec<NativeAxis>,
    rulings: Vec<NativeRuling>,
    grids: Vec<Grid>,
    rules: PageRules,
    source_cells: Vec<Vec<usize>>,
}
fn add(f: &mut F, b: [f32; 4], text: &str) -> usize {
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
    f.originals.push(Col {
        id,
        x: b[0],
        y: b[1],
        x1: b[2],
        y1: b[3],
        text: text.into(),
    });
    f.bounds.push(NativeBounds::Glyphs(b));
    f.axes.push(NativeAxis::Right);
    f.rulings.push(NativeRuling::Unknown);
    f.flows.push(None);
    id
}
fn cell(f: &mut F, row: usize, col: usize, rect: [f32; 4], ids: Vec<usize>) {
    let text = ids
        .iter()
        .map(|i| f.words[*i].text.as_str())
        .collect::<Vec<_>>()
        .join("");
    f.table.cells.push(TableCell {
        row,
        col,
        rowspan: 1,
        colspan: 1,
        x0: rect[0],
        y0: rect[1],
        x1: rect[2],
        y1: rect[3],
        text,
        words: ids,
    });
}
fn fixture(record: bool, n: usize) -> F {
    assert!(n >= 2);
    let bottom = 80. + 20. * n as f32;
    let xs = vec![0., 80., 160., 240.];
    let ys = vec![20., 40., bottom];
    let g = Grid {
        xs: xs.clone(),
        ys: ys.clone(),
        cells: (0..2)
            .flat_map(|y| {
                (0..3).map(move |x| GridCell {
                    x0: x,
                    x1: x + 1,
                    y0: y,
                    y1: y + 1,
                })
            })
            .collect(),
    };
    let mut f = F {
        table: Table {
            x0: 0.,
            y0: 20.,
            x1: 240.,
            y1: bottom,
            rows: 3,
            cols: 3,
            cells: vec![],
        },
        words: vec![],
        originals: vec![],
        bounds: vec![],
        axes: vec![],
        rulings: vec![],
        flows: vec![],
        grids: vec![g],
        rules: PageRules::default(),
        source_cells: vec![vec![]; 6],
    };
    for x in xs {
        f.rules.vertical.push(VRule {
            x,
            y0: 20.,
            y1: bottom,
        });
    }
    for y in ys {
        f.rules.horizontal.push(HRule {
            y,
            x0: 0.,
            x1: 240.,
        });
    }
    for c in 0..3 {
        let x = 5. + 80. * c as f32;
        let id = add(&mut f, [x, 25., x + 60., 33.], "見出し");
        f.source_cells[c].push(id);
        cell(
            &mut f,
            0,
            c,
            [80. * c as f32, 20., 80. * (c + 1) as f32, 40.],
            vec![id],
        );
    }
    for k in 0..2 {
        let y = 50. + 20. * k as f32;
        let id = add(
            &mut f,
            [5., y, 65., y + 8.],
            if k == 0 { "項目甲" } else { "項目乙" },
        );
        f.source_cells[3].push(id);
        cell(&mut f, 1 + k, 0, [0., 40., 80., bottom], vec![id]);
    }
    for k in 0..if record { 2 } else { 1 } {
        let y = 50. + 20. * k as f32;
        let id = add(
            &mut f,
            [85., y, 125., y + 8.],
            if record {
                if k == 0 {
                    "100"
                } else {
                    "200"
                }
            } else {
                "補足"
            },
        );
        f.source_cells[4].push(id);
        cell(&mut f, 1 + k, 1, [80., 40., 160., bottom], vec![id]);
    }
    let mut body = vec![];
    for k in 0..n {
        let y = 50. + 20. * k as f32;
        let id = add(
            &mut f,
            [165., y, 225., y + 8.],
            if k + 1 == n {
                "末尾。"
            } else {
                "任意の説明"
            },
        );
        body.push(id);
    }
    f.source_cells[5] = body.clone();
    cell(&mut f, 1, 2, [160., 40., 240., bottom], body[..1].to_vec());
    cell(&mut f, 2, 2, [160., 40., 240., bottom], body[1..].to_vec());
    f
}
fn prove(f: &F) -> Option<Table> {
    source_plan(
        &f.table,
        Source {
            flows: Some(&f.flows),
            words: &f.words,
            originals: &f.originals,
            bounds: &f.bounds,
            axes: &f.axes,
            rulings: &f.rulings,
            grids: &f.grids,
            rules: &f.rules,
        },
    )
}
fn patch(f: &mut F, id: usize, text: &str) {
    f.words[id].raw = text.into();
    f.words[id].text = text.into();
    f.originals.iter_mut().find(|w| w.id == id).unwrap().text = text.into();
    for c in &mut f.table.cells {
        c.text = c.words.iter().map(|i| f.words[*i].text.as_str()).collect();
    }
}
fn native(f: F) {
    let t = prove(&f)
        .expect("A complete source sentence cannot be broken by unsupported neighbor-row anchors");
    assert_eq!((t.rows, t.cols, t.cells.len()), (2, 3, 6));
    for (row, col, expected) in f
        .source_cells
        .iter()
        .enumerate()
        .map(|(i, w)| (i / 3, i % 3, w))
    {
        let c: Vec<_> = t
            .cells
            .iter()
            .filter(|c| c.row == row && c.col == col)
            .collect();
        assert_eq!(c.len(), 1);
        assert_eq!(&c[0].words, expected);
    }
    let mut a: Vec<_> = f.table.cells.iter().flat_map(|c| c.words.clone()).collect();
    let mut b: Vec<_> = t.cells.iter().flat_map(|c| c.words.clone()).collect();
    a.sort_unstable();
    b.sort_unstable();
    assert_eq!(a, b);
}
fn reject(f: F) {
    assert!(
        prove(&f).is_none(),
        "Incomplete or non-sentence evidence cannot authorize a rewrite"
    );
}
#[test]
fn complete_native_sentence_scopes_the_original_physical_body_row() {
    native(fixture(false, 3));
}
#[test]
fn source_sentence_has_no_printed_line_count_maximum() {
    for n in [3, 7, 29] {
        native(fixture(false, n));
    }
}
#[test]
fn arbitrary_words_do_not_classify_the_layout() {
    for text in ["x", "alphabetic", "説明12345", "別の内容"] {
        let mut f = fixture(false, 4);
        for id in f.source_cells[5][..3].to_vec() {
            patch(&mut f, id, text);
        }
        native(f);
    }
}
#[test]
fn paired_numeric_records_keep_their_values_while_a_shared_sentence_spans_them() {
    let f = fixture(true, 4);
    let t = prove(&f).expect("Shared prose must span real value records");
    assert_eq!(t.rows, 3);
    for old in f.table.cells.iter().filter(|c| c.col < 2) {
        assert!(t.cells.contains(old));
    }
    let c: Vec<_> = t.cells.iter().filter(|c| c.col == 2 && c.row > 0).collect();
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].rowspan, 2);
    assert_eq!(c[0].words, f.source_cells[5]);
}
#[test]
fn no_sentence_terminal_supplies_no_semantic_scope() {
    let mut f = fixture(false, 3);
    let id = *f.source_cells[5].last().unwrap();
    patch(&mut f, id, "未完の説明");
    reject(f);
}
#[test]
fn complete_sentences_already_in_separate_records_do_not_merge() {
    let mut f = fixture(false, 3);
    let id = f.source_cells[5][0];
    patch(&mut f, id, "独立した文。");
    reject(f);
}
#[test]
fn decimals_are_not_sentence_terminals() {
    let mut f = fixture(false, 3);
    for id in f.source_cells[5].clone() {
        patch(&mut f, id, "12.34");
    }
    reject(f);
}
#[test]
fn parentheses_and_quotes_keep_internal_periods_in_their_scope() {
    let mut f = fixture(false, 3);
    let ids = f.source_cells[5].clone();
    patch(&mut f, ids[0], "説明（「引用。");
    patch(&mut f, ids[1], "続き」）");
    patch(&mut f, ids[2], "末尾。");
    native(f);
}
#[test]
fn unmatched_or_incompatible_delimiters_are_not_sentence_evidence() {
    for text in ["（説明。", "説明）。", "（説明］。"] {
        let mut f = fixture(false, 3);
        let id = *f.source_cells[5].last().unwrap();
        patch(&mut f, id, text);
        reject(f);
    }
}
#[test]
fn missing_original_occurrence_and_duplicate_ids_are_rejected() {
    for mode in 0..3 {
        let mut f = fixture(false, 3);
        let id = f.source_cells[5][0];
        match mode {
            0 => {
                f.originals.retain(|w| w.id != id);
            }
            1 => {
                let w = f.originals.iter().find(|w| w.id == id).unwrap().clone();
                f.originals.push(w);
            }
            _ => {
                let c = f
                    .table
                    .cells
                    .iter_mut()
                    .find(|c| c.words.contains(&id))
                    .unwrap();
                c.words.push(id);
            }
        }
        reject(f);
    }
}
#[test]
fn rewritten_source_tokens_cannot_mint_a_sentence() {
    let mut f = fixture(false, 3);
    let id = f.source_cells[5][0];
    f.words[id].text = "rewrite".into();
    reject(f);
}
#[test]
fn changed_cell_material_is_not_silently_reconstructed() {
    let mut f = fixture(false, 3);
    f.table
        .cells
        .iter_mut()
        .find(|c| c.col == 2 && c.row == 2)
        .unwrap()
        .text
        .push('X');
    reject(f);
}
#[test]
fn unknown_compatibility_or_opposite_axes_are_not_guessed() {
    for a in [
        NativeAxis::Unknown,
        NativeAxis::Compatibility,
        NativeAxis::Left,
        NativeAxis::Down,
    ] {
        let mut f = fixture(false, 3);
        let id = f.source_cells[5][0];
        f.axes[id] = a;
        reject(f);
    }
}
#[test]
fn unknown_and_nonpositive_source_rectangles_fail_closed() {
    for mode in 0..3 {
        let mut f = fixture(false, 3);
        let id = f.source_cells[5][0];
        f.bounds[id] = match mode {
            0 => NativeBounds::Unknown,
            1 => NativeBounds::Compatibility,
            _ => NativeBounds::Glyphs([0., 0., 0., 1.]),
        };
        reject(f);
    }
}
#[test]
fn moved_or_stale_source_projection_is_not_a_cell_witness() {
    for mode in 0..2 {
        let mut f = fixture(false, 3);
        let id = f.source_cells[5][0];
        if mode == 0 {
            f.words[id].x0 += 1.;
        } else {
            f.originals.iter_mut().find(|w| w.id == id).unwrap().x += 1.;
        }
        reject(f);
    }
}
#[test]
fn an_unowned_current_source_word_inside_the_frame_is_ambiguous() {
    let mut f = fixture(false, 3);
    add(&mut f, [170., 110., 200., 118.], "foreign");
    reject(f);
}
#[test]
fn a_partly_crossing_source_word_has_no_unique_cell() {
    let mut f = fixture(false, 3);
    add(&mut f, [155., 110., 165., 118.], "cross");
    reject(f);
}
#[test]
fn absent_actual_wall_and_a_gap_in_its_support_cannot_be_bridged() {
    for gap in [false, true] {
        let mut f = fixture(false, 3);
        f.rules.vertical.retain(|v| v.x != 160.);
        if gap {
            let end = f.table.y1;
            f.rules.vertical.extend([
                VRule {
                    x: 160.,
                    y0: 20.,
                    y1: 60.,
                },
                VRule {
                    x: 160.,
                    y0: 60.01,
                    y1: end,
                },
            ]);
        }
        reject(f);
    }
}
#[test]
fn duplicate_source_grid_and_incomplete_leaf_partition_are_ambiguous() {
    for mode in 0..2 {
        let mut f = fixture(false, 3);
        if mode == 0 {
            f.grids.push(f.grids[0].clone());
        } else {
            f.grids[0].cells.pop();
        }
        reject(f);
    }
}
#[test]
fn an_explicit_physical_row_boundary_remains_a_boundary() {
    let mut f = fixture(false, 3);
    let g = &mut f.grids[0];
    g.ys.insert(2, 60.);
    g.cells = (0..3)
        .flat_map(|y| {
            (0..3).map(move |x| GridCell {
                x0: x,
                x1: x + 1,
                y0: y,
                y1: y + 1,
            })
        })
        .collect();
    f.rules.horizontal.push(HRule {
        y: 60.,
        x0: 0.,
        x1: 240.,
    });
    reject(f);
}
#[test]
fn arbitrary_scale_and_translation_preserve_the_source_partition() {
    for (sx, sy, tx, ty) in [(0.5, 3., 30., 7.), (4., 0.25, 5., 20.)] {
        let mut f = fixture(false, 4);
        let tr = |b: [f32; 4]| {
            [
                b[0] * sx + tx,
                b[1] * sy + ty,
                b[2] * sx + tx,
                b[3] * sy + ty,
            ]
        };
        for i in 0..f.words.len() {
            let b = tr(f.words[i].source_box);
            let w = &mut f.words[i];
            w.source_box = b;
            w.x0 = b[0];
            w.y0 = b[1];
            w.x1 = b[2];
            w.y1 = b[3];
            f.bounds[i] = NativeBounds::Glyphs(b);
            let c = f.originals.iter_mut().find(|c| c.id == i).unwrap();
            c.x = b[0];
            c.y = b[1];
            c.x1 = b[2];
            c.y1 = b[3];
        }
        for c in &mut f.table.cells {
            let b = tr([c.x0, c.y0, c.x1, c.y1]);
            c.x0 = b[0];
            c.y0 = b[1];
            c.x1 = b[2];
            c.y1 = b[3];
        }
        let b = tr([f.table.x0, f.table.y0, f.table.x1, f.table.y1]);
        f.table.x0 = b[0];
        f.table.y0 = b[1];
        f.table.x1 = b[2];
        f.table.y1 = b[3];
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
        native(f);
    }
}

#[test]
fn bare_numeric_value_rows_and_a_following_note_are_not_one_sentence() {
    let mut f = fixture(false, 3);
    let ids = f.source_cells[5].clone();
    patch(&mut f, ids[0], "100");
    patch(&mut f, ids[1], "200");
    patch(&mut f, ids[2], "注記。");
    reject(f);
}
#[test]
fn a_two_line_sentence_spans_atomic_neighbor_records_without_collapsing_them() {
    let f = fixture(false, 2);
    let t = prove(&f)
        .expect("Sentence fragments must coalesce even when neighbor row evidence remains");
    assert_eq!(t.rows, 3);
    let c: Vec<_> = t.cells.iter().filter(|c| c.col == 2 && c.row > 0).collect();
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].rowspan, 2);
    assert_eq!(c[0].words, f.source_cells[5]);
}

#[test]
fn an_original_candidate_without_the_matching_word_slot_is_not_ignored() {
    let mut f = fixture(false, 3);
    f.originals.push(Col {
        id: 999,
        x: 170.,
        y: 110.,
        x1: 200.,
        y1: 118.,
        text: "unknown".into(),
    });
    reject(f);
}
#[test]
fn an_unbounded_number_of_value_records_keeps_its_row_correspondence() {
    for n in [2, 4, 11] {
        let mut f = fixture(true, n + 1);
        let bottom = f.table.y1;
        for k in 2..n {
            let y = 50. + 20. * k as f32;
            let a = add(&mut f, [5., y, 65., y + 8.], "任意の項目");
            f.source_cells[3].push(a);
            let b = add(&mut f, [85., y, 125., y + 8.], "12345");
            f.source_cells[4].push(b);
        }
        f.table.cells.retain(|c| c.row == 0);
        f.table.rows = n + 1;
        for k in 0..n {
            let a = f.source_cells[3][k];
            let b = f.source_cells[4][k];
            cell(&mut f, k + 1, 0, [0., 40., 80., bottom], vec![a]);
            cell(&mut f, k + 1, 1, [80., 40., 160., bottom], vec![b]);
            let ids = if k + 1 == n {
                f.source_cells[5][k..].to_vec()
            } else {
                vec![f.source_cells[5][k]]
            };
            cell(&mut f, k + 1, 2, [160., 40., 240., bottom], ids);
        }
        let t = prove(&f).unwrap();
        assert_eq!(t.rows, n + 1);
        for c in f.table.cells.iter().filter(|c| c.col < 2) {
            assert!(t.cells.contains(c));
        }
        let last: Vec<_> = t.cells.iter().filter(|c| c.col == 2 && c.row > 0).collect();
        assert_eq!(last.len(), 1);
        assert_eq!(last[0].rowspan, n);
        assert_eq!(last[0].words, f.source_cells[5]);
    }
}

#[test]
fn an_actual_horizontal_record_rule_cannot_be_ignored_by_a_coarse_grid() {
    let mut f = fixture(false, 3);
    f.rules.horizontal.push(HRule {
        y: 60.,
        x0: 0.,
        x1: 240.,
    });
    reject(f);
}
#[test]
fn an_actual_vertical_source_wall_cannot_be_ignored_by_a_coarse_grid() {
    let mut f = fixture(false, 3);
    for (k, id) in f.source_cells[5].clone().into_iter().enumerate() {
        let w = &mut f.words[id];
        let b = if k == 1 {
            [200., w.y0, 225., w.y1]
        } else {
            [165., w.y0, 185., w.y1]
        };
        w.x0 = b[0];
        w.x1 = b[2];
        w.source_box = b;
        f.bounds[id] = NativeBounds::Glyphs(b);
        let c = f.originals.iter_mut().find(|c| c.id == id).unwrap();
        c.x = b[0];
        c.x1 = b[2];
    }
    f.rules.vertical.push(VRule {
        x: 190.,
        y0: 40.,
        y1: f.table.y1,
    });
    reject(f);
}

#[test]
fn near_border_strokes_without_glyphs_on_both_sides_are_not_internal_cuts() {
    let mut f = fixture(false, 3);
    let bottom = f.table.y1;
    for y in [20.01, 39.99, 40.01, bottom - 0.01] {
        f.rules.horizontal.push(HRule {
            y,
            x0: 0.,
            x1: 240.,
        });
    }
    native(f);
}
#[test]
fn an_unlocated_nonfinite_ruling_cannot_certify_a_source_partition() {
    let mut f = fixture(false, 3);
    f.rules.horizontal.push(HRule {
        y: f32::NAN,
        x0: 0.,
        x1: 240.,
    });
    reject(f);
}
#[test]
fn disconnected_internal_strokes_do_not_become_a_complete_row_boundary() {
    let mut f = fixture(false, 3);
    f.rules.horizontal.extend([
        HRule {
            y: 60.,
            x0: 160.,
            x1: 199.99,
        },
        HRule {
            y: 60.,
            x0: 200.,
            x1: 240.,
        },
    ]);
    native(f);
}

#[test]
fn an_unemitted_graphic_value_slot_is_not_proof_of_one_prose_record() {
    let mut f = fixture(false, 3);
    let id = f.source_cells[4][0];
    f.words[id].raw = "─".into();
    f.words[id].text.clear();
    f.words[id].status = WordStatus::Empty;
    f.originals.retain(|c| c.id != id);
    f.rulings[id] = NativeRuling::Horizontal([85., 53., 125., 54.]);
    for c in &mut f.table.cells {
        if c.words.contains(&id) {
            c.words.clear();
            c.text.clear();
        }
    }
    reject(f);
}
#[test]
fn a_verified_overlay_inside_sparse_prose_does_not_create_a_value_record() {
    let mut f = fixture(false, 4);
    let id = add(&mut f, [170., 92., 180., 93.], "─");
    f.words[id].text.clear();
    f.words[id].status = WordStatus::Empty;
    f.originals.retain(|c| c.id != id);
    f.rulings[id] = NativeRuling::Horizontal([170., 92., 180., 93.]);
    native(f);
}
#[test]
fn a_graphic_at_an_other_field_record_band_remains_opaque_even_if_it_overlaps_text() {
    let mut f = fixture(false, 3);
    let id = add(&mut f, [170., 52., 180., 53.], "─");
    f.words[id].text.clear();
    f.words[id].status = WordStatus::Empty;
    f.originals.retain(|c| c.id != id);
    f.rulings[id] = NativeRuling::Horizontal([170., 52., 180., 53.]);
    reject(f);
}
#[test]
fn one_native_spanning_sentence_cell_can_cover_real_physical_record_rows() {
    let mut f = fixture(false, 3);
    let bottom = f.table.y1;
    f.grids[0].ys.insert(2, 60.);
    let mut cells: Vec<_> = (0..3)
        .map(|x| GridCell {
            x0: x,
            x1: x + 1,
            y0: 0,
            y1: 1,
        })
        .collect();
    for x in 0..2 {
        for y in 1..3 {
            cells.push(GridCell {
                x0: x,
                x1: x + 1,
                y0: y,
                y1: y + 1,
            });
        }
    }
    cells.push(GridCell {
        x0: 2,
        x1: 3,
        y0: 1,
        y1: 3,
    });
    f.grids[0].cells = cells;
    f.rules.horizontal.push(HRule {
        y: 60.,
        x0: 0.,
        x1: 160.,
    });
    for c in &mut f.table.cells {
        if c.row > 0 && c.col < 2 {
            if c.row == 1 {
                c.y1 = 60.;
            } else {
                c.y0 = 60.;
                c.y1 = bottom;
            }
        }
    }
    let t = prove(&f)
        .expect("One true source rowspan must not be fragmented at neighboring real record rows");
    assert_eq!(t.rows, 3);
    let cs: Vec<_> = t.cells.iter().filter(|c| c.col == 2 && c.row > 0).collect();
    assert_eq!(cs.len(), 1);
    assert_eq!(cs[0].rowspan, 2);
    assert_eq!(cs[0].words, f.source_cells[5]);
    for c in f.table.cells.iter().filter(|c| c.col < 2) {
        assert!(t.cells.contains(c));
    }
}
#[test]
fn a_native_colspan_header_does_not_limit_sentence_scope_to_rectangular_grids() {
    let mut f = fixture(false, 3);
    f.grids[0].cells = (0..3)
        .map(|x| GridCell {
            x0: x,
            x1: x + 1,
            y0: 1,
            y1: 2,
        })
        .chain([GridCell {
            x0: 0,
            x1: 3,
            y0: 0,
            y1: 1,
        }])
        .collect();
    for v in &mut f.rules.vertical {
        if v.x == 80. || v.x == 160. {
            v.y0 = 40.;
        }
    }
    let ids = f.source_cells[..3]
        .iter()
        .flatten()
        .copied()
        .collect::<Vec<_>>();
    f.table.cells.retain(|c| c.row > 0);
    cell(&mut f, 0, 0, [0., 20., 240., 40.], ids.clone());
    let c = f.table.cells.last_mut().unwrap();
    c.colspan = 3;
    let t = prove(&f).unwrap();
    assert_eq!((t.rows, t.cols, t.cells.len()), (2, 3, 4));
    let h = t.cells.iter().find(|c| c.row == 0).unwrap();
    assert_eq!(h.colspan, 3);
    assert_eq!(h.words, ids);
}

#[test]
fn a_scalar_value_among_text_values_and_a_note_is_not_a_prose_sentence() {
    for value in ["2.57％", "１００", "1/2"] {
        let mut f = fixture(false, 3);
        let ids = f.source_cells[5].clone();
        patch(&mut f, ids[0], value);
        patch(&mut f, ids[1], "任意の期間配分基準");
        patch(&mut f, ids[2], "別の項目に付く任意の注記。");
        reject(f);
    }
}
#[test]
fn parallel_parenthetical_field_labels_do_not_become_one_sentence() {
    for (a, b) in [
        ("（甲）", "（乙）"),
        ("(alpha)", "(beta)"),
        ("［任意］", "［別項］"),
    ] {
        let mut f = fixture(false, 3);
        let ids = f.source_cells[5].clone();
        patch(&mut f, ids[0], &format!("{a}任意の値"));
        patch(&mut f, ids[1], &format!("{b}異なる説明"));
        reject(f);
    }
}

#[test]
fn same_characters_in_a_different_current_order_do_not_borrow_original_sentence_scope() {
    let mut f = fixture(false, 3);
    let c = f
        .table
        .cells
        .iter_mut()
        .find(|c| c.col == 2 && c.row == 1)
        .unwrap();
    c.text = c.text.chars().rev().collect();
    reject(f);
}
use crate::native_word_flow::NativeFlow;
fn flow_add(f: &mut F, b: [f32; 4], baseline: f64, text: &str) -> usize {
    let id = add(f, b, text);
    f.flows[id] = Some(NativeFlow{mode:None,
        start: f64::from(b[0]),
        baseline,
        end: f64::from(b[2]),
        leading: 8., trailing:0., prefix:0. });
    id
}
fn paragraph_field(n: usize, sections: usize, atomic: bool) -> F {
    assert!(n >= sections && sections >= 2);
    let bottom = 80. + 12. * n.max(sections + 1) as f32;
    let mut f = F {
        table: Table {
            x0: 0.,
            y0: 20.,
            x1: 160.,
            y1: bottom,
            rows: sections + 1,
            cols: 2,
            cells: vec![],
        },
        words: vec![],
        originals: vec![],
        bounds: vec![],
        axes: vec![],
        rulings: vec![],
        flows: vec![],
        grids: vec![Grid {
            xs: vec![0., 80., 160.],
            ys: vec![20., 40., bottom],
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
        }],
        rules: PageRules::default(),
        source_cells: vec![vec![]; 4],
    };
    for x in [0., 80., 160.] {
        f.rules.vertical.push(VRule {
            x,
            y0: 20.,
            y1: bottom,
        });
    }
    for y in [20., 40., bottom] {
        f.rules.horizontal.push(HRule {
            y,
            x0: 0.,
            x1: 160.,
        });
    }
    for c in 0..2 {
        let x = 5. + 80. * c as f32;
        let id = flow_add(&mut f, [x, 24., x + 55., 32.184], 30., "任意の見出し");
        f.source_cells[c].push(id);
        cell(
            &mut f,
            0,
            c,
            [80. * c as f32, 20., 80. * (c + 1) as f32, 40.],
            vec![id],
        );
    }
    let mut ids = vec![];
    for i in 0..n {
        let y = 50. + 12. * i as f64;
        let end = if i + 1 == n { 29. } else { 77. };
        let id = flow_add(
            &mut f,
            [5., (y - 6.) as f32, end, (y + 2.184) as f32],
            y,
            "任意の項目名",
        );
        ids.push(id);
    }
    f.source_cells[2] = ids.clone();
    for r in 0..sections {
        let group = if r + 1 == sections {
            ids[r..].to_vec()
        } else {
            vec![ids[r]]
        };
        cell(&mut f, r + 1, 0, [0., 40., 80., bottom], group);
    }
    for r in 0..sections {
        let y = 50. + 12. * r as f64;
        let terminal = if atomic && r + 1 == sections {
            "。"
        } else {
            ""
        };
        let text = format!("（任意{r}）別の説明{terminal}");
        let id = flow_add(
            &mut f,
            [85., (y - 6.) as f32, 155., (y + 2.184) as f32],
            y,
            &text,
        );
        let mut ids = vec![id];
        if !atomic && r + 1 == sections {
            let y = y + 12.;
            ids.push(flow_add(
                &mut f,
                [85., (y - 6.) as f32, 130., (y + 2.184) as f32],
                y,
                "末尾。",
            ));
        }
        f.source_cells[3].extend(&ids);
        cell(&mut f, r + 1, 1, [80., 40., 160., bottom], ids);
    }
    f
}
fn one_field(f: F) -> Table {
    let t=prove(&f).expect("Complete source paragraph and its parallel sections remain inside their one original physical field row");
    assert_eq!(
        (t.rows, t.cols, t.cells.len()),
        (2, f.table.cols, f.table.cols * 2)
    );
    for (row, col, ids) in f
        .source_cells
        .iter()
        .enumerate()
        .map(|(i, ids)| (i / f.table.cols, i % f.table.cols, ids))
    {
        let c = t
            .cells
            .iter()
            .find(|c| c.row == row && c.col == col)
            .unwrap();
        assert_eq!(&c.words, ids);
    }
    t
}
#[test]
fn native_wrapped_field_and_its_sections_have_no_line_or_section_count_cap() {
    for n in [2, 3, 7, 29] {
        one_field(paragraph_field(n, 2, false));
    }
    for s in [2, 3, 11] {
        one_field(paragraph_field(s + 3, s, false));
    }
}
#[test]
fn native_field_scope_uses_arbitrary_body_words_and_section_labels() {
    for body in ["x", "未見の項目名", "unseen field label"] {
        let mut f = paragraph_field(3, 2, false);
        for id in f.source_cells[2].clone() {
            patch(&mut f, id, body);
        }
        for (id, text) in f.source_cells[3][..2].to_vec().into_iter().zip([
            "[alpha] arbitrary description",
            "[beta] unrelated explanation",
        ]) {
            patch(&mut f, id, text);
        }
        one_field(f);
    }
}
#[test]
fn a_column_of_atomic_value_records_keeps_its_logical_rows() {
    reject(paragraph_field(7, 2, true));
}
#[test]
fn a_missing_native_flow_never_uses_font_box_overlap_as_a_fallback() {
    for id in [2, 3, 5] {
        let mut f = paragraph_field(3, 2, false);
        f.flows[id] = None;
        reject(f);
    }
}
#[test]
fn a_next_glyph_that_fits_the_previous_line_supplies_no_wrap_scope() {
    let mut f = paragraph_field(3, 2, false);
    for id in f.source_cells[2][..2].to_vec() {
        f.words[id].x1 = 65.;
        f.words[id].source_box[2] = 65.;
        f.originals.iter_mut().find(|c| c.id == id).unwrap().x1 = 65.;
        if let NativeBounds::Glyphs(ref mut b) = f.bounds[id] {
            b[2] = 65.;
        }
        f.flows[id].as_mut().unwrap().end = 65.;
    }
    reject(f);
}
#[test]
fn a_top_level_sentence_end_stops_a_forced_field_continuation() {
    let mut f = paragraph_field(3, 2, false);
    let id = f.source_cells[2][0];
    patch(&mut f, id, "既に完結した文。");
    reject(f);
}
#[test]
fn internal_quoted_periods_do_not_end_a_wrapped_field() {
    let mut f = paragraph_field(3, 2, false);
    let ids = f.source_cells[2].clone();
    patch(&mut f, ids[0], "任意（「引用。");
    patch(&mut f, ids[1], "続き」）の");
    patch(&mut f, ids[2], "項目名");
    one_field(f);
}
#[test]
fn an_incomplete_or_mismatched_quote_does_not_supply_field_scope() {
    for text in ["任意（「引用", "任意）説明", "任意［説明）"] {
        let mut f = paragraph_field(3, 2, false);
        let id = f.source_cells[2][0];
        patch(&mut f, id, text);
        reject(f);
    }
}
#[test]
fn parallel_field_sections_preserve_their_paragraph_breaks() {
    let t = one_field(paragraph_field(7, 3, false));
    let c = t.cells.iter().find(|c| c.row == 1 && c.col == 1).unwrap();
    assert_eq!(c.text.matches('\n').count(), 2);
}
#[test]
fn complete_native_field_scope_does_not_need_a_sentence_terminal() {
    let mut f = paragraph_field(3, 2, false);
    let id = *f.source_cells[3].last().unwrap();
    patch(&mut f, id, "末尾");
    one_field(f);
}
#[test]
fn a_scalar_value_with_other_text_values_is_not_one_field() {
    let mut f = paragraph_field(3, 2, false);
    let id = f.source_cells[3][0];
    patch(&mut f, id, "2.57％");
    reject(f);
}
#[test]
fn a_true_source_row_boundary_is_not_removed_by_wrap_capacity() {
    let mut f = paragraph_field(3, 2, false);
    let bottom = f.table.y1;
    f.grids[0].ys.insert(2, 60.);
    f.grids[0].cells = (0..3)
        .flat_map(|y| {
            (0..2).map(move |x| GridCell {
                x0: x,
                x1: x + 1,
                y0: y,
                y1: y + 1,
            })
        })
        .collect();
    f.rules.horizontal.push(HRule {
        y: 60.,
        x0: 0.,
        x1: 160.,
    });
    for c in &mut f.table.cells {
        if c.row == 1 {
            c.y1 = 60.;
        }
        if c.row == 2 {
            c.y0 = 60.;
            c.y1 = bottom;
        }
    }
    reject(f);
}
fn with_common_scalar(mut f: F) -> F {
    let old = f.source_cells.clone();
    let shifted: std::collections::HashSet<_> = old[1].iter().chain(&old[3]).copied().collect();
    for id in shifted {
        let w = &mut f.words[id];
        w.x0 += 80.;
        w.x1 += 80.;
        w.source_box[0] += 80.;
        w.source_box[2] += 80.;
        let c = f.originals.iter_mut().find(|c| c.id == id).unwrap();
        c.x += 80.;
        c.x1 += 80.;
        if let NativeBounds::Glyphs(ref mut b) = f.bounds[id] {
            b[0] += 80.;
            b[2] += 80.;
        }
        let n = f.flows[id].as_mut().unwrap();
        n.start += 80.;
        n.end += 80.;
    }
    for c in &mut f.table.cells {
        if c.col == 1 {
            c.col = 2;
            c.x0 += 80.;
            c.x1 += 80.;
        }
    }
    f.table.cols = 3;
    f.table.x1 = 240.;
    let bottom = f.table.y1;
    let g = &mut f.grids[0];
    for c in &mut g.cells {
        if c.x0 == 1 {
            c.x0 += 1;
            c.x1 += 1;
        }
    }
    g.xs = vec![0., 80., 160., 240.];
    for y in 0..2 {
        g.cells.push(GridCell {
            x0: 1,
            x1: 2,
            y0: y,
            y1: y + 1,
        });
    }
    for v in &mut f.rules.vertical {
        if v.x == 160. {
            v.x = 240.;
        }
    }
    f.rules.vertical.push(VRule {
        x: 160.,
        y0: 20.,
        y1: bottom,
    });
    for h in &mut f.rules.horizontal {
        h.x1 = 240.;
    }
    let h = flow_add(&mut f, [85., 24., 140., 32.184], 30., "別の欄");
    cell(&mut f, 0, 1, [80., 20., 160., 40.], vec![h]);
    let v = flow_add(&mut f, [85., 44., 140., 52.184], 50., "100");
    cell(&mut f, 1, 1, [80., 40., 160., bottom], vec![v]);
    f.source_cells = vec![
        old[0].clone(),
        vec![h],
        old[1].clone(),
        old[2].clone(),
        vec![v],
        old[3].clone(),
    ];
    f
}
#[test]
fn one_shared_scalar_cell_is_not_a_column_of_independent_value_records() {
    one_field(with_common_scalar(paragraph_field(7, 3, false)));
}
#[test]
fn two_independent_scalar_records_keep_their_field_rows() {
    let mut f = with_common_scalar(paragraph_field(7, 2, false));
    let bottom = f.table.y1;
    let id = flow_add(&mut f, [85., 56., 140., 64.184], 62., "200");
    f.source_cells[4].push(id);
    cell(&mut f, 2, 1, [80., 40., 160., bottom], vec![id]);
    reject(f);
}
#[test]
fn an_unrelated_header_without_flow_does_not_block_a_complete_body_field() {
    let mut f = paragraph_field(7, 3, false);
    f.flows[0] = None;
    one_field(f);
}
#[test]
fn original_field_flow_survives_independent_scale_and_translation() {
    for (sx, sy, tx, ty) in [(0.5f32, 3., 30., 7.), (4., 0.25, 5., 20.)] {
        let mut f = paragraph_field(7, 3, false);
        let tr = |b: [f32; 4]| {
            [
                b[0] * sx + tx,
                b[1] * sy + ty,
                b[2] * sx + tx,
                b[3] * sy + ty,
            ]
        };
        for i in 0..f.words.len() {
            let b = tr(f.words[i].source_box);
            let w = &mut f.words[i];
            w.source_box = b;
            w.x0 = b[0];
            w.y0 = b[1];
            w.x1 = b[2];
            w.y1 = b[3];
            f.bounds[i] = NativeBounds::Glyphs(b);
            let c = f.originals.iter_mut().find(|c| c.id == i).unwrap();
            c.x = b[0];
            c.y = b[1];
            c.x1 = b[2];
            c.y1 = b[3];
            let n = f.flows[i].as_mut().unwrap();
            n.start = n.start * f64::from(sx) + f64::from(tx);
            n.end = n.end * f64::from(sx) + f64::from(tx);
            n.leading *= f64::from(sx);
            n.trailing *= f64::from(sx);
            n.prefix *= f64::from(sx);
            n.baseline = n.baseline * f64::from(sy) + f64::from(ty);
        }
        for c in &mut f.table.cells {
            let b = tr([c.x0, c.y0, c.x1, c.y1]);
            c.x0 = b[0];
            c.y0 = b[1];
            c.x1 = b[2];
            c.y1 = b[3];
        }
        let b = tr([f.table.x0, f.table.y0, f.table.x1, f.table.y1]);
        f.table.x0 = b[0];
        f.table.y0 = b[1];
        f.table.x1 = b[2];
        f.table.y1 = b[3];
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
        one_field(f);
    }
}

#[test]fn numbers_in_an_explicit_section_continuation_do_not_create_new_records(){for s in ["1034 （139.10）","１２３４","0.55％","1,000"]{let mut f=paragraph_field(7,2,false);let id=*f.source_cells[3].last().unwrap();patch(&mut f,id,s);one_field(f);}}
#[test]fn numeric_heads_remain_independent_even_with_balanced_parentheses(){for s in ["(100)200","（１００）２００"]{let mut f=paragraph_field(7,2,false);let id=f.source_cells[3][0];patch(&mut f,id,s);reject(f);}}

fn already_complete_shared_description(mut f:F)->F{let ids=f.source_cells[2].clone();f.table.cells.retain(|c|!(c.col==0 && c.row>1));let text:String=ids.iter().map(|id|f.originals.iter().find(|w|w.id==*id).unwrap().text.as_str()).collect();let c=f.table.cells.iter_mut().find(|c|c.col==0&&c.row==1).unwrap();c.words=ids;c.text=text;f}
#[test]fn a_complete_source_wrapped_cell_can_already_be_one_logical_fragment(){for n in [3,7,29]{one_field(already_complete_shared_description(paragraph_field(n,2,false)));}}
#[test]fn a_shared_complete_description_does_not_erase_atomic_counterpart_records(){reject(already_complete_shared_description(paragraph_field(3,2,true)));}

fn numbered_value_continuations(n:usize,indent:f32)->F{let mut f=paragraph_field(n+4,n+2,false);let ids=f.source_cells[3].clone();patch(&mut f,ids[0],"（13）任意の所在地");for &id in &ids[1..n+1]{patch(&mut f,id,"（309.90）");let w=&mut f.words[id];w.x0+=indent;w.source_box[0]+=indent;let c=f.originals.iter_mut().find(|c|c.id==id).unwrap();c.x+=indent;if let NativeBounds::Glyphs(ref mut b)=f.bounds[id]{b[0]+=indent;}f.flows[id].as_mut().unwrap().start+=f64::from(indent);}patch(&mut f,ids[n+1],"（14）別の所在地");f}
#[test]fn indented_parenthetical_values_can_continue_numbered_source_sections(){for n in [1,3,11]{for indent in [0.25,8.,24.]{one_field(numbered_value_continuations(n,indent));}}}
#[test]fn fullwidth_numbered_sections_and_values_have_the_same_structure(){let mut f=numbered_value_continuations(1,8.);let ids=f.source_cells[3].clone();patch(&mut f,ids[0],"（１３）任意の所在地");patch(&mut f,ids[1],"（３０９．９０）");patch(&mut f,ids[2],"（１４）別の所在地");one_field(f);}
#[test]fn a_value_at_the_same_source_margin_remains_an_independent_record(){reject(numbered_value_continuations(1,0.));}
#[test]fn an_unparenthesized_number_does_not_borrow_a_numbered_section(){let mut f=numbered_value_continuations(1,8.);let id=f.source_cells[3][1];patch(&mut f,id,"309.90");reject(f);}
#[test]fn a_label_without_an_integer_ordinal_cannot_claim_a_bare_parenthetical_value(){let mut f=numbered_value_continuations(1,8.);let ids=f.source_cells[3].clone();patch(&mut f,ids[0],"（甲）任意の所在地");patch(&mut f,ids[2],"（乙）別の所在地");reject(f);}
#[test]fn numeric_ordinal_value_heads_still_certify_independent_scalar_records(){let mut f=numbered_value_continuations(1,8.);let ids=f.source_cells[3].clone();patch(&mut f,ids[0],"（13）200");patch(&mut f,ids[2],"（14）400");reject(f);}
#[test]fn parenthetical_values_do_not_erase_an_atomic_counterpart_column(){let mut f=numbered_value_continuations(1,8.);let tail=*f.source_cells[3].last().unwrap();let b=if let NativeBounds::Glyphs(b)=f.bounds[tail]{b}else{unreachable!()};let baseline=f.flows[tail-1].unwrap().baseline;let dy=baseline-f.flows[tail].unwrap().baseline;let nb=[b[0],(f64::from(b[1])+dy)as f32,b[2],(f64::from(b[3])+dy)as f32];let w=&mut f.words[tail];w.y0=nb[1];w.y1=nb[3];w.source_box=nb;let c=f.originals.iter_mut().find(|c|c.id==tail).unwrap();c.y=nb[1];c.y1=nb[3];f.bounds[tail]=NativeBounds::Glyphs(nb);f.flows[tail].as_mut().unwrap().baseline=baseline;reject(f);}

#[test]fn ordinal_digits_do_not_turn_sign_only_scalar_heads_into_explanatory_sections(){for body in ["－","△","▲","+%"]{let mut f=numbered_value_continuations(1,8.);let ids=f.source_cells[3].clone();patch(&mut f,ids[0],&format!("（13）{body}"));patch(&mut f,ids[2],&format!("（14）{body}"));reject(f);}}

#[test]fn a_later_explanatory_line_does_not_hide_a_numeric_source_section_head(){for body in ["－","△","123","+%"]{let mut f=numbered_value_continuations(1,8.);let id=f.source_cells[3][2];patch(&mut f,id,&format!("（14）{body}"));reject(f);}}

#[test]fn a_wordless_native_cell_cannot_panic_while_other_fields_are_reconciled(){let mut f=with_common_scalar(paragraph_field(7,3,false));let n=f.words.len()-2;f.words.truncate(n);f.originals.retain(|w|w.id<n);f.bounds.truncate(n);f.axes.truncate(n);f.rulings.truncate(n);f.flows.truncate(n);for c in &mut f.table.cells{if c.col==1{c.words.clear();c.text.clear();}}f.source_cells[1].clear();f.source_cells[4].clear();let result=std::panic::catch_unwind(||prove(&f));assert!(result.is_ok(),"An empty field is not evidence of an ordinal head and must remain safe");}
