//! Independently reviewed physical columns and original glyph provenance.
//! The complete expected Analysis edits the adopted126source only at the
//! reviewed131column and formats that source order with the prior formatter.
use kanpo_parser::{analyze_item, page_inputs, PageInput};
use serde_json::Value;
fn original() -> Vec<PageInput> {
    page_inputs(include_bytes!(
        "data/native-column-flow/20260930g002150131.pdf"
    ))
    .unwrap()
}
fn expected() -> Value {
    serde_json::from_str(include_str!("data/quoted-references/20260930g002150131-expected129.json")).unwrap()
}
fn baseline() -> Value {
    serde_json::from_str(include_str!("data/native-column-flow/baseline131-126.json")).unwrap()
}
fn value(p: &[PageInput]) -> Value {
    serde_json::from_slice(&serde_json::to_vec(&analyze_item(p)).unwrap()).unwrap()
}
fn words(p: &[PageInput]) -> Vec<String> {
    let mut rest = p[0].xhtml.as_str();
    let mut out = vec![];
    while let Some(at) = rest.find("<word ") {
        rest = &rest[at..];
        let end = rest.find("</word>").unwrap() + 7;
        out.push(rest[..end].to_string());
        rest = &rest[end..];
    }
    assert_eq!(out.len(), 25);
    out
}
fn only_words(p: &mut [PageInput], words: &[String]) {
    let start = p[0].xhtml.find("<page ").unwrap();
    let after = start + p[0].xhtml[start..].find('>').unwrap() + 1;
    let opening = p[0].xhtml[start..after].to_string();
    p[0].xhtml = format!(
        "<html><body><doc>{opening}{}</page></doc></body></html>",
        words.join("\n")
    );
}
fn remove(s: &mut String, key: &str) {
    let prefix = format!(" {key}=\"");
    let at = s.find(&prefix).unwrap();
    let end = at + prefix.len() + s[at + prefix.len()..].find('"').unwrap() + 1;
    s.replace_range(at..end, "");
}
fn attr<'a>(s: &'a str, key: &str) -> &'a str {
    let prefix = format!(" {key}=\"");
    let at = s.find(&prefix).unwrap() + prefix.len();
    &s[at..at + s[at..].find('"').unwrap()]
}
fn original_word(word: &str, id: usize) -> bool {
    let source = baseline();
    let w = &source["pages"][0]["words"][id];
    word.split_once('>')
        .unwrap()
        .1
        .strip_suffix("</word>")
        .unwrap()
        == w["raw"].as_str().unwrap()
        && ["xMin", "yMin", "xMax", "yMax"]
            .iter()
            .enumerate()
            .all(|(i, k)| {
                attr(word, k).parse::<f32>().unwrap().to_bits()
                    == (w["source_box"][i].as_f64().unwrap() as f32).to_bits()
            })
}
fn change_source(p: &mut [PageInput], id: usize, mut change: impl FnMut(&mut String)) {
    let mut ws = words(p);
    let mut count = 0;
    for word in &mut ws {
        if original_word(word, id) {
            change(word);
            count += 1;
        }
    }
    assert_eq!(count, 1);
    only_words(p, &ws);
}
#[test]
fn complete_original_column_order_and_every_source_word_are_preserved() {
    assert_eq!(value(&original()), expected());
}
#[test]
fn reversed_or_interleaved_xml_and_absent_presentation_wrappers_keep_source_ownership() {
    for mode in 0..3 {
        let mut p = original();
        let mut ws = words(&p);
        if mode == 0 {
            ws.reverse();
        } else if mode == 1 {
            let (a, b): (Vec<_>, Vec<_>) =
                ws.into_iter().enumerate().partition(|(i, _)| i % 2 == 0);
            ws = a.into_iter().chain(b).map(|(_, w)| w).collect();
        }
        only_words(&mut p, &ws);
        assert_eq!(value(&p), expected());
    }
}
#[test]
fn a_missing_page_primitive_count_cannot_certify_a_continuation() {
    let mut p = original();
    remove(&mut p[0].xhtml, "data-native-glyph-count");
    assert_eq!(value(&p), baseline());
}
#[test]
fn every_original_flow_field_is_required_for_the_bridge() {
    for field in [
        "start",
        "baseline",
        "end",
        "leading",
        "trailing",
        "prefix",
        "first-glyph",
        "after-glyph",
    ] {
        let mut p = original();
        change_source(&mut p, 4, |w| {
            remove(w, &format!("data-native-flow-{field}"))
        });
        assert_eq!(value(&p), baseline(), "{field}");
    }
}
#[test]
fn one_source_baseline_bit_does_not_round_into_a_witness() {
    let mut p = original();
    change_source(&mut p, 4, |w| {
        let key = "data-native-flow-baseline";
        let old = attr(w, key).to_string();
        let n = old.parse::<f64>().unwrap();
        *w = w.replace(
            &format!("{key}=\"{old}\""),
            &format!("{key}=\"{}\"", f64::from_bits(n.to_bits() + 1)),
        );
    });
    assert_eq!(value(&p), baseline());
}
#[test]
fn copied_primitive_ranges_do_not_borrow_another_words_source_identity() {
    let mut p = original();
    change_source(&mut p, 4, |w| {
        for (key, new) in [("first-glyph", "0"), ("after-glyph", "10")] {
            let k = format!("data-native-flow-{key}");
            let old = attr(w, &k).to_string();
            *w = w.replace(&format!("{k}=\"{old}\""), &format!("{k}=\"{new}\""));
        }
    });
    assert_eq!(value(&p), baseline());
}
#[test]
fn opaque_font_identity_bijection_does_not_choose_a_column() {
    let mut p = original();
    let mut ws = words(&p);
    for w in &mut ws {
        for key in ["data-native-styles", "data-native-content-styles"] {
            let prefix = format!(" {key}=\"");
            if !w.contains(&prefix) {
                continue;
            }
            let old = attr(w, key).to_string();
            if old == "unknown" {
                continue;
            }
            let new = old
                .split_whitespace()
                .map(|s| {
                    let (f, b) = s.split_once(':').unwrap();
                    format!("{}:{b}", f.parse::<usize>().unwrap() + 1000)
                })
                .collect::<Vec<_>>()
                .join(" ");
            *w = w.replace(&format!("{key}=\"{old}\""), &format!("{key}=\"{new}\""));
        }
    }
    only_words(&mut p, &ws);
    assert_eq!(value(&p), expected());
}
fn composition() -> Vec<PageInput> {
    let mut pages = page_inputs(include_bytes!(
        "archive/pages/20260930g00215/20260930g002150011.pdf"
    ))
    .unwrap();
    pages.extend(original());
    pages.extend(
        page_inputs(include_bytes!(
            "data/native-section-roots/20260930g002150475.pdf"
        ))
        .unwrap(),
    );
    pages
}
#[test]
fn literal_replacement_scope_and_physical_order_precede_the_independent_fiscal_root() {
    let p = composition();
    let a = value(&p);
    let e: Value =
        serde_json::from_str(include_str!("data/native-column-flow/expected475-126.json")).unwrap();
    // Root splitting retains the containing Article's format. The bare
    // 「関東地方整備局公示」 title on 475 now starts its own Article in the
    // composition too, so the root's containing Article is the same 475
    // notice as in isolation (unknown), not the 131 Cabinet order (prose,
    // composition-format126.json).
    assert_eq!(
        a["articles"].as_array().unwrap().last(),
        e["articles"].as_array().unwrap().last()
    );
}
#[test]
fn a_broken_source_bridge_cannot_release_the_later_fiscal_root() {
    let mut p = composition();
    change_source(&mut p[1..2], 4, |w| remove(w, "data-native-flow-start"));
    let a = value(&p);
    let e: Value =
        serde_json::from_str(include_str!("data/native-column-flow/expected475-126.json")).unwrap();
    assert_ne!(
        a["articles"].as_array().unwrap().last().unwrap()["text"],
        e["articles"].as_array().unwrap().last().unwrap()["text"]
    );
}
