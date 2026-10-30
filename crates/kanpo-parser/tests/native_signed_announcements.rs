//! Complete original signed publications and immutable source controls.
use serde_json::Value;
fn root() -> std::path::PathBuf {
    env!("CARGO_MANIFEST_DIR").into()
}
fn data() -> std::path::PathBuf {
    root().join("tests/data/native-signed-announcements")
}
fn index() -> Value {
    serde_json::from_str(include_str!("data/native-signed-announcements/index.json")).unwrap()
}
fn input(c: &Value) -> Vec<kanpo_parser::PageInput> {
    kanpo_parser::page_inputs(&std::fs::read(root().join(c["PDF"].as_str().unwrap())).unwrap())
        .unwrap()
}
fn actual(input: &[kanpo_parser::PageInput]) -> Value {
    serde_json::from_str(&serde_json::to_string(&kanpo_parser::analyze_item(input)).unwrap())
        .unwrap()
}
fn gold(folder: &str, key: &str) -> Value {
    serde_json::from_slice(&std::fs::read(data().join(folder).join(format!("{key}.json"))).unwrap())
        .unwrap()
}
/// The stored golds stay as verified by `verify_expected.py`. 20260729h017570006 now keeps its 叙位・叙勲 section
/// (one TOC item) as one article: `expected/20260729h017570006-personnel263n.json` is the stored gold with
/// articles 1 (「叙位・叙勲 / 〇叙位」) and 2 (「〇叙勲」) joined.
fn expected(key: &str) -> Value {
    match key {
        "20260729h017570006" => gold("expected", "20260729h017570006-personnel263n"),
        _ => gold("expected", key),
    }
}
#[test]
fn complete_original_signed_announcements_keep_all_source_material() {
    for c in index()["cases"].as_array().unwrap() {
        let key = c["key"].as_str().unwrap();
        assert!(
            actual(&input(c)) == expected(key),
            "{key}: complete source-cut Analysis"
        );
    }
}
#[test]
fn an_existing_coauthored_committee_and_the_following_exam_remain_exact() {
    let c = &index()["unchanged_control"];
    assert!(actual(&input(c)) == gold("control", c["key"].as_str().unwrap()));
}
fn reverse_words(html: &str) -> String {
    let (mut spans, mut cursor) = (vec![], 0);
    while let Some(at) = html[cursor..].find("<word ") {
        let start = cursor + at;
        let end = start + html[start..].find("</word>").unwrap() + 7;
        spans.push((start, end));
        cursor = end;
    }
    let mut out = String::new();
    cursor = 0;
    for (&(start, end), &(a, b)) in spans.iter().zip(spans.iter().rev()) {
        out.push_str(&html[cursor..start]);
        out.push_str(&html[a..b]);
        cursor = end;
    }
    out.push_str(&html[cursor..]);
    out
}
#[test]
fn complete_signed_publications_do_not_depend_on_word_node_order() {
    for c in index()["cases"].as_array().unwrap() {
        let mut input = input(c);
        for p in &mut input {
            p.xhtml = reverse_words(&p.xhtml);
        }
        let key = c["key"].as_str().unwrap();
        assert!(actual(&input) == expected(key), "{key}");
    }
}
#[test]
fn a_caption_without_its_own_complete_original_glyph_styles_keeps_the_baseline() {
    let index = index();
    let c = index["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["key"] == "20260807g001780069")
        .unwrap();
    let key = c["key"].as_str().unwrap();
    let baseline = gold("baseline", key);
    let id = c["source"]["source_words"][0].as_u64().unwrap() as usize;
    let raw = baseline["pages"][0]["words"][id]["raw"]
        .as_str()
        .unwrap()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let mut input = input(c);
    let html = &mut input[0].xhtml;
    let needle = format!(">{raw}</word>");
    assert_eq!(html.match_indices(&needle).count(), 1);
    let end = html.find(&needle).unwrap();
    let start = html[..end].rfind("<word ").unwrap();
    let name = " data-native-styles=\"";
    let from = start + html[start..end].find(name).unwrap();
    let value = from + name.len();
    let after = value + html[value..end].find('"').unwrap() + 1;
    html.replace_range(from..after, "");
    assert!(actual(&input) == baseline);
}
