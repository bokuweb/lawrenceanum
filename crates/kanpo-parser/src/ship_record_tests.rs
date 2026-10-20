use super::*;
fn fixture() -> Vec<Col> {
    let data = [
        (20., 48., 12., 20., "名称"),
        (64., 92., 12., 20., "所在地"),
        (108., 148., 12., 20., "有効期間"),
        (20., 48., 32., 40., "甲工場"),
        (64., 92., 32., 40., "乙市１号"),
        (108., 148., 32., 40., "令和八年"),
        (20., 70., 41., 49., "〇関係省告示第二号"),
        (20., 90., 60., 68., "次の記事の本文"),
        (20., 90., 80., 88., "本文の続き"),
        (100., 170., 60., 68., "右の継続表の内容"),
        (100., 170., 80., 88., "継続表の内容"),
    ];
    data.into_iter()
        .enumerate()
        .map(|(id, (x, x1, y, y1, text))| Col {
            id,
            x,
            x1,
            y,
            y1,
            text: text.into(),
        })
        .collect()
}
#[test]
fn complete_record_over_the_start_of_a_real_divider() {
    let w = fixture();
    let t = record_above_divider(&w, 100., &[(100., 45., 200.)], &[]).unwrap();
    assert_eq!((t.rows, t.cols), (2, 3));
    let ids: Vec<_> = t.cells.iter().flat_map(|c| c.words.iter().copied()).collect();
    assert_eq!(ids.len(), 6);
    assert!((0..6).all(|id| ids.contains(&id)));
}
#[test]
fn separate_fenced_articles_are_not_one_record() {
    assert!(record_above_divider(&fixture(), 100., &[(100., 45., 200.), (100., 10., 40.)], &[]).is_none());
    assert!(record_above_divider(&fixture(), 100., &[(100., 45., 200.)], &[(25., 20., 170.)]).is_none());
}
#[test]
fn short_or_missing_or_distant_dividers_do_not_promote_records() {
    for rules in [vec![], vec![(100., 45., 90.)], vec![(104., 45., 200.)], vec![(100., 70., 200.)]] {
        assert!(record_above_divider(&fixture(), 100., &rules, &[]).is_none());
    }
}
#[test]
fn crossing_body_and_offset_heading_are_not_a_transition() {
    let mut w = fixture();
    w[6].text = "通常の本文".into();
    assert!(record_above_divider(&w, 100., &[(100., 45., 200.)], &[]).is_none());
    let mut w = fixture();
    w[6].y += 1.;
    w[6].y1 += 1.;
    assert!(record_above_divider(&w, 100., &[(100., 45., 200.)], &[]).is_none());
}
#[test]
fn incomplete_second_column_and_repeated_source_ids_are_rejected() {
    let mut w = fixture();
    w.retain(|w| w.id < 9);
    assert!(record_above_divider(&w, 100., &[(100., 45., 200.)], &[]).is_none());
    let mut w = fixture();
    w[1].id = w[0].id;
    assert!(record_above_divider(&w, 100., &[(100., 45., 200.)], &[]).is_none());
}

#[test]
fn source_id_shared_with_the_remaining_region_is_not_removed() {
    let mut w = fixture();
    w[10].id = w[0].id;
    assert!(record_above_divider(&w, 100., &[(100., 45., 200.)], &[]).is_none());
}
