use std::collections::{BTreeMap, BTreeSet};
fn check(page: &str) {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/source-frame-fences");
    let input =
        crate::page_inputs(&std::fs::read(root.join(format!("{page}.pdf"))).unwrap()).unwrap();
    let a = crate::analyze_item(&input);
    let gold: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join(format!("{page}-frame-gold.json"))).unwrap(),
    )
    .unwrap();
    let expected = gold["original_independent_frames"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            f["used_Word_ids"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_u64().unwrap() as usize)
                .collect::<BTreeSet<_>>()
        })
        .collect::<Vec<_>>();
    let mut owners = BTreeMap::new();
    for (frame, ids) in expected.iter().enumerate() {
        for &id in ids {
            assert!(owners.insert(id, frame).is_none());
        }
    }
    let used = a.pages[0]
        .words
        .iter()
        .filter(|w| w.status == crate::trace::WordStatus::Used)
        .map(|w| w.id)
        .collect::<BTreeSet<_>>();
    assert_eq!(owners.keys().copied().collect::<BTreeSet<_>>(), used);
    let groups = crate::notice_title_scope_tests::parts(&a, &input);
    let mut seen = BTreeSet::new();
    for group in &groups {
        assert!(!group.is_empty());
        let frame = owners[&group[0]];
        for id in group {
            assert!(seen.insert(*id));
            assert_eq!(
                owners[id], frame,
                "an article cannot borrow the previous frame's source words"
            );
        }
    }
    assert_eq!(seen, used);
    // These originals have one independently reviewed publication per frame.
    // Other fixtures prove only no cross-frame mixing and allow future finer
    // internal partitioning or detection of the currently empty information panel.
    if let Some(n) = gold["necessary_independent_publication_count"].as_u64() {
        assert_eq!(groups.len(), n as usize);
        for wanted in expected {
            assert_eq!(
                groups
                    .iter()
                    .filter(|g| g.iter().copied().collect::<BTreeSet<_>>() == wanted)
                    .count(),
                1
            );
        }
    }
}

#[test]
fn original_20260721g001610168() {
    check("20260721g001610168");
}

#[test]
fn original_20260722g001620107() {
    check("20260722g001620107");
}

#[test]
fn original_20260806g001770086() {
    check("20260806g001770086");
}

#[test]
fn original_20260825g001890119() {
    check("20260825g001890119");
}

#[test]
fn original_20260908g002000062() {
    check("20260908g002000062");
}

#[test]
fn original_20260916g002070076() {
    check("20260916g002070076");
}

#[test]
fn original_20260925g002120075() {
    check("20260925g002120075");
}

#[test]
fn original_20260925g002120088() {
    check("20260925g002120088");
}

#[test]
fn original_20260928g002130113() {
    check("20260928g002130113");
}

#[test]
fn original_20261005g002190079() {
    check("20261005g002190079");
}

#[test]
fn original_20261020g002300078() {
    check("20261020g002300078");
}
