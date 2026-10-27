use kanpo_parser::{analyze_item, page_inputs};
use serde_json::Value;
use std::{collections::BTreeSet, path::PathBuf};
fn gold() -> Vec<Value> {
    serde_json::from_str(include_str!("expected-native-font-aliases157.json")).unwrap()
}
fn strip_aliases(mut xml: String) -> String {
    while let Some(start) = xml.find(" data-native-font-aliases=\"") {
        let value = start + " data-native-font-aliases=\"".len();
        let after = value + xml[value..].find('"').unwrap() + 1;
        xml.replace_range(start..after, "");
    }
    xml
}
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn insertions_only(old: &str, new: &str, allowed: &BTreeSet<char>) {
    let a: Vec<_> = old.chars().collect();
    let b: Vec<_> = new.chars().collect();
    let (mut i, mut j) = (0, 0);
    while i < a.len() {
        assert!(j < b.len(), "original text lost");
        if a[i] == b[j] {
            i += 1;
            j += 1;
        } else {
            assert!(
                allowed.contains(&b[j]),
                "unexpected inserted character {}",
                b[j]
            );
            j += 1;
        }
    }
    assert!(b[j..].iter().all(|c| allowed.contains(c)));
}
fn same_tree(old: &Value, new: &Value, allowed: &BTreeSet<char>, field: &str) {
    match (old, new) {
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len());
            for (k, v) in a {
                same_tree(v, b.get(k).expect("original key"), allowed, k);
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "original boundaries");
            for (a, b) in a.iter().zip(b) {
                same_tree(a, b, allowed, field);
            }
        }
        (Value::String(a), Value::String(b)) if a != b => {
            assert_eq!(field, "text", "raw/source/owner metadata must stay exact");
            insertions_only(a, b, allowed);
        }
        _ => assert_eq!(old, new),
    }
}
fn check(key: &str) {
    let g = gold().into_iter().find(|g| g["page"] == key).unwrap();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(g["fixture"].as_str().unwrap());
    let mut inputs = page_inputs(&std::fs::read(path).unwrap()).unwrap();
    let a = analyze_item(&inputs);
    for p in &mut inputs {
        p.xhtml = strip_aliases(p.xhtml.clone());
    }
    let legacy = analyze_item(&inputs);
    let mut expected_ids = BTreeSet::new();
    let mut restored = BTreeSet::new();
    for row in g["words"].as_array().unwrap() {
        let id = row["id"].as_u64().unwrap() as usize;
        expected_ids.insert(id);
        let n = &a.pages[0].words[id];
        let old = &legacy.pages[0].words[id];
        assert_eq!(n.id, id);
        assert_eq!(n.raw, row["raw"].as_str().unwrap());
        assert_eq!(n.text, row["new"].as_str().unwrap());
        assert_eq!(old.text, row["old"].as_str().unwrap());
        assert_eq!(serde_json::to_value(n.status).unwrap(), row["new_status"]);
        for (i, b) in row["source_box"].as_array().unwrap().iter().enumerate() {
            assert_eq!(n.source_box[i], b.as_f64().unwrap() as f32);
        }
        assert!(
            compact(&a.text).contains(&compact(&n.text)),
            "restored original word in body"
        );
        let mut before = old.text.chars().peekable();
        for c in n.text.chars() {
            if before.peek() == Some(&c) {
                before.next();
            } else {
                restored.insert(c);
            }
        }
        assert!(before.next().is_none());
    }
    let changed_ids: BTreeSet<_> = a.pages[0]
        .words
        .iter()
        .zip(&legacy.pages[0].words)
        .filter(|(a, b)| a.text != b.text)
        .map(|(w, _)| w.id)
        .collect();
    assert_eq!(changed_ids, expected_ids);
    same_tree(
        &serde_json::to_value(legacy).unwrap(),
        &serde_json::to_value(a).unwrap(),
        &restored,
        "",
    );
}
macro_rules! original {
    ($name:ident,$key:literal) => {
        #[test]
        fn $name() {
            check($key);
        }
    };
}
original!(source_20260723_224, "20260723g001630224");
original!(source_20260723_235, "20260723g001630235");
original!(source_20260724_099, "20260724g001660099");
original!(source_20260728_084, "20260728g001680084");
original!(source_20260729_071, "20260729g001700071");
original!(source_20260814_090, "20260814g001820090");
original!(source_20260821_095, "20260821g001870095");
original!(source_20260826_068, "20260826g001900068");
original!(source_20260826_067, "20260826g001900067");
original!(source_20260828_072, "20260828g001920072");
original!(source_20260907_060, "20260907g001990060");
original!(source_20260916_074, "20260916g002070074");
original!(source_20260917_090, "20260917g002080090");
original!(source_20260924_114, "20260924g002110114");
original!(source_20260925_071, "20260925g002120071");
original!(source_20260925_081, "20260925g002120081");
