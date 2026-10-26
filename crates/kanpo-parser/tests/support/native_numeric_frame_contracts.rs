//! Preserve prior source material while accepting a reviewed numeric-table projection.
//! The wire JSON anchors source words and content, not an accuracy percentage.
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
fn ids(line: &Value) -> Vec<usize> {
    line["words"]
        .as_array()
        .unwrap()
        .iter()
        .chain(
            line.get("table")
                .into_iter()
                .flat_map(|t| t["cells"].as_array().unwrap())
                .flat_map(|c| c["words"].as_array().unwrap()),
        )
        .map(|id| id.as_u64().unwrap() as usize)
        .collect()
}
fn source_words(page: &Value) -> Vec<usize> {
    page["regions"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| r["lines"].as_array().unwrap())
        .flat_map(ids)
        .collect()
}
fn styled_runs(runs: &Value) -> Vec<(char, Map<String, Value>)> {
    let mut out = vec![];
    for run in runs.as_array().unwrap() {
        let mut style = run.as_object().unwrap().clone();
        style.remove("text");
        style.remove("table");
        out.extend(
            run["text"]
                .as_str()
                .unwrap_or("")
                .chars()
                .filter(|c| *c != '\n')
                .map(|c| (c, style.clone())),
        );
        if let Some(table) = run.get("table") {
            for row in table["rows"].as_array().unwrap() {
                for (column, cell) in row.as_array().unwrap().iter().enumerate() {
                    if column > 0 {
                        out.push(('\u{3000}', Map::new()));
                    }
                    out.extend(styled_runs(cell));
                }
            }
        }
    }
    out
}
fn styled_document(doc: &Value) -> Vec<(char, Map<String, Value>)> {
    let mut out = vec![];
    for block in doc["blocks"].as_array().unwrap() {
        match block["kind"].as_str().unwrap() {
            "paragraph" => out.extend(styled_runs(&block["runs"])),
            "shinkyu" => {
                for row in block["rows"].as_array().unwrap() {
                    out.extend(styled_runs(&row["after"]));
                    out.extend(styled_runs(&row["before"]));
                }
            }
            other => panic!("unexpected block {other}"),
        }
    }
    out
}
pub(crate) fn material(before: &Value, after: &Value) {
    assert_eq!(before["text"], after["text"]);
    assert_eq!(before["format"], after["format"]);
    let old = before["articles"].as_array().unwrap();
    let new = after["articles"].as_array().unwrap();
    assert_eq!(old.len(), new.len());
    for (a, b) in old.iter().zip(new) {
        assert_eq!(a["text"], b["text"]);
        assert_eq!(a["document"]["format"], b["document"]["format"]);
        // Preserve every character, style and ASCII/fullwidth space. Only LF
        // and the new table containers are normalized for this comparison.
        assert_eq!(
            styled_document(&a["document"]),
            styled_document(&b["document"])
        );
    }
}
pub(crate) fn source_inventory(before: &Value, after: &Value) -> BTreeSet<usize> {
    let old = before["words"].as_array().unwrap();
    let new = after["words"].as_array().unwrap();
    assert_eq!(old.len(), new.len());
    let mut added = BTreeSet::new();
    for (a, b) in old.iter().zip(new) {
        let mut a = a.as_object().unwrap().clone();
        let mut b = b.as_object().unwrap().clone();
        let old_status = a.remove("status").unwrap();
        let new_status = b.remove("status").unwrap();
        assert_eq!(a, b, "original Word text/raw/ID/source rectangle changed");
        if old_status != new_status {
            assert_eq!(old_status, "noise");
            assert_eq!(new_status, "used");
            assert!(matches!(
                a["text"].as_str().unwrap(),
                "△" | "▲" | "−" | "－" | "+" | "-"
            ));
            added.insert(a["id"].as_u64().unwrap() as usize);
        }
    }
    let old_ids = source_words(before);
    let new_ids = source_words(after);
    assert_eq!(
        old_ids,
        new_ids
            .iter()
            .filter(|id| !added.contains(id))
            .copied()
            .collect::<Vec<_>>()
    );
    let mut counts = BTreeMap::new();
    for id in &new_ids {
        *counts.entry(*id).or_insert(0) += 1;
    }
    for id in &added {
        assert_eq!(counts.get(id), Some(&1));
    }
    assert_eq!(
        new_ids
            .into_iter()
            .filter(|id| !old_ids.contains(id))
            .collect::<BTreeSet<_>>(),
        added
    );
    added
}

