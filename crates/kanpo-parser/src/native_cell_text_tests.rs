use super::*;
use crate::trace::{Line, Table, TableCell, WordStatus};
struct Fixture {
    units: Vec<Unit>,
    words: Vec<Word>,
    bounds: Vec<NativeBounds>,
    flows: Vec<Option<crate::native_cell_origin::Origin>>,
}
fn fixture() -> Fixture {
    let specs = [
        ("arbitrary", 20., 40., 65., 46., 0., 0, 9),
        ("label", 70., 40., 95., 46., 5., 9, 14),
        ("continued", 20., 52., 65., 58., 0., 14, 23),
    ];
    let mut f = Fixture {
        units: vec![],
        words: vec![],
        bounds: vec![],
        flows: vec![],
    };
    for (text, x, y, end, base, prefix, first, after) in specs {
        let id = f.words.len();
        let b = [x, y, end, y + 8.];
        f.words.push(Word {
            id,
            x0: x,
            y0: y,
            x1: end,
            y1: y + 8.,
            source_box: b,
            raw: text.into(),
            text: text.into(),
            status: WordStatus::Used,
        });
        f.bounds.push(NativeBounds::Glyphs(b));
        let raw=format!("<word xmin='{x}' ymin='{y}' xmax='{end}' ymax='{}' data-native-axis='right' data-wmode='0' data-native-flow-start='{x}' data-native-flow-end='{end}' data-native-flow-baseline='{base}' data-native-flow-leading='4' data-native-flow-trailing='0' data-native-flow-prefix='{prefix}' data-native-flow-first-glyph='{first}' data-native-flow-after-glyph='{after}'>{text}</word>",y+8.);
        let raw = raw.replacen(" data-native-axis", &format!(" data-native-cell-origin='2 {first} {after} {x} {base} {end} 4 0 {prefix} {x} {y} {end} {} 8 8' data-native-axis", y + 8.), 1);
        let h = scraper::Html::parse_fragment(&raw);
        let w = h
            .select(&scraper::Selector::parse("word").unwrap())
            .next()
            .unwrap();
        f.flows
            .push(crate::native_cell_origin::from_word(w, Some(23)));
    }
    assert!(f.flows.iter().all(Option::is_some));
    let table = Table {
        x0: 10.,
        y0: 30.,
        x1: 110.,
        y1: 80.,
        rows: 1,
        cols: 1,
        cells: vec![TableCell {
            row: 0,
            col: 0,
            rowspan: 1,
            colspan: 1,
            x0: 10.,
            y0: 30.,
            x1: 110.,
            y1: 80.,
            text: "arbitrarylabelcontinued".into(),
            words: vec![2, 0, 1],
        }],
    };
    f.units = vec![Unit::Plain(vec![Line {
        text: table.render(),
        words: vec![],
        table: Some(table),
    }])];
    f
}
fn cell(f: &Fixture) -> &TableCell {
    let Unit::Plain(lines) = &f.units[0] else {
        panic!()
    };
    &lines[0].table.as_ref().unwrap().cells[0]
}
fn apply(f: &mut Fixture) {
    restore(&mut f.units, &f.words, &f.bounds, &f.flows)
}
#[test]
fn source_same_baseline_gaps_preserve_the_existing_logical_cell_text() {
    let mut f = fixture();
    apply(&mut f);
    assert_eq!(cell(&f).text, "arbitrary labelcontinued");
    assert_eq!(cell(&f).words, vec![2, 0, 1]);
    let Unit::Plain(lines) = &f.units[0] else {
        panic!()
    };
    assert_eq!(lines[0].text, lines[0].table.as_ref().unwrap().render());
}
#[test]
fn unknown_flow_does_not_guess_cell_line_or_word_boundaries() {
    let mut f = fixture();
    f.flows[1] = None;
    let before = snapshot(&f.units);
    apply(&mut f);
    assert_eq!(snapshot(&f.units), before);
}
#[test]
fn incomplete_source_occurrence_does_not_repair_a_cell() {
    let mut f = fixture();
    f.bounds[1] = NativeBounds::Unknown;
    let before = snapshot(&f.units);
    apply(&mut f);
    assert_eq!(snapshot(&f.units), before);
}
#[test]
fn a_cell_must_contain_each_actual_native_glyph() {
    let mut f = fixture();
    let Unit::Plain(lines) = &mut f.units[0] else {
        panic!()
    };
    lines[0].table.as_mut().unwrap().cells[0].x1 = 80.;
    let before = snapshot(&f.units);
    apply(&mut f);
    assert_eq!(snapshot(&f.units), before);
}
#[test]
fn repeated_word_inventory_is_not_a_second_source_occurrence() {
    let mut f = fixture();
    let Unit::Plain(lines) = &mut f.units[0] else {
        panic!()
    };
    lines[0].table.as_mut().unwrap().cells[0].words.push(0);
    let before = snapshot(&f.units);
    apply(&mut f);
    assert_eq!(snapshot(&f.units), before);
}
#[test]
fn arbitrary_existing_nonwhitespace_material_is_never_replaced() {
    let mut f = fixture();
    let Unit::Plain(lines) = &mut f.units[0] else {
        panic!()
    };
    lines[0].table.as_mut().unwrap().cells[0].text.push('外');
    let before = snapshot(&f.units);
    apply(&mut f);
    assert_eq!(snapshot(&f.units), before);
}

