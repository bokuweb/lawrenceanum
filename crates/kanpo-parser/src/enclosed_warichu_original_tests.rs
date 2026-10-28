fn check(page: &str, pdf: &[u8]) {
    let all: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/data/enclosed-warichu/references.json"
    ))
    .unwrap();
    let reference = all
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["page"] == page)
        .unwrap();
    let expected: Vec<usize> = reference["ordered_source_words"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as usize)
        .collect();
    let wanted: std::collections::BTreeSet<_> = expected.iter().copied().collect();
    let a = crate::analyze_item(&crate::page_inputs(pdf).unwrap());
    let lines: Vec<_> = a
        .pages
        .iter()
        .flat_map(|p| &p.regions)
        .flat_map(|r| &r.lines)
        .collect();
    let positions: Vec<_> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.words.iter().any(|i| wanted.contains(i)))
        .map(|(i, _)| i)
        .collect();
    assert!(!positions.is_empty(), "missing source citation");
    assert!(
        positions.windows(2).all(|p| p[0] + 1 == p[1]),
        "unrelated text interrupts the citation"
    );
    let actual: Vec<_> = positions
        .iter()
        .flat_map(|&i| lines[i].words.iter().copied())
        .collect();
    assert_eq!(
        actual, expected,
        "complete citation Word ownership and source order"
    );
    let text: String = positions
        .iter()
        .flat_map(|&i| lines[i].text.chars())
        .filter(|c| !c.is_whitespace())
        .collect();
    assert_eq!(text, reference["compact_text"].as_str().unwrap());
    for id in wanted {
        assert_eq!(
            lines
                .iter()
                .flat_map(|l| l.words.iter().chain(
                    l.table
                        .iter()
                        .flat_map(|t| t.cells.iter().flat_map(|c| &c.words))
                ))
                .filter(|&&i| i == id)
                .count(),
            1,
            "source word{id} must have one owner"
        );
    }
}
macro_rules! original {
    ($name:ident,$page:literal) => {
        #[test]
        fn $name() {
            check(
                $page,
                include_bytes!(concat!("../tests/data/enclosed-warichu/", $page, ".pdf")),
            );
        }
    };
}
original!(source_20260731g001720043, "20260731g001720043");
original!(source_20260813h017670002, "20260813h017670002");
original!(source_20260827g001910001, "20260827g001910001");
original!(source_20260831g001930004, "20260831g001930004");
original!(source_20260916g002060046, "20260916g002060046");
original!(source_20260916g002060261, "20260916g002060261");
original!(source_20260916g002060293, "20260916g002060293");
original!(source_20260916g002060296, "20260916g002060296");
original!(source_20260930g002150046, "20260930g002150046");
