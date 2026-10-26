use crate::lines::{HRule, PageRules, VRule};
use crate::quoted_inline_tables as candidate;
use crate::trace::{Line, Table, TableCell, Unit, Word, WordStatus};
include!("quoted_inline_table_shape_fixture.rs");
fn state(u: &[Unit]) -> String {
    serde_json::to_string(&u.iter().map(|v|match v{Unit::Plain(l)=>serde_json::json!({"plain":l}),Unit::Shinkyu{preamble,after,before,outside,..}=>serde_json::json!({"preamble":preamble,"after":after,"before":before,"outside":outside})}).collect::<Vec<_>>()).unwrap()
}
fn plain(u: &mut Unit) -> &mut Vec<Line> {
    let Unit::Plain(l) = u else { panic!() };
    l
}
#[test]
fn physical_original_pattern_repairs_quote_and_wrapped_cell() {
    let (mut u, w, r) = sample();
    candidate::restore(&mut u, &w, Some(&r));
    assert_eq!(plain(&mut u[0])[0].words, [22]);
    assert_eq!(plain(&mut u[1])[1].words, [29]);
    assert_eq!(plain(&mut u[1])[2].words, [23, 19]);
    let t = plain(&mut u[2])[0].table.as_ref().unwrap();
    assert_eq!(t.cells[0].words, [24, 31]);
    assert_eq!(t.cells[0].text, "及び防災庁");
}
fn negative(f: impl FnOnce(&mut Vec<Unit>, &mut Vec<Word>, &mut PageRules)) {
    let (mut u, mut w, mut r) = sample();
    f(&mut u, &mut w, &mut r);
    let before = state(&u);
    candidate::restore(&mut u, &w, Some(&r));
    assert_eq!(state(&u), before);
}
#[test]
fn missing_native_rules_no_change() {
    let (mut u, w, _) = sample();
    let before = state(&u);
    candidate::restore(&mut u, &w, None);
    assert_eq!(state(&u), before);
}
#[test]
fn missing_strip_cap_no_change() {
    negative(|_, _, r| r.horizontal.retain(|h| (h.y - 385.5).abs() > 0.01));
}
#[test]
fn missing_strip_vertical_no_change() {
    negative(|_, _, r| r.vertical.retain(|v| (v.x - 455.5).abs() > 0.01));
}
#[test]
fn closed_independent_strip_is_not_wrapped() {
    negative(|_, _, r| {
        r.horizontal.push(HRule {
            y: 421.,
            x0: 455.5,
            x1: 472.5,
        })
    });
}
#[test]
fn extra_unknown_empty_inside_strip_no_change() {
    negative(|_, w, _| {
        let mut z = w[24].clone();
        z.id = w.len();
        z.text.clear();
        z.raw = "\u{eaaa}".into();
        z.status = WordStatus::Empty;
        w.push(z)
    });
}
#[test]
fn unknown_fragment_does_not_partly_move() {
    negative(|u, w, _| {
        w[24].text = "及\u{eaaa}防災".into();
        plain(&mut u[1])[1].text = "を「及\u{eaaa}防災".into()
    });
}
#[test]
fn fragment_not_inside_native_strip_no_change() {
    negative(|_, w, _| {
        w[24].x0 += 9.;
        w[24].x1 += 9.
    });
}
#[test]
fn wrong_quote_word_no_change() {
    negative(|u, w, _| {
        w[17].text = "（".into();
        plain(&mut u[0])[0].text = "（".into()
    });
}
#[test]
fn duplicate_original_owner_no_change() {
    negative(|u, _, _| {
        plain(&mut u[2]).push(Line::text("及び防災".into(), vec![24]));
    });
}
#[test]
fn ordinary_inline_table_with_no_overflow_fragment_unchanged() {
    negative(|u, _, _| {
        plain(&mut u[1])[1].words = vec![23, 19];
        plain(&mut u[1])[1].text = "を「".into();
    });
}
#[test]
fn no_matching_prose_column_top_no_change() {
    negative(|_, w, _| {
        w[22].y0 += 40.;
    });
}
#[test]
fn ambiguous_third_table_cell_no_change() {
    negative(|u, _, _| {
        let t = plain(&mut u[2])[0].table.as_mut().unwrap();
        t.cols = 3;
        let mut c = t.cells[1].clone();
        c.col = 2;
        t.cells.push(c)
    });
}
// Metamorphic checks of the physical/quotation rule. No expected agency name,
// original page id, output offset, or original source id is used by the rule.
fn each_line(u: &mut [Unit], mut f: impl FnMut(&mut Line)) {
    for v in u {
        match v {
            Unit::Plain(ls) => {
                for l in ls {
                    f(l)
                }
            }
            _ => panic!("fixture has only Plain source units"),
        }
    }
}
fn native_counts(u: &[Unit]) -> std::collections::BTreeMap<usize, usize> {
    let mut m = std::collections::BTreeMap::new();
    for v in u {
        if let Unit::Plain(ls) = v {
            for l in ls {
                for id in l.words.iter().chain(
                    l.table
                        .iter()
                        .flat_map(|t| t.cells.iter().flat_map(|c| &c.words)),
                ) {
                    *m.entry(*id).or_insert(0) += 1;
                }
            }
        }
    }
    m
}
fn sync(l: &mut Line, w: &[Word]) {
    l.text = if let Some(t) = &l.table {
        t.render()
    } else {
        l.words.iter().map(|&id| w[id].text.as_str()).collect()
    }
}
fn prove_repair(mut u: Vec<Unit>, w: Vec<Word>, r: PageRules, fragment: &[usize], first: usize) {
    let counts = native_counts(&u);
    assert!(counts.values().all(|&n| n == 1));
    let before = state(&u);
    let old = u
        .iter()
        .filter_map(|v| {
            if let Unit::Plain(ls) = v {
                Some(ls)
            } else {
                None
            }
        })
        .flatten()
        .filter_map(|l| l.table.as_ref())
        .find(|t| t.cells[0].words == [first])
        .unwrap()
        .cells[0]
        .clone();
    let expected_text = fragment
        .iter()
        .map(|&id| w[id].text.as_str())
        .collect::<String>()
        + &old.text;
    let expected_ids = fragment
        .iter()
        .copied()
        .chain(old.words.iter().copied())
        .collect::<Vec<_>>();
    candidate::restore(&mut u, &w, Some(&r));
    assert_ne!(
        state(&u),
        before,
        "valid physical continuation must be restored"
    );
    assert_eq!(
        native_counts(&u),
        counts,
        "same original words owned exactly once"
    );
    let cell = u
        .iter()
        .filter_map(|v| {
            if let Unit::Plain(ls) = v {
                Some(ls)
            } else {
                None
            }
        })
        .flatten()
        .filter_map(|l| l.table.as_ref())
        .find_map(|t| t.cells.iter().find(|c| c.words == expected_ids))
        .unwrap();
    assert_eq!(cell.text, expected_text);
}
fn relabel_fragment(u: &mut [Unit], w: &mut [Word], s: &str) {
    w[24].text = s.into();
    w[24].raw = s.into();
    sync(&mut plain(&mut u[1])[1], w);
}
#[test]
fn wrap_rule_does_not_depend_on_label_script_or_length() {
    for label in [
        "防",
        "これは十二文字より十分に長い任意の置換対象名称である",
        "AlphaBeta_V2",
        "123456",
        "Δοκιμή",
    ] {
        let (mut u, mut w, r) = sample();
        relabel_fragment(&mut u, &mut w, label);
        prove_repair(u, w, r, &[24], 31);
    }
}
fn split_word(u: &mut [Unit], w: &mut Vec<Word>, id: usize, at: usize) -> usize {
    let s = w[id].text.clone();
    let left = s.chars().take(at).collect::<String>();
    let right = s.chars().skip(at).collect::<String>();
    assert!(!left.is_empty() && !right.is_empty());
    let y = w[id].y0 + (w[id].y1 - w[id].y0) * (at as f32 / s.chars().count() as f32);
    let mut b = w[id].clone();
    b.id = w.len();
    b.text = right.clone();
    b.raw = right;
    b.y0 = y;
    w[id].text = left.clone();
    w[id].raw = left;
    w[id].y1 = y;
    let new = b.id;
    w.push(b);
    each_line(u, |l| {
        if l.table.is_none() {
            if let Some(i) = l.words.iter().position(|&v| v == id) {
                l.words.insert(i + 1, new);
                sync(l, w)
            }
        }
    });
    new
}
#[test]
fn wrap_rule_preserves_a_fragment_split_into_multiple_native_words() {
    let (mut u, mut w, r) = sample();
    let id = split_word(&mut u, &mut w, 24, 2);
    prove_repair(u, w, r, &[24, id], 31);
}
#[test]
fn wrap_rule_accepts_prose_continuation_split_into_native_words() {
    let (mut u, mut w, r) = sample();
    split_word(&mut u, &mut w, 22, 8);
    prove_repair(u, w, r, &[24], 31);
}
#[test]
fn wrap_rule_accepts_replacement_connector_in_one_native_word() {
    let (mut u, mut w, r) = sample();
    w[23].text = "を「".into();
    w[23].raw = "を「".into();
    w[23].x0 = w[23].x0.min(w[19].x0);
    w[23].x1 = w[23].x1.max(w[19].x1);
    w[23].y0 = w[23].y0.min(w[19].y0);
    w[23].y1 = w[23].y1.max(w[19].y1);
    plain(&mut u[1])[1].words.retain(|&id| id != 19);
    sync(&mut plain(&mut u[1])[1], &w);
    w.remove(19);
    for (i, v) in w.iter_mut().enumerate() {
        v.id = i
    }
    each_line(&mut u, |l| {
        for id in &mut l.words {
            assert_ne!(*id, 19);
            if *id > 19 {
                *id -= 1
            }
        }
        if let Some(t) = &mut l.table {
            for c in &mut t.cells {
                for id in &mut c.words {
                    assert_ne!(*id, 19);
                    if *id > 19 {
                        *id -= 1
                    }
                }
            }
        }
    });
    prove_repair(u, w, r, &[23], 30);
}
fn extra_line(w: &mut Vec<Word>, s: &str) -> Line {
    let mut v = w[22].clone();
    v.id = w.len();
    v.raw = s.into();
    v.text = s.into();
    v.x0 += 300.;
    v.x1 += 300.;
    let id = v.id;
    w.push(v);
    Line::text(s.into(), vec![id])
}
#[test]
fn wrap_rule_preserves_unrelated_surrounding_plain_lines() {
    let (mut u, mut w, r) = sample();
    for i in 0..3 {
        let l = extra_line(&mut w, &format!("周辺{i}"));
        plain(&mut u[i]).insert(0, l);
    }
    prove_repair(u, w, r, &[24], 31);
}
fn affine(u: &mut [Unit], w: &mut [Word], r: &mut PageRules, s: f32, dx: f32, dy: f32) {
    for v in w {
        v.x0 = v.x0 * s + dx;
        v.x1 = v.x1 * s + dx;
        v.y0 = v.y0 * s + dy;
        v.y1 = v.y1 * s + dy
    }
    each_line(u, |l| {
        if let Some(t) = &mut l.table {
            t.x0 = t.x0 * s + dx;
            t.x1 = t.x1 * s + dx;
            t.y0 = t.y0 * s + dy;
            t.y1 = t.y1 * s + dy;
            for c in &mut t.cells {
                c.x0 = c.x0 * s + dx;
                c.x1 = c.x1 * s + dx;
                c.y0 = c.y0 * s + dy;
                c.y1 = c.y1 * s + dy
            }
        }
    });
    for h in &mut r.horizontal {
        h.x0 = h.x0 * s + dx;
        h.x1 = h.x1 * s + dx;
        h.y = h.y * s + dy
    }
    for v in &mut r.vertical {
        v.x = v.x * s + dx;
        v.y0 = v.y0 * s + dy;
        v.y1 = v.y1 * s + dy
    }
}
#[test]
fn wrap_rule_is_invariant_under_uniform_scale() {
    for s in [0.25, 0.5, 2., 4.] {
        let (mut u, mut w, mut r) = sample();
        affine(&mut u, &mut w, &mut r, s, 0., 0.);
        prove_repair(u, w, r, &[24], 31);
    }
}
#[test]
fn wrap_rule_is_invariant_under_translation() {
    for (dx, dy) in [(137., 52.), (-15., -20.)] {
        let (mut u, mut w, mut r) = sample();
        affine(&mut u, &mut w, &mut r, 1., dx, dy);
        prove_repair(u, w, r, &[24], 31);
    }
}
#[test]
fn wrap_rule_supports_a_vertical_stack_of_three_cells() {
    let (mut u, mut w, mut r) = sample();
    for ui in [1, 2] {
        let t = plain(&mut u[ui])[0].table.as_mut().unwrap();
        let old = t.cells[1].clone();
        let mid = (old.y0 + old.y1) * 0.5;
        let mut new = old.clone();
        new.col = 2;
        new.y0 = mid;
        let mut word = w[old.words[0]].clone();
        word.id = w.len();
        word.text = "第三欄".into();
        word.raw = word.text.clone();
        word.y0 = mid;
        word.y1 = old.y1;
        new.words = vec![word.id];
        new.text = word.text.clone();
        w.push(word);
        for &id in &t.cells[1].words {
            w[id].y0 = old.y0 + (w[id].y0 - old.y0) * 0.5;
            w[id].y1 = old.y0 + (w[id].y1 - old.y0) * 0.5;
        }
        t.cells[1].y1 = mid;
        t.cols = 3;
        t.cells.push(new);
        r.horizontal.push(HRule {
            x0: t.x0,
            x1: t.x1,
            y: mid,
        });
        let l = &mut plain(&mut u[ui])[0];
        sync(l, &w);
    }
    prove_repair(u, w, r, &[24], 31);
}
#[test]
fn wrap_rule_rejects_an_ambiguous_native_strip() {
    negative(|_, _, r| {
        let cap = r
            .horizontal
            .iter()
            .find(|h| (h.y - 385.5).abs() < 0.01)
            .unwrap()
            .clone();
        r.horizontal.push(cap);
        let v = r
            .vertical
            .iter()
            .find(|v| (v.x - 455.5).abs() < 0.01)
            .unwrap()
            .clone();
        r.vertical.push(v);
    });
}
#[test]
fn wrap_rule_rejects_two_independent_possible_quote_openers() {
    let (mut u, mut w, r) = sample();
    let old = plain(&mut u[0]).clone();
    for l in old {
        let mut nl = l.clone();
        for id in &mut nl.words {
            let mut nw = w[*id].clone();
            nw.id = w.len();
            *id = nw.id;
            w.push(nw)
        }
        plain(&mut u[0]).push(nl)
    }
    let before = state(&u);
    candidate::restore(&mut u, &w, Some(&r));
    assert_eq!(state(&u), before);
}

