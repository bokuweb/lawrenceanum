use super::*;
fn original() -> (Vec<crate::PageInput>, String, Vec<Option<PageRules>>) {
    let inputs = crate::page_inputs(include_bytes!("../tests/data/workbook/case-04.pdf")).unwrap();
    let xhtml = crate::combine_pages(inputs.iter().map(|p| p.xhtml.as_str()));
    let rules = inputs.iter().map(|p| p.rules.clone()).collect();
    (inputs, xhtml, rules)
}
#[test]
fn original_sparse_continuation_retains_empty_columns_partial_crossbars_and_final_spanning_row() {
    let (_, xhtml, rules) = original();
    let mut pages = crate::vertical::analyze_pages_with_rules(&xhtml, &rules);
    let pairs = restore(&mut pages, &xhtml, &rules);
    assert_eq!(pairs.len(), 14);
    for p in pages.iter().take(8).skip(1) {
        assert_eq!(p.tables.len(), 2);
        assert!(p.tables.iter().all(|t| t.cols == 4));
    }
    let t = &pages[1].tables[0];
    assert_eq!(t.rows, 2);
    for col in 0..2 {
        let c = t.cells.iter().find(|c| c.col == col).unwrap();
        assert_eq!(c.rowspan, 2);
        assert!(c.text.is_empty());
    }
    let c = t.cells.iter().find(|c| c.col == 3 && c.row == 0).unwrap();
    assert!(c
        .text
        .starts_with("４　発信転送を行う機能のみを提供する場合"));
    assert!(t
        .cells
        .iter()
        .any(|c| c.col == 2 && c.row == 1 && !c.text.is_empty()));
    let end = &pages[7].tables[0];
    assert_eq!(end.rows, 2);
    assert!(end
        .cells
        .iter()
        .any(|c| c.row == 1 && c.colspan == 4 && c.text == "［略］"));
    assert!(pages[7]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .any(|l| l.table.is_none() && l.text == "［注１〜４　略］"));
    let texts = pages.iter().map(|p| p.text.clone()).collect::<Vec<_>>();
    assert_eq!(restore(&mut pages, &xhtml, &rules), pairs);
    assert_eq!(
        texts,
        pages.iter().map(|p| p.text.clone()).collect::<Vec<_>>()
    );
    let groups = groups(&pages, &pairs);
    assert_eq!(groups.len(), 2);
    assert_eq!(
        groups
            .iter()
            .map(|g| g.table.rows.len())
            .collect::<Vec<_>>(),
        [6, 5]
    );
    let analysis = crate::analysis_from_pages(pages.clone(), &xhtml, &rules);
    let actual = analysis
        .articles
        .iter()
        .flat_map(|a| &a.document.blocks)
        .flat_map(|b| match b {
            crate::Block::Paragraph { runs } => runs.iter().collect::<Vec<_>>(),
            crate::Block::Shinkyu { rows, .. } => rows
                .iter()
                .flat_map(|r| r.after.iter().chain(&r.before))
                .collect(),
        })
        .filter_map(|r| r.table.as_ref())
        .collect::<Vec<_>>();
    for g in groups {
        let n = g.table.rows.len();
        assert!(g
            .table
            .merges
            .iter()
            .any(|m| m.row == 2 && m.col == 0 && m.rowspan == n - 3));
        assert!(g
            .table
            .merges
            .iter()
            .any(|m| m.row == n - 1 && m.colspan == 4));
        assert!(actual.contains(&&g.table));
    }
}

#[test]
fn source_membership_rejects_missing_duplicated_foreign_and_crossing_occurrences() {
    let (_, xhtml, rules) = original();
    let pages = crate::vertical::analyze_pages_with_rules(&xhtml, &rules);
    let doc = scraper::Html::parse_document(&xhtml);
    let sel = scraper::Selector::parse("page").unwrap();
    let scene = crate::native_source_scene::for_page(
        doc.select(&sel).nth(1).unwrap(),
        rules[1].as_ref().unwrap(),
    )
    .unwrap();
    let g = grid(&columns(&pages[0].tables[0]).unwrap(), &scene).unwrap();
    let p = &pages[1];
    let lines = &p
        .regions
        .iter()
        .find(|r| r.role == Role::After)
        .unwrap()
        .lines;
    assert!(recover(lines, p, &g, &scene).is_some());
    let id = lines[0].words[0];
    for kind in 0..4 {
        let mut p = p.clone();
        match kind {
            0 => p.native_bounds[id] = NativeBounds::Unknown,
            1 => p.native_axes[id] = NativeAxis::Compatibility,
            2 => p.native_horizontal_baselines[id] = None,
            _ => {
                let NativeBounds::Glyphs(mut b) = p.native_bounds[id] else {
                    unreachable!()
                };
                b[0] = g.xs[2];
                p.native_bounds[id] = NativeBounds::Glyphs(b);
            }
        }
        assert!(recover(lines, &p, &g, &scene).is_none(), "kind {kind}");
    }
    let mut missing = lines.clone();
    missing[0].words.remove(0);
    assert!(recover(&missing, p, &g, &scene).is_none());
    let mut duplicate = lines.clone();
    duplicate[0].words.push(id);
    assert!(recover(&duplicate, p, &g, &scene).is_none());
    let foreign = &p
        .regions
        .iter()
        .find(|r| r.role == Role::Before)
        .unwrap()
        .lines;
    assert!(recover(foreign, p, &g, &scene).is_none());
    // Changing normalized prose consistently cannot affect cell ownership.
    let mut arbitrary = p.clone();
    for w in &mut arbitrary.words {
        w.text = w
            .text
            .chars()
            .map(|c| if c.is_whitespace() { c } else { '任' })
            .collect();
    }
    let mut arbitrary_lines = lines.clone();
    for l in &mut arbitrary_lines {
        l.text = l
            .words
            .iter()
            .map(|id| arbitrary.words[*id].text.as_str())
            .collect();
    }
    let t = recover(&arbitrary_lines, &arbitrary, &g, &scene).unwrap().1;
    let original = recover(lines, p, &g, &scene).unwrap().1;
    assert_eq!(
        t.cells.iter().map(|c| &c.words).collect::<Vec<_>>(),
        original.cells.iter().map(|c| &c.words).collect::<Vec<_>>()
    );
}
#[test]
fn coverage_never_bridges_a_physical_gap_or_moves_a_column() {
    assert!(coverage(vec![[1., 3.], [3., 5.]], 1., 5.));
    assert!(!coverage(vec![[1., 2.9], [3., 5.]], 1., 5.));
    assert!(!vertical(&[[10., 1., 10., 5.]], 10.1, 1., 5.));
    for scale in [0.5f32, 2., 100.] {
        for offset in [-100f32, 0., 300.] {
            let f = |x: f32| x * scale + offset;
            assert!(vertical(
                &[[f(10.), f(1.), f(10.), f(5.)]],
                f(10.),
                f(1.),
                f(5.)
            ));
            assert!(!vertical(
                &[
                    [f(10.), f(1.), f(10.), f(3.)],
                    [f(10.), f(3.1), f(10.), f(5.)]
                ],
                f(10.),
                f(1.),
                f(5.)
            ));
        }
    }
}

