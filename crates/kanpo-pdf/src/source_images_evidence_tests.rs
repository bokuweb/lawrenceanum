use super::tests::{objects, pdf, stream};
use super::*;
fn read(bytes: &[u8]) -> Vec<XObjectPageSources> {
    let old = crate::extract_pages(bytes).unwrap();
    let a = extract(bytes).unwrap();
    assert_eq!(
        serde_json::to_string(&old).unwrap(),
        serde_json::to_string(&a.iter().map(|p| p.page()).collect::<Vec<_>>()).unwrap()
    );
    assert!(a.iter().all(|p| !p.visibility_verified()));
    a
}
fn gap(p: &XObjectPageSources, k: GapKind) {
    assert!(
        p.capture_gaps().iter().any(|g| g.kind == k),
        "missing explicit {k:?}: {:?}",
        p.capture_gaps()
    );
}
#[test]
fn form_bbox_is_transformed_before_child_image_cm_and_does_not_leak() {
    let b = pdf(objects(b"q 4 0 0 5 7 11 cm /F Do Q /Pic Do"));
    let a = read(&b);
    let d = a[0].images();
    assert_eq!(d.len(), 2);
    assert_eq!(d[0].clips.len(), 1);
    let c = &d[0].clips[0];
    assert_eq!(c.origin, ClipOrigin::FormBox);
    assert_eq!(c.form_box, Some([0., 0., 10., 10.]));
    assert_eq!(
        c.page_quad,
        Some([[47., 289.], [127., 289.], [127., 139.], [47., 139.]])
    );
    assert!(c.geometry_complete);
    assert!(d[1].clips.is_empty());
}
#[test]
fn winding_rule_and_q_restore_keep_path_clips_in_their_own_scope() {
    let a = read(&pdf(objects(b"q 0 0 40 30 re W* n /Pic Do Q /Pic Do")));
    let d = a[0].images();
    assert_eq!(d[0].clips.len(), 1);
    let c = &d[0].clips[0];
    assert_eq!(c.origin, ClipOrigin::Path);
    assert_eq!(c.rule, ClipRule::EvenOdd);
    assert!(c.geometry_complete);
    assert_eq!(
        c.subpaths[0].points,
        vec![[0., 400.], [40., 400.], [40., 370.], [0., 370.]]
    );
    assert!(d[1].clips.is_empty());
}
#[test]
fn curve_clip_without_control_points_is_recorded_as_incomplete_geometry() {
    let a = read(&pdf(objects(b"0 0 m 0 40 40 40 40 0 c W n /Pic Do")));
    let d = &a[0].images()[0];
    assert_eq!(d.clips.len(), 1);
    assert!(!d.clips[0].geometry_complete);
    assert!(!d.source_geometry_complete);
    assert_eq!(d.page_clip_bounds, None);
}
#[test]
fn outer_path_and_implicit_form_bbox_are_both_retained() {
    let a = read(&pdf(objects(b"0 0 100 100 re W n /F Do")));
    let d = &a[0].images()[0];
    assert_eq!(d.clips.len(), 2);
    assert_eq!(d.clips[0].origin, ClipOrigin::Path);
    assert_eq!(d.clips[1].origin, ClipOrigin::FormBox);
}
#[test]
fn payload_and_document_identity_do_not_follow_identical_native_page_values() {
    let mut x = objects(b"/Pic Do");
    let white = pdf(x.clone());
    x[5]=stream(&[0;6],"/Type /XObject /Subtype /Image /Width 2 /Height 3 /ColorSpace /DeviceGray /BitsPerComponent 8");
    let black = pdf(x);
    let a = read(&white);
    let b = read(&black);
    assert_eq!(
        serde_json::to_string(a[0].page()).unwrap(),
        serde_json::to_string(b[0].page()).unwrap()
    );
    assert_eq!(
        a[0].document_sha256(),
        <[u8; 32]>::from(Sha256::digest(&white))
    );
    assert_ne!(a[0].document_sha256(), b[0].document_sha256());
    let da = &a[0].images()[0];
    let db = &b[0].images()[0];
    assert_eq!(da.encoded_bytes(), &[255; 6]);
    assert_eq!(db.encoded_bytes(), &[0; 6]);
    assert_ne!(da.encoded_sha256(), db.encoded_sha256());
    let mut copy = da.encoded_bytes().to_vec();
    copy.fill(0);
    assert_eq!(da.encoded_bytes(), &[255; 6]);
    assert!(da
        .declared_dictionary()
        .contains_key(b"ColorSpace".as_slice()));
}
#[test]
fn noninteger_or_nonpositive_dimensions_are_not_cast_to_valid_pixel_counts() {
    let mut o = objects(b"/Pic Do");
    o[5]=stream(&[255;6],"/Type /XObject /Subtype /Image /Width 2.5 /Height 0 /ColorSpace /DeviceGray /BitsPerComponent 8");
    let a = read(&pdf(o));
    let d = &a[0].images()[0];
    assert_eq!((d.pixel_width, d.pixel_height), (None, None));
    gap(&a[0], GapKind::InvalidImageDimensions);
    assert_eq!(
        d.declared_dictionary().get(b"Width".as_slice()),
        Some(&crate::Object::Real(2.5))
    );
}
#[test]
fn recursive_form_is_not_silently_treated_as_having_no_more_image_sources() {
    let mut o = objects(b"/F Do");
    o[4] = stream(
        b"/F Do /Pic Do",
        "/Subtype /Form /BBox [0 0 10 10] /Resources << /XObject << /F 5 0 R /Pic 6 0 R >> >>",
    );
    let a = read(&pdf(o));
    assert_eq!(a[0].images().len(), 1);
    gap(&a[0], GapKind::RecursiveForm);
}
#[test]
fn a_traversal_depth_limit_must_report_any_unobserved_image() {
    let depth = 20usize;
    let image = 5 + depth;
    let mut o = objects(b"/F Do");
    o.truncate(4);
    for i in 0..depth {
        let next = if i + 1 == depth { image } else { 6 + i };
        o.push(stream(
            b"/Next Do",
            &format!(
                "/Subtype /Form /BBox [0 0 10 10] /Resources << /XObject << /Next {next} 0 R >> >>"
            ),
        ));
    }
    o.push(stream(
        &[255; 6],
        "/Subtype /Image /Width 2 /Height 3 /ColorSpace /DeviceGray /BitsPerComponent 8",
    ));
    let a = read(&pdf(o));
    if a[0].images().is_empty() {
        gap(&a[0], GapKind::FormDepthLimit);
    } else {
        assert_eq!(a[0].images().len(), 1);
    }
}
#[test]
fn inline_image_omission_remains_explicit_alongside_captured_named_images() {
    let a = read(&pdf(objects(
        b"BI /W 2 /H 1 /BPC 8 /CS /G ID ab\nEI /Pic Do",
    )));
    assert_eq!(a[0].images().len(), 1);
    gap(&a[0], GapKind::InlineImage);
}
#[test]
fn missing_named_resource_keeps_a_gap_and_available_later_draw() {
    let a = read(&pdf(objects(b"/Absent Do /Pic Do")));
    assert_eq!(a[0].images().len(), 1);
    gap(&a[0], GapKind::MissingResource);
    assert!(a[0]
        .capture_gaps()
        .iter()
        .any(|g| g.resource_name == b"Absent"));
}
#[test]
fn unsupported_form_decoding_is_reported_without_mutating_legacy_native_values() {
    let mut o = objects(b"/F Do");
    o[4] = stream(
        b"unsupported",
        "/Subtype /Form /BBox [0 0 10 10] /Filter /DCTDecode",
    );
    let a = read(&pdf(o));
    assert!(a[0].images().is_empty());
    gap(&a[0], GapKind::FormDecodeFailure);
}
#[test]
fn unsupported_page_content_is_not_reported_as_a_complete_empty_source_graph() {
    let mut o = objects(b"/Pic Do");
    o[3] = stream(b"unsupported", "/Filter /DCTDecode");
    let a = read(&pdf(o));
    assert!(a[0].images().is_empty());
    gap(&a[0], GapKind::PageContentFailure);
}
#[test]
fn invalid_form_matrix_has_a_gap_instead_of_a_certified_identity_fallback() {
    let mut o = objects(b"/F Do /Pic Do");
    o[4]=stream(b"/Pic Do","/Subtype /Form /BBox [0 0 10 10] /Matrix [2 0 0 3 10] /Resources << /XObject << /Pic 6 0 R >> >>");
    let a = read(&pdf(o));
    gap(&a[0], GapKind::InvalidFormMatrix);
    assert!(!a[0].images()[0].source_geometry_complete);
    assert!(a[0].images()[1].source_geometry_complete);
}
#[test]
fn missing_required_form_bbox_is_not_a_known_unclipped_context() {
    let mut o = objects(b"/F Do");
    o[4] = stream(
        b"/Pic Do",
        "/Subtype /Form /Resources << /XObject << /Pic 6 0 R >> >>",
    );
    let a = read(&pdf(o));
    gap(&a[0], GapKind::InvalidFormBox);
    assert!(!a[0].images()[0].source_geometry_complete);
}
#[test]
fn graphics_state_declarations_are_retained_without_claiming_visible_body() {
    let mut o = objects(b"q /G gs /Pic Do Q /Pic Do");
    o[2]=b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] /Contents 4 0 R /Resources << /XObject << /Pic 6 0 R >> /ExtGState << /G << /ca 0 /BM /Multiply >> >> >> >>".to_vec();
    let a = read(&pdf(o));
    assert_eq!(a[0].images()[0].graphics_states, vec![b"G".to_vec()]);
    assert!(a[0].images()[1].graphics_states.is_empty());
    assert!(!a[0].visibility_verified());
}
#[test]
fn incomplete_content_matrix_keeps_geometry_uncertainty_within_q_scope() {
    let a = read(&pdf(objects(b"q 1 0 0 1 10 cm /Pic Do Q /Pic Do")));
    gap(&a[0], GapKind::InvalidContentMatrix);
    assert!(!a[0].images()[0].source_geometry_complete);
    assert!(a[0].images()[1].source_geometry_complete);
}

