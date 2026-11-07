use kanpo_parser::media::{analyze_pdf, Block, Document, Field, Inline, Target};

#[test]
fn certified_source_layout_matches_the_image_aware_fields_and_keeps_raw_native_auditable() {
    let source = analyze_pdf(include_bytes!("data/workbook/case-04.pdf")).unwrap();
    for field in [Field::Paragraph, Field::After, Field::Before] {
        assert_eq!(
            kanpo_parser::source_underlines::field_text(&source.source_layout, field),
            source
                .documents
                .iter()
                .map(|d| d.field_text(field))
                .collect::<String>()
        );
    }
    assert_eq!(source.frames.bindings.len(), 2);
    assert!(
        kanpo_parser::source_underlines::field_text(&source.native, Field::After).contains('□')
    );
    assert!(
        !kanpo_parser::source_underlines::field_text(&source.source_layout, Field::After)
            .contains('□')
    );
    assert!(serde_json::to_value(&source)
        .unwrap()
        .get("source_layout")
        .is_none());
}

#[test]
fn original_radio_notice_keeps_both_ruby_readings_on_their_source_parent_characters() {
    let pdf = include_bytes!("data/workbook/case-11.pdf");
    let result = analyze_pdf(pdf).unwrap();
    assert!(
        result.ruby.unresolved.is_empty(),
        "{:?}",
        result.ruby.unresolved
    );
    assert_eq!(result.ruby.bindings.len(), 2);
    let source = kanpo_pdf::extract_pages_with_xobject_sources(pdf).unwrap();
    let expected = source[0]
        .document_sha256()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    assert_eq!(
        result.ruby.document_sha256.as_deref(),
        Some(expected.as_str())
    );
    assert!(!result.ruby.coverage_verified);
    let mut fields = Vec::new();
    for binding in &result.ruby.bindings {
        assert_eq!(
            (
                binding.page,
                binding.text.as_str(),
                binding.reading.as_str(),
                binding.length
            ),
            (0, "輻", "ふく", 1)
        );
        assert_eq!(binding.parent_glyphs.len(), 1);
        assert_eq!(binding.ruby_glyphs.len(), 2);
        let page = source[binding.page].page();
        assert_eq!(page.glyphs[binding.parent_glyphs[0]].text, "輻");
        assert_eq!(
            binding
                .ruby_glyphs
                .iter()
                .map(|&i| page.glyphs[i].text.as_str())
                .collect::<String>(),
            "ふく"
        );
        let field = result.documents[binding.document]
            .field_text(binding.field)
            .chars()
            .collect::<Vec<_>>();
        assert_eq!(field[binding.offset], '輻');
        assert_eq!(field[binding.offset + 1], '射');
        fields.push(binding.field);
    }
    fields.sort_by_key(|f| if *f == Field::After { 0 } else { 1 });
    assert_eq!(fields, vec![Field::After, Field::Before]);
    verify_document_material(&result);
}