fn stroke_scene(lines: Vec<[f32; 4]>) -> Scene {
    let lines = lines
        .into_iter()
        .map(|l| {
            let b = [
                l[0].min(l[2]) - 0.1,
                l[1].min(l[3]) - 0.1,
                l[0].max(l[2]) + 0.1,
                l[1].max(l[3]) + 0.1,
            ];
            (l, b)
        })
        .collect::<Vec<_>>();
    let cert = serde_json::json!({"version":1,"glyphs":0,"ignored":[],"rectangle_ids":[],"words":[],"unknown":[],"rectangles":[],"rulings":[],"rectangle_obstacles":[],"images":[],"lines":lines,"fills":[]});
    let html = scraper::Html::parse_fragment(&format!(
        "<page data-native-glyph-count='0' data-native-scene='{cert}'></page>"
    ));
    crate::native_source_scene::for_page(
        html.select(&scraper::Selector::parse("page").unwrap())
            .next()
            .unwrap(),
        &PageRules::default(),
    )
    .unwrap()
}
#[test]
fn native_grid_proof_requires_all_columns_and_open_top_under_scaling_and_translation() {
    for (scale, offset) in [(1f32, 0f32), (0.5, 150.), (7., -100.)] {
        let f = |n: f32| n * scale + offset;
        let xs = [10., 20., 40., 80.].map(f);
        let mut ls = xs
            .iter()
            .map(|&x| [x, f(1.), x, f(51.)])
            .collect::<Vec<_>>();
        ls.push([f(40.), f(25.), f(80.), f(25.)]);
        let g = grid(&xs, &stroke_scene(ls.clone())).unwrap();
        assert_eq!(
            g.cells,
            vec![
                GridCell {
                    x0: 0,
                    x1: 1,
                    y0: 0,
                    y1: 2
                },
                GridCell {
                    x0: 1,
                    x1: 2,
                    y0: 0,
                    y1: 2
                },
                GridCell {
                    x0: 2,
                    x1: 3,
                    y0: 0,
                    y1: 1
                },
                GridCell {
                    x0: 2,
                    x1: 3,
                    y0: 1,
                    y1: 2
                }
            ]
        );
        let mut closed = ls.clone();
        closed.push([f(10.), f(1.), f(80.), f(1.)]);
        assert!(grid(&xs, &stroke_scene(closed)).is_none());
        let mut missing = ls.clone();
        missing.remove(1);
        assert!(grid(&xs, &stroke_scene(missing)).is_none());
        let mut moved = ls.clone();
        moved[1][0] += scale;
        moved[1][2] += scale;
        assert!(grid(&xs, &stroke_scene(moved)).is_none());
        let mut gap = ls;
        gap[1][3] = f(24.);
        gap.push([f(20.), f(26.), f(20.), f(51.)]);
        assert!(grid(&xs, &stroke_scene(gap)).is_none());
    }
}

#[test]
fn open_row_join_keeps_source_scalar_order_or_retains_separate_editable_fragments() {
    use crate::model::{NestedTable, Run};
    let table = |texts: [&str; 3]| NestedTable {
        rows: vec![texts.into_iter().map(|s| vec![Run::plain(s)]).collect()],
        merges: vec![],
    };
    let a = table(["甲", "乙", "丙"]);
    let mut joined = a.clone();
    assert!(!crate::model::join_open_table_row(
        &mut joined,
        &table(["丁", "", "戊"])
    ));
    assert_eq!(
        joined, a,
        "rejected join is atomic and cannot disturb source annotations"
    );
    assert!(crate::model::join_open_table_row(
        &mut joined,
        &table(["", "", "丁"])
    ));
    assert_eq!(
        joined.rows[0][2]
            .iter()
            .map(|r| r.text.as_str())
            .collect::<String>(),
        "丙丁"
    );
}
