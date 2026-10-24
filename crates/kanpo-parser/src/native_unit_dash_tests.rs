use super::*;

struct Fixture {
    words: Vec<Word>,
    bounds: Vec<NativeBounds>,
    axes: Vec<NativeAxis>,
    styles: Vec<Option<NativeStyles>>,
    units: Vec<Unit>,
    rules: PageRules,
}
fn fixture(unit: &str, number: &str, scale: f32, dx: f32, dy: f32) -> Fixture {
    let right = 50. + unit.chars().count() as f32 * 8.;
    let boxes = [
        [50., 80., right, 88.],
        [36., 80., 44., 88.],
        [50. - number.chars().count() as f32 * 4., 20., right, 28.],
        [10., 140., 25., 148.],
    ];
    let mut words = Vec::new();
    let mut bounds = Vec::new();
    for (id, (raw, text)) in [
        (unit.to_string(), unit.to_string()),
        ("\u{E61C}".into(), "".into()),
        (format!("{number}{unit}"), format!("{number}{unit}")),
        ("prefix".into(), "prefix".into()),
    ]
    .into_iter()
    .enumerate()
    {
        let b = boxes[id];
        let b = [
            b[0] * scale + dx,
            b[1] * scale + dy,
            b[2] * scale + dx,
            b[3] * scale + dy,
        ];
        bounds.push(NativeBounds::Glyphs(b));
        words.push(Word {
            id,
            x0: b[0],
            y0: b[1],
            x1: b[2],
            y1: b[3],
            source_box: b,
            raw,
            text,
            status: if id == 1 {
                WordStatus::Empty
            } else {
                WordStatus::Used
            },
        });
    }
    Fixture {
        words,
        bounds,
        axes: vec![NativeAxis::Right; 4],
        styles: vec![
            Some(
                [(0, (8_f64 * scale as f64).to_bits())]
                    .into_iter()
                    .collect()
            );
            4
        ],
        units: vec![Unit::Plain(vec![
            Line::text("unchanged prefix".into(), vec![3]),
            Line::text(format!("{number}{unit}"), vec![2]),
            Line::text(unit.into(), vec![0]),
        ])],
        rules: PageRules::default(),
    }
}
fn run(f: &mut Fixture) {
    restore(
        &mut f.units,
        &mut f.words,
        &f.bounds,
        &f.axes,
        &f.styles,
        Some(&f.rules),
    );
}
fn lines(f: &Fixture) -> &[Line] {
    let Unit::Plain(lines) = &f.units[0] else {
        panic!()
    };
    lines
}
fn held(f: &mut Fixture) {
    let before: Vec<_> = lines(f).to_vec();
    let words = serde_json::to_string(&f.words).unwrap();
    run(f);
    assert_eq!(lines(f), before);
    assert_eq!(serde_json::to_string(&f.words).unwrap(), words);
}

