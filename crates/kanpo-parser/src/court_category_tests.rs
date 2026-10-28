// Included by court_record_tests; all names and counts below are independent
// synthetic witnesses, not dispatch keys in the parser.
fn category_bounds(p: &mut PageTrace, id: usize, b: Rect) {
    let w = &mut p.words[id];
    w.source_box = b;
    w.x0 = b[0];
    w.y0 = b[1];
    w.x1 = b[2];
    w.y1 = b[3];
    p.native_bounds[id] = NativeBounds::Glyphs(b);
}
fn category_style(p: &mut PageTrace, id: usize, font: usize, size: f64) {
    let s = BTreeSet::from([(font, size.to_bits())]);
    p.native_styles[id] = Some(s.clone());
    p.native_content_styles[id] = ContentStyles::Glyphs(s);
}
fn category_fixture(next_column: bool, titles: &[&str]) -> (PageTrace, PageRules) {
    let mut p = valid();
    let count = titles.len();
    let shift = 20. * count as f32;
    category_bounds(
        &mut p,
        5,
        if next_column {
            [560., 60. + shift, 720., 70. + shift]
        } else {
            [60., 140. + shift, 220., 150. + shift]
        },
    );
    category_bounds(
        &mut p,
        6,
        if next_column {
            [568., 80. + shift, 740., 90. + shift]
        } else {
            [68., 160. + shift, 240., 170. + shift]
        },
    );
    for (i, t) in titles.iter().enumerate() {
        let y = if next_column { 40. } else { 140. } + 20. * i as f32;
        let x = if next_column { 600. } else { 120. };
        let id = p.words.len();
        add(&mut p, t, [x, y, x + 100., y + 10.], 2);
        category_style(&mut p, id, 2, 9.);
        let l = p.regions[0].lines.pop().unwrap();
        p.regions[0].lines.insert(5 + i, l);
    }
    p.native_horizontal_baselines = p.words.iter().map(|w| Some((w.y1 - 1.) as f64)).collect();
    p.doc_text = p.regions[0]
        .lines
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    p.text = p.doc_text.clone();
    let mut r = PageRules::default();
    if next_column {
        p.width = 1000.;
        r.vertical = [0., 500., 1000.]
            .into_iter()
            .map(|x| crate::lines::VRule {
                x,
                y0: 0.,
                y1: 800.,
            })
            .collect();
    }
    (p, r)
}
#[test]
fn category_prefix_uses_typography_without_category_vocabulary_or_row_count() {
    for titles in [
        vec!["X"],
        vec!["任意の非常に長い区分名で文字数の上限を設けない"],
        vec!["任意", "の", "複数行", "見出し"],
    ] {
        for next in [false, true] {
            let (p, r) = category_fixture(next, &titles);
            assert_eq!(cuts(&p, &r), vec![5], "{titles:?} {next}");
        }
    }
}
#[test]
fn category_scope_is_invariant_under_source_font_renaming_and_affine_page_scaling() {
    for next in [false, true] {
        for scale in [0.125f32, 2., 16.] {
            let (mut p, mut r) = category_fixture(next, &["未登録の区分"]);
            for id in 0..p.words.len() {
                let b = p.words[id].source_box;
                category_bounds(
                    &mut p,
                    id,
                    [
                        b[0] * scale - 80.,
                        b[1] * scale + 30.,
                        b[2] * scale - 80.,
                        b[3] * scale + 30.,
                    ],
                );
                p.native_horizontal_baselines[id] =
                    p.native_horizontal_baselines[id].map(|v| v * scale as f64 + 30.);
                if let ContentStyles::Glyphs(s) = &p.native_content_styles[id] {
                    let s = s
                        .iter()
                        .map(|(f, v)| (f + 1234, (f64::from_bits(*v) * scale as f64).to_bits()))
                        .collect::<BTreeSet<_>>();
                    p.native_styles[id] = Some(s.clone());
                    p.native_content_styles[id] = ContentStyles::Glyphs(s);
                }
            }
            for v in &mut r.vertical {
                v.x = v.x * scale - 80.;
                v.y0 = v.y0 * scale + 30.;
                v.y1 = v.y1 * scale + 30.;
            }
            assert_eq!(cuts(&p, &r), vec![5], "{next} {scale}");
        }
    }
}
#[test]
fn a_printed_horizontal_separator_breaks_category_ownership_in_either_column() {
    for next in [false, true] {
        let (p, mut r) = category_fixture(next, &["任意区分"]);
        r.horizontal.push(crate::lines::HRule {
            y: if next { 65. } else { 155. },
            x0: if next { 500. } else { 0. },
            x1: if next { 1000. } else { 500. },
        });
        assert!(cuts(&p, &r).is_empty(), "next column {next}");
    }
}
#[test]
fn ordinary_prose_and_case_references_cannot_be_a_display_category() {
    for text in [
        "任意の文。",
        "任意の文.",
        "任意の文．",
        "令和九年（フ）第3号",
    ] {
        let (p, r) = category_fixture(false, &[text]);
        assert!(cuts(&p, &r).is_empty(), "{text}");
    }
    for (font, size) in [(0, 9.), (2, 8.), (2, 0.), (2, f64::NAN), (2, f64::INFINITY)] {
        let (mut p, r) = category_fixture(false, &["任意"]);
        category_style(&mut p, 7, font, size);
        assert!(cuts(&p, &r).is_empty(), "{font} {size}");
    }
}
#[test]
fn category_cannot_borrow_unknown_direction_geometry_styles_or_baselines() {
    for next in [false, true] {
        for id in [4, 5, 6, 7] {
            let (mut p, r) = category_fixture(next, &["任意"]);
            p.native_horizontal_baselines[id] = None;
            assert!(cuts(&p, &r).is_empty(), "baseline {id} {next}");
            let (mut p, r) = category_fixture(next, &["任意"]);
            p.native_bounds[id] = NativeBounds::Unknown;
            assert!(cuts(&p, &r).is_empty());
            let (mut p, r) = category_fixture(next, &["任意"]);
            p.native_axes[id] = NativeAxis::Down;
            assert!(cuts(&p, &r).is_empty());
            let (mut p, r) = category_fixture(next, &["任意"]);
            p.native_content_styles[id] = ContentStyles::Unknown;
            if id == 6 {
                // The ordinary body has complete original glyph fonts and an
                // exact horizontal baseline. Header/caption/signature semantics
                // still require known character-class evidence.
                assert_eq!(cuts(&p, &r), vec![5]);
                p.native_styles[id] = None;
            }
            assert!(cuts(&p, &r).is_empty());
        }
    }
}
#[test]
fn category_is_not_a_boundary_inside_quotes_enclosures_or_nonbody_scopes() {
    for next in [false, true] {
        let (mut p, r) = category_fixture(next, &["任意"]);
        p.words[3].text = "引用「".into();
        p.regions[0].lines[3].text = "引用「".into();
        assert!(cuts(&p, &r).is_empty());
        let (mut p, r) = category_fixture(next, &["任意"]);
        p.regions[0].role = Role::After;
        assert!(cuts(&p, &r).is_empty());
        let (p, mut r) = category_fixture(next, &["任意"]);
        r.enclosures.push(p.words[7].source_box);
        assert!(cuts(&p, &r).is_empty());
    }
}
#[test]
fn category_column_transition_requires_adjacent_complete_native_column_walls() {
    for remove in 0..3 {
        let (p, mut r) = category_fixture(true, &["任意"]);
        r.vertical.remove(remove);
        assert!(cuts(&p, &r).is_empty());
    }
    let (p, mut r) = category_fixture(true, &["任意"]);
    r.vertical[1].y1 = 120.;
    assert!(cuts(&p, &r).is_empty());
    let (p, mut r) = category_fixture(true, &["任意"]);
    r.vertical.push(crate::lines::VRule {
        x: 520.,
        y0: 0.,
        y1: 800.,
    });
    assert!(cuts(&p, &r).is_empty());
}
#[test]
fn category_transition_requires_the_last_old_and_first_new_native_content() {
    for b in [
        [60., 135., 150., 145.],
        [550., 20., 600., 30.],
        [490., 20., 510., 30.],
        [620., 41., 650., 46.],
    ] {
        let (mut p, r) = category_fixture(true, &["任意"]);
        add(&mut p, "別の原文", b, 0);
        p.native_horizontal_baselines.push(Some((b[3] - 1.) as f64));
        assert!(cuts(&p, &r).is_empty(), "{b:?}");
    }
}
#[test]
fn a_category_cannot_take_a_footer_from_an_unproved_source_region() {
    let (mut p, r) = category_fixture(false, &["任意"]);
    let lines = p.regions[0].lines.split_off(5);
    p.regions.push(Region {
        role: Role::Body,
        unit: 1,
        part: 0,
        lines,
    });
    assert!(cuts(&p, &r).is_empty());
}
#[test]
fn native_baselines_are_private_provenance_not_public_analysis_fields() {
    let (p, _) = category_fixture(true, &["任意"]);
    let value = serde_json::to_value(&p).unwrap();
    assert!(value.get("native_horizontal_baselines").is_none());
}
#[test]
fn a_typed_table_cannot_supply_the_display_category() {
    let (mut p, r) = category_fixture(false, &["任意"]);
    let l = &mut p.regions[0].lines[5];
    let b = p.words[7].source_box;
    l.words.clear();
    l.table = Some(crate::trace::Table {
        x0: b[0],
        y0: b[1],
        x1: b[2],
        y1: b[3],
        rows: 1,
        cols: 1,
        cells: vec![crate::trace::TableCell {
            row: 0,
            col: 0,
            rowspan: 1,
            colspan: 1,
            x0: b[0],
            y0: b[1],
            x1: b[2],
            y1: b[3],
            text: l.text.clone(),
            words: vec![7],
        }],
    });
    assert!(cuts(&p, &r).is_empty());
}
