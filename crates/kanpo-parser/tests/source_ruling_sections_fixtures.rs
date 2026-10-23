//! Printed ruling relationships are source anchors, never production switches.
const ANNEX: &[u8] = include_bytes!("archive/pages/20260731g00172/20260731g001720151.pdf");
const ANNEX_ORDER: &[usize] = &[
    107, 61, 95, 96, 97, 63, 98, 99, 78, 47, 2, 34, 35, 36, 11, 37, 38, 20, 116, 103, 126,
];
fn analyze(bytes: &[u8]) -> kanpo_parser::Analysis {
    kanpo_parser::analyze_item(&kanpo_parser::page_inputs(bytes).unwrap())
}
fn assert_order(a: &kanpo_parser::Analysis, expected: &[usize]) {
    assert_page_order(a, 0, expected)
}
fn assert_page_order(a: &kanpo_parser::Analysis, page: usize, expected: &[usize]) {
    let mut ids = Vec::new();
    for region in &a.pages[page].regions {
        for line in &region.lines {
            ids.extend(line.words.iter().copied());
            if let Some(table) = &line.table {
                let mut cells: Vec<_> = table.cells.iter().collect();
                cells.sort_by_key(|c| (c.row, c.col));
                ids.extend(cells.into_iter().flat_map(|c| c.words.iter().copied()));
            }
        }
    }
    assert_eq!(
        ids.into_iter()
            .filter(|id| expected.contains(id))
            .collect::<Vec<_>>(),
        expected,
        "Complete printed source sections precede the next article"
    );
}
#[test]
fn original_comparison_notes_finish_before_the_full_width_annex() {
    assert_order(&analyze(ANNEX), ANNEX_ORDER);
}
#[test]
fn original_all_upper_omissions_precede_the_next_article() {
    let a = analyze(include_bytes!(
        "data/source-ruling-sections/20260930g002150345.pdf"
    ));
    // The four omissions in the upper panels' second/third columns all
    // precede Article 6, its text, and the following comparison legend.
    // This assertion does not certify the existing internal upper-panel layout.
    assert_order(&a, &[89, 65, 26, 2, 134, 125, 24]);
}
fn mutate_heading(change: impl Fn(&str) -> String) -> Vec<kanpo_parser::PageInput> {
    let mut pages = kanpo_parser::page_inputs(ANNEX).unwrap();
    let mut count = 0;
    pages[0].xhtml = pages[0]
        .xhtml
        .split_inclusive("</word>")
        .map(|part| {
            if part.contains(" data-native-ymin=\"399.785600\"") {
                count += 1;
                change(part)
            } else {
                part.to_owned()
            }
        })
        .collect();
    assert_eq!(
        count, 2,
        "Bind test mutations to the two original heading occurrences"
    );
    pages
}
fn fallback_control() -> kanpo_parser::Analysis {
    let pages = mutate_heading(|part| {
        part.replace("data-native-axis=\"right\"", "data-native-axis=\"unknown\"")
    });
    kanpo_parser::analyze_item(&pages)
}
fn assert_fallback(change: impl Fn(&str) -> String) {
    let a = kanpo_parser::analyze_item(&mutate_heading(change));
    assert_eq!(
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&fallback_control()).unwrap(),
        "Unknown or contradictory source evidence retains the complete legacy Analysis"
    );
}
#[test]
fn the_original_boundary_does_not_depend_on_heading_vocabulary() {
    for labels in [["甲", "乙"], ["x", "y"]] {
        let pages = mutate_heading(|part| {
            part.replace(">附</word>", &format!(">{}</word>", labels[0]))
                .replace(">則</word>", &format!(">{}</word>", labels[1]))
        });
        assert_order(&kanpo_parser::analyze_item(&pages), ANNEX_ORDER);
    }
}
#[test]
fn missing_owned_native_bound_retains_the_legacy_analysis() {
    assert_fallback(|part| part.replace(" data-native-ymin=\"399.785600\"", ""));
}
#[test]
fn a_native_box_crossing_the_rule_retains_the_legacy_analysis() {
    assert_fallback(|part| {
        part.replace(
            "data-native-ymin=\"399.785600\"",
            "data-native-ymin=\"395.785600\"",
        )
    });
}
#[test]
fn a_source_box_changing_sides_retains_the_legacy_analysis() {
    assert_fallback(|part| {
        part.replace(
            "data-native-ymin=\"399.785600\"",
            "data-native-ymin=\"380\"",
        )
        .replace(
            "data-native-ymax=\"407.785600\"",
            "data-native-ymax=\"388\"",
        )
    });
}
#[test]
fn nonfinite_native_geometry_retains_the_legacy_analysis() {
    assert_fallback(|part| {
        part.replace(
            "data-native-ymin=\"399.785600\"",
            "data-native-ymin=\"NaN\"",
        )
    });
}
#[test]
fn the_actual_source_axis_is_required_for_its_own_occurrence() {
    for axis in [
        "",
        "data-native-axis=\"unknown\"",
        "data-native-axis=\"down\"",
    ] {
        assert_fallback(|part| part.replace("data-native-axis=\"right\"", axis));
    }
}

#[test]
fn the_original_annex_boundary_remains_closed_after_the_previous_page() {
    let mut inputs = kanpo_parser::page_inputs(include_bytes!(
        "archive/pages/20260731g00172/20260731g001720150.pdf"
    ))
    .unwrap();
    inputs.extend(kanpo_parser::page_inputs(ANNEX).unwrap());
    assert_page_order(&kanpo_parser::analyze_item(&inputs), 1, ANNEX_ORDER);
}
