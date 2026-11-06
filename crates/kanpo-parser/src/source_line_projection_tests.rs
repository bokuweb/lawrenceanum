use super::*;
fn word(id: usize, text: &str) -> Word {
    Word {
        id,
        raw: text.into(),
        text: text.into(),
        source_box: [0.; 4],
        x0: 0.,
        y0: 0.,
        x1: 8.,
        y1: 16.,
        status: WordStatus::Used,
    }
}
fn col(id: usize, x: f32, y: f32, text: &str) -> Col {
    Col {
        id,
        x,
        x1: x + 8.,
        y,
        y1: y + 8. * text.chars().count() as f32,
        text: text.into(),
    }
}

#[test]
fn actual_prefix_slices_keep_non_bmp_scalars_and_the_remaining_source_suffix() {
    let words = vec![word(0, "甲😀"), word(1, "乙 丙丁")];
    let cols = vec![
        col(0, 16., 0., &words[0].text),
        col(1, 8., 0., &words[1].text),
    ];
    let mut capture = Capture::default();
    capture.seed(&cols, &words);
    // The first two source characters include whitespace. The renderer's
    // left part drops it, and the remaining original suffix stays separate.
    capture.two(&cols, &[0], &[1], &cols[0], "乙", true, "甲😀・乙");
    assert_eq!(value(&capture.entries[&0]), "甲😀・乙");
    assert_eq!(value(&capture.entries[&1]), "丙丁");
    assert_eq!(
        capture.entries[&0][1].origin,
        Origin::Word { word: 0, scalar: 1 }
    );
    assert_eq!(
        capture.entries[&1][0].origin,
        Origin::Word { word: 1, scalar: 1 }
    );
    assert_eq!(
        capture.entries[&0][2].origin,
        Origin::ParallelJoin {
            before: vec![0],
            after: vec![1]
        }
    );
    capture.two(
        &cols,
        &[0],
        &[1],
        &cols[0],
        "foreign expected text",
        true,
        "甲😀・乙",
    );
    assert!(capture.entries.is_empty());
}

#[test]
fn a_three_part_observation_follows_the_actual_join_decisions() {
    let words = vec![word(0, "甲、"), word(1, "乙"), word(2, "丙😀")];
    let cols = vec![
        col(0, 24., 0., &words[0].text),
        col(1, 16., 0., &words[1].text),
        col(2, 8., 0., &words[2].text),
    ];
    let mut capture = Capture::default();
    capture.seed(&cols, &words);
    let parts = ["甲、".into(), "乙".into(), "丙😀".into()];
    capture.three(
        &cols,
        &[0],
        &[1],
        &[2],
        0,
        &parts,
        [false, false, true],
        "甲、乙・丙😀",
    );
    assert_eq!(capture.entries.len(), 1);
    assert_eq!(value(&capture.entries[&0]), "甲、乙・丙😀");
    assert_eq!(
        capture.entries[&0]
            .iter()
            .filter(|t| matches!(t.origin, Origin::ParallelJoin { .. }))
            .count(),
        1
    );
    assert_eq!(
        capture.entries[&0].last().unwrap().origin,
        Origin::Word { word: 2, scalar: 1 }
    );
}

#[test]
fn foreign_text_duplicate_owners_and_incomplete_tokens_cannot_seed_a_projection() {
    let words = vec![word(0, "同じ文言"), word(1, "同じ文言")];
    let mut cols = vec![
        col(0, 16., 0., &words[0].text),
        col(1, 8., 0., &words[1].text),
    ];
    let mut capture = Capture::default();
    cols.push(cols[0].clone());
    capture.seed(&cols, &words);
    assert!(!capture.entries.contains_key(&0));
    assert!(capture.entries.contains_key(&1));
    cols[1].text.push('X');
    capture.seed(&cols, &words);
    assert!(capture.entries.is_empty());
}

#[test]
fn original_late_word_normalization_and_row_order_require_complete_source_identity() {
    let pdf = include_bytes!("../tests/data/workbook/case-17.pdf");
    let sources = kanpo_pdf::extract_pages_with_aj1_sources(pdf).unwrap();
    let inputs = crate::page_inputs_from_sources(&sources);
    let native = crate::analyze_item(&inputs);
    for (pi, ri, li) in [(0, 0, 0), (1, 0, 73), (5, 0, 43)] {
        let doc = scraper::Html::parse_document(&inputs[pi].xhtml);
        let node = doc
            .select(&scraper::Selector::parse("page").unwrap())
            .next()
            .unwrap();
        let projection = Page::new(
            node,
            sources[pi].page().width as f32,
            sources[pi].page().height as f32,
            &inputs[pi],
            &sources[pi],
        );
        let line = &native.pages[pi].regions[ri].lines[li];
        let expected = line
            .text
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>();
        assert_eq!(
            value(&projection.line(line, &native.pages[pi]).unwrap()),
            expected
        );
        let mut changed = line.clone();
        changed.text.push('X');
        assert!(projection.line(&changed, &native.pages[pi]).is_none());
        let mut changed = line.clone();
        changed.words.push(line.words[0]);
        assert!(projection.line(&changed, &native.pages[pi]).is_none());
        for mutation in 0..3 {
            let mut changed = native.pages[pi].clone();
            let word = &mut changed.words[line.words[0]];
            match mutation {
                0 => word.raw.push('X'),
                1 => word.text.push('X'),
                _ => word.source_box[0] += 0.5,
            }
            assert!(projection.line(line, &changed).is_none());
        }
    }
}

