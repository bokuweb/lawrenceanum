use super::*;
use crate::native_terminal_glyph::{for_words, Terminals};
fn jiggle156(s: &mut Fixture, line: usize) {
    let Unit::Plain(ls) = &s.0[0] else {
        unreachable!()
    };
    let id = *ls[line].words.last().unwrap();
    s.1[id].x0 += 0.25;
    s.1[id].x1 += 0.25;
    let w = &mut s.1[id];
    w.source_box = [w.x0, w.y0, w.x1, w.y1];
}
fn source156() -> Fixture {
    let mut s = source152(&[
        ("A", &[Some("11")]),
        ("B", &[Some("22")]),
        ("C", &[Some("33")]),
    ]);
    jiggle156(&mut s, 1);
    s
}
fn metadata156(
    s: &Fixture,
    omit: &[usize],
    overrides: &[(usize, (usize, usize))],
    width: f64,
) -> Terminals {
    let mut xml = format!("<page data-native-glyph-count='{}'>", (s.1.len() + 1) * 16);
    for w in &s.1 {
        let [first, after] = overrides
            .iter()
            .find(|(id, _)| *id == w.id)
            .map(|(_, r)| [r.0, r.1])
            .unwrap_or([w.id * 16, w.id * 16 + 2]);
        let [x0, y0, x1, y1] = w.source_box;
        let raw = w
            .raw
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        let flow=s.3.get(w.id).copied().flatten().map(|y|{let start=w.x0 as f64;let end=w.x1 as f64;let leading=(end-start)*0.5;let tail=if omit.contains(&w.id){String::new()}else{format!(" data-native-terminal-glyph='1 {first} {after} {} {end} {y}'",end-width)};format!(" data-wmode='0' data-native-axis='right' data-native-flow-start='{start}' data-native-flow-baseline='{y}' data-native-flow-end='{end}' data-native-flow-leading='{leading}' data-native-flow-trailing='0' data-native-flow-prefix='0' data-native-flow-first-glyph='{first}' data-native-flow-after-glyph='{after}'{tail}")}).unwrap_or_default();
        xml.push_str(&format!(
            "<word xmin='{x0}' ymin='{y0}' xmax='{x1}' ymax='{y1}'{flow}>{raw}</word>"
        ));
    }
    xml.push_str("</page>");
    let h = scraper::Html::parse_fragment(&xml);
    for_words(
        h.select(&scraper::Selector::parse("page").unwrap())
            .next()
            .unwrap(),
        &s.1,
    )
}
fn tails156(s: &Fixture) -> Terminals {
    metadata156(s, &[], &[], 20.)
}
fn run156(s: &mut Fixture, t: &Terminals) {
    restore(&mut s.0, &s.1, &s.2, &s.3);
    restore_matrix_with_terminal(&mut s.0, &s.1, &s.2, &s.3, || t.clone());
}
fn yes156(mut s: Fixture, t: Terminals, rows: usize) {
    let words = serde_json::to_value(&s.1).unwrap();
    let ids =
        s.0.iter()
            .flat_map(|u| match u {
                Unit::Plain(ls) => ls.iter().flat_map(|l| l.words.clone()).collect::<Vec<_>>(),
                _ => vec![],
            })
            .collect::<Vec<_>>();
    run156(&mut s, &t);
    let ts = tables152(&s);
    assert_eq!(ts.len(), 1);
    assert_eq!((ts[0].rows, ts[0].cols), (rows, 2));
    assert_eq!(
        ts[0]
            .cells
            .iter()
            .flat_map(|c| c.words.iter().copied())
            .collect::<Vec<_>>(),
        ids
    );
    assert_eq!(serde_json::to_value(&s.1).unwrap(), words);
}
fn no156(mut s: Fixture, t: Terminals) {
    let before = units152(&s.0);
    let words = serde_json::to_value(&s.1).unwrap();
    run156(&mut s, &t);
    assert_eq!(units152(&s.0), before);
    assert_eq!(serde_json::to_value(&s.1).unwrap(), words);
}
fn coincident156() -> Fixture {
    let mut s = source156();
    let id = s.1.len();
    let mut w = s.1[0].clone();
    w.id = id;
    w.text = "Suffix".into();
    w.raw = "Suffix".into();
    s.1[0].raw = "Prefix".into();
    s.1[0].text = "Prefix".into();
    s.1.push(w);
    s.3.push(s.3[0]);
    if let Unit::Plain(ls) = &mut s.0[0] {
        ls[0].words.insert(1, id);
        ls[0].text = "PrefixSuffix\u{3000}11".into();
    }
    s
}
#[test]
fn terminal156_closed_parent_uses_the_common_original_last_glyph_band() {
    let s = source156();
    let t = tails156(&s);
    yes156(s, t, 3);
}
#[test]
fn terminal156_source_headings_keep_empty_amount_cells_inside_the_same_closed_parent() {
    let mut s = source152(&[
        ("Heading", &[None]),
        ("A", &[Some("11")]),
        ("B", &[Some("22")]),
    ]);
    jiggle156(&mut s, 1);
    let t = tails156(&s);
    yes156(s, t, 3);
}
#[test]
fn terminal156_overlapping_full_amount_boxes_cannot_substitute_for_disjoint_terminal_glyphs() {
    let s = source156();
    let t = metadata156(&s, &[], &[], 0.1);
    no156(s, t);
}
#[test]
fn terminal156_an_unknown_original_tail_cannot_be_borrowed_from_another_row() {
    let s = source156();
    let t = metadata156(&s, &[3], &[], 20.);
    no156(s, t);
}
#[test]
fn terminal156_one_unsupported_numeric_row_blocks_the_whole_new_band() {
    let mut s = source152(&[
        ("A", &[Some("11"), None]),
        ("B", &[Some("22"), Some("33")]),
        ("C", &[Some("44"), None]),
    ]);
    jiggle156(&mut s, 2);
    let t = tails156(&s);
    no156(s, t);
}
#[test]
fn terminal156_an_open_source_frame_supplies_no_complete_parent() {
    for index in [0, 1] {
        let mut s = source156();
        s.2.horizontal.remove(index);
        let t = tails156(&s);
        no156(s, t);
    }
}
#[test]
fn terminal156_an_internal_full_source_crosscut_is_not_a_guessed_parent_join() {
    let mut s = source156();
    s.2.horizontal.push(HRule {
        y: 20.,
        x0: 0.,
        x1: 150.,
    });
    let t = tails156(&s);
    no156(s, t);
}
#[test]
fn terminal156_an_unowned_used_source_word_blocks_the_whole_parent() {
    let mut s = source156();
    let id = s.1.len();
    let mut w = s.1[4].clone();
    w.id = id;
    w.x0 = 60.;
    w.x1 = 80.;
    w.source_box = [w.x0, w.y0, w.x1, w.y1];
    w.raw = "Orphan".into();
    w.text = "Orphan".into();
    s.1.push(w);
    s.3.push(s.3[4]);
    let t = tails156(&s);
    no156(s, t);
}
#[test]
fn terminal156_duplicate_source_word_ownership_is_not_repaired_by_a_table() {
    let mut s = source156();
    if let Unit::Plain(ls) = &mut s.0[0] {
        ls[2].words.push(0);
    }
    let t = tails156(&s);
    no156(s, t);
}
#[test]
fn terminal156_separate_existing_units_stay_separate() {
    let mut s = source156();
    if let Unit::Plain(ls) = &mut s.0[0] {
        let tail = ls.split_off(2);
        s.0.push(Unit::Plain(tail));
    }
    let t = tails156(&s);
    no156(s, t);
}
#[test]
fn terminal156_comparison_role_authority_is_not_plain_numeric_scope() {
    let mut s = source156();
    let Unit::Plain(ls) = s.0.remove(0) else {
        unreachable!()
    };
    s.0.push(Unit::Shinkyu {
        labels: None,
        preamble: ls,
        after: vec![],
        before: vec![],
        outside: vec![],
    });
    let t = tails156(&s);
    no156(s, t);
}
#[test]
fn terminal156_existing_typed_table_authority_stays_exact() {
    let mut s = source152(&[
        ("A", &[Some("11")]),
        ("B", &[Some("22")]),
        ("C", &[Some("33")]),
    ]);
    restore(&mut s.0, &s.1, &s.2, &s.3);
    assert_eq!(tables152(&s).len(), 1);
    let t = tails156(&s);
    no156(s, t);
}
#[test]
fn terminal156_existing_distinct_repeated_amount_columns_are_not_collapsed() {
    let mut s = source152(&[
        ("A", &[Some("11"), None]),
        ("B", &[Some("22"), None]),
        ("C", &[None, Some("33")]),
        ("D", &[None, Some("44")]),
    ]);
    let t = tails156(&s);
    run156(&mut s, &t);
    let ts = tables152(&s);
    assert_eq!(ts.len(), 1);
    assert_eq!((ts[0].rows, ts[0].cols), (4, 3));
}
#[test]
fn terminal156_coincident_label_advances_need_their_own_ordered_source_ranges() {
    let s = coincident156();
    let t = tails156(&s);
    yes156(s, t, 3);
}
#[test]
fn terminal156_reversed_original_label_ranges_are_not_reordered_to_fit() {
    let s = coincident156();
    let id = s.1.len() - 1;
    let t = metadata156(&s, &[], &[(0, (id * 16, id * 16 + 2)), (id, (0, 2))], 20.);
    no156(s, t);
}
#[test]
fn terminal156_coincident_label_words_cannot_borrow_missing_source_evidence() {
    let s = coincident156();
    let id = s.1.len() - 1;
    let t = metadata156(&s, &[id], &[], 20.);
    no156(s, t);
}
#[test]
fn terminal156_shared_label_origins_do_not_authorize_different_complete_advances() {
    let mut s = coincident156();
    let w = s.1.last_mut().unwrap();
    w.x1 += 1.;
    w.source_box[2] = w.x1;
    let t = tails156(&s);
    no156(s, t);
}
#[test]
fn terminal156_identical_numeric_origins_still_decline_even_with_ordered_ranges() {
    let mut s = source156();
    let id = s.1.len();
    let mut w = s.1[1].clone();
    w.id = id;
    w.text = "1".into();
    w.raw = "1".into();
    s.1[1].text = "1".into();
    s.1[1].raw = "1".into();
    s.1.push(w);
    s.3.push(s.3[1]);
    if let Unit::Plain(ls) = &mut s.0[0] {
        ls[0].words.push(id);
    }
    let t = tails156(&s);
    no156(s, t);
}
#[test]
fn terminal156_overlapping_source_glyph_owners_cannot_supply_a_label_tail() {
    let s = coincident156();
    let id = s.1.len() - 1;
    let t = metadata156(&s, &[], &[(id, (16, 18))], 20.);
    no156(s, t);
}
#[test]
fn terminal156_affine_coordinates_and_original_rule_order_keep_the_same_band() {
    for scale in [0.25, 4.] {
        let mut s = source156();
        s.2.horizontal.reverse();
        s.2.vertical.reverse();
        for w in &mut s.1 {
            w.x0 = w.x0 * scale + 17.;
            w.x1 = w.x1 * scale + 17.;
            w.y0 = w.y0 * scale + 31.;
            w.y1 = w.y1 * scale + 31.;
            w.source_box = [w.x0, w.y0, w.x1, w.y1];
        }
        for y in &mut s.3 {
            *y = y.map(|y| y * scale as f64 + 31.);
        }
        for h in &mut s.2.horizontal {
            h.x0 = h.x0 * scale + 17.;
            h.x1 = h.x1 * scale + 17.;
            h.y = h.y * scale + 31.;
        }
        for v in &mut s.2.vertical {
            v.x = v.x * scale + 17.;
            v.y0 = v.y0 * scale + 31.;
            v.y1 = v.y1 * scale + 31.;
        }
        let t = metadata156(&s, &[], &[], 20. * scale as f64);
        yes156(s, t, 3);
    }
}
#[test]
fn terminal156_a_heading_does_not_replace_a_second_numeric_row_witness() {
    let s = source152(&[("Heading", &[None]), ("A", &[Some("11")])]);
    let t = tails156(&s);
    no156(s, t);
}
#[test]
fn terminal156_a_tail_cannot_override_its_original_word_baseline_vector() {
    let mut s = source156();
    let t = tails156(&s);
    s.3[3] = s.3[3].map(|y| y + 1.);
    no156(s, t);
}
#[test]
fn terminal156_an_unknown_original_row_baseline_blocks_the_parent() {
    let mut s = source156();
    s.3[2] = None;
    let t = tails156(&s);
    no156(s, t);
}

