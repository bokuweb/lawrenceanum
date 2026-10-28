use super::*;
fn fixture() -> (String, PageRules) {
    static INPUT: std::sync::OnceLock<(String, PageRules)> = std::sync::OnceLock::new();
    INPUT
        .get_or_init(|| {
            let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/archive/pages/20260807g00178/20260807g001780121.pdf");
            let i = crate::page_inputs(&std::fs::read(p).unwrap())
                .unwrap()
                .remove(0);
            (i.xhtml, i.rules.unwrap())
        })
        .clone()
}
fn packet(html: &str) -> serde_json::Value {
    let d = scraper::Html::parse_document(html);
    let p = d
        .select(&scraper::Selector::parse("page").unwrap())
        .next()
        .unwrap();
    serde_json::from_str(p.value().attr("data-native-scene").unwrap()).unwrap()
}
fn replace_attr(html: &str, key: &str, value: Option<&str>) -> String {
    let mut s = html.to_string();
    let needle = format!(" {key}=\"");
    let a = s.find(&needle).unwrap();
    let start = a + needle.len();
    let end = start + s[start..].find('"').unwrap() + 1;
    let text = value.map(|v| format!("{needle}{v}\"")).unwrap_or_default();
    s.replace_range(a..end, &text);
    s
}
fn rewrite(html: &str, v: &serde_json::Value) -> String {
    replace_attr(
        html,
        "data-native-scene",
        Some(
            &v.to_string()
                .replace('&', "&amp;")
                .replace('"', "&quot;")
                .replace('<', "&lt;")
                .replace('>', "&gt;"),
        ),
    )
}
fn accepted(html: &str, r: &PageRules) -> bool {
    let d = scraper::Html::parse_document(html);
    for_page(
        d.select(&scraper::Selector::parse("page").unwrap())
            .next()
            .unwrap(),
        r,
    )
    .is_some()
}
#[test]
fn original_complete_v2_keeps_every_nonflow_envelope_opaque() {
    let (h, r) = fixture();
    let p = packet(&h);
    assert_eq!(p["version"], 2);
    let d = scraper::Html::parse_document(&h);
    let s = for_page(
        d.select(&scraper::Selector::parse("page").unwrap())
            .next()
            .unwrap(),
        &r,
    )
    .unwrap();
    for w in p["word_obstacles"].as_array().unwrap() {
        let b: [f64; 4] = serde_json::from_value(w[2].clone()).unwrap();
        assert!(!s.unobstructed(b.map(|x| x as f32)));
    }
}
#[test]
fn v2_obstacles_are_not_silently_accepted_as_legacy_v1() {
    let (h, r) = fixture();
    let mut p = packet(&h);
    p["version"] = 1.into();
    assert!(!accepted(&rewrite(&h, &p), &r));
}
#[test]
fn missing_and_duplicated_opaque_records_are_rejected() {
    let (h, r) = fixture();
    let p = packet(&h);
    let mut changed = p.clone();
    changed.as_object_mut().unwrap().remove("word_obstacles");
    assert!(!accepted(&rewrite(&h, &changed), &r));
    let mut changed = p;
    let first = changed["word_obstacles"][0].clone();
    changed["word_obstacles"]
        .as_array_mut()
        .unwrap()
        .push(first);
    assert!(!accepted(&rewrite(&h, &changed), &r));
}
#[test]
fn each_opaque_envelope_must_match_its_own_word_attribute() {
    let (h, r) = fixture();
    let mut p = packet(&h);
    let x = p["word_obstacles"][0][2][0].as_f64().unwrap();
    p["word_obstacles"][0][2][0] = (x - 1.).into();
    assert!(!accepted(&rewrite(&h, &p), &r));
}
#[test]
fn an_opaque_record_does_not_make_a_missing_occurrence_attribute_optional() {
    let (h, r) = fixture();
    assert!(!accepted(
        &replace_attr(&h, "data-native-source-occurrence", None),
        &r
    ));
}
#[test]
fn native_flow_and_nonflow_certificates_cannot_compete_for_the_same_word() {
    let (h, r) = fixture();
    let changed = h.replacen(
        "data-native-source-occurrence=\"",
        "data-native-flow-first-glyph=\"0\" data-native-source-occurrence=\"",
        1,
    );
    assert!(!accepted(&changed, &r));
}
#[test]
fn changing_a_source_word_cannot_keep_its_opaque_range() {
    let (h, r) = fixture();
    let at = h.find("data-native-source-occurrence=").unwrap();
    let start = at + h[at..].find('>').unwrap() + 1;
    let mut changed = h;
    changed.insert(start, '別');
    assert!(!accepted(&changed, &r));
}
#[test]
fn a_coherently_changed_envelope_still_has_to_contain_the_original_public_box() {
    let (h, r) = fixture();
    let mut p = packet(&h);
    let b = &mut p["word_obstacles"][0][2];
    let width = b[2].as_f64().unwrap() - b[0].as_f64().unwrap();
    b[0] = (b[0].as_f64().unwrap() + width).into();
    b[2] = (b[2].as_f64().unwrap() + width).into();
    let v = &p["word_obstacles"][0];
    let b = &v[2];
    let attr = format!("1 {} {} {} {} {} {}", v[0], v[1], b[0], b[1], b[2], b[3]);
    let changed = replace_attr(
        &rewrite(&h, &p),
        "data-native-source-occurrence",
        Some(&attr),
    );
    assert!(!accepted(&changed, &r));
}
#[test]
fn opaque_source_occurrences_do_not_depend_on_xhtml_word_emission_order() {
    let (h, r) = fixture();
    let mut nodes = vec![];
    let mut cursor = 0;
    while let Some(at) = h[cursor..].find("<word ") {
        let start = cursor + at;
        let end = start + h[start..].find("</word>").unwrap() + 7;
        nodes.push((start, end));
        cursor = end;
    }
    let mut out = String::new();
    cursor = 0;
    for ((start, end), (from, to)) in nodes.iter().zip(nodes.iter().rev()) {
        out.push_str(&h[cursor..*start]);
        out.push_str(&h[*from..*to]);
        cursor = *end;
    }
    out.push_str(&h[cursor..]);
    assert!(accepted(&out, &r));
}