fn verify_sources(case: usize, result: &kanpo_parser::media::MediaAnalysis) {
    let review: serde_json::Value =
        serde_json::from_str(include_str!("data/workbook/media-source-review.json")).unwrap();
    let original = review["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["case"].as_u64() == Some(case as u64))
        .unwrap();
    assert_eq!(
        result.assets.len(),
        original["images"].as_array().unwrap().len()
    );
    for (asset, source) in result
        .assets
        .iter()
        .zip(original["images"].as_array().unwrap())
    {
        assert_eq!(
            asset.document_sha256,
            original["document_sha256"].as_str().unwrap()
        );
        assert_eq!(asset.page, source["page"].as_u64().unwrap() as usize);
        assert_eq!(asset.draw, source["draw"].as_u64().unwrap() as usize);
        assert_eq!(
            asset.encoded_sha256,
            source["encoded_sha256"].as_str().unwrap()
        );
        assert_eq!(
            asset.encoded_bytes.len(),
            source["encoded_bytes"].as_u64().unwrap() as usize
        );
        let hash = asset
            .encoded_bytes
            .iter()
            .fold(0xcbf29ce484222325u64, |h, b| {
                (h ^ (*b as u64)).wrapping_mul(0x100000001b3)
            });
        assert_eq!(
            format!("{hash:016x}"),
            source["encoded_fnv1a64"].as_str().unwrap()
        );
        assert_eq!(asset.source["object_ref"], source["object_ref"]);
        assert_eq!(asset.source["pixel_width"], source["pixel_width"]);
        assert_eq!(asset.source["pixel_height"], source["pixel_height"]);
        assert_eq!(serde_json::json!(asset.crop_box), source["crop_box"]);
        assert_eq!(serde_json::json!(asset.rotate), source["rotate"]);
    }
}

#[derive(Debug, PartialEq)]
enum Material {
    Char(char),
    Table(Vec<Vec<Vec<Material>>>, Vec<kanpo_parser::model::CellMerge>),
}
fn native_material(runs: &[kanpo_parser::Run]) -> Vec<Material> {
    let mut out = Vec::new();
    for run in runs {
        out.extend(run.text.chars().map(Material::Char));
        if let Some(table) = &run.table {
            out.push(Material::Table(
                table
                    .rows
                    .iter()
                    .map(|r| r.iter().map(|c| native_material(c)).collect())
                    .collect(),
                table.merges.clone(),
            ));
        }
    }
    out
}
fn media_material(runs: &[Inline]) -> Vec<Material> {
    let mut out = Vec::new();
    for run in runs {
        match run {
            Inline::Text { text, .. } => out.extend(text.chars().map(Material::Char)),
            Inline::Table { table } => out.push(Material::Table(
                table
                    .rows
                    .iter()
                    .map(|r| r.iter().map(|c| media_material(c)).collect())
                    .collect(),
                table.merges.clone(),
            )),
            Inline::Image { .. } => {}
        }
    }
    out
}
fn verify_document_material(result: &kanpo_parser::media::MediaAnalysis) {
    for (native, media) in result
        .native
        .articles
        .iter()
        .map(|a| &a.document)
        .zip(&result.documents)
    {
        assert_eq!(native.format, media.format);
        assert_eq!(native.blocks.len(), media.blocks.len());
        for (a, b) in native.blocks.iter().zip(&media.blocks) {
            match (a, b) {
                (kanpo_parser::Block::Paragraph { runs: a }, Block::Paragraph { runs: b }) => {
                    assert_eq!(native_material(a), media_material(b))
                }
                (
                    kanpo_parser::Block::Shinkyu {
                        rows: a,
                        labels: al,
                    },
                    Block::Shinkyu {
                        rows: b,
                        labels: bl,
                    },
                ) => {
                    assert_eq!(al, bl);
                    assert_eq!(a.len(), b.len());
                    for (a, b) in a.iter().zip(b) {
                        assert_eq!(native_material(&a.after), media_material(&b.after));
                        assert_eq!(native_material(&a.before), media_material(&b.before));
                        for (a, b) in [
                            (&a.after_table, &b.after_table),
                            (&a.before_table, &b.before_table),
                        ] {
                            assert_eq!(a.is_some(), b.is_some());
                            if let (Some(a), Some(b)) = (a, b) {
                                assert_eq!(a.merges, b.merges);
                                let aa = a
                                    .rows
                                    .iter()
                                    .map(|r| {
                                        r.iter().map(|c| native_material(c)).collect::<Vec<_>>()
                                    })
                                    .collect::<Vec<_>>();
                                let bb = b
                                    .rows
                                    .iter()
                                    .map(|r| {
                                        r.iter().map(|c| media_material(c)).collect::<Vec<_>>()
                                    })
                                    .collect::<Vec<_>>();
                                assert_eq!(aa, bb);
                            }
                        }
                    }
                }
                _ => panic!("a native document block changed kind"),
            }
        }
    }
}

fn figures(document: &Document) -> Vec<usize> {
    fn runs(items: &[Inline], out: &mut Vec<usize>) {
        for item in items {
            match item {
                Inline::Image { asset } => out.push(*asset),
                Inline::Table { table } => {
                    for row in &table.rows {
                        for cell in row {
                            runs(cell, out);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    for block in &document.blocks {
        match block {
            Block::Paragraph { runs: r } => runs(r, &mut out),
            Block::Shinkyu { rows, .. } => {
                for row in rows {
                    runs(&row.after, &mut out);
                    if let Some(t) = &row.after_table {
                        for r in &t.rows {
                            for c in r {
                                runs(c, &mut out);
                            }
                        }
                    }
                    runs(&row.before, &mut out);
                    if let Some(t) = &row.before_table {
                        for r in &t.rows {
                            for c in r {
                                runs(c, &mut out);
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

#[test]
fn original_coin_picture_occupies_its_original_merged_table_cell() {
    let result = analyze_pdf(include_bytes!("data/workbook/case-18.pdf")).unwrap();
    assert!(result.unresolved.is_empty(), "{:?}", result.unresolved);
    assert_eq!(result.assets.len(), 1);
    assert_eq!(result.placements.len(), 1);
    assert!(matches!(
        result.placements[0].target,
        Target::Cell {
            field: Field::Paragraph,
            page: 1,
            row: 3,
            col: 1,
            ..
        }
    ));
    assert_eq!(figures(&result.documents[0]), vec![0]);
    assert!(!result.assets[0].encoded_bytes.is_empty());
    assert!(!result.assets[0].visibility_verified);
    assert!(!result.coverage_verified);
    verify_sources(18, &result);
    verify_document_material(&result);
}

#[test]
fn original_medical_and_ship_forms_keep_all_draws_and_their_document_order() {
    for (pdf, count) in [
        (include_bytes!("data/workbook/case-10.pdf").as_slice(), 1),
        (include_bytes!("data/workbook/case-15.pdf").as_slice(), 2),
        (include_bytes!("data/workbook/case-16.pdf").as_slice(), 8),
    ] {
        let result = analyze_pdf(pdf).unwrap();
        assert!(result.unresolved.is_empty(), "{:?}", result.unresolved);
        assert_eq!(result.assets.len(), count);
        assert_eq!(result.placements.len(), count);
        verify_sources(
            match count {
                1 => 10,
                2 => 15,
                _ => 16,
            },
            &result,
        );
        verify_document_material(&result);
        assert_eq!(
            figures(&result.documents[0]),
            (0..count).collect::<Vec<_>>()
        );
        for (asset, placement) in result.assets.iter().zip(&result.placements) {
            assert_eq!(asset.id, placement.asset);
            assert!(!asset.encoded_bytes.is_empty());
            assert!(asset.quad.is_some() && asset.bounds.is_some());
        }
        // The two final ship forms are the actual two comparison fields. The
        // preceding replacement forms stay in the prose flow.
        if count == 8 {
            assert!(matches!(
                result.placements[6].target,
                Target::Stream {
                    field: Field::After,
                    ..
                }
            ));
            assert!(matches!(
                result.placements[7].target,
                Target::Stream {
                    field: Field::Before,
                    ..
                }
            ));
            assert!(result.placements[..6].iter().all(|p| matches!(
                p.target,
                Target::Stream {
                    field: Field::Paragraph,
                    ..
                }
            )));
        }
    }
}

#[test]
fn painted_glyph_readings_are_available_in_the_same_image_aware_document() {
    let pdf = include_bytes!("data/workbook/case-06.pdf");
    let report = serde_json::from_str(include_str!(
        "data/workbook/case-06.paint-observations.json"
    ))
    .unwrap();
    let result = kanpo_parser::media::analyze_pdf_with_observations(pdf, report).unwrap();
    let audit = result.painted_text.as_ref().unwrap();
    assert_eq!(audit.bindings.len(), 8);
    assert!(audit.unresolved.is_empty());
    assert!(result.assets.is_empty() && result.placements.is_empty());
    let chars = result
        .documents
        .iter()
        .flat_map(|d| d.blocks.iter())
        .flat_map(|b| match b {
            Block::Paragraph { runs } => media_material(runs),
            _ => vec![],
        })
        .map(|m| match m {
            Material::Char(c) => c,
            _ => panic!("unexpected table"),
        })
        .collect::<String>();
    for number in ["別表第五（二十八）", "別表第五（三十一）"] {
        assert_eq!(
            chars.matches(number).count(),
            result.native.text.matches(number).count() + 4
        );
    }
}

#[test]
fn original_supplement_keeps_full_page_surfaces_and_separate_graphic_at_page_scope() {
    use kanpo_parser::media::PageImageKind;
    for (pdf, expected) in [
        (include_bytes!("data/workbook/case-10-listed-1.pdf").as_slice(), 2),
        (include_bytes!("data/workbook/case-10-listed-2.pdf").as_slice(), 1),
        (include_bytes!("data/workbook/case-10-listed-3.pdf").as_slice(), 1),
    ] {
        let result=analyze_pdf(pdf).unwrap();
        assert!(result.unresolved.is_empty(),"{:?}",result.unresolved);
        assert_eq!(result.page_placements.len(),expected);
        assert!(result.placements.is_empty());
        assert!(matches!(result.page_placements[0].kind,PageImageKind::PageSurface));
        if expected==2 {assert!(matches!(result.page_placements[1].kind,PageImageKind::IndependentGraphic));}
        for placement in &result.page_placements {
            let asset=&result.assets[placement.asset];
            assert_eq!(asset.page,placement.page);
            assert!(!asset.encoded_bytes.is_empty());
            assert!(asset.quad.is_some());
        }
        let native=kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
        assert_eq!(serde_json::to_value(native).unwrap(),serde_json::to_value(result.native).unwrap());
    }
}
