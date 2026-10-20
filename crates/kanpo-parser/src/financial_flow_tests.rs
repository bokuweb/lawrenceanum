use super::*;

// Geometry controls deliberately use unnamed account categories and arbitrary
// values. Only source layout is relevant to admission; no financial caption is
// a recognition key.
fn fixture() -> (Vec<Unit>, Vec<Word>) {
    let mut words = Vec::new();
    let mut add = |text: &str, raw: &str, x0: f32, y0: f32, x1: f32, y1: f32, status: WordStatus| {
        let id = words.len();
        words.push(Word {
            id,
            x0,
            y0,
            x1,
            y1,
            raw: raw.into(),
            text: text.into(),
            status,
        });
    };
    add("別表", "別表", 188., 20., 204., 28., WordStatus::Used);
    let fields = [
        (32., 52.),
        (80., 100.),
        (140., 160.),
        (188., 220.),
        (260., 280.),
        (308., 340.),
        (380., 400.),
        (440., 460.),
        (500., 520.),
    ];
    for &(a, b) in &fields {
        add("見出し", "見出し", a, 50., b, 58., WordStatus::Used);
    }
    let master = [100., 160., 220., 280., 400., 500.];
    for (r, &y) in master.iter().enumerate() {
        add(
            "会計",
            "会計",
            if r == 0 || r == 5 { 20. } else { 32. },
            y,
            52.,
            y + 8.,
            WordStatus::Used,
        );
        for x in [100., 160., 280., 400., 460., 520.] {
            add("12,345", "12,345", x - 24., y, x, y + 8., WordStatus::Used);
        }
        let ys: Vec<f32> = match r {
            0 => vec![90., 98., 106.],
            1 => vec![152., 160., 168.],
            2 => vec![212., 220., 228.],
            3 => vec![272., 280., 288.],
            4 => (0..12).map(|i| 350. + i as f32 * 8.).collect(),
            _ => vec![500.],
        };
        for y in ys {
            for x in [188., 308.] {
                add("内訳", "内訳", x, y, x + 16., y + 8., WordStatus::Used);
                add("1", "1", x + 28., y, x + 32., y + 8., WordStatus::Used);
            }
        }
    }
    add("", "\u{f0000}", 24., 150., 32., 450., WordStatus::Empty);
    for (i, ch) in ["共", "通", "区", "分", "等"].iter().enumerate() {
        let y = 200. + i as f32 * 40.;
        add(ch, ch, 20., y, 28., y + 8., WordStatus::Used);
    }
    let lines = words
        .iter()
        .filter(|w| w.status == WordStatus::Used)
        .map(|w| Line::text(w.text.clone(), vec![w.id]))
        .collect();
    (vec![Unit::Plain(lines)], words)
}
fn unchanged(mut units: Vec<Unit>, mut words: Vec<Word>) {
    let before = units.iter().map(Unit::text).collect::<Vec<_>>();
    let old = words
        .iter()
        .map(|w| (w.id, w.text.clone(), w.status, w.x0, w.y0, w.x1, w.y1))
        .collect::<Vec<_>>();
    restore(&mut units, &mut words);
    assert_eq!(units.iter().map(Unit::text).collect::<Vec<_>>(), before);
    assert_eq!(
        words
            .iter()
            .map(|w| (w.id, w.text.clone(), w.status, w.x0, w.y0, w.x1, w.y1))
            .collect::<Vec<_>>(),
        old
    );
}
#[test]
fn unnamed_wide_matrix_keeps_all_old_ids_and_shared_cell_and_is_idempotent() {
    let (mut units, mut words) = fixture();
    let old: HashSet<usize> = words.iter().filter(|w| w.status == WordStatus::Used).map(|w| w.id).collect();
    let geometry = infer(&words, &old).expect("synthetic geometry");
    let _assembled = build(&words, &geometry).expect("synthetic detail assembly");
    restore(&mut units, &mut words);
    let Unit::Plain(lines) = &units[0] else { panic!() };
    let t = lines.iter().find_map(|l| l.table.as_ref()).expect("matrix admitted");
    assert_eq!((t.rows, t.cols), (7, 10));
    let c = t.cells.iter().find(|c| c.text == "共通区分等").unwrap();
    assert_eq!((c.row, c.col, c.rowspan), (2, 0, 4));
    let new: Vec<usize> = lines
        .iter()
        .flat_map(|l| {
            l.table
                .as_ref()
                .map(|t| t.cells.iter().flat_map(|c| c.words.iter().copied()).collect::<Vec<_>>())
                .unwrap_or_else(|| l.words.clone())
        })
        .collect();
    assert_eq!(new.len(), old.len());
    assert_eq!(new.into_iter().collect::<HashSet<_>>(), old);
    let previous = units.iter().map(Unit::text).collect::<Vec<_>>();
    restore(&mut units, &mut words);
    assert_eq!(units.iter().map(Unit::text).collect::<Vec<_>>(), previous);
}
#[test]
fn one_missing_main_amount_rejects_matrix() {
    let (u, mut w) = fixture();
    let id = w.iter().find(|w| w.x1 == 100. && w.y0 == 160.).unwrap().id;
    w[id].text.clear();
    unchanged(u, w);
}
#[test]
fn no_native_grouping_witness_rejects_matrix() {
    let (u, mut w) = fixture();
    w.iter_mut().find(|w| w.status == WordStatus::Empty).unwrap().raw.clear();
    unchanged(u, w);
}
#[test]
fn two_grouping_witnesses_reject_matrix() {
    let (u, mut w) = fixture();
    let mut z = w.iter().find(|w| w.status == WordStatus::Empty).unwrap().clone();
    z.id = w.len();
    w.push(z);
    unchanged(u, w);
}
#[test]
fn nonuniform_shared_category_rejects_matrix() {
    let (u, mut w) = fixture();
    w.iter_mut().find(|w| w.text == "区").unwrap().y0 += 5.;
    unchanged(u, w);
}
#[test]
fn missing_same_baseline_category_rejects_matrix() {
    let (u, mut w) = fixture();
    w.iter_mut().find(|w| w.text == "会計" && w.y0 == 100.).unwrap().text.clear();
    unchanged(u, w);
}
#[test]
fn missing_column_caption_rejects_matrix() {
    let (u, mut w) = fixture();
    w.iter_mut().find(|w| w.y0 == 50. && w.x1 == 100.).unwrap().text.clear();
    unchanged(u, w);
}
#[test]
fn independent_prose_bridging_full_width_blank_rejects_matrix() {
    let (mut u, mut w) = fixture();
    let id = w.len();
    w.push(Word {
        id,
        x0: 80.,
        y0: 112.,
        x1: 520.,
        y1: 155.,
        raw: "別本文".into(),
        text: "別本文".into(),
        status: WordStatus::Used,
    });
    let Unit::Plain(lines) = &mut u[0] else { panic!() };
    lines.push(Line::text("別本文".into(), vec![id]));
    unchanged(u, w);
}
#[test]
fn crossing_old_line_is_not_split_or_stolen() {
    let (mut u, w) = fixture();
    let Unit::Plain(lines) = &mut u[0] else { panic!() };
    let title = lines.remove(0);
    lines[0].text = format!("{}{}", title.text, lines[0].text);
    lines[0].words.extend(title.words);
    unchanged(u, w);
}
#[test]
fn a_preexisting_table_is_never_rebuilt() {
    let (mut u, w) = fixture();
    let Unit::Plain(lines) = &mut u[0] else { panic!() };
    lines[0].table = Some(Table {
        x0: 20.,
        y0: 20.,
        x1: 36.,
        y1: 28.,
        rows: 1,
        cols: 1,
        cells: vec![],
    });
    unchanged(u, w);
}
#[test]
fn multi_article_units_are_not_combined() {
    let (mut u, w) = fixture();
    u.push(Unit::Plain(vec![]));
    unchanged(u, w);
}
#[test]
fn duplicate_original_owner_rejects_matrix() {
    let (mut u, w) = fixture();
    let Unit::Plain(lines) = &mut u[0] else { panic!() };
    let duplicated = lines[1].clone();
    lines.push(duplicated);
    unchanged(u, w);
}

