// Whole-scene ordering contracts, frozen before an ordering implementation.

fn scene(f: &F) -> Option<Vec<SceneAtom>> {
    let pair = plan(f)?;
    source_scene(
        Source {
            inherited_explicit_roles: f.inherited_explicit_roles,
            words: &f.words,
            cols: &f.cols,
            bounds: &f.bounds,
            axes: &f.axes,
            grids: &f.grids,
            rules: &f.rules,
        },
        pair,
    )
}
struct S {
    f: F,
    expected: Vec<(bool, Vec<usize>)>,
}
fn add_closed(f: &mut F, rect: [f32; 4], rows: usize, columns: usize, text: &str) -> Vec<usize> {
    let xs: Vec<_> = (0..=columns)
        .map(|i| rect[0] + (rect[2] - rect[0]) * i as f32 / columns as f32)
        .collect();
    let ys: Vec<_> = (0..=rows)
        .map(|i| rect[1] + (rect[3] - rect[1]) * i as f32 / rows as f32)
        .collect();
    f.grids.push(Grid {
        xs: xs.clone(),
        ys: ys.clone(),
        cells: (0..rows)
            .flat_map(|y| {
                (0..columns).map(move |x| GridCell {
                    x0: x,
                    x1: x + 1,
                    y0: y,
                    y1: y + 1,
                })
            })
            .collect(),
    });
    for &x in &xs {
        f.rules.vertical.push(VRule {
            x,
            y0: rect[1],
            y1: rect[3],
        });
    }
    for &y in &ys {
        f.rules.horizontal.push(HRule {
            y,
            x0: rect[0],
            x1: rect[2],
        });
    }
    let mut ids = vec![];
    for y in 0..rows {
        for x in 0..columns {
            let (w, h) = (xs[x + 1] - xs[x], ys[y + 1] - ys[y]);
            ids.push(add_word(
                f,
                [
                    xs[x] + w * 0.2,
                    ys[y] + h * 0.2,
                    xs[x] + w * 0.8,
                    ys[y] + h * 0.8,
                ],
                text,
            ));
        }
    }
    ids
}
fn scene_fixture(frames: usize, rows: usize, foreign: bool, text: &str) -> S {
    let mut f = source_pair(2, "cell");
    let pair = (0..f.words.len()).collect();
    let mut expected = vec![(true, pair)];
    if foreign {
        let a = add_word(&mut f, [5., 80., 145., 88.], text);
        let b = add_word(&mut f, [5., 100., 145., 108.], text);
        expected.push((false, vec![a, b]));
    }
    for j in 0..frames {
        let left = 200. + 200. * j as f32;
        let ids = add_closed(&mut f, [left, 20., left + 100., 140.], rows, 2, text);
        expected.push((false, ids));
    }
    S { f, expected }
}
fn ids(atom: &SceneAtom) -> Vec<usize> {
    match atom {
        SceneAtom::Pair(p) => p.after.iter().chain(&p.before).copied().collect(),
        SceneAtom::Body(ids) => ids.clone(),
    }
}
fn accepted(s: S) {
    let atoms = scene(&s.f).expect("A complete uniquely ruled source scene must be ordered");
    let got: Vec<_> = atoms
        .iter()
        .map(|a| {
            let mut owned = ids(a);
            owned.sort_unstable();
            (matches!(a, SceneAtom::Pair(_)), owned)
        })
        .collect();
    assert_eq!(got, s.expected);
    let mut all: Vec<_> = atoms.iter().flat_map(ids).collect();
    all.sort_unstable();
    let used: Vec<_> =
        s.f.words
            .iter()
            .filter(|w| w.status == WordStatus::Used)
            .map(|w| w.id)
            .collect();
    assert_eq!(
        all, used,
        "All current original occurrences are retained exactly once"
    );
}
#[test]
fn scene_pair_opaque_body_and_neighbor_frames_follow_native_rulings() {
    accepted(scene_fixture(2, 2, true, "record"));
}
#[test]
fn scene_requires_no_frame_or_record_count_cap() {
    for n in [1, 3, 7] {
        for rows in [1, 5, 19] {
            accepted(scene_fixture(n, rows, true, "data"));
        }
    }
}
#[test]
fn scene_body_wording_does_not_choose_an_order() {
    for text in ["x", "123.45", "任意の長い本文"] {
        accepted(scene_fixture(2, 3, true, text));
    }
}
#[test]
fn scene_closing_rule_can_certify_an_ordinary_tail_without_a_successor() {
    accepted(scene_fixture(0, 1, true, "tail"));
}
#[test]
fn scene_can_order_only_the_pair_and_two_ordinary_frames() {
    accepted(scene_fixture(2, 2, false, "data"));
}
#[test]
fn scene_ordering_is_covariant_under_native_scale_and_translation() {
    for (sx, sy, tx, ty) in [(0.5, 3., 30., 7.), (4., 0.25, 5., 20.)] {
        let mut s = scene_fixture(2, 2, true, "data");
        let f = &mut s.f;
        let tr = |b: &mut [f32; 4]| {
            b[0] = b[0] * sx + tx;
            b[2] = b[2] * sx + tx;
            b[1] = b[1] * sy + ty;
            b[3] = b[3] * sy + ty;
        };
        for w in &mut f.words {
            let mut b = [w.x0, w.y0, w.x1, w.y1];
            tr(&mut b);
            w.x0 = b[0];
            w.y0 = b[1];
            w.x1 = b[2];
            w.y1 = b[3];
            tr(&mut w.source_box);
        }
        for c in &mut f.cols {
            let mut b = [c.x, c.y, c.x1, c.y1];
            tr(&mut b);
            c.x = b[0];
            c.y = b[1];
            c.x1 = b[2];
            c.y1 = b[3];
        }
        for b in &mut f.bounds {
            if let NativeBounds::Glyphs(b) = b {
                tr(b);
            }
        }
        for g in &mut f.grids {
            for x in &mut g.xs {
                *x = *x * sx + tx;
            }
            for y in &mut g.ys {
                *y = *y * sy + ty;
            }
        }
        for v in &mut f.rules.vertical {
            v.x = v.x * sx + tx;
            v.y0 = v.y0 * sy + ty;
            v.y1 = v.y1 * sy + ty;
        }
        for h in &mut f.rules.horizontal {
            h.y = h.y * sy + ty;
            h.x0 = h.x0 * sx + tx;
            h.x1 = h.x1 * sx + tx;
        }
        accepted(s);
    }
}
#[test]
fn scene_ordinary_components_may_continue_without_a_drawn_top_cap() {
    let mut s = scene_fixture(2, 2, true, "data");
    s.f.rules
        .horizontal
        .retain(|h| !(h.y == 20. && h.x0 == 200.));
    accepted(s);
}
#[test]
fn scene_a_foreign_envelope_must_not_overlap_a_frame() {
    let mut s = scene_fixture(2, 2, true, "data");
    add_word(&mut s.f, [205., 5., 235., 13.], "outside");
    assert!(scene(&s.f).is_none());
}
#[test]
fn scene_partial_native_membership_cannot_cross_an_ordinary_wall() {
    let mut s = scene_fixture(2, 2, true, "data");
    add_word(&mut s.f, [190., 50., 210., 58.], "cross");
    assert!(scene(&s.f).is_none());
}
#[test]
fn scene_overlapping_incompatible_frames_do_not_claim_occurrences_twice() {
    let mut s = scene_fixture(2, 2, true, "data");
    add_closed(&mut s.f, [250., 20., 350., 140.], 2, 2, "other");
    assert!(scene(&s.f).is_none());
}
#[test]
fn scene_nested_frames_remain_one_opaque_parent_scope() {
    let mut s = scene_fixture(2, 2, true, "data");
    let extra = add_closed(&mut s.f, [210., 30., 290., 130.], 2, 2, "nested");
    s.expected[2].1.extend(extra);
    accepted(s);
}
#[test]
fn scene_an_empty_unrelated_frame_adds_no_material_atom() {
    let mut s = scene_fixture(2, 2, true, "data");
    let start = s.f.words.len();
    add_closed(&mut s.f, [800., 20., 900., 140.], 2, 2, "empty");
    s.f.words.truncate(start);
    s.f.cols.truncate(start);
    s.f.bounds.truncate(start);
    s.f.axes.truncate(start);
    accepted(s);
}
#[test]
fn scene_diagonal_incomparable_material_cannot_be_guessed_into_order() {
    let mut f = source_pair(2, "cell");
    add_closed(&mut f, [200., 20., 300., 140.], 2, 2, "data");
    add_closed(&mut f, [100., 200., 200., 320.], 2, 2, "other");
    assert!(scene(&f).is_none());
}
#[test]
fn scene_an_actual_current_caption_row_cannot_be_assumed_ordinary() {
    let mut s = scene_fixture(2, 2, true, "data");
    let a = s.expected[2].1[0];
    let b = s.expected[2].1[1];
    for (id, text) in [(a, "変更後"), (b, "変更前")] {
        s.f.words[id].text = text.into();
        s.f.words[id].raw = text.into();
        s.f.cols.iter_mut().find(|c| c.id == id).unwrap().text = text.into();
    }
    assert!(scene(&s.f).is_none(),"An inherited fallback must defer when a current atom has unclassified printed role captions");
}
#[test]
fn scene_a_prose_mention_is_not_a_printed_caption_row() {
    accepted(scene_fixture(2, 2, true, "変更後の欄に掲げる任意の本文"));
}
#[test]
fn scene_missing_ruling_fragments_do_not_get_a_wider_tolerance() {
    let mut s = scene_fixture(2, 2, true, "data");
    s.f.rules.vertical.retain(|v| v.x != 200.);
    s.f.rules.vertical.push(VRule {
        x: 200.,
        y0: 20.,
        y1: 90.,
    });
    s.f.rules.vertical.push(VRule {
        x: 200.,
        y0: 90.01,
        y1: 140.,
    });
    assert!(scene(&s.f).is_none());
}

