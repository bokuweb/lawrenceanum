use super::*;
use crate::{
    grid::{Grid, GridCell},
    lines::{HRule, PageRules, VRule},
    native_word_axes::NativeAxis,
    trace::{Line, NativeBounds, Unit, Word, WordStatus},
    vertical::Col,
};
struct Fixture {
    units: Vec<Unit>,
    words: Vec<Word>,
    bounds: Vec<NativeBounds>,
    axes: Vec<NativeAxis>,
    grids: Vec<Grid>,
    rules: PageRules,
    expected: Vec<usize>,
}
fn fixture(rows: usize, cols: usize, label: &str) -> Fixture {
    let xs: Vec<_> = (0..=cols).map(|i| 60. + 140. * i as f32).collect();
    let ys: Vec<_> = (0..=rows).map(|i| 30. + 120. * i as f32).collect();
    let mut words = Vec::new();
    let mut cells = Vec::new();
    for r in 0..rows {
        for c in 0..cols {
            cells.push(GridCell {
                x0: c,
                x1: c + 1,
                y0: r,
                y1: r + 1,
            });
            for tail in 0..if c == 0 { 2 } else { 1 } {
                let id = words.len();
                let x = xs[c] + 20.;
                let y = ys[r] + 15. + 65. * tail as f32;
                words.push(Word {
                    id,
                    x0: x,
                    y0: y,
                    x1: x + 30.,
                    y1: y + 8.,
                    source_box: [x, y, x + 30., y + 8.],
                    raw: label.into(),
                    text: label.into(),
                    status: WordStatus::Used,
                });
            }
        }
    }
    let bounds = words
        .iter()
        .map(|w| NativeBounds::Glyphs([w.x0, w.y0, w.x1, w.y1]))
        .collect();
    let axes = vec![NativeAxis::Right; words.len()];
    let expected = words.iter().map(|w| w.id).collect();
    let units = vec![Unit::Plain(crate::horizontal::row_lines(
        words.iter().map(col).collect(),
    ))];
    let rules = PageRules {
        vertical: xs
            .iter()
            .map(|&x| VRule {
                x,
                y0: ys[0],
                y1: *ys.last().unwrap(),
            })
            .collect(),
        horizontal: ys
            .iter()
            .map(|&y| HRule {
                y,
                x0: xs[0],
                x1: *xs.last().unwrap(),
            })
            .collect(),
        ..Default::default()
    };
    Fixture {
        units,
        words,
        bounds,
        axes,
        grids: vec![Grid { xs, ys, cells }],
        rules,
        expected,
    }
}
fn col(w: &Word) -> Col {
    Col {
        id: w.id,
        x: w.x0,
        y: w.y0,
        x1: w.x1,
        y1: w.y1,
        text: w.text.clone(),
    }
}
fn scopes(units: &[Unit]) -> Vec<(String, Vec<Line>)> {
    let mut out = Vec::new();
    for u in units {
        match u {
            Unit::Plain(ls) => out.push(("plain".into(), ls.clone())),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            } => {
                out.push(("preamble".into(), preamble.clone()));
                for (name, parts) in [("after", after), ("before", before), ("outside", outside)] {
                    for part in parts {
                        out.push((name.into(), part.clone()));
                    }
                }
            }
        }
    }
    out
}
fn ids(units: &[Unit]) -> Vec<usize> {
    scopes(units)
        .into_iter()
        .flat_map(|(_, ls)| {
            ls.into_iter().flat_map(|l| {
                let mut ids = l.words;
                for t in l.table {
                    for c in t.cells {
                        ids.extend(c.words)
                    }
                }
                ids
            })
        })
        .collect()
}
fn run(f: &mut Fixture) {
    restore(
        &mut f.units,
        &f.words,
        &f.bounds,
        &f.axes,
        &f.grids,
        &f.rules,
    );
}
fn unchanged(mut f: Fixture) {
    let old = scopes(&f.units);
    run(&mut f);
    assert_eq!(scopes(&f.units), old);
}
#[test]
fn complete_first_cell_precedes_adjacent_cell() {
    let mut f = fixture(1, 2, "任意");
    assert_ne!(ids(&f.units), f.expected);
    run(&mut f);
    assert_eq!(ids(&f.units), f.expected);
}
#[test]
fn arbitrary_cell_and_row_counts_have_no_result_specific_cap() {
    for rows in [1, 2, 4] {
        for cols in [2, 3, 7] {
            let mut f = fixture(rows, cols, "data");
            run(&mut f);
            assert_eq!(ids(&f.units), f.expected);
        }
    }
}
#[test]
fn cell_flow_does_not_depend_on_japanese_words_or_lengths() {
    for text in [
        "x",
        "甲乙丙丁戊己庚辛壬癸任意の長い文字列",
        "（一般）",
        "x+y",
    ] {
        let mut f = fixture(1, 3, text);
        run(&mut f);
        assert_eq!(ids(&f.units), f.expected);
    }
}
#[test]
fn a_cell_can_close_its_own_multiline_citation() {
    let mut f = fixture(1, 2, "outside");
    f.words[0].text = "（begin".into();
    f.words[1].text = "end）".into();
    f.units = vec![Unit::Plain(crate::horizontal::row_lines(
        f.words.iter().map(col).collect(),
    ))];
    run(&mut f);
    assert_eq!(ids(&f.units), f.expected);
}
#[test]
fn source_scale_translation_and_grid_cell_storage_order_do_not_change_flow() {
    for scale in [0.5, 1., 3.] {
        let mut f = fixture(2, 3, "data");
        for w in &mut f.words {
            w.x0 = w.x0 * scale + 13.;
            w.x1 = w.x1 * scale + 13.;
            w.y0 = w.y0 * scale + 19.;
            w.y1 = w.y1 * scale + 19.;
        }
        f.bounds = f
            .words
            .iter()
            .map(|w| NativeBounds::Glyphs([w.x0, w.y0, w.x1, w.y1]))
            .collect();
        for g in &mut f.grids {
            for x in &mut g.xs {
                *x = *x * scale + 13.
            }
            for y in &mut g.ys {
                *y = *y * scale + 19.
            }
            g.cells.reverse();
        }
        for r in &mut f.rules.vertical {
            r.x = r.x * scale + 13.;
            r.y0 = r.y0 * scale + 19.;
            r.y1 = r.y1 * scale + 19.;
        }
        for r in &mut f.rules.horizontal {
            r.y = r.y * scale + 19.;
            r.x0 = r.x0 * scale + 13.;
            r.x1 = r.x1 * scale + 13.;
        }
        f.units = vec![Unit::Plain(crate::horizontal::row_lines(
            f.words.iter().map(col).collect(),
        ))];
        run(&mut f);
        assert_eq!(ids(&f.units), f.expected);
    }
}
#[test]
fn missing_original_bounds_prevent_a_complete_membership_proof() {
    let mut f = fixture(1, 2, "data");
    f.bounds.truncate(1);
    unchanged(f);
}
#[test]
fn unknown_native_rectangle_is_not_a_compatibility_fallback() {
    let mut f = fixture(1, 2, "data");
    f.bounds[0] = NativeBounds::Unknown;
    unchanged(f);
}
#[test]
fn only_participating_rightward_words_are_supported() {
    for axis in [
        NativeAxis::Unknown,
        NativeAxis::Compatibility,
        NativeAxis::Down,
        NativeAxis::Left,
        NativeAxis::Up,
    ] {
        let mut f = fixture(1, 2, "data");
        f.axes[0] = axis;
        unchanged(f);
    }
}
#[test]
fn a_glyph_cell_crossing_a_physical_cell_boundary_is_not_reassigned_by_center() {
    let mut f = fixture(1, 2, "data");
    f.bounds[0] = NativeBounds::Glyphs([80., 45., 220., 53.]);
    unchanged(f);
}
#[test]
fn invalid_native_geometry_is_insufficient() {
    for b in [
        [80., f32::NAN, 110., 53.],
        [80., 45., f32::INFINITY, 53.],
        [110., 45., 80., 53.],
    ] {
        let mut f = fixture(1, 2, "data");
        f.bounds[0] = NativeBounds::Glyphs(b);
        unchanged(f);
    }
}
#[test]
fn original_word_ids_must_match_their_own_metadata_slots() {
    let mut f = fixture(1, 2, "data");
    f.words[0].id = 1;
    unchanged(f);
}
#[test]
fn an_original_word_missing_from_the_current_scope_is_not_silently_dropped() {
    let mut f = fixture(1, 2, "data");
    if let Unit::Plain(ls) = &mut f.units[0] {
        ls[0].words.retain(|&i| i != 2);
    }
    unchanged(f);
}
#[test]
fn duplicate_ownership_anywhere_prevents_reordering() {
    let mut f = fixture(1, 2, "data");
    f.units
        .push(Unit::Plain(vec![Line::text("data".into(), vec![0])]));
    unchanged(f);
}
#[test]
fn an_already_modified_line_cannot_be_reconstructed_from_stale_words() {
    let mut f = fixture(1, 2, "data");
    if let Unit::Plain(ls) = &mut f.units[0] {
        ls[0].text.push_str(" injected");
    }
    unchanged(f);
}
#[test]
fn current_tables_are_not_replaced_or_reordered() {
    let mut f = fixture(1, 2, "data");
    let table = crate::trace::Table {
        x0: 60.,
        y0: 30.,
        x1: 340.,
        y1: 150.,
        rows: 1,
        cols: 1,
        cells: vec![crate::trace::TableCell {
            row: 0,
            col: 0,
            rowspan: 1,
            colspan: 1,
            x0: 60.,
            y0: 30.,
            x1: 340.,
            y1: 150.,
            text: "data".into(),
            words: vec![0],
        }],
    };
    f.units = vec![Unit::Plain(vec![
        Line {
            text: table.render(),
            words: vec![],
            table: Some(table),
        },
        Line::text("data".into(), vec![1, 2]),
    ])];
    unchanged(f);
}
#[test]
fn a_grid_split_between_after_and_before_scopes_is_not_moved_across_roles() {
    let mut f = fixture(1, 2, "data");
    f.units = vec![Unit::Shinkyu {
        preamble: vec![],
        after: vec![crate::horizontal::row_lines(
            f.words[..2].iter().map(col).collect(),
        )],
        before: vec![crate::horizontal::row_lines(
            f.words[2..].iter().map(col).collect(),
        )],
        outside: vec![],
    }];
    unchanged(f);
}
#[test]
fn a_missing_or_short_separator_does_not_certify_cell_flow() {
    for short in [false, true] {
        let mut f = fixture(1, 2, "data");
        f.rules.vertical.retain(|r| r.x != 200.);
        if short {
            f.rules.vertical.push(VRule {
                x: 200.,
                y0: 40.,
                y1: 60.,
            });
        }
        unchanged(f);
    }
}
#[test]
fn overlapping_grid_cells_are_not_a_partition() {
    let mut f = fixture(1, 2, "data");
    f.grids[0].cells[0].x1 = 2;
    unchanged(f);
}
#[test]
fn a_citation_that_only_closes_in_another_cell_cannot_authorize_a_new_flow() {
    let mut f = fixture(1, 2, "data");
    f.words[0].text = "（begin".into();
    f.words[2].text = "end）".into();
    f.units = vec![Unit::Plain(crate::horizontal::row_lines(
        f.words.iter().map(col).collect(),
    ))];
    unchanged(f);
}
#[test]
fn already_cell_ordered_source_lines_are_byte_identical() {
    let mut f = fixture(1, 2, "data");
    f.units = vec![Unit::Plain(
        f.words
            .iter()
            .map(|w| Line::text(w.text.clone(), vec![w.id]))
            .collect(),
    )];
    unchanged(f);
}
#[test]
fn a_foreign_word_in_the_candidate_line_window_is_not_displaced() {
    let mut f = fixture(1, 2, "data");
    let id = f.words.len();
    f.words.push(Word {
        id,
        x0: 500.,
        y0: 45.,
        x1: 530.,
        y1: 53.,
        source_box: [500., 45., 530., 53.],
        raw: "foreign".into(),
        text: "foreign".into(),
        status: WordStatus::Used,
    });
    f.bounds.push(NativeBounds::Glyphs([500., 45., 530., 53.]));
    f.axes.push(NativeAxis::Right);
    f.units = vec![Unit::Plain(crate::horizontal::row_lines(
        f.words.iter().map(col).collect(),
    ))];
    unchanged(f);
}
#[test]
fn ambiguous_duplicate_grid_ownership_is_not_guessed() {
    let mut f = fixture(1, 2, "data");
    f.grids.push(f.grids[0].clone());
    unchanged(f);
}
#[test]
fn a_nonparticipating_source_word_with_known_bounds_does_not_require_a_horizontal_axis() {
    let mut f = fixture(1, 2, "data");
    let id = f.words.len();
    f.words.push(Word {
        id,
        x0: 500.,
        y0: 300.,
        x1: 508.,
        y1: 330.,
        source_box: [500., 300., 508., 330.],
        raw: "outside".into(),
        text: "outside".into(),
        status: WordStatus::Used,
    });
    f.bounds
        .push(NativeBounds::Glyphs([500., 300., 508., 330.]));
    f.axes.push(NativeAxis::Down);
    f.units
        .push(Unit::Plain(vec![Line::text("outside".into(), vec![id])]));
    run(&mut f);
    let mut wanted = f.expected;
    wanted.push(id);
    assert_eq!(ids(&f.units), wanted);
}

include!("row_correspondence_cases.rs");

include!("printed_row_cases.rs");

include!("fragmented_record_cases.rs");

include!("bounded_outside_flow_cases.rs");
