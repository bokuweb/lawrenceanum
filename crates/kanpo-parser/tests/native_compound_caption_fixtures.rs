//! Gold comes from164 by splitting only source-reviewed Article/run ranges.
//! Native table objects, formatting, run whitespace and all other Analysis
//! fields remain exact. No current parser output generates these expectations.
use kanpo_parser::{analyze_item, page_inputs};
use serde_json::Value;
use std::path::PathBuf;
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests")
}
fn read(path: &str) -> Value {
    serde_json::from_slice(&std::fs::read(root().join(path)).unwrap()).unwrap()
}
fn index() -> Value {
    read("data/native-compound-captions165/index.json")
}
fn value(paths: &[&str]) -> Value {
    let input = paths
        .iter()
        .flat_map(|p| page_inputs(&std::fs::read(root().join(p)).unwrap()).unwrap())
        .collect::<Vec<_>>();
    serde_json::from_str(&serde_json::to_string(&analyze_item(&input)).unwrap()).unwrap()
}
fn single(key: &str) -> Value {
    let i = index();
    let c = i["single"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["key"] == key)
        .unwrap();
    value(&[c["pdf"].as_str().unwrap()])
}
#[test]
fn source_reviewed_wrapped_captions_preserve_every_analysis_field_and_all_old_material() {
    let i = index();
    let cases = i["single"].as_array().unwrap();
    assert_eq!(cases.len(), 10);
    for c in cases {
        let key = c["key"].as_str().unwrap();
        assert_eq!(
            value(&[c["pdf"].as_str().unwrap()]),
            read(c["gold"].as_str().unwrap()),
            "{key}"
        );
    }
}
#[test]
fn original_consecutive_pages_keep_the_new_root_and_original_financial_tables() {
    let i = index();
    let c = &i["window"];
    let paths = c["pdfs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(paths.len(), 2);
    assert_eq!(value(&paths), read(c["gold"].as_str().unwrap()));
    let a = single("20261016g002270086");
    let articles = a["articles"].as_array().unwrap();
    assert_eq!(articles.len(), 2);
    let tables = |article: &Value| {
        article["document"]["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|b| b["runs"].as_array().unwrap())
            .filter(|r| r.get("table").is_some())
            .count()
    };
    assert_eq!(tables(&articles[0]), 3);
    assert_eq!(tables(&articles[1]), 0);
    assert!(articles[1]["text"]
        .as_str()
        .unwrap()
        .starts_with("（仮称）佐世保市鹿町町風力発電所設置計画に係\nる環境影響評価準備書の公告"));
}
#[test]
fn a_compound_caption_keeps_three_coissuers_in_one_native_procedure() {
    let a = single("20260929h017970031");
    let articles = a["articles"].as_array().unwrap();
    assert_eq!(articles.len(), 8);
    let joint = articles[5]["text"].as_str().unwrap();
    assert!(joint.starts_with("資本金及び準備金の額の減少公告、吸収分割公告\n並びに合併公告"));
    for issuer in [
        "株式会社デジタルホールディングス",
        "Bonds Investment Group株式会社",
        "株式会社オプト",
    ] {
        assert!(joint.contains(issuer));
        assert!(articles
            .iter()
            .enumerate()
            .all(|(i, r)| i == 5 || !r["text"].as_str().unwrap().contains(issuer)));
    }
    let compact = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    assert_eq!(
        compact(a["text"].as_str().unwrap()),
        compact(
            &articles
                .iter()
                .map(|r| r["text"].as_str().unwrap())
                .collect::<String>()
        )
    );
}
