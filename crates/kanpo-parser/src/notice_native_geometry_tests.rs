//! The published compatibility rectangle is not an ownership certificate when
//! the source already supplies a different original-glyph rectangle.
use crate::notice_boundaries::cuts;
use crate::trace::WordStatus;
fn input(compat: [f32; 4], native: [f32; 4]) -> crate::PageInput {
    let mut xhtml = String::from("<html><body><doc><page width='600' height='800'>");
    for (text, b, n, font) in [
        ("任意の表題", compat, native, 1),
        (
            "2026年7月1日",
            [100., 70., 240., 80.],
            [100., 70., 240., 80.],
            2,
        ),
        (
            "清算人任意",
            [100., 90., 240., 100.],
            [100., 90., 240., 100.],
            2,
        ),
    ] {
        xhtml.push_str(&format!("<flow><block><line><word xmin='{}' ymin='{}' xmax='{}' ymax='{}' data-wmode='0' data-native-xmin='{}' data-native-ymin='{}' data-native-xmax='{}' data-native-ymax='{}' data-native-styles='{font}:4020000000000000'>{text}</word></line></block></flow>",b[0],b[1],b[2],b[3],n[0],n[1],n[2],n[3]));
    }
    xhtml.push_str("</page></doc></body></html>");
    let mut rules = crate::lines::PageRules::default();
    rules.enclosures = vec![[90., 40., 250., 110.]];
    crate::PageInput {
        xhtml,
        rules: Some(rules),
    }
}
fn check(input: &crate::PageInput) -> Vec<usize> {
    let a = crate::analyze_item(std::slice::from_ref(input));
    assert_eq!(a.pages.len(), 1);
    assert_eq!(
        a.pages[0]
            .words
            .iter()
            .filter(|w| matches!(w.status, WordStatus::Used))
            .count(),
        3
    );
    cuts(&a.pages[0], input.rules.as_ref().unwrap())
}
#[test]
fn original_bounds_inside_the_frame_do_not_require_a_compatibility_box_tolerance() {
    let p = input([260., 50., 400., 60.], [100., 50., 240., 60.]);
    assert_eq!(check(&p), vec![0]);
}
#[test]
fn a_compatibility_box_inside_cannot_override_original_bounds_outside() {
    let p = input([100., 50., 240., 60.], [260., 50., 400., 60.]);
    assert!(check(&p).is_empty());
}
#[test]
fn incomplete_or_nonfinite_explicit_native_geometry_cannot_authorize_a_frame() {
    let p = input([100., 50., 240., 60.], [100., 50., 240., 60.]);
    for bad in ["NaN", "inf", "-inf", "not-a-number"] {
        let mut q = p.clone();
        q.xhtml = q.xhtml.replacen(
            "data-native-xmin='100'",
            &format!("data-native-xmin='{bad}'"),
            1,
        );
        assert!(check(&q).is_empty(), "{bad}");
    }
    let mut q = p;
    q.xhtml = q.xhtml.replacen("data-native-xmin='100'", "", 1);
    assert!(check(&q).is_empty());
}
#[test]
fn native_ownership_metadata_keeps_published_word_coordinates_and_text_exact() {
    let p = input([260., 50., 400., 60.], [100., 50., 240., 60.]);
    let mut q = p.clone();
    q.xhtml = q
        .xhtml
        .replace("data-native-x", "unused-native-x")
        .replace("data-native-y", "unused-native-y");
    let a = crate::analyze_item(&[p]);
    let b = crate::analyze_item(&[q]);
    assert_eq!(
        serde_json::to_value(&a.pages).unwrap(),
        serde_json::to_value(&b.pages).unwrap()
    );
    assert_eq!(a.text, b.text);
}

