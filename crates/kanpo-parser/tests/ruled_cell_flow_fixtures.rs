//! Source Word IDs and wording below are frozen test witnesses, never runtime switches.
fn fixture(relative: &str) -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(relative)
        .to_str()
        .unwrap()
        .to_owned()
}
fn analyze(paths: &[&str]) -> kanpo_parser::Analysis {
    let mut inputs = Vec::new();
    for name in paths {
        inputs.extend(kanpo_parser::page_inputs(&std::fs::read(name).unwrap()).unwrap());
    }
    kanpo_parser::analyze_item(&inputs)
}
fn order(a: &kanpo_parser::Analysis, page: usize, expected: &[usize]) {
    let mut ids = Vec::new();
    for r in &a.pages[page].regions {
        for l in &r.lines {
            ids.extend(l.words.iter().copied());
            if let Some(t) = &l.table {
                let mut cells: Vec<_> = t.cells.iter().collect();
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
        "Within a printed row, finish each ruled cell before moving to the next cell"
    );
}
fn originals() -> (String, String) {
    (
        fixture("data/ruled-cell-flow/20260930g002150344.pdf"),
        fixture("data/source-ruling-sections/20260930g002150345.pdf"),
    )
}
#[test]
fn standalone_right_main_cell_finishes_before_its_two_omission_cells() {
    let (_, p) = originals();
    order(&analyze(&[&p]), 0, &[32, 26, 2]);
}
#[test]
fn standalone_left_main_cell_already_finishes_before_its_omission_cells() {
    let (_, p) = originals();
    order(&analyze(&[&p]), 0, &[97, 89, 65]);
}
#[test]
fn previous_page_context_preserves_left_cell_boundaries() {
    let (previous, p) = originals();
    order(&analyze(&[&previous, &p]), 1, &[97, 89, 65]);
}
#[test]
fn previous_page_context_preserves_right_cell_boundaries() {
    let (previous, p) = originals();
    order(&analyze(&[&previous, &p]), 1, &[32, 26, 2]);
}
#[test]
fn the_closed_outer_boundary_remains_before_the_next_article() {
    let (_, p) = originals();
    order(&analyze(&[&p]), 0, &[89, 65, 26, 2, 134, 125, 24]);
}

fn assert_case(name: &str) {
    let refs: serde_json::Value =
        serde_json::from_str(include_str!("data/ruled-cell-flow/references.json")).unwrap();
    let a = analyze(&[&fixture(&format!("data/ruled-cell-flow/{name}.pdf"))]);
    for expected in refs["orders"][name].as_array().unwrap() {
        let ids: Vec<usize> = expected
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as usize)
            .collect();
        order(&a, 0, &ids);
    }
}
#[test]
fn a_complete_wrapped_system_precedes_its_date() {
    assert_case("20260731g001720178");
}
#[test]
fn a_wrapped_system_on_a_continuation_page_precedes_its_date() {
    assert_case("20260731g001720180");
}
#[test]
fn complete_fishery_categories_precede_their_own_quotas() {
    assert_case("20260908h017850006");
}
#[test]
fn each_whaling_record_finishes_before_its_own_quota_and_next_record() {
    assert_case("20260908h017850007");
}
#[test]
fn left_omission_cells_precede_the_rightmost_continued_description() {
    assert_case("20260916g002060274");
}
#[test]
fn route_headers_and_own_rates_remain_with_their_physical_rows() {
    assert_case("20260930g002160061");
}
fn coordinate(relative: &str, point: usize, expected: &[usize], text: &str) {
    let a = analyze(&[&fixture(relative)]);
    let l = a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .find(|l| l.words.contains(&point))
        .unwrap();
    let mut got = l.words.clone();
    let mut wanted = expected.to_vec();
    got.sort_unstable();
    wanted.sort_unstable();
    assert_eq!(
        got, wanted,
        "Keep all original coordinate fields and their six angle glyphs on one source record"
    );
    assert_eq!(l.text, text);
}
#[test]
fn parallel_coordinate_frames_keep_complete_records_and_angle_symbols() {
    coordinate(
        "archive/pages/20260728h01756/20260728h017560005.pdf",
        1969,
        &[
            1969, 1937, 1879, 1821, 1792, 1763, 1705, 1647, 1589, 1560, 1531, 1908, 1850, 1734,
            1676, 1618, 1502,
        ],
        "12　34°43′49.4106″　136°25′35.9276″",
    );
}
#[test]
fn an_already_fragmented_record_uses_its_native_printed_row() {
    coordinate(
        "data/ruled-cell-flow/20261019h018100002.pdf",
        1021,
        &[
            1021, 987, 956, 926, 896, 865, 835, 805, 775, 745, 715, 685, 654, 624, 594, 564, 534,
        ],
        "１　39°03′00.7840″　141°12′09.0973″",
    );
}
#[test]
fn repeated_financial_record_rows_preserve_the_complete_analysis() {
    let a = analyze(&[&fixture("data/ruled-cell-flow/20260716g001590046.pdf")]);
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "data/ruled-cell-flow/20260716g001590046-baseline114.json"
    ))
    .unwrap();
    let actual: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    assert_eq!(
        actual, expected,
        "Do not lose label/amount correspondence, headers, any Word, Table or article"
    );
}

#[test]
fn previous_page_context_keeps_the_native_coordinate_record_complete() {
    let a = analyze(&[
        &fixture("data/ruled-cell-flow/20261019h018100001.pdf"),
        &fixture("data/ruled-cell-flow/20261019h018100002.pdf"),
    ]);
    let mut expected = vec![
        1021, 987, 956, 926, 896, 865, 835, 805, 775, 745, 715, 685, 654, 624, 594, 564, 534,
    ];
    let line = a.pages[1]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .find(|l| l.words.contains(&1021))
        .unwrap();
    let mut actual = line.words.clone();
    actual.sort_unstable();
    expected.sort_unstable();
    assert_eq!(
        actual, expected,
        "Do not mix adjacent comparison prose into the coordinate record under page continuation"
    );
    assert_eq!(line.text, "１　39°03′00.7840″　141°12′09.0973″");
}
