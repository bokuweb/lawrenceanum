#[test]
fn a_native_printed_row_can_rejoin_fragments_from_different_legacy_lines() {
    for cols_count in [2, 3, 5] {
        for count in [2, 4] {
            let mut f = physical_records(cols_count, count, 0.);
            f.bounds = f
                .words
                .iter()
                .map(|w| NativeBounds::Glyphs([w.x0, w.y0, w.x1, w.y1]))
                .collect();
            f.axes = vec![NativeAxis::Right; f.words.len()];
            f.units = vec![Unit::Plain(
                f.words
                    .iter()
                    .map(|w| Line::text(w.text.clone(), vec![w.id]))
                    .collect(),
            )];
            run(&mut f);
            let lines = scopes(&f.units)[0].1.clone();
            let expected: Vec<Vec<usize>> = (0..count)
                .map(|r| (0..cols_count).map(|c| c * count + r).collect())
                .collect();
            assert_eq!(
                lines.iter().map(|l| l.words.clone()).collect::<Vec<_>>(),
                expected
            );
        }
    }
}