fn snapshot(units: &[Unit]) -> Vec<(String, Vec<Line>)> {
    let mut out = vec![];
    for u in units {
        match u {
            Unit::Plain(l) => out.push(("plain".into(), l.clone())),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            ..} => {
                out.push(("preamble".into(), preamble.clone()));
                for (role, parts) in [("after", after), ("before", before), ("outside", outside)] {
                    for l in parts {
                        out.push((role.into(), l.clone()));
                    }
                }
            }
        }
    }
    out
}
fn evidence(
    w: &Word,
    baseline: f64,
    prefix: f64,
    trailing: f64,
    first: usize,
    after: usize,
    total: usize,
    mode: &str,
    axis: &str,
) -> Option<crate::native_cell_origin::Origin> {
    let raw=format!("<word xmin='{}' ymin='{}' xmax='{}' ymax='{}' data-native-xmin='{}' data-native-ymin='{}' data-native-xmax='{}' data-native-ymax='{}' data-native-axis='{axis}' data-wmode='{mode}' data-native-flow-start='{}' data-native-flow-end='{}' data-native-flow-baseline='{baseline}' data-native-flow-leading='4' data-native-flow-trailing='{trailing}' data-native-flow-prefix='{prefix}' data-native-flow-first-glyph='{first}' data-native-flow-after-glyph='{after}'>任意</word>",w.x0,w.y0,w.x1,w.y1,w.x0,w.y0,w.x1,w.y1,w.x0,w.x1);
    let raw = raw.replacen(" data-native-axis", &format!(" data-native-cell-origin='2 {first} {after} {} {baseline} {} 4 {trailing} {prefix} {} {} {} {} 8 8' data-native-axis", w.x0, w.x1, w.x0, w.y0, w.x1, w.y1), 1);
    let h = scraper::Html::parse_fragment(&raw);
    crate::native_cell_origin::from_word(
        h.select(&scraper::Selector::parse("word").unwrap())
            .next()
            .unwrap(),
        Some(total),
    )
}
fn unchanged(mut f: Fixture) {
    let before = snapshot(&f.units);
    apply(&mut f);
    assert_eq!(snapshot(&f.units), before);
}
#[test]
fn missing_original_paint_between_words_does_not_certify_one_cell_flow() {
    let mut f = fixture();
    f.flows[1] = evidence(&f.words[1], 46., 5., 0., 10, 14, 23, "0", "right");
    assert!(f.flows[1].is_some());
    unchanged(f);
}
#[test]
fn disagreeing_original_page_glyph_counts_do_not_share_a_cell_proof() {
    let mut f = fixture();
    f.flows[1] = evidence(&f.words[1], 46., 5., 0., 9, 14, 24, "0", "right");
    assert!(f.flows[1].is_some());
    unchanged(f);
}
#[test]
fn source_prefix_must_account_for_the_actual_gap() {
    let mut f = fixture();
    f.flows[1] = evidence(&f.words[1], 46., 4., 0., 9, 14, 23, "0", "right");
    assert!(f.flows[1].is_some());
    unchanged(f);
}
#[test]
fn rightward_vertical_mode_is_not_horizontal_cell_evidence() {
    let mut f = fixture();
    f.flows[1] = evidence(&f.words[1], 46., 5., 0., 9, 14, 23, "1", "right");
    assert!(f.flows[1].is_none());
    unchanged(f);
}
#[test]
fn compatibility_rectangles_do_not_supply_original_glyphs() {
    let mut f = fixture();
    f.bounds[0] = NativeBounds::Compatibility;
    unchanged(f);
}
#[test]
fn source_occurrences_used_in_another_line_do_not_move_into_a_table() {
    let mut f = fixture();
    let Unit::Plain(lines) = &mut f.units[0] else {
        panic!()
    };
    lines.push(Line::text("arbitrary".into(), vec![0]));
    unchanged(f);
}
#[test]
fn stale_table_rendering_cannot_silently_change_its_surrounding_line() {
    let mut f = fixture();
    let Unit::Plain(lines) = &mut f.units[0] else {
        panic!()
    };
    lines[0].text = "別の出典の本文".into();
    unchanged(f);
}
#[test]
fn nonfinite_cell_geometry_is_unknown() {
    let mut f = fixture();
    let Unit::Plain(lines) = &mut f.units[0] else {
        panic!()
    };
    lines[0].table.as_mut().unwrap().cells[0].x0 = f32::NAN;
    let before = format!("{:?}", snapshot(&f.units));
    apply(&mut f);
    assert_eq!(format!("{:?}", snapshot(&f.units)), before);
}
#[test]
fn original_word_record_and_inventory_are_immutable() {
    let mut f = fixture();
    let words = serde_json::to_string(&f.words).unwrap();
    let inventory = cell(&f).words.clone();
    apply(&mut f);
    assert_eq!(serde_json::to_string(&f.words).unwrap(), words);
    assert_eq!(cell(&f).words, inventory);
    assert_eq!(cell(&f).text, "arbitrary labelcontinued");
}
#[test]
fn repair_is_idempotent() {
    let mut f = fixture();
    apply(&mut f);
    let once = snapshot(&f.units);
    apply(&mut f);
    assert_eq!(snapshot(&f.units), once);
    assert_eq!(cell(&f).text, "arbitrary labelcontinued");
}
#[test]
fn glyph_advances_with_zero_gap_do_not_invent_word_spaces() {
    let mut f = fixture();
    f.words[1].x0 = 65.;
    f.words[1].source_box[0] = 65.;
    f.bounds[1] = NativeBounds::Glyphs([65., 40., 95., 48.]);
    f.flows[1] = evidence(&f.words[1], 46., 0., 0., 9, 14, 23, "0", "right");
    apply(&mut f);
    assert_eq!(cell(&f).text, "arbitrarylabelcontinued");
}
#[test]
fn a_source_proven_gap_has_no_font_fraction_or_minimum_point_cutoff() {
    let mut f = fixture();
    f.words[1].x0 = 65.001;
    f.words[1].source_box[0] = 65.001;
    f.bounds[1] = NativeBounds::Glyphs([65.001, 40., 95., 48.]);
    f.flows[1] = evidence(&f.words[1], 46., 0.001, 0., 9, 14, 23, "0", "right");
    apply(&mut f);
    assert_eq!(cell(&f).text, "arbitrary labelcontinued");
}
#[test]
fn the_full_punctuation_envelope_must_fit_even_when_the_legacy_box_fits() {
    let mut f = fixture();
    let Unit::Plain(lines) = &mut f.units[0] else {
        panic!()
    };
    let cell = &mut lines[0].table.as_mut().unwrap().cells[0];
    cell.y0 = 39.9;
    f.flows[0].as_mut().unwrap().bounds[1] = 39.84;
    unchanged(f);
}

