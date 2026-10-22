#[path = "support/legacy_analysis.rs"]
mod legacy_analysis;
use kanpo_parser::{analyze_item, page_inputs, trace::WordStatus};
use serde_json::{json, Value};
const P: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/archive/expected-coordinate-rotated");
fn analyze(page: &str) -> kanpo_parser::Analysis {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/archive/pages").join(&page[..14]).join(format!("{page}.pdf"));
    let bytes = std::fs::read(path).unwrap();
    analyze_item(&page_inputs(&bytes).unwrap())
}
#[test]
fn restores_all_twenty_one_native_coordinate_pairs_in_their_source_cells() {
    let a = analyze("20260821h017730004");
    let expected: Value = serde_json::from_str(
        &std::fs::read_to_string(format!("{P}/1773p4-expected-pairs.json")).unwrap(),
    )
    .unwrap();
    let cells: Vec<_> = a.pages[0]
        .regions
        .iter()
        .flat_map(|r| r.lines.iter())
        .filter_map(|l| l.table.as_ref())
        .flat_map(|t| t.cells.iter())
        .collect();
    for row in expected.as_array().unwrap() {
        for key in ["latitude", "longitude"] {
            let value = row[key].as_str().unwrap();
            assert_eq!(
                cells.iter().filter(|c| c.text == value).count(),
                1,
                "point {} {key} must have exact original suffixes in one original cell: {value}",
                row["point"]
            );
        }
    }
    for ch in ['°', '′', '″'] {
        assert_eq!(
            a.text.matches(ch).count(),
            42,
            "all 42 {ch} original glyphs must be restored"
        );
    }
}
#[test]
fn source_ids_raw_geometry_and_prior_owners_remain_exact() {
    let a = analyze("20260821h017730004");
    let before: Value = serde_json::from_str(
        &std::fs::read_to_string(format!("{P}/20260821h017730004-baseline35.json"))
            .unwrap(),
    )
    .unwrap();
    let old = &before["pages"][0];
    let p = &a.pages[0];
    assert_eq!(p.words.len(), old["words"].as_array().unwrap().len());
    let mut added = 0;
    for (w, b) in p.words.iter().zip(old["words"].as_array().unwrap()) {
        assert_eq!(w.id, b["id"].as_u64().unwrap() as usize);
        assert_eq!(w.raw, b["raw"].as_str().unwrap());
        for (k, v) in [("x0", w.x0), ("y0", w.y0), ("x1", w.x1), ("y1", w.y1)] {
            assert_eq!(v, b[k].as_f64().unwrap() as f32);
        }
        let ch = match w.raw.as_str() {
            "\u{EA6F}" => Some("°"),
            "\u{EA6D}" => Some("′"),
            "\u{EA6E}" => Some("″"),
            _ => None,
        };
        if let Some(ch) = ch {
            added += 1;
            assert_eq!(w.text, ch);
            assert_eq!(w.status, WordStatus::Used);
            let mut owners = 0;
            for r in &p.regions {
                for l in &r.lines {
                    owners += usize::from(l.words.contains(&w.id));
                    if let Some(t) = &l.table {
                        for c in &t.cells {
                            if c.words.contains(&w.id) {
                                owners += 1;
                                assert!(
                                    w.x0 >= c.x0 && w.x1 <= c.x1 && w.y0 >= c.y0 && w.y1 <= c.y1
                                );
                            }
                        }
                    }
                }
            }
            assert_eq!(owners, 1);
        } else {
            assert_eq!(
                legacy_analysis::value(w),
                *b
            );
        }
    }
    assert_eq!(added, 126);
    for (r, oldr) in p.regions.iter().zip(old["regions"].as_array().unwrap()) {
        assert_eq!(
            json!([&r.role, r.unit, &r.part]),
            json!([oldr["role"], oldr["unit"], oldr["part"]])
        );
        for (l, oldl) in r.lines.iter().zip(oldr["lines"].as_array().unwrap()) {
            assert_eq!(
                l.text.replace(['°', '′', '″'], ""),
                oldl["text"].as_str().unwrap()
            );
            assert_eq!(serde_json::to_value(&l.words).unwrap(), oldl["words"]);
            if let Some(t) = &l.table {
                let ot = &oldl["table"];
                assert_eq!(t.cells.len(), ot["cells"].as_array().unwrap().len());
                for (c, oc) in t.cells.iter().zip(ot["cells"].as_array().unwrap()) {
                    assert_eq!(
                        c.text.replace(['°', '′', '″'], ""),
                        oc["text"].as_str().unwrap()
                    );
                    let oi: Vec<usize> = oc["words"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_u64().unwrap() as usize)
                        .collect();
                    assert_eq!(&c.words[..oi.len()], oi);
                    let mut cv =
                        serde_json::from_str::<Value>(&serde_json::to_string(c).unwrap()).unwrap();
                    cv["text"] = oc["text"].clone();
                    cv["words"] = oc["words"].clone();
                    assert_eq!(cv, *oc);
                }
            }
        }
    }
    assert_eq!(
        a.text.replace(['°', '′', '″'], ""),
        before["text"].as_str().unwrap()
    );
}
#[test]
fn nearby_noncoordinate_and_already_correct_horizontal_tables_are_unchanged() {
    for page in ["20260821h017730003", "20260717h017500004"] {
        let a = analyze(page);
        let before: Value = serde_json::from_str(
            &std::fs::read_to_string(format!("{P}/{page}-baseline35.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            legacy_analysis::value(&a),
            before,
            "all Analysis negative {page}"
        );
        assert_eq!(
            a.text,
            before["text"].as_str().unwrap(),
            "frozen fresh35 negative {page}"
        );
    }
}
