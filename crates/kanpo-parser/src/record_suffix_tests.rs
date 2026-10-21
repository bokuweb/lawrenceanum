use super::*;
use crate::trace::Line;
fn col(id: usize, x: f32, y: f32, y1: f32, t: &str) -> Col {
    Col {
        id,
        x: x - 4.,
        x1: x + 4.,
        y,
        y1,
        text: t.into(),
    }
}
fn fixture() -> Vec<Vec<Col>> {
    let mut bands = vec![vec![], vec![], vec![], vec![]];
    let mut id = 0;
    for x in [200., 160., 120., 80.] {
        for (b, y, t) in [
            (0, 100., "団体名称"),
            (1, 148., "代表氏名"),
            (2, 196., "事務所住所"),
        ] {
            bands[b].push(col(id, x, y, y + 32., t));
            id += 1;
        }
        bands[2].push(col(id, x - 8., 196., 228., "折返住所"));
        id += 1;
        for (y, y1, t) in [
            (236., 276., "衆議院議員"),
            (296., 304., "〇"),
            (332., 348., "八、"),
            (356., 380., "二、一六"),
        ] {
            bands[3].push(col(id, x, y, y1, t));
            id += 1;
        }
    }
    bands
}
#[test]
fn a_labeled_header_is_required_before_enabling_the_alternative() {
    let u = vec![Unit::Plain(vec![Line::text(
        "政治団体の名称　代表者の氏名　所在地　届出年月日".into(),
        vec![],
    )])];
    assert!(has_prior_record_header(&u));
    assert!(!has_prior_record_header(&[]));
    let u = vec![Unit::Plain(vec![Line::text(
        "河西宏一　東京都　八、二、一六".into(),
        vec![],
    )])];
    assert!(!has_prior_record_header(&u));
    let u = vec![Unit::Plain(vec![Line::text(
        "代表者の氏名　届出年月日".into(),
        vec![],
    )])];
    assert!(!has_prior_record_header(&u));
}
#[test]
fn wrapped_addresses_share_the_one_column_mark_and_date_suffix() {
    let b = fixture();
    let tops = infer_stacked_suffix(&b, 3, &[]).unwrap();
    assert_eq!(tops.len(), 2);
    assert!(tops[0] > 276. && tops[0] < 296. && tops[1] > 304. && tops[1] < 332.);
}
#[test]
fn only_the_last_band_can_be_a_suffix() {
    let mut b = fixture();
    b.push(b[3].clone());
    assert!(infer_stacked_suffix(&b, 3, &[]).is_none());
}
#[test]
fn fewer_than_three_records_are_not_sufficient() {
    let mut b = fixture();
    for a in &mut b {
        a.retain(|c| c.x > 130.);
    }
    assert!(infer_stacked_suffix(&b, 3, &[]).is_none());
}
#[test]
fn a_missing_mark_is_not_inferred() {
    let mut b = fixture();
    b[3].retain(|c| !(c.text == "〇" && c.x > 190.));
    assert!(infer_stacked_suffix(&b, 3, &[]).is_none());
}
#[test]
fn a_non_date_tail_is_not_joined() {
    let mut b = fixture();
    b[3].iter_mut().find(|c| c.text == "二、一六").unwrap().text = "本文です。".into();
    assert!(infer_stacked_suffix(&b, 3, &[]).is_none());
}
#[test]
fn an_unmatched_record_anchor_is_rejected() {
    let mut b = fixture();
    for c in &mut b[3] {
        if c.x > 190. {
            c.x += 5.;
            c.x1 += 5.;
        }
    }
    assert!(infer_stacked_suffix(&b, 3, &[]).is_none());
}
#[test]
fn a_crossing_article_rule_is_a_fence() {
    let b = fixture();
    assert!(infer_stacked_suffix(&b, 3, &[(232., 70., 210.)]).is_none());
}
#[test]
fn a_distant_or_short_rule_does_not_split_the_records() {
    let b = fixture();
    assert!(infer_stacked_suffix(&b, 3, &[(232., 300., 400.), (232., 100., 110.)]).is_some());
}
#[test]
fn misaligned_field_baselines_are_rejected() {
    let mut b = fixture();
    b[3].iter_mut().find(|c| c.text == "〇").unwrap().y += 8.;
    assert!(infer_stacked_suffix(&b, 3, &[]).is_none());
}
#[test]
fn equal_column_counts_without_two_independent_anchor_bands_are_not_enough() {
    let mut b = fixture();
    for band in &mut b[..2] {
        for c in band {
            c.x += 8.;
            c.x1 += 8.;
        }
    }
    assert!(infer_stacked_suffix(&b, 3, &[]).is_none());
}
#[test]
fn prose_in_a_name_column_is_rejected() {
    let mut b = fixture();
    b[0][0].text.push('。');
    assert!(infer_stacked_suffix(&b, 3, &[]).is_none());
}
#[test]
fn a_suffix_with_an_extra_unmatched_word_is_rejected() {
    let mut b = fixture();
    b[3].push(col(999, 220., 236., 268., "無関係"));
    assert!(infer_stacked_suffix(&b, 3, &[]).is_none());
}
#[test]
fn duplicate_source_ids_are_rejected() {
    let mut b = fixture();
    b[3][1].id = b[3][0].id;
    assert!(infer_stacked_suffix(&b, 3, &[]).is_none());
}

