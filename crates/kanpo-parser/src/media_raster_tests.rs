use super::*;
use serde_json::json;
use std::sync::OnceLock;

fn analysis() -> MediaAnalysis {
    static SOURCE: OnceLock<MediaAnalysis> = OnceLock::new();
    SOURCE
        .get_or_init(|| {
            crate::media::analyze_pdf(include_bytes!("../tests/data/workbook/case-10.pdf")).unwrap()
        })
        .clone()
}

// Synthetic recognizer proposals exercise source attachment and the editable
// representation. They are not assertions that OCR read this source text.
fn layout(asset: &Asset) -> Layout {
    let w = asset.source["pixel_width"].as_u64().unwrap() as usize;
    let h = asset.source["pixel_height"].as_u64().unwrap() as usize;
    let text = "<甲&𠮷>";
    serde_json::from_value(json!({
        "schema":1,"kind":"source_derived_raster_layout","coverage_verified":false,"visibility":"unverified",
        "source":{"document_sha256":asset.document_sha256,"encoded_sha256":asset.encoded_sha256,
                  "source_PNG_sha256":"c".repeat(64),"source_RGBA_sha256":"d".repeat(64),"page":asset.page,"draw":asset.draw},
        "canvas":{"pixel_width":w,"pixel_height":h},
        "source_frame":{"pixel_width":w,"pixel_height":h,"image_width":w,"image_height":h,
            "recognition_to_image":[1,0,0,1,0,0],"image_to_user":asset.source["image_to_user"],
            "crop_box":asset.crop_box,"rotate":asset.rotate},
        "provenance":{"preparation_sha256":"e".repeat(64),"ocr_index_sha256":"f".repeat(64),
            "recognition_parent_sha256":"a".repeat(64),"producer_sha256":"b".repeat(64)},
        "orientation":{"quarter_turns_ccw":0,"globally_resolved":true},
        "observations":[{"id":0,"tile":0,"region":0,"text":text,"confidence":0.0,"is_vertical":false,
            "source_pixels":[0,0,20,20],"core":[0,0,w,h],"ink_pixels":20,"ink_bounds_source_pixels":[3,4,9,12],
            "ownership":"owned_ink","word":0,"raw":{"text":text,"confidence":0.0,"is_vertical":false}}],
        "runs":[],"words":[{"id":0,"text":text,"alternatives":[{"text":text,"observations":[0]}],
            "bounds_source_pixels":[3,4,9,12],"bounds_layout_pixels":[3,4,9,12],"support_sha256":"a".repeat(64),
            "support_pixels":20,"cell":7,"cells":[7],"writing_modes":[false],"status":"candidate_unverified"}],
        "cells":[{"id":7,"interior_source_pixels":[2,2,19,19],"frame_source_pixels":[1,1,20,20],
            "frame_layout_pixels":[1,1,20,20],"boundary_kind":"solid_source","words":[0]}],
        "tables":[{"id":0,"cells":[7],"x_cuts":[1,20],"y_cuts":[1,20],
            "slots":[{"cell":7,"row":0,"col":0,"rowspan":1,"colspan":1}],"uncovered_slots":[],
            "overlapping_cells":[],"status":"geometric_candidates_unverified"}],
        "lines":[{"id":0,"cell":7,"words":[0],"status":"ordered_disjoint_horizontal_words"}],
        "unresolved":[],"outside_words":[],"failed_tiles":[],
        "ownership_policy":"actual_copy_ink_bbox_center_in_half_open_source_core",
        "alias_policy":"exact_global_copy_ink_row_runs_and_exact_raw_text_agreement"
    })).unwrap()
}

#[test]
fn editable_candidates_attach_to_the_original_draw_without_changing_native_material() {
    let mut result = analysis();
    let before = serde_json::to_value(&result).unwrap();
    let report = layout(&result.assets[0]);
    attach(&mut result, vec![report]).unwrap();
    let saved = result.assets[0].raster_layout.as_ref().unwrap();
    assert_eq!(saved.words[0].text.as_deref(), Some("<甲&𠮷>"));
    assert!(!saved.coverage_verified);
    assert!(!result.coverage_verified);
    let mut after = serde_json::to_value(&result).unwrap();
    after["assets"][0]
        .as_object_mut()
        .unwrap()
        .remove("raster_layout");
    assert_eq!(after, before);
    let mut edited = (**saved).clone();
    edited.words[0].text = Some("別の文字".into());
    assert_eq!(edited.words[0].text.as_deref(), Some("別の文字"));
    assert_eq!(saved.words[0].alternatives[0].text, "<甲&𠮷>");
}

