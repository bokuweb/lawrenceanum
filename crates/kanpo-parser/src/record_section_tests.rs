use super::*;
fn sample() -> (Vec<Vec<Col>>, Vec<f32>) {
    let mut bs = vec![vec![], vec![], vec![]];
    let mut id = 0;
    let mut add = |band: usize, x: f32, y: f32, y1: f32, text: &str| {
        bs[band].push(Col {
            id,
            x,
            x1: x + 8.,
            y,
            y1,
            text: text.into(),
        });
        id += 1;
    };
    for x in [240., 200., 80., 40.] {
        add(0, x, 10., 50., "任意の団体名");
        add(1, x, 100., 132., "甲乙丙丁");
        add(2, x, 180., 220., "八、四、一");
    }
    add(0, 160., 10., 18., "（二）");
    add(0, 160., 26., 130., "任意の区分見出し");
    add(0, 128., 10., 42., "名称");
    add(1, 128., 100., 124., "氏名");
    add(2, 128., 180., 220., "年月日");
    (bs, vec![244., 204., 84., 44.])
}
#[test]
fn complete_section_and_following_header_have_their_own_source_rows() {
    let (bs, a) = sample();
    assert_eq!(record_section_anchors(&bs, &a), vec![164., 132.]);
}
#[test]
fn section_geometry_is_invariant_under_scale_translation_and_word_order() {
    for scale in [0.125, 1., 8.] {
        let (mut bs, mut a) = sample();
        for c in bs.iter_mut().flatten() {
            c.x = c.x * scale + 13.;
            c.x1 = c.x1 * scale + 13.;
            c.y = c.y * scale + 23.;
            c.y1 = c.y1 * scale + 23.;
        }
        for b in &mut bs {
            b.reverse();
        }
        a.iter_mut().for_each(|x| *x = *x * scale + 13.);
        assert_eq!(
            record_section_anchors(&bs, &a),
            vec![164. * scale + 13., 132. * scale + 13.]
        );
    }
}
#[test]
fn the_caption_and_section_number_have_no_character_limit_or_specific_wording() {
    for marker in ["（三百二十一）", "（12345）", "（１２３）"] {
        let (mut bs, a) = sample();
        bs[0].iter_mut().find(|c| c.text == "（二）").unwrap().text = marker.into();
        bs[0]
            .iter_mut()
            .find(|c| c.text == "任意の区分見出し")
            .unwrap()
            .text = "Z任意caption".repeat(30);
        assert_eq!(record_section_anchors(&bs, &a), vec![164., 132.]);
    }
}
#[test]
fn missing_caption_or_header_or_cross_field_span_holds_the_old_partition() {
    for mode in 0..4 {
        let (mut bs, a) = sample();
        match mode {
            0 => bs[0].retain(|c| c.text != "任意の区分見出し"),
            1 => bs.iter_mut().for_each(|b| b.retain(|c| c.x != 128.)),
            2 => {
                bs[0]
                    .iter_mut()
                    .find(|c| c.text == "任意の区分見出し")
                    .unwrap()
                    .y1 = 70.
            }
            _ => {
                bs[0].iter_mut().find(|c| c.text == "（二）").unwrap().text =
                    "「（二）引用」".into()
            }
        }
        assert!(record_section_anchors(&bs, &a).is_empty(), "mode={mode}");
    }
}
#[test]
fn repeated_role_or_labels_in_one_field_do_not_certify_a_header() {
    for same_role in [true, false] {
        let (mut bs, a) = sample();
        if same_role {
            for c in bs.iter_mut().flatten().filter(|c| c.x == 128.) {
                c.text = "名称".into();
            }
        } else {
            let mut moved = vec![];
            for b in bs.iter_mut().skip(1) {
                let i = b.iter().position(|c| c.x == 128.).unwrap();
                let mut c = b.remove(i);
                c.y = 50.;
                c.y1 = 58.;
                moved.push(c);
            }
            bs[0].extend(moved);
        }
        assert!(record_section_anchors(&bs, &a).is_empty());
    }
}
#[test]
fn invalid_source_or_crossing_words_cannot_create_new_record_rows() {
    for mode in 0..8 {
        let (mut bs, a) = sample();
        match mode {
            0 => bs[0][0].x = f32::NAN,
            1 => bs[0][0].x1 = bs[0][0].x,
            2 => bs[0][1].id = bs[0][0].id,
            3 => {
                let mut c = bs[0][0].clone();
                c.id = 100;
                c.x = 150.;
                c.x1 = 180.;
                bs[0].push(c);
            }
            4 => {
                let mut c = bs[0][0].clone();
                c.id = 100;
                c.x = 120.;
                c.x1 = 144.;
                bs[0].push(c);
            }
            5 => {
                bs[0]
                    .iter_mut()
                    .find(|c| c.text == "任意の区分見出し")
                    .unwrap()
                    .text = "\u{eabc}".into()
            }
            6 => {
                bs[0][0].x = -f32::MAX;
                bs[0][0].x1 = f32::MAX;
            }
            _ => {
                bs[0][0].y = -f32::MAX;
                bs[0][0].y1 = f32::MAX;
            }
        }
        assert!(record_section_anchors(&bs, &a).is_empty(), "mode={mode}");
    }
}