fn generality_office(t: &str) -> Vec<Vec<Col>> {
    let mut b = fixture();
    for c in &mut b[3] {
        if c.text == "衆議院議員" {
            c.text = t.into();
        }
    }
    b
}
#[test]
fn generality_office_latin_is_already_supported_control() {
    assert_eq!(
        infer_stacked_suffix(&generality_office("OfficeAlpha"), 3, &[]),
        infer_stacked_suffix(&fixture(), 3, &[])
    );
}
#[test]
fn generality_office_short_payload_same_source_geometry() {
    assert_eq!(
        infer_stacked_suffix(&generality_office("部"), 3, &[]),
        infer_stacked_suffix(&fixture(), 3, &[])
    );
}
#[test]
fn generality_office_long_payload_same_source_geometry() {
    assert_eq!(
        infer_stacked_suffix(&generality_office(&"部".repeat(25)), 3, &[]),
        infer_stacked_suffix(&fixture(), 3, &[])
    );
}
#[test]
fn generality_office_numeric_mixed_payload_same_source_geometry() {
    assert_eq!(
        infer_stacked_suffix(&generality_office("第2部"), 3, &[]),
        infer_stacked_suffix(&fixture(), 3, &[])
    );
}

// Opaque office payload regressions. Coordinates/IDs/date/markers stay fixed.
#[test]
fn opaque_office_fullwidth_digits_preserve_the_same_field() {
    assert_eq!(
        infer_stacked_suffix(&generality_office("第２部"), 3, &[]),
        infer_stacked_suffix(&fixture(), 3, &[])
    );
}
#[test]
fn opaque_office_empty_payload_is_rejected() {
    assert!(infer_stacked_suffix(&generality_office(""), 3, &[]).is_none());
}
#[test]
fn opaque_office_whitespace_only_payload_is_rejected() {
    for text in [" ", "　", "\t", "\r\n", "\u{2003}", "　 \t"] {
        assert!(
            infer_stacked_suffix(&generality_office(text), 3, &[]).is_none(),
            "blank payload {:?}",
            text
        );
    }
}
#[test]
fn opaque_office_sentence_delimiters_and_embedded_markers_are_rejected() {
    for text in [
        "部署。",
        "部署、",
        "部〇署",
        "部○署",
        "第2部。",
        "Office,。",
    ] {
        assert!(
            infer_stacked_suffix(&generality_office(text), 3, &[]).is_none(),
            "structural punctuation {:?}",
            text
        );
    }
}
#[test]
fn opaque_office_payload_does_not_relax_the_date_syntax() {
    for text in [
        "本文です",
        "八、二",
        "八、、二",
        "abc、def、ghi",
        "八、二、一六。",
    ] {
        let mut bands = generality_office("OfficeAlpha");
        for col in &mut bands[3] {
            if col.text == "八、" {
                col.text = text.into();
            } else if col.text == "二、一六" {
                col.text.clear();
            }
        }
        assert!(
            infer_stacked_suffix(&bands, 3, &[]).is_none(),
            "non-date {:?}",
            text
        );
    }
}

