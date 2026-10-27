//! Complete original Analysis, independently constructed from old AST source cuts.
use serde_json::Value;
use std::path::PathBuf;
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/native-instrument-roots")
        .join(name)
}
fn case(key: &str) -> Value {
    let index: Value =
        serde_json::from_slice(&std::fs::read(fixture("index.json")).unwrap()).unwrap();
    index["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["key"] == key)
        .unwrap()
        .clone()
}
fn gold(case: &Value, field: &str) -> Value {
    serde_json::from_slice(&std::fs::read(fixture(case[field].as_str().unwrap())).unwrap()).unwrap()
}
fn inputs(case: &Value) -> Vec<kanpo_parser::PageInput> {
    case["pdfs"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|p| {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(p["path"].as_str().unwrap());
            kanpo_parser::page_inputs(&std::fs::read(path).unwrap()).unwrap()
        })
        .collect()
}
fn actual(inputs: &[kanpo_parser::PageInput]) -> Value {
    // Match the public JSON representation of original f32 coordinates.
    serde_json::from_str(&serde_json::to_string(&kanpo_parser::analyze_item(inputs)).unwrap())
        .unwrap()
}
fn original(key: &str) {
    let c = case(key);
    assert_eq!(actual(&inputs(&c)), gold(&c, "expected"), "{key}");
}
macro_rules! source_case {
    ($name:ident,$key:literal) => {
        #[test]
        fn $name() {
            original($key);
        }
    };
}
source_case!(original_20260728h017560010, "20260728h017560010");
source_case!(original_20260803g001740041, "20260803g001740041");
source_case!(original_20260803g001740043, "20260803g001740043");
source_case!(original_20260806h017630009, "20260806h017630009");
source_case!(original_20260807g001780069, "20260807g001780069");
source_case!(original_20260901g001950035, "20260901g001950035");
source_case!(original_20260901g001950037, "20260901g001950037");
source_case!(original_20260901g001950038, "20260901g001950038");
source_case!(original_20260901g001950040, "20260901g001950040");
source_case!(original_20260901g001950042, "20260901g001950042");
source_case!(original_20260901g001950043, "20260901g001950043");
source_case!(original_20260901g001950044, "20260901g001950044");
source_case!(original_20260901g001950045, "20260901g001950045");
source_case!(original_20260901g001950046, "20260901g001950046");
source_case!(original_20260901g001950047, "20260901g001950047");
source_case!(original_20260901g001950048, "20260901g001950048");
source_case!(original_20260904g001980021, "20260904g001980021");
source_case!(original_20260911h017880010, "20260911h017880010");
source_case!(original_20260925h017950008, "20260925h017950008");
source_case!(original_20261015h018080009, "20261015h018080009");
source_case!(original_tax_page_boundary, "tax-page-boundary");
source_case!(original_exam_page_continuation, "exam-page-continuation");

fn without_word_attribute(c: &Value, id: usize, attribute: &str) -> Vec<kanpo_parser::PageInput> {
    let before = gold(c, "baseline");
    let raw = before["pages"][0]["words"][id]["raw"].as_str().unwrap();
    let text = raw
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let mut pages = inputs(c);
    let html = &mut pages[0].xhtml;
    let needle = format!(">{text}</word>");
    assert_eq!(html.match_indices(&needle).count(), 1);
    let end = html.find(&needle).unwrap();
    let start = html[..end].rfind("<word ").unwrap();
    let name = format!(" {attribute}=\"");
    let at = start + html[start..end].find(&name).unwrap();
    let from = at + name.len();
    let after = from + html[from..end].find('"').unwrap() + 1;
    html.replace_range(at..after, "");
    pages
}
#[test]
fn an_original_heading_without_its_own_native_content_style_keeps_the_baseline() {
    let c = case("20260901g001950038");
    let id = c["cuts"][0]["words"][0].as_u64().unwrap() as usize;
    assert_eq!(
        actual(&without_word_attribute(
            &c,
            id,
            "data-native-content-styles"
        )),
        gold(&c, "baseline")
    );
}
#[test]
fn an_original_date_without_its_own_source_baseline_keeps_the_baseline() {
    let c = case("20260901g001950038");
    let id = c["native_header_witness"]["date_words"][0]
        .as_u64()
        .unwrap() as usize;
    assert_eq!(
        actual(&without_word_attribute(&c, id, "data-native-flow-baseline")),
        gold(&c, "baseline")
    );
}
#[test]
fn an_original_signature_without_its_own_source_baseline_keeps_the_baseline() {
    let c = case("20260901g001950038");
    let id = c["native_header_witness"]["signature_words"][0]
        .as_u64()
        .unwrap() as usize;
    assert_eq!(
        actual(&without_word_attribute(&c, id, "data-native-flow-baseline")),
        gold(&c, "baseline")
    );
}
