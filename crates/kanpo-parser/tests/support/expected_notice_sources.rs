//! Overlay independently reviewed additive source scopes on historical goldens.
//! Every prior character, source Word, article and Document field remains exact.
use serde_json::Value;
pub fn with_notices(mut expected: Value, spec: &str) -> Value {
    let spec: Value = serde_json::from_str(spec).unwrap();
    let articles = expected["articles"].as_array_mut().unwrap();
    let mut used = std::collections::BTreeSet::new();
    for row in spec["articles"].as_array().unwrap() {
        let index = row["article"].as_u64().unwrap() as usize;
        assert!(used.insert(index));
        let article = articles[index].as_object_mut().unwrap();
        assert!(!article.contains_key("notices"));
        assert!(!row["notices"].as_array().unwrap().is_empty());
        article.insert("notices".into(), row["notices"].clone());
    }
    expected
}
