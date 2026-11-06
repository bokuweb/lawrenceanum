use kanpo_parser::{
    model::Block,
    source_underlines::{analyze_pdf, Field},
};

#[test]
fn original_cff_underlines_reach_document_runs_without_changing_any_native_text_or_ownership() {
    let pdf = include_bytes!("data/workbook/case-01.pdf");
    let result = analyze_pdf(pdf).unwrap();
    assert_eq!(result.applied_fields, vec![Field::After, Field::Before]);
    assert!(
        result.unresolved_fields.is_empty(),
        "{:?}",
        result.unresolved_fields
    );
    assert!(!result.glyph_bindings.is_empty());
    assert_eq!(result.native.text, result.styled.text);
    assert_eq!(result.native.format, result.styled.format);
    assert_eq!(
        serde_json::to_value(&result.native.pages).unwrap(),
        serde_json::to_value(&result.styled.pages).unwrap()
    );
    assert_eq!(result.native.articles.len(), result.styled.articles.len());
    for (a, b) in result.native.articles.iter().zip(&result.styled.articles) {
        assert_eq!(a.text, b.text);
        assert_eq!(a.document.to_text(), b.document.to_text());
        assert_eq!(
            serde_json::to_value(&a.notices).unwrap(),
            serde_json::to_value(&b.notices).unwrap()
        );
    }
    let row = result
        .styled
        .articles
        .iter()
        .flat_map(|a| &a.document.blocks)
        .find_map(|b| match b {
            Block::Shinkyu { rows, .. } => rows.first(),
            _ => None,
        })
        .unwrap();
    // Original PDF and independent RTF both underline the inserted caption and
    // article number. The opposite-side insertion marker has no underline.
    assert_eq!(
        row.after[0].text,
        "（都道府県による非常災害廃棄物の処理に関する協定において定める事項）"
    );
    assert!(row.after[0].underline);
    assert!(row
        .after
        .iter()
        .any(|r| r.text == "第一条の二の三" && r.underline));
    assert_eq!(row.before[0].text, "（新設）");
    assert!(!row.before[0].underline);
    let normal = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    assert_eq!(
        serde_json::to_value(normal).unwrap(),
        serde_json::to_value(result.native).unwrap()
    );
}

#[test]
fn source_without_cff_decoration_does_not_gain_underlines_or_lose_text() {
    let pdf = include_bytes!("data/workbook/case-10.pdf");
    let result = analyze_pdf(pdf).unwrap();
    assert!(result.applied_fields.is_empty());
    assert!(result.glyph_bindings.is_empty());
    assert_eq!(
        serde_json::to_value(result.native).unwrap(),
        serde_json::to_value(result.styled).unwrap()
    );
}

#[test]
fn other_original_comparisons_retain_every_character_and_all_table_structure() {
    fn shape(value: serde_json::Value) -> serde_json::Value {
        use serde_json::Value;
        match value {
            Value::Array(items)
                if items.iter().all(|item| {
                    item.as_object().is_some_and(|o| {
                        o.contains_key("text")
                            && o.keys()
                                .all(|k| ["text", "underline", "table"].contains(&k.as_str()))
                    })
                }) =>
            {
                let mut out = Vec::new();
                let mut text = String::new();
                for run in items {
                    text.push_str(run["text"].as_str().unwrap());
                    if let Some(table) = run.get("table") {
                        if !text.is_empty() {
                            out.push(Value::String(std::mem::take(&mut text)));
                        }
                        out.push(shape(table.clone()));
                    }
                }
                if !text.is_empty() {
                    out.push(Value::String(text));
                }
                Value::Array(out)
            }
            Value::Array(items) => Value::Array(items.into_iter().map(shape).collect()),
            Value::Object(items) => {
                Value::Object(items.into_iter().map(|(k, v)| (k, shape(v))).collect())
            }
            other => other,
        }
    }
    for pdf in [
        include_bytes!("data/workbook/case-02.pdf").as_slice(),
        include_bytes!("data/workbook/case-03.pdf").as_slice(),
        include_bytes!("data/workbook/case-04.pdf").as_slice(),
        include_bytes!("data/workbook/case-11.pdf").as_slice(),
        include_bytes!("data/workbook/case-15.pdf").as_slice(),
        include_bytes!("data/workbook/case-16.pdf").as_slice(),
    ] {
        let result = analyze_pdf(pdf).unwrap();
        assert_eq!(result.applied_fields, vec![Field::After, Field::Before]);
        // Text, row pairing, nested table rows, cell merges, source words and
        // article ownership all survive independently of Run splitting/styles.
        assert_eq!(
            shape(serde_json::to_value(result.native).unwrap()),
            shape(serde_json::to_value(result.styled).unwrap())
        );
    }
}