#[test]
fn a_known_bbox_with_unrepresentable_transformed_corners_cannot_certify_source_geometry() {
    let huge = format!("1{}", "0".repeat(308));
    let mut o = objects(b"/F Do");
    o[4]=stream(b"/Pic Do",&format!("/Subtype /Form /BBox [0 0 10 10] /Matrix [{huge} 0 0 {huge} 0 0] /Resources << /XObject << /Pic 6 0 R >> >>"));
    let a = read(&pdf(o));
    assert!(!a[0].images()[0].clips[0].geometry_complete);
    assert!(!a[0].images()[0].source_geometry_complete);
    gap(&a[0], GapKind::UnrepresentableGeometry);
}
#[test]
fn an_invalid_explicit_form_resource_dictionary_is_not_known_inherited_scope() {
    let mut o = objects(b"/F Do /Pic Do");
    o[4] = stream(b"/Pic Do", "/Subtype /Form /BBox [0 0 10 10] /Resources 3");
    let a = read(&pdf(o));
    gap(&a[0], GapKind::InvalidFormResources);
    assert!(!a[0].images()[0].source_geometry_complete);
    assert!(a[0].images()[1].source_geometry_complete);
}
#[test]
fn optional_null_dictionary_entries_keep_the_specified_absent_entry_defaults() {
    let mut o = objects(b"/F Do");
    o[4] = stream(
        b"/Pic Do",
        "/Subtype /Form /BBox [0 0 10 10] /Matrix null /Resources null",
    );
    let a = read(&pdf(o));
    assert!(
        a[0].capture_gaps().is_empty(),
        "optional null must use absent-entry defaults: {:?}",
        a[0].capture_gaps()
    );
    assert_eq!(a[0].images()[0].image_to_user, [1., 0., 0., 1., 0., 0.]);
    assert!(a[0].images()[0].source_geometry_complete);
}

