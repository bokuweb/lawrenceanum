use kanpo_parser::{analyze_item, page_inputs};

#[test]
fn three_original_court_records_keep_their_complete_source_membership() {
    let refs: serde_json::Value =
        serde_json::from_str(include_str!("data/native-court-records/references.json")).unwrap();
    let inputs = page_inputs(include_bytes!(
        "data/native-court-records/20260925h017950021.pdf"
    ))
    .unwrap();
    let a = analyze_item(&inputs);
    let value: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    assert_eq!(value["pages"][0]["words"], refs["words"]);
    assert_eq!(value["pages"][0]["text"], refs["page_text"]);
    assert_eq!(value["text"], refs["page_text"]);
    let records = refs["records"].as_array().unwrap();
    assert_eq!(
        a.articles.len(),
        records.len(),
        "Distinct signed court records must not share one article"
    );
    let lines: Vec<_> = a.pages[0].regions.iter().flat_map(|r| &r.lines).collect();
    let mut all_document_text = String::new();
    for (article, record) in a.articles.iter().zip(records) {
        assert_eq!(article.text, record["text"].as_str().unwrap());
        let start = record["lines"][0].as_u64().unwrap() as usize;
        let end = record["lines"][1].as_u64().unwrap() as usize;
        assert_eq!(
            serde_json::json!(lines[start..end]
                .iter()
                .flat_map(|l| l.words.iter().copied())
                .collect::<Vec<_>>()),
            record["word_ids"]
        );
        let mut actual = String::new();
        for block in &article.document.blocks {
            let kanpo_parser::Block::Paragraph { runs } = block else {
                panic!("Original court prose must remain prose")
            };
            for run in runs {
                assert!(run.table.is_none());
                actual.push_str(&run.text);
            }
        }
        let compact = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
        assert_eq!(compact(&actual), compact(&article.text));
        all_document_text.push_str(&actual);
    }
    let expected: String = refs["baseline_document"]["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|b| b["runs"].as_array().unwrap())
        .map(|r| r["text"].as_str().unwrap())
        .collect();
    assert_eq!(
        all_document_text, expected,
        "Every original document byte, including spaces and line breaks, must occur once"
    );
}
