//! Original-source field witnesses plus regression controls; no whole-page accuracy claim.
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn original() -> kanpo_parser::Analysis {
    let bytes = include_bytes!("data/native-grid-fragments/20260813h017670003.pdf");
    kanpo_parser::analyze_item(&kanpo_parser::page_inputs(bytes).unwrap())
}
fn witnesses() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "data/native-grid-fragments/source-left-cell-witnesses131.json"
    ))
    .unwrap()
}
#[test]
fn all_original_left_cells_keep_their_printed_record_and_column() {
    let a = original();
    let expected = witnesses();
    let tables = a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .collect::<Vec<_>>();
    let t = tables.iter().find(|t| (t.rows, t.cols) == (6, 6)).unwrap();
    for (row, cells) in expected["rows"].as_array().unwrap().iter().enumerate() {
        for (col, value) in cells.as_array().unwrap().iter().enumerate() {
            let c = t
                .cells
                .iter()
                .find(|c| (c.row, c.col) == (row, col))
                .unwrap();
            assert_eq!(
                compact(&c.text),
                compact(value.as_str().unwrap()),
                "row={row} col={col}"
            );
        }
    }
}
#[test]
fn document_continuation_keeps_all_fifteen_records_in_six_columns() {
    let a = original();
    let expected = witnesses();
    let doc: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&a.articles[1].document).unwrap()).unwrap();
    fn tables<'a>(v: &'a serde_json::Value, out: &mut Vec<&'a serde_json::Value>) {
        match v {
            serde_json::Value::Object(o) => {
                if let Some(t) = o.get("table") {
                    out.push(t);
                }
                for (k, v) in o {
                    if k != "table" {
                        tables(v, out)
                    }
                }
            }
            serde_json::Value::Array(v) => {
                for x in v {
                    tables(x, out)
                }
            }
            _ => {}
        }
    }
    let mut ts = vec![];
    tables(&doc, &mut ts);
    assert_eq!(ts.len(), 1);
    let rows = ts[0]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 16);
    assert!(rows.iter().all(|r| r.as_array().unwrap().len() == 6));
    for (i, row) in expected["rows"].as_array().unwrap().iter().enumerate() {
        for (j, value) in row.as_array().unwrap().iter().enumerate() {
            let text = rows[i][j]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r["text"].as_str().unwrap())
                .collect::<String>();
            assert_eq!(
                compact(&text),
                compact(value.as_str().unwrap()),
                "row={i} col={j}"
            );
        }
    }
    for (i, row) in rows.iter().skip(1).enumerate() {
        let text = row[0]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["text"].as_str().unwrap())
            .collect::<String>();
        assert!(text.starts_with(&format!("第{}号", 31776 + i)));
    }
}
#[test]
fn unrelated_amendment_and_complete_right_table_stay_verbatim() {
    let a = original();
    let actual: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    let baseline: serde_json::Value = serde_json::from_str(include_str!(
        "data/native-grid-fragments/20260813h017670003-baseline130.json"
    ))
    .unwrap();
    assert_eq!(actual["articles"][0], baseline["articles"][0]);
    let table = |v: &serde_json::Value| {
        v["pages"][0]["regions"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|r| r["lines"].as_array().unwrap())
            .find_map(|l| (l["table"]["rows"] == 10).then(|| l["table"].clone()))
            .unwrap()
    };
    assert_eq!(table(&actual), table(&baseline));
    assert_eq!(actual["pages"][0]["words"], baseline["pages"][0]["words"]);
}

#[test]
fn complete_reviewed_analysis_is_a_regression_contract() {
    // This candidate golden freezes reviewed behavior; it is not an independent
    // oracle for unresolved amendment roles or every other page-quality issue.
    let actual: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&original()).unwrap()).unwrap();
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "data/native-grid-fragments/20260813h017670003-reviewed131.json"
    ))
    .unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn original_unruled_statement_rows_keep_labels_and_amounts_paired() {
    let bytes = include_bytes!("archive/pages/20260716g00159/20260716g001590045.pdf");
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(bytes).unwrap());
    let tables = a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .collect::<Vec<_>>();
    assert_eq!(tables.len(), 6);
    assert!(tables.iter().all(|t| (t.rows, t.cols) == (5, 4)));
    let t = tables[0];
    for (row, label, amount) in [
        (1, "流動負債", "20,651"),
        (2, "固定負債", "180,672"),
        (3, "剰余金", "249,226"),
    ] {
        for (col, expected) in [(2, label), (3, amount)] {
            assert_eq!(
                t.cells
                    .iter()
                    .find(|c| (c.row, c.col) == (row, col))
                    .unwrap()
                    .text,
                expected
            );
        }
    }
}
