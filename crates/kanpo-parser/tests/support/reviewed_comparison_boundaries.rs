//! Source-reviewed transitions; retain the complete older Analysis as the guard.
use serde_json::Value;

/// The printed supplementary paragraph precedes the next ministry's notice.
/// Move only its complete original Word-ID lines, changing no content or metadata.
pub fn preceding_supplement(mut old: Value) -> Value {
    let regions = old["pages"][0]["regions"].as_array_mut().unwrap();
    let source = regions
        .iter_mut()
        .find(|r| r["unit"] == 2 && r["role"] == "preamble" && r["part"] == 0)
        .unwrap();
    let ids = [127, 128, 137];
    let mut moved = vec![];
    let mut seen = std::collections::BTreeSet::new();
    source["lines"].as_array_mut().unwrap().retain(|line| {
        let words: Vec<_> = line["words"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| id.as_u64().unwrap())
            .collect();
        if words.iter().any(|id| ids.contains(id)) {
            assert!(words.iter().all(|id| ids.contains(id)));
            assert!(line.get("table").is_none());
            seen.extend(words);
            moved.push(line.clone());
            false
        } else {
            true
        }
    });
    assert_eq!(seen, ids.into_iter().collect());
    let target = regions
        .iter_mut()
        .find(|r| r["unit"] == 1 && r["role"] == "outside" && r["part"] == 0)
        .unwrap();
    target["lines"].as_array_mut().unwrap().extend(moved);
    old
}
