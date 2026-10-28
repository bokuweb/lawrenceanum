//! Exact original coordinate fidelity must preserve all other native output.
fn actual(input: &[kanpo_parser::PageInput]) -> serde_json::Value {
    serde_json::from_str(&serde_json::to_string(&kanpo_parser::analyze_item(input)).unwrap())
        .unwrap()
}
fn input(bytes: &[u8]) -> Vec<kanpo_parser::PageInput> {
    kanpo_parser::page_inputs(bytes).unwrap()
}
fn expected(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap()
}
#[test]
fn complete_original_20260724g001660079() {
    let inputs = input(include_bytes!(
        "data/native-source-floats/20260724g001660079.pdf"
    ));
    assert!(
        actual(&inputs)
            == expected(include_str!(
                "data/native-source-floats/expected/20260724g001660079.json"
            )),
        "complete independent old-AST gold must match, not only restored bar strings"
    );
}
#[test]
fn complete_original_20260826g001900055() {
    let inputs = input(include_bytes!(
        "data/native-source-floats/20260826g001900055.pdf"
    ));
    assert!(
        actual(&inputs)
            == expected(include_str!(
                "data/native-source-floats/expected/20260826g001900055.json"
            )),
        "complete independent old-AST gold must match, not only restored bar strings"
    );
}
#[test]
fn complete_original_20260902g001960132() {
    let inputs = input(include_bytes!(
        "data/native-source-floats/20260902g001960132.pdf"
    ));
    assert!(
        actual(&inputs)
            == expected(include_str!(
                "data/native-source-floats/expected/20260902g001960132.json"
            )),
        "complete independent old-AST gold must match, not only restored bar strings"
    );
}
#[test]
fn complete_original_20260924g002110108() {
    let inputs = input(include_bytes!(
        "data/native-source-floats/20260924g002110108.pdf"
    ));
    assert!(
        actual(&inputs)
            == expected(include_str!(
                "data/native-source-floats/expected/20260924g002110108.json"
            )),
        "complete independent old-AST gold must match, not only restored bar strings"
    );
}
fn cases() -> Vec<(
    Vec<kanpo_parser::PageInput>,
    serde_json::Value,
    serde_json::Value,
)> {
    vec![
        (
            input(include_bytes!(
                "data/native-source-floats/20260724g001660079.pdf"
            )),
            expected(include_str!(
                "data/native-source-floats/baseline/20260724g001660079.json"
            )),
            expected(include_str!(
                "data/native-source-floats/expected/20260724g001660079.json"
            )),
        ),
        (
            input(include_bytes!(
                "data/native-source-floats/20260826g001900055.pdf"
            )),
            expected(include_str!(
                "data/native-source-floats/baseline/20260826g001900055.json"
            )),
            expected(include_str!(
                "data/native-source-floats/expected/20260826g001900055.json"
            )),
        ),
        (
            input(include_bytes!(
                "data/native-source-floats/20260902g001960132.pdf"
            )),
            expected(include_str!(
                "data/native-source-floats/baseline/20260902g001960132.json"
            )),
            expected(include_str!(
                "data/native-source-floats/expected/20260902g001960132.json"
            )),
        ),
        (
            input(include_bytes!(
                "data/native-source-floats/20260924g002110108.pdf"
            )),
            expected(include_str!(
                "data/native-source-floats/baseline/20260924g002110108.json"
            )),
            expected(include_str!(
                "data/native-source-floats/expected/20260924g002110108.json"
            )),
        ),
    ]
}
#[test]
fn absent_original_paint_keeps_the_complete_previous_abstention() {
    for (mut inputs, old, _) in cases() {
        let h = &mut inputs[0].xhtml;
        let needle = " data-native-scene=\"";
        let at = h.find(needle).unwrap();
        let start = at + needle.len();
        let end = start + h[start..].find('"').unwrap() + 1;
        h.replace_range(at..end, "");
        assert!(actual(&inputs)==old,"absence is not empty paint; no source rectangle may be inferred from a row label or an expected amount");
    }
}
#[test]
fn source_occurrences_keep_all_complete_outputs_when_word_nodes_are_reversed() {
    for (mut inputs, _, gold) in cases() {
        let h = &inputs[0].xhtml;
        let (mut spans, mut cursor) = (vec![], 0);
        while let Some(at) = h[cursor..].find("<word ") {
            let start = cursor + at;
            let end = start + h[start..].find("</word>").unwrap() + 7;
            spans.push((start, end));
            cursor = end;
        }
        let mut out = String::new();
        cursor = 0;
        for (&(start, end), &(a, b)) in spans.iter().zip(spans.iter().rev()) {
            out.push_str(&h[cursor..start]);
            out.push_str(&h[a..b]);
            cursor = end;
        }
        out.push_str(&h[cursor..]);
        inputs[0].xhtml = out;
        assert!(
            actual(&inputs) == gold,
            "source glyph ownership is independent of emission order"
        );
    }
}
