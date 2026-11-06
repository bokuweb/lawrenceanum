use kanpo_parser::{
    media::{analyze_pdf, Field},
    source_baseline_shifts::Shift,
};

#[test]
fn original_formula_scripts_keep_their_source_placement_and_own_underline() {
    let pdf = include_bytes!("data/workbook/case-09.pdf");
    let result = analyze_pdf(pdf).unwrap();
    assert!(result.baseline_shifts.unresolved.is_empty());
    assert_eq!(result.baseline_shifts.bindings.len(), 3);
    for (field, glyphs) in [
        (Field::Before, [1273, 1274]),
        (Field::After, [1290, 1291]),
        (Field::After, [1348, 1349]),
    ] {
        let b = result
            .baseline_shifts
            .bindings
            .iter()
            .find(|b| b.source.glyphs == glyphs)
            .unwrap();
        assert_eq!(b.page, 3);
        assert_eq!(b.field, field);
        assert_eq!(b.text, "−６");
        assert_eq!(b.source.shift, Shift::Raised);
        assert!(b.source.size < b.source.parent_size);
        assert_eq!(
            result.documents[b.document]
                .field_text(field)
                .chars()
                .skip(b.offset)
                .take(b.length)
                .collect::<String>(),
            b.text
        );
        for id in glyphs {
            assert_eq!(
                result
                    .underlines
                    .glyph_bindings
                    .iter()
                    .any(|g| g.page == 3 && g.glyph == id),
                field == Field::After
            );
        }
    }
    let native = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    assert_eq!(
        serde_json::to_value(&native).unwrap(),
        serde_json::to_value(&result.native).unwrap()
    );
    for field in [Field::Paragraph, Field::After, Field::Before] {
        assert_eq!(
            result
                .documents
                .iter()
                .map(|d| d.field_text(field))
                .collect::<String>(),
            kanpo_parser::source_underlines::field_text(&native, field)
        );
    }
}

#[test]
fn a_source_word_with_changed_raw_bounds_or_identity_cannot_supply_a_shifted_run() {
    let pdf = include_bytes!("data/workbook/case-09.pdf");
    let source = kanpo_pdf::extract_pages_with_aj1_sources(pdf).unwrap();
    let native = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let runs = kanpo_parser::source_baseline_shifts::for_page(&native.pages[3], &source[3]);
    assert_eq!(runs.len(), 3);
    for run in &runs {
        for mutation in 0..4 {
            let mut page = native.pages[3].clone();
            let w = &mut page.words[run.word];
            match mutation {
                0 => w.raw.push('X'),
                1 => w.source_box[0] += 0.5,
                2 => w.text.push('X'),
                _ => w.id = usize::MAX,
            }
            assert!(
                kanpo_parser::source_baseline_shifts::for_page(&page, &source[3])
                    .iter()
                    .all(|r| r.glyphs != run.glyphs)
            );
        }
    }
}
