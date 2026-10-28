use super::*;
fn pdf() -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../kanpo-parser/tests/archive/pages/20260807g00178/20260807g001780121.pdf"),
    )
    .unwrap()
}
fn page() -> Page {
    static PAGE: std::sync::OnceLock<Page> = std::sync::OnceLock::new();
    PAGE.get_or_init(|| {
        crate::extract_pages_with_aj1_sources(&pdf())
            .unwrap()
            .remove(0)
            .page()
            .clone()
    })
    .clone()
}
fn original() -> (Page, Word) {
    let p = page();
    let w = words(&p)
        .into_iter()
        .find(|w| native_glyph_range(&p, w).is_none() && native_source_occurrence(&p, w).is_some())
        .unwrap();
    (p, w)
}
#[test]
fn original_source_identity_does_not_grant_a_flow_certificate() {
    let (p, w) = original();
    let o = native_source_occurrence(&p, &w).unwrap();
    assert_eq!(
        (o.first, o.after),
        (*w.glyphs.first().unwrap(), w.glyphs.last().unwrap() + 1)
    );
    assert!(native_glyph_range(&p, &w).is_none());
}
#[test]
fn every_original_glyph_and_public_box_stays_in_the_opaque_envelope() {
    let (p, w) = original();
    let o = native_source_occurrence(&p, &w).unwrap();
    for b in w
        .glyphs
        .iter()
        .map(|&i| poppler_box(&p.glyphs[i]))
        .chain([[w.x_min, w.y_min, w.x_max, w.y_max]])
    {
        assert!(
            o.bounds[0] <= b[0]
                && o.bounds[1] <= b[1]
                && b[2] <= o.bounds[2]
                && b[3] <= o.bounds[3]
        );
    }
}
#[test]
fn source_identity_has_no_baseline_drift_budget() {
    for delta in [0.25, 1., 13.] {
        let (mut p, w) = original();
        let g = &mut p.glyphs[w.glyphs[1]];
        g.y += delta;
        g.end_y += delta;
        g.bbox[1] += delta;
        g.bbox[3] += delta;
        assert!(native_source_occurrence(&p, &w).is_some());
        assert!(native_glyph_range(&p, &w).is_none());
    }
}
#[test]
fn changed_word_text_does_not_keep_an_occurrence() {
    let (p, mut w) = original();
    w.text.push('別');
    assert!(native_source_occurrence(&p, &w).is_none());
}
#[test]
fn changing_the_existing_projection_is_not_a_rounding_allowance() {
    let (p, mut w) = original();
    w.x_min = w.x_min.next_up();
    assert!(native_source_occurrence(&p, &w).is_none());
}
#[test]
fn missing_a_source_glyph_never_proves_a_contiguous_occurrence() {
    let (p, mut w) = original();
    w.glyphs.remove(1);
    assert!(native_source_occurrence(&p, &w).is_none());
}
#[test]
fn source_order_and_unique_glyph_ownership_remain_required() {
    let (p, w) = original();
    let mut changed = w.clone();
    changed.glyphs.reverse();
    assert!(native_source_occurrence(&p, &changed).is_none());
    changed = w;
    changed.glyphs[1] = changed.glyphs[0];
    assert!(native_source_occurrence(&p, &changed).is_none());
}
#[test]
fn incomplete_or_nonfinite_source_geometry_is_not_an_opaque_envelope() {
    let (p, w) = original();
    for value in [f64::NAN, f64::INFINITY] {
        let mut changed = p.clone();
        changed.glyphs[w.glyphs[1]].bbox[0] = value;
        assert!(native_source_occurrence(&changed, &w).is_none());
    }
    let mut changed = w;
    *changed.glyphs.last_mut().unwrap() = usize::MAX;
    assert!(native_source_occurrence(&p, &changed).is_none());
}
#[test]
fn ordinary_flow_ranges_keep_their_exact_source_occurrences() {
    let p = page();
    for w in words(&p) {
        if let Some(range) = native_glyph_range(&p, &w) {
            let o = native_source_occurrence(&p, &w).unwrap();
            assert_eq!(range, (o.first, o.after));
        }
    }
}
#[test]
fn the_complete_original_scene_keeps_nonflow_words_as_opaque_obstacles() {
    let sources = crate::extract_pages_with_aj1_sources(&pdf()).unwrap();
    let s = &sources[0];
    let scene = s.lineage.scene_capture(s.page()).unwrap();
    assert_eq!(scene.version, 2);
    assert!(scene.unknown.is_empty());
    let expected = words(s.page())
        .into_iter()
        .filter(|w| native_glyph_range(s.page(), w).is_none())
        .map(|w| native_source_occurrence(s.page(), &w).unwrap())
        .collect::<Vec<_>>();
    assert!(!expected.is_empty());
    assert_eq!(scene.word_obstacles.len(), expected.len());
    for o in expected {
        assert!(scene.word_obstacles.contains(&(o.first, o.after, o.bounds)));
    }
}
#[test]
fn an_existing_complete_v1_scene_has_no_new_payload_field() {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../kanpo-parser/tests/data/native-marker-records/source.pdf");
    let s = crate::extract_pages_with_aj1_sources(&std::fs::read(p).unwrap())
        .unwrap()
        .remove(0);
    let scene = s.lineage.scene_capture(s.page()).unwrap();
    assert_eq!(scene.version, 1);
    assert!(scene.word_obstacles.is_empty());
    assert!(serde_json::to_value(scene)
        .unwrap()
        .get("word_obstacles")
        .is_none());
}
