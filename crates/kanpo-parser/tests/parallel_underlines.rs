use kanpo_parser::media::{analyze_pdf, Field};

#[test]
fn original_double_underlines_keep_two_contours_and_exact_document_scalar_positions() {
    for (pdf, expected, exhaustive) in [
        (
            include_bytes!("data/workbook/case-04.pdf").as_slice(),
            vec![
                (Field::After, 0, 185, "カ"),
                (Field::After, 0, 187, "ヨ"),
                (Field::Before, 0, 82, "カ"),
            ],
            false,
        ),
        (
            include_bytes!("data/workbook/case-11.pdf").as_slice(),
            vec![
                (Field::After, 2, 31, "２"),
                (Field::After, 2, 39, "３"),
                (Field::After, 4, 23, "４"),
                (Field::Before, 2, 42, "２"),
                (Field::Before, 4, 20, "３"),
            ],
            true,
        ),
        (
            include_bytes!("data/workbook/case-16.pdf").as_slice(),
            vec![(Field::After, 10, 55, "二"), (Field::Before, 10, 57, "二")],
            true,
        ),
    ] {
        let result = analyze_pdf(pdf).unwrap();
        assert!(result.underlines.unresolved_parallel.is_empty());
        assert!(result.underlines.unresolved_parallel_words.is_empty());
        if exhaustive {
            assert_eq!(result.underlines.parallel_bindings.len(), expected.len());
        }
        let sources = kanpo_pdf::extract_pages_with_xobject_sources(pdf).unwrap();
        for (field, page, word, text) in expected {
            // Independently reviewed original PDF labels: two parallel bars,
            // rather than two neighbouring single-bar glyph segments.
            let found = result
                .underlines
                .parallel_bindings
                .iter()
                .filter(|b| {
                    b.field == field && b.source_word.page == page && b.source_word.word == word
                })
                .collect::<Vec<_>>();
            assert_eq!(found.len(), 1);
            let binding = found[0];
            assert_eq!(binding.parallel_strokes, 2);
            assert_eq!(
                result.documents[binding.document]
                    .field_text(field)
                    .chars()
                    .skip(binding.offset)
                    .take(binding.length)
                    .collect::<String>(),
                text
            );
            assert_eq!(
                binding.source_document_sha256,
                sources[page]
                    .document_sha256()
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            );
            let glyphs = result
                .underlines
                .glyph_bindings
                .iter()
                .filter(|b| b.page == page && b.word == word)
                .collect::<Vec<_>>();
            assert_eq!(glyphs.len(), 1);
            assert_eq!(glyphs[0].parallel_strokes, Some(2));
            assert_eq!(glyphs[0].decoration_glyphs.len(), 1);
            let source = glyphs[0].decoration_glyphs[0];
            let original = sources[page]
                .native_sources()
                .glyph_separate_painted_rectangles(source)
                .unwrap();
            assert_eq!(original.len(), 2);
            assert_eq!(
                glyphs[0]
                    .decoration_components
                    .iter()
                    .map(|p| (p.glyph, p.component, p.bounds))
                    .collect::<Vec<_>>(),
                vec![(source, 0, original[0]), (source, 1, original[1])]
            );
        }
        let native = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
        assert_eq!(
            serde_json::to_value(result.native).unwrap(),
            serde_json::to_value(native).unwrap()
        );
    }
}

#[test]
fn rectangular_contours_of_an_original_body_character_are_not_reused_as_decorations() {
    let pdf = include_bytes!("data/workbook/case-11.pdf");
    let sources = kanpo_pdf::extract_pages_with_xobject_sources(pdf).unwrap();
    assert_eq!(sources[0].page().glyphs[840].text, "三");
    assert_eq!(
        sources[0]
            .native_sources()
            .glyph_separate_painted_rectangles(840)
            .unwrap()
            .len(),
        3
    );
    let result = analyze_pdf(pdf).unwrap();
    assert!(!result
        .underlines
        .glyph_bindings
        .iter()
        .any(|b| b.page == 0 && b.decoration_glyphs.contains(&840)));
}
