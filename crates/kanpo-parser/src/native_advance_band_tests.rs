use super::*;
const VALUE: &str =
    "1 2 4 10 99.992 100 30 8 0 0 10.000000 90.000000 30.000000 102.000000 10 89.992 30 102";
const BASE:&str="xmin='10' ymin='90' xmax='30' ymax='102' data-wmode='0' data-native-axis='right' data-native-styles='0:4020000000000000'";
fn parse(attrs: &str, value: &str, count: Option<usize>) -> Option<Band> {
    let s = format!("<word {attrs} data-native-advance-band='{value}'>arbitrary</word>");
    let d = scraper::Html::parse_fragment(&s);
    let w = d
        .select(&scraper::Selector::parse("word").unwrap())
        .next()
        .unwrap();
    from_word(w, count)
}
fn changed(field: usize, value: &str) -> String {
    let mut fields = VALUE.split_whitespace().collect::<Vec<_>>();
    fields[field] = value;
    fields.join(" ")
}
#[test]
fn parsed_band144_keeps_two_source_origins_and_distinct_glyph_envelope() {
    let b = parse(BASE, VALUE, Some(4)).unwrap();
    assert_eq!((b.min_baseline, b.max_baseline), (99.992, 100.));
    assert_eq!(b.glyph_bounds, [10., 89.992, 30., 102.]);
    assert_eq!(b.projection, [10., 90., 30., 102.]);
    assert_eq!((b.first, b.after, b.glyph_count), (2, 4, 4));
    assert!(b.belongs([10., 90., 30., 102.]));
}
#[test]
fn parsed_band144_requires_exact_version_and_complete_shape() {
    let fields = VALUE.split_whitespace().collect::<Vec<_>>();
    for skip in 0..18 {
        let s = fields
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != skip)
            .map(|(_, s)| *s)
            .collect::<Vec<_>>()
            .join(" ");
        assert!(parse(BASE, &s, Some(4)).is_none());
    }
    for version in ["0", "2", "bad"] {
        assert!(parse(BASE, &changed(0, version), Some(4)).is_none());
    }
    assert!(parse(BASE, &format!("{VALUE} extra"), Some(4)).is_none());
}
#[test]
fn parsed_band144_requires_its_own_original_page_glyph_range() {
    for count in [None, Some(0), Some(3)] {
        assert!(parse(BASE, VALUE, count).is_none());
    }
    for (field, value) in [(1, "4"), (1, "5"), (1, "-1"), (2, "2"), (2, "5")] {
        assert!(parse(BASE, &changed(field, value), Some(4)).is_none());
    }
}
#[test]
fn parsed_band144_rejects_nonfinite_scalar_or_rectangle_fields() {
    for i in 3..18 {
        for bad in ["NaN", "inf", "-inf"] {
            assert!(parse(BASE, &changed(i, bad), Some(4)).is_none());
        }
    }
}
#[test]
fn parsed_band144_rejects_inverted_ranges_advances_or_rectangles() {
    for (field, v) in [
        (4, "101"),
        (6, "9"),
        (7, "0"),
        (7, "-1"),
        (7, "21"),
        (12, "9"),
        (13, "89"),
        (16, "9"),
        (17, "89"),
    ] {
        assert!(parse(BASE, &changed(field, v), Some(4)).is_none());
    }
}
#[test]
fn parsed_band144_requires_explicit_horizontal_mode_axis_and_native_bounds() {
    for (from, to) in [
        ("data-wmode='0'", ""),
        ("data-wmode='0'", "data-wmode='1'"),
        ("data-native-axis='right'", ""),
        ("data-native-axis='right'", "data-native-axis='unknown'"),
        ("data-native-axis='right'", "data-native-axis='down'"),
        ("xmin='10'", "xmin='NaN'"),
    ] {
        assert!(parse(&BASE.replace(from, to), VALUE, Some(4)).is_none());
    }
}
#[test]
fn parsed_band144_requires_complete_finite_source_styles() {
    for s in ["", "bad", "0:0000000000000000", "0:7ff0000000000000"] {
        assert!(parse(&BASE.replace("0:4020000000000000", s), VALUE, Some(4)).is_none());
    }
    assert!(parse(
        &BASE.replace("data-native-styles='0:4020000000000000'", ""),
        VALUE,
        Some(4)
    )
    .is_none());
}
#[test]
fn parsed_band144_rejects_foreign_projection_or_along_axis_envelope() {
    for (field, v) in [
        (10, "11"),
        (11, "91"),
        (12, "31"),
        (13, "103"),
        (14, "11"),
        (16, "31"),
    ] {
        assert!(parse(BASE, &changed(field, v), Some(4)).is_none());
    }
    assert!(parse(&BASE.replace("xmax='30'", "xmax='31'"), VALUE, Some(4)).is_none());
}
#[test]
fn parsed_band144_whole_origin_range_must_be_in_original_glyph_envelope() {
    for (field, v) in [(4, "89"), (5, "103"), (15, "100"), (17, "99")] {
        assert!(parse(BASE, &changed(field, v), Some(4)).is_none());
    }
}
#[test]
fn parsed_band144_never_rehabilitates_partial_or_contradictory_exact_flow_metadata() {
    for key in [
        "start",
        "baseline",
        "end",
        "leading",
        "trailing",
        "prefix",
        "first-glyph",
        "after-glyph",
    ] {
        let attrs = format!("{BASE} data-native-flow-{key}='10'");
        assert!(parse(&attrs, VALUE, Some(4)).is_none());
    }
}
#[test]
fn parsed_band144_projection_reference_is_exact_canonical_renderer_precision() {
    let value="1 0 2 10.123456789 99.99199999999999 100.00000000000001 30.123456789 8 0 0 10.123457 90.000000 30.123457 102.000000 10.123456789 89.99199999999999 30.123456789 102";
    let attrs = BASE
        .replace("xmin='10'", "xmin='10.123457'")
        .replace("xmax='30'", "xmax='30.123457'");
    let b = parse(&attrs, value, Some(2)).unwrap();
    assert_eq!(b.min_baseline.to_bits(), 99.99199999999999_f64.to_bits());
    assert_eq!(b.max_baseline.to_bits(), 100.00000000000001_f64.to_bits());
    assert_eq!(b.projection, [10.123457, 90., 30.123457, 102.]);
}
#[test]
fn parsed_band144_belongs_never_accepts_a_changed_source_projection_or_range() {
    let b = parse(BASE, VALUE, Some(4)).unwrap();
    assert!(!b.belongs([11., 90., 30., 102.]));
    let bad = Band {
        min_baseline: 101.,
        ..b
    };
    assert!(!bad.belongs(b.projection));
    let bad = Band { after: 5, ..b };
    assert!(!bad.belongs(b.projection));
}