#[test]
fn degenerate_or_inverted_native_rectangles_are_not_compatibility_fallbacks() {
    for b in [
        [100., 50., 100., 60.],
        [240., 50., 100., 60.],
        [100., 60., 240., 50.],
        [100., 50., 240., 50.],
    ] {
        assert!(check(&input([100., 50., 240., 60.], b)).is_empty(), "{b:?}");
    }
}
#[test]
fn unknown_writing_mode_or_missing_vertical_geometry_cannot_authorize_a_frame() {
    let p = input([100., 50., 240., 60.], [100., 50., 240., 60.]);
    for mode in ["2", "unknown"] {
        let mut q = p.clone();
        q.xhtml = q
            .xhtml
            .replace("data-wmode='0'", &format!("data-wmode='{mode}'"));
        assert!(check(&q).is_empty());
    }
    let mut q = p;
    q.xhtml = q
        .xhtml
        .replace("data-wmode='0'", "data-wmode='1'")
        .replace("data-native-x", "unused-native-x")
        .replace("data-native-y", "unused-native-y");
    assert!(check(&q).is_empty());
}
#[test]
fn legacy_or_horizontal_words_without_explicit_bounds_retain_the_old_geometry() {
    let mut p = input([100., 50., 240., 60.], [100., 50., 240., 60.]);
    p.xhtml = p
        .xhtml
        .replace("data-native-x", "unused-native-x")
        .replace("data-native-y", "unused-native-y");
    assert_eq!(check(&p), vec![0]);
    p.xhtml = p.xhtml.replace("data-wmode='0'", "");
    assert_eq!(check(&p), vec![0]);
}
#[test]
fn conflicting_geometry_for_identical_source_occurrences_abstains_in_either_input_order() {
    let p = input([100., 50., 240., 60.], [100., 50., 240., 60.]);
    let start = p.xhtml.find("<flow>").unwrap();
    let end = p.xhtml.find("</flow>").unwrap() + "</flow>".len();
    let source = &p.xhtml[start..end];
    let conflicting = source
        .replace("data-native-xmin='100'", "data-native-xmin='260'")
        .replace("data-native-xmax='240'", "data-native-xmax='400'");
    for words in [
        format!("{source}{conflicting}"),
        format!("{conflicting}{source}"),
    ] {
        let mut q = p.clone();
        q.xhtml.replace_range(start..end, &words);
        let a = crate::analyze_item(&[q.clone()]);
        assert!(cuts(&a.pages[0], q.rules.as_ref().unwrap()).is_empty());
    }
}
#[test]
fn source_metadata_stays_attached_when_word_emission_order_is_reversed() {
    let p = input([260., 50., 400., 60.], [100., 50., 240., 60.]);
    let start = p.xhtml.find("<flow>").unwrap();
    let end = p.xhtml.rfind("</flow>").unwrap() + "</flow>".len();
    let mut flows: Vec<_> = p.xhtml[start..end].split_inclusive("</flow>").collect();
    flows.reverse();
    let mut q = p.clone();
    q.xhtml.replace_range(start..end, &flows.concat());
    assert_eq!(check(&p), vec![0]);
    assert_eq!(check(&q), vec![0]);
    assert_eq!(
        serde_json::to_value(crate::analyze_item(&[p])).unwrap(),
        serde_json::to_value(crate::analyze_item(&[q])).unwrap()
    );
}
#[test]
fn ownership_uses_transformed_original_bounds_without_a_distance_or_scale_threshold() {
    let p = input([260., 50., 400., 60.], [100., 50., 240., 60.]);
    let baseline = crate::analyze_item(&[p.clone()]);
    for k in [0.125, 1., 8.] {
        let mut page = baseline.pages[0].clone();
        let mut rules = p.rules.clone().unwrap();
        for w in &mut page.words {
            w.x0 = w.x0 * k - 100.;
            w.x1 = w.x1 * k - 100.;
            w.y0 = w.y0 * k - 200.;
            w.y1 = w.y1 * k - 200.;
        }
        for b in &mut page.native_bounds {
            if let crate::trace::NativeBounds::Glyphs(b) = b {
                b[0] = b[0] * k - 100.;
                b[2] = b[2] * k - 100.;
                b[1] = b[1] * k - 200.;
                b[3] = b[3] * k - 200.;
            }
        }
        for b in &mut rules.enclosures {
            b[0] = b[0] * k - 100.;
            b[2] = b[2] * k - 100.;
            b[1] = b[1] * k - 200.;
            b[3] = b[3] * k - 200.;
        }
        assert_eq!(cuts(&page, &rules), vec![0]);
    }
}

#[test]
fn conflicting_source_geometry_does_not_reassign_word_ids_between_independent_frames() {
    let p = input([500., 50., 590., 60.], [100., 50., 240., 60.]);
    let start = p.xhtml.find("<flow>").unwrap();
    let end = p.xhtml.rfind("</flow>").unwrap() + "</flow>".len();
    let flows: Vec<_> = p.xhtml[start..end].split_inclusive("</flow>").collect();
    assert_eq!(flows.len(), 3);
    let title2 = flows[0]
        .replace("data-native-xmin='100'", "data-native-xmin='350'")
        .replace("data-native-xmax='240'", "data-native-xmax='490'");
    let right = |s: &str| {
        s.replace("xmin='100'", "xmin='350'")
            .replace("xmax='240'", "xmax='490'")
    };
    let date2 = right(flows[1]);
    let signature2 = right(flows[2]);
    let mut values = vec![];
    for titles in [
        format!("{}{title2}", flows[0]),
        format!("{title2}{}", flows[0]),
    ] {
        let mut q = p.clone();
        q.xhtml.replace_range(
            start..end,
            &format!("{titles}{}{date2}{}{signature2}", flows[1], flows[2]),
        );
        q.xhtml = q.xhtml.replace("width='600'", "width='800'");
        q.rules
            .as_mut()
            .unwrap()
            .enclosures
            .push([340., 40., 500., 110.]);
        values.push(serde_json::to_value(crate::analyze_item(&[q])).unwrap());
    }
    assert!(
        values[0] == values[1],
        "ambiguous original occurrences changed ownership under source emission reversal"
    );
}