#[test]
fn source_cell_prefixes_need_one_original_unowned_glyph_and_complete_cell_text() {
    let pdf = include_bytes!("../tests/data/workbook/case-17.pdf");
    let sources = kanpo_pdf::extract_pages_with_aj1_sources(pdf).unwrap();
    let inputs = crate::page_inputs_from_sources(&sources);
    let native = crate::analyze_item(&inputs);
    let trace = &native.pages[4];
    let doc = scraper::Html::parse_document(&inputs[4].xhtml);
    let node = doc
        .select(&scraper::Selector::parse("page").unwrap())
        .next()
        .unwrap();
    let mut projection = Page::new(
        node,
        sources[4].page().width as f32,
        sources[4].page().height as f32,
        &inputs[4],
        &sources[4],
    );
    let cell = &trace.regions[0].lines[3].table.as_ref().unwrap().cells[3];
    let tokens = projection.cell(cell, trace).unwrap();
    assert_eq!(value(&tokens), "△×");
    let Origin::RelocatedWord {
        word, target_word, ..
    } = tokens[0].origin
    else {
        panic!("missing original sign")
    };
    assert_eq!(word, 75);
    assert!(cell.words.contains(&target_word));
    assert_ne!(trace.words[word].status, WordStatus::Used);
    assert_eq!(projection.auxiliary_locations(trace).len(), 5);
    let mut changed = cell.clone();
    changed.text.push('X');
    assert!(projection.cell(&changed, trace).is_none());
    for mutation in 0..4 {
        let mut changed = trace.clone();
        let sign = &mut changed.words[word];
        match mutation {
            0 => sign.raw.push('X'),
            1 => sign.text.push('X'),
            2 => sign.source_box[0] += 0.5,
            _ => sign.status = WordStatus::Used,
        }
        assert!(projection.cell(cell, &changed).is_none());
    }
    let event = projection
        .capture
        .prefixes
        .iter()
        .find(|p| p.word == word)
        .unwrap()
        .clone();
    projection.capture.prefixes.push(event);
    assert!(projection.cell(cell, trace).is_none());
}

#[test]
fn original_prefix_captures_keep_all_eight_unique_source_signs() {
    let pdf = include_bytes!("../tests/data/workbook/case-17.pdf");
    let sources = kanpo_pdf::extract_pages_with_aj1_sources(pdf).unwrap();
    let inputs = crate::page_inputs_from_sources(&sources);
    let native = crate::analyze_item(&inputs);
    for pi in [4, 9] {
        let doc = scraper::Html::parse_document(&inputs[pi].xhtml);
        let node = doc
            .select(&scraper::Selector::parse("page").unwrap())
            .next()
            .unwrap();
        let projection = Page::new(
            node,
            sources[pi].page().width as f32,
            sources[pi].page().height as f32,
            &inputs[pi],
            &sources[pi],
        );
        let expected = if pi == 4 { 5 } else { 3 };
        assert_eq!(projection.capture.prefixes.len(), expected);
        assert_eq!(
            projection.auxiliary_locations(&native.pages[pi]).len(),
            expected
        );
        for r in &native.pages[pi].regions {
            for l in &r.lines {
                if let Some(table) = &l.table {
                    for (ci, c) in table.cells.iter().enumerate() {
                        if c.text.contains('△') {
                            assert_eq!(
                                projection.cell(c, &native.pages[pi]).map(|t| value(&t)),
                                Some(
                                    c.text
                                        .chars()
                                        .filter(|c| !c.is_whitespace())
                                        .collect::<String>()
                                ),
                                "cell {ci}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn physical_header_order_requires_one_baseline_and_disjoint_original_advances() {
    let pdf = include_bytes!("../tests/data/workbook/case-17.pdf");
    let sources = kanpo_pdf::extract_pages_with_aj1_sources(pdf).unwrap();
    let inputs = crate::page_inputs_from_sources(&sources);
    let native = crate::analyze_item(&inputs);
    let doc = scraper::Html::parse_document(&inputs[5].xhtml);
    let node = doc
        .select(&scraper::Selector::parse("page").unwrap())
        .next()
        .unwrap();
    let mut projection = Page::new(
        node,
        sources[5].page().width as f32,
        sources[5].page().height as f32,
        &inputs[5],
        &sources[5],
    );
    let trace = &native.pages[5];
    let cell = &trace.regions[0].lines[10].table.as_ref().unwrap().cells[0];
    assert_eq!(
        projection.cell(cell, trace).map(|t| value(&t)),
        Some("金額".into())
    );
    let first = cell.words[0];
    let second = cell.words[1];
    let original = projection.geometry[&first];
    projection.geometry.get_mut(&first).unwrap().1 += 1.;
    assert!(projection.cell(cell, trace).is_none());
    projection.geometry.insert(first, original);
    projection.geometry.get_mut(&first).unwrap().2 = projection.geometry[&second].2;
    assert!(projection.cell(cell, trace).is_none());
}