#[test]
fn source_line_wrapping_is_not_evidence_of_missing_word_spaces() {
    for (old, native) in [
        ("販売費及び一般管理費", "販売費及び一般管理\n費"),
        ("ホルムアルデヒド", "ホルムアルデ\nヒド"),
        ("ｱﾙﾌｧﾍﾞｰﾀ", "ｱﾙﾌｧ\nﾍﾞｰﾀ"),
        ("arbitrary", "arbi\ntrary"),
        ("123456", "123\n456"),
    ] {
        assert_eq!(source_gaps(old, native).as_deref(), Some(old));
    }
}
#[test]
fn native_spacing_does_not_split_ideographic_or_syllabic_text() {
    for (old, native) in [
        ("任意の項目", "任 意 の項目"),
        ("インターファーム", "インター ファーム"),
        ("ｱﾙﾌｧﾍﾞｰﾀ", "ｱﾙﾌｧ ﾍﾞｰﾀ"),
        ("한글문자", "한글 문자"),
    ] {
        assert_eq!(source_gaps(old, native).as_deref(), Some(old));
    }
}
#[test]
fn original_native_same_baseline_gaps_separate_alphabetic_or_numeric_text() {
    for (old, native) in [
        ("randomtokens", "random tokens"),
        ("αβγδεζ", "αβγ δεζ"),
        ("абвгде", "абв где"),
        ("אבגד", "אב גד"),
        ("123456", "123 456"),
    ] {
        assert_eq!(source_gaps(old, native).as_deref(), Some(native));
    }
}
#[test]
fn original_cell_whitespace_is_preserved_verbatim() {
    assert_eq!(
        source_gaps("first\nsecond\tthird", "first second\nthird").as_deref(),
        Some("first\nsecond\tthird")
    );
}
#[test]
fn same_letters_in_different_order_never_certify_whitespace_repair() {
    assert!(source_gaps("alphabeta", "beta alpha").is_none());
    assert!(source_gaps("甲乙", "乙 甲").is_none());
}

