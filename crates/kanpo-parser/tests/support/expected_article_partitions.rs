//! Compose reviewed article partitions with historical full-Analysis oracles.
//! The old oracles and their glyph corrections stay intact. A new partition
//! must preserve every old article endpoint and all Run bytes/styles/tables.
use serde_json::Value;

fn runs(articles: &[Value]) -> Vec<Value> {
    let mut output: Vec<Value> = Vec::new();
    for article in articles {
        for block in article["document"]["blocks"].as_array().unwrap() {
            assert_eq!(block["kind"], "paragraph");
            for run in block["runs"].as_array().unwrap() {
                if run.get("table").is_some_and(|t| !t.is_null()) {
                    output.push(run.clone());
                    continue;
                }
                let mut style = run.as_object().unwrap().clone();
                let text = style.remove("text").unwrap();
                if let Some(last) = output.last_mut() {
                    let mut previous_style = last.as_object().unwrap().clone();
                    if previous_style.get("table").is_none_or(|t| t.is_null()) {
                        let previous_text = previous_style.remove("text").unwrap();
                        if previous_style == style {
                            last["text"] = format!(
                                "{}{}",
                                previous_text.as_str().unwrap(),
                                text.as_str().unwrap()
                            )
                            .into();
                            continue;
                        }
                    }
                }
                output.push(run.clone());
            }
        }
    }
    output
}

pub fn with_articles(mut original: Value, partitions: &str) -> Value {
    let partitions: Value = serde_json::from_str(partitions).unwrap();
    let new = partitions.as_array().unwrap();
    let mut next = 0;
    for old in original["articles"].as_array().unwrap() {
        let start = next;
        let mut text = String::new();
        while next < new.len() {
            if next > start {
                text.push('\n');
            }
            text.push_str(new[next]["text"].as_str().unwrap());
            assert!(
                old["text"].as_str().unwrap().starts_with(&text),
                "partition changed historical text or crossed an old endpoint"
            );
            assert_eq!(old["document"]["format"], new[next]["document"]["format"]);
            next += 1;
            if text == old["text"].as_str().unwrap() {
                break;
            }
        }
        assert_eq!(text, old["text"].as_str().unwrap());
        assert!(
            runs(std::slice::from_ref(old)) == runs(&new[start..next]),
            "partition changed historical Run bytes, styles or whole tables"
        );
    }
    assert_eq!(next, new.len());
    original["articles"] = partitions;
    original
}
