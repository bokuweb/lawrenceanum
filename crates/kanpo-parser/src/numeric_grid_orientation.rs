use crate::{grid::Grid, vertical::Col};

/// A small ruled table can contain vertical category cells, horizontally spaced
/// account names, and half-width amounts whose two digits form a square bbox.
/// Only three independent physical label/amount rows establish this exception.
pub(crate) fn horizontal_numeric_rows(g: &Grid, words: &[Col]) -> bool {
    if !(3..=5).contains(&g.xs.len()) || !(5..=31).contains(&g.ys.len()) {
        return false;
    }
    let last = g.xs.len() - 2;
    let mut cell_words: Vec<Vec<&Col>> = vec![Vec::new(); g.cells.len()];
    for w in words {
        let Some(i) = g.cell_at((w.x + w.x1) / 2., (w.y + w.y1) / 2.) else {
            return false;
        };
        cell_words[i].push(w);
    }
    let japanese = |c: char| matches!(c, '\u{3040}'..='\u{30ff}' | '\u{3400}'..='\u{9fff}');
    let mut rows = std::collections::BTreeSet::new();
    for (i, c) in g.cells.iter().enumerate() {
        if c.x0 != last || c.x1 != last + 1 || c.y1 != c.y0 + 1 || cell_words[i].is_empty() {
            continue;
        }
        let nums = &cell_words[i];
        if !nums.iter().all(|w| {
            w.text
                .chars()
                .all(|ch| ch.is_ascii_digit() || ",.△▲−-()（）".contains(ch))
        }) || !nums
            .iter()
            .any(|w| w.text.chars().any(|ch| ch.is_ascii_digit()))
        {
            continue;
        }
        let Some(j) = g
            .cells
            .iter()
            .position(|l| l.x0 + 1 == l.x1 && l.x1 == c.x0 && l.y0 == c.y0 && l.y1 == c.y1)
        else {
            continue;
        };
        let label = &cell_words[j];
        if label.is_empty() {
            continue;
        }
        let chars: Vec<_> = label.iter().flat_map(|w| w.text.chars()).collect();
        if !(2..=30).contains(&chars.len())
            || chars.iter().filter(|ch| japanese(**ch)).count() < 2
            || chars.iter().any(|ch| "。；;！？!?".contains(*ch))
        {
            continue;
        }
        let h = label
            .iter()
            .map(|w| w.y1 - w.y)
            .fold(f32::INFINITY, f32::min);
        if !(3.0..=20.0).contains(&h) {
            continue;
        }
        let yc = (label[0].y + label[0].y1) / 2.;
        if !label.iter().chain(nums.iter()).all(|w| {
            let wh = w.y1 - w.y;
            ((w.y + w.y1) / 2. - yc).abs() <= 1.0f32.max(h * 0.2)
                && wh >= h * 0.75
                && wh <= h * 1.25
                && w.x1 - w.x >= wh * 0.75
        }) {
            continue;
        }
        let mut sorted = label.clone();
        sorted.sort_by(|a, b| a.x.total_cmp(&b.x));
        if sorted.windows(2).any(|p| p[0].x1 > p[1].x + 0.5)
            || sorted.last().unwrap().x1 - sorted[0].x < h * 1.5
        {
            continue;
        }
        rows.insert(c.y0);
    }
    rows.len() >= 3
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::GridCell;
    fn fixture() -> (Grid, Vec<Col>) {
        let g = Grid {
            xs: vec![0., 24., 96., 128.],
            ys: vec![0., 10., 20., 30., 40.],
            cells: (0..4)
                .flat_map(|y| {
                    (0..3).map(move |x| GridCell {
                        x0: x,
                        x1: x + 1,
                        y0: y,
                        y1: y + 1,
                    })
                })
                .collect(),
        };
        let mut ws = vec![];
        for r in 1..4 {
            for (x, text) in [
                (28., "流"),
                (44., "動"),
                (60., "資"),
                (76., "産"),
                (116., "43"),
            ] {
                ws.push(Col {
                    id: ws.len(),
                    x,
                    x1: x + 8.,
                    y: r as f32 * 10. + 1.,
                    y1: r as f32 * 10. + 9.184,
                    text: text.into(),
                });
            }
        }
        (g, ws)
    }
    #[test]
    fn same_baseline_label_and_amount_in_three_physical_rows() {
        let (g, w) = fixture();
        assert!(horizontal_numeric_rows(&g, &w));
    }
    #[test]
    fn unrelated_rows_are_not_horizontal_witnesses() {
        let (g, mut w) = fixture();
        for z in w.iter_mut().filter(|z| z.text == "43") {
            z.y += 3.;
            z.y1 += 3.;
        }
        assert!(!horizontal_numeric_rows(&g, &w));
    }
    #[test]
    fn vertical_japanese_labels_do_not_vote_horizontal() {
        let (g, mut w) = fixture();
        for z in w.iter_mut().filter(|z| z.text != "43") {
            z.x1 = z.x + 4.;
            z.y1 = z.y + 8.;
            z.text = "流動".into();
        }
        assert!(!horizontal_numeric_rows(&g, &w));
    }
    #[test]
    fn numeric_only_grid_has_no_label_witness() {
        let (g, mut w) = fixture();
        for z in &mut w {
            z.text = "43".into();
        }
        assert!(!horizontal_numeric_rows(&g, &w));
    }
    #[test]
    fn two_rows_insufficient() {
        let (g, mut w) = fixture();
        w.retain(|z| z.y < 30.);
        assert!(!horizontal_numeric_rows(&g, &w));
    }
    #[test]
    fn paragraph_label_is_not_a_short_row() {
        let (g, mut w) = fixture();
        for z in w.iter_mut().filter(|z| z.text == "流") {
            z.text = "本文。".into();
        }
        assert!(!horizontal_numeric_rows(&g, &w));
    }
    #[test]
    fn merged_amount_column_is_not_a_physical_row() {
        let (mut g, w) = fixture();
        g.cells.retain(|c| c.x0 != 2);
        g.cells.push(GridCell {
            x0: 2,
            x1: 3,
            y0: 1,
            y1: 4,
        });
        assert!(!horizontal_numeric_rows(&g, &w));
    }
    #[test]
    fn second_baseline_inside_label_cell_is_ambiguous() {
        let (g, mut w) = fixture();
        let mut z = w[0].clone();
        z.id = 100;
        z.y += 4.;
        z.y1 += 4.;
        for r in 0..3 {
            let mut q = z.clone();
            q.y += 10. * r as f32;
            q.y1 += 10. * r as f32;
            w.push(q);
        }
        assert!(!horizontal_numeric_rows(&g, &w));
    }
    #[test]
    fn vertical_category_cell_does_not_replace_horizontal_row_evidence() {
        let (mut g, mut w) = fixture();
        g.cells.retain(|c| c.x0 != 0 || c.y0 == 0);
        g.cells.push(GridCell {
            x0: 0,
            x1: 1,
            y0: 1,
            y1: 4,
        });
        w.push(Col {
            id: 100,
            x: 4.,
            x1: 12.,
            y: 11.,
            y1: 35.,
            text: "資産の部".into(),
        });
        assert!(horizontal_numeric_rows(&g, &w));
    }
    #[test]
    fn vertical_amounts_are_not_horizontal_row_evidence() {
        let (g, mut w) = fixture();
        for z in w.iter_mut().filter(|z| z.text == "43") {
            z.y1 = z.y + 16.;
        }
        assert!(!horizontal_numeric_rows(&g, &w));
    }
    #[test]
    fn missing_physical_label_rows_do_not_qualify() {
        let (mut g, w) = fixture();
        g.cells.retain(|c| c.x0 != 1);
        assert!(!horizontal_numeric_rows(&g, &w));
    }
    #[test]
    fn mixed_label_and_amount_cell_is_not_two_physical_columns() {
        let (mut g, w) = fixture();
        g.cells.retain(|c| c.x0 == 0);
        for r in 0..4 {
            g.cells.push(GridCell {
                x0: 1,
                x1: 3,
                y0: r,
                y1: r + 1,
            });
        }
        assert!(!horizontal_numeric_rows(&g, &w));
    }
    #[test]
    fn fractional_or_prose_amount_is_not_a_numeric_witness() {
        let (g, mut w) = fixture();
        for z in w.iter_mut().filter(|z| z.text == "43") {
            z.text = "4/3".into();
        }
        assert!(!horizontal_numeric_rows(&g, &w));
    }
}