#[test]
fn existing_combining_sequences_keep_their_source_base_property() {
    assert_eq!(
        source_gaps("e\u{301}word", "e\u{301} word").as_deref(),
        Some("e\u{301} word")
    );
    assert_eq!(
        source_gaps("e\u{301}word", "e \u{301}word").as_deref(),
        Some("e\u{301}word")
    );
}
#[test]
fn source_alphabetic_symbol_gaps_are_not_tied_to_sample_names() {
    assert_eq!(
        source_gaps("one&two", "one & two").as_deref(),
        Some("one & two")
    );
    assert_eq!(
        source_gaps("other&value", "other & value").as_deref(),
        Some("other & value")
    );
}

#[test]
fn a_font_size_change_does_not_prove_a_word_separator() {
    let mut f = fixture();
    f.flows[0].as_mut().unwrap().sizes[1] = 16.;
    apply(&mut f);
    assert_eq!(cell(&f).text, "arbitrarylabelcontinued");
}
#[test]
fn equal_original_endpoint_sizes_do_not_depend_on_their_absolute_scale() {
    for size in [0.125, 8., 128.] {
        let mut f = fixture();
        for flow in &mut f.flows {
            flow.as_mut().unwrap().sizes = [size, size];
        }
        apply(&mut f);
        assert_eq!(cell(&f).text, "arbitrary labelcontinued");
    }
}
