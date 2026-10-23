fn verify(pdf: &[u8], expected: &str, source: &str) {
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    assert_eq!(a.text.trim_end(), expected.trim_end());
    let p = &a.pages[0];
    let proof: serde_json::Value = serde_json::from_str(source).unwrap();
    let mut ids = std::collections::HashSet::new();
    for b in proof["bindings"].as_array().unwrap() {
        let s = &b["glyph"];
        let id = s["id"].as_u64().unwrap() as usize;
        assert!(ids.insert(id));
        let w = &p.words[id];
        assert_eq!(w.id, id);
        assert_eq!(w.raw, "\u{E68D}");
        assert_eq!(w.text, "㎡");
        assert_eq!(w.status, kanpo_parser::trace::WordStatus::Used);
        let mut owners = vec![];
        for r in &p.regions {
            for l in &r.lines {
                if l.words.contains(&id) {
                    assert!(l.table.is_none());
                    owners.push((r, l));
                }
                if let Some(t) = &l.table {
                    assert!(!t.cells.iter().any(|c| c.words.contains(&id)));
                }
            }
        }
        assert_eq!(owners.len(), 1);
        let (r, l) = owners[0];
        assert_eq!(r.unit, b["region_unit"].as_u64().unwrap() as usize);
        assert_eq!(r.part, b["region_part"].as_u64().unwrap() as usize);
        assert_eq!(serde_json::to_value(r.role).unwrap(), b["region_role"]);
        assert_eq!(l.text, b["after_owner_text"].as_str().unwrap());
        assert_eq!(
            serde_json::to_value(&l.words).unwrap(),
            b["after_owner_ids"]
        );
    }
    assert_eq!(ids.len(), 2);
    let old = proof["baseline_words"].as_array().unwrap();
    assert_eq!(p.words.len(), old.len());
    for (w, b) in p.words.iter().zip(old) {
        assert_eq!(w.id, b["id"].as_u64().unwrap() as usize);
        assert_eq!(w.raw, b["raw"].as_str().unwrap());
        for (k, v) in [("x0", w.x0), ("y0", w.y0), ("x1", w.x1), ("y1", w.y1)] {
            assert!((v as f64 - b[k].as_f64().unwrap()).abs() < 0.001);
        }
        if ids.contains(&w.id) {
            assert_eq!(b["status"], "empty");
            assert_eq!(b["text"], "");
        } else {
            assert_eq!(w.text, b["text"].as_str().unwrap());
            assert_eq!(serde_json::to_value(w.status).unwrap(), b["status"]);
        }
    }
}
macro_rules! form {
    ($name:ident,$pdf:literal,$text:literal,$source:literal) => {
        #[test]
        fn $name() {
            verify(
                include_bytes!($pdf),
                include_str!($text),
                include_str!($source),
            );
        }
    };
}
form!(
    blank_area_form_117,
    "archive/pages/20260731g00172/20260731g001720117.pdf",
    "archive/expected-blank-area/20260731g001720117.txt",
    "archive/expected-blank-area/20260731g001720117-source.json"
);
form!(
    blank_area_form_122,
    "archive/pages/20260731g00172/20260731g001720122.pdf",
    "archive/expected-blank-area/20260731g001720122.txt",
    "archive/expected-blank-area/20260731g001720122-source.json"
);
form!(
    blank_area_form_127,
    "archive/pages/20260731g00172/20260731g001720127.pdf",
    "archive/expected-blank-area/20260731g001720127.txt",
    "archive/expected-blank-area/20260731g001720127-source.json"
);
form!(
    blank_area_form_133,
    "archive/pages/20260731g00172/20260731g001720133.pdf",
    "archive/expected-blank-area/20260731g001720133.txt",
    "archive/expected-blank-area/20260731g001720133-source.json"
);
form!(
    blank_area_form_139,
    "archive/pages/20260731g00172/20260731g001720139.pdf",
    "archive/expected-blank-area/20260731g001720139.txt",
    "archive/expected-blank-area/20260731g001720139-source.json"
);
#[test]
fn ambiguous_shared_numeric_unit_stays_unowned() {
    let a = kanpo_parser::analyze_item(
        &kanpo_parser::page_inputs(include_bytes!(
            "archive/pages/20260803g00174/20260803g001740106.pdf"
        ))
        .unwrap(),
    );
    assert_eq!(
        a.text.trim_end(),
        include_str!("archive/expected-blank-area/20260803g001740106.txt").trim_end()
    );
    let w = &a.pages[0].words[170];
    assert_eq!(w.raw, "\u{E68D}");
    assert_eq!(w.text, "");
    assert_eq!(w.status, kanpo_parser::trace::WordStatus::Empty);
}
#[test]
fn matching_labels_without_real_rules_do_not_prove_unit_ownership() {
    let mut input = kanpo_parser::page_inputs(include_bytes!(
        "archive/pages/20260731g00172/20260731g001720117.pdf"
    ))
    .unwrap();
    input[0].rules = None;
    let a = kanpo_parser::analyze_item(&input);
    let ws: Vec<_> = a.pages[0]
        .words
        .iter()
        .filter(|w| w.raw == "\u{E68D}")
        .collect();
    assert_eq!(ws.len(), 2);
    assert!(ws
        .iter()
        .all(|w| w.text.is_empty() && w.status == kanpo_parser::trace::WordStatus::Empty));
}
