use serde_json::{json, Value};

// These are expectations for the two printed laws in the original PDF, not
// parser output. Keep the historical character-restoration golden files intact.
pub fn separate_original_laws_63_and_64(expected: &mut Value) {
    let refs: Value =
        serde_json::from_str(include_str!("../data/legal-summaries/references.json")).unwrap();
    let records = refs["expected"]["20260723g001630002"]["expected_records"]
        .as_array()
        .unwrap();
    let title = records[1].as_str().unwrap().lines().next().unwrap();
    let articles = expected["articles"].as_array_mut().unwrap();
    assert_eq!(articles.len(), 1);
    assert_eq!(
        articles[0]["text"],
        json!(records
            .iter()
            .map(|r| r.as_str().unwrap())
            .collect::<Vec<_>>()
            .join("\n"))
    );
    let mut second = articles[0].clone();
    let document_text = articles[0]["document"]["blocks"][0]["runs"][0]["text"]
        .as_str()
        .unwrap();
    assert_eq!(
        articles[0]["document"]["blocks"].as_array().unwrap().len(),
        1
    );
    assert_eq!(
        articles[0]["document"]["blocks"][0]["runs"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let (first, tail) = document_text.split_once(title).unwrap();
    let first = first.to_owned();
    let tail = title.to_owned() + tail;
    articles[0]["text"] = records[0].clone();
    second["text"] = records[1].clone();
    articles[0]["document"]["blocks"][0]["runs"][0]["text"] = json!(first);
    second["document"]["blocks"][0]["runs"][0]["text"] = json!(tail);
    articles.push(second);
}
