//! Source no-value marks are part of their own printed amount fields.
use serde_json::Value;
use std::path::PathBuf;
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/native-amount-marks")
}
fn case(key: &str) -> Value {
    let index: Value =
        serde_json::from_slice(&std::fs::read(root().join("index.json")).unwrap()).unwrap();
    index["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["key"] == key)
        .unwrap()
        .clone()
}
fn gold(case: &Value, name: &str) -> Value {
    serde_json::from_slice(&std::fs::read(root().join(case[name].as_str().unwrap())).unwrap())
        .unwrap()
}
fn inputs(case: &Value) -> Vec<kanpo_parser::PageInput> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(case["pdf"].as_str().unwrap());
    kanpo_parser::page_inputs(&std::fs::read(path).unwrap()).unwrap()
}
fn actual(pages: &[kanpo_parser::PageInput]) -> Value {
    serde_json::from_str(&serde_json::to_string(&kanpo_parser::analyze_item(pages)).unwrap())
        .unwrap()
}
macro_rules! original {
    ($test:ident,$key:literal) => {
        #[test]
        fn $test() {
            let c = case($key);
            assert_eq!(actual(&inputs(&c)), gold(&c, "expected"), $key);
        }
    };
}
original!(complete_20260731g001720299, "20260731g001720299");
original!(complete_20260731g001720301, "20260731g001720301");
original!(complete_20260731g001720304, "20260731g001720304");
original!(complete_20260831g001930183, "20260831g001930183");
original!(complete_20260902g001960127, "20260902g001960127");
original!(complete_20260902g001960128, "20260902g001960128");
original!(complete_20260902g001960129, "20260902g001960129");
original!(complete_20260902g001960130, "20260902g001960130");
original!(complete_20260930g002150478, "20260930g002150478");
original!(complete_20260930g002150479, "20260930g002150479");
fn remove_attribute(html: &mut String, attribute: &str) {
    let needle = format!(" {attribute}=\"");
    while let Some(at) = html.find(&needle) {
        let from = at + needle.len();
        let end = from + html[from..].find('"').unwrap() + 1;
        html.replace_range(at..end, "");
    }
}
#[test]
fn all_originals_without_outline_evidence_keep_the_complete_previous_analysis() {
    let index: Value =
        serde_json::from_slice(&std::fs::read(root().join("index.json")).unwrap()).unwrap();
    for c in index["cases"].as_array().unwrap() {
        let mut pages = inputs(c);
        for page in &mut pages {
            remove_attribute(&mut page.xhtml, "data-native-outline-rectangle");
        }
        assert_eq!(actual(&pages), gold(c, "baseline"), "{}", c["key"]);
    }
}
#[test]
fn missing_original_glyph_count_keeps_the_entire_previous_analysis() {
    let c = case("20260902g001960127");
    let mut pages = inputs(&c);
    remove_attribute(&mut pages[0].xhtml, "data-native-glyph-count");
    assert_eq!(actual(&pages), gold(&c, "baseline"));
}
#[test]
fn an_outline_transferred_between_actual_equal_raw_marks_cannot_authorize_either() {
    let c = case("20260731g001720301");
    let mut pages = inputs(&c);
    let baseline = gold(&c, "baseline");
    let html = scraper::Html::parse_document(&pages[0].xhtml);
    let selector = scraper::Selector::parse("word").unwrap();
    let source = |id: usize| {
        let w = &baseline["pages"][0]["words"][id];
        html.select(&selector)
            .find(|node| {
                node.text().collect::<String>() == w["raw"].as_str().unwrap()
                    && ["xmin", "ymin", "xmax", "ymax"]
                        .iter()
                        .enumerate()
                        .all(|(i, key)| {
                            node.value().attr(key).unwrap().parse::<f32>().unwrap()
                                == w["source_box"][i].as_f64().unwrap() as f32
                        })
            })
            .unwrap()
    };
    let a = source(955);
    let b = source(956);
    assert_eq!(a.text().collect::<String>(), b.text().collect::<String>());
    let first = a
        .value()
        .attr("data-native-flow-first-glyph")
        .unwrap()
        .to_string();
    let outline = b
        .value()
        .attr("data-native-outline-rectangle")
        .unwrap()
        .to_string();
    remove_attribute(&mut pages[0].xhtml, "data-native-outline-rectangle");
    let at = pages[0]
        .xhtml
        .find(&format!("data-native-flow-first-glyph=\"{first}\""))
        .unwrap();
    let end = at + pages[0].xhtml[at..].find('>').unwrap();
    pages[0].xhtml.insert_str(
        end,
        &format!(" data-native-outline-rectangle=\"{outline}\""),
    );
    assert_eq!(actual(&pages), baseline);
}
#[test]
fn original_multiple_amount_columns_keep_their_twelve_source_marks() {
    let a = kanpo_parser::analyze_item(
        &kanpo_parser::page_inputs(include_bytes!(
            "data/native-amount-marks/20260902g001960127.pdf"
        ))
        .unwrap(),
    );
    for row in [
        "税外収入　17,999　－　17,999　22,494　－　22,494",
        "社会保障費　－　82,508　△82,508　－　67,720　△67,720",
        "防衛関係費　－　26,361　△26,361　－　23,971　△23,971",
        "公共事業費　－　20,860　△20,860　－　20,519　△20,519",
        "義務教育費　－　5,408　△5,408　－　5,136　△5,136",
        "その他支払　－　60,598　△60,598　－　54,800　△54,800",
    ] {
        assert!(
            a.text.lines().any(|line| line == row),
            "missing source row: {row}"
        );
    }
    assert_eq!(
        a.pages[0].words.iter().filter(|w| w.text == "－").count(),
        12
    );
}