#[test]
fn a_source_word_crossing_a_cell_boundary_must_not_be_reassigned() {
    negative(|_, w, _| {
        w[32].y0 = 110.;
    });
}
#[test]
fn closing_quote_and_following_prose_can_share_one_native_word() {
    let (mut u, mut w, r) = sample();
    w[35].text = w[35].text.clone() + &w[33].text;
    w[35].raw = w[35].text.clone();
    w[35].x0 = w[35].x0.min(w[33].x0);
    w[35].x1 = w[35].x1.max(w[33].x1);
    w[35].y0 = w[35].y0.min(w[33].y0);
    w[35].y1 = w[35].y1.max(w[33].y1);
    plain(&mut u[2])[1].words = vec![35];
    sync(&mut plain(&mut u[2])[1], &w);
    w.remove(33);
    for (i, v) in w.iter_mut().enumerate() {
        v.id = i
    }
    each_line(&mut u, |l| {
        for id in &mut l.words {
            assert_ne!(*id, 33);
            if *id > 33 {
                *id -= 1
            }
        }
        if let Some(t) = &mut l.table {
            for c in &mut t.cells {
                for id in &mut c.words {
                    assert_ne!(*id, 33);
                    if *id > 33 {
                        *id -= 1
                    }
                }
            }
        }
    });
    prove_repair(u, w, r, &[24], 31);
}

