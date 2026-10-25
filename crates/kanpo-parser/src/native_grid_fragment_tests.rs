use super::*;
use crate::{
    grid::GridCell,
    lines::{HRule, VRule},
};
struct Fixture {
    units: Vec<Unit>,
    words: Vec<Word>,
    bounds: Vec<NativeBounds>,
    axes: Vec<NativeAxis>,
    rules: PageRules,
    grid: Grid,
}
fn word(id: usize, b: Rect, text: &str) -> Word {
    Word {
        id,
        x0: b[0],
        y0: b[1],
        x1: b[2],
        y1: b[3],
        source_box: b,
        raw: text.into(),
        text: text.into(),
        status: WordStatus::Used,
    }
}
fn fixture() -> Fixture {
    let coords = [
        [5., 5., 35., 13.],
        [105., 5., 150., 13.],
        [105., 20., 155., 28.],
        [5., 55., 40., 63.],
        [105., 55., 140., 63.],
    ];
    let words = coords
        .iter()
        .zip(["first", "owner", "address", "second", "other"])
        .enumerate()
        .map(|(id, (&b, t))| word(id, b, t))
        .collect::<Vec<_>>();
    let cells = (0..2)
        .flat_map(|row| {
            (0..2).map(move |col| GridCell {
                x0: col,
                y0: row,
                x1: col + 1,
                y1: row + 1,
            })
        })
        .collect();
    let grid = Grid {
        xs: vec![0., 100., 200.],
        ys: vec![0., 50., 100.],
        cells,
    };
    let t = Table {
        x0: 0.,
        y0: 0.,
        x1: 200.,
        y1: 100.,
        rows: 2,
        cols: 2,
        cells: vec![TableCell {
            row: 0,
            col: 0,
            rowspan: 1,
            colspan: 1,
            x0: 0.,
            y0: 0.,
            x1: 100.,
            y1: 50.,
            text: "first".into(),
            words: vec![0],
        }],
    };
    let mut lines = vec![Line {
        text: t.render(),
        words: vec![],
        table: Some(t),
    }];
    lines.extend(
        words
            .iter()
            .skip(1)
            .map(|w| Line::text(w.text.clone(), vec![w.id])),
    );
    let mut rules = PageRules::default();
    rules.horizontal = [0., 50., 100.]
        .into_iter()
        .map(|y| HRule {
            y,
            x0: 0.,
            x1: 200.,
        })
        .collect();
    rules.vertical = [0., 100., 200.]
        .into_iter()
        .map(|x| VRule {
            x,
            y0: 0.,
            y1: 100.,
        })
        .collect();
    Fixture {
        units: vec![Unit::Plain(lines)],
        words,
        bounds: coords.into_iter().map(NativeBounds::Glyphs).collect(),
        axes: vec![NativeAxis::Right; 5],
        rules,
        grid,
    }
}
fn snapshot(u: &[Unit]) -> String {
    let mut rows = vec![];
    visit(u, |allowed, l| rows.push((allowed, l.clone())));
    serde_json::to_string(&rows).unwrap()
}
fn apply(f: &mut Fixture) {
    restore(
        &mut f.units,
        &f.words,
        &f.bounds,
        &f.axes,
        &f.rules,
        400.,
        200.,
    );
}
fn unchanged(mut f: Fixture) {
    let old = snapshot(&f.units);
    apply(&mut f);
    assert_eq!(snapshot(&f.units), old);
}
fn table(f: &Fixture) -> &Table {
    let Unit::Plain(l) = &f.units[0] else {
        panic!()
    };
    assert_eq!(l.len(), 1);
    l[0].table.as_ref().unwrap()
}
#[test]
fn complete_source_grid_recovers_a_wrapped_owner_cell() {
    let mut f = fixture();
    apply(&mut f);
    let t = table(&f);
    assert_eq!((t.rows, t.cols, t.cells.len()), (2, 2, 4));
    assert_eq!(t.cells[1].text, "owner\naddress");
    assert_eq!(t.cells[1].words, vec![1, 2]);
    assert_eq!(t.cells[2].text, "second");
}
#[test]
fn absent_painted_row_does_not_erase_existing_logical_cells() {
    let mut f = fixture();
    let Unit::Plain(lines) = &mut f.units[0] else {
        panic!()
    };
    let t = lines[0].table.as_mut().unwrap();
    for (row, id, y0, y1) in [(0, 1, 0., 16.), (1, 2, 16., 50.)] {
        t.cells.push(TableCell {
            row,
            col: 1,
            rowspan: 1,
            colspan: 1,
            x0: 100.,
            y0,
            x1: 200.,
            y1,
            text: f.words[id].text.clone(),
            words: vec![id],
        });
    }
    t.rows = 3;
    lines[0].text = t.render();
    lines.drain(1..3);
    unchanged(f);
}
#[test]
fn complete_table_is_left_verbatim_including_its_spaces() {
    let mut f = fixture();
    apply(&mut f);
    let Unit::Plain(l) = &mut f.units[0] else {
        panic!()
    };
    let t = l[0].table.as_mut().unwrap();
    t.cells[0].text = "first  ".into();
    l[0].text = t.render();
    unchanged(f);
}
#[test]
fn complete_logical_cell_ownership_does_not_require_identical_table_envelope() {
    let mut f = fixture();
    apply(&mut f);
    let Unit::Plain(l) = &mut f.units[0] else {
        panic!()
    };
    let t = l[0].table.as_mut().unwrap();
    t.x0 = 1.;
    t.x1 = 199.;
    t.cells[0].text = "first  ".into();
    l[0].text = t.render();
    unchanged(f);
}
#[test]
fn an_intact_fragment_cell_keeps_its_original_text_and_word_order() {
    let mut f = fixture();
    let Unit::Plain(l) = &mut f.units[0] else {
        panic!()
    };
    let t = l[0].table.as_mut().unwrap();
    t.cells[0].text = "first  ".into();
    l[0].text = t.render();
    apply(&mut f);
    assert_eq!(table(&f).cells[0].text, "first  ");
    assert_eq!(table(&f).cells[0].words, vec![0]);
    assert_eq!(table(&f).cells[1].text, "owner\naddress");
}
#[test]
fn certified_grid_core_is_translation_scale_and_text_independent() {
    for scale in [0.01, 0.5, 2., 25.] {
        let mut f = fixture();
        let transform = |b: Rect| {
            [
                b[0] * scale + 311.,
                b[1] * scale - 217.,
                b[2] * scale + 311.,
                b[3] * scale - 217.,
            ]
        };
        for (w, b) in f.words.iter_mut().zip(f.bounds.iter_mut()) {
            let NativeBounds::Glyphs(old) = b else {
                panic!()
            };
            let new = transform(*old);
            *b = NativeBounds::Glyphs(new);
            *w = word(w.id, new, &format!("different{}", w.id));
        }
        for x in &mut f.grid.xs {
            *x = *x * scale + 311.;
        }
        for y in &mut f.grid.ys {
            *y = *y * scale - 217.;
        }
        for h in &mut f.rules.horizontal {
            h.x0 = h.x0 * scale + 311.;
            h.x1 = h.x1 * scale + 311.;
            h.y = h.y * scale - 217.;
        }
        for v in &mut f.rules.vertical {
            v.x = v.x * scale + 311.;
            v.y0 = v.y0 * scale - 217.;
            v.y1 = v.y1 * scale - 217.;
        }
        let Unit::Plain(lines) = &mut f.units[0] else {
            panic!()
        };
        for (i, l) in lines.iter_mut().enumerate() {
            if let Some(t) = &mut l.table {
                let b = transform(table_box(t));
                t.x0 = b[0];
                t.y0 = b[1];
                t.x1 = b[2];
                t.y1 = b[3];
                t.cells[0].text = f.words[0].text.clone();
                l.text = t.render();
            } else {
                l.text = f.words[i].text.clone();
            }
        }
        let (s, first, l) =
            candidate(&f.units, &f.words, &f.bounds, &f.axes, &f.grid, &f.rules).unwrap();
        replace(&mut f.units, &s, first, l);
        assert_eq!(table(&f).cells[1].text, "different1\ndifferent2");
    }
}
#[test]
fn unknown_bounds_do_not_borrow_compatibility_geometry() {
    let mut f = fixture();
    f.bounds[2] = NativeBounds::Compatibility;
    unchanged(f);
}
#[test]
fn every_original_advance_must_be_explicit_horizontal() {
    for axis in [
        NativeAxis::Down,
        NativeAxis::Left,
        NativeAxis::Up,
        NativeAxis::Unknown,
        NativeAxis::Compatibility,
    ] {
        let mut f = fixture();
        f.axes[2] = axis;
        unchanged(f);
    }
}
#[test]
fn a_word_crossing_an_original_cell_wall_blocks_repair() {
    let mut f = fixture();
    f.bounds[2] = NativeBounds::Glyphs([95., 20., 155., 28.]);
    unchanged(f);
}
#[test]
fn an_original_word_without_a_logical_owner_blocks_repair() {
    let mut f = fixture();
    let Unit::Plain(l) = &mut f.units[0] else {
        panic!()
    };
    l.remove(2);
    unchanged(f);
}
#[test]
fn duplicate_source_ownership_across_units_is_ambiguous() {
    let mut f = fixture();
    f.units
        .push(Unit::Plain(vec![Line::text("owner".into(), vec![1])]));
    unchanged(f);
}
#[test]
fn duplicate_source_ownership_inside_a_table_is_ambiguous() {
    let mut f = fixture();
    let Unit::Plain(l) = &mut f.units[0] else {
        panic!()
    };
    l[0].table.as_mut().unwrap().cells[0].words.push(0);
    unchanged(f);
}
#[test]
fn an_intervening_opaque_line_is_not_crossed() {
    let mut f = fixture();
    let Unit::Plain(l) = &mut f.units[0] else {
        panic!()
    };
    l.insert(2, Line::text("opaque".into(), vec![]));
    unchanged(f);
}
#[test]
fn an_explicit_blank_line_is_not_crossed() {
    let mut f = fixture();
    let Unit::Plain(l) = &mut f.units[0] else {
        panic!()
    };
    l.insert(2, Line::text(String::new(), vec![]));
    unchanged(f);
}
#[test]
fn comparative_and_preamble_roles_are_never_borrowed() {
    for role in 0..3 {
        let mut f = fixture();
        let Unit::Plain(l) = f.units.remove(0) else {
            panic!()
        };
        f.units.push(match role {
            0 => Unit::Shinkyu {
                preamble: l,
                after: vec![],
                before: vec![],
                outside: vec![],
            },
            1 => Unit::Shinkyu {
                preamble: vec![],
                after: vec![l],
                before: vec![],
                outside: vec![],
            },
            _ => Unit::Shinkyu {
                preamble: vec![],
                after: vec![],
                before: vec![l],
                outside: vec![],
            },
        });
        unchanged(f);
    }
}
#[test]
fn outside_and_body_fragments_can_share_one_proven_original_table() {
    let mut f = fixture();
    let Unit::Plain(mut l) = f.units.remove(0) else {
        panic!()
    };
    let rest = l.split_off(1);
    f.units.push(Unit::Shinkyu {
        preamble: vec![],
        after: vec![],
        before: vec![],
        outside: vec![l],
    });
    f.units.push(Unit::Plain(rest));
    apply(&mut f);
    let Unit::Shinkyu { outside, .. } = &f.units[0] else {
        panic!()
    };
    assert_eq!(
        outside[0][0].table.as_ref().unwrap().cells[1].text,
        "owner\naddress"
    );
    let Unit::Plain(l) = &f.units[1] else {
        panic!()
    };
    assert!(l.is_empty());
}
#[test]
fn an_existing_table_crossing_the_source_frame_is_preserved() {
    let mut f = fixture();
    let Unit::Plain(l) = &mut f.units[0] else {
        panic!()
    };
    l[0].table.as_mut().unwrap().x0 = -10.;
    unchanged(f);
}
#[test]
fn unknown_inserted_material_is_not_erased() {
    let mut f = fixture();
    let Unit::Plain(l) = &mut f.units[0] else {
        panic!()
    };
    l[2].text.push_str(" extra");
    unchanged(f);
}
#[test]
fn there_must_be_an_actual_closed_original_grid() {
    let mut f = fixture();
    f.rules = PageRules::default();
    unchanged(f);
}
#[test]
fn no_typed_fragment_means_no_new_table_is_invented() {
    let mut f = fixture();
    let Unit::Plain(l) = &mut f.units[0] else {
        panic!()
    };
    l[0] = Line::text("first".into(), vec![0]);
    unchanged(f);
}
#[test]
fn an_original_article_heading_is_a_boundary() {
    let mut f = fixture();
    f.words[3].text = "〇任意省告示第一号".into();
    let Unit::Plain(l) = &mut f.units[0] else {
        panic!()
    };
    l[3].text = f.words[3].text.clone();
    unchanged(f);
}
#[test]
fn numeric_zero_at_a_cell_start_is_not_an_article_heading() {
    let mut f = fixture();
    f.words[3].text = "〇・五".into();
    let Unit::Plain(l) = &mut f.units[0] else {
        panic!()
    };
    l[3].text = f.words[3].text.clone();
    apply(&mut f);
    assert_eq!(table(&f).cells[2].text, "〇・五");
}
