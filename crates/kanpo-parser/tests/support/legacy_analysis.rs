//! Compare the full legacy schema while separately testing main's additive
//! source_box metadata against the unmodified XHTML occurrences.
pub fn value<T: serde::Serialize>(actual: &T) -> serde_json::Value {
    let mut value: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(actual).unwrap()).unwrap();
    fn visit(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(object) => {
                if ["id", "raw", "status", "x0", "y0", "x1", "y1"]
                    .iter()
                    .all(|key| object.contains_key(*key))
                {
                    let source = object.remove("source_box").expect("new Word metadata");
                    let bounds = source.as_array().unwrap();
                    assert_eq!(bounds.len(), 4);
                    assert!(bounds.iter().all(|n| n.as_f64().unwrap().is_finite()));
                }
                for field in object.values_mut() {
                    visit(field);
                }
            }
            serde_json::Value::Array(array) => array.iter_mut().for_each(visit),
            _ => {}
        }
    }
    visit(&mut value);
    value
}
