//! Complete original matrices, including independent printed total columns.
#[path = "support/native_numeric_matrix_contracts.rs"]
mod matrix_contract;
use serde_json::Value;
use std::path::{Path, PathBuf};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests")
}
fn data(name: &str) -> PathBuf {
    root().join("data/native-numeric-parents").join(name)
}
fn value(path: PathBuf) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn pdf(key: &str) -> Vec<u8> {
    let name = format!("{key}.pdf");
    let paths = [
        data(&name),
        root().join("data/native-numeric-frames").join(&name),
        root().join("data/native-numeric-matrix").join(&name),
        root()
            .join("archive/pages")
            .join(&key[..key.len() - 4])
            .join(&name),
    ];
    std::fs::read(
        paths
            .iter()
            .find(|p| p.exists())
            .expect("original PDF fixture"),
    )
    .unwrap()
}
fn wire<T: serde::Serialize>(v: &T) -> Value {
    serde_json::from_str(&serde_json::to_string(v).unwrap()).unwrap()
}
fn analyze(keys: &[&str]) -> (Value, Vec<Value>, Vec<String>) {
    let pdfs = keys.iter().map(|key| pdf(key)).collect::<Vec<_>>();
    let inputs = pdfs
        .iter()
        .flat_map(|pdf| kanpo_parser::page_inputs(pdf).unwrap())
        .collect::<Vec<_>>();
    let a = wire(&kanpo_parser::analyze_item(&inputs));
    let rules = pdfs
        .iter()
        .map(|pdf| wire(&kanpo_parser::page_rules(pdf).unwrap()))
        .collect();
    let xhtml = pdfs
        .iter()
        .map(|pdf| kanpo_parser::bbox_xhtml(pdf).unwrap())
        .collect();
    (a, rules, xhtml)
}
fn baseline(key: &str) -> Value {
    if key == "20260731g001730066" {
        value(root().join("data/native-numeric-frames/20260731g001730066-baseline147.json"))
    } else {
        value(data(&format!("{key}-baseline152.json")))
    }
}
fn printed_cells(a: &Value, page: usize) -> Vec<Vec<String>> {
    a["pages"][page]["regions"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| r["lines"].as_array().unwrap())
        .filter_map(|l| l.get("table"))
        .flat_map(|t| {
            (0..t["rows"].as_u64().unwrap()).map(move |row| {
                t["cells"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|c| c["row"] == row)
                    .map(|c| {
                        c["text"]
                            .as_str()
                            .unwrap()
                            .chars()
                            .filter(|c| !c.is_whitespace())
                            .collect()
                    })
                    .collect()
            })
        })
        .collect()
}
fn checked(key: &str, manual: &str, tables: usize) -> Value {
    let (a, rules, xhtml) = analyze(&[key]);
    assert_eq!(
        matrix_contract::reviewed_parent_projection(&baseline(key), &a, &rules, &xhtml),
        tables
    );
    let expected: Vec<Vec<String>> =
        serde_json::from_slice(&std::fs::read(data(manual)).unwrap()).unwrap();
    assert_eq!(
        printed_cells(&a, 0),
        expected,
        "every printed cell including blank amount slots"
    );
    a
}
#[test]
fn all_seventy_four_original_rows_share_the_complete_parent_column_scheme() {
    let a = checked("20260731g001730066", "expenses-source-74rows153.json", 2);
    assert_eq!(
        a["articles"][0]["document"],
        value(data("expenses-document153.json"))
    );
}
#[test]
fn a_second_printed_column_retains_its_own_totals_and_all_forty_seven_rows() {
    checked("20261016g002280523", "statement-source-47rows153.json", 2);
}
#[test]
fn independent_total_columns_keep_the_four_printed_profit_rows() {
    checked("20260803g001740087", "profit-source-4rows153.json", 1);
}
#[test]
fn an_unseen_following_page_cannot_supply_a_complete_parent() {
    let key = "20261016g002280003";
    let (a, _, _) = analyze(&[key]);
    assert_eq!(
        a,
        value(root().join("data/native-numeric-matrix/20261016g002280003-baseline151.json"))
    );
}
#[test]
fn a_real_source_gap_does_not_supply_a_complete_parent() {
    let key = "20261016g002280121";
    let (a, _, _) = analyze(&[key]);
    assert_eq!(
        a,
        value(root().join("data/native-numeric-matrix/20261016g002280121-baseline151.json"))
    );
}

