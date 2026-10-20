use super::*;

fn column(id: usize, x: f32, y: f32, y1: f32, text: &str) -> Col {
    Col {
        id,
        x,
        x1: x + 8.,
        y,
        y1,
        text: text.into(),
    }
}
fn fixture() -> Vec<Col> {
    vec![
        column(0, 200., 10., 42., "（新設）"),
        column(1, 100., 10., 66., "第三十七条の三"),
        column(2, 80., 10., 42., "（新設）"),
        column(3, 50., 10., 42., "（新設）"),
        column(4, 100., 74., 98., "（略）"),
    ]
}
fn together(columns: Vec<Col>, hrules: Vec<(f32, f32, f32)>) -> bool {
    split_tiers(
        columns,
        &TierRules {
            tables: vec![],
            boxes: vec![],
            hrules,
            vrules: vec![],
            sideways: false,
        },
    )
    .iter()
    .any(|tier| tier.iter().any(|c| c.id == 1) && tier.iter().any(|c| c.id == 4))
}
#[test]
fn an_overlapping_cut_keeps_a_same_column_shorthand() {
    assert!(together(fixture(), vec![]));
}
#[test]
fn a_distant_shorthand_is_a_separate_tier() {
    let mut columns = fixture();
    columns[4].y += 20.;
    columns[4].y1 += 20.;
    assert!(!together(columns, vec![]));
}
#[test]
fn another_column_is_not_a_continuation() {
    let mut columns = fixture();
    columns[4].x -= 10.;
    columns[4].x1 -= 10.;
    assert!(!together(columns, vec![]));
}
#[test]
fn a_crossing_cell_rule_is_a_fence() {
    assert!(!together(fixture(), vec![(70., 45., 210.)]));
}