#[test]
fn unknown_private_glyph_is_not_restored_or_assigned_to_a_cell() {
    let (mut u, mut w) = fixture();
    let id = w.len();
    w.push(Word {
        id,
        x0: 216.,
        y0: 90.,
        x1: 220.,
        y1: 98.,
        raw: "\u{e600}".into(),
        text: String::new(),
        status: WordStatus::Empty,
    });
    restore(&mut u, &mut w);
    let Unit::Plain(lines) = &u[0] else { panic!() };
    let table = lines.iter().find_map(|l| l.table.as_ref()).unwrap();
    assert!(table.cells.iter().all(|c| !c.words.contains(&id)));
    assert!(w[id].text.is_empty());
    assert_eq!(w[id].status, WordStatus::Empty);
}
#[test]
fn raw_dash_overlapping_an_existing_value_rejects_the_entire_candidate() {
    let (u, mut w) = fixture();
    let id = w.len();
    w.push(Word {
        id,
        x0: 216.,
        y0: 90.,
        x1: 220.,
        y1: 98.,
        raw: "\u{e61c}".into(),
        text: String::new(),
        status: WordStatus::Empty,
    });
    unchanged(u, w);
}
#[test]
fn two_raw_dashes_in_one_amount_slot_reject_the_entire_candidate() {
    let (mut u, mut w) = fixture();
    let missing = w.iter().find(|w| w.text == "1" && w.x0 == 216. && w.y0 == 90.).unwrap().id;
    w[missing].text.clear();
    w[missing].raw.clear();
    w[missing].status = WordStatus::Empty;
    let Unit::Plain(lines) = &mut u[0] else { panic!() };
    lines.retain(|l| !l.words.contains(&missing));
    for _ in 0..2 {
        let id = w.len();
        w.push(Word {
            id,
            x0: 216.,
            y0: 90.,
            x1: 220.,
            y1: 98.,
            raw: "\u{e61c}".into(),
            text: String::new(),
            status: WordStatus::Empty,
        });
    }
    unchanged(u, w);
}
#[test]
fn independent_full_width_empty_detail_is_not_absorbed_as_a_wrap() {
    let (mut u, mut w) = fixture();
    let missing = w.iter().find(|w| w.text == "1" && w.x0 == 216. && w.y0 == 90.).unwrap().id;
    w[missing].text.clear();
    w[missing].raw.clear();
    w[missing].status = WordStatus::Empty;
    let Unit::Plain(lines) = &mut u[0] else { panic!() };
    lines.retain(|l| !l.words.contains(&missing));
    unchanged(u, w);
}
#[test]
fn malformed_numeric_group_rejects_the_matrix() {
    let (u, mut w) = fixture();
    w.iter_mut().find(|w| w.x1 == 100. && w.y0 == 160.).unwrap().text = "12,34".into();
    unchanged(u, w);
}

#[test]
fn numeric_parts_are_validated_in_native_x_order_not_source_id_order() {
    let (_, mut words) = fixture();
    let owners = words.iter().filter(|w| w.status == WordStatus::Used).map(|w| w.id).collect();
    let geometry = infer(&words, &owners).unwrap();
    let id = words.len();
    words.push(Word {
        id,
        x0: 10.,
        y0: 0.,
        x1: 14.,
        y1: 8.,
        raw: "3".into(),
        text: "3".into(),
        status: WordStatus::Used,
    });
    words.push(Word {
        id: id + 1,
        x0: 13.,
        y0: 0.,
        x1: 21.,
        y1: 8.,
        raw: ",".into(),
        text: ",".into(),
        status: WordStatus::Used,
    });
    words.push(Word {
        id: id + 2,
        x0: 18.,
        y0: 0.,
        x1: 30.,
        y1: 8.,
        raw: "090".into(),
        text: "090".into(),
        status: WordStatus::Used,
    });
    assert!(complete_amount(&[&words[id + 2], &words[id + 1], &words[id]], &geometry));
}
