fn records(cols_count: usize, count: usize, label: &str) -> Fixture {
    let mut f = fixture(1, cols_count, label);
    f.words.clear();
    for c in 0..cols_count {
        for r in 0..count {
            let id = f.words.len();
            let x = f.grids[0].xs[c] + 20.;
            let y = 45. + 15. * r as f32;
            f.words.push(Word {
                id,
                x0: x,
                y0: y,
                x1: x + 30.,
                y1: y + 8.,
                source_box: [x, y, x + 30., y + 8.],
                raw: label.into(),
                text: label.into(),
                status: WordStatus::Used,
            });
        }
    }
    f.bounds = f
        .words
        .iter()
        .map(|w| NativeBounds::Glyphs([w.x0, w.y0, w.x1, w.y1]))
        .collect();
    f.axes = vec![NativeAxis::Right; f.words.len()];
    f.units = vec![Unit::Plain(crate::horizontal::row_lines(
        f.words.iter().map(col).collect(),
    ))];
    f
}
#[test]
fn repeated_aligned_rows_preserve_records_for_arbitrary_vocabulary_and_counts() {
    for c in [2, 3, 5] {
        for n in [2, 3, 6] {
            for label in ["alpha", "科目", "123.45"] {
                unchanged(records(c, n, label));
            }
        }
    }
}
#[test]
fn blank_entries_do_not_destroy_the_remaining_repeated_row_correspondence() {
    let mut f = records(2, 4, "entry");
    let id = 5;
    f.words[id].status = WordStatus::Empty;
    f.words[id].text.clear();
    f.units = vec![Unit::Plain(crate::horizontal::row_lines(
        f.words
            .iter()
            .filter(|w| w.status == WordStatus::Used)
            .map(col)
            .collect(),
    ))];
    unchanged(f);
}
#[test]
fn row_correspondence_is_transitive_across_adjacent_columns() {
    let mut f = records(3, 3, "entry");
    for id in [2, 6] {
        f.words[id].status = WordStatus::Empty;
        f.words[id].text.clear();
    }
    f.units = vec![Unit::Plain(crate::horizontal::row_lines(
        f.words
            .iter()
            .filter(|w| w.status == WordStatus::Used)
            .map(col)
            .collect(),
    ))];
    unchanged(f);
}
#[test]
fn row_records_and_independent_cell_flow_coexist_in_distinct_printed_frames() {
    let mut f = records(2, 2, "entry");
    let mut other = fixture(1, 2, "entry");
    let offset = f.words.len();
    for w in &mut other.words {
        w.id += offset;
        w.x0 += 400.;
        w.x1 += 400.;
        w.source_box[0] += 400.;
        w.source_box[2] += 400.;
    }
    for g in &mut other.grids {
        for x in &mut g.xs {
            *x += 400.;
        }
    }
    for v in &mut other.rules.vertical {
        v.x += 400.;
    }
    for h in &mut other.rules.horizontal {
        h.x0 += 400.;
        h.x1 += 400.;
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
    assert_eq!(ids(&f.units), vec![0, 2, 1, 3, 4, 5, 6]);
}
#[test]
fn original_lines_unaffected_by_the_word_permutation_are_not_split() {
    let mut f = fixture(2, 2, "data");
    let mut chosen = vec![
        f.words[0].clone(),
        f.words[2].clone(),
        f.words[3].clone(),
        f.words[4].clone(),
        f.words[5].clone(),
    ];
    for (id, w) in chosen.iter_mut().enumerate() {
        w.id = id;
    }
    f.words = chosen;
    f.bounds = f
        .words
        .iter()
        .map(|w| NativeBounds::Glyphs([w.x0, w.y0, w.x1, w.y1]))
        .collect();
    f.axes = vec![NativeAxis::Right; f.words.len()];
    f.units = vec![Unit::Plain(crate::horizontal::row_lines(
        f.words.iter().map(col).collect(),
    ))];
    let header = scopes(&f.units)[0].1[0].clone();
    run(&mut f);
    assert_eq!(scopes(&f.units)[0].1[0], header);
    assert_eq!(ids(&f.units), vec![0, 1, 2, 3, 4]);
}
