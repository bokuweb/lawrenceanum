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
