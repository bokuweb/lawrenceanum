use super::*;
fn original_inputs() -> Vec<crate::PageInput> {
    let mut out = vec![];
    for bytes in [
        include_bytes!("../../kanpo-pdf/tests/data/native-compound-scene/20260828g001920025.pdf")
            .as_slice(),
        include_bytes!("../../kanpo-pdf/tests/data/native-compound-scene/20260828g001920026.pdf")
            .as_slice(),
        include_bytes!("../tests/data/native-role-frames/20260828g001920027.pdf").as_slice(),
    ] {
        out.extend(crate::page_inputs(bytes).unwrap());
    }
    out
}
#[test]
fn explicit_native_caption_provenance_is_distinct_from_inferred_current_roles() {
    let inputs = original_inputs();
    let pages = crate::analyze_item(&inputs).pages;
    assert!(
        explicit_page(&pages[0]),
        "the original printed after/before captions are a source root"
    );
    assert!(!explicit_page(&pages[1]),"correct inferred roles on a headerless continuation cannot manufacture printed source captions");
    assert!(
        !explicit_page(&pages[2]),
        "an original landscape table has no new printed after/before captions"
    );
}

fn original_caption_page() -> PageTrace {
    crate::analyze_item(&original_inputs()).pages.remove(0)
}
#[test]
fn a_shifted_display_caption_cannot_borrow_unchanged_native_geometry() {
    let mut p = original_caption_page();
    let id = p
        .words
        .iter()
        .find(|w| w.status == WordStatus::HeaderLabel)
        .unwrap()
        .id;
    p.words[id].x0 += 1.;
    p.words[id].x1 += 1.;
    assert!(!explicit_page(&p));
}
#[test]
fn original_projection_remains_a_valid_caption_coordinate_frame() {
    let mut p = original_caption_page();
    for w in &mut p.words {
        if w.status == WordStatus::HeaderLabel {
            [w.x0, w.y0, w.x1, w.y1] = w.source_box;
        }
    }
    assert!(explicit_page(&p));
}
#[test]
fn a_different_native_frame_cannot_lend_its_geometry_to_the_original_display_caption() {
    let mut p = original_caption_page();
    let id = p
        .words
        .iter()
        .find(|w| w.status == WordStatus::HeaderLabel)
        .unwrap()
        .id;
    let NativeBounds::Glyphs(ref mut b) = p.native_bounds[id] else {
        panic!("original native bounds")
    };
    b[0] += 1.;
    b[2] += 1.;
    assert!(!explicit_page(&p));
}

include!("native_role_frame_probe.rs");

#[test]
fn source_caption_semantics_survive_missing_legacy_label_metadata() {
    let mut p = original_caption_page();
    p.native_comparison_labels.fill(None);
    assert_eq!(
        printed_pair(RoleSource {
            regions: &p.regions,
            words: &p.words,
            bounds: &p.native_bounds,
            axes: &p.native_axes
        }),
        Some(crate::ComparisonLabels::AMENDMENT)
    );
}

#[test]
fn original_printed_root_and_rotated_continuation_keep_the_existing_nested_lines() {
    let inputs = original_inputs();
    let mut pages = crate::analyze_item(&inputs).pages;
    let lines = pages[2]
        .regions
        .iter()
        .flat_map(|r| r.lines.clone())
        .collect();
    pages[2].regions = vec![Region {
        role: Role::Body,
        unit: 0,
        part: 0,
        lines,
    }];
    pages[2].native_comparison_labels.fill(None);
    let mut state = frames::State::default();
    for i in 0..3 {
        let dom = scraper::Html::parse_document(&inputs[i].xhtml);
        let node = dom
            .select(&scraper::Selector::parse("page").unwrap())
            .next()
            .unwrap();
        let plan = state.step(&pages[i], node, inputs[i].rules.as_ref().unwrap());
        if i < 2 {
            assert!(plan.is_none());
            continue;
        }
        let Some(Unit::Shinkyu {
            after,
            before,
            labels,
            ..
        }) = plan
        else {
            panic!("a printed root and matching complete native frame must retain its source roles across rotation")
        };
        assert_eq!(labels, Some(crate::ComparisonLabels::AMENDMENT));
        assert_eq!(after[0].len(), 5);
        assert_eq!(before[0].len(), 5);
        let original = &pages[i].regions[0].lines;
        for (a, b) in after[0].iter().chain(&before[0]).zip(original) {
            assert_eq!(
                serde_json::to_value(a).unwrap(),
                serde_json::to_value(b).unwrap()
            );
        }
    }
}

#[test]
fn public_sequence_restores_the_landscape_fields_without_editing_nested_lines() {
    let inputs = original_inputs();
    let actual = crate::analyze_item(&inputs);
    let p = &actual.pages[2];
    assert_eq!(
        p.regions.iter().map(|r| r.role).collect::<Vec<_>>(),
        vec![Role::After, Role::Before]
    );
    assert_eq!(p.regions[0].lines.len(), 5);
    assert_eq!(p.regions[1].lines.len(), 5);
}

include!("native_role_frame_countertests.rs");

include!("native_role_prefix_tests.rs");

include!("native_mixed_line_fields_tests.rs");
