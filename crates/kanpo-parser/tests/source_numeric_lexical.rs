//! Original literal labels/rates and independent full-frame source authority.
#[path = "support/native_numeric_matrix_contracts.rs"]
mod contract;
use serde_json::{json, Value};
use std::path::PathBuf;
fn data(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/native-numeric-labels")
        .join(name)
}
fn read(name: &str) -> Value {
    serde_json::from_slice(&std::fs::read(data(name)).unwrap()).unwrap()
}
fn wire<T: serde::Serialize>(v: &T) -> Value {
    serde_json::from_str(&serde_json::to_string(v).unwrap()).unwrap()
}
fn analyze(keys: &[&str]) -> (Value, Vec<Value>, Vec<String>) {
    analyze_profile(keys, true)
}
fn analyze_profile(keys: &[&str], terminal: bool) -> (Value, Vec<Value>, Vec<String>) {
    let pdfs = keys
        .iter()
        .map(|key| std::fs::read(data(&format!("{key}.pdf"))).unwrap())
        .collect::<Vec<_>>();
    let mut inputs = pdfs
        .iter()
        .flat_map(|pdf| kanpo_parser::page_inputs(pdf).unwrap())
        .collect::<Vec<_>>();
    if !terminal {
        for i in &mut inputs {
            i.xhtml = without_terminal(&i.xhtml);
        }
    }
    (
        wire(&kanpo_parser::analyze_item(&inputs)),
        pdfs.iter()
            .map(|pdf| wire(&kanpo_parser::page_rules(pdf).unwrap()))
            .collect(),
        pdfs.iter()
            .map(|pdf| {
                let x = kanpo_parser::bbox_xhtml(pdf).unwrap();
                if terminal {
                    x
                } else {
                    without_terminal(&x)
                }
            })
            .collect(),
    )
}
fn baseline(key: &str) -> Value {
    read(&format!("{key}-baseline153.json"))
}
fn tables(page: &Value) -> Vec<&Value> {
    page["regions"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| r["lines"].as_array().unwrap())
        .filter_map(|l| l.get("table"))
        .collect()
}
fn source_rows(a: &Value, pi: usize) -> Value {
    json!(tables(&a["pages"][pi])
        .into_iter()
        .filter(|t| t["cols"] == 2)
        .flat_map(
            |t| (0..t["rows"].as_u64().unwrap()).map(move |row| t["cells"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|c| c["row"] == row)
                .map(|c| c["text"].clone())
                .collect::<Vec<_>>())
        )
        .collect::<Vec<_>>())
}
fn manual(key: &str) -> Value {
    json!(read("rates-source40rows155.json")["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["page"] == key)
        .map(|r| vec![r["label"].clone(), r["amount"].clone()])
        .collect::<Vec<_>>())
}
fn checked(key: &str, count: usize) -> Value {
    let (a, rules, xhtml) = analyze(&[key]);
    assert_eq!(
        contract::reviewed_lexical_projection(&baseline(key), &a, &rules, &xhtml),
        count
    );
    assert_eq!(
        source_rows(&a, 0),
        manual(key),
        "every independently transcribed label/rate and its source order"
    );
    a
}
#[test]
fn embedded_years_keep_the_first_twenty_six_original_rates_in_their_own_rows() {
    let a = checked("20260724g001650655", 13);
    assert_eq!(
        a["articles"][0]["document"],
        read("rates-documents155.json")[0]
    );
}
#[test]
fn the_next_fourteen_rates_keep_the_independent_existing_grids() {
    let a = checked("20260724g001650656", 7);
    assert_eq!(
        a["articles"][0]["document"],
        read("rates-documents155.json")[1]
    );
    assert_eq!(
        tables(&a["pages"][0])
            .into_iter()
            .filter(|t| t["cols"] == 3)
            .count(),
        2
    );
}
#[test]
fn legacy_flow_profile_does_not_round_distinct_edges_without_terminal_glyph_evidence() {
    let key = "20260724g001650654";
    assert_eq!(analyze_profile(&[key], false).0, baseline(key));
}
#[test]
fn actual_three_consecutive_pages_retain_all_material_and_the_complete_forty_rate_witness() {
    let keys = [
        "20260724g001650654",
        "20260724g001650655",
        "20260724g001650656",
    ];
    let (a, rules, xhtml) = analyze_profile(&keys, false);
    let old = read("consecutive-three-pages-baseline153.json");
    assert_eq!(
        contract::reviewed_lexical_projection(&old, &a, &rules, &xhtml),
        20
    );
    assert_eq!(a["pages"][0], old["pages"][0]);
    assert_eq!(source_rows(&a, 1), manual(keys[1]));
    assert_eq!(source_rows(&a, 2), manual(keys[2]));
}
fn first_table(a: &mut Value) -> &mut Value {
    a["pages"][0]["regions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .flat_map(|r| r["lines"].as_array_mut().unwrap())
        .find_map(|l| l.get_mut("table"))
        .unwrap()
}
fn first_doc_table(a: &mut Value) -> &mut Value {
    a["articles"][0]["document"]["blocks"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .flat_map(|b| b["runs"].as_array_mut().unwrap())
        .find_map(|r| r.get_mut("table"))
        .unwrap()
}
fn rejected(old: &Value, a: &Value, rules: &[Value], xhtml: &[String]) {
    assert!(
        std::panic::catch_unwind(|| contract::reviewed_lexical_projection(old, a, rules, xhtml))
            .is_err()
    );
}
#[test]
fn the_contract_rejects_source_payload_style_ownership_and_original_rule_corruption() {
    let key = "20260724g001650655";
    let old = baseline(key);
    let (_, rules, xhtml) = analyze(&[key]);
    let a = read(&format!("{key}-reviewed155.json"));
    for mutate in [
        |v: &mut Value| v["pages"][0]["words"][0]["raw"] = json!("changed"),
        |v: &mut Value| v["pages"][0]["words"][0]["status"] = json!("noise"),
        |v: &mut Value| v["pages"][0]["words"][0]["source_box"][0] = json!(0),
        |v: &mut Value| {
            first_table(v)["cells"][0]["words"]
                .as_array_mut()
                .unwrap()
                .pop()
                .map(|_| ())
                .unwrap()
        },
        |v: &mut Value| first_table(v)["cells"][1]["text"] = json!("999"),
        |v: &mut Value| first_table(v)["cells"][0]["x1"] = json!(0),
        |v: &mut Value| first_doc_table(v)["rows"][0][0][0]["bold"] = json!(true),
        |v: &mut Value| first_doc_table(v)["rows"][0][0][0]["text"] = json!("changed"),
        |v: &mut Value| {
            first_doc_table(v)["merges"] = json!([{"row":0,"col":0,"rowspan":2,"colspan":1}])
        },
    ] {
        let mut changed = a.clone();
        mutate(&mut changed);
        rejected(&old, &changed, &rules, &xhtml);
    }
    let mut no_cap = rules.clone();
    no_cap[0]["horizontal"] = json!([]);
    rejected(&old, &a, &no_cap, &xhtml);
    let mut no_flow = xhtml.clone();
    no_flow[0] = no_flow[0].replace("data-native-axis=\"right\"", "data-native-axis=\"unknown\"");
    assert_ne!(no_flow, xhtml);
    rejected(&old, &a, &rules, &no_flow);
}
#[test]
fn material_alone_does_not_prove_a_coordinated_extra_column() {
    let key = "20260724g001650655";
    let old = baseline(key);
    let (_, rules, xhtml) = analyze(&[key]);
    let mut a = read(&format!("{key}-reviewed155.json"));
    let words = a["pages"][0]["words"].clone();
    let t = first_table(&mut a);
    let max = t["cells"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|c| c["words"].as_array().unwrap())
        .map(|id| words[id.as_u64().unwrap() as usize]["x1"].as_f64().unwrap())
        .reduce(f64::max)
        .unwrap();
    let edge = t["x1"].as_f64().unwrap();
    let cut = max + (edge - max) * 0.5;
    let mut extra = vec![];
    for cell in t["cells"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .filter(|c| c["col"] == 1)
    {
        let mut empty = cell.clone();
        cell["x1"] = json!(cut);
        empty["col"] = json!(2);
        empty["x0"] = json!(cut);
        empty["text"] = json!("");
        empty["words"] = json!([]);
        extra.push(empty);
    }
    t["cols"] = json!(3);
    t["cells"].as_array_mut().unwrap().extend(extra);
    t["cells"]
        .as_array_mut()
        .unwrap()
        .sort_by_key(|c| (c["row"].as_u64().unwrap(), c["col"].as_u64().unwrap()));
    for row in first_doc_table(&mut a)["rows"].as_array_mut().unwrap() {
        row.as_array_mut().unwrap().push(json!([]));
    }
    assert_eq!(
        contract::lexical_material_projection_for_counterexample(&old, &a, &rules),
        13
    );
    rejected(&old, &a, &rules, &xhtml);
}
#[test]
fn coordinated_word_geometry_cannot_borrow_the_original_source_flow() {
    let key = "20260724g001650655";
    let mut old = baseline(key);
    let (_, rules, xhtml) = analyze(&[key]);
    let mut a = read(&format!("{key}-reviewed155.json"));
    let id = first_table(&mut a)["cells"][0]["words"][0]
        .as_u64()
        .unwrap() as usize;
    let x = a["pages"][0]["words"][id]["x1"].as_f64().unwrap() + 0.125;
    for v in [&mut old, &mut a] {
        v["pages"][0]["words"][id]["x1"] = json!(x);
    }
    assert_eq!(
        contract::lexical_material_projection_for_counterexample(&old, &a, &rules),
        13
    );
    rejected(&old, &a, &rules, &xhtml);
}

// Explicit legacy provenance profile removes only the new optional source field.
fn without_terminal(x: &str) -> String {
    let mut out = x.to_owned();
    while let Some(i) = out.find(" data-native-terminal-glyph=\"") {
        let start = i + " data-native-terminal-glyph=\"".len();
        let end = start + out[start..].find('"').unwrap() + 1;
        out.replace_range(i..end, "");
    }
    out
}
