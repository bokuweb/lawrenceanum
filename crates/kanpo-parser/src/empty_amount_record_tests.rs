use super::*;
fn line(id: usize, x: f32, y: f32, width: f32, text: &str) -> Line {
    Line {
        y0: y,
        y1: y + 8.,
        words: vec![Col {
            id,
            x,
            x1: x + width,
            y,
            y1: y + 8.,
            text: text.into(),
        }],
    }
}
fn fixture() -> (Grid, Vec<Vec<Line>>) {
    let g = Grid {
        xs: vec![0., 100., 160., 220., 280.],
        ys: vec![0., 10., 70., 130.],
        cells: (0..3)
            .flat_map(|y| {
                (0..4).map(move |x| GridCell {
                    x0: x,
                    x1: x + 1,
                    y0: y,
                    y1: y + 1,
                })
            })
            .collect(),
    };
    let mut c = vec![vec![]; 12];
    c[4] = vec![
        line(0, 8., 13., 80., "Ⅰ当期変動額"),
        line(1, 24., 28., 48., "出資金の受入"),
        line(2, 24., 44., 72., "不要財産に係る国庫"),
        line(3, 24., 52., 64., "納付等による減資"),
    ];
    for r in 0..3 {
        let y = 74. + r as f32 * 16.;
        c[8].push(line(10 + r, 24., y, 64., "資産取引項目"));
        c[9].push(line(20 + r, 130., y, 16., "12"));
        c[10].push(line(30 + r, 190., y, 16., "34"));
    }
    (g, c)
}
#[test]
fn independent_short_item_then_wrapped_item_starts_new_row() {
    let (g, c) = fixture();
    assert_eq!(empty_amount_record_anchors(&g, &c), vec![43.5]);
}
#[test]
fn actual_wrap_near_right_edge_remains_one_item() {
    let (g, mut c) = fixture();
    c[4][1].words[0].x1 = 98.;
    assert!(empty_amount_record_anchors(&g, &c).is_empty());
}
#[test]
fn tightly_spaced_lines_remain_wrapped() {
    let (g, mut c) = fixture();
    c[4][2].y0 = 36.;
    c[4][2].y1 = 44.;
    assert!(empty_amount_record_anchors(&g, &c).is_empty());
}
#[test]
fn changed_indent_cannot_identify_two_equal_items() {
    let (g, mut c) = fixture();
    c[4][2].words[0].x = 35.;
    assert!(empty_amount_record_anchors(&g, &c).is_empty());
}
#[test]
fn amount_in_target_band_keeps_existing_alignment() {
    let (g, mut c) = fixture();
    c[5].push(line(100, 130., 28., 16., "12"));
    assert!(empty_amount_record_anchors(&g, &c).is_empty());
}
#[test]
fn no_numeric_body_has_no_financial_evidence() {
    let (g, mut c) = fixture();
    c[9].clear();
    c[10].clear();
    assert!(empty_amount_record_anchors(&g, &c).is_empty());
}
#[test]
fn one_numeric_column_is_insufficient() {
    let (g, mut c) = fixture();
    c[10].clear();
    assert!(empty_amount_record_anchors(&g, &c).is_empty());
}
#[test]
fn two_aligned_numeric_rows_insufficient() {
    let (g, mut c) = fixture();
    c[9].pop();
    c[10].pop();
    assert!(empty_amount_record_anchors(&g, &c).is_empty());
}
#[test]
fn no_section_heading_is_not_an_item_block() {
    let (g, mut c) = fixture();
    c[4][0].words[0].text = "通常の説明".into();
    assert!(empty_amount_record_anchors(&g, &c).is_empty());
}
#[test]
fn paragraph_punctuation_disqualifies_empty_band() {
    let (g, mut c) = fixture();
    c[4][1].words[0].text = "出資金の説明。".into();
    assert!(empty_amount_record_anchors(&g, &c).is_empty());
}
#[test]
fn heading_and_items_without_indent_not_separate() {
    let (g, mut c) = fixture();
    c[4][0].words[0].x = 24.;
    assert!(empty_amount_record_anchors(&g, &c).is_empty());
}
#[test]
fn physical_merged_label_column_is_not_an_empty_record_band() {
    let (mut g, c) = fixture();
    g.cells[4].x1 = 2;
    assert!(empty_amount_record_anchors(&g, &c).is_empty());
}

#[test]
fn numbered_subsections_do_not_flatten_a_hierarchy_into_peer_items() {
    let (g, mut cells) = fixture();
    cells[4][1].words[0].text = "（１）利益処分".into();
    assert!(empty_amount_record_anchors(&g, &cells).is_empty());
}
