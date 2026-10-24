//! Original printed section root. Expected Documents copy the adopted source
//! Runs split at the independently reviewed masthead, keeping the whole Table.
use kanpo_parser::{analyze_item, page_inputs, PageInput};
use serde_json::Value;
fn input() -> Vec<PageInput> {
    page_inputs(include_bytes!(
        "data/native-section-roots/20260930g002150475.pdf"
    ))
    .unwrap()
}
fn old() -> Value {
    serde_json::from_str(include_str!("data/native-section-roots/baseline125.json")).unwrap()
}
fn expected() -> Value {
    serde_json::from_str(include_str!("data/native-section-roots/expected126.json")).unwrap()
}
fn value(p: &[PageInput]) -> Value {
    serde_json::from_str(&serde_json::to_string(&analyze_item(p)).unwrap()).unwrap()
}
#[test]
fn original_primary_section_and_whole_financial_table_belong_to_their_own_article() {
    assert_eq!(value(&input()), expected());
}
fn words(p: &[PageInput]) -> Vec<String> {
    let mut rest = p[0].xhtml.as_str();
    let mut out = vec![];
    while let Some(at) = rest.find("<word ") {
        rest = &rest[at..];
        let end = rest.find("</word>").unwrap() + "</word>".len();
        out.push(rest[..end].into());
        rest = &rest[end..];
    }
    assert!(!out.is_empty());
    out
}
fn only_words(p: &mut [PageInput], words: &[String]) {
    p[0].xhtml = format!(
        "<html><body><doc><page width=\"595\" height=\"842\">{}</page></doc></body></html>",
        words.join("\n")
    );
}
#[test]
fn original_word_emission_order_does_not_choose_the_section_boundary() {
    for interleaved in [false, true] {
        let mut p = input();
        let mut ws = words(&p);
        if interleaved {
            let (a, b): (Vec<_>, Vec<_>) =
                ws.into_iter().enumerate().partition(|(i, _)| i % 2 == 0);
            ws = a.into_iter().chain(b).map(|(_, w)| w).collect();
        } else {
            ws.reverse();
        }
        only_words(&mut p, &ws);
        assert_eq!(value(&p), expected());
    }
}
#[test]
fn presentation_wrappers_do_not_choose_the_section_boundary() {
    let mut p = input();
    let ws = words(&p);
    only_words(&mut p, &ws);
    assert_eq!(value(&p), expected());
}
fn attr<'a>(word: &'a str, name: &str) -> Option<&'a str> {
    let prefix = format!(" {name}=\"");
    let start = word.find(&prefix)? + prefix.len();
    Some(&word[start..start + word[start..].find('"')?])
}
fn original_header(word: &str) -> bool {
    let b = old();
    [635, 275].into_iter().any(|id| {
        let w = &b["pages"][0]["words"][id];
        let raw = word
            .split_once('>')
            .unwrap()
            .1
            .strip_suffix("</word>")
            .unwrap();
        raw == w["raw"].as_str().unwrap()
            && ["xMin", "yMin", "xMax", "yMax"]
                .iter()
                .enumerate()
                .all(|(i, name)| {
                    attr(word, name).unwrap().parse::<f32>().unwrap().to_bits()
                        == (w["source_box"][i].as_f64().unwrap() as f32).to_bits()
                })
    })
}
fn header_change(p: &mut [PageInput], mut mutate: impl FnMut(&mut String)) {
    let mut ws = words(p);
    let mut count = 0;
    for w in &mut ws {
        if original_header(w) {
            mutate(w);
            count += 1;
        }
    }
    assert_eq!(
        count, 2,
        "Both exact original source glyphs must be selected, never nearest text"
    );
    only_words(p, &ws);
}
fn remove(word: &mut String, name: &str) {
    let prefix = format!(" {name}=\"");
    let start = word.find(&prefix).unwrap();
    let end = start + prefix.len() + word[start + prefix.len()..].find('"').unwrap() + 1;
    word.replace_range(start..end, "");
}
#[test]
fn unknown_header_content_styles_preserve_the_original_unclaimed_article() {
    let mut p = input();
    header_change(&mut p, |word| remove(word, "data-native-content-styles"));
    assert_eq!(value(&p), old());
}
#[test]
fn unknown_header_axes_preserve_the_original_unclaimed_article() {
    let mut p = input();
    header_change(&mut p, |word| {
        *word = word.replace("data-native-axis=\"right\"", "data-native-axis=\"unknown\"")
    });
    assert_eq!(value(&p), old());
}
#[test]
fn compatibility_header_boxes_do_not_borrow_native_glyph_authority() {
    let mut p = input();
    header_change(&mut p, |word| {
        assert_eq!(attr(word, "data-wmode"), Some("1"));
        for name in [
            "data-native-xmin",
            "data-native-ymin",
            "data-native-xmax",
            "data-native-ymax",
        ] {
            remove(word, name);
        }
    });
    assert_eq!(value(&p), old());
}
#[test]
fn unruled_label_words_do_not_claim_an_independent_source_section() {
    let mut p = input();
    let rules = p[0].rules.as_mut().unwrap();
    assert_eq!(rules.horizontal.len(), 18);
    rules.horizontal.clear();
    assert_eq!(value(&p), old());
}
#[test]
fn the_numbered_plain_text_lookup_contract_is_unchanged() {
    assert_eq!(
        kanpo_parser::segment_articles("告示\n〇総務省告示第一号\n任意の本文")[1],
        "〇総務省告示第一号\n任意の本文"
    );
    assert_eq!(
        kanpo_parser::segment_articles("任意の本文。\n資料\n任意の後続本文。").len(),
        1
    );
}
