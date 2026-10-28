use super::*;
struct Fixture {
    html: String,
    trace: PageTrace,
    rules: PageRules,
}
fn fixture() -> &'static Fixture {
    static F: std::sync::OnceLock<Fixture> = std::sync::OnceLock::new();
    F.get_or_init(|| {
        let i = crate::page_inputs(include_bytes!(
            "../tests/data/native-source-paragraphs/20260731g001720011.pdf"
        ))
        .unwrap()
        .remove(0);
        let trace =
            crate::vertical::analyze_pages_with_rules(&i.xhtml, &[i.rules.clone()]).remove(0);
        Fixture {
            html: i.xhtml,
            trace,
            rules: i.rules.unwrap(),
        }
    })
}
fn fixture22() -> &'static Fixture {
    static F: std::sync::OnceLock<Fixture> = std::sync::OnceLock::new();
    F.get_or_init(|| {
        let i = crate::page_inputs(include_bytes!(
            "../tests/data/native-source-paragraphs/20260731g001720022.pdf"
        ))
        .unwrap()
        .remove(0);
        let trace =
            crate::vertical::analyze_pages_with_rules(&i.xhtml, &[i.rules.clone()]).remove(0);
        Fixture {
            html: i.xhtml,
            trace,
            rules: i.rules.unwrap(),
        }
    })
}
#[test]
fn multiple_original_word_primitives_can_form_one_source_column() {
    let f = fixture22();
    let d = scraper::Html::parse_document(&f.html);
    let p = d
        .select(&scraper::Selector::parse("page").unwrap())
        .next()
        .unwrap();
    assert!(for_page(&f.trace,p,&f.rules).contains(&(57..73)),"the next original column has two contiguous glyph primitives, with its exact original tracking gap; that cannot erase the paragraph witness");
}
#[test]
fn original_border_paint_does_not_shrink_the_source_geometric_frame() {
    let f = fixture22();
    let d = scraper::Html::parse_document(&f.html);
    let p = d
        .select(&scraper::Selector::parse("page").unwrap())
        .next()
        .unwrap();
    let s = crate::native_source_scene::for_page(p, &f.rules).unwrap();
    assert_eq!(s.paragraph_frame([226.1201,428.8398,414.0881,792.8398]),Some([425.,793.]),"the original em box is inside the geometric border even when it overlaps part of the border's actual paint");
    assert_eq!(
        s.paragraph_frame([226.1201, 428.8398, 414.0881, 793.01]),
        None,
        "a source box crossing the actual geometric fence must still refuse"
    );
}
#[test]
fn a_partial_coincident_stroke_is_not_an_original_complete_paragraph_fence() {
    let f = fixture22();
    let d = scraper::Html::parse_document(&f.html);
    let p = d
        .select(&scraper::Selector::parse("page").unwrap())
        .next()
        .unwrap();
    let mut v: serde_json::Value =
        serde_json::from_str(p.value().attr("data-native-scene").unwrap()).unwrap();
    v["lines"].as_array_mut().unwrap().push(serde_json::json!([
        [250., 793., 300., 793.],
        [249.9, 792.7, 300.1, 793.3]
    ]));
    let encoded = v
        .to_string()
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let h = rewrite_all(&f.html, "data-native-scene", Some(&encoded));
    let d = scraper::Html::parse_document(&h);
    let p = d
        .select(&scraper::Selector::parse("page").unwrap())
        .next()
        .unwrap();
    let s = crate::native_source_scene::for_page(p, &f.rules).unwrap();
    assert_eq!(
        s.paragraph_frame([226.1201, 428.8398, 414.0881, 792.8398]),
        None
    );
}
fn read(h: &str, r: &PageRules) -> Vec<Range<usize>> {
    let d = scraper::Html::parse_document(h);
    for_page(
        &fixture().trace,
        d.select(&scraper::Selector::parse("page").unwrap())
            .next()
            .unwrap(),
        r,
    )
}
fn rewrite_all(h: &str, name: &str, value: Option<&str>) -> String {
    let mut out = h.to_string();
    let needle = format!(" {name}=\"");
    let mut cursor = 0;
    while let Some(pos) = out[cursor..].find(&needle) {
        let at = cursor + pos;
        let from = at + needle.len();
        let end = from + out[from..].find('"').unwrap() + 1;
        let replacement = value.map(|v| format!("{needle}{v}\"")).unwrap_or_default();
        out.replace_range(at..end, &replacement);
        cursor = at + replacement.len();
    }
    out
}
fn scene_change(change: impl FnOnce(&mut serde_json::Value)) -> String {
    let f = fixture();
    let d = scraper::Html::parse_document(&f.html);
    let p = d
        .select(&scraper::Selector::parse("page").unwrap())
        .next()
        .unwrap();
    let mut v: serde_json::Value =
        serde_json::from_str(p.value().attr("data-native-scene").unwrap()).unwrap();
    change(&mut v);
    let encoded = v
        .to_string()
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    rewrite_all(&f.html, "data-native-scene", Some(&encoded))
}
#[test]
fn original_native_capacity_and_indent_bound_the_two_complete_edit_paragraphs() {
    let f = fixture();
    assert_eq!(read(&f.html, &f.rules), vec![47..55, 55..63]);
}
#[test]
fn missing_source_scene_never_means_empty_paint() {
    let f = fixture();
    assert!(read(&rewrite_all(&f.html, "data-native-scene", None), &f.rules).is_empty());
}
#[test]
fn unknown_source_glyphs_do_not_authorize_paragraph_scope() {
    let f = fixture();
    let h = scene_change(|v| v["unknown"] = serde_json::json!([0]));
    assert!(read(&h, &f.rules).is_empty());
}
#[test]
fn images_and_fills_in_the_body_prevent_literal_scope_evidence() {
    let f = fixture();
    for key in ["images", "fills"] {
        let h = scene_change(|v| v[key] = serde_json::json!([[40., 426., 550., 792.]]));
        assert!(read(&h, &f.rules).is_empty(), "{key}");
    }
}
#[test]
fn internal_source_strokes_are_not_removed_as_paragraph_borders() {
    let f = fixture();
    let h = scene_change(|v| {
        v["lines"].as_array_mut().unwrap().push(serde_json::json!([
            [350., 430., 350., 785.],
            [349.75, 430., 350.25, 785.]
        ]))
    });
    assert!(!read(&h, &f.rules).contains(&(47..55)));
}
#[test]
fn native_original_top_and_bottom_fences_are_required() {
    let f = fixture();
    for y in [425., 793.] {
        let mut r = f.rules.clone();
        r.horizontal.retain(|h| h.y != y);
        assert!(read(&f.html, &r).is_empty(), "missing actual y{y}");
    }
}
#[test]
fn missing_axes_or_source_ranges_do_not_become_compatibility_geometry() {
    let f = fixture();
    for key in [
        "data-native-axis",
        "data-native-flow-first-glyph",
        "data-native-flow-after-glyph",
    ] {
        assert!(
            read(&rewrite_all(&f.html, key, None), &f.rules).is_empty(),
            "{key}"
        );
    }
}
#[test]
fn invalid_native_source_frames_cannot_bound_a_literal_operation() {
    let f = fixture();
    for value in ["NaN", "inf", "-inf"] {
        assert!(read(
            &rewrite_all(&f.html, "data-native-xmin", Some(value)),
            &f.rules
        )
        .is_empty());
    }
}
#[test]
fn changed_dom_source_styles_cannot_keep_an_old_page_certificate() {
    let f = fixture();
    let h = rewrite_all(&f.html, "data-native-styles", Some("999:4020000000000000"));
    assert!(read(&h, &f.rules).is_empty());
}
#[test]
fn changed_original_raw_text_cannot_retain_the_old_scene() {
    let f = fixture();
    assert!(read(&f.html.replace("</word>", "x</word>"), &f.rules).is_empty());
}
fn columns(n: usize, k: f64, dx: f64, dy: f64) -> Vec<Option<Column>> {
    (0..=n)
        .map(|i| {
            let x = (12 * (n + 2 - i)) as f64;
            let start = if i == 0 || i == n { 18. } else { 10. };
            let end = if i + 1 == n { 60. } else { 100. };
            Some(Column {
                ids: vec![i],
                scope: 0,
                bounds: [
                    (x * k + dx - 4. * k) as f32,
                    (start * k + dy) as f32,
                    (x * k + dx + 4. * k) as f32,
                    (end * k + dy) as f32,
                ],
                path: [x * k + dx, start * k + dy, end * k + dy],
                steps: [8. * k, 0., 0.],
                range: (i * 20, (i + 1) * 20),
                styles: NativeStyles::from([(7, (8. * k).to_bits())]),
            })
        })
        .collect()
}
fn proved(c: &[Option<Column>], bottom: f64) -> Vec<Range<usize>> {
    candidates(c)
        .into_iter()
        .filter(|r| {
            capacity(
                &c[r.start..=r.end]
                    .iter()
                    .map(|x| x.as_ref().unwrap())
                    .collect::<Vec<_>>(),
                bottom,
            )
        })
        .collect()
}
#[test]
fn complete_source_column_counts_and_coordinate_scale_do_not_supply_budgets() {
    for n in [2, 3, 17, 65] {
        for k in [0.125, 1., 8.] {
            let c = columns(n, k, -13., -23.);
            assert_eq!(proved(&c, 104. * k - 23.), vec![0..n]);
            assert!(capacity(
                &c.iter().map(|x| x.as_ref().unwrap()).collect::<Vec<_>>(),
                104. * k - 23.
            ));
        }
    }
}
#[test]
fn source_font_identity_is_an_equality_relation_not_a_named_font_rule() {
    for font in [0, 7, 999] {
        let mut c = columns(3, 1., 0., 0.);
        for x in c.iter_mut().flatten() {
            x.styles = NativeStyles::from([(font, 8f64.to_bits())]);
        }
        assert_eq!(proved(&c, 104.), vec![0..3]);
    }
}
#[test]
fn an_unfilled_earlier_line_is_not_a_capacity_wrap() {
    let c = columns(3, 1., 0., 0.);
    let cs = c.iter().map(|x| x.as_ref().unwrap()).collect::<Vec<_>>();
    assert!(
        !capacity(&cs, 108.),
        "the next complete glyph still fits, so the line did not have to wrap"
    );
}
#[test]
fn source_kerning_and_tracking_cannot_be_discarded_to_create_a_paragraph() {
    let mut c = columns(3, 1., 0., 0.);
    c[1].as_mut().unwrap().steps[2] = -8.;
    assert!(!capacity(
        &c.iter().map(|x| x.as_ref().unwrap()).collect::<Vec<_>>(),
        104.
    ));
    let mut c = columns(3, 1., 0., 0.);
    c[2].as_mut().unwrap().steps[1] = 80.;
    assert!(
        !capacity(
            &c.iter().map(|x| x.as_ref().unwrap()).collect::<Vec<_>>(),
            104.
        ),
        "a last line without capacity does not prove an intentional paragraph end"
    );
}
#[test]
fn hanging_indents_and_a_different_next_indent_do_not_close_one_source_paragraph() {
    for index in [1, 3] {
        let mut c = columns(3, 1., 0., 0.);
        c[index].as_mut().unwrap().path[1] += 3.;
        assert!(proved(&c, 104.).is_empty());
    }
}
#[test]
fn source_range_gaps_crossing_columns_or_other_regions_cannot_supply_a_witness() {
    let mut c = columns(3, 1., 0., 0.);
    c[1].as_mut().unwrap().range.0 += 1;
    assert!(proved(&c, 104.).is_empty());
    let mut c = columns(3, 1., 0., 0.);
    c[1].as_mut().unwrap().bounds[2] = c[0].as_ref().unwrap().bounds[2];
    assert!(proved(&c, 104.).is_empty());
    let mut c = columns(3, 1., 0., 0.);
    c[1].as_mut().unwrap().scope = 1;
    assert!(proved(&c, 104.).is_empty());
}
#[test]
fn a_full_last_column_or_a_missing_witness_is_an_unfinished_source_scope() {
    let mut c = columns(3, 1., 0., 0.);
    c[2].as_mut().unwrap().path[2] = 100.;
    assert!(proved(&c, 104.).is_empty());
    let mut c = columns(3, 1., 0., 0.);
    c.pop();
    assert!(proved(&c, 104.).is_empty());
}
#[test]
fn a_short_last_line_must_fit_the_next_full_glyph_not_just_its_short_advance() {
    let mut c = columns(2, 1., 0., 0.);
    c[1].as_mut().unwrap().path[2] = 99.;
    c[1].as_mut().unwrap().bounds[3] = 99.;
    c[2].as_mut().unwrap().steps[0] = 4.;
    assert!(!capacity(&c.iter().map(|x|x.as_ref().unwrap()).collect::<Vec<_>>(),106.),"the 8-unit original glyph envelope still crosses the source paint, even when its advance is 4");
}

