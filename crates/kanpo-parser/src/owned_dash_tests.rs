use super::*;
fn word(ws: &mut Vec<Word>, x: f32, y: f32, text: &str) -> usize {
    let id = ws.len();
    ws.push(Word {
        source_box: [x, y, x + text.chars().count() as f32 * 4., y + 8.],
        id,
        x0: x,
        y0: y,
        x1: x + text.chars().count() as f32 * 4.,
        y1: y + 8.,
        raw: text.into(),
        text: text.into(),
        status: WordStatus::Used,
    });
    id
}
fn fixture() -> (Vec<Line>, Vec<Word>, Vec<(f32, f32, f32)>) {
    let mut ws = vec![];
    let mut ls = vec![];
    for (y, label, value) in [
        (10., "収入", Some("100千円")),
        (30., "費用", None),
        (50., "合計", Some("200千円")),
    ] {
        let mut ids = vec![word(&mut ws, 10., y, label)];
        let mut text = label.to_string();
        if let Some(v) = value {
            ids.push(word(&mut ws, 330., y, v));
            text.push_str(v);
        }
        ls.push(Line::text(text, ids));
    }
    let id = word(&mut ws, 342., 30., "\u{E61C}");
    ws[id].x1 = 350.;
    ws[id].text.clear();
    ws[id].status = WordStatus::Empty;
    (ls, ws, vec![(0., 0., 70.), (360., 0., 70.)])
}
fn run(mut l: Vec<Line>, mut w: Vec<Word>, r: Vec<(f32, f32, f32)>) -> (Vec<Line>, Vec<Word>) {
    plain(&mut l, &mut w, &r);
    (l, w)
}
fn unchanged(l: Vec<Line>, w: Vec<Word>, r: Vec<(f32, f32, f32)>) {
    let old = (format!("{l:?}"), format!("{w:?}"));
    let (l, w) = run(l, w, r);
    assert_eq!(old, (format!("{l:?}"), format!("{w:?}")));
}
#[test]
fn units_allow_a_unitless_zero() {
    let (l, w, r) = fixture();
    let (l, w) = run(l, w, r);
    assert_eq!(l[1].text, "費用　－");
    assert_eq!(w.last().unwrap().status, WordStatus::Used);
}
#[test]
fn repeat_is_idempotent() {
    let (l, w, r) = fixture();
    let (l, w) = run(l, w, r.clone());
    unchanged(l, w, r);
}
#[test]
fn unknown_pua_is_not_a_dash() {
    let (l, mut w, r) = fixture();
    w.last_mut().unwrap().raw = "\u{E61D}".into();
    unchanged(l, w, r);
}
#[test]
fn already_used_is_not_appended() {
    let (l, mut w, r) = fixture();
    w.last_mut().unwrap().status = WordStatus::Used;
    unchanged(l, w, r);
}
#[test]
fn duplicate_row_owner_is_ambiguous() {
    let (mut l, w, r) = fixture();
    l.push(l[1].clone());
    unchanged(l, w, r);
}
#[test]
fn duplicate_native_dash_is_ambiguous() {
    let (l, mut w, r) = fixture();
    let mut d = w.last().unwrap().clone();
    d.id = w.len();
    w.push(d);
    unchanged(l, w, r);
}
#[test]
fn missing_rails_rejects() {
    let (l, w, _) = fixture();
    unchanged(l, w, vec![]);
}
#[test]
fn narrow_neighbour_frame_rejects() {
    let (l, w, mut r) = fixture();
    r.push((200., 0., 70.));
    unchanged(l, w, r);
}
#[test]
fn one_anchor_is_not_enough() {
    let (mut l, w, r) = fixture();
    l.remove(0);
    unchanged(l, w, r);
}
#[test]
fn mismatched_right_edge_rejects() {
    let (l, mut w, r) = fixture();
    for i in [1, 4] {
        w[i].x1 -= 6.;
    }
    unchanged(l, w, r);
}
#[test]
fn plain_prose_is_not_a_label() {
    let (mut l, mut w, r) = fixture();
    l[1].text = "費用。".into();
    w[2].text = "費用。".into();
    unchanged(l, w, r);
}
#[test]
fn different_y_label_is_not_an_owner() {
    let (l, mut w, r) = fixture();
    w[2].y0 += 4.;
    w[2].y1 += 4.;
    unchanged(l, w, r);
}
#[test]
fn another_word_on_row_prevents_append() {
    let (mut l, mut w, r) = fixture();
    let id = word(&mut w, 355., 30., "円");
    l[1].words.push(id);
    unchanged(l, w, r);
}
#[test]
fn no_units_or_multi_amount_anchor_rejects() {
    let (l, mut w, r) = fixture();
    for i in [1, 4] {
        w[i].text = "100".into();
    }
    unchanged(l, w, r);
}
#[test]
fn tall_dash_rejects() {
    let (l, mut w, r) = fixture();
    w.last_mut().unwrap().y1 += 15.;
    unchanged(l, w, r);
}
fn table_fixture() -> (Table, Vec<Word>, Vec<Grid>) {
    let mut w = vec![];
    let a = word(&mut w, 24., 10., "流動資産");
    let b = word(&mut w, 24., 18., "固定資産");
    let c = word(&mut w, 204., 10., "100");
    w[c].x1 = 224.;
    let e = word(&mut w, 204., 31., "200");
    w[e].x1 = 224.;
    let d = word(&mut w, 216., 18., "\u{E61C}");
    w[d].x1 = 224.;
    w[d].text.clear();
    w[d].status = WordStatus::Empty;
    let cell = |row, col, x0, x1, y0, y1, id, text: &str| TableCell {
        row,
        col,
        rowspan: 1,
        colspan: 1,
        x0,
        x1,
        y0,
        y1,
        words: vec![id],
        text: text.into(),
    };
    let t = Table {
        x0: 0.,
        y0: 0.,
        x1: 240.,
        y1: 40.,
        cols: 3,
        rows: 4,
        cells: vec![
            cell(1, 1, 20., 160., 10., 30., a, "流動資産"),
            cell(2, 1, 20., 160., 10., 30., b, "固定資産"),
            cell(1, 2, 160., 240., 10., 30., c, "100"),
            cell(3, 2, 160., 240., 30., 40., e, "200"),
        ],
    };
    let g = Grid {
        xs: vec![0., 20., 160., 240.],
        ys: vec![0., 10., 30., 40.],
        cells: (0..3)
            .flat_map(|y| {
                (0..3).map(move |x| crate::grid::GridCell {
                    x0: x,
                    x1: x + 1,
                    y0: y,
                    y1: y + 1,
                })
            })
            .collect(),
    };
    (t, w, vec![g])
}
fn table_unchanged(mut t: Table, mut w: Vec<Word>, g: Vec<Grid>) {
    let old = (t.clone(), format!("{w:?}"));
    table(&mut t, &mut w, &g);
    assert_eq!(old, (t, format!("{w:?}")));
}
#[test]
fn split_physical_cell_has_a_unique_label_row() {
    let (mut t, mut w, g) = table_fixture();
    table(&mut t, &mut w, &g);
    assert!(t
        .cells
        .iter()
        .any(|c| c.row == 2 && c.col == 2 && c.text == "－"));
    table_unchanged(t, w, g);
}
#[test]
fn absent_physical_grid_is_not_inferred() {
    let (t, w, _) = table_fixture();
    table_unchanged(t, w, vec![]);
}
#[test]
fn overlapping_physical_cells_are_ambiguous() {
    let (t, w, mut g) = table_fixture();
    let duplicate = g[0].cells[5];
    g[0].cells.push(duplicate);
    table_unchanged(t, w, g);
}
#[test]
fn another_value_on_same_logical_row_is_preserved() {
    let (mut t, w, g) = table_fixture();
    let mut c = t.cells[2].clone();
    c.row = 2;
    t.cells.push(c);
    table_unchanged(t, w, g);
}
#[test]
fn merged_target_physical_cell_is_rejected() {
    let (t, w, mut g) = table_fixture();
    g[0].cells[5].y1 = 3;
    table_unchanged(t, w, g);
}
#[test]
fn second_logical_label_is_required() {
    let (mut t, w, g) = table_fixture();
    t.cells.remove(0);
    table_unchanged(t, w, g);
}
#[test]
fn a_second_grid_is_ambiguous() {
    let (t, w, mut g) = table_fixture();
    g.push(g[0].clone());
    table_unchanged(t, w, g);
}
#[test]
fn rail_overshoot_over_one_and_quarter_pt_is_rejected() {
    let (l, w, mut r) = fixture();
    for v in &mut r {
        v.2 = 36.;
    }
    unchanged(l, w, r);
}
#[test]
fn duplicate_owner_in_another_unit_is_ambiguous() {
    let (l, mut w, r) = fixture();
    let mut units = vec![Unit::Plain(l.clone()), Unit::Plain(vec![l[1].clone()])];
    let all = |us: &[Unit]| us.iter().filter_map(|u|if let Unit::Plain(ls)=u{Some(ls)}else{None}).flatten().cloned().collect::<Vec<_>>();
    let old = all(&units);
    restore_owned_dashes(&mut units, &mut w, &[], &r);
    assert_eq!(old, all(&units));
}
#[test]
fn numeric_runs_reset_their_right_edge_between_baselines() {
 let mut w=vec![];let a=word(&mut w,100.,0.,"100");w[a].x1=200.;let b=word(&mut w,120.,20.,"200");w[b].x1=132.;let ns=runs(&[a,b],&w,8.);assert_eq!(ns.len(),2);assert_eq!(ns[0].edge,200.);assert_eq!(ns[1].edge,132.);
}