#[test]
fn terminal156_touching_source_segments_prove_an_internal_full_crosscut() {
    let mut s = source156();
    s.2.horizontal.extend([
        HRule {
            y: 20.,
            x0: 0.,
            x1: 75.,
        },
        HRule {
            y: 20.,
            x0: 75.,
            x1: 150.,
        },
    ]);
    let t = tails156(&s);
    no156(s, t);
}
#[test]
fn terminal156_overlapping_source_segments_prove_an_internal_full_crosscut() {
    let mut s = source156();
    s.2.horizontal.extend([
        HRule {
            y: 20.,
            x0: 0.,
            x1: 90.,
        },
        HRule {
            y: 20.,
            x0: 75.,
            x1: 150.,
        },
    ]);
    let t = tails156(&s);
    no156(s, t);
}
#[test]
fn terminal156_a_wider_original_crosscut_is_not_ignored_at_the_frame_edges() {
    let mut s = source156();
    s.2.horizontal.push(HRule {
        y: 20.,
        x0: -5.,
        x1: 155.,
    });
    let t = tails156(&s);
    no156(s, t);
}
#[test]
fn terminal156_a_real_source_crosscut_gap_is_never_bridged_by_proximity() {
    let mut s = source156();
    s.2.horizontal.extend([
        HRule {
            y: 20.,
            x0: 0.,
            x1: 74.875,
        },
        HRule {
            y: 20.,
            x0: 75.,
            x1: 150.,
        },
    ]);
    let t = tails156(&s);
    yes156(s, t, 3);
}

#[test]
fn terminal156_existing_source_grid_never_rebuilds_unneeded_evidence() {
    let mut s = source152(&[
        ("A", &[Some("11")]),
        ("B", &[Some("22")]),
        ("C", &[Some("33")]),
    ]);
    restore(&mut s.0, &s.1, &s.2, &s.3);
    assert_eq!(tables152(&s).len(), 1);
    let before = units152(&s.0);
    restore_matrix_with_terminal(&mut s.0, &s.1, &s.2, &s.3, || {
        panic!("existing source table requires no new evidence")
    });
    assert_eq!(units152(&s.0), before);
}
#[test]
fn terminal156_no_closed_parent_never_scans_unused_source_evidence() {
    let mut s = source156();
    s.2.horizontal.clear();
    let before = units152(&s.0);
    restore_matrix_with_terminal(&mut s.0, &s.1, &s.2, &s.3, || {
        panic!("no closed source parent")
    });
    assert_eq!(units152(&s.0), before);
}
