//! Original PDF controls for a source-certified terminal numeric column.
//! Baseline147 JSON anchors unchanged material; it is not an accuracy oracle.
use serde_json::Value;
#[path = "support/native_numeric_frame_contracts.rs"]
mod numeric_contract;
use numeric_contract::{material, source_inventory};
use std::path::PathBuf;
fn data(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/native-numeric-frames")
        .join(name)
}
fn baseline(key: &str) -> Value {
    serde_json::from_slice(&std::fs::read(data(&format!("{key}-baseline147.json"))).unwrap())
        .unwrap()
}
fn analyze(keys: &[&str]) -> Value {
    let inputs: Vec<_> = keys
        .iter()
        .flat_map(|key| {
            kanpo_parser::page_inputs(&std::fs::read(data(&format!("{key}.pdf"))).unwrap()).unwrap()
        })
        .collect();
    // Match the public JSON wire codec: direct to_value expands f32 rectangles
    // into f64 values and would make unchanged source coordinates compare unequal.
    serde_json::from_str(&serde_json::to_string(&kanpo_parser::analyze_item(&inputs)).unwrap())
        .unwrap()
}
fn tables(page: &Value) -> Vec<&Value> {
    page["regions"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| r["lines"].as_array().unwrap())
        .filter_map(|line| line.get("table"))
        .collect()
}
fn cell(table: &Value, row: u64, column: u64) -> &str {
    table["cells"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["row"] == row && c["col"] == column)
        .unwrap()["text"]
        .as_str()
        .unwrap()
}
#[test]
fn original_cash_flow_recovers_each_printed_row_and_terminal_amount() {
    let key = "20260825g001890074";
    let old = baseline(key);
    let new = analyze(&[key]);
    material(&old, &new);
    let ts = tables(&new["pages"][0]);
    assert_eq!(
        ts.iter()
            .map(|t| (t["rows"].as_u64().unwrap(), t["cols"].as_u64().unwrap()))
            .collect::<Vec<_>>(),
        [(38, 2), (24, 2), (5, 2)]
    );
    assert_eq!(ts[0], tables(&old["pages"][0])[0]);
    assert_eq!(cell(ts[1], 0, 0), "買取債権の取得による支出");
    assert_eq!(cell(ts[1], 0, 1), "△973,990,240,000");
    assert_eq!(cell(ts[1], 19, 0), "小計");
    assert_eq!(cell(ts[1], 19, 1), "744,943,589,967");
    assert_eq!(cell(ts[1], 23, 1), "604,181,379,501");
    assert_eq!(cell(ts[2], 4, 0), "有形固定資産の売却による収入");
    assert_eq!(cell(ts[2], 4, 1), "2,794,245,000");
}
#[test]
fn original_detached_signs_gain_unique_printed_word_owners() {
    let key = "20260825g001890074";
    let old = baseline(key);
    let new = analyze(&[key]);
    let added = source_inventory(&old["pages"][0], &new["pages"][0]);
    assert_eq!(added.len(), 12);
    for table in tables(&new["pages"][0]) {
        for c in table["cells"].as_array().unwrap() {
            for id in c["words"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as usize)
            {
                let word = &new["pages"][0]["words"][id];
                assert!(c["x0"].as_f64().unwrap() <= word["x0"].as_f64().unwrap());
                assert!(word["x1"].as_f64().unwrap() <= c["x1"].as_f64().unwrap());
                assert!(c["y0"].as_f64().unwrap() <= word["y0"].as_f64().unwrap());
                assert!(word["y1"].as_f64().unwrap() <= c["y1"].as_f64().unwrap());
                if added.contains(&id) {
                    assert!(c["text"]
                        .as_str()
                        .unwrap()
                        .starts_with(word["text"].as_str().unwrap()));
                }
            }
        }
    }
}
#[test]
fn original_consecutive_pages_keep_all_article_material_and_prior_tables() {
    let old: Value =
        serde_json::from_slice(&std::fs::read(data("consecutive-pages-baseline147.json")).unwrap())
            .unwrap();
    let new = analyze(&[
        "20260825g001890073",
        "20260825g001890074",
        "20260825g001890075",
    ]);
    material(&old, &new);
    assert_eq!(new["articles"].as_array().unwrap().len(), 1);
    assert_eq!(old["pages"][0], new["pages"][0]);
    assert_eq!(old["pages"][2], new["pages"][2]);
    assert_eq!(tables(&new["pages"][1]).len(), 3);
    assert_eq!(
        source_inventory(&old["pages"][1], &new["pages"][1]).len(),
        12
    );
}
#[test]
fn original_asset_frame_keeps_its_independent_announcements() {
    let key = "20260803g001740089";
    let old = baseline(key);
    let new = analyze(&[key]);
    material(&old, &new);
    source_inventory(&old["pages"][0], &new["pages"][0]);
    assert_eq!(new["articles"].as_array().unwrap().len(), 2);
    let ts = tables(&new["pages"][0]);
    assert!(ts.iter().any(|t| t["rows"] == 5
        && t["cols"] == 2
        && cell(t, 0, 0) == "現金及び預金"
        && cell(t, 4, 1) == "256,168,748"));
}
#[test]
fn original_expense_frame_is_not_limited_to_cash_flow_vocabulary() {
    let key = "20260820g001860077";
    let old = baseline(key);
    let new = analyze(&[key]);
    material(&old, &new);
    source_inventory(&old["pages"][0], &new["pages"][0]);
    assert!(tables(&new["pages"][0]).iter().any(|t| t["rows"] == 14
        && t["cols"] == 2
        && cell(t, 0, 0) == "給与・賞与及び手当"
        && cell(t, 13, 1) == "256,579,627"));
}
#[test]
fn original_profit_disposal_with_multiple_amount_lanes_is_unchanged() {
    let key = "20260717g001600100";
    assert_eq!(analyze(&[key]), baseline(key));
}
#[test]
fn original_expenses_with_multiple_amount_lanes_are_unchanged() {
    let key = "20260731g001730066";
    assert_eq!(analyze(&[key]), baseline(key));
}
#[test]
fn original_multi_lane_assets_are_unchanged() {
    for key in ["20261016g002280002", "20261016g002280553"] {
        assert_eq!(analyze(&[key]), baseline(key), "{key}");
    }
}
#[test]
fn original_unframed_financial_flow_is_not_a_numeric_frame() {
    let key = "20260731g001720293";
    assert_eq!(analyze(&[key]), baseline(key));
}

#[test]
fn renderer_fixture_is_the_actual_original_cash_flow_document() {
    let fixture: Value =
        serde_json::from_slice(&std::fs::read(data("renderer-document.json")).unwrap()).unwrap();
    let actual = analyze(&["20260825g001890074"]);
    assert_eq!(actual["articles"][0]["document"], fixture);
}