#[test]
fn foreign_stream_page_or_geometry_cannot_borrow_the_original_image_placement() {
    let result = analysis();
    let asset = &result.assets[0];
    let original = layout(asset);
    for mutation in 0..6 {
        let mut report = original.clone();
        match mutation {
            0 => report.source.encoded_sha256 = "0".repeat(64),
            1 => report.source.page += 1,
            2 => report.source_frame.image_to_user[4] += 1.,
            3 => report.source_frame.crop_box[0] += 1.,
            4 => report.source_frame.recognition_to_image[4] += 1.,
            _ => report.source_frame.image_width += 1,
        }
        assert!(
            report.validate_for_asset(asset).is_err(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn duplicate_or_foreign_reports_do_not_partially_mutate_the_analysis() {
    let mut result = analysis();
    let before = serde_json::to_value(&result).unwrap();
    let good = layout(&result.assets[0]);
    let mut foreign = good.clone();
    foreign.source.draw += 1;
    assert!(attach(&mut result, vec![good.clone(), foreign]).is_err());
    assert_eq!(serde_json::to_value(&result).unwrap(), before);
    assert!(attach(&mut result, vec![good.clone(), good]).is_err());
    assert_eq!(serde_json::to_value(&result).unwrap(), before);
}

#[test]
fn every_cardinal_source_pixel_permutation_projects_back_to_the_original_image_quad() {
    let result = analysis();
    let asset = &result.assets[0];
    let original = layout(asset);
    let (w, h) = (original.canvas.pixel_width, original.canvas.pixel_height);
    let matrices = [
        [1., 0., 0., 1., 0., 0.],
        [0., 1., -1., 0., w as f64, 0.],
        [-1., 0., 0., -1., w as f64, h as f64],
        [0., -1., 1., 0., 0., h as f64],
    ];
    for (turn, matrix) in matrices.into_iter().enumerate() {
        for mirrored in [false, true] {
            let mut report = original.clone();
            let (cw, ch) = if turn % 2 == 1 { (h, w) } else { (w, h) };
            report.canvas.pixel_width = cw;
            report.canvas.pixel_height = ch;
            report.source_frame.pixel_width = cw;
            report.source_frame.pixel_height = ch;
            let mut matrix = matrix;
            if mirrored {
                matrix[4] += matrix[0] * cw as f64;
                matrix[5] += matrix[1] * cw as f64;
                matrix[0] = -matrix[0];
                matrix[1] = -matrix[1];
            }
            report.source_frame.recognition_to_image = matrix;
            report.observations[0].core = [0, 0, cw, ch];
            report.validate_for_asset(asset).unwrap();
            let quad = report
                .source_frame
                .project([0., 0., cw as f64, ch as f64])
                .unwrap();
            let expected = asset.quad.unwrap();
            assert!(quad.iter().all(|p| expected.contains(p)));
        }
    }
}

#[test]
fn source_ownership_aliases_and_conflicting_readings_remain_explicit() {
    let result = analysis();
    let asset = &result.assets[0];
    let mut report = layout(asset);
    let mut second = report.observations[0].clone();
    second.id = 1;
    second.region = 1;
    second.text = "乙".into();
    second.raw["text"] = json!("乙");
    second.confidence = 0.99;
    second.raw["confidence"] = json!(0.99);
    report.observations.push(second);
    report.words[0].alternatives.push(Alternative {
        text: "乙".into(),
        observations: vec![1],
    });
    report.words[0].text = None;
    report.lines[0].status = "reading_order_unresolved".into();
    report.validate_for_asset(asset).unwrap();
    report.words[0].text = Some("乙".into());
    assert!(report.validate_for_asset(asset).is_err());
    let mut report = layout(asset);
    report.observations[0].core = [20, 0, 30, 20];
    assert!(report.validate_for_asset(asset).is_err());
    let mut report = layout(asset);
    report.words[0].alternatives[0].observations.push(0);
    assert!(report.validate_for_asset(asset).is_err());
}

#[test]
fn geometric_cell_holes_and_unresolved_overlap_cannot_be_silently_filled() {
    let result = analysis();
    let asset = &result.assets[0];
    let original = layout(asset);
    let mut report = original.clone();
    report.tables[0].uncovered_slots.push([0, 0]);
    assert!(report.validate_for_asset(asset).is_err());
    let mut report = original.clone();
    report.tables[0].x_cuts.insert(1, 5.);
    report.tables[0].slots[0].colspan = 2;
    assert!(report.validate_for_asset(asset).is_err());
    let mut report = original;
    report.cells[0].words.clear();
    assert!(report.validate_for_asset(asset).is_err());
}

fn character_layer(report: &Layout) -> characters::Layer {
    let (w, h) = (report.canvas.pixel_width, report.canvas.pixel_height);
    let texts = ["甲", "𠮷\u{3099}"];
    let confidences = [0.1, 0.99];
    let raw_candidates=texts.iter().zip(confidences).map(|(text,confidence)|json!({
        "text":text,"confidence":confidence,"characters":[{"text":text,"unicode_scalar_offset":0,
            "unicode_scalar_length":text.chars().count(),"quad":[[0.,1.],[1.,1.],[1.,0.],[0.,0.]]}]
    })).collect::<Vec<_>>();
    let characters=texts.iter().enumerate().map(|(i,text)|json!({
        "id":i,"observation":0,"candidate":i,"character":0,"unicode_scalar_offset":0,
        "unicode_scalar_length":text.chars().count(),"text":text,
        "quad_source_pixels":[[0,0],[w,0],[w,h],[0,h]],"source_pixels":[0,0,w,h],"glyph_right_axis":0,
        "ink_pixels":20,"ink_bounds_source_pixels":[3,4,9,12],"support_sha256":"a".repeat(64),
        "cells":[7],"cell":7,"containing_words":[0],"overlapping_words":[],"geometry_status":"source_character_quad_candidate"
    })).collect::<Vec<_>>();
    let candidates = texts
        .iter()
        .enumerate()
        .map(|(i, text)| {
            json!({"observation":0,"candidate":i,
        "text":text,"confidence":confidences[i],"characters":[i],"fragments":[i]})
        })
        .collect::<Vec<_>>();
    let fragments=texts.iter().enumerate().map(|(i,text)|json!({"id":i,"observation":0,"candidate":i,
        "text":text,"characters":[i],"cell":7,"glyph_right_axis":0,"bounds_source_pixels":[3,4,9,12],
        "status":"model_sequence_fragment_candidate"})).collect::<Vec<_>>();
    serde_json::from_value(json!({"schema":1,"kind":"source_derived_raster_characters","source":report.source,
        "canvas":report.canvas,"recognition_parent_sha256":report.provenance.recognition_parent_sha256,
        "report_sha256":"a".repeat(64),"raw_report":{"schema":1,"kind":"source_derived_raster_orientation",
            "source":report.source,"recognition_parent_sha256":report.provenance.recognition_parent_sha256,
            "vision":{"page":{"revision":3,"language_correction":false,"observations":[{"candidates":raw_candidates}]}}},
        "implementation_sha256":{"raster_characters.py":"a".repeat(64),"raster_layout.py":"a".repeat(64),
            "raster_orientation.py":"a".repeat(64),"source_gutters.py":"a".repeat(64)},
        "characters":characters,"candidates":candidates,"fragments":fragments,"unresolved":[],
        "association_policy":"complete_copy_ink_bounds_in_unique_innermost_source_cell",
        "fragment_policy":"raw_character_ranges_split_at_supported_cell_or_axis_changes",
        "coverage_verified":false,"visibility":"unverified"
    })).unwrap()
}

#[test]
fn all_character_alternatives_keep_scalar_ranges_positions_and_original_raw_candidates() {
    let mut result = analysis();
    let mut report = layout(&result.assets[0]);
    let base = serde_json::to_value(&report).unwrap();
    report.character_layer = Some(Box::new(character_layer(&report)));
    attach(&mut result, vec![report]).unwrap();
    let saved = result.assets[0].raster_layout.as_ref().unwrap();
    let layer = saved.character_layer.as_ref().unwrap();
    assert_eq!(
        layer
            .candidates
            .iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>(),
        ["甲", "𠮷\u{3099}"]
    );
    assert_eq!(layer.characters[1].unicode_scalar_length, 2);
    assert_eq!(layer.fragments[1].cell, Some(7));
    assert_eq!(layer.candidates[0].confidence, 0.1);
    assert_eq!(layer.candidates[1].confidence, 0.99);
    let mut after = serde_json::to_value(saved).unwrap();
    after.as_object_mut().unwrap().remove("character_layer");
    assert_eq!(after, base);
}

#[test]
fn character_source_scalar_candidate_and_fragment_changes_are_rejected_atomically() {
    let result = analysis();
    let mut original = layout(&result.assets[0]);
    original.character_layer = Some(Box::new(character_layer(&original)));
    for mutation in 0..12 {
        let mut report = original.clone();
        let layer = report.character_layer.as_mut().unwrap();
        match mutation {
            0 => layer.source.document_sha256 = "c".repeat(64),
            1 => layer.recognition_parent_sha256 = "c".repeat(64),
            2 => layer.candidates.pop().map(|_| ()).unwrap(),
            3 => layer.characters[1].unicode_scalar_length = 1,
            4 => layer.characters[0].source_pixels.as_mut().unwrap()[0] += 1,
            5 => layer.characters[0].cell = None,
            6 => layer.fragments[0].text = "修正".into(),
            7 => layer.candidates[0].confidence = 0.99,
            8 => layer.characters[0].containing_words.clear(),
            9 => {
                layer.raw_report["vision"]["page"]["observations"][0]["candidates"][1]["text"] =
                    json!("別")
            }
            10 => layer.fragments[0].characters = vec![1],
            _ => layer.characters[0].glyph_right_axis = Some(1),
        }
        let mut target = result.clone();
        let before = serde_json::to_value(&target).unwrap();
        assert!(
            attach(&mut target, vec![report]).is_err(),
            "mutation {mutation}"
        );
        assert_eq!(serde_json::to_value(&target).unwrap(), before);
    }
}

#[test]
fn unpositioned_whitespace_keeps_a_supported_raw_range_without_inventing_glyph_geometry() {
    let result = analysis();
    let mut report = layout(&result.assets[0]);
    let mut layer = character_layer(&report);
    let raw = &mut layer.raw_report["vision"]["page"]["observations"][0]["candidates"][0];
    let first = raw["characters"][0].clone();
    let mut third = first.clone();
    third["text"] = json!("乙");
    third["unicode_scalar_offset"] = json!(3);
    raw["text"] = json!("甲 \u{3000}乙");
    raw["characters"] = json!([first,
        {"text":" \u{3000}","unicode_scalar_offset":1,"unicode_scalar_length":2,"geometry":"unavailable"},third]);
    let mut second = layer.characters[0].clone();
    second.id = 1;
    second.character = 1;
    second.text = " \u{3000}".into();
    second.unicode_scalar_offset = 1;
    second.unicode_scalar_length = 2;
    second.source_pixels = None;
    second.quad_source_pixels = None;
    second.glyph_right_axis = None;
    second.ink_pixels = 0;
    second.ink_bounds_source_pixels = None;
    second.support_sha256 = None;
    second.cells.clear();
    second.cell = None;
    second.containing_words.clear();
    second.geometry_status = "whitespace_position_unverified".into();
    let mut third = layer.characters[0].clone();
    third.id = 2;
    third.character = 2;
    third.text = "乙".into();
    third.unicode_scalar_offset = 3;
    layer.characters[1].id = 3;
    layer.characters.insert(1, second);
    layer.characters.insert(2, third);
    layer.candidates[0].text = "甲 \u{3000}乙".into();
    layer.candidates[0].characters = vec![0, 1, 2];
    layer.candidates[1].characters = vec![3];
    layer.fragments[0].text = "甲 \u{3000}乙".into();
    layer.fragments[0].characters = vec![0, 1, 2];
    layer.fragments[1].characters = vec![3];
    layer.unresolved.push(characters::Unresolved {
        reason: "whitespace_position_unverified".into(),
        characters: vec![1],
        fragments: vec![],
    });
    layer.validate(&report).unwrap();
    assert_eq!(layer.fragments[0].cell, Some(7));
    assert!(layer.characters[1].source_pixels.is_none());
    let mut missing = layer.clone();
    missing.unresolved.clear();
    assert!(missing.validate(&report).is_err());
    let mut missing = layer.clone();
    missing.fragments[0].characters = vec![0, 2];
    assert!(missing.validate(&report).is_err());
    report.character_layer = Some(Box::new(layer));
    report.validate_for_asset(&result.assets[0]).unwrap();
}
