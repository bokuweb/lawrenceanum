use kanpo_parser::media::{analyze_pdf, Document, Field};

#[test]
fn original_boxed_zeroes_are_text_with_source_frame_geometry_and_no_duplicate_square() {
    let pdf = include_bytes!("data/workbook/case-04.pdf");
    let result = analyze_pdf(pdf).unwrap();
    assert_eq!(result.frames.bindings.len(), 2);
    assert!(result.frames.unresolved.is_empty());
    let sources = kanpo_pdf::extract_pages_with_xobject_sources(pdf).unwrap();
    for (field, frame, parent) in [(Field::After, 834, 835), (Field::Before, 1283, 1284)] {
        let binding = result
            .frames
            .bindings
            .iter()
            .find(|b| b.field == field)
            .unwrap();
        assert_eq!(binding.page, 0);
        assert_eq!(binding.frame_glyph, frame);
        assert_eq!(binding.parent_glyphs, vec![parent]);
        assert_eq!(
            binding.frame,
            sources[0]
                .native_sources()
                .glyph_rectangle_frame(frame)
                .unwrap()
        );
        assert_eq!(binding.text, "０");
        assert_eq!(binding.removed_text, "□");
        assert_eq!(binding.removed_length, 1);
        assert_eq!(
            result.documents[binding.document]
                .field_text(field)
                .chars()
                .skip(binding.offset)
                .take(binding.length)
                .collect::<String>(),
            "０"
        );
    }
    let native = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    assert_eq!(
        serde_json::to_value(&result.native).unwrap(),
        serde_json::to_value(&native).unwrap()
    );
    for field in [Field::Paragraph, Field::After, Field::Before] {
        let mut expected = kanpo_parser::source_underlines::field_text(&native, field)
            .chars()
            .collect::<Vec<_>>();
        let mut removals = result
            .frames
            .bindings
            .iter()
            .filter(|b| b.field == field)
            .collect::<Vec<_>>();
        removals.sort_by_key(|b| std::cmp::Reverse(b.removed_offset));
        for b in removals {
            assert_eq!(
                expected[b.removed_offset..b.removed_offset + b.removed_length]
                    .iter()
                    .collect::<String>(),
                b.removed_text
            );
            expected.drain(b.removed_offset..b.removed_offset + b.removed_length);
        }
        assert_eq!(
            result
                .documents
                .iter()
                .map(|d| d.field_text(field))
                .collect::<String>(),
            expected.iter().collect::<String>()
        );
    }
    assert_eq!(result.underlines.parallel_bindings.len(), 18);
    assert!(result.underlines.unresolved_parallel.is_empty());
    for b in &result.underlines.parallel_bindings {
        assert_eq!(
            result.documents[b.document]
                .field_text(b.field)
                .chars()
                .skip(b.offset)
                .take(b.length)
                .collect::<String>(),
            native.pages[b.source_word.page].words[b.source_word.word].text
        );
    }
    let original = native
        .articles
        .iter()
        .map(|a| Document::from(&a.document))
        .collect::<Vec<_>>();
    fn table_shapes(value: &serde_json::Value, out: &mut Vec<serde_json::Value>) {
        match value {
            serde_json::Value::Object(m) => {
                if let (Some(rows), Some(merges)) =
                    (m.get("rows").and_then(|r| r.as_array()), m.get("merges"))
                {
                    out.push(serde_json::json!({"columns":rows.iter().map(|r|r.as_array().unwrap().len()).collect::<Vec<_>>(),"merges":merges}));
                }
                for v in m.values() {
                    table_shapes(v, out);
                }
            }
            serde_json::Value::Array(values) => {
                for v in values {
                    table_shapes(v, out);
                }
            }
            _ => {}
        }
    }
    let mut before = Vec::new();
    let mut after = Vec::new();
    table_shapes(&serde_json::to_value(original).unwrap(), &mut before);
    table_shapes(
        &serde_json::to_value(&result.documents).unwrap(),
        &mut after,
    );
    assert_eq!(before, after);
}
