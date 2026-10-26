//! Independent original matrix transcriptions and source-parent counterexamples.
#[path = "support/native_numeric_matrix_contracts.rs"]
mod matrix_contract;
use serde_json::{json, Value};
use std::path::PathBuf;
fn data(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/native-numeric-matrix")
        .join(name)
}
fn legacy(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/native-numeric-frames")
        .join(name)
}
fn pdf(key: &str) -> Vec<u8> {
    let name = format!("{key}.pdf");
    let path = if data(&name).exists() {
        data(&name)
    } else {
        legacy(&name)
    };
    std::fs::read(path).unwrap()
}
fn wire<T: serde::Serialize>(v: &T) -> Value {
    serde_json::from_str(&serde_json::to_string(v).unwrap()).unwrap()
}
fn analyze(keys: &[&str]) -> Value {
    let inputs = keys
        .iter()
        .flat_map(|key| kanpo_parser::page_inputs(&pdf(key)).unwrap())
        .collect::<Vec<_>>();
    wire(&kanpo_parser::analyze_item(&inputs))
}
fn rules(keys: &[&str]) -> Vec<Value> {
    keys.iter()
        .map(|key| wire(&kanpo_parser::page_rules(&pdf(key)).unwrap()))
        .collect()
}
fn value(path: PathBuf) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn baseline(key: &str) -> Value {
    let path = data(&format!("{key}-baseline151.json"));
    value(if path.exists() {
        path
    } else {
        legacy(&format!("{key}-baseline147.json"))
    })
}
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn matrix_case(key: &str, transcription: &str, count: usize) -> Value {
    let a = analyze(&[key]);
    assert_eq!(
        matrix_contract::reviewed_projection(&baseline(key), &a, &rules(&[key])),
        count
    );
    let expected = value(data(transcription));
    let rows = expected.as_array().unwrap();
    let tables = a["pages"][0]["regions"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| r["lines"].as_array().unwrap())
        .filter_map(|l| l.get("table"))
        .filter(|t| {
            t["rows"] == rows.len()
                && t["cols"] == rows[0].as_array().unwrap().len()
                && t["cells"].as_array().unwrap().iter().any(|c| {
                    c["row"] == 0
                        && c["col"] == 0
                        && compact(c["text"].as_str().unwrap()) == rows[0][0].as_str().unwrap()
                })
        })
        .collect::<Vec<_>>();
    assert_eq!(tables.len(), 1);
    for (r, row) in rows.iter().enumerate() {
        for (c, text) in row.as_array().unwrap().iter().enumerate() {
            let cells = tables[0]["cells"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|cell| cell["row"] == r && cell["col"] == c)
                .collect::<Vec<_>>();
            assert_eq!(cells.len(), 1);
            assert_eq!(
                compact(cells[0]["text"].as_str().unwrap()),
                text.as_str().unwrap(),
                "printed row{r}col{c}"
            );
        }
    }
    a
}
#[test]
fn original_profit_disposal_keeps_all_six_rows_and_empty_amount_slots() {
    let a = matrix_case("20260717g001600100", "profit-source-6rows152.json", 1);
    assert_eq!(
        a["articles"][0]["document"],
        value(data("profit-document152.json"))
    );
}
#[test]
fn the_expense_continuation_does_not_infer_columns_from_only_its_first_printed_column() {
    let key = "20260731g001730066";
    let a = analyze(&[key]);
    let before = baseline(key);
    let reviewed = value(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/data/native-numeric-parents/20260731g001730066-reviewed153.json"),
    );
    let xhtml = kanpo_parser::bbox_xhtml(&pdf(key)).unwrap();
    assert_eq!(
        matrix_contract::reviewed_parent_projection(&before, &reviewed, &rules(&[key]), &[xhtml]),
        2
    );
    assert_eq!(a, reviewed);
    // Keep the independent old37row transcription as a literal source witness.
    // The new74row transcription separately fixes all complete parent columns.
    let expected = value(data("expenses-source-37rows152.json"));
    let lines = before["pages"][0]["regions"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| r["lines"].as_array().unwrap())
        .collect::<Vec<_>>();
    let start = lines
        .iter()
        .position(|l| compact(l["text"].as_str().unwrap()) == "経常費用")
        .unwrap();
    for (i, row) in expected.as_array().unwrap().iter().enumerate() {
        let dense = row
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\u{3000}");
        assert_eq!(
            compact(lines[start + i]["text"].as_str().unwrap()),
            compact(&dense)
        );
    }
}
#[test]
fn original_three_amount_rails_keep_their_four_physical_columns() {
    let a = matrix_case("20260831g001940043", "statement-source-15rows152.json", 2);
    assert_eq!(
        a["articles"][0]["document"],
        value(data("statement-document152.json"))
    );
}
#[test]
fn overlapping_source_strokes_do_not_authorize_an_easy_parent_fragment() {
    let key = "20260910g002020088";
    assert_eq!(analyze(&[key]), baseline(key));
}
#[test]
fn an_actual_source_gap_does_not_hide_later_parent_amount_columns() {
    let key = "20261016g002280121";
    assert_eq!(analyze(&[key]), baseline(key));
}
#[test]
fn a_table_starting_at_the_page_bottom_needs_its_unseen_continuation() {
    let key = "20261016g002280003";
    assert_eq!(analyze(&[key]), baseline(key));
}
#[test]
fn unsupported_wrapped_numbered_parent_labels_keep_their_complete_prior_analysis() {
    let key = "20261016g002280002";
    assert_eq!(analyze(&[key]), baseline(key));
}
#[test]
fn actual_consecutive_pages_keep_all_source_and_document_material() {
    let keys = [
        "20260717g001600099",
        "20260717g001600100",
        "20260717g001600101",
    ];
    let old = value(data("consecutive-three-pages-baseline151.json"));
    let new = analyze(&keys);
    assert_eq!(
        matrix_contract::reviewed_projection(&old, &new, &rules(&keys)),
        1
    );
}
fn native_matrix_mut(a: &mut Value) -> &mut Value {
    for r in a["pages"][0]["regions"].as_array_mut().unwrap() {
        for l in r["lines"].as_array_mut().unwrap() {
            if l["table"]["rows"] == 6 && l["table"]["cols"] == 3 {
                return &mut l["table"];
            }
        }
    }
    panic!("missing source matrix")
}
fn document_matrix_mut(a: &mut Value) -> &mut Value {
    for ar in a["articles"].as_array_mut().unwrap() {
        for b in ar["document"]["blocks"].as_array_mut().unwrap() {
            if let Some(rs) = b.get_mut("runs") {
                for r in rs.as_array_mut().unwrap() {
                    if r["table"]["rows"].as_array().is_some_and(|rows| {
                        rows.len() == 6 && rows[0].as_array().unwrap().len() == 3
                    }) {
                        return &mut r["table"];
                    }
                }
            }
        }
    }
    panic!("missing document matrix")
}
#[test]
fn the_projection_contract_rejects_material_style_geometry_and_slot_corruption() {
    let key = "20260717g001600100";
    let old = baseline(key);
    let new = analyze(&[key]);
    let rules = rules(&[key]);
    assert_eq!(matrix_contract::reviewed_projection(&old, &new, &rules), 1);
    let mutations: Vec<fn(&mut Value, &mut Vec<Value>)> = vec![
        |a, _| a["pages"][0]["words"][0]["text"] = json!("changed"),
        |a, _| a["pages"][0]["words"][0]["raw"] = json!("changed raw source"),
        |a, _| a["pages"][0]["words"][0]["x0"] = json!(1.),
        |a, _| {
            let t = native_matrix_mut(a);
            let id = t["cells"][0]["words"][0].clone();
            t["cells"][1]["words"].as_array_mut().unwrap().push(id);
        },
        |a, _| native_matrix_mut(a)["cells"][1]["text"] = json!("0"),
        |a, _| {
            let t = native_matrix_mut(a);
            let x = t["cells"][3]["x1"].as_f64().unwrap() + 0.001;
            t["cells"][3]["x1"] = json!(x);
            t["cells"][4]["x0"] = json!(x);
        },
        |a, _| {
            document_matrix_mut(a)["rows"][0]
                .as_array_mut()
                .unwrap()
                .remove(1);
        },
        |a, _| document_matrix_mut(a)["rows"][0][0][0]["text"] = json!("Ⅰ処分対象利益"),
        |a, _| document_matrix_mut(a)["rows"][0][0][0]["bold"] = json!(true),
        |a, _| {
            for b in a["articles"][0]["document"]["blocks"]
                .as_array_mut()
                .unwrap()
            {
                if let Some(rs) = b.get_mut("runs") {
                    for r in rs.as_array_mut().unwrap() {
                        if r["table"]["rows"]
                            .as_array()
                            .is_some_and(|rows| rows.len() == 16)
                        {
                            r["table"]["rows"][0][0][0]["bold"] = json!(true);
                            return;
                        }
                    }
                }
            }
            panic!("missing prior opaque table");
        },
        |a, _| a["articles"][0]["document"]["format"] = json!("changed"),
        |_, r| r[0]["vertical"] = json!([]),
    ];
    for (i, mutate) in mutations.into_iter().enumerate() {
        let mut a = new.clone();
        let mut r = rules.clone();
        mutate(&mut a, &mut r);
        assert!(
            std::panic::catch_unwind(|| matrix_contract::reviewed_projection(&old, &a, &r))
                .is_err(),
            "corruption{i} must fail"
        );
    }
}