// Calendar component function-contract probes: IDs/geometry/record structure fixed.
fn calendar59_bands(value: &str) -> Vec<Vec<Col>> {
    let mut bands = fixture();
    let (year, rest) = match value.find('、') {
        Some(i) => (&value[..i + '、'.len_utf8()], &value[i + '、'.len_utf8()..]),
        None => (value, ""),
    };
    for c in &mut bands[3] {
        if c.text == "八、" {
            c.text = year.into();
        } else if c.text == "二、一六" {
            c.text = rest.into();
        }
    }
    let shape = |bs: &[Vec<Col>]| {
        bs.iter()
            .flatten()
            .map(|c| {
                (
                    c.id,
                    c.x.to_bits(),
                    c.x1.to_bits(),
                    c.y.to_bits(),
                    c.y1.to_bits(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(shape(&bands), shape(&fixture()));
    bands
}
fn calendar59_accept(value: &str) {
    assert_eq!(
        infer_stacked_suffix(&calendar59_bands(value), 3, &[]),
        infer_stacked_suffix(&fixture(), 3, &[]),
        "valid supported numeric date {}",
        value
    );
}
fn calendar59_reject(value: &str) {
    assert!(
        infer_stacked_suffix(&calendar59_bands(value), 3, &[]).is_none(),
        "invalid or unsupported date {}",
        value
    );
}
#[test]
fn calendar59_month99_is_not_a_date() {
    calendar59_reject("八、九九、一六");
}
#[test]
fn calendar59_day99_is_not_a_date() {
    calendar59_reject("八、二、九九");
}
#[test]
fn calendar59_four_digit_ascii_year_is_not_a_length_error() {
    calendar59_accept("2026、2、16");
}
#[test]
fn calendar59_four_digit_fullwidth_year_is_equivalent() {
    calendar59_accept("２０２６、２、１６");
}
#[test]
fn calendar59_four_digit_kanji_place_value_year_is_equivalent() {
    calendar59_accept("二〇二六、二、一六");
}
#[test]
fn calendar59_zero_year_is_invalid() {
    for s in ["〇、二、一六", "0、2、16", "零、二、十六"] {
        calendar59_reject(s);
    }
}
#[test]
fn calendar59_zero_month_is_invalid() {
    calendar59_reject("八、〇、一六");
}
#[test]
fn calendar59_zero_day_is_invalid() {
    calendar59_reject("八、二、〇");
}
#[test]
fn calendar59_month13_is_invalid() {
    calendar59_reject("八、一三、一六");
}
#[test]
fn calendar59_february30_is_invalid() {
    calendar59_reject("八、二、三〇");
}
#[test]
fn calendar59_april31_is_invalid() {
    calendar59_reject("八、四、三一");
}
#[test]
fn calendar59_other_short_months_reject31() {
    for m in ["六", "九", "十一"] {
        calendar59_reject(&format!("八、{m}、三十一"));
    }
}
#[test]
fn calendar59_long_months_accept31() {
    for m in ["一", "三", "五", "七", "八", "十", "十二"] {
        calendar59_accept(&format!("八、{m}、三十一"));
    }
}
#[test]
fn calendar59_february29_does_not_infer_an_era_or_leap_year() {
    for y in ["八", "7", "2026"] {
        calendar59_accept(&format!("{y}、二、二十九"));
    }
}
#[test]
fn calendar59_single_tens_usual_forms_are_supported() {
    for s in [
        "十、一、十一",
        "一十、一、十",
        "十一、二、二十",
        "二十一、十二、三十一",
        "九十九、十一、二十一",
        "八、十、三十",
    ] {
        calendar59_accept(s);
    }
}
#[test]
fn calendar59_repeated_tens_are_invalid() {
    for s in [
        "十十、二、一六",
        "一十十、二、一六",
        "八、十十、一六",
        "八、二、一十十",
    ] {
        calendar59_reject(s);
    }
}
#[test]
fn calendar59_zero_and_nonkanji_unit_tens_forms_are_unsupported() {
    for s in [
        "零十、二、一六",
        "〇十、二、一六",
        "八、二、十〇",
        "1十、二、一六",
        "八、二、十1",
        "八、二、十１",
    ] {
        calendar59_reject(s);
    }
}
#[test]
fn calendar59_u32_max_year_is_not_a_character_length_cap() {
    calendar59_accept("4294967295、1、1");
}
#[test]
fn calendar59_checked_numeric_overflow_is_rejected() {
    for s in [
        "4294967296、1、1",
        "四二九四九六七二九六、一、一",
        "99999999999999999999、1、1",
        "8、4294967296、1",
        "8、1、4294967296",
    ] {
        calendar59_reject(s);
    }
}
#[test]
fn calendar59_decimal_notations_preserve_value() {
    for s in [
        "8、2、16",
        "８、２、１６",
        "八、二、一六",
        "八、二、十六",
        "零八、〇二、〇一六",
        "0八、２、十六",
    ] {
        calendar59_accept(s);
    }
}
#[test]
fn calendar59_unsupported_grammar_and_wrong_separators_remain_rejected() {
    for s in [
        "元、二、一六",
        "百、二、一六",
        "八、二、百",
        "八,二,一六",
        "八、二",
        "八、二、一六、七",
        "八、、一六",
        "八、二、",
        "-1、2、16",
        "8、2、16。",
    ] {
        calendar59_reject(s);
    }
}
