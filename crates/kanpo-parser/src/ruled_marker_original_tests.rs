fn check(page: &str, pdf: &[u8]) {
    let a = crate::analyze_item(&crate::page_inputs(pdf).unwrap());
    check_trace(page, &a.pages[0]);
}

fn check_trace(page: &str, trace: &crate::trace::PageTrace) {
    let all: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/data/ruled-marker-bands/references.json"
    ))
    .unwrap();
    let reference = all
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["page"] == page)
        .unwrap();
    let mut roles = std::collections::HashMap::new();
    for r in &trace.regions {
        for l in &r.lines {
            for id in l.words.iter().chain(
                l.table
                    .iter()
                    .flat_map(|t| t.cells.iter().flat_map(|c| &c.words)),
            ) {
                assert!(
                    roles.insert(*id, r.role).is_none(),
                    "{page} duplicate Word{id}"
                );
            }
        }
    }
    for (kind, ids) in reference["expected_Word_roles"].as_object().unwrap() {
        for id in ids.as_array().unwrap() {
            let id = id.as_u64().unwrap() as usize;
            if kind == "outside" {
                assert!(
                    !matches!(
                        roles[&id],
                        crate::trace::Role::After | crate::trace::Role::Before
                    ),
                    "{page} outside Word{id}"
                )
            } else {
                let expected = if kind.ends_with("after") {
                    crate::trace::Role::After
                } else {
                    crate::trace::Role::Before
                };
                assert_eq!(roles[&id], expected, "{page} {kind} Word{id}");
            }
        }
    }
    let sequence: Vec<_> = trace
        .regions
        .iter()
        .flat_map(|r| r.lines.iter())
        .flat_map(|l| {
            l.words.iter().copied().chain(
                l.table
                    .iter()
                    .flat_map(|t| t.cells.iter().flat_map(|c| c.words.iter().copied())),
            )
        })
        .collect();
    let mut previous = None;
    for part in reference["source_part_order"].as_array().unwrap() {
        let key = part.as_str().unwrap();
        let expected: std::collections::HashSet<_> = reference["expected_Word_roles"][key]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as usize)
            .collect();
        let positions: Vec<_> = sequence
            .iter()
            .enumerate()
            .filter(|(_, id)| expected.contains(id))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(
            positions.len(),
            expected.len(),
            "{page} {key} complete field"
        );
        if let Some(&first) = positions.first() {
            if let Some(last) = previous {
                assert!(last < first, "{page} field order {key}")
            }
            previous = positions.last().copied();
        }
    }
    if let Some(orders) = reference
        .get("source_number_head_order")
        .and_then(|v| v.as_object())
    {
        for (part, heads) in orders {
            let expected: Vec<_> = heads
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as usize)
                .collect();
            let wanted: std::collections::HashSet<_> = expected.iter().copied().collect();
            let actual: Vec<_> = sequence
                .iter()
                .copied()
                .filter(|id| wanted.contains(id))
                .collect();
            assert_eq!(actual, expected, "{page} printed item head order {part}");
        }
    }
    if let Some(expected) = reference.get("preserved_comparison_regions") {
        let ids: std::collections::HashSet<_> = reference["preserved_comparison_word_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap())
            .collect();
        let value: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(trace).unwrap()).unwrap();
        let actual: Vec<_> = value["regions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| {
                r["lines"].as_array().unwrap().iter().any(|l| {
                    l["words"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|v| ids.contains(&v.as_u64().unwrap()))
                })
            })
            .cloned()
            .collect();
        assert_eq!(
            &serde_json::Value::Array(actual),
            expected,
            "{page} existing upper comparison must be byte-for-byte equivalent"
        );
    }
}
#[test]
fn source_20260731g001720089() {
    check(
        "20260731g001720089",
        include_bytes!("../tests/data/ruled-marker-bands/20260731g001720089.pdf"),
    );
}
#[test]
fn source_20260731g001720146() {
    check(
        "20260731g001720146",
        include_bytes!("../tests/data/ruled-marker-bands/20260731g001720146.pdf"),
    );
}
#[test]
fn source_20260807g001780056() {
    check(
        "20260807g001780056",
        include_bytes!("../tests/data/ruled-marker-bands/20260807g001780056.pdf"),
    );
}
#[test]
fn source_20260807g001780057() {
    check(
        "20260807g001780057",
        include_bytes!("../tests/data/ruled-marker-bands/20260807g001780057.pdf"),
    );
}
#[test]
fn source_20260821g001870003() {
    check(
        "20260821g001870003",
        include_bytes!("../tests/data/ruled-marker-bands/20260821g001870003.pdf"),
    );
}
#[test]
fn source_20260821g001870004() {
    check(
        "20260821g001870004",
        include_bytes!("../tests/data/ruled-marker-bands/20260821g001870004.pdf"),
    );
}
#[test]
fn source_20260807g001780058() {
    check(
        "20260807g001780058",
        include_bytes!("../tests/data/ruled-marker-bands/20260807g001780058.pdf"),
    );
}