fn assert_original_record_sections(pdf: &[u8], gold: &str) {
    let reference: serde_json::Value = serde_json::from_str(gold).unwrap();
    let pages = crate::page_inputs(pdf).unwrap();
    let a = crate::analyze_item(&pages);
    let lines: Vec<_> = a.pages[0].regions.iter().flat_map(|r| &r.lines).collect();
    let used: std::collections::BTreeSet<_> = a.pages[0]
        .words
        .iter()
        .filter(|w| matches!(w.status, crate::trace::WordStatus::Used))
        .map(|w| w.id)
        .collect();
    let all_ids: Vec<_> = lines.iter().flat_map(|l| l.words.iter().copied()).collect();
    let unique_ids: std::collections::BTreeSet<_> = all_ids.iter().copied().collect();
    assert_eq!(
        all_ids.len(),
        unique_ids.len(),
        "a complete source word appears twice"
    );
    assert_eq!(unique_ids, used, "every used source word keeps one owner");
    let mut positions = Vec::new();
    for role in ["row", "section", "header"] {
        let expected: std::collections::BTreeSet<usize> = reference[role]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i.as_u64().unwrap() as usize)
            .collect();
        let found = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| {
                l.words
                    .iter()
                    .copied()
                    .collect::<std::collections::BTreeSet<_>>()
                    == expected
            })
            .collect::<Vec<_>>();
        assert_eq!(found.len(), 1, "role={role}");
        let (position, line) = found[0];
        assert_eq!(
            line.words.len(),
            expected.len(),
            "duplicate source word in {role}"
        );
        positions.push(position);
        if role == "row" {
            assert_eq!(
                line.text,
                reference["row_fields"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|s| s.as_str().unwrap())
                    .collect::<Vec<_>>()
                    .join("　")
            );
        }
        if role == "section" {
            assert_eq!(line.text, reference["section_text"].as_str().unwrap());
        }
    }
    assert!(positions.windows(2).all(|p| p[0] + 1 == p[1]));
}
#[test]
fn original_record_section_and_header_have_separate_complete_word_ownership() {
    assert_original_record_sections(
        include_bytes!("../tests/corpus/2026-09-01-gougai195-p16/page.pdf"),
        include_str!("../tests/data/record-sections/20260901g001950016.json"),
    );
}
#[test]
fn another_original_preserves_the_reversed_address_change_and_its_new_section() {
    assert_original_record_sections(
        include_bytes!("../tests/data/record-sections/20261001g002170056.pdf"),
        include_str!("../tests/data/record-sections/20261001g002170056.json"),
    );
}

#[test]
fn competing_section_columns_and_invalid_record_axes_are_held() {
    let (mut bs, a) = sample();
    let mut c = bs[0].iter().find(|c| c.text == "（二）").unwrap().clone();
    c.id = 100;
    c.x = 148.;
    c.x1 = 156.;
    bs[0].push(c);
    let mut c = bs[0]
        .iter()
        .find(|c| c.text == "任意の区分見出し")
        .unwrap()
        .clone();
    c.id = 101;
    c.x = 148.;
    c.x1 = 156.;
    bs[0].push(c);
    assert!(record_section_anchors(&bs, &a).is_empty());
    let (bs, mut a) = sample();
    a[1] = a[0];
    assert!(record_section_anchors(&bs, &a).is_empty());
    a[1] = f32::NAN;
    assert!(record_section_anchors(&bs, &a).is_empty());
}
