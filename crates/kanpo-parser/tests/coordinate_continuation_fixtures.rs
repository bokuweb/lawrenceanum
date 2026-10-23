use kanpo_parser::trace::WordStatus;
fn verify(pdf: &[u8], source: &str, expected_total: usize, unchanged: bool) {
    let before: serde_json::Value = serde_json::from_str(source).unwrap();
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let p = &a.pages[0];
    let mut n = 0;
    for w in &p.words {
        let symbol = match w.raw.as_str() {
            "\u{EA6F}" => "°",
            "\u{EA6D}" => "′",
            "\u{EA6E}" => "″",
            _ => continue,
        };
        if !unchanged {
            assert_eq!(
                w.text, symbol,
                "original glyph ID {} must remain in its coordinate owner",
                w.id
            );
            assert_eq!(w.status, WordStatus::Used);
            let mut owners = 0;
            for r in &p.regions {
                for l in &r.lines {
                    owners += usize::from(l.words.contains(&w.id));
                    if let Some(t) = &l.table {
                        for c in &t.cells {
                            owners += usize::from(c.words.contains(&w.id));
                        }
                    }
                }
            }
            assert_eq!(owners, 1);
        }
        n += 1;
    }
    assert_eq!(n, expected_total);
    assert_eq!(p.words.len(), before["words"].as_array().unwrap().len());
    for (w, b) in p.words.iter().zip(before["words"].as_array().unwrap()) {
        assert_eq!(w.id, b["id"].as_u64().unwrap() as usize);
        assert_eq!(w.raw, b["raw"].as_str().unwrap());
        for (k, v) in [("x0", w.x0), ("x1", w.x1), ("y0", w.y0), ("y1", w.y1)] {
            assert!((v as f64 - b[k].as_f64().unwrap()).abs() < 0.001);
        }
        if !matches!(w.raw.as_str(), "\u{EA6F}" | "\u{EA6D}" | "\u{EA6E}") || unchanged {
            assert_eq!(w.text, b["text"].as_str().unwrap());
            assert_eq!(serde_json::to_value(w.status).unwrap(), b["status"]);
        }
    }
    assert_eq!(
        a.text,
        before["expected_text"].as_str().unwrap(),
        "full literal text, including original-reviewed article ordering"
    );
    for binding in before["bindings"].as_array().unwrap() {
        let id = binding["word_id"].as_u64().unwrap() as usize;
        let o = &binding["owner"];
        let r = &p.regions[o["region"].as_u64().unwrap() as usize];
        assert_eq!(r.unit, o["unit"].as_u64().unwrap() as usize);
        assert_eq!(r.part, o["part"].as_u64().unwrap() as usize);
        let l = &r.lines[o["line"].as_u64().unwrap() as usize];
        let owner_ids = if o["cell"].is_null() {
            &l.words
        } else {
            &l.table.as_ref().unwrap().cells[o["cell"].as_u64().unwrap() as usize].words
        };
        assert!(owner_ids.contains(&id));
        for anchor in binding["source_numeric_ids"].as_array().unwrap() {
            assert!(owner_ids.contains(&(anchor.as_u64().unwrap() as usize)));
        }
    }
    for e in before["expected_coordinate_fields"].as_array().unwrap() {
        let o = &e["owner"];
        let l = &p.regions[o["region"].as_u64().unwrap() as usize].lines
            [o["line"].as_u64().unwrap() as usize];
        let text = if o["cell"].is_null() {
            &l.text
        } else {
            &l.table.as_ref().unwrap().cells[o["cell"].as_u64().unwrap() as usize].text
        };
        let compact: String = text.split_whitespace().collect();
        assert!(
            compact.contains(e["expected_without_whitespace"].as_str().unwrap()),
            "point {} has incorrect suffix positions",
            e["point"]
        );
    }
    if unchanged {
        assert_eq!(a.text, before["text"].as_str().unwrap());
    }
}
#[test]
fn restores_20260717h017500003() {
    verify(
        include_bytes!("archive/pages/20260717h01750/20260717h017500003.pdf"),
        include_str!("archive/expected-coordinate-continuation/20260717h017500003.json"),
        408,
        false,
    );
}
#[test]
fn restores_20260717h017500004() {
    verify(
        include_bytes!("archive/pages/20260717h01750/20260717h017500004.pdf"),
        include_str!("archive/expected-coordinate-continuation/20260717h017500004.json"),
        360,
        false,
    );
}
#[test]
fn restores_20260824h017740007() {
    verify(
        include_bytes!("archive/pages/20260824h01774/20260824h017740007.pdf"),
        include_str!("archive/expected-coordinate-continuation/20260824h017740007.json"),
        132,
        false,
    );
}
#[test]
fn restores_20261014h018070005() {
    verify(
        include_bytes!("archive/pages/20261014h01807/20261014h018070005.pdf"),
        include_str!("archive/expected-coordinate-continuation/20261014h018070005.json"),
        252,
        false,
    );
}
#[test]
fn preserves_20260716h017490002() {
    verify(
        include_bytes!("archive/pages/20260716h01749/20260716h017490002.pdf"),
        include_str!("archive/expected-coordinate-continuation/20260716h017490002.json"),
        0,
        true,
    );
}
#[test]
fn preserves_20260928h017960004() {
    verify(
        include_bytes!("archive/pages/20260928h01796/20260928h017960004.pdf"),
        include_str!("archive/expected-coordinate-continuation/20260928h017960004.json"),
        300,
        true,
    );
}
