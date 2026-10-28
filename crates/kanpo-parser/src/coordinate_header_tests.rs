fn headed_fixture(headers: usize, label: &str) -> (Vec<Unit>, Vec<Word>, PageRules) {
    let (mut u, mut w, mut r) = fixture(false);
    let shift = 18. * headers as f32;
    for word in &mut w {
        word.y0 += shift;
        word.y1 += shift;
        word.source_box[1] += shift;
        word.source_box[3] += shift;
    }
    for v in &mut r.vertical {
        v.y1 += shift;
    }
    for h in &mut r.horizontal {
        h.y += shift;
    }
    let mut leading = Vec::new();
    for row in 0..headers {
        let y = 5. + 18. * row as f32;
        let mut ids = Vec::new();
        for x in [6., 29., 97.] {
            let id = w.len();
            w.push(Word {
                id,
                x0: x,
                y0: y,
                x1: x + 8.,
                y1: y + 8.,
                source_box: [x, y, x + 8., y + 8.],
                raw: label.into(),
                text: label.into(),
                status: WordStatus::Used,
            });
            ids.push(id);
        }
        leading.push(Line::text(format!("{label}　{label}　{label}"), ids));
        r.horizontal.push(HRule {
            y: 18. * row as f32,
            x0: 0.,
            x1: 159.,
        });
    }
    if let Unit::Plain(lines) = &mut u[0] {
        leading.append(lines);
        *lines = leading;
    }
    (u, w, r)
}
#[test]
fn native_text_header_prefixes_do_not_depend_on_coordinate_heading_vocabulary_or_row_caps() {
    for headers in [1, 3] {
        for label in ["x", "任意", "Latitude"] {
            let (mut u, mut w, r) = headed_fixture(headers, label);
            let old: Vec<_> = if let Unit::Plain(ls) = &u[0] {
                ls[..headers].to_vec()
            } else {
                unreachable!()
            };
            restore_coordinate_rows(&mut u, &mut w, Some(&r), 200.);
            assert_eq!(
                w.iter()
                    .filter(|w| ["°", "′", "″"].contains(&w.text.as_str()))
                    .count(),
                12
            );
            if let Unit::Plain(ls) = &u[0] {
                assert_eq!(ls[..headers], old);
            }
        }
    }
}
#[test]
fn incomplete_or_ambiguous_header_ownership_rejects_the_complete_grid() {
    for mode in 0..4 {
        let (mut u, mut w, r) = headed_fixture(1, "label");
        match mode {
            0 => {
                if let Unit::Plain(ls) = &mut u[0] {
                    ls[0].words.pop();
                }
            }
            1 => u.push(Unit::Plain(vec![Line::text("label".into(), vec![34])])),
            2 => {
                w[34].status = WordStatus::Empty;
                w[34].text.clear();
            }
            _ => {
                w[34].x0 = 22.;
                w[34].x1 = 30.;
            }
        }
        let before = snap(&u, &w);
        restore_coordinate_rows(&mut u, &mut w, Some(&r), 200.);
        assert_eq!(snap(&u, &w), before);
    }
}
#[test]
fn a_damaged_first_numeric_record_cannot_be_reclassified_as_a_header() {
    let (mut u, mut w, r) = fixture(false);
    w[0].text = "label".into();
    let before = snap(&u, &w);
    restore_coordinate_rows(&mut u, &mut w, Some(&r), 200.);
    assert_eq!(snap(&u, &w), before);
}
#[test]
fn malformed_coordinate_body_after_a_header_is_never_partially_restored() {
    let (mut u, mut w, r) = headed_fixture(1, "label");
    w[17].text = "3".into();
    let before = snap(&u, &w);
    restore_coordinate_rows(&mut u, &mut w, Some(&r), 200.);
    assert_eq!(snap(&u, &w), before);
}
