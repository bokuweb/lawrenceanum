fn verify_units(pdf: &[u8], expected: &str, source: &str, count: usize) {
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    assert_eq!(a.text.trim_end(), expected.trim_end());
    let proof: Vec<serde_json::Value> = serde_json::from_str(source).unwrap();
    assert_eq!(proof.len(), count);
    let p = &a.pages[0];
    for s in proof {
        let id = s["word_id"].as_u64().unwrap() as usize;
        let w = &p.words[id];
        assert_eq!(w.id, id);
        assert_eq!(w.raw, s["raw"].as_str().unwrap());
        assert_eq!(w.text, s["after_text"].as_str().unwrap());
        assert_eq!(w.status, kanpo_parser::trace::WordStatus::Used);
        for (v, n) in [w.x0, w.y0, w.x1, w.y1]
            .iter()
            .zip(s["word_bbox"].as_array().unwrap())
        {
            assert!((*v as f64 - n.as_f64().unwrap()).abs() < 0.001);
        }
        let mut owners = 0;
        for r in &p.regions {
            for l in &r.lines {
                if l.words.contains(&id) {
                    owners += 1;
                    assert_eq!(
                        serde_json::to_value(&l.words).unwrap(),
                        s["owner_source_ids"]
                    );
                    assert_eq!(l.text, s["owner_text_after"].as_str().unwrap());
                }
                if let Some(t) = &l.table {
                    for c in &t.cells {
                        if c.words.contains(&id) {
                            owners += 1;
                            assert_eq!(
                                serde_json::to_value(&c.words).unwrap(),
                                s["owner_source_ids"]
                            );
                            assert_eq!(c.text, s["owner_text_after"].as_str().unwrap());
                            assert_eq!(c.row, s["cell_row"].as_u64().unwrap() as usize);
                            assert_eq!(c.col, s["cell_col"].as_u64().unwrap() as usize);
                        }
                    }
                }
            }
        }
        assert_eq!(owners, 1, "unit source Word {id}");
    }
}
#[test]
fn dormitory_area_restores_square_metres_inside_the_existing_cell() {
    verify_units(
        include_bytes!("archive/pages/20260730g00171/20260730g001710130.pdf"),
        include_str!("archive/expected-area-units/20260730g001710130.txt"),
        include_str!("archive/expected-area-units/20260730g001710130-source.json"),
        1,
    );
}
#[test]
fn prose_and_calculation_keep_their_original_unit_word_ids() {
    verify_units(
        include_bytes!("archive/pages/20260817g00183/20260817g001830033.pdf"),
        include_str!("archive/expected-area-units/20260817g001830033.txt"),
        include_str!("archive/expected-area-units/20260817g001830033-source.json"),
        2,
    );
}
#[test]
fn both_compared_form_area_labels_keep_their_own_square_metre_symbols() {
    verify_units(
        include_bytes!("archive/pages/20260731g00172/20260731g001720119.pdf"),
        include_str!("archive/expected-area-units/20260731g001720119.txt"),
        include_str!("archive/expected-area-units/20260731g001720119-source.json"),
        2,
    );
}
#[test]
fn unowned_empty_form_units_are_not_attached_by_guessing() {
    let a = kanpo_parser::analyze_item(
        &kanpo_parser::page_inputs(include_bytes!(
            "archive/pages/20260731g00172/20260731g001720117.pdf"
        ))
        .unwrap(),
    );
    assert_eq!(
        a.text.trim_end(),
        include_str!("archive/expected-area-units/20260731g001720117.txt").trim_end()
    );
    let units: Vec<_> = a.pages[0]
        .words
        .iter()
        .filter(|w| w.raw == "\u{E68D}")
        .collect();
    assert_eq!(units.len(), 2);
    assert!(units
        .iter()
        .all(|w| w.text.is_empty() && w.status == kanpo_parser::trace::WordStatus::Empty));
}
#[test]
fn neighboring_square_kilometre_glyph_does_not_become_square_metres() {
    let a = kanpo_parser::analyze_item(
        &kanpo_parser::page_inputs(include_bytes!(
            "archive/pages/20260803g00174/20260803g001740029.pdf"
        ))
        .unwrap(),
    );
    assert_eq!(
        a.text.trim_end(),
        include_str!("archive/expected-area-units/20260803g001740029.txt").trim_end()
    );
    assert!(a.pages[0].words.iter().any(|w| w.raw.contains('\u{E68E}')));
}
#[test]
fn normal_unicode_square_metres_are_unchanged() {
    let a = kanpo_parser::analyze_item(
        &kanpo_parser::page_inputs(include_bytes!(
            "archive/pages/20260909g00201/20260909g002010013.pdf"
        ))
        .unwrap(),
    );
    let expected = include_str!("archive/expected-area-units/20260909g002010013.txt");
    assert_eq!(a.text.trim_end(), expected.trim_end());
    assert!(expected.contains('㎡'));
}
#[test]
fn existing_square_centimetre_mapping_and_nearby_numbers_are_unchanged() {
    let a = kanpo_parser::analyze_item(
        &kanpo_parser::page_inputs(include_bytes!(
            "archive/pages/20260831g00193/20260831g001930165.pdf"
        ))
        .unwrap(),
    );
    let expected = include_str!("archive/expected-area-units/20260831g001930165.txt");
    assert_eq!(a.text.trim_end(), expected.trim_end());
    assert!(expected.contains('㎠'));
}
