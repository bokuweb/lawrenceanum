use kanpo_parser::trace::WordStatus;

fn assert_source_dashes(a: &kanpo_parser::Analysis, expected_ids: &[usize]) {
    let p = &a.pages[0];
    let raw_ids: Vec<_> = p.words.iter().filter(|w| w.raw == "\u{E61C}").map(|w| w.id).collect();
    assert_eq!(raw_ids, expected_ids);
    let mut source_ids = std::collections::HashSet::new();
    for line in p.regions.iter().flat_map(|r| &r.lines) {
        if let Some(table) = &line.table {
            assert_eq!(line.text, table.render());
        }
        for &id in line
            .words
            .iter()
            .chain(line.table.iter().flat_map(|t| t.cells.iter().flat_map(|c| &c.words)))
        {
            assert!(source_ids.insert(id), "duplicate source word {id}");
        }
    }
    for &id in expected_ids {
        let w = &p.words[id];
        assert_eq!(w.status, WordStatus::Used);
        assert_eq!(w.text, "－");
        let cells: Vec<_> = p
            .regions
            .iter()
            .flat_map(|r| &r.lines)
            .flat_map(|l| l.table.iter().flat_map(|t| &t.cells))
            .filter(|c| c.words.contains(&id))
            .collect();
        assert_eq!(cells.len(), 1);
        let c = cells[0];
        assert_eq!((c.rowspan, c.colspan), (1, 1));
        assert_eq!(c.text, "－");
        assert!(w.x0 >= c.x0 && w.x1 <= c.x1 && w.y0 >= c.y0 && w.y1 <= c.y1);
    }
    for w in p.words.iter().filter(|w| w.status == WordStatus::Used) {
        assert!(source_ids.contains(&w.id), "missing source word {}", w.id);
    }
}

#[test]
fn merged_asset_labels_keep_ten_separate_original_dash_cells() {
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(include_bytes!("archive/pages/20260930g00216/20260930g002160055.pdf")).unwrap());
    assert_source_dashes(&a, &[0, 1, 2, 3, 4, 5, 6, 7, 8, 93]);
    let tables: Vec<_> = a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .collect();
    assert_eq!(tables.len(), 4);
    let asset_tables: Vec<_> = tables.iter().filter(|t| t.cols == 3).collect();
    assert_eq!(asset_tables.len(), 3);
    assert!(asset_tables.iter().all(|t| t.cells.iter().any(|c| c.colspan == 2)));
    assert_eq!(asset_tables.iter().flat_map(|t| &t.cells).filter(|c| c.text == "③　帳簿価額").count(), 2);
    let continuation = asset_tables.iter().find(|t| t.rows == 12).unwrap();
    assert!(continuation.cells.iter().any(|c| c.text == "⑧　国庫納付等の額" && c.col == 0));
    assert!(a.text.contains("3,075,773,722"));
    assert!(a.text.contains("142,596,224"));
}

#[test]
fn next_page_asset_forms_restore_thirteen_dashes_without_filling_empty_notes() {
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(include_bytes!("archive/pages/20260930g00216/20260930g002160056.pdf")).unwrap());
    assert_source_dashes(&a, &[190, 191, 192, 193, 194, 195, 196, 197, 198, 199, 200, 201, 202]);
    let tables: Vec<_> = a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .collect();
    assert_eq!(tables.iter().flat_map(|t| &t.cells).filter(|c| c.text == "－").count(), 13);
    for t in tables.iter().filter(|t| t.cols == 3) {
        for note in t.cells.iter().filter(|c| c.text.contains("備考")) {
            assert!(!t.cells.iter().any(|c| c.row == note.row && c.col == 2 && c.text == "－"));
        }
    }
    assert!(a.text.contains("1,099,617,668"));
    assert!(a.text.contains("14,374,633,174"));
}
