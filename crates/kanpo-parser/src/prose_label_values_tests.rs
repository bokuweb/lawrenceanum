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

// A change of page units alone must not change which source columns move.
// Other fixed-distance guards are deliberately not generalized in these tests.
fn size_contract_expected(mut t: Vec<Vec<Col>>, r: TierRules) {
    let before = t.clone();
    let mut expected = vec![before[0].clone(), Vec::new()];
    for col in &before[1] {
        if [20, 21, 22].contains(&col.id) {
            expected[0].push(col.clone());
        } else {
            expected[1].push(col.clone());
        }
    }
    rejoin_prose_label_values(&mut t, &r);
    let signature = |tiers: &[Vec<Col>]| {
        tiers
            .iter()
            .map(|tier| {
                tier.iter()
                    .map(|c| {
                        (
                            c.id,
                            c.x.to_bits(),
                            c.x1.to_bits(),
                            c.y.to_bits(),
                            c.y1.to_bits(),
                            c.text.clone(),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(signature(&t), signature(&expected));
}
fn scaled_size_contract(scale: f32, close_heading_gap: bool) -> (Vec<Vec<Col>>, TierRules) {
    let (mut t, r) = fixture();
    for c in t.iter_mut().flatten() {
        if close_heading_gap && [1, 3, 5].contains(&c.id) {
            c.y = 58.;
        }
        c.x *= scale;
        c.x1 *= scale;
        c.y *= scale;
        c.y1 *= scale;
    }
    (t, r)
}
#[test]
fn half_scale_keeps_label_value_source_membership() {
    let (t, r) = scaled_size_contract(0.5, false);
    size_contract_expected(t, r);
}
#[test]
fn double_scale_keeps_label_value_source_membership() {
    let (t, r) = scaled_size_contract(2., false);
    size_contract_expected(t, r);
}
#[test]
fn eighth_scale_positive_columns_keep_label_value_source_membership() {
    let (t, r) = scaled_size_contract(0.125, true);
    size_contract_expected(t, r);
}
#[test]
fn eightfold_scale_positive_columns_keep_label_value_source_membership() {
    let (t, r) = scaled_size_contract(8., true);
    size_contract_expected(t, r);
}
#[test]
fn finite_negative_page_coordinates_keep_label_value_source_membership() {
    let (mut t, r) = fixture();
    for c in t.iter_mut().flatten() {
        c.x -= 1024.;
        c.x1 -= 1024.;
        c.y -= 1024.;
        c.y1 -= 1024.;
    }
    size_contract_expected(t, r);
}
// Unknown/degenerate source geometry cannot authorize a transfer. Keep the
// entire source inventory byte-for-byte even when NaN makes PartialEq unusable.
fn size_contract_abstains(mut t: Vec<Vec<Col>>, r: TierRules) {
    let signature = |tiers: &[Vec<Col>]| {
        tiers
            .iter()
            .map(|tier| {
                tier.iter()
                    .map(|c| {
                        (
                            c.id,
                            c.x.to_bits(),
                            c.x1.to_bits(),
                            c.y.to_bits(),
                            c.y1.to_bits(),
                            c.text.clone(),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    };
    let before = signature(&t);
    rejoin_prose_label_values(&mut t, &r);
    assert_eq!(signature(&t), before);
}
#[test]
fn nonfinite_source_rectangle_cannot_authorize_label_value_join() {
    for (tier, index) in [(0, 0), (0, 1), (0, 6), (1, 0), (1, 3)] {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for coordinate in 0..4 {
                let (mut t, r) = fixture();
                let c = &mut t[tier][index];
                match coordinate {
                    0 => c.x = value,
                    1 => c.x1 = value,
                    2 => c.y = value,
                    _ => c.y1 = value,
                }
                size_contract_abstains(t, r);
            }
        }
    }
}
#[test]
fn nonpositive_source_rectangle_cannot_authorize_label_value_join() {
    for (tier, index) in [(0, 0), (0, 1), (0, 6), (1, 0), (1, 3)] {
        for inverted in [false, true] {
            for along in [false, true] {
                let (mut t, r) = fixture();
                let c = &mut t[tier][index];
                let delta = if inverted { 1. } else { 0. };
                if along {
                    c.y1 = c.y - delta;
                } else {
                    c.x1 = c.x - delta;
                }
                size_contract_abstains(t, r);
            }
        }
    }
}
#[test]
fn repeated_id_within_heading_tier_cannot_authorize_label_value_join() {
    let (mut t, r) = fixture();
    t[0][1].id = t[0][0].id;
    size_contract_abstains(t, r);
}
#[test]
fn repeated_id_across_tiers_cannot_authorize_label_value_join() {
    let (mut t, r) = fixture();
    t[1][0].id = t[0][0].id;
    size_contract_abstains(t, r);
}
#[test]
fn repeated_id_in_outside_value_cannot_authorize_label_value_join() {
    let (mut t, r) = fixture();
    t[1][3].id = t[1][0].id;
    size_contract_abstains(t, r);
}

#[test]
fn nonfinite_rule_coordinates_cannot_authorize_label_value_join() {
    for scale in [1., 2.] {
        for vertical in [false, true] {
            for coordinate in 0..3 {
                for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                    let (t, mut r) = scaled_size_contract(scale, false);
                    let mut rule = (0., 200., 300.);
                    match coordinate {
                        0 => rule.0 = value,
                        1 => rule.1 = value,
                        _ => rule.2 = value,
                    }
                    if vertical {
                        r.vrules.push(rule);
                    } else {
                        r.hrules.push(rule);
                    }
                    size_contract_abstains(t, r);
                }
            }
        }
    }
}
#[test]
fn nonfinite_frame_coordinates_cannot_authorize_label_value_join() {
    for scale in [1., 2.] {
        for table in [false, true] {
            for coordinate in 0..4 {
                for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                    let (t, mut r) = scaled_size_contract(scale, false);
                    let mut frame = (200., 220., 0., 20.);
                    match coordinate {
                        0 => frame.0 = value,
                        1 => frame.1 = value,
                        2 => frame.2 = value,
                        _ => frame.3 = value,
                    }
                    if table {
                        r.tables.push(frame);
                    } else {
                        r.boxes.push(frame);
                    }
                    size_contract_abstains(t, r);
                }
            }
        }
    }
}

// Preserve the independent review's original specification unchanged.
#[test]
fn value_vector_order_does_not_change_geometric_decision() {
    let (mut t, r) = fixture();
    for c in &mut t[1] {
        c.y += 8.;
        c.y1 += 8.;
    }
    t[1][1].y += 0.75;
    t[1][1].y1 += 0.75;
    let mut other = t.clone();
    other[1].swap(0, 1);
    rejoin_prose_label_values(&mut t, &r);
    rejoin_prose_label_values(&mut other, &r);
    let contains = |tiers: &[Vec<Col>]| {
        tiers
            .iter()
            .map(|v| {
                let mut ids: Vec<_> = v.iter().map(|c| c.id).collect();
                ids.sort();
                ids
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(contains(&t), contains(&other));
}
fn ordered_value_tops(order: [usize; 3], starts: [f32; 3]) -> (Vec<Vec<Col>>, TierRules) {
    let (mut t, r) = fixture();
    for (c, y) in t[1].iter_mut().take(3).zip(starts) {
        c.y1 += y - c.y;
        c.y = y;
    }
    let originals = t[1][..3].to_vec();
    for (i, source) in order.into_iter().enumerate() {
        t[1][i] = originals[source].clone();
    }
    (t, r)
}
const SOURCE_PERMUTATIONS: [[usize; 3]; 6] = [
    [0, 1, 2],
    [0, 2, 1],
    [1, 0, 2],
    [1, 2, 0],
    [2, 0, 1],
    [2, 1, 0],
];
#[test]
fn all_value_permutations_use_the_same_earliest_geometric_boundary() {
    for order in SOURCE_PERMUTATIONS {
        let (t, r) = ordered_value_tops(order, [162., 162.75, 162.]);
        size_contract_expected(t, r);
    }
}
#[test]
fn incompatible_value_tops_abstain_for_every_source_permutation() {
    // A middle-first anchor can conceal a total spread larger than the existing
    // one-point alignment allowance. No permutation may invent that agreement.
    for order in SOURCE_PERMUTATIONS {
        let (t, r) = ordered_value_tops(order, [161., 162.5, 161.75]);
        size_contract_abstains(t, r);
    }
}
#[test]
fn wholly_late_value_band_abstains_for_every_source_permutation() {
    for order in SOURCE_PERMUTATIONS {
        let (t, r) = ordered_value_tops(order, [162.25, 163., 162.5]);
        size_contract_abstains(t, r);
    }
}
#[test]
fn wholly_early_value_band_abstains_for_every_source_permutation() {
    for order in SOURCE_PERMUTATIONS {
        let (t, r) = ordered_value_tops(order, [148., 148.75, 148.25]);
        size_contract_abstains(t, r);
    }
}
#[test]
fn translated_value_permutations_preserve_all_source_fields_and_membership() {
    for order in SOURCE_PERMUTATIONS {
        let (mut t, r) = ordered_value_tops(order, [162., 162.75, 162.]);
        for c in t.iter_mut().flatten() {
            c.x -= 1024.;
            c.x1 -= 1024.;
            c.y -= 1024.;
            c.y1 -= 1024.;
        }
        size_contract_expected(t, r);
    }
}
