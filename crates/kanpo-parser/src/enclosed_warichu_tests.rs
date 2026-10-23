use super::*;
fn sample(n: usize) -> Vec<Col> {
    let mut cs = vec![
        Col {
            id: 0,
            x: 96.,
            x1: 104.,
            y: 0.,
            y1: 40.,
            text: "任意の引用（abc".into(),
        },
        Col {
            id: 1,
            x: 96.,
            x1: 104.,
            y: 120.,
            y1: 200.,
            text: "XYZ）本文".into(),
        },
    ];
    for k in 0..n {
        for j in 0..3 {
            let x = 100. + n as f32 * 4. - (k + 1) as f32 * 8.;
            cs.push(Col {
                id: cs.len(),
                x,
                x1: x + 8.,
                y: 40. + j as f32 * 32.,
                y1: 56. + j as f32 * 32.,
                text: format!("{k}{j}"),
            });
        }
    }
    cs
}
#[test]
fn lane_count_and_wording_are_not_fixed() {
    for n in [2, 3, 4, 7, 16] {
        let mut cs = sample(n);
        let expected: String = cs[2..].iter().map(|c| c.text.as_str()).collect();
        let ownership = enclosed_warichu::merge(&mut cs, &PageRules::default());
        assert_eq!(cs.len(), 1, "lanes={n}");
        assert_eq!(ownership.len(), n * 3 + 1);
        assert!(cs.iter().any(|c| c
            .text
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .collect::<String>()
            == format!("任意の引用（abc{expected}XYZ）本文")));
    }
}
#[test]
fn uniform_scale_translation_and_input_order_do_not_change_ownership() {
    for scale in [0.125, 1., 8.] {
        let mut cs = sample(4);
        for c in &mut cs {
            c.x = c.x * scale + 13.;
            c.x1 = c.x1 * scale + 13.;
            c.y = c.y * scale + 23.;
            c.y1 = c.y1 * scale + 23.;
        }
        cs.reverse();
        enclosed_warichu::merge(&mut cs, &PageRules::default());
        assert_eq!(cs.len(), 1);
    }
}
#[test]
fn independent_axis_scaling_does_not_impose_an_internal_line_gap() {
    for (sx, sy) in [(0.125, 8.), (8., 0.125)] {
        let mut cs = sample(4);
        for c in &mut cs {
            c.x = c.x * sx + 13.;
            c.x1 = c.x1 * sx + 13.;
            c.y = c.y * sy + 23.;
            c.y1 = c.y1 * sy + 23.;
        }
        enclosed_warichu::merge(&mut cs, &PageRules::default());
        assert_eq!(cs.len(), 1);
    }
}
#[test]
fn absent_brackets_parent_axis_mismatch_and_crossing_words_are_held() {
    for mode in 0..5 {
        let mut cs = sample(4);
        match mode {
            0 => cs[0].text = "前の段落".into(),
            1 => cs[1].text = "次の段落".into(),
            2 => {
                cs[0].x += 4.;
                cs[0].x1 += 4.;
            }
            3 => cs[2].y -= 4.,
            _ => cs[2].x1 += 2.,
        }
        assert!(
            enclosed_warichu::merge(&mut cs, &PageRules::default()).is_empty(),
            "mode={mode}"
        );
        assert_eq!(cs.len(), 14);
    }
}
#[test]
fn crossed_rules_or_invalid_duplicate_or_unknown_source_cannot_authorize_a_merge() {
    for mode in 0..7 {
        let mut cs = sample(4);
        let mut rules = PageRules::default();
        match mode {
            0 => rules.vertical.push(crate::lines::VRule {
                x: 100.,
                y0: 0.,
                y1: 200.,
            }),
            1 => rules.horizontal.push(crate::lines::HRule {
                y: 80.,
                x0: 0.,
                x1: 200.,
            }),
            2 => cs[3].id = cs[2].id,
            3 => cs[2].y = f32::NAN,
            4 => cs[2].x1 = cs[2].x,
            5 => cs[2].text = "\u{e000}".into(),
            _ => cs[2].y1 = cs[2].y,
        }
        assert!(
            enclosed_warichu::merge(&mut cs, &rules).is_empty(),
            "mode={mode}"
        );
        assert_eq!(cs.len(), 14);
    }
}
#[test]
fn lane_boundaries_are_whitespace_and_not_invented_punctuation() {
    let mut cs = sample(3);
    let expected: Vec<String> = (0..3)
        .map(|k| (0..3).map(|j| format!("{k}{j}")).collect())
        .collect();
    enclosed_warichu::merge(&mut cs, &PageRules::default());
    let merged = cs.iter().find(|c| c.id == 0).unwrap();
    assert_eq!(
        merged
            .text
            .strip_prefix("任意の引用（abc")
            .unwrap()
            .strip_suffix("XYZ）本文")
            .unwrap()
            .split_whitespace()
            .collect::<Vec<_>>(),
        expected.iter().map(String::as_str).collect::<Vec<_>>()
    );
}
#[test]
fn malformed_rule_coordinates_hold_the_source_partition() {
    let mut cs = sample(4);
    let mut rules = PageRules::default();
    rules.vertical.push(crate::lines::VRule {
        x: f32::NAN,
        y0: 0.,
        y1: 200.,
    });
    assert!(enclosed_warichu::merge(&mut cs, &rules).is_empty());
    assert_eq!(cs.len(), 14);
}

#[test]
fn printed_middle_dot_is_preserved_without_synthetic_punctuation() {
    let mut cs = sample(3);
    cs[2].text = "原文・記号".into();
    let count = |cols: &[Col]| {
        let mut h = std::collections::BTreeMap::new();
        for ch in cols
            .iter()
            .flat_map(|c| c.text.chars())
            .filter(|c| !c.is_whitespace())
        {
            *h.entry(ch).or_insert(0usize) += 1;
        }
        h
    };
    let source = count(&cs);
    enclosed_warichu::merge(&mut cs, &PageRules::default());
    assert_eq!(cs.len(), 1);
    assert_eq!(count(&cs), source);
    assert_eq!(cs[0].text.matches('・').count(), 1);
}
#[test]
fn competing_enclosing_parent_interpretations_are_held() {
    let mut cs = sample(3);
    let c = &cs[1];
    cs.push(Col {
        id: 100,
        x: c.x,
        x1: c.x1,
        y: c.y,
        y1: c.y1,
        text: c.text.clone(),
    });
    let before: Vec<_> = cs.iter().map(|c| (c.id, c.text.clone())).collect();
    assert!(enclosed_warichu::merge(&mut cs, &PageRules::default()).is_empty());
    assert_eq!(
        cs.iter()
            .map(|c| (c.id, c.text.clone()))
            .collect::<Vec<_>>(),
        before
    );
}