#[test]
fn scene_an_inherited_pair_cannot_start_after_prior_ordinary_material() {
    let mut f = source_pair(2, "cell");
    add_word(&mut f, [-140., 25., -20., 33.], "priorbody");
    assert!(scene(&f).is_none());
}
#[test]
fn scene_ordinary_body_spans_can_continue_across_native_frame_components() {
    let mut s = scene_fixture(2, 2, true, "data");
    let a = s.expected[1].1[0];
    let b = s.expected[2].1[0];
    for (id, text) in [(a, "（opening"), (b, "closing）")] {
        s.f.words[id].text = text.into();
        s.f.words[id].raw = text.into();
        s.f.cols.iter_mut().find(|c| c.id == id).unwrap().text = text.into();
    }
    accepted(s);
}

#[test]
fn scene_a_gap_outside_the_required_shared_projection_does_not_widen_coverage() {
    let mut s = scene_fixture(2, 2, true, "data");
    s.f.rules.vertical.retain(|v| v.x != 200.);
    s.f.rules.vertical.push(VRule {
        x: 200.,
        y0: 20.,
        y1: 75.,
    });
    s.f.rules.vertical.push(VRule {
        x: 200.,
        y0: 75.01,
        y1: 140.,
    });
    accepted(s);
}
#[test]
fn scene_derived_axis_coordinates_are_not_asserted_to_be_drawn_strokes() {
    let mut s = scene_fixture(2, 2, true, "data");
    let last = s.f.grids[1].ys.len() - 1;
    s.f.grids[1].ys[last] += 0.125;
    accepted(s);
}
#[test]
fn scene_a_current_pair_with_printed_captions_uses_the_explicit_caption_path() {
    let mut s = scene_fixture(2, 2, true, "data");
    for (id, text) in [(0, "変更後"), (1, "変更前")] {
        s.f.words[id].text = text.into();
        s.f.words[id].raw = text.into();
        s.f.cols.iter_mut().find(|c| c.id == id).unwrap().text = text.into();
    }
    assert!(scene(&s.f).is_none());
}
