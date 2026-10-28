fn analyze() -> kanpo_parser::Analysis {
    kanpo_parser::analyze_item(
        &kanpo_parser::page_inputs(include_bytes!(
            "data/source-trailing-records/20261020g002300016.pdf"
        ))
        .unwrap(),
    )
}
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
const RECORDS:[(&str,&str);3]=[
("手持ち形電動工具の安全性－第２－15部：ヘッジトリマ及びグラスシャーの個別要求事項","Ｃ9745－２－15"),
("変圧器，電源装置，リアクトル及びこれに類する装置の安全性－第２－７部：玩具用変圧器及び玩具用電源装置の個別要求事項及び試験","Ｃ61558－２－７"),
("定格電圧１kVを超え52kV以下の金属閉鎖形スイッチギヤ及びコントロールギヤ","Ｃ62271－200"),
];
#[test]
fn original_first_long_label_is_not_interrupted_by_its_code() {
    let a = analyze();
    assert!(compact(&a.text).contains(RECORDS[0].0));
}
#[test]
fn original_transformer_label_is_not_interrupted_by_its_code() {
    let a = analyze();
    assert!(compact(&a.text).contains(RECORDS[1].0));
}
#[test]
fn original_switchgear_label_is_not_interrupted_by_its_code() {
    let a = analyze();
    assert!(compact(&a.text).contains(RECORDS[2].0));
}
fn in_runs(runs: &[kanpo_parser::Run], label: &str, code: &str) -> bool {
    runs.iter().any(|r| {
        r.table.as_ref().is_some_and(|t| {
            t.rows.iter().any(|row| {
                let values: Vec<_> = row
                    .iter()
                    .map(|cell| compact(&cell.iter().map(|r| r.text.as_str()).collect::<String>()))
                    .collect();
                values.windows(2).any(|p| p[0] == label && p[1] == code)
                    || row.iter().any(|cell| in_runs(cell, label, code))
            })
        })
    })
}
#[test]
fn original_records_keep_complete_label_and_code_in_the_same_typed_row() {
    let a = analyze();
    for (label, code) in RECORDS {
        assert!(
            a.articles
                .iter()
                .any(|a| a.document.blocks.iter().any(|b| match b {
                    kanpo_parser::Block::Paragraph { runs } => in_runs(runs, label, code),
                    _ => false,
                })),
            "source row must preserve complete label/code ownership: {code}"
        );
    }
}

#[test]
fn all_eighty_two_original_records_conserve_every_native_body_word_once() {
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
        vec![(37, 2), (45, 2)]
    );
    let mut ids = std::collections::HashSet::new();
    for id in tables.iter().flat_map(|t| &t.cells).flat_map(|c| &c.words) {
        assert!(ids.insert(*id), "original Word {id} must occur once");
    }
    let used: std::collections::HashSet<_> = p
        .words
        .iter()
        .filter(|w| w.status == kanpo_parser::trace::WordStatus::Used)
        .map(|w| w.id)
        .collect();
    assert_eq!(ids, used);
    assert_eq!(ids.len(), 180);
}
#[test]
fn original_complete_analysis_keeps_the_reviewed_record_contract() {
    // Regression contract after nonblind review of the 82 name/code rows.
    // This reference is not independent whole-page transcription gold.
    let actual: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&analyze()).unwrap()).unwrap();
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "data/source-trailing-records/20261020g002300016-reviewed142.json"
    ))
    .unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn previously_correct_wrapped_names_do_not_acquire_word_separators() {
    let a = kanpo_parser::analyze_item(
        &kanpo_parser::page_inputs(include_bytes!(
            "archive/pages/20261020g00230/20261020g002300023.pdf"
        ))
        .unwrap(),
    );
    let expected = include_str!("archive/expected-code-pairs/20261020g002300023.tsv")
        .lines()
        .map(|l| l.replace('\t', "　"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(a.text.trim_end(), expected.trim_end());
}
