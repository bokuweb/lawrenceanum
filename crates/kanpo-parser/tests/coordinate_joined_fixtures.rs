fn verify(pdf: &[u8], expected: &str, source: &str, rows: usize) {
    use kanpo_parser::trace::WordStatus;
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let p = &a.pages[0];
    let tables: Vec<_> = p
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .filter(|t| t.cols == 3 && t.cells.iter().any(|c| c.row == 0 && c.text == "北緯"))
        .collect();
    assert_eq!(tables.len(), 1);
    let t = tables[0];
    let mut count = 0;
    for s in expected.lines() {
        let f: Vec<_> = s.split('\t').collect();
        let c = t
            .cells
            .iter()
            .find(|c| {
                c.col == 0
                    && c.text
                        .chars()
                        .map(|c| {
                            if ('０'..='９').contains(&c) {
                                char::from_u32(c as u32 - '０' as u32 + '0' as u32).unwrap()
                            } else {
                                c
                            }
                        })
                        .collect::<String>()
                        == f[0]
            })
            .unwrap();
        for (col, text) in [(1, f[1]), (2, f[2])] {
            assert_eq!(
                t.cells
                    .iter()
                    .find(|x| x.row == c.row && x.col == col)
                    .unwrap()
                    .text,
                text
            );
        }
        count += 1;
    }
    assert_eq!(count, rows);
    let proof: serde_json::Value = serde_json::from_str(source).unwrap();
    let bindings = proof["bindings"].as_array().unwrap();
    assert_eq!(bindings.len(), rows * 2);
    let mut ids = std::collections::HashSet::new();
    for b in bindings {
        let c = t
            .cells
            .iter()
            .find(|c| {
                c.row == b["row"].as_u64().unwrap() as usize
                    && c.col == b["col"].as_u64().unwrap() as usize
            })
            .unwrap();
        let old: Vec<usize> = serde_json::from_value(b["source_ids"].clone()).unwrap();
        assert_eq!(&c.words[..old.len()], old);
        assert_eq!(c.words.len(), old.len() + 3);
        assert_eq!(
            c.text.replace(['°', '′', '″'], ""),
            b["before_text"].as_str().unwrap()
        );
        for s in b["symbols"].as_array().unwrap() {
            let id = s[0].as_u64().unwrap() as usize;
            assert!(ids.insert(id));
            let w = &p.words[id];
            assert_eq!(w.id, id);
            assert_eq!(w.raw, s[1].as_str().unwrap());
            assert_eq!(w.text, s[2].as_str().unwrap());
            assert_eq!(w.status, WordStatus::Used);
            assert!(c.words.contains(&id));
            assert!(c.words.contains(&(s[3].as_u64().unwrap() as usize)));
            let mut n = 0;
            for r in &p.regions {
                for l in &r.lines {
                    assert!(!l.words.contains(&id));
                    if let Some(t) = &l.table {
                        for q in &t.cells {
                            n += usize::from(q.words.contains(&id));
                        }
                    }
                }
            }
            assert_eq!(n, 1);
        }
    }
    assert_eq!(ids.len(), rows * 6);
    let old = proof["baseline_words"].as_array().unwrap();
    assert_eq!(p.words.len(), old.len());
    for (w, b) in p.words.iter().zip(old) {
        assert_eq!(w.id, b["id"].as_u64().unwrap() as usize);
        assert_eq!(w.raw, b["raw"].as_str().unwrap());
        for (k, v) in [("x0", w.x0), ("x1", w.x1), ("y0", w.y0), ("y1", w.y1)] {
            assert!((v as f64 - b[k].as_f64().unwrap()).abs() < 0.001);
        }
        if !ids.contains(&w.id) {
            assert_eq!(w.text, b["text"].as_str().unwrap());
            assert_eq!(serde_json::to_value(w.status).unwrap(), b["status"]);
        } else {
            assert_eq!(b["status"], "empty");
            assert_eq!(b["text"], "");
        }
    }
}
#[test]
fn twelve_paired_coordinates_restore_all_seventy_two_original_symbols() {
    verify(
        include_bytes!("archive/pages/20260824h01774/20260824h017740007.pdf"),
        include_str!("archive/expected-coordinate-joined/20260824h017740007.tsv"),
        include_str!("archive/expected-coordinate-joined/20260824h017740007-source.json"),
        12,
    );
}
#[test]
fn sixteen_paired_coordinates_restore_all_ninety_six_original_symbols() {
    verify(
        include_bytes!("archive/pages/20261014h01807/20261014h018070005.pdf"),
        include_str!("archive/expected-coordinate-joined/20261014h018070005.tsv"),
        include_str!("archive/expected-coordinate-joined/20261014h018070005-source.json"),
        16,
    );
}
fn unchanged(pdf: &[u8], expected: &str) {
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    assert_eq!(a.text.trim_end(), expected.trim_end());
}
#[test]
fn unheaded_continuation_tables_are_unchanged() {
    unchanged(
        include_bytes!("archive/pages/20260824h01774/20260824h017740006.pdf"),
        include_str!("archive/expected-coordinate-joined/20260824h017740006.txt"),
    );
}
#[test]
fn vertical_numeric_geometry_is_not_widened() {
    unchanged(
        include_bytes!("archive/pages/20260728h01756/20260728h017560005.pdf"),
        include_str!("archive/expected-coordinate-joined/20260728h017560005.txt"),
    );
}
#[test]
fn existing_spaced_coordinates_keep_all_existing_recovered_symbols() {
    unchanged(
        include_bytes!("archive/pages/20260928h01796/20260928h017960004.pdf"),
        include_str!("archive/expected-coordinate-joined/20260928h017960004.txt"),
    );
}
