use super::continues_numbered_numeric_table;
use crate::vertical::Col;
fn band(numbers: &[&str], y: f32, x_offset: f32) -> Vec<Col> {
    let mut cols = Vec::new();
    for (i, n) in numbers.iter().enumerate() {
        let x = 500.0 - i as f32 * 20.0 + x_offset;
        for (dy, text) in [(0.0, *n), (60.0, "1.2"), (120.0, "1.1"), (180.0, "1.2")] {
            cols.push(Col {
                id: cols.len(),
                text: text.into(),
                x,
                x1: x + 8.0,
                y: y + dy,
                y1: y + dy + 8.0,
            });
        }
    }
    cols
}
#[test]
fn ordinary_numeric_rows_continue_between_tiers() {
    assert!(continues_numbered_numeric_table(
        &band(&["７", "８", "９"], 80.0, 0.0),
        &band(&["10", "11", "12"], 450.0, 0.3)
    ));
    let a = band(&["7", "8", "9"], 80.0, 0.0);
    let b = band(&["10", "11", "12", "令和元", "２", "３"], 450.0, 0.0);
    assert!(
        continues_numbered_numeric_table(&a, &b),
        "later nonnumeric year labels do not erase the continuing integer prefix"
    );
}
#[test]
fn similar_shinkyu_and_other_rows_do_not_establish_number_continuity() {
    let a = band(&["7", "8", "9"], 80.0, 0.0);
    for b in [
        band(&["7", "8", "9"], 450.0, 0.0),
        band(&["10", "11"], 450.0, 0.0),
        band(&["10", "12", "13"], 450.0, 0.0),
        band(&["11", "12", "13"], 450.0, 0.0),
        band(&["10", "11", "12"], 450.0, 15.0),
        band(&["令和元", "10", "11", "12"], 450.0, 0.0),
    ] {
        assert!(!continues_numbered_numeric_table(&a, &b));
    }
    for a in [
        band(&["7", "9", "10"], 80.0, 0.0),
        band(&["7", "8", "9.0"], 80.0, 0.0),
        band(&["7", "8", "9", "備考"], 80.0, 0.0),
    ] {
        assert!(!continues_numbered_numeric_table(&a, &band(&["11", "12", "13"], 450.0, 0.0)));
    }
}
#[test]
fn prose_marks_and_misaligned_cells_keep_existing_classification() {
    let a = band(&["7", "8", "9"], 80.0, 0.0);
    let b = band(&["10", "11", "12"], 450.0, 0.0);
    for text in ["（略）", "（新設）", "（削る）", "電気設備の説明"] {
        let mut changed = a.clone();
        changed[1].text = text.into();
        assert!(!continues_numbered_numeric_table(&changed, &b));
    }
    let mut changed = a.clone();
    changed[4].y += 10.0;
    changed[4].y1 += 10.0;
    assert!(!continues_numbered_numeric_table(&changed, &b));
    let a: Vec<_> = a.into_iter().filter(|c| c.id % 4 < 2).collect();
    assert!(!continues_numbered_numeric_table(&a, &b));
}