#[test]
fn source_derived_units_magnitudes_scales_and_translation_no_fixed_row_or_text_limits() {
    for unit in ["kg", "mL", "人", "千円", "arbitraryUnitSuffix"] {
        for number in ["1,234", "1234567890", "−1,234", "０１２３４５６７８９"] {
            for scale in [0.125, 1., 8.] {
                let mut f = fixture(unit, number, scale, -37., 91.);
                let prefix = lines(&f)[0].clone();
                run(&mut f);
                assert_eq!(
                    lines(&f)[2].text,
                    format!("－{unit}"),
                    "{unit}/{number}/{scale}"
                );
                assert_eq!(lines(&f)[2].words, vec![1, 0]);
                assert_eq!(lines(&f)[0], prefix);
                let first = lines(&f).to_vec();
                run(&mut f);
                assert_eq!(lines(&f), first);
            }
        }
    }
}
#[test]
fn unknown_axes_bounds_font_or_rules_do_not_authorize_recovery() {
    for mode in 0..7 {
        let mut f = fixture("kg", "1,234", 1., 0., 0.);
        match mode {
            0 => f.axes[1] = NativeAxis::Unknown,
            1 => f.axes[0] = NativeAxis::Compatibility,
            2 => f.bounds[1] = NativeBounds::Unknown,
            3 => f.bounds[2] = NativeBounds::Compatibility,
            4 => f.styles[1] = None,
            5 => f.styles[0] = None,
            _ => f.styles[2] = None,
        };
        held(&mut f);
    }
    let mut f = fixture("kg", "1,234", 1., 0., 0.);
    restore(
        &mut f.units,
        &mut f.words,
        &f.bounds,
        &f.axes,
        &f.styles,
        None,
    );
    assert_eq!(f.words[1].status, WordStatus::Empty);
}
#[test]
fn different_numeric_punctuation_font_must_be_observed_not_guessed() {
    let mut f = fixture("kg", "1,234", 1., 0., 0.);
    f.styles[1] = Some([(1, 8_f64.to_bits())].into_iter().collect());
    held(&mut f);
    // A complete observed numeric tail can carry a distinct punctuation font.
    let mut f = fixture("kg", "234", 1., 0., 0.);
    f.styles[1] = Some([(1, 8_f64.to_bits())].into_iter().collect());
    let b = [20., 20., 38., 28.];
    f.words.push(Word {
        id: 4,
        x0: b[0],
        y0: b[1],
        x1: b[2],
        y1: b[3],
        source_box: b,
        raw: "1,".into(),
        text: "1,".into(),
        status: WordStatus::Used,
    });
    f.bounds.push(NativeBounds::Glyphs(b));
    f.axes.push(NativeAxis::Right);
    f.styles
        .push(Some([(1, 8_f64.to_bits())].into_iter().collect()));
    let Unit::Plain(ls) = &mut f.units[0] else {
        panic!()
    };
    ls[1].text = "1,234kg".into();
    ls[1].words = vec![4, 2];
    run(&mut f);
    assert_eq!(lines(&f)[2].words, vec![1, 0]);
}

#[test]
fn wrong_column_baseline_unit_and_unit_style_are_held() {
    for mode in 0..4 {
        let mut f = fixture("kg", "1,234", 1., 0., 0.);
        match mode {
            0 => {
                if let NativeBounds::Glyphs(b) = &mut f.bounds[2] {
                    b[2] += 1.;
                }
            }
            1 => {
                if let NativeBounds::Glyphs(b) = &mut f.bounds[1] {
                    b[1] += 1.;
                    b[3] += 1.;
                }
            }
            2 => {
                f.words[0].raw = "otherUnit".into();
                f.words[0].text = "otherUnit".into();
                let Unit::Plain(ls) = &mut f.units[0] else {
                    panic!()
                };
                ls[2].text = "otherUnit".into();
            }
            _ => f.styles[2] = Some([(1, 8_f64.to_bits())].into_iter().collect()),
        }
        held(&mut f);
    }
}
#[test]
fn no_original_mark_or_no_numeric_witness_never_fills_a_value() {
    let mut f = fixture("kg", "1,234", 1., 0., 0.);
    f.words[1].raw = "".into();
    held(&mut f);
    let mut f = fixture("kg", "1,234", 1., 0., 0.);
    let Unit::Plain(ls) = &mut f.units[0] else {
        panic!()
    };
    ls.remove(1);
    held(&mut f);
}
#[test]
fn duplicate_source_mark_or_ambiguous_owner_is_held() {
    let mut f = fixture("kg", "1,234", 1., 0., 0.);
    let mut w = f.words[1].clone();
    w.id = 4;
    f.words.push(w);
    f.bounds.push(f.bounds[1].clone());
    f.axes.push(NativeAxis::Right);
    f.styles.push(f.styles[1].clone());
    held(&mut f);
    let mut f = fixture("kg", "1,234", 1., 0., 0.);
    let Unit::Plain(ls) = &mut f.units[0] else {
        panic!()
    };
    ls.push(ls[2].clone());
    held(&mut f);
}
#[test]
fn any_intervening_source_occurrence_or_crossing_rule_prevents_joining() {
    let mut f = fixture("kg", "1,234", 1., 0., 0.);
    f.bounds[3] = NativeBounds::Glyphs([45., 80., 49., 88.]);
    held(&mut f);
    let mut f = fixture("kg", "1,234", 1., 0., 0.);
    f.rules.vertical.push(crate::lines::VRule {
        x: 47.,
        y0: 70.,
        y1: 100.,
    });
    held(&mut f);
    let mut f = fixture("kg", "1,234", 1., 0., 0.);
    f.rules.horizontal.push(crate::lines::HRule {
        y: 60.,
        x0: 0.,
        x1: 80.,
    });
    held(&mut f);
}
#[test]
fn a_changed_or_generated_unit_suffix_is_not_rebuilt_from_word_values() {
    let mut f = fixture("kg", "1,234", 1., 0., 0.);
    let Unit::Plain(ls) = &mut f.units[0] else {
        panic!()
    };
    ls[2].text = "corrected alternativeUnit".into();
    held(&mut f);
}

