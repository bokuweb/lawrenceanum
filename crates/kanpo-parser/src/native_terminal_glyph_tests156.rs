use super::*;
use crate::trace::WordStatus;
const FLOW:&str="data-wmode='0' data-native-axis='right' data-native-flow-start='10' data-native-flow-baseline='100' data-native-flow-end='30' data-native-flow-leading='8' data-native-flow-trailing='0' data-native-flow-prefix='0' data-native-flow-first-glyph='7' data-native-flow-after-glyph='10'";
fn xml(extra: &str) -> String {
    format!("<word xmin='10' ymin='90' xmax='30' ymax='105' {FLOW} {extra}>Arbitrary</word>")
}
fn parse(raw: &str, total: Option<usize>) -> Option<Terminal> {
    let h = scraper::Html::parse_fragment(raw);
    from_word(
        h.select(&scraper::Selector::parse("word").unwrap())
            .next()
            .unwrap(),
        total,
    )
}
fn good() -> String {
    xml("data-native-terminal-glyph='1 7 10 27 30 100'")
}
fn word() -> Word {
    Word {
        id: 0,
        raw: "Arbitrary".into(),
        text: "Arbitrary".into(),
        x0: 10.,
        y0: 90.,
        x1: 30.,
        y1: 105.,
        source_box: [10., 90., 30., 105.],
        status: WordStatus::Used,
    }
}
#[test]
fn terminal156_complete_tail_keeps_its_own_range_and_variable_glyph_advance() {
    let t = parse(&good(), Some(12)).unwrap();
    assert_eq!(t.advance_for(&word()), Some([27., 30.]));
    assert_eq!(t.range_for(&word()), Some(7..10));
    assert_ne!(30. - 27., t.flow.leading);
}
#[test]
fn terminal156_single_glyph_count_is_independent_of_unicode_length() {
    let s = good()
        .replace("Arbitrary", "ffi")
        .replace("leading='8'", "leading='20'")
        .replace("after-glyph='10'", "after-glyph='8'")
        .replace("1 7 10 27 30 100", "1 7 8 10 30 100");
    let t = parse(&s, Some(12)).unwrap();
    let mut w = word();
    w.raw = "ffi".into();
    w.text = "ffi".into();
    assert_eq!(t.advance_for(&w), Some([10., 30.]));
    assert_eq!(t.range_for(&w), Some(7..8));
}
#[test]
fn terminal156_source_affine_coordinates_keep_the_same_identity() {
    for scale in [0.25, 4.] {
        let x0 = 10. * scale + 17.;
        let x1 = 30. * scale + 17.;
        let y0 = 90. * scale + 31.;
        let y1 = 105. * scale + 31.;
        let y = 100. * scale + 31.;
        let start = 27. * scale + 17.;
        let leading = 8. * scale;
        let s=format!("<word xmin='{x0}' ymin='{y0}' xmax='{x1}' ymax='{y1}' data-wmode='0' data-native-axis='right' data-native-flow-start='{x0}' data-native-flow-baseline='{y}' data-native-flow-end='{x1}' data-native-flow-leading='{leading}' data-native-flow-trailing='0' data-native-flow-prefix='0' data-native-flow-first-glyph='7' data-native-flow-after-glyph='10' data-native-terminal-glyph='1 7 10 {start} {x1} {y}'>Arbitrary</word>");
        let t = parse(&s, Some(12)).unwrap();
        let mut w = word();
        w.x0 = x0 as f32;
        w.x1 = x1 as f32;
        w.y0 = y0 as f32;
        w.y1 = y1 as f32;
        w.source_box = [w.x0, w.y0, w.x1, w.y1];
        assert_eq!(t.advance_for(&w), Some([start, x1]));
    }
}
#[test]
fn terminal156_a_tail_cannot_be_borrowed_by_another_raw_occurrence_or_projection() {
    let t = parse(&good(), Some(12)).unwrap();
    let mut w = word();
    w.raw = "Other".into();
    assert_eq!(t.advance_for(&w), None);
    let mut w = word();
    w.source_box[0] += 0.125;
    assert_eq!(t.advance_for(&w), None);
    let mut w = word();
    w.x1 += 0.125;
    assert_eq!(t.advance_for(&w), None);
    let mut w = word();
    w.y0 = w.y1;
    assert_eq!(t.advance_for(&w), None);
}
#[test]
fn terminal156_absent_tail_does_not_reconstruct_a_last_width_from_the_leading_width() {
    assert_eq!(parse(&xml(""), Some(12)), None);
}
#[test]
fn terminal156_unknown_context_mode_or_axis_declines() {
    for n in [None, Some(0), Some(9)] {
        assert_eq!(parse(&good(), n), None);
    }
    for (k, v) in [
        ("data-wmode='0'", "data-wmode='1'"),
        ("data-native-axis='right'", "data-native-axis='unknown'"),
        ("data-native-axis='right'", "data-native-axis='down'"),
    ] {
        assert_eq!(parse(&good().replace(k, v), Some(12)), None);
    }
}
#[test]
fn terminal156_incomplete_or_conflicting_glyph_ranges_decline() {
    for value in [
        "",
        "2 7 10 27 30 100",
        "1 7 10 27 30",
        "1 7 10 27 30 100 extra",
        "1 6 10 27 30 100",
        "1 7 11 27 30 100",
        "1 10 7 27 30 100",
        "1 7 7 27 30 100",
        "1 7 99 27 30 100",
    ] {
        assert_eq!(
            parse(
                &xml(&format!("data-native-terminal-glyph='{value}'")),
                Some(12)
            ),
            None
        );
    }
    for field in [
        "data-native-flow-first-glyph='7'",
        "data-native-flow-after-glyph='10'",
    ] {
        assert_eq!(parse(&good().replace(field, ""), Some(12)), None);
    }
}
#[test]
fn terminal156_invalid_tail_extent_or_own_baseline_declines() {
    for value in [
        "1 7 10 9 30 100",
        "1 7 10 30 30 100",
        "1 7 10 31 30 100",
        "1 7 10 NaN 30 100",
        "1 7 10 27 31 100",
        "1 7 10 27 30 100.001",
        "1 7 10 27 30 NaN",
    ] {
        assert_eq!(
            parse(
                &xml(&format!("data-native-terminal-glyph='{value}'")),
                Some(12)
            ),
            None
        );
    }
    let one = good()
        .replace("after-glyph='10'", "after-glyph='8'")
        .replace("1 7 10 27 30 100", "1 7 8 27 30 100");
    assert_eq!(parse(&one, Some(12)), None);
}
#[test]
fn terminal156_an_incomplete_native_flow_cannot_be_repaired_by_a_tail() {
    for field in [
        "data-native-flow-start='10'",
        "data-native-flow-baseline='100'",
        "data-native-flow-end='30'",
        "data-native-flow-leading='8'",
        "data-native-flow-trailing='0'",
        "data-native-flow-prefix='0'",
    ] {
        assert_eq!(parse(&good().replace(field, ""), Some(12)), None);
    }
}
#[test]
fn terminal156_original_identity_handles_reordered_trace_word_ids() {
    let first = good();
    let second = good()
        .replace("xmin='10'", "xmin='40'")
        .replace("xmax='30'", "xmax='60'")
        .replace("start='10'", "start='40'")
        .replace("end='30'", "end='60'")
        .replace("1 7 10 27 30 100", "1 10 13 57 60 100")
        .replace("first-glyph='7'", "first-glyph='10'")
        .replace("after-glyph='10'", "after-glyph='13'")
        .replace("Arbitrary", "Other");
    let h = scraper::Html::parse_fragment(&format!(
        "<page data-native-glyph-count='13'>{first}{second}</page>"
    ));
    let page = h
        .select(&scraper::Selector::parse("page").unwrap())
        .next()
        .unwrap();
    let mut a = word();
    a.raw = "Other".into();
    a.text = "Other".into();
    a.x0 = 40.;
    a.x1 = 60.;
    a.source_box = [40., 90., 60., 105.];
    let mut b = word();
    b.id = 1;
    let ws = vec![a, b];
    let ts = for_words(page, &ws).values;
    assert_eq!(
        ts[0].as_ref().and_then(|t| t.advance_for(&ws[0])),
        Some([57., 60.])
    );
    assert_eq!(
        ts[1].as_ref().and_then(|t| t.advance_for(&ws[1])),
        Some([27., 30.])
    );
}
#[test]
fn terminal156_ambiguous_source_occurrences_do_not_borrow_a_good_tail() {
    for second in [good(), xml("")] {
        let h = scraper::Html::parse_fragment(&format!(
            "<page data-native-glyph-count='12'>{}{second}</page>",
            good()
        ));
        let page = h
            .select(&scraper::Selector::parse("page").unwrap())
            .next()
            .unwrap();
        assert_eq!(for_words(page, &[word()]).values, vec![None]);
    }
}
#[test]
fn terminal156_invalid_trace_ids_are_not_metadata_vector_positions() {
    let h = scraper::Html::parse_fragment(&format!(
        "<page data-native-glyph-count='12'>{}</page>",
        good()
    ));
    let page = h
        .select(&scraper::Selector::parse("page").unwrap())
        .next()
        .unwrap();
    let mut w = word();
    w.id = 9;
    assert_eq!(for_words(page, &[w]).values, vec![None]);
}

#[test]
fn terminal156_one_original_glyph_range_cannot_certify_two_different_words() {
    let first = good();
    let second = good()
        .replace("xmin='10'", "xmin='40'")
        .replace("xmax='30'", "xmax='60'")
        .replace("start='10'", "start='40'")
        .replace("end='30'", "end='60'")
        .replace("1 7 10 27 30 100", "1 9 12 57 60 100")
        .replace("first-glyph='7'", "first-glyph='9'")
        .replace("after-glyph='10'", "after-glyph='12'")
        .replace("Arbitrary", "Other");
    let h = scraper::Html::parse_fragment(&format!(
        "<page data-native-glyph-count='13'>{first}{second}</page>"
    ));
    let page = h
        .select(&scraper::Selector::parse("page").unwrap())
        .next()
        .unwrap();
    let a = word();
    let mut b = word();
    b.id = 1;
    b.raw = "Other".into();
    b.text = "Other".into();
    b.x0 = 40.;
    b.x1 = 60.;
    b.source_box = [40., 90., 60., 105.];
    assert_eq!(for_words(page, &[a, b]).values, vec![None, None]);
}
