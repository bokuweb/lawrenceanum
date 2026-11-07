use super::*;

fn sample(scale: f32, dx: f32, dy: f32) -> (Grid, Vec<Col>) {
    let cells = vec![
        GridCell {
            x0: 0,
            x1: 2,
            y0: 0,
            y1: 1,
        },
        GridCell {
            x0: 0,
            x1: 1,
            y0: 1,
            y1: 2,
        },
        GridCell {
            x0: 1,
            x1: 2,
            y0: 1,
            y1: 2,
        },
        GridCell {
            x0: 2,
            x1: 3,
            y0: 0,
            y1: 2,
        },
        GridCell {
            x0: 3,
            x1: 4,
            y0: 0,
            y1: 2,
        },
        GridCell {
            x0: 0,
            x1: 1,
            y0: 2,
            y1: 3,
        },
        GridCell {
            x0: 1,
            x1: 2,
            y0: 2,
            y1: 3,
        },
        GridCell {
            x0: 2,
            x1: 3,
            y0: 2,
            y1: 3,
        },
        GridCell {
            x0: 3,
            x1: 4,
            y0: 2,
            y1: 3,
        },
    ];
    let grid = Grid {
        xs: vec![0., 40., 120., 200., 400.]
            .into_iter()
            .map(|x| dx + x * scale)
            .collect(),
        ys: vec![0., 20., 80., 140.]
            .into_iter()
            .map(|y| dy + y * scale)
            .collect(),
        cells,
    };
    let placements = [
        (4., 5., "group"),
        (3., 45., "type"),
        (43., 45., "shape"),
        (123., 5., "A1"),
        (123., 28., "continuation"),
        (123., 51., "end"),
        (240., 42., "conditions"),
        (3., 91., "entry"),
        (43., 91., "code"),
        (123., 91., "equipment"),
        (240., 91., "content"),
    ];
    let words = placements
        .into_iter()
        .enumerate()
        .map(|(id, (x, y, text))| Col {
            id,
            x: dx + x * scale,
            y: dy + y * scale,
            x1: dx + (x + text.len() as f32 * 5.) * scale,
            y1: dy + (y + 8.) * scale,
            text: text.into(),
        })
        .collect();
    (grid, words)
}

#[test]
fn original_partial_rules_preserve_mixed_header_spans_and_wrapped_text() {
    for (scale, dx, dy) in [(1., 0., 0.), (0.7, 31., 17.), (1.8, 101., 85.)] {
        let (grid, words) = sample(scale, dx, dy);
        assert_eq!(ruled_header_rows(&grid), 2);
        // The decision must also work in comparative fields, where the legacy
        // financial header heuristic is deliberately disabled by item_cols.
        for item_cols in [0, usize::MAX] {
            let table = grid_table(&grid, &words, item_cols).unwrap();
            assert_eq!((table.rows, table.cols), (3, 4));
            let cell = table
                .cells
                .iter()
                .find(|c| c.row == 0 && c.col == 2)
                .unwrap();
            assert_eq!(cell.rowspan, 2);
            assert_eq!(cell.text, "A1continuationend");
            let conditions = table
                .cells
                .iter()
                .find(|c| c.row == 0 && c.col == 3)
                .unwrap();
            assert_eq!(
                (conditions.rowspan, conditions.text.as_str()),
                (2, "conditions")
            );
            assert!(!table.cells.iter().any(|c| c.row == 1 && c.col >= 2));
            let mut ids = table
                .cells
                .iter()
                .flat_map(|c| c.words.iter().copied())
                .collect::<Vec<_>>();
            ids.sort();
            assert_eq!(ids, (0..words.len()).collect::<Vec<_>>());
        }
    }
}

#[test]
fn ambiguous_or_body_spanning_components_are_not_certified_as_headers() {
    let (mut grid, _) = sample(1., 0., 0.);
    grid.cells.push(grid.cells[0]);
    assert_eq!(ruled_header_rows(&grid), 0);
    let (mut grid, _) = sample(1., 0., 0.);
    grid.cells[3].y1 = 3;
    assert_eq!(ruled_header_rows(&grid), 0);
    let (mut grid, _) = sample(1., 0., 0.);
    grid.cells[0].x1 = 1;
    assert_eq!(ruled_header_rows(&grid), 0);
}
