#[path = "support/legacy_analysis.rs"]
mod legacy_analysis;
#[test]
fn fifty_coordinate_pairs_restore_three_verified_symbols_and_preserve_source_ownership() {
    use kanpo_parser::trace::WordStatus;
    let pdf = include_bytes!("archive/pages/20260928h01796/20260928h017960004.pdf");
    let out = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let page = &out.pages[0];
    let expected = include_str!("archive/expected-coordinate-symbols/20260928h017960004.tsv");
    let tables: Vec<_> = page
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .filter(|t| t.cols == 3 && t.cells.iter().any(|c| c.text == "北緯"))
        .collect();
    let mut count = 0;
    for line in expected.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let t = tables[f[0].parse::<usize>().unwrap() - 1];
        let point = t
            .cells
            .iter()
            .find(|c| c.col == 0 && c.text == f[1])
            .unwrap();
        let lat = t
            .cells
            .iter()
            .find(|c| c.row == point.row && c.col == 1)
            .unwrap();
        let lon = t
            .cells
            .iter()
            .find(|c| c.row == point.row && c.col == 2)
            .unwrap();
        assert_eq!(lat.text, f[2]);
        assert_eq!(lon.text, f[3]);
        count += 1;
    }
    assert_eq!(count, 50);
    for ch in "°′″".chars() {
        assert_eq!(out.text.chars().filter(|c| *c == ch).count(), 100);
    }
    let contract: serde_json::Value = serde_json::from_str(include_str!(
        "archive/expected-coordinate-symbols/20260928h017960004-source.json"
    ))
    .unwrap();
    let old = contract["baseline_words"].as_array().unwrap();
    assert_eq!(page.words.len(), old.len());
    for (w, b) in page.words.iter().zip(old) {
        assert_eq!(w.id, b[0].as_u64().unwrap() as usize);
        assert_eq!(w.raw, b[1].as_str().unwrap());
        for (coord, v) in [w.x0, w.y0, w.x1, w.y1]
            .iter()
            .zip(b[4].as_array().unwrap())
        {
            assert!((*coord as f64 - v.as_f64().unwrap()).abs() < 0.001);
        }
        let mapped = match w.raw.as_str() {
            "\u{EA6F}" => Some("°"),
            "\u{EA6D}" => Some("′"),
            "\u{EA6E}" => Some("″"),
            _ => None,
        };
        if let Some(symbol) = mapped {
            assert_eq!(w.text, symbol);
            assert_eq!(w.status, WordStatus::Used);
            assert_eq!(b[3], "empty");
        } else {
            assert_eq!(w.text, b[2].as_str().unwrap());
            assert_eq!(serde_json::to_value(w.status).unwrap(), b[3]);
        }
    }
    let bindings = contract["restored_bindings"].as_array().unwrap();
    assert_eq!(bindings.len(), 300);
    for b in bindings {
        let id = b[0].as_u64().unwrap() as usize;
        let mut owners = Vec::new();
        for r in &page.regions {
            for l in &r.lines {
                assert!(!l.words.contains(&id));
                if let Some(t) = &l.table {
                    for c in &t.cells {
                        if c.words.contains(&id) {
                            owners.push((r, c));
                        }
                    }
                }
            }
        }
        assert_eq!(owners.len(), 1);
        let (r, c) = owners[0];
        assert_eq!(r.unit, b[1].as_u64().unwrap() as usize);
        assert_eq!(r.part, b[2].as_u64().unwrap() as usize);
        assert_eq!(serde_json::to_value(r.role).unwrap(), b[3]);
        assert_eq!(c.row, b[4].as_u64().unwrap() as usize);
        assert_eq!(c.col, b[5].as_u64().unwrap() as usize);
        let anchor = b[6].as_u64().unwrap() as usize;
        assert!(c.words.contains(&anchor));
        let suffix = &page.words[id];
        let numeric = &page.words[anchor];
        let em = numeric.y1 - numeric.y0;
        assert!((-0.35 * em..=0.10 * em).contains(&(suffix.x0 - numeric.x1)));
        assert!((suffix.y0 - numeric.y0).abs() <= em * 0.05);
    }
}
#[test]
fn airport_coordinate_letters_and_unheaded_decimal_minutes_remain_unchanged() {
    let pdf = include_bytes!("archive/pages/20260806g00177/20260806g001770016.pdf");
    let out = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    assert_eq!(
        out.text.trim_end(),
        include_str!("archive/expected-coordinate-symbols/20260806g001770016.txt").trim_end()
    );
}
#[test]
fn all_original_coordinate_frames_restore_with_the_existing_table_unchanged() {
    let pdf = include_bytes!("archive/pages/20260728h01756/20260728h017560005.pdf");
    let out = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    assert_eq!(
        out.text.trim_end(),
        include_str!(
            "archive/expected-coordinate-joined/20260728h017560005-after-continuation.txt"
        )
        .trim_end()
    );
    let original: serde_json::Value = serde_json::from_str(include_str!(
        "archive/expected-coordinate-joined/20260728h017560005-original-control.json"
    ))
    .unwrap();
    let words = original["words"].as_array().unwrap();
    assert_eq!(out.pages[0].words.len(), words.len());
    let mut restored = 0;
    for (word, before) in out.pages[0].words.iter().zip(words) {
        let mut expected = before.clone();
        if before["status"] == "empty" {
            if let Some(symbol) = match word.raw.as_str() {
                "\u{EA6F}" => Some("°"),
                "\u{EA6D}" => Some("′"),
                "\u{EA6E}" => Some("″"),
                _ => None,
            } {
                expected["text"] = serde_json::Value::String(symbol.into());
                expected["status"] = serde_json::Value::String("used".into());
                restored += 1;
            }
        }
        let actual: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(word).unwrap()).unwrap();
        assert_eq!(actual, expected);
    }
    assert_eq!(restored,87*6,"Only the original six angle glyphs in each of the formerly interleaved records are restored");
    let tables: Vec<_> = out.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .map(legacy_analysis::value)
        .collect();
    assert_eq!(tables, original["full_tables"].as_array().unwrap().clone());
    for symbol in ['°', '′', '″'] {
        assert_eq!(out.text.matches(symbol).count(), 116 * 2);
    }
    let mut seen = std::collections::HashSet::new();
    for line in out.pages[0].regions.iter().flat_map(|r| &r.lines) {
        for &id in line.words.iter().chain(
            line.table
                .iter()
                .flat_map(|t| t.cells.iter().flat_map(|c| &c.words)),
        ) {
            assert!(
                seen.insert(id),
                "A source occurrence must not be attached to multiple records"
            );
        }
    }
}
#[test]
fn rotated_trace_retains_exact_numeric_geometry_and_native_suffix_owners() {
    let pdf = include_bytes!("archive/pages/20260717h01750/20260717h017500006.pdf");
    let out = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    assert_eq!(
        out.text.trim_end(),
        include_str!("archive/expected-coordinate-symbols/20260717h017500006.txt").trim_end()
    );
    let trace: serde_json::Value = legacy_analysis::value(&out.pages[0]);
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "archive/expected-coordinate-rotated/20260717h017500006-page.json"
    ))
    .unwrap();
    assert_eq!(trace, expected);
    for symbol in ['°', '′', '″'] {
        assert_eq!(out.text.matches(symbol).count(), 46);
    }
}