#[test]
fn parsed_band144_actual_producer_keeps_native_precision_without_common_flow() {
    let sources =
        kanpo_pdf::extract_pages_with_aj1_sources(include_bytes!("../tests/data/source-advance-band/20261020g002300021.pdf")).unwrap();
    let x = kanpo_pdf::bbox::bbox_layout_with_native_styles(&sources);
    let d = scraper::Html::parse_document(&x);
    let page = d
        .select(&scraper::Selector::parse("page").unwrap())
        .next()
        .unwrap();
    let count = page
        .value()
        .attr("data-native-glyph-count")
        .unwrap()
        .parse()
        .unwrap();
    let w = page
        .select(&scraper::Selector::parse("word").unwrap())
        .find(|w| w.text().collect::<String>() == "Ｒ6211\u{e61c}12")
        .unwrap();
    assert!(crate::native_word_flow::from_word(w).is_none());
    let b = from_word(w, Some(count)).unwrap();
    assert_eq!((b.first, b.after), (2095, 2103));
    assert!(b.min_baseline < b.max_baseline);
    assert_eq!(b.min_baseline.to_bits(), 454.5453999999994_f64.to_bits());
    assert_eq!(b.max_baseline.to_bits(), 454.55339999999944_f64.to_bits());
    assert!(b.glyph_bounds[1] < f64::from(b.projection[1]));
}

#[test]
fn parsed_band144_one_glyph_cannot_claim_several_origins_or_a_foreign_envelope() {
    assert!(parse(BASE, &changed(2, "3"), Some(4)).is_none());
    let valid = "1 2 3 10 100 100 30 20 0 0 10.000000 90.000000 30.000000 102.000000 10 90 30 102";
    assert!(parse(BASE, valid, Some(4)).is_some());
    assert!(parse(BASE, &valid.replace("30 20 0 0", "30 8 0 0"), Some(4)).is_none());
    assert!(parse(
        BASE,
        &valid.replace("10 90 30 102", "10 89 30 102"),
        Some(4)
    )
    .is_none());
}