#[test]
fn source_20260731g001720088() {
    check(
        "20260731g001720088",
        include_bytes!("../tests/data/ruled-marker-bands/20260731g001720088.pdf"),
    );
}

// A source-proven local field must retain its roles even after a known global
// comparison page. The prefix is an existing supported source, not fabricated text.
fn check_after_comparison(page: &str, pdf: &[u8]) {
    let mut pages = crate::page_inputs(include_bytes!(
        "../tests/data/ruled-marker-bands/20260724g001650782.pdf"
    ))
    .unwrap();
    pages.extend(crate::page_inputs(pdf).unwrap());
    let a = crate::analyze_item(&pages);
    check_trace(page, &a.pages[1]);
}
macro_rules! comparison_context {
    ($name:ident, $page:literal) => {
        #[test]
        fn $name() {
            check_after_comparison(
                $page,
                include_bytes!(concat!("../tests/data/ruled-marker-bands/", $page, ".pdf")),
            );
        }
    };
}
comparison_context!(context_20260731g001720088, "20260731g001720088");
comparison_context!(context_20260731g001720089, "20260731g001720089");
comparison_context!(context_20260731g001720146, "20260731g001720146");
comparison_context!(context_20260807g001780056, "20260807g001780056");
comparison_context!(context_20260807g001780057, "20260807g001780057");
comparison_context!(context_20260807g001780058, "20260807g001780058");
comparison_context!(context_20260821g001870003, "20260821g001870003");
comparison_context!(context_20260821g001870004, "20260821g001870004");

#[test]
fn consecutive_original_fields_keep_source_roles_and_order() {
    let sources: [(&str, &[u8]); 3] = [
        (
            "20260807g001780056",
            include_bytes!("../tests/data/ruled-marker-bands/20260807g001780056.pdf"),
        ),
        (
            "20260807g001780057",
            include_bytes!("../tests/data/ruled-marker-bands/20260807g001780057.pdf"),
        ),
        (
            "20260807g001780058",
            include_bytes!("../tests/data/ruled-marker-bands/20260807g001780058.pdf"),
        ),
    ];
    let pages: Vec<_> = sources
        .iter()
        .flat_map(|(_, pdf)| crate::page_inputs(pdf).unwrap())
        .collect();
    let a = crate::analyze_item(&pages);
    for ((page, _), trace) in sources.iter().zip(&a.pages) {
        check_trace(page, trace);
    }
}

#[test]
fn local_field_does_not_certify_following_plain_page() {
    let following = crate::page_inputs(include_bytes!(
        "../tests/archive/pages/20260729g00169/20260729g001690035.pdf"
    ))
    .unwrap();
    let single = crate::analyze_item(&following);
    let mut pages = crate::page_inputs(include_bytes!(
        "../tests/data/ruled-marker-bands/20260807g001780056.pdf"
    ))
    .unwrap();
    pages.extend(following);
    let combined = crate::analyze_item(&pages);
    assert!(
        serde_json::to_string(&combined.pages[1]).unwrap()
            == serde_json::to_string(&single.pages[0]).unwrap(),
        "Local source evidence cannot justify legacy guesses on an unrelated following page"
    );
}
