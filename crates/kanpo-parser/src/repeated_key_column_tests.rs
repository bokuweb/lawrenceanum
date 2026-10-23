use super::*;
fn word(id: usize, x: f32, y: f32, width: f32, text: &str) -> Col {
    Col {
        id,
        x,
        x1: x + width,
        y,
        y1: y + 8.0,
        text: text.into(),
    }
}
fn sample(n: usize) -> Vec<Col> {
    let mut words = vec![
        word(100, 0., 0., 40., "登録番号"),
        word(101, 72., 0., 48., "製品及び型式"),
        word(102, 176., 0., 40., "形状分類"),
        word(103, 260., 0., 64., "申請者の名称住所"),
    ];
    for i in 0..n {
        let y = 12. + 20. * i as f32;
        words.push(word(i * 10, 0., y, 40., &format!("第{}号", 100 + i)));
        words.push(word(
            i * 10 + 1,
            64.,
            y,
            if i == 0 { 72. } else { 8. },
            if i == 0 { "最初の製品型式" } else { "〃" },
        ));
        words.push(word(
            i * 10 + 2,
            160.,
            y,
            if i == 0 { 40. } else { 8. },
            if i == 0 { "ボンネット" } else { "〃" },
        ));
        words.push(word(
            i * 10 + 3,
            256.,
            y,
            if i == 0 { 80. } else { 8. },
            if i == 0 { "登録製造株式会社" } else { "〃" },
        ));
    }
    words
}
#[test]
fn headed_repeated_column_crosses_large_gaps() {
    let (t, rest) = keyed_tables(sample(4), &[]);
    assert_eq!(t.len(), 1);
    assert_eq!((t[0].rows, t[0].cols), (5, 4));
    assert!(rest.is_empty());
    let cells = &t[0].cells;
    assert_eq!(cells.iter().find(|c| c.row == 1 && c.col == 3).unwrap().text, "登録製造株式会社");
    let ids: Vec<_> = cells.iter().flat_map(|c| c.words.iter()).collect();
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(ids.len(), unique.len());
}
#[test]
fn no_headers_keeps_the_large_gap_boundary() {
    let mut w = sample(4);
    w.retain(|w| w.y > 0.);
    let (_, rest) = keyed_tables(w, &[]);
    assert!(rest.iter().any(|w| w.id == 3));
}
#[test]
fn actual_values_in_later_rows_do_not_prove_a_repeated_column() {
    let mut w = sample(4);
    w.iter_mut().find(|w| w.id == 23).unwrap().text = "別会社".into();
    let (_, rest) = keyed_tables(w, &[]);
    assert!(rest.iter().any(|w| w.id == 3));
}
#[test]
fn a_real_rule_still_stops_the_table() {
    let (_, rest) = keyed_tables(sample(4), &[(220., 0., 120.)]);
    assert!(rest.iter().any(|w| w.id == 3));
}
#[test]
fn mismatched_font_size_does_not_prove_a_repeated_column() {
    let mut w = sample(4);
    w.iter_mut().find(|w| w.id == 23).unwrap().y1 -= 3.;
    let (_, rest) = keyed_tables(w, &[]);
    assert!(rest.iter().any(|w| w.id == 3));
}
#[test]
fn a_missing_row_mark_does_not_prove_a_repeated_column() {
    let mut w = sample(4);
    w.retain(|w| w.id != 23);
    let (_, rest) = keyed_tables(w, &[]);
    assert!(rest.iter().any(|w| w.id == 3));
}
#[test]
fn a_distant_column_beyond_twelve_em_is_not_pulled_in() {
    let mut w = sample(4);
    for c in &mut w {
        if c.x >= 256. {
            c.x += 40.;
            c.x1 += 40.;
        }
    }
    let (_, rest) = keyed_tables(w, &[]);
    assert!(rest.iter().any(|w| w.id == 3));
}
#[test]
fn two_record_geometric_tables_keep_existing_boundaries() {
    let (t, rest) = keyed_tables(sample(2), &[]);
    assert!(t.is_empty());
    assert_eq!(rest.len(), 12);
}
#[test]
fn fewer_than_three_independent_headers_do_not_expand_the_table() {
    let mut w = sample(4);
    w.retain(|w| w.id < 100 || w.id == 103);
    let (_, rest) = keyed_tables(w, &[]);
    assert!(rest.iter().any(|w| w.id == 3));
}
#[test]
fn a_header_far_above_the_first_value_does_not_prove_a_column() {
    let mut w = sample(4);
    let h = w.iter_mut().find(|w| w.id == 103).unwrap();
    h.y -= 24.;
    h.y1 -= 24.;
    let (_, rest) = keyed_tables(w, &[]);
    assert!(rest.iter().any(|w| w.id == 3));
}
#[test]
fn a_sentence_as_initial_value_does_not_prove_a_table_column() {
    let mut w = sample(4);
    w.iter_mut().find(|w| w.id == 3).unwrap().text = "隣の記事。".into();
    let (_, rest) = keyed_tables(w, &[]);
    assert!(rest.iter().any(|w| w.id == 3));
}
