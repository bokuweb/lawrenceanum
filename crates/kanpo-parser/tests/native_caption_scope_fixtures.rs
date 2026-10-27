//! Frozen source-reviewed boundaries. Every field is compared, including all
//! original words, body bytes and Document run whitespace. Gold was constructed
//! from163 by splitting only reviewed source scopes, preserving prior format.
use kanpo_parser::{analyze_item, page_inputs};
use serde_json::Value;
use std::path::PathBuf;
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/native-caption-scopes164")
}
fn read(name: &str) -> Value {
    serde_json::from_slice(&std::fs::read(root().join(name)).unwrap()).unwrap()
}
fn value(pdfs: &[&str]) -> Value {
    let pages = pdfs
        .iter()
        .flat_map(|p| page_inputs(&std::fs::read(root().join(p)).unwrap()).unwrap())
        .collect::<Vec<_>>();
    serde_json::from_str(&serde_json::to_string(&analyze_item(&pages)).unwrap()).unwrap()
}
#[test]
fn source_reviewed_single_pages_keep_every_field_except_the_proven_article_splits() {
    let index = read("index.json");
    let cases = index["single"].as_array().unwrap();
    assert_eq!(cases.len(), 26);
    for c in cases {
        let name = c["pdf"].as_str().unwrap();
        assert_eq!(value(&[name]), read(c["gold"].as_str().unwrap()), "{name}");
    }
}
#[test]
fn original_page_pairs_keep_native_continuations_and_separate_proven_publications() {
    let index = read("index.json");
    let cases = index["pairs"].as_array().unwrap();
    assert_eq!(cases.len(), 7);
    for c in cases {
        let names = c["pdfs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            value(&names),
            read(c["gold"].as_str().unwrap()),
            "{names:?}"
        );
    }
}
#[test]
fn thirteen_original_notices_keep_their_own_publishers_and_all_body_material() {
    let a = value(&["20261016h018090031.pdf"]);
    let articles = a["articles"].as_array().unwrap();
    assert_eq!(articles.len(), 13);
    let publishers = [
        "合同会社ＳＫＹｇａｒｄｅｎ",
        "株式会社インターヨーロッパ",
        "有限会社いざわ",
        "Ｊａｐａｎ株式会社",
        "株式会社アルダグラム",
        "株式会社Ｙｅｎｙ",
        "三和段ボール株式会社",
        "株式会社雲仙富貴屋",
        "嘉新琉球ＣＯＬＬＥＣＴＩＶＥ株式会社",
        "株式会社アモック・システム",
        "住華運輸株式会社",
        "マルスイホールディングス株式会社",
        "株式会社リアルワールド",
    ];
    for (i, p) in publishers.iter().enumerate() {
        let t = articles[i]["text"].as_str().unwrap();
        assert!(t.contains(p), "{p}");
        for (j, q) in publishers.iter().enumerate() {
            if i != j {
                assert!(!t.contains(q), "{p}: {q}");
            }
        }
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