#[test]
fn all_semantic_lines_in_one_plain_unit_produce_the_same_repair() {
    let (u, w, r) = sample();
    let lines = u
        .into_iter()
        .flat_map(|v| {
            if let Unit::Plain(ls) = v {
                ls
            } else {
                panic!()
            }
        })
        .collect();
    prove_repair(vec![Unit::Plain(lines)], w, r, &[24], 31);
}
#[test]
fn each_semantic_line_in_its_own_plain_unit_produces_the_same_repair() {
    let (u, w, r) = sample();
    let units = u
        .into_iter()
        .flat_map(|v| {
            if let Unit::Plain(ls) = v {
                ls
            } else {
                panic!()
            }
        })
        .map(|l| Unit::Plain(vec![l]))
        .collect();
    prove_repair(units, w, r, &[24], 31);
}
#[test]
fn two_independent_physical_continuations_are_repaired_once_each() {
    let (mut u, mut w, r) = sample();
    let (mut more, mut ww, mut rr) = sample();
    let n = w.len();
    affine(&mut more, &mut ww, &mut rr, 1., 600., 0.);
    for v in &mut ww {
        v.id += n
    }
    each_line(&mut more, |l| {
        for id in &mut l.words {
            *id += n
        }
        if let Some(t) = &mut l.table {
            for c in &mut t.cells {
                for id in &mut c.words {
                    *id += n
                }
            }
        }
    });
    u.extend(more);
    w.extend(ww);
    let mut rules = r;
    rules.horizontal.extend(rr.horizontal);
    rules.vertical.extend(rr.vertical);
    let before = native_counts(&u);
    candidate::restore(&mut u, &w, Some(&rules));
    assert_eq!(native_counts(&u), before);
    let cells = u
        .iter()
        .filter_map(|v| {
            if let Unit::Plain(ls) = v {
                Some(ls)
            } else {
                None
            }
        })
        .flatten()
        .filter_map(|l| l.table.as_ref())
        .flat_map(|t| &t.cells)
        .collect::<Vec<_>>();
    for expected in [vec![24, 31], vec![24 + n, 31 + n]] {
        assert_eq!(cells.iter().filter(|c| c.words == expected).count(), 1);
    }
}
#[test]
fn a_new_old_role_boundary_is_never_crossed() {
    let (mut u, w, r) = sample();
    let ls = plain(&mut u[1]).clone();
    u[1] = Unit::Shinkyu {labels:None,
        preamble: vec![],
        after: vec![ls],
        before: vec![],
        outside: vec![],
    };
    let before = state(&u);
    candidate::restore(&mut u, &w, Some(&r));
    assert_eq!(state(&u), before);
}