#[test]
fn mixed_writing_directions_use_source_cell_geometry_to_order_all_style_flags() {
    let result = analyze_pdf(include_bytes!("data/workbook/case-09.pdf")).unwrap();
    assert_eq!(result.scope, "cardinal_cff_rectangle_decorations");
    assert_eq!(result.applied_fields, vec![Field::After, Field::Before]);
    assert!(
        result.source_issues.is_empty(),
        "{:?}",
        result.source_issues
    );
    assert!(!result
        .unresolved_fields
        .iter()
        .any(|f| f.field == Field::After || f.field == Field::Before));
    assert_eq!(result.native.text, result.styled.text);
    assert_eq!(
        serde_json::to_value(&result.native.pages).unwrap(),
        serde_json::to_value(&result.styled.pages).unwrap()
    );
    for (a, b) in result.native.articles.iter().zip(&result.styled.articles) {
        assert_eq!(a.document.to_text(), b.document.to_text());
    }
    // Source PDF p.2: these two inline horizontal Words share the vertical
    // column's right-side CFF bars. Letter contents do not select the parent.
    for (word, glyphs) in [(95, [652, 653, 654]), (96, [669, 670, 671])] {
        assert_eq!(result.native.pages[1].words[word].text, "MHz");
        let bound = result
            .glyph_bindings
            .iter()
            .filter(|b| b.page == 1 && b.word == word)
            .map(|b| b.glyph)
            .collect::<Vec<_>>();
        assert_eq!(bound, glyphs);
    }
    // A wider inline object in the neighbouring column must not suppress the
    // underline of the original シ. Its original bar is visually present.
    assert!(result
        .glyph_bindings
        .iter()
        .any(|b| b.page == 2 && b.glyph == 811));
    // The original horizontal source underlines just the final ５MHzとする。
    // Its rulings 1690..1697 are themselves grouped into one bbox Word.
    let horizontal = result
        .glyph_bindings
        .iter()
        .filter(|b| b.page == 3 && b.word == 15)
        .map(|b| b.glyph)
        .collect::<Vec<_>>();
    assert_eq!(horizontal, (1682..1690).collect::<Vec<_>>());
    let unique = result
        .glyph_bindings
        .iter()
        .map(|b| (b.page, b.glyph))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(unique.len(), result.glyph_bindings.len());
}

#[test]
fn original_mixed_font_baselines_do_not_puncture_continuous_rulings() {
    let pdf = include_bytes!("data/workbook/case-09.pdf");
    let result = analyze_pdf(pdf).unwrap();
    let source = kanpo_pdf::extract_pages_with_aj1_sources(pdf).unwrap();
    // Reviewed in the original PDF: the parentheses/function/unit characters
    // share continuous bars with the surrounding source Word. Their font
    // origins differ; no RTF or expected substring supplies the association.
    for (page, glyph) in [(3, 965), (3, 966), (3, 977), (3, 1286), (3, 1344), (4, 98)] {
        let binding = result.glyph_bindings.iter()
            .find(|b| b.page == page && b.glyph == glyph).unwrap();
        assert!(!binding.decoration_components.is_empty());
        for component in &binding.decoration_components {
            assert_eq!(component.component, 0);
            assert_eq!(source[page].glyph_painted_rectangle(component.glyph), Some(component.bounds));
        }
    }
    assert_eq!(result.applied_fields, vec![Field::After, Field::Before]);
    assert!(result.source_issues.is_empty());
    let native = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    assert_eq!(serde_json::to_value(native).unwrap(), serde_json::to_value(&result.native).unwrap());
    for (a, b) in result.native.articles.iter().zip(&result.styled.articles) {
        assert_eq!(a.document.to_text(), b.document.to_text());
    }
}
