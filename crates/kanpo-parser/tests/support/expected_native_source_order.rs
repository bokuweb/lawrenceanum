//! Compare an individually reviewed source-order oracle with its historical
//! full oracle. Only complete lines and their article boundaries may move;
//! Word geometry, roles, text styles and complete tables remain checked.
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

fn canonical(mut value: Value) -> String {
    value.sort_all_objects();
    serde_json::to_string(&value).unwrap()
}

fn characters(text: &str) -> BTreeMap<char, usize> {
    let mut counts = BTreeMap::new();
    for c in text.chars().filter(|c| !c.is_whitespace()) {
        *counts.entry(c).or_default() += 1;
    }
    counts
}

fn lines(page: &Value) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for region in page["regions"].as_array().unwrap() {
        for line in region["lines"].as_array().unwrap() {
            let key = canonical(serde_json::json!([region["role"], line]));
            *counts.entry(key).or_default() += 1;
        }
    }
    counts
}

fn runs(analysis: &Value) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for article in analysis["articles"].as_array().unwrap() {
        for block in article["document"]["blocks"].as_array().unwrap() {
            assert_eq!(block["kind"], "paragraph");
            for run in block["runs"].as_array().unwrap() {
                if run.get("table").is_some_and(|t| !t.is_null()) {
                    let key = canonical(serde_json::json!(["table", run]));
                    *counts.entry(key).or_default() += 1;
                } else {
                    let mut style = run.as_object().unwrap().clone();
                    let text = style.remove("text").unwrap();
                    for c in text
                        .as_str()
                        .unwrap()
                        .chars()
                        .filter(|c| !c.is_whitespace())
                    {
                        let key = canonical(serde_json::json!(["text", c.to_string(), style]));
                        *counts.entry(key).or_default() += 1;
                    }
                }
            }
        }
    }
    counts
}

pub fn with_reviewed_order(original: Value, reviewed: &str) -> Value {
    let reviewed: Value = serde_json::from_str(reviewed).unwrap();
    let metadata = |value: &Value, movable: &[&str]| {
        let mut fields = value.as_object().unwrap().clone();
        for key in movable {
            fields.remove(*key);
        }
        fields
    };
    assert!(
        metadata(&original, &["pages", "text", "articles"])
            == metadata(&reviewed, &["pages", "text", "articles"])
    );
    let old_pages = original["pages"].as_array().unwrap();
    let new_pages = reviewed["pages"].as_array().unwrap();
    assert_eq!(old_pages.len(), new_pages.len());
    for (old, new) in old_pages.iter().zip(new_pages) {
        assert!(
            metadata(old, &["regions", "text"]) == metadata(new, &["regions", "text"]),
            "historical Word fields, page geometry and layout must stay exact"
        );
        assert!(
            lines(old) == lines(new),
            "only complete same-role lines/tables may move"
        );
        assert!(
            characters(old["text"].as_str().unwrap()) == characters(new["text"].as_str().unwrap())
        );
    }
    assert!(
        characters(original["text"].as_str().unwrap())
            == characters(reviewed["text"].as_str().unwrap())
    );
    assert!(
        runs(&original) == runs(&reviewed),
        "every styled text character and whole structured table must be retained"
    );
    let formats = |a: &Value| -> BTreeSet<String> {
        a["articles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|article| canonical(article["document"]["format"].clone()))
            .collect()
    };
    assert!(formats(&original) == formats(&reviewed));
    reviewed
}