#[test]
fn actual_consecutive_pages_retain_the_complete_middle_source_parent() {
    let keys = [
        "20260731g001730065",
        "20260731g001730066",
        "20260731g001730067",
    ];
    let (a, rules, xhtml) = analyze(&keys);
    let before = value(data("consecutive-three-pages-baseline152.json"));
    assert_eq!(
        matrix_contract::reviewed_parent_projection(&before, &a, &rules, &xhtml),
        2
    );
    let expected: Vec<Vec<String>> =
        serde_json::from_slice(&std::fs::read(data("expenses-source-74rows153.json")).unwrap())
            .unwrap();
    assert_eq!(printed_cells(&a, 1), expected);
    assert_eq!(a["pages"][0], before["pages"][0]);
    assert_eq!(a["pages"][2], before["pages"][2]);
}
fn first_document_table(a: &mut Value) -> &mut Value {
    for block in a["articles"][0]["document"]["blocks"]
        .as_array_mut()
        .unwrap()
    {
        for run in block["runs"].as_array_mut().unwrap() {
            if run.get("table").is_some() {
                return run.get_mut("table").unwrap();
            }
        }
    }
    panic!("backend source table");
}
#[test]
fn source_material_preservation_alone_cannot_approve_an_incomplete_parent_scheme() {
    let (mut a, rules, xhtml) = analyze(&["20260731g001730066"]);
    let before = baseline("20260731g001730066");
    let words = a["pages"][0]["words"].clone();
    let t = a["pages"][0]["regions"][0]["lines"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .filter_map(|l| l.get_mut("table"))
        .nth(1)
        .unwrap();
    let last = t["cols"].as_u64().unwrap() as usize - 1;
    let edge = t["x1"].as_f64().unwrap();
    let max_word = t["cells"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|c| c["words"].as_array().unwrap())
        .map(|id| words[id.as_u64().unwrap() as usize]["x1"].as_f64().unwrap())
        .reduce(f64::max)
        .unwrap();
    let split = max_word + (edge - max_word) * 0.5;
    let count = t["rows"].as_u64().unwrap() as usize;
    let mut empty = vec![];
    for cell in t["cells"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .filter(|c| c["col"] == last)
    {
        let mut c = cell.clone();
        cell["x1"] = serde_json::json!(split);
        c["col"] = serde_json::json!(last + 1);
        c["x0"] = serde_json::json!(split);
        c["text"] = serde_json::json!("");
        c["words"] = serde_json::json!([]);
        empty.push(c);
    }
    t["cells"].as_array_mut().unwrap().extend(empty);
    t["cols"] = serde_json::json!(last + 2);
    let rows = first_document_table(&mut a)["rows"].as_array_mut().unwrap();
    for row in rows.iter_mut().rev().take(count) {
        row.as_array_mut().unwrap().push(serde_json::json!([]));
    }
    assert_eq!(
        matrix_contract::material_projection_for_counterexample(&before, &a, &rules),
        2,
        "every word/row/text/style remains; this is insufficient parent authority"
    );
    assert!(
        std::panic::catch_unwind(|| matrix_contract::reviewed_parent_projection(
            &before, &a, &rules, &xhtml
        ))
        .is_err(),
        "both source parts must use the same complete columns"
    );
}
#[test]
fn parent_source_and_document_corruptions_are_rejected() {
    let (a, rules, xhtml) = analyze(&["20260731g001730066"]);
    let before = baseline("20260731g001730066");
    for case in 0..12 {
        let mut bad = a.clone();
        let mut rs = rules.clone();
        let mut xs = xhtml.clone();
        match case {
            0 => {
                bad["pages"][0]["words"][0]["raw"] = serde_json::json!("counterfeit");
            }
            1 => {
                bad["pages"][0]["words"][0]["x1"] = serde_json::json!(1000);
            }
            2 => {
                rs[0]["horizontal"].as_array_mut().unwrap().remove(0);
            }
            3 => {
                rs[0]["horizontal"].as_array_mut().unwrap().pop();
            }
            4 => {
                let ts = bad["pages"][0]["regions"][0]["lines"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(|l| l.get("table"))
                    .collect::<Vec<_>>();
                let left = ts[0]["x1"].as_f64().unwrap();
                let right = ts[1]["x0"].as_f64().unwrap();
                rs[0]["vertical"].as_array_mut().unwrap().retain(|v| {
                    let x = v["x"].as_f64().unwrap();
                    !(left < x && x < right)
                });
            }
            5 => {
                xs[0] = xs[0].replace("data-native-axis=\"right\"", "data-native-axis=\"left\"");
            }
            6 => {
                let doc = scraper::Html::parse_document(&xs[0]);
                let sel = scraper::Selector::parse("word").unwrap();
                let id = bad["pages"][0]["regions"][0]["lines"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(|l| l.get("table"))
                    .next()
                    .unwrap()["cells"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|c| {
                        c["col"].as_u64().unwrap() > 0 && !c["words"].as_array().unwrap().is_empty()
                    })
                    .unwrap()["words"][0]
                    .as_u64()
                    .unwrap() as usize;
                let word = &bad["pages"][0]["words"][id];
                let source = doc
                    .select(&sel)
                    .find(|node| {
                        ["xmin", "ymin", "xmax", "ymax"]
                            .iter()
                            .enumerate()
                            .all(|(i, k)| {
                                node.value()
                                    .attr(k)
                                    .unwrap()
                                    .parse::<f32>()
                                    .unwrap()
                                    .to_bits()
                                    == (word["source_box"][i].as_f64().unwrap() as f32).to_bits()
                            })
                    })
                    .unwrap();
                let y = source.value().attr("data-native-flow-baseline").unwrap();
                xs[0] = xs[0].replace(
                    &format!("data-native-flow-baseline=\"{y}\""),
                    "data-native-flow-baseline=\"NaN\"",
                );
            }
            7 => {
                first_document_table(&mut bad)["rows"][0]
                    .as_array_mut()
                    .unwrap()
                    .remove(1);
            }
            8 => {
                first_document_table(&mut bad)["rows"][0][0][0]["bold"] = serde_json::json!(true);
            }
            9 => {
                let t = bad["pages"][0]["regions"][0]["lines"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .filter_map(|l| l.get_mut("table"))
                    .next()
                    .unwrap();
                t["cells"][0]["words"]
                    .as_array_mut()
                    .unwrap()
                    .push(serde_json::json!(0));
            }
            10 => {
                let t = bad["pages"][0]["regions"][0]["lines"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .filter_map(|l| l.get_mut("table"))
                    .next()
                    .unwrap();
                t["cells"][1]["text"] = serde_json::json!("0");
            }
            _ => {
                let t = bad["pages"][0]["regions"][0]["lines"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .filter_map(|l| l.get_mut("table"))
                    .next()
                    .unwrap();
                t["cells"][1]["x1"] = serde_json::json!(1000);
            }
        }
        assert!(
            std::panic::catch_unwind(|| matrix_contract::reviewed_parent_projection(
                &before, &bad, &rs, &xs
            ))
            .is_err(),
            "corruption{case}"
        );
    }
}
