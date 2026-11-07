use kanpo_parser::{
    media::analyze_pdf,
    source_line_projection::Origin,
    source_underlines::{field_text, Field},
};
#[test]
fn original_parallel_columns_and_horizontal_item_order_have_forward_scalar_provenance() {
    for (pdf, count, generated) in [
        (include_bytes!("data/workbook/case-12.pdf").as_slice(), 2, 2),
        // These are wrapped quotation lanes, not a short agency-name note.
        // Each lane now retains its direct original Words; no synthetic join
        // character or composed-line projection is needed.
        (include_bytes!("data/workbook/case-13.pdf").as_slice(), 0, 0),
        (
            include_bytes!("data/workbook/case-17.pdf").as_slice(),
            26,
            1,
        ),
    ] {
        let result = analyze_pdf(pdf).unwrap();
        assert_eq!(result.underlines.line_projections.len(), count);
        if count == 0 {
            for ids in [vec![27,28,29,30,34],vec![32,33,36]] {
                let page=&result.native.pages[0];
                let found=page.regions.iter().flat_map(|r| &r.lines).filter(|l|l.words==ids).collect::<Vec<_>>();
                assert_eq!(found.len(),1);
                assert_eq!(found[0].text,ids.iter().map(|&id|page.words[id].text.as_str()).collect::<String>());
            }
        }
        assert!(!result
            .underlines
            .unresolved_fields
            .iter()
            .any(|f| f.field == Field::Paragraph));
        assert!(result.underlines.source_issues.is_empty());
        let mut actual_generated = 0;
        let mut relocated = 0;
        for record in &result.underlines.line_projections {
            let text = field_text(&result.native, record.field);
            assert_eq!(
                text.chars()
                    .skip(record.offset)
                    .take(record.tokens.len())
                    .collect::<String>(),
                record.text
            );
            for token in &record.tokens {
                match &token.origin {
                    Origin::Word { word, scalar } | Origin::RelocatedWord { word, scalar, .. } => {
                        if matches!(token.origin, Origin::RelocatedWord { .. }) {
                            relocated += 1;
                        }
                        let word = &result.native.pages[record.page].words[*word];
                        assert_eq!(
                            word.text
                                .chars()
                                .filter(|c| !c.is_whitespace())
                                .nth(*scalar),
                            Some(token.character)
                        );
                    }
                    Origin::ParallelJoin { before, after } => {
                        actual_generated += 1;
                        assert_eq!(token.character, '・');
                        assert!(!before.is_empty() && !after.is_empty());
                    }
                    _ => panic!("body projection contains whitespace"),
                }
            }
        }
        assert_eq!(actual_generated, generated);
        assert_eq!(relocated, if count == 26 { 8 } else { 0 });
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
                field_text(&native, field)
            );
        }
    }
}
