//! Exact in-place article partitions; historical native/source fields remain authoritative.
use serde_json::Value;
#[path = "native_numeric_frame_contracts.rs"]
mod numeric_contract;
fn tables(v: &Value, out: &mut Vec<Value>) {
    match v {
        Value::Array(xs) => {
            for x in xs {
                tables(x, out);
            }
        }
        Value::Object(m) => {
            for (k, v) in m {
                if k == "table" {
                    out.push(v.clone());
                } else {
                    tables(v, out);
                }
            }
        }
        _ => (),
    }
}
pub(crate) fn preserved(old: &Value, new: &Value) {
    assert_eq!(
        old["pages"], new["pages"],
        "all original words, geometry, roles, source owners and native tables"
    );
    assert_eq!(old["text"], new["text"]);
    assert_eq!(old["format"], new["format"]);
    let new_articles = new["articles"].as_array().unwrap();
    let mut cursor = 0;
    for a in old["articles"].as_array().unwrap() {
        let start = cursor;
        let mut text = String::new();
        while cursor < new_articles.len() {
            if cursor > start {
                text.push('\n');
            }
            text.push_str(new_articles[cursor]["text"].as_str().unwrap());
            cursor += 1;
            if text == a["text"].as_str().unwrap() {
                break;
            }
            assert!(
                a["text"].as_str().unwrap().starts_with(&text),
                "an old article is partitioned in place without crossing its original boundary"
            );
        }
        assert_eq!(text, a["text"]);
        let parts = &new_articles[start..cursor];
        assert!(!parts.is_empty());
        if parts.len() == 1 {
            assert_eq!(a, &parts[0], "unrelated complete ArticleAnalysis");
        } else {
            let before = numeric_contract::styled_document(&a["document"]);
            let after: Vec<_> = parts
                .iter()
                .flat_map(|p| numeric_contract::styled_document(&p["document"]))
                .collect();
            assert_eq!(
                before, after,
                "every Document character/style/ASCII/fullwidth gap; LF excluded"
            );
            let mut before = vec![];
            let mut after = vec![];
            tables(&a["document"], &mut before);
            for p in parts {
                tables(&p["document"], &mut after);
            }
            assert_eq!(before, after, "all original Document tables and order");
            let before = a
                .get("notices")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let after: Vec<_> = parts
                .iter()
                .flat_map(|p| {
                    p.get("notices")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .cloned()
                })
                .collect();
            assert_eq!(before, after, "the prior notice/source graph");
        }
    }
    assert_eq!(
        cursor,
        new_articles.len(),
        "every new article has exactly one old material owner"
    );
}

/// Fixed reviewed arrays preserve exact regression coverage while keeping the
/// historical132/133 native/material anchors and independent source witnesses.
pub(crate) fn reviewed_regression(key: &str, old: &Value, actual: &Value) {
    let reviewed: Value = serde_json::from_str(include_str!(
        "../data/native-court-columns/reviewed-article-projections150.json"
    ))
    .unwrap();
    let mut expected = old.clone();
    if let Some(row) = reviewed["cases"].get(key) {
        expected["articles"] = row["articles"].clone();
        preserved(old, &expected);
    }
    let reviewed151: Value = serde_json::from_str(include_str!(
        "../data/native-court-body-fonts/reviewed-article-projections151.json"
    ))
    .unwrap();
    if let Some(row) = reviewed151["cases"].get(key) {
        let previous = expected.clone();
        expected["articles"] = row["articles"].clone();
        preserved(&previous, &expected);
    }
    assert!(
        actual == &expected,
        "fixed source-reviewed article partitions150/151 or unchanged complete Analysis: {key}"
    );
}