#[test]
fn image_source_bundle_preserves_existing_native_aj1_font_evidence() {
    let bytes = include_bytes!(
        "../../kanpo-parser/tests/archive/pages/20260818g00184/20260818g001840038.pdf"
    );
    let old = crate::extract_pages_with_aj1_sources(bytes).unwrap();
    let new = read(bytes);
    let legacy = crate::bbox::bbox_layout_with_aj1_sources(&old);
    let actual =
        crate::bbox::bbox_layout_with_aj1_sources(std::slice::from_ref(new[0].native_sources()));
    assert_eq!(
        actual, legacy,
        "image source collection must not erase source-verified native font candidates"
    );
}

#[test]
fn form_clip_uses_effective_crop_origin_and_page_rotation() {
    let mut o = objects(b"/F Do");
    o[2]=b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] /CropBox [10 20 210 380] /Rotate 90 /Contents 4 0 R /Resources << /XObject << /F 5 0 R /Pic 6 0 R >> >> >>".to_vec();
    let a = read(&pdf(o));
    assert_eq!(a[0].media_box(), [0., 0., 300., 400.]);
    assert_eq!(a[0].crop_box(), [10., 20., 210., 380.]);
    assert_eq!((a[0].page().width, a[0].page().height), (360., 200.));
    let c = &a[0].images()[0].clips[0];
    assert_eq!(
        c.page_quad,
        Some([[0., 0.], [0., 20.], [30., 20.], [30., 0.]])
    );
}
