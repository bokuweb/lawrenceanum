fn physical_records(cols_count: usize, count: usize, dx: f32) -> Fixture {
    let mut f = records(cols_count, count, "arbitrary");
    let ys: Vec<_> = (0..=count).map(|r| 30. + 20. * r as f32).collect();
    for (id, w) in f.words.iter_mut().enumerate() {
        w.y0 = 35. + 20. * (id % count) as f32;
        w.y1 = w.y0 + 8.;
        w.x0 += dx;
        w.x1 += dx;
        w.source_box = [w.x0, w.y0, w.x1, w.y1];
    }
    let g = &mut f.grids[0];
    for x in &mut g.xs {
        *x += dx;
    }
    g.ys = ys.clone();
    g.cells = (0..count)
        .flat_map(|r| {
            (0..cols_count).map(move |c| crate::grid::GridCell {
                x0: c,
                x1: c + 1,
                y0: r,
                y1: r + 1,
            })
        })
        .collect();
    f.rules.vertical =
        g.xs.iter()
            .map(|&x| crate::lines::VRule {
                x,
                y0: ys[0],
                y1: *ys.last().unwrap(),
            })
            .collect();
    f.rules.horizontal = ys
        .iter()
        .map(|&y| crate::lines::HRule {
            y,
            x0: g.left(),
            x1: g.right(),
        })
        .collect();
    f
}
#[test]
fn a_printed_record_keeps_its_fields_on_one_line_after_parallel_frames_are_separated() {
    for cols_count in [2, 3, 5] {
        for count in [2, 4] {
            let mut f = physical_records(cols_count, count, 0.);
            let mut other = physical_records(cols_count, count, 140. * cols_count as f32 + 100.);
            let offset = f.words.len();
            for w in &mut other.words {
                w.id += offset;
            }
            f.words.extend(other.words);
            f.grids.extend(other.grids);
            f.rules.vertical.extend(other.rules.vertical);
            f.rules.horizontal.extend(other.rules.horizontal);
            f.bounds = f
                .words
                .iter()
                .map(|w| NativeBounds::Glyphs([w.x0, w.y0, w.x1, w.y1]))
                .collect();
            f.axes = vec![NativeAxis::Right; f.words.len()];
            f.units = vec![Unit::Plain(crate::horizontal::row_lines(
                f.words.iter().map(col).collect(),
            ))];
            run(&mut f);
            let lines = scopes(&f.units)[0].1.clone();
            let expected: Vec<Vec<usize>> = (0..2)
                .flat_map(|frame| {
                    (0..count).map(move |r| {
                        (0..cols_count)
                            .map(|c| frame * offset + c * count + r)
                            .collect()
                    })
                })
                .collect();
            assert_eq!(
                lines.iter().map(|l| l.words.clone()).collect::<Vec<_>>(),
                expected
            );
        }
    }
}
