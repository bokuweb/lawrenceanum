fn assert_source_ids(a: &kanpo_parser::Analysis) {
    let p = &a.pages[0];
    let mut ids = std::collections::HashSet::new();
    for l in p.regions.iter().flat_map(|r| &r.lines) {
        if let Some(t) = &l.table {
            assert_eq!(l.text, t.render());
        }
        for &id in l.words.iter().chain(l.table.iter().flat_map(|t| t.cells.iter().flat_map(|c| &c.words))) {
            assert!(ids.insert(id), "duplicate source word {id}");
        }
    }
    for w in p.words.iter().filter(|w| w.status == kanpo_parser::trace::WordStatus::Used) {
        assert!(ids.contains(&w.id), "missing source word {}", w.id);
    }
}
fn fixture() -> kanpo_parser::Analysis {
    kanpo_parser::analyze_item(&kanpo_parser::page_inputs(include_bytes!("archive/pages/20260930g00215/20260930g002150414.pdf")).unwrap())
}

// 全21行×10欄を原本から転記。右端21値が表外の別の一覧へ流れた退行を検知する。
#[test]
fn longer_outer_frame_keeps_every_last_numeric_cell_in_its_year_row() {
    let a = fixture();
    assert_source_ids(&a);
    let tables: Vec<_> = a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .collect();
    let t = tables.iter().find(|t| (t.x0 - 468.5).abs() < 0.1).unwrap();
    assert_eq!((t.rows, t.cols), (21, 10));
    assert!((t.x1 - 780.5).abs() < 0.1);
    assert!((t.y1 - 421.5).abs() < 0.1);
    let expected = "\
18 1.51 1.55 1.60 1.36 1.48 1.51 1.52 1.35 1.49
19 1.50 1.53 1.54 1.36 1.47 1.50 1.50 1.34 1.48
20 1.45 1.46 1.44 1.32 1.41 1.44 1.45 1.30 1.42
21 1.49 1.53 1.55 1.38 1.46 1.49 1.49 1.34 1.50
22 1.58 1.62 1.67 1.44 1.54 1.56 1.55 1.40 1.60
23 1.59 1.62 1.69 1.45 1.55 1.55 1.54 1.41 1.62
24 1.57 1.54 1.63 1.44 1.53 1.54 1.53 1.40 1.59
25 1.51 1.44 1.54 1.40 1.46 1.49 1.47 1.39 1.51
26 1.42 1.32 1.41 1.35 1.37 1.42 1.37 1.35 1.40
27 1.39 1.29 1.38 1.34 1.35 1.40 1.35 1.34 1.38
28 1.39 1.31 1.39 1.36 1.37 1.42 1.37 1.35 1.39
29 1.37 1.30 1.37 1.34 1.35 1.38 1.35 1.33 1.37
30 1.33 1.27 1.33 1.31 1.32 1.34 1.32 1.30 1.33
令和元 1.32 1.27 1.33 1.30 1.31 1.33 1.31 1.30 1.31
2 1.33 1.29 1.35 1.31 1.33 1.35 1.33 1.31 1.33
3 1.28 1.25 1.30 1.26 1.27 1.29 1.27 1.25 1.27
4 1.24 1.21 1.25 1.22 1.23 1.25 1.23 1.21 1.23
5 1.16 1.14 1.17 1.15 1.15 1.17 1.16 1.14 1.16
6 1.10 1.09 1.10 1.10 1.09 1.11 1.11 1.09 1.10
7 1.05 1.04 1.05 1.05 1.04 1.07 1.05 1.04 1.06
8 1.00 1.00 1.00 1.00 1.00 1.00 1.00 1.00 1.00";
    assert_eq!(expected.lines().count(), t.rows);
    for (r, line) in expected.lines().enumerate() {
        let wanted: Vec<_> = line.split_whitespace().collect();
        assert_eq!(wanted.len(), t.cols);
        for (c, expected) in wanted.iter().enumerate() {
            let cells: Vec<_> = t.cells.iter().filter(|cell| cell.row == r && cell.col == c).collect();
            assert_eq!(cells.len(), 1, "row{r} col{c}");
            let cell = cells[0];
            assert_eq!((cell.rowspan, cell.colspan), (1, 1));
            let text: String = cell
                .text
                .chars()
                .filter(|c| !c.is_whitespace())
                .map(|c| {
                    if ('０'..='９').contains(&c) {
                        char::from_u32(c as u32 - '０' as u32 + '0' as u32).unwrap()
                    } else {
                        c
                    }
                })
                .collect();
            assert_eq!(&text, expected, "row{r} col{c}");
            assert!(!cell.words.is_empty());
        }
    }
}

#[test]
fn extending_the_numeric_edge_keeps_left_table_and_lower_prose_separate() {
    let a = fixture();
    assert_source_ids(&a);
    let lines: Vec<_> = a.pages[0].regions.iter().flat_map(|r| &r.lines).collect();
    let tables: Vec<_> = lines.iter().filter_map(|l| l.table.as_ref()).collect();
    assert_eq!(tables.len(), 2);
    let left = tables.iter().find(|t| (t.x0 - 100.5).abs() < 0.1).unwrap();
    assert_eq!((left.rows, left.cols), (28, 10));
    assert!((left.x1 - 412.5).abs() < 0.1);
    let right = tables.iter().find(|t| (t.x0 - 468.5).abs() < 0.1).unwrap();
    assert_eq!((right.rows, right.cols), (21, 10));
    assert!(!right.cells.iter().any(|c| c.text.contains("備考") || c.text.contains("公営住宅")));
    assert!(lines.iter().any(|l| l.table.is_none() && l.text.contains("備考")));
    for word in a.pages[0].words.iter().filter(|w| right.cells.iter().any(|c| c.words.contains(&w.id))) {
        assert!(word.y1 <= 421.5 + 1.);
    }
    assert!(a.text.contains("簡易耐火構造"));
    assert!(a.text.contains("特殊耐火構造"));
    assert!(a.text.contains("長野県"));
}
