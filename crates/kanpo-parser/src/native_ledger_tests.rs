use super::*;
fn seed() -> (Vec<trace::Unit>, Vec<trace::Word>, crate::lines::PageRules) {
    let mut words = Vec::new();
    let mut cells = Vec::new();
    for (r, label) in ["流動資産", "固定資産", "資産合計"].iter().enumerate() {
        let id = words.len();
        words.push(trace::Word {
            source_box: [4., 5. + r as f32 * 20., 36., 13. + r as f32 * 20.],
            id,
            x0: 4.,
            y0: 5. + r as f32 * 20.,
            x1: 36.,
            y1: 13. + r as f32 * 20.,
            raw: (*label).into(),
            text: (*label).into(),
            status: crate::trace::WordStatus::Used,
        });
    }
    cells.push(trace::TableCell {
        row: 0,
        col: 0,
        rowspan: 3,
        colspan: 1,
        x0: 0.,
        y0: 0.,
        x1: 40.,
        y1: 60.,
        text: "流動資産固定資産資産合計".into(),
        words: vec![0, 1, 2],
    });
    for r in 0..3 {
        for c in 1..3 {
            let id = words.len();
            let value = format!("{}", 100 + r * 100 + c);
            words.push(trace::Word {
                source_box: [c as f32 * 40. + 4., r as f32 * 20. + 5., c as f32 * 40. + 20., r as f32 * 20. + 13.],
                id,
                x0: c as f32 * 40. + 4.,
                y0: r as f32 * 20. + 5.,
                x1: c as f32 * 40. + 20.,
                y1: r as f32 * 20. + 13.,
                raw: value.clone(),
                text: value.clone(),
                status: crate::trace::WordStatus::Used,
            });
            cells.push(trace::TableCell {
                row: r,
                col: c,
                rowspan: 1,
                colspan: 1,
                x0: c as f32 * 40.,
                y0: r as f32 * 20.,
                x1: (c + 1) as f32 * 40.,
                y1: (r + 1) as f32 * 20.,
                text: value,
                words: vec![id],
            });
        }
    }
    let t = trace::Table {
        x0: 0.,
        y0: 0.,
        x1: 120.,
        y1: 60.,
        rows: 3,
        cols: 3,
        cells,
    };
    let u = vec![trace::Unit::Plain(vec![trace::Line {
        text: t.render(),
        words: vec![],
        table: Some(t),
    }])];
    let rules = crate::lines::PageRules {
        enclosures: Vec::new(), short_fraction_bars: Vec::new(),
        vertical: [0., 40., 80., 120.]
            .into_iter()
            .map(|x| crate::lines::VRule { x, y0: 0., y1: 60. })
            .collect(),
        horizontal: [(0., 0.), (20., 40.), (40., 40.), (60., 0.)]
            .into_iter()
            .map(|(y, x0)| crate::lines::HRule { y, x0, x1: 120. })
            .collect(),
    };
    (u, words, rules)
}
fn t(u: &mut [trace::Unit]) -> &mut trace::Table {
    let trace::Unit::Plain(l) = &mut u[0] else { unreachable!() };
    l[0].table.as_mut().unwrap()
}
fn run(u: &mut [trace::Unit], w: &[trace::Word], r: &crate::lines::PageRules) {
    repair_native_ledger_tables(u, w, r);
}
#[test]
fn separates_three_ledgers_and_keeps_word_ids() {
    let (mut u, w, r) = seed();
    run(&mut u, &w, &r);
    assert_eq!(u[0].text(), "流動資産　101　102\n固定資産　201　202\n資産合計　301　302");
    let mut ids: Vec<_> = t(&mut u).cells.iter().flat_map(|c| c.words.clone()).collect();
    ids.sort();
    assert_eq!(ids, (0..9).collect::<Vec<_>>());
    let before = u[0].text();
    run(&mut u, &w, &r);
    assert_eq!(u[0].text(), before);
}
#[test]
fn a_label_without_any_amount_remains_its_own_row() {
    let (mut u, w, r) = seed();
    t(&mut u).cells.retain(|c| c.row != 1 || c.col == 0);
    run(&mut u, &w, &r);
    assert_eq!(u[0].text(), "流動資産　101　102\n固定資産\n資産合計　301　302");
}
#[test]
fn ordinary_multiline_independent_text_is_not_a_ledger() {
    let (mut u, mut w, r) = seed();
    for (id, text) in [(0, "東京都港区"), (1, "芝公園二丁目"), (2, "株式会社甲")] {
        w[id].text = text.into();
        w[id].raw = text.into();
    }
    let before = u[0].text();
    run(&mut u, &w, &r);
    assert_eq!(u[0].text(), before);
}
#[test]
fn vertical_label_words_do_not_trigger_native_reconstruction() {
    let (mut u, mut w, r) = seed();
    for w in &mut w[..3] {
        w.x1 = w.x0 + 8.;
        w.y1 = w.y0 + 32.;
    }
    let before = u[0].text();
    run(&mut u, &w, &r);
    assert_eq!(u[0].text(), before);
}
#[test]
fn a_prose_value_column_is_not_numeric() {
    let (mut u, w, r) = seed();
    t(&mut u).cells[1].text = "東京都".into();
    let before = u[0].text();
    run(&mut u, &w, &r);
    assert_eq!(u[0].text(), before);
}
#[test]
fn no_physical_divider_does_not_merge_label_with_numeric_column() {
    let (mut u, w, mut r) = seed();
    r.vertical.retain(|v| v.x != 40.);
    let before = u[0].text();
    run(&mut u, &w, &r);
    assert_eq!(u[0].text(), before);
}
