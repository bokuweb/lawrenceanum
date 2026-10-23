use std::collections::BTreeSet;
fn check(page: &str) {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/source-financial-native");
    let input =
        crate::page_inputs(&std::fs::read(root.join(format!("{page}.pdf"))).unwrap()).unwrap();
    let a = crate::analyze_item(&input);
    let gold: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join(format!("{page}-frame-gold.json"))).unwrap(),
    )
    .unwrap();
    let groups = crate::notice_title_scope_tests::parts(&a, &input)
        .into_iter()
        .map(|ids| ids.into_iter().collect::<BTreeSet<_>>())
        .collect::<Vec<_>>();
    let wanted = gold["original_independent_frames"]
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
    assert_eq!(
        a.articles.len(),
        gold["necessary_independent_publication_count"]
            .as_u64()
            .unwrap() as usize,
        "independent original frames, not baseline article count"
    );
    assert_eq!(groups.len(), wanted.len());
    for expected in wanted {
        assert_eq!(
            groups.iter().filter(|found| **found == expected).count(),
            1,
            "one complete original publication must own all and only its source Word occurrences"
        );
    }
}
#[test]
fn original_20260722g001620112() {
    check("20260722g001620112");
}
#[test]
fn original_20260728g001680085() {
    check("20260728g001680085");
}
#[test]
fn original_20260824g001880111() {
    check("20260824g001880111");
}
#[test]
fn original_20260827g001910056() {
    check("20260827g001910056");
}
#[test]
fn original_20260828g001920063() {
    check("20260828g001920063");
}
#[test]
fn original_20260828g001920078() {
    check("20260828g001920078");
}
#[test]
fn original_20260828g001920071() {
    check("20260828g001920071");
}
#[test]
fn original_20260925g002120073() {
    check("20260925g002120073");
}
#[test]
fn original_20261008g002220093() {
    check("20261008g002220093");
}
#[test]
fn original_20261014g002250072() {
    check("20261014g002250072");
}
#[test]
fn original_20261020g002300087() {
    check("20261020g002300087");
}

#[test]
fn reviewed_original_20260721g001610172() {
    check("20260721g001610172");
}

#[test]
fn reviewed_original_20260723g001630240() {
    check("20260723g001630240");
}

#[test]
fn reviewed_original_20260724g001660092() {
    check("20260724g001660092");
}

#[test]
fn reviewed_original_20260724g001660098() {
    check("20260724g001660098");
}

#[test]
fn reviewed_original_20260724g001660104() {
    check("20260724g001660104");
}

#[test]
fn reviewed_original_20260728g001680079() {
    check("20260728g001680079");
}

#[test]
fn reviewed_original_20260820g001860128() {
    check("20260820g001860128");
}

#[test]
fn reviewed_original_20260825g001890100() {
    check("20260825g001890100");
}

#[test]
fn reviewed_original_20260826g001900072() {
    check("20260826g001900072");
}

#[test]
fn reviewed_original_20260828g001920076() {
    check("20260828g001920076");
}

#[test]
fn reviewed_original_20260918g002100079() {
    check("20260918g002100079");
}

#[test]
fn reviewed_original_20261002g002180088() {
    check("20261002g002180088");
}
