fn analyze() -> kanpo_parser::Analysis {
    kanpo_parser::analyze_item(
        &kanpo_parser::page_inputs(include_bytes!(
            "data/source-advance-band/20261020g002300021.pdf"
        ))
        .unwrap(),
    )
}
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn records() -> Vec<(&'static str, &'static str)> {
    include_str!("data/source-advance-band/20261020g002300021-reviewed144.tsv")
        .lines()
        .map(|line| line.split_once('\t').unwrap())
        .collect()
}
#[test]
fn original_photocatalyst_name_is_complete_before_its_code() {
    assert!(compact(&analyze().text).contains(
        "ファインセラミックス－光触媒材料の空気浄化性能試験方法－第１部：窒素酸化物の除去性能"
    ));
}
#[test]
fn original_grinder_name_is_complete_before_its_mixed_origin_code() {
    assert!(compact(&analyze().text)
        .contains("といし－寸法－第12部：ストレートグラインダにおけるばり取り用研削といし"));
}
#[test]
fn original_mixed_origin_code_has_a_structured_table_owner() {
    let a = analyze();
    assert!(a.pages[0].regions.iter().flat_map(|r| &r.lines).any(|l| {
        l.table
            .as_ref()
            .is_some_and(|t| t.cells.iter().any(|c| c.col == 1 && c.text == "Ｒ6211－12"))
    }));
}
#[test]
fn all_eighty_five_reviewed_source_pairs_keep_physical_order() {
    let a = analyze();
    let actual: Vec<_> = a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .flat_map(|t| {
            (0..t.rows).map(move |row| {
                let cell = |col| {
                    &t.cells
                        .iter()
                        .find(|c| c.row == row && c.col == col)
                        .unwrap()
                        .text
                };
                (cell(0).as_str(), cell(1).as_str())
            })
        })
        .collect();
    let expected = records();
    assert_eq!(expected.len(), 85);
    assert_eq!(actual, expected);
}
fn in_runs(runs: &[kanpo_parser::Run], label: &str, code: &str) -> bool {
    runs.iter().any(|r| {
        r.table.as_ref().is_some_and(|t| {
            t.rows.iter().any(|row| {
                let values: Vec<_> = row
                    .iter()
                    .map(|cell| compact(&cell.iter().map(|r| r.text.as_str()).collect::<String>()))
                    .collect();
                values.as_slice() == [label, code]
                    || row.iter().any(|cell| in_runs(cell, label, code))
            })
        })
    })
}
#[test]
fn all_reviewed_pairs_keep_their_typed_document_row_owner() {
    let a = analyze();
    for (label, code) in records() {
        assert!(
            a.articles
                .iter()
                .any(|a| a.document.blocks.iter().any(|b| match b {
                    kanpo_parser::Block::Paragraph { runs } => in_runs(runs, label, code),
                    _ => false,
                })),
            "source name/code row: {code}"
        );
    }
}
#[test]
fn both_original_panels_conserve_every_used_native_word_once() {
    let a = analyze();
    let p = &a.pages[0];
    let tables: Vec<_> = p
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .collect();
    assert_eq!(
        tables.iter().map(|t| (t.rows, t.cols)).collect::<Vec<_>>(),
        vec![(39, 2), (46, 2)]
    );
    let mut ids = std::collections::HashSet::new();
    for id in tables.iter().flat_map(|t| &t.cells).flat_map(|c| &c.words) {
        assert!(ids.insert(*id), "source Word {id} must occur once");
    }
    let used: std::collections::HashSet<_> = p
        .words
        .iter()
        .filter(|w| w.status == kanpo_parser::trace::WordStatus::Used)
        .map(|w| w.id)
        .collect();
    assert_eq!(ids, used);
    assert_eq!(ids.len(), 182);
}
#[test]
fn original_complete_analysis_keeps_the_reviewed_source_contract() {
    // Nonblind regression contract after inspecting all 85 source name/code rows.
    // This reference is not independent whole-page transcription gold.
    let actual: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&analyze()).unwrap()).unwrap();
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "data/source-advance-band/20261020g002300021-reviewed144.json"
    ))
    .unwrap();
    assert_eq!(actual, expected);
}