/// Only complete contiguous primitive rows may become two-column tables.
/// Every other line, role, coordinate and prior source occurrence stays exact.
pub(crate) fn reviewed_projection(before: &Value, after: &Value) -> usize {
    material(before, after);
    let old_pages = before["pages"].as_array().unwrap();
    let new_pages = after["pages"].as_array().unwrap();
    assert_eq!(old_pages.len(), new_pages.len());
    let mut promoted = 0;
    for (old, new) in old_pages.iter().zip(new_pages) {
        let added = source_inventory(old, new);
        let mut a = old.as_object().unwrap().clone();
        let mut b = new.as_object().unwrap().clone();
        for key in ["words", "regions"] {
            a.remove(key);
            b.remove(key);
        }
        assert_eq!(a, b);
        let old_regions = old["regions"].as_array().unwrap();
        let new_regions = new["regions"].as_array().unwrap();
        assert_eq!(old_regions.len(), new_regions.len());
        for (a, b) in old_regions.iter().zip(new_regions) {
            let mut am = a.as_object().unwrap().clone();
            let mut bm = b.as_object().unwrap().clone();
            am.remove("lines");
            bm.remove("lines");
            assert_eq!(am, bm);
            let old_lines = a["lines"].as_array().unwrap();
            let mut at = 0;
            for line in b["lines"].as_array().unwrap() {
                if old_lines.get(at) == Some(line) {
                    at += 1;
                    continue;
                }
                let table = line
                    .get("table")
                    .expect("only a numeric table may change a source line");
                let count = table["rows"].as_u64().unwrap() as usize;
                assert!(count >= 2);
                assert_eq!(table["cols"], 2);
                assert!(line["words"].as_array().unwrap().is_empty());
                let original = &old_lines[at..at + count];
                assert!(original.iter().all(|l| l.get("table").is_none()));
                assert_eq!(
                    line["text"],
                    original
                        .iter()
                        .map(|l| l["text"].as_str().unwrap())
                        .collect::<Vec<_>>()
                        .join("\n")
                );
                let cells = table["cells"].as_array().unwrap();
                assert_eq!(cells.len(), count * 2);
                for (row, old_line) in original.iter().enumerate() {
                    let pair: Vec<_> = cells.iter().filter(|c| c["row"] == row as u64).collect();
                    assert_eq!(pair.len(), 2);
                    assert_eq!(pair[0]["col"], 0);
                    assert_eq!(pair[1]["col"], 1);
                    assert_eq!(
                        pair.iter()
                            .map(|c| c["text"].as_str().unwrap())
                            .collect::<Vec<_>>()
                            .join("\u{3000}"),
                        old_line["text"]
                    );
                    let mut retained = vec![];
                    for (column, cell) in pair.into_iter().enumerate() {
                        assert_eq!(cell["rowspan"], 1);
                        assert_eq!(cell["colspan"], 1);
                        for id in cell["words"].as_array().unwrap() {
                            let index = id.as_u64().unwrap() as usize;
                            let word = &new["words"][index];
                            for (low, high) in [("x0", "x1"), ("y0", "y1")] {
                                assert!(cell[low].as_f64().unwrap() <= word[low].as_f64().unwrap());
                                assert!(
                                    word[high].as_f64().unwrap() <= cell[high].as_f64().unwrap()
                                );
                            }
                            if added.contains(&index) {
                                assert_eq!(column, 1);
                                assert!(cell["text"]
                                    .as_str()
                                    .unwrap()
                                    .starts_with(word["text"].as_str().unwrap()));
                            } else {
                                retained.push(id.clone());
                            }
                        }
                    }
                    assert_eq!(retained, *old_line["words"].as_array().unwrap());
                }
                at += count;
                promoted += 1;
            }
            assert_eq!(at, old_lines.len());
        }
    }
    promoted
}
