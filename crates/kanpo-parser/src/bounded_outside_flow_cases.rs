fn bounded_outside_fixture(n: usize, label: &str) -> Fixture {
    let mut f = fixture(1, 2, "cell");
    let mut right = fixture(2, 2, "cell");
    let offset = f.words.len();
    for w in &mut right.words {
        w.id += offset;
        w.x0 += 500.;
        w.x1 += 500.;
        w.source_box[0] += 500.;
        w.source_box[2] += 500.;
    }
    for g in &mut right.grids {
        for x in &mut g.xs {
            *x += 500.;
        }
    }
    for v in &mut right.rules.vertical {
        v.x += 500.;
    }
    for h in &mut right.rules.horizontal {
        h.x0 += 500.;
        h.x1 += 500.;
    }
    f.words.extend(right.words);
    f.grids.extend(right.grids);
    f.rules.vertical.extend(right.rules.vertical);
    f.rules.horizontal.extend(right.rules.horizontal);
    let frame_end = f.words.len();
    let mut foreign = Vec::new();
    for j in 0..n {
        let id = f.words.len();
        let x = if n == 1 || j > 0 {
            80. + 10. * j.saturating_sub(1) as f32
        } else {
            55.
        };
        let y = 165.;
        f.words.push(Word {
            id,
            x0: x,
            y0: y,
            x1: x + 5.,
            y1: y + 8.,
            source_box: [x, y, x + 5., y + 8.],
            raw: label.into(),
            text: label.into(),
            status: WordStatus::Used,
        });
        foreign.push(id);
    }
    f.expected = (0..offset)
        .chain(foreign)
        .chain(offset..frame_end)
        .collect();
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
fn a_source_ruled_bounded_gap_retains_outside_prose_between_its_two_frame_witnesses() {
    for n in [1, 2, 8] {
        for label in ["x", "任意の長い本文断片", "123.45"] {
            let mut f = bounded_outside_fixture(n, label);
            run(&mut f);
            assert_eq!(ids(&f.units), f.expected);
        }
    }
}
#[test]
fn both_physical_ordering_witnesses_are_required_for_an_outside_gap() {
    for horizontal in [true, false] {
        let mut f = bounded_outside_fixture(2, "outside");
        if horizontal {
            f.rules.horizontal.retain(|h| !(h.y == 150. && h.x0 < 500.));
        } else {
            f.rules.vertical.retain(|v| v.x != 560.);
        }
        unchanged(f);
    }
}
#[test]
fn ambiguous_or_stale_outside_source_evidence_rejects_the_whole_component() {
    for mode in 0..4 {
        let mut f = bounded_outside_fixture(2, "outside");
        match mode {
            0 => f.axes[9] = NativeAxis::Unknown,
            1 => f.axes[9] = NativeAxis::Compatibility,
            2 => f
                .units
                .push(Unit::Plain(vec![Line::text("outside".into(), vec![9])])),
            _ => {
                if let Unit::Plain(ls) = &mut f.units[0] {
                    ls[2].text.push_str(" injected");
                }
            }
        }
        unchanged(f);
    }
}
#[test]
fn an_outside_citation_cannot_borrow_its_close_from_a_printed_frame() {
    let mut f = bounded_outside_fixture(2, "outside");
    f.words[9].text = "（begin".into();
    f.words[3].text = "end）".into();
    f.units = vec![Unit::Plain(crate::horizontal::row_lines(
        f.words.iter().map(col).collect(),
    ))];
    unchanged(f);
}

#[test]
fn a_diagonal_single_outside_marker_has_no_preceding_frame_witness() {
    let mut f = bounded_outside_fixture(1, "marker");
    let w = &mut f.words[9];
    w.x0 = 55.;
    w.x1 = 60.;
    w.source_box = [55., 165., 60., 173.];
    f.bounds[9] = NativeBounds::Glyphs(w.source_box);
    f.units = vec![Unit::Plain(crate::horizontal::row_lines(
        f.words.iter().map(col).collect(),
    ))];
    unchanged(f);
}