fn primitive(i: usize) -> Column {
    let y = 10. + 16. * i as f64;
    Column {
        ids: vec![i],
        scope: 0,
        bounds: [96., y as f32, 104., (y + 8.) as f32],
        path: [100., y, y + 8.],
        steps: [8., 8., 0.],
        range: (i, i + 1),
        styles: NativeStyles::from([(7, 8f64.to_bits())]),
    }
}
#[test]
fn original_column_joins_have_no_primitive_count_budget() {
    for n in [2, 3, 17, 65] {
        let mut c = primitive(0);
        for i in 1..n {
            assert!(join(&mut c, primitive(i)));
        }
        assert_eq!(c.ids, (0..n).collect::<Vec<_>>());
        assert_eq!(c.range, (0, n));
        assert_eq!(c.path, [100., 10., primitive(n - 1).path[2]]);
    }
}
#[test]
fn source_column_joins_require_the_actual_tracking_and_prefix() {
    for mismatch in [-8., 8.] {
        let mut a = primitive(0);
        let mut b = primitive(1);
        b.steps[2] = mismatch;
        assert!(!join(&mut a, b));
        assert_eq!(a.ids, vec![0]);
    }
    let mut a = primitive(0);
    let b = primitive(1);
    a.steps[1] = 0.;
    assert!(!join(&mut a, b));
}
#[test]
fn a_source_column_join_cannot_cross_identity_geometry_or_style() {
    for kind in 0..5 {
        let mut a = primitive(0);
        let mut b = primitive(1);
        match kind {
            0 => b.scope = 1,
            1 => b.range.0 += 1,
            2 => b.path[0] += 1.,
            3 => b.bounds[1] = a.bounds[3] - 1.,
            _ => b.styles = NativeStyles::from([(8, 8f64.to_bits())]),
        }
        assert!(!join(&mut a, b), "mismatch {kind}");
        assert_eq!(a.ids, vec![0]);
    }
}
#[test]
fn original_blank_tracking_space_is_not_proof_that_no_image_is_present() {
    let f = fixture22();
    let d = scraper::Html::parse_document(&f.html);
    let p = d
        .select(&scraper::Selector::parse("page").unwrap())
        .next()
        .unwrap();
    let mut v: serde_json::Value =
        serde_json::from_str(p.value().attr("data-native-scene").unwrap()).unwrap();
    // The eight-unit original gap between the next paragraph's two primitives.
    v["images"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!([227., 445., 229., 452.]));
    let encoded = v
        .to_string()
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let h = rewrite_all(&f.html, "data-native-scene", Some(&encoded));
    let d = scraper::Html::parse_document(&h);
    let p = d
        .select(&scraper::Selector::parse("page").unwrap())
        .next()
        .unwrap();
    let scene = crate::native_source_scene::for_page(p, &f.rules).unwrap();
    assert_eq!(
        scene.paragraph_frame([226.1201, 428.8398, 414.0881, 792.8398]),
        None
    );
}
