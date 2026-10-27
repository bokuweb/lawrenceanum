//! Actual original PDF Glyph advances prove a terminal band in a complete frame.
#[path = "support/native_numeric_matrix_contracts.rs"]
mod contract;
use serde_json::{json, Value};
use std::path::PathBuf;
const KEYS: [&str; 4] = [
    "20260724g001650654",
    "20260817g001830033",
    "20261016g002280418",
    "20261016g002280573",
];
fn data(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/native-terminal-glyphs")
        .join(name)
}
fn read(name: &str) -> Value {
    serde_json::from_slice(&std::fs::read(data(name)).unwrap()).unwrap()
}
fn wire<T: serde::Serialize>(v: &T) -> Value {
    serde_json::from_str(&serde_json::to_string(v).unwrap()).unwrap()
}
fn original(pdf: &[u8]) -> Value {
    let pages = kanpo_pdf::extract_pages(pdf).unwrap();
    assert_eq!(pages.len(), 1);
    let p = &pages[0];
    json!({"glyphs":p.glyphs.iter().map(|g|json!({"x":g.x.to_string(),"end_x":g.end_x.to_string(),"y":g.y.to_string(),"vertical":g.vertical})).collect::<Vec<_>>(),"words":kanpo_pdf::bbox::words(p).iter().map(|w|json!({"raw":w.text,"box":[w.x_min,w.y_min,w.x_max,w.y_max],"glyphs":w.glyphs})).collect::<Vec<_>>()})
}
fn analyze(keys: &[&str]) -> (Value, Vec<Value>, Vec<String>, Vec<Value>) {
    let pdfs = keys
        .iter()
        .map(|k| {
            let path = data(&format!("{k}.pdf"));
            std::fs::read(if path.exists() {
                path
            } else {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/data/native-numeric-labels")
                    .join(format!("{k}.pdf"))
            })
            .unwrap()
        })
        .collect::<Vec<_>>();
    let inputs = pdfs
        .iter()
        .flat_map(|pdf| kanpo_parser::page_inputs(pdf).unwrap())
        .collect::<Vec<_>>();
    (
        wire(&kanpo_parser::analyze_item(&inputs)),
        inputs
            .iter()
            .map(|i| wire(i.rules.as_ref().unwrap()))
            .collect(),
        inputs.iter().map(|i| i.xhtml.clone()).collect(),
        pdfs.iter().map(|pdf| original(pdf)).collect(),
    )
}
fn manual(k: &str) -> Value {
    json!(read("manual-source117rows156.json")["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["page"] == k)
        .map(|r| vec![r["label"].clone(), r["amount"].clone()])
        .collect::<Vec<_>>())
}
fn source_rows(a: &Value) -> Value {
    json!(a["pages"][0]["regions"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| r["lines"].as_array().unwrap())
        .filter_map(|l| l.get("table"))
        .filter(|t| t["cols"] == 2)
        .flat_map(|t| (0..t["rows"].as_u64().unwrap()).map(move |i| t["cells"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["row"] == i)
            .map(|c| c["text"].clone())
            .collect::<Vec<_>>()))
        .collect::<Vec<_>>())
}
fn checked(k: &str, index: usize) {
    let (a, r, x, o) = analyze(&[k]);
    let old = read(&format!("{k}-baseline155.json"));
    assert_eq!(
        contract::reviewed_terminal_projection(&old, &a, &r, &x, &o),
        1
    );
    assert_eq!(source_rows(&a), manual(k));
    assert_eq!(
        a["articles"][0]["document"],
        read("documents156.json")[index]
    );
}
#[test]
fn original_rates_and_coincident_numbered_labels_keep_all_twenty_amounts() {
    checked(KEYS[0], 0);
}
#[test]
fn a_full_cashflow_parent_keeps_two_empty_section_amounts() {
    checked(KEYS[1], 1);
}
#[test]
fn a_long_cashflow_parent_keeps_all_thirty_amounts() {
    checked(KEYS[2], 2);
}
#[test]
fn another_full_cashflow_parent_keeps_all_thirty_eight_amounts() {
    checked(KEYS[3], 3);
}
fn without_terminal(x: &str) -> String {
    let mut out = x.to_owned();
    while let Some(i) = out.find(" data-native-terminal-glyph=\"") {
        let end = i + " data-native-terminal-glyph=\"".len();
        let end = end + out[end..].find('"').unwrap() + 1;
        out.replace_range(i..end, "");
    }
    out
}
#[test]
fn original_flow_and_nearby_edges_alone_do_not_invent_last_glyph_evidence() {
    for k in KEYS {
        let pdf = std::fs::read(data(&format!("{k}.pdf"))).unwrap();
        let mut inputs = kanpo_parser::page_inputs(&pdf).unwrap();
        for i in &mut inputs {
            let x = without_terminal(&i.xhtml);
            assert!(!x.contains(" data-native-terminal-glyph="));
            i.xhtml = x;
        }
        assert_eq!(
            wire(&kanpo_parser::analyze_item(&inputs)),
            read(&format!("{k}-baseline155.json"))
        );
    }
}
fn rejected(old: &Value, a: &Value, r: &[Value], x: &[String], o: &[Value]) {
    assert!(
        std::panic::catch_unwind(|| contract::reviewed_terminal_projection(old, a, r, x, o))
            .is_err()
    );
}
#[test]
fn authority_rejects_missing_or_corrupted_tail_range_baseline_and_real_original_glyph() {
    let k = KEYS[0];
    let (_, r, x, o) = analyze(&[k]);
    let a = read(&format!("{k}-reviewed156.json"));
    let old = read(&format!("{k}-baseline155.json"));
    rejected(&old, &a, &r, &[without_terminal(&x[0])], &o);
    for attr in [
        "data-native-terminal-glyph",
        "data-native-flow-first-glyph",
        "data-native-flow-after-glyph",
        "data-native-flow-baseline",
    ] {
        let mut wrong = x.clone();
        wrong[0] = wrong[0].replace(&format!("{attr}=\""), &format!("{attr}=\"999 "));
        rejected(&old, &a, &r, &wrong, &o);
    }
    let mut wrong = o.clone();
    wrong[0]["glyphs"][0]["x"] = json!("-123");
    rejected(&old, &a, &r, &x, &wrong);
    let mut wrong = r.clone();
    wrong[0]["horizontal"] = json!([]);
    rejected(&old, &a, &wrong, &x, &o);
}
#[test]
fn material_and_heading_amount_slots_remain_authoritative() {
    let k = KEYS[1];
    let (_, r, x, o) = analyze(&[k]);
    let old = read(&format!("{k}-baseline155.json"));
    let a = read(&format!("{k}-reviewed156.json"));
    for mutate in [
        |v: &mut Value| v["pages"][0]["words"][0]["raw"] = json!("changed"),
        |v: &mut Value| v["pages"][0]["words"][0]["status"] = json!("noise"),
        |v: &mut Value| {
            let t = v["pages"][0]["regions"][0]["lines"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find_map(|l| l.get_mut("table"))
                .unwrap();
            t["cells"][1]["text"] = json!("0");
        },
        |v: &mut Value| {
            let t = v["articles"][0]["document"]["blocks"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .flat_map(|b| b["runs"].as_array_mut().unwrap())
                .find_map(|r| r.get_mut("table"))
                .unwrap();
            t["rows"][0][1] = json!([{"text":"0"}]);
        },
    ] {
        let mut wrong = a.clone();
        mutate(&mut wrong);
        rejected(&old, &wrong, &r, &x, &o);
    }
}

#[test]
fn actual_three_consecutive_pages_change_only_the_newly_certified_source_parent() {
    let keys = [
        "20260724g001650654",
        "20260724g001650655",
        "20260724g001650656",
    ];
    let (a, r, x, o) = analyze(&keys);
    let old = read("consecutive-three-pages-baseline155.json");
    assert_eq!(
        contract::reviewed_terminal_projection(&old, &a, &r, &x, &o),
        1
    );
    assert_eq!(a["pages"][1], old["pages"][1]);
    assert_eq!(a["pages"][2], old["pages"][2]);
    assert_eq!(source_rows(&a), manual(keys[0]));
}