#[test]
fn an_unrelated_corrected_label_in_the_target_line_keeps_every_byte() {
    let mut f = fixture("kg", "1,234", 1., 0., 0.);
    f.bounds[3] = NativeBounds::Glyphs([10., 80., 25., 88.]);
    let Unit::Plain(ls) = &mut f.units[0] else {
        panic!()
    };
    ls[0] = Line::text("".into(), vec![]);
    ls[2] = Line::text("corrected prefix　kg  ".into(), vec![3, 0]);
    run(&mut f);
    assert_eq!(lines(&f)[2].text, "corrected prefix　－kg  ");
    assert_eq!(lines(&f)[2].words, vec![3, 1, 0]);
    assert_eq!(f.words[3].text, "prefix");
}

#[test]
fn an_unrelated_corrected_label_in_the_witness_cannot_hide_the_numeric_tail() {
    let mut f = fixture("kg", "1,234", 1., 0., 0.);
    f.bounds[3] = NativeBounds::Glyphs([10., 20., 25., 28.]);
    let Unit::Plain(ls) = &mut f.units[0] else {
        panic!()
    };
    ls[0] = Line::text("".into(), vec![]);
    ls[1] = Line::text("corrected prefix　1,234kg".into(), vec![3, 2]);
    let old = ls[1].clone();
    run(&mut f);
    assert_eq!(lines(&f)[2].text, "－kg");
    assert_eq!(lines(&f)[1], old);
}

#[test]
fn a_changed_numeric_tail_is_not_used_as_a_source_witness() {
    let mut f = fixture("kg", "1,234", 1., 0., 0.);
    let Unit::Plain(ls) = &mut f.units[0] else {
        panic!()
    };
    ls[1].text = "corrected 9,876kg".into();
    held(&mut f);
}

#[test]
fn one_original_source_dash_cannot_be_claimed_by_two_units() {
    let mut f = fixture("kg", "1,234", 1., 0., 0.);
    let before = lines(&f).to_vec();
    let mut unit = f.words[0].clone();
    unit.id = 4;
    f.words.push(unit);
    f.bounds.push(f.bounds[0].clone());
    f.axes.push(NativeAxis::Right);
    f.styles.push(f.styles[0].clone());
    let mut number = f.words[2].clone();
    number.id = 5;
    number.y0 += 20.;
    number.y1 += 20.;
    number.source_box = [number.x0, number.y0, number.x1, number.y1];
    f.bounds.push(NativeBounds::Glyphs(number.source_box));
    f.words.push(number);
    f.axes.push(NativeAxis::Right);
    f.styles.push(f.styles[2].clone());
    f.units.push(Unit::Plain(vec![
        Line::text("1,234kg".into(), vec![5]),
        Line::text("kg".into(), vec![4]),
    ]));
    run(&mut f);
    assert_eq!(
        f.words[1].status,
        WordStatus::Empty,
        "A single source occurrence cannot be inserted into two independently owned unit lines"
    );
    assert_eq!(lines(&f), before);
    let Unit::Plain(other) = &f.units[1] else {
        panic!()
    };
    assert_eq!(other[1].text, "kg");
}