#[test]
fn closing_before_opening_in_ordinary_prose_refuses_the_repair() {
    negative(|u, w, _| {
        w[22].text = "」継続「".into();
        w[22].raw = w[22].text.clone();
        sync(&mut plain(&mut u[0])[1], w);
    });
}
#[test]
fn every_empty_source_bbox_touching_the_wrapped_strip_refuses_the_repair() {
    let mut changed = Vec::new();
    for (name, bbox) in [
        ("normal", [454., 390., 456., 392.]),
        ("inverted", [456., 392., 454., 390.]),
        ("edge", [453.5, 390., 455.5, 392.]),
        ("zero_width", [456., 383.5, 456., 385.5]),
    ] {
        let (mut u, mut w, r) = sample();
        let mut z = w[24].clone();
        z.id = w.len();
        z.text.clear();
        z.raw = "\u{EAAA}".into();
        z.status = WordStatus::Empty;
        [z.x0, z.y0, z.x1, z.y1] = bbox;
        let id = z.id;
        w.push(z);
        let before = state(&u);
        candidate::restore(&mut u, &w, Some(&r));
        let unchanged = state(&u) == before;
        eprintln!("strip Empty bbox {name}, id={id}, unchanged={unchanged}");
        if !unchanged {
            changed.push(name);
        }
    }
    assert!(
        changed.is_empty(),
        "intersecting Empty source ignored for bbox variants: {changed:?}"
    );
}
#[test]
fn an_independently_owned_used_word_touching_the_strip_refuses_the_repair() {
    let (mut u, mut w, r) = sample();
    let mut z = w[24].clone();
    z.id = w.len();
    z.text = "独立注記".into();
    z.raw = z.text.clone();
    z.status = WordStatus::Used;
    z.x0 = 454.;
    z.x1 = 456.;
    z.y0 = 390.;
    z.y1 = 392.;
    let id = z.id;
    w.push(z);
    plain(&mut u[0]).push(Line::text(w[id].text.clone(), vec![id]));
    assert_eq!(native_counts(&u)[&id], 1);
    let before = state(&u);
    candidate::restore(&mut u, &w, Some(&r));
    assert_eq!(
        state(&u),
        before,
        "one owner does not make intersecting source part of this cell"
    );
}
#[test]
fn properly_nested_ordinary_quotes_remain_a_valid_continuation() {
    let (mut u, mut w, r) = sample();
    w[22].text = "「外側「内側」外側」に、".into();
    w[22].raw = w[22].text.clone();
    sync(&mut plain(&mut u[0])[1], &w);
    prove_repair(u, w, r, &[24], 31);
}
#[test]
fn two_disjoint_id_plans_with_interleaving_source_ranges_are_both_refused() {
    let (u, mut w, mut r) = sample();
    let (mut other, mut ww, mut rr) = sample();
    let offset = w.len();
    affine(&mut other, &mut ww, &mut rr, 1., 600., 0.);
    for z in &mut ww {
        z.id += offset;
    }
    each_line(&mut other, |l| {
        for id in &mut l.words {
            *id += offset;
        }
        if let Some(t) = &mut l.table {
            for c in &mut t.cells {
                for id in &mut c.words {
                    *id += offset;
                }
            }
        }
    });
    let source_ids_a = native_counts(&u);
    let source_ids_b = native_counts(&other);
    assert!(source_ids_a.keys().all(|id| !source_ids_b.contains_key(id)));
    let mut interleaved = Vec::new();
    for (a, b) in u.into_iter().zip(other) {
        interleaved.push(a);
        interleaved.push(b);
    }
    w.extend(ww);
    r.horizontal.extend(rr.horizontal);
    r.vertical.extend(rr.vertical);
    assert!(native_counts(&interleaved).values().all(|&n| n == 1));
    let before = state(&interleaved);
    candidate::restore(&mut interleaved, &w, Some(&r));
    assert_eq!(
        state(&interleaved),
        before,
        "A1,A2,B1,B2,C1,C2 ranges conflict even without shared IDs"
    );
}
#[test]
fn any_extra_source_intersecting_the_new_first_cell_refuses_the_repair() {
    let mut changed = Vec::new();
    for used in [false, true] {
        for (name, bbox) in [
            ("normal", [434., 80., 436., 90.]),
            ("inverted", [436., 90., 434., 80.]),
            ("edge", [429.214, 80., 431.214, 82.]),
            ("zero_width", [435., 67., 435., 69.]),
        ] {
            let (mut u, mut w, r) = sample();
            let mut z = w[31].clone();
            z.id = w.len();
            [z.x0, z.y0, z.x1, z.y1] = bbox;
            z.text = if used {
                "別の原字".into()
            } else {
                String::new()
            };
            z.raw = if used {
                z.text.clone()
            } else {
                "\u{EAAA}".into()
            };
            z.status = if used {
                WordStatus::Used
            } else {
                WordStatus::Empty
            };
            let id = z.id;
            w.push(z);
            if used {
                plain(&mut u[0]).push(Line::text(w[id].text.clone(), vec![id]));
                assert_eq!(native_counts(&u)[&id], 1);
            }
            let before = state(&u);
            candidate::restore(&mut u, &w, Some(&r));
            let unchanged = state(&u) == before;
            eprintln!("first cell used={used} bbox={name}, id={id}, unchanged={unchanged}");
            if !unchanged {
                changed.push((used, name));
            }
        }
    }
    assert!(
        changed.is_empty(),
        "new first cell has source other than Word31: {changed:?}"
    );
}
#[test]
fn a_fragment_whose_id_order_reverses_its_native_geometry_is_refused() {
    let (mut u, mut w, r) = sample();
    let second = split_word(&mut u, &mut w, 24, 2);
    let l = &mut plain(&mut u[1])[1];
    let first = l.words.iter().position(|&id| id == 24).unwrap();
    assert_eq!(l.words[first + 1], second);
    l.words.swap(first, first + 1);
    sync(l, &w);
    assert!(w[24].y0 < w[second].y0);
    assert_eq!(&l.words[first..first + 2], &[second, 24]);
    let before = state(&u);
    candidate::restore(&mut u, &w, Some(&r));
    assert_eq!(
        state(&u),
        before,
        "source ownership and text sync cannot justify reverse geometric concatenation"
    );
}

#[test]
fn a_close_in_a_different_native_column_refuses_the_repair() {
    let mut changed = Vec::new();
    for id in [29, 35] {
        for dx in [-100., 100.] {
            let (mut u, mut w, r) = sample();
            w[id].x0 += dx;
            w[id].x1 += dx;
            assert_eq!(native_counts(&u)[&id], 1);
            let before = state(&u);
            candidate::restore(&mut u, &w, Some(&r));
            let unchanged = state(&u) == before;
            eprintln!("close source id={id}, dx={dx}, unchanged={unchanged}");
            if !unchanged {
                changed.push((id, dx));
            }
        }
    }
    assert!(
        changed.is_empty(),
        "closing source must belong to the corresponding native table column: {changed:?}"
    );
}
