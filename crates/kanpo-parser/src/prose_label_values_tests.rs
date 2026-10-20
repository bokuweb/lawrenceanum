use super::*;
fn c(id: usize, x: f32, y: f32, y1: f32, t: &str) -> Col {
    Col {
        id,
        x,
        x1: x + 8.,
        y,
        y1,
        text: t.into(),
    }
}
fn fixture() -> (Vec<Vec<Col>>, TierRules) {
    let mut a = vec![];
    let mut b = vec![];
    for (k, n) in ["（四）", "（五）", "（六）"].iter().enumerate() {
        let x = 100. - k as f32 * 22.;
        a.push(c(k * 2, x, 50., 58., n));
        a.push(c(k * 2 + 1, x, 66., 146., "制限の対象となる場所"));
        b.push(c(
            20 + k,
            x,
            154.,
            314.,
            "新たに地上に設ける電柱を対象とする。",
        ));
    }
    a.push(c(40, 30., 100., 200., "公告"));
    b.push(c(41, 20., 154., 250., "別記事の文章は次段に保存する。"));
    (
        vec![a, b],
        TierRules {
            tables: vec![],
            boxes: vec![],
            hrules: vec![],
            vrules: vec![],
            sideways: true,
        },
    )
}
fn ids(t: &[Vec<Col>]) -> Vec<usize> {
    let mut x: Vec<_> = t.iter().flatten().map(|c| c.id).collect();
    x.sort();
    x
}
#[test]
fn aligned_labels_move_only_corresponding_values() {
    let (mut t, r) = fixture();
    let old = ids(&t);
    rejoin_prose_label_values(&mut t, &r);
    assert_eq!(t[0].len(), 10);
    assert_eq!(t[1].iter().map(|c| c.id).collect::<Vec<_>>(), vec![41]);
    assert_eq!(ids(&t), old);
    let prior = t.clone();
    rejoin_prose_label_values(&mut t, &r);
    assert_eq!(ids(&t), ids(&prior));
    assert_eq!(t[0].len(), prior[0].len());
}
fn unchanged(mut t: Vec<Vec<Col>>, r: TierRules) {
    let old: Vec<_> = t
        .iter()
        .map(|t| t.iter().map(|c| c.id).collect::<Vec<_>>())
        .collect();
    rejoin_prose_label_values(&mut t, &r);
    assert_eq!(
        t.iter()
            .map(|t| t.iter().map(|c| c.id).collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        old
    );
}
#[test]
fn ordinary_two_tiers_stay_separate() {
    let (mut t, r) = fixture();
    t[0].retain(|c| c.id % 2 == 1 || c.id == 40);
    unchanged(t, r);
}
#[test]
fn upright_page_stays_separate() {
    let (t, mut r) = fixture();
    r.sideways = false;
    unchanged(t, r);
}
#[test]
fn missing_label_stays_separate() {
    let (mut t, r) = fixture();
    t[0].retain(|c| c.id != 2 && c.id != 3);
    unchanged(t, r);
}
#[test]
fn nonconsecutive_numbers_stay_separate() {
    let (mut t, r) = fixture();
    t[0][2].text = "（七）".into();
    unchanged(t, r);
}
#[test]
fn missing_aligned_value_stays_separate() {
    let (mut t, r) = fixture();
    t[1].retain(|c| c.id != 21);
    unchanged(t, r);
}
#[test]
fn unequal_label_bottom_stays_separate() {
    let (mut t, r) = fixture();
    t[0][3].y1 -= 3.;
    unchanged(t, r);
}
#[test]
fn unequal_value_top_stays_separate() {
    let (mut t, r) = fixture();
    t[1][1].y += 3.;
    unchanged(t, r);
}
#[test]
fn large_gap_stays_separate() {
    let (mut t, r) = fixture();
    for c in &mut t[1] {
        c.y += 20.;
        c.y1 += 20.;
    }
    unchanged(t, r);
}
#[test]
fn pure_numbers_stay_separate() {
    let (mut t, r) = fixture();
    for c in &mut t[1] {
        c.text = "一二三四五六七八九〇一二三四五六".into();
    }
    unchanged(t, r);
}
#[test]
fn next_article_heading_stays_separate() {
    let (mut t, r) = fixture();
    t[1][1].text = "公告".into();
    unchanged(t, r);
}
#[test]
fn notice_marker_stays_separate() {
    let (mut t, r) = fixture();
    t[1][1].text = "〇別記事の冒頭である。".into();
    unchanged(t, r);
}
#[test]
fn crossing_horizontal_rule_stays_separate() {
    let (t, mut r) = fixture();
    r.hrules.push((150., 50., 110.));
    unchanged(t, r);
}
#[test]
fn crossing_vertical_rule_stays_separate() {
    let (t, mut r) = fixture();
    r.vrules.push((90., 140., 160.));
    unchanged(t, r);
}
#[test]
fn crossing_frame_stays_separate() {
    let (t, mut r) = fixture();
    r.boxes.push((50., 110., 140., 160.));
    unchanged(t, r);
}
#[test]
fn overlapping_value_columns_stay_separate() {
    let (mut t, r) = fixture();
    t[1].push(c(30, 99., 154., 314., "もう一つの文章が同じ列に重なる。"));
    unchanged(t, r);
}

#[test]
fn another_section_heading_stays_separate() {
    let (mut t, r) = fixture();
    t[1][1].text = "官庁報告".into();
    unchanged(t, r);
}
#[test]
fn crossing_table_stays_separate() {
    let (t, mut r) = fixture();
    r.tables.push((50., 110., 140., 160.));
    unchanged(t, r);
}
#[test]
fn non_japanese_label_stays_separate() {
    let (mut t, r) = fixture();
    t[0][3].text = "別件の説明です。".into();
    unchanged(t, r);
}

#[test]
fn subpoint_label_center_rounding_keeps_reading_order() {
    let (mut t, r) = fixture();
    t[0][1].x += 0.01;
    t[0][1].x1 += 0.01;
    rejoin_prose_label_values(&mut t, &r);
    assert_eq!(t[0].len(), 10);
    assert_eq!(t[1].len(), 1);
}
