//! Physical source relations checked against the original printed pages.
//! These IDs are test oracles, never conditions in the parser.
use std::collections::HashSet;

fn actual(page: &str) -> (crate::Analysis, Vec<crate::PageInput>) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/archive/pages")
        .join(&page[..14])
        .join(format!("{page}.pdf"));
    let inputs = crate::page_inputs(&std::fs::read(path).unwrap()).unwrap();
    (crate::analyze_item(&inputs), inputs)
}
fn original_groups(page: &str, gold: &str) {
    let (a, inputs) = actual(page);
    let gold: serde_json::Value = serde_json::from_str(gold).unwrap();
    let parts = crate::notice_title_scope_tests::parts(&a, &inputs);
    let actual: Vec<HashSet<_>> = parts.iter().map(|p| p.iter().copied().collect()).collect();
    let mut seen = HashSet::new();
    for group in gold["expected_source_groups"].as_array().unwrap() {
        let expected: HashSet<_> = group["source_word_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i.as_u64().unwrap() as usize)
            .collect();
        assert_eq!(
            actual.iter().filter(|p| **p == expected).count(),
            1,
            "an original frame's complete source membership changed: {page}"
        );
        for id in expected {
            assert!(seen.insert(id));
        }
    }
    if gold["complete_page"] == true {
        assert_eq!(
            actual.len(),
            gold["expected_source_groups"].as_array().unwrap().len()
        );
        assert_eq!(
            seen,
            a.pages[0]
                .words
                .iter()
                .filter(|w| matches!(w.status, crate::trace::WordStatus::Used))
                .map(|w| w.id)
                .collect()
        );
        assert_eq!(parts.iter().map(Vec::len).sum::<usize>(), seen.len());
    }
}
#[test]
fn an_original_split_notice_and_its_statement_own_the_complete_common_frame() {
    original_groups(
        "20260721g001610161",
        include_str!("../tests/expected-company-article-sources/20260721g001610161.json"),
    );
}
#[test]
fn eight_original_enclosures_own_all_628_used_words_once() {
    original_groups(
        "20260728g001680070",
        include_str!("../tests/expected-company-article-sources/20260728g001680070.json"),
    );
}
#[test]
fn seven_original_enclosures_own_all_600_used_words_once() {
    original_groups(
        "20261019g002290061",
        include_str!("../tests/expected-company-article-sources/20261019g002290061.json"),
    );
}
fn columns(page: &str, groups: &[&[usize]]) {
    let (a, _) = actual(page);
    for group in groups {
        assert!(
            a.pages[0]
                .regions
                .iter()
                .flat_map(|r| &r.lines)
                .any(|l| l.words == *group),
            "an original column was split or spliced: {page} {group:?}"
        );
    }
}
#[test]
fn spaced_single_character_names_stay_with_their_original_vertical_signatures() {
    columns(
        "20260723g001630240",
        &[&[333, 334, 335], &[345, 346, 347], &[515, 516, 517]],
    );
}
#[test]
fn repeated_name_suffixes_are_not_collected_into_a_foreign_horizontal_line() {
    columns(
        "20260925g002120088",
        &[
            &[308, 309, 310],
            &[356, 357, 358],
            &[493, 494, 495],
            &[549, 550, 551],
            &[579, 580, 581],
        ],
    );
}
#[test]
fn a_statement_date_and_address_are_not_spliced_with_another_notices_prose() {
    let (a, _) = actual("20260817g001830080");
    let lines: Vec<_> = a.pages[0].regions.iter().flat_map(|r| &r.lines).collect();
    let line = |id| lines.iter().position(|l| l.words.contains(&id)).unwrap();
    let (date, address) = (line(516), line(449));
    assert!(date < address);
    assert!(!lines[date..=address].iter().any(|l| l.words.contains(&184)));
}
#[test]
fn separate_original_statement_labels_and_vertical_names_remain_separate() {
    columns("20260928g002130124", &[&[593, 594, 595]]);
    let (a, _) = actual("20260928g002130124");
    let lines: Vec<_> = a.pages[0].regions.iter().flat_map(|r| &r.lines).collect();
    let owners = |id| {
        lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.words.contains(&id))
            .map(|(i, _)| i)
            .collect::<Vec<_>>()
    };
    assert_eq!(owners(583).len(), 1);
    assert_eq!(owners(401).len(), 1);
    assert_ne!(owners(583), owners(401));
}