#[test]
fn source_font_identity_bijection_and_source_word_emission_order_do_not_change_result() {
    let mut f = fixture("quantityUnit", "1,234", 1., 0., 0.);
    for styles in f.styles.iter_mut().flatten() {
        *styles = styles
            .iter()
            .map(|(font, size)| (font + 91, *size))
            .collect();
    }
    let permutation = [3, 1, 0, 2];
    let mut inverse = [0; 4];
    for (new, old) in permutation.iter().enumerate() {
        inverse[*old] = new;
    }
    f.words = permutation
        .iter()
        .enumerate()
        .map(|(new, &old)| {
            let mut w = f.words[old].clone();
            w.id = new;
            w
        })
        .collect();
    f.bounds = permutation
        .iter()
        .map(|&old| f.bounds[old].clone())
        .collect();
    f.axes = permutation.iter().map(|&old| f.axes[old]).collect();
    f.styles = permutation
        .iter()
        .map(|&old| f.styles[old].clone())
        .collect();
    let Unit::Plain(ls) = &mut f.units[0] else {
        panic!()
    };
    for l in ls {
        l.words = l.words.iter().map(|&old| inverse[old]).collect();
    }
    run(&mut f);
    assert_eq!(lines(&f)[2].words, vec![inverse[1], inverse[0]]);
    assert_eq!(lines(&f)[2].text, "－quantityUnit");
}

#[test]
fn nonfinite_or_nonpositive_recorded_font_size_and_rulings_are_held() {
    for size in [0., -8., f64::NAN, f64::INFINITY] {
        let mut f = fixture("kg", "1,234", 1., 0., 0.);
        for s in f.styles.iter_mut().flatten() {
            *s = [(0, size.to_bits())].into_iter().collect();
        }
        held(&mut f);
    }
    let mut f = fixture("kg", "1,234", 1., 0., 0.);
    f.rules.vertical.push(crate::lines::VRule {
        x: f32::NAN,
        y0: 0.,
        y1: 100.,
    });
    held(&mut f);
}

#[test]
fn a_foreign_occurrence_or_ruling_inside_the_numeric_witness_invalidates_it() {
    let mut f = fixture("kg", "1,234", 1., 0., 0.);
    f.bounds[3] = NativeBounds::Glyphs([34., 20., 40., 28.]);
    held(&mut f);
    let mut f = fixture("kg", "1,234", 1., 0., 0.);
    f.rules.vertical.push(crate::lines::VRule {
        x: 47.,
        y0: 10.,
        y1: 30.,
    });
    held(&mut f);
}

#[test]
fn existing_table_cells_keep_all_attributes_and_are_not_reconstructed() {
    let mut f = fixture("kg", "1,234", 1., 0., 0.);
    let table = crate::trace::Table {
        x0: 30.,
        y0: 80.,
        x1: 70.,
        y1: 88.,
        rows: 1,
        cols: 1,
        cells: vec![crate::trace::TableCell {
            row: 0,
            col: 0,
            rowspan: 1,
            colspan: 1,
            x0: 30.,
            y0: 80.,
            x1: 70.,
            y1: 88.,
            text: "kg".into(),
            words: vec![0],
        }],
    };
    let Unit::Plain(ls) = &mut f.units[0] else {
        panic!()
    };
    ls[2] = Line {
        text: "kg".into(),
        words: vec![],
        table: Some(table),
    };
    held(&mut f);
}

#[test]
fn a_previously_classified_comparison_keeps_its_original_lines() {
    let mut f = fixture("kg", "1,234", 1., 0., 0.);
    let Unit::Plain(ls) = f.units.remove(0) else {
        panic!()
    };
    let old = ls.clone();
    f.units.push(Unit::Shinkyu {
        preamble: vec![],
        after: vec![ls],
        before: vec![],
        outside: vec![],
    });
    run(&mut f);
    let Unit::Shinkyu { after, .. } = &f.units[0] else {
        panic!()
    };
    assert_eq!(after[0], old);
    assert_eq!(f.words[1].status, WordStatus::Empty);
}
