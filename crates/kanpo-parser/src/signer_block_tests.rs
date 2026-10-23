use super::*;
use std::collections::HashMap;
fn fixture() -> (Vec<trace::Line>, HashMap<usize, (f32, f32, f32, f32)>) {
    let rows = [
        ("令和８年10月16日", 85., 157., 352.8211),
        ("貸借対照表", 341., 509., 383.8597),
        ("（令和８年３月31日現在）", 377., 473., 394.2197),
        ("岩手県盛岡市上田三丁目18番８号", 645., 765., 352.8211),
        ("国立大学法人岩手大学", 685., 765., 363.1411),
        ("学長　山本　欣郎", 701., 765., 373.5011),
    ];
    (
        rows.iter().enumerate().map(|(i, r)| trace::Line::text(r.0.into(), vec![i])).collect(),
        rows.iter().enumerate().map(|(i, r)| (i, (r.1, r.2, r.3, r.3 + 8.184))).collect(),
    )
}
#[test]
fn long_aligned_address_corporation_and_signer_return_after_date() {
    let (mut ls, g) = fixture();
    let orig = ls.clone();
    raise_signer_blocks(&mut ls, &g);
    assert_eq!(ls.iter().map(|l| l.words[0]).collect::<Vec<_>>(), [0, 3, 4, 5, 1, 2]);
    let first = ls.clone();
    raise_signer_blocks(&mut ls, &g);
    assert_eq!(ls, first);
    let mut actual = ls.iter().flat_map(|l| &l.words).copied().collect::<Vec<_>>();
    actual.sort();
    assert_eq!(actual, orig.iter().flat_map(|l| &l.words).copied().collect::<Vec<_>>());
}
#[test]
fn a_separate_name_line_stays_with_the_signer() {
    let (mut ls, mut g) = fixture();
    ls[5].text = "学長".into();
    ls.push(trace::Line::text("山本　欣郎".into(), vec![6]));
    g.insert(6, (725., 765., 383.8211, 392.0051));
    raise_signer_blocks(&mut ls, &g);
    assert_eq!(ls.iter().map(|l| l.words[0]).collect::<Vec<_>>(), [0, 3, 4, 5, 6, 1, 2]);
}
#[test]
fn no_corporation_line_does_not_absorb_previous_text() {
    let (mut ls, g) = fixture();
    ls[4].text = "岩手大学の決算科目".into();
    let orig = ls.clone();
    raise_signer_blocks(&mut ls, &g);
    assert_eq!(ls, orig);
}
#[test]
fn no_address_number_does_not_trigger_the_wider_exception() {
    let (mut ls, g) = fixture();
    ls[3].text = "岩手県盛岡市の受託研究費".into();
    let orig = ls.clone();
    raise_signer_blocks(&mut ls, &g);
    assert_eq!(ls, orig);
}
#[test]
fn prose_in_an_address_shaped_line_is_not_a_signature() {
    let (mut ls, g) = fixture();
    ls[3].text = "岩手県盛岡市１番の資産。".into();
    let orig = ls.clone();
    raise_signer_blocks(&mut ls, &g);
    assert_eq!(ls, orig);
}
#[test]
fn a_neighbouring_article_address_has_a_different_right_edge() {
    let (mut ls, mut g) = fixture();
    g.insert(3, (625., 745., 352.8211, 361.0051));
    let orig = ls.clone();
    raise_signer_blocks(&mut ls, &g);
    assert_eq!(ls, orig);
}
#[test]
fn a_neighbouring_corporation_has_a_different_right_edge() {
    let (mut ls, mut g) = fixture();
    g.insert(4, (665., 745., 363.1411, 371.3251));
    let orig = ls.clone();
    raise_signer_blocks(&mut ls, &g);
    assert_eq!(ls, orig);
}
#[test]
fn a_date_on_another_baseline_does_not_capture_the_block() {
    let (mut ls, mut g) = fixture();
    g.insert(0, (85., 157., 342.5011, 350.6851));
    let orig = ls.clone();
    raise_signer_blocks(&mut ls, &g);
    assert_eq!(ls, orig);
}
#[test]
fn no_date_does_not_move_anyone() {
    let (mut ls, g) = fixture();
    ls[0].text = "貸借対照表の続き".into();
    let orig = ls.clone();
    raise_signer_blocks(&mut ls, &g);
    assert_eq!(ls, orig);
}
#[test]
fn a_table_cell_with_a_role_word_does_not_become_a_signer() {
    let (mut ls, g) = fixture();
    ls[5].table = Some(trace::Table {
        x0: 600.,
        y0: 373.,
        x1: 765.,
        y1: 382.,
        rows: 1,
        cols: 1,
        cells: vec![],
    });
    let orig = ls.clone();
    raise_signer_blocks(&mut ls, &g);
    assert_eq!(ls, orig);
}
#[test]
fn a_subpoint_baseline_difference_does_not_drop_a_same_height_date() {
    let (mut ls, mut g) = fixture();
    g.insert(3, (660., 765., 352.8211, 361.0051));
    g.insert(0, (85., 157., 352.8261, 361.0101));
    raise_signer_blocks(&mut ls, &g);
    assert_eq!(ls.iter().map(|l| l.words[0]).collect::<Vec<_>>(), [0, 3, 4, 5, 1, 2]);
}
#[test]
fn the_date_tolerance_does_not_accept_a_lower_row() {
    let (mut ls, mut g) = fixture();
    g.insert(3, (660., 765., 352.8211, 361.0051));
    g.insert(0, (85., 157., 352.8411, 361.0251));
    let orig = ls.clone();
    raise_signer_blocks(&mut ls, &g);
    assert_eq!(ls, orig);
}
