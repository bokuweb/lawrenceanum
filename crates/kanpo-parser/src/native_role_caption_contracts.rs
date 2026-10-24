//! Source caption provenance regressions.
use crate::native_role_continuation::{explicit_pair, RoleSource};
use crate::{
    native_word_axes::NativeAxis,
    trace::{Line, NativeBounds, Region, Role, Table, TableCell, Word, WordStatus},
};

struct F {
    regions: Vec<Region>,
    words: Vec<Word>,
    bounds: Vec<NativeBounds>,
    axes: Vec<NativeAxis>,
    labels: [Vec<usize>; 2],
    bodies: [usize; 2],
}
fn add(f: &mut F, b: [f32; 4], text: &str, axis: NativeAxis, status: WordStatus) -> usize {
    let id = f.words.len();
    f.words.push(Word {
        id,
        x0: b[0],
        y0: b[1],
        x1: b[2],
        y1: b[3],
        source_box: b,
        raw: text.into(),
        text: text.into(),
        status,
    });
    f.bounds.push(NativeBounds::Glyphs(b));
    f.axes.push(axis);
    id
}
fn set_box(f: &mut F, id: usize, b: [f32; 4]) {
    let w = &mut f.words[id];
    w.x0 = b[0];
    w.y0 = b[1];
    w.x1 = b[2];
    w.y1 = b[3];
    w.source_box = b;
    f.bounds[id] = NativeBounds::Glyphs(b);
}
fn union(f: &F, ids: &[usize]) -> [f32; 4] {
    ids.iter().fold(
        [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ],
        |a, &i| {
            let b = f.words[i].source_box;
            [
                a[0].min(b[0]),
                a[1].min(b[1]),
                a[2].max(b[2]),
                a[3].max(b[3]),
            ]
        },
    )
}
fn fixture(stem: &str, axis: NativeAxis, split: bool, unowned: bool) -> F {
    let mut f = F {
        regions: vec![],
        words: vec![],
        bounds: vec![],
        axes: vec![],
        labels: [vec![], vec![]],
        bodies: [0, 0],
    };
    let transform = |b: [f32; 4]| {
        if axis == NativeAxis::Right {
            [b[1], 200. - b[2], b[3], 200. - b[0]]
        } else {
            b
        }
    };
    for k in 0..2 {
        let text = format!("{stem}{}", if k == 0 { '後' } else { '前' });
        let offset = 100. * k as f32;
        if split {
            for (j, ch) in text.chars().enumerate() {
                let y = 20. + offset + 20. * j as f32;
                let id = add(
                    &mut f,
                    transform([100., y, 108., y + 8.]),
                    &ch.to_string(),
                    axis,
                    if unowned {
                        WordStatus::HeaderLabel
                    } else {
                        WordStatus::Used
                    },
                );
                f.labels[k].push(id);
            }
        } else {
            let id = add(
                &mut f,
                transform([100., 20. + offset, 108., 68. + offset]),
                &text,
                axis,
                if unowned {
                    WordStatus::HeaderLabel
                } else {
                    WordStatus::Used
                },
            );
            f.labels[k].push(id);
        }
        f.bodies[k] = add(
            &mut f,
            transform([60., 10. + offset, 68., 80. + offset]),
            "任意本文",
            axis,
            WordStatus::Used,
        );
    }
    for k in 0..2 {
        let role = if k == 0 { Role::After } else { Role::Before };
        let mut lines = vec![];
        if !unowned {
            lines.push(Line::text(
                format!("{stem}{}", if k == 0 { '後' } else { '前' }),
                f.labels[k].clone(),
            ));
        }
        lines.push(Line::text("任意本文".into(), vec![f.bodies[k]]));
        f.regions.push(Region {
            unit: 2,
            part: 0,
            role,
            lines,
        });
    }
    f
}
fn prove(f: &F) -> bool {
    explicit_pair(RoleSource {
        regions: &f.regions,
        words: &f.words,
        bounds: &f.bounds,
        axes: &f.axes,
    })
}
#[test]
fn both_publication_caption_forms_are_source_semantics() {
    for stem in ["改正", "変更"] {
        for axis in [NativeAxis::Down, NativeAxis::Right] {
            assert!(prove(&fixture(stem, axis, false, false)));
        }
    }
}
#[test]
fn split_source_caption_glyphs_need_no_spacing_cap() {
    assert!(prove(&fixture("変更", NativeAxis::Down, true, false)));
}
#[test]
fn explicitly_omitted_header_words_can_bind_by_native_scope() {
    assert!(prove(&fixture("改正", NativeAxis::Down, true, true)));
}
#[test]
fn an_original_header_cell_inside_a_table_is_still_source_evidence() {
    let mut f = fixture("変更", NativeAxis::Down, true, false);
    let b = union(&f, &f.labels[1]);
    let c = f.words[f.bodies[1]].source_box;
    let ids = f.labels[1]
        .iter()
        .copied()
        .chain([f.bodies[1]])
        .collect::<Vec<_>>();
    let outer = union(&f, &ids);
    let cells = vec![
        TableCell {
            row: 0,
            col: 0,
            rowspan: 1,
            colspan: 1,
            x0: b[0],
            y0: b[1],
            x1: b[2],
            y1: b[3],
            text: "変更前".into(),
            words: f.labels[1].clone(),
        },
        TableCell {
            row: 1,
            col: 0,
            rowspan: 1,
            colspan: 1,
            x0: c[0],
            y0: c[1],
            x1: c[2],
            y1: c[3],
            text: "任意本文".into(),
            words: vec![f.bodies[1]],
        },
    ];
    let table = Table {
        x0: outer[0],
        y0: outer[1],
        x1: outer[2],
        y1: outer[3],
        rows: 2,
        cols: 1,
        cells,
    };
    let mut line = Line::text(table.render(), vec![]);
    line.table = Some(table);
    f.regions[1].lines = vec![line];
    assert!(prove(&f));
}
#[test]
fn source_role_proof_is_covariant_under_scale_and_translation() {
    for (sx, sy, tx, ty) in [(0.5, 3., 30., 7.), (4., 0.25, 5., 20.)] {
        let mut f = fixture("変更", NativeAxis::Down, true, false);
        for i in 0..f.words.len() {
            let b = f.words[i].source_box;
            set_box(
                &mut f,
                i,
                [
                    b[0] * sx + tx,
                    b[1] * sy + ty,
                    b[2] * sx + tx,
                    b[3] * sy + ty,
                ],
            );
        }
        assert!(prove(&f));
    }
}
#[test]
fn arbitrary_body_wording_cannot_change_caption_evidence() {
    for text in ["x", "123.45", "別の長い内容"] {
        let mut f = fixture("変更", NativeAxis::Down, true, false);
        for i in f.bodies {
            f.words[i].raw = text.into();
            f.words[i].text = text.into();
        }
        assert!(prove(&f));
    }
}
#[test]
fn synthesized_region_labels_are_not_printed_source_captions() {
    let mut f = fixture("変更", NativeAxis::Down, true, false);
    for ids in &f.labels {
        for &i in ids {
            f.words[i].raw = "x".into();
            f.words[i].text = "x".into();
        }
    }
    assert!(!prove(&f));
}
#[test]
fn a_later_body_unit_ends_the_previous_role_scope() {
    let mut f = fixture("変更", NativeAxis::Down, true, false);
    let id = add(
        &mut f,
        [20., 210., 28., 240.],
        "body",
        NativeAxis::Down,
        WordStatus::Used,
    );
    f.regions.push(Region {
        unit: 3,
        part: 0,
        role: Role::Body,
        lines: vec![Line::text("body".into(), vec![id])],
    });
    assert!(!prove(&f));
}
#[test]
fn outside_text_ends_the_last_comparison_unit() {
    let mut f = fixture("変更", NativeAxis::Down, true, false);
    let id = add(
        &mut f,
        [20., 210., 28., 240.],
        "outside",
        NativeAxis::Down,
        WordStatus::Used,
    );
    f.regions.push(Region {
        unit: 2,
        part: 0,
        role: Role::Outside,
        lines: vec![Line::text("outside".into(), vec![id])],
    });
    assert!(!prove(&f));
}
#[test]
fn both_original_counterparts_are_required() {
    let mut f = fixture("変更", NativeAxis::Down, true, false);
    for &id in &f.labels[1] {
        f.words[id].raw = "x".into();
        f.words[id].text = "x".into();
    }
    assert!(!prove(&f));
}
#[test]
fn caption_words_must_have_complete_known_native_geometry_and_axes() {
    for mode in 0..4 {
        let mut f = fixture("変更", NativeAxis::Down, true, false);
        let id = f.labels[0][0];
        match mode {
            0 => f.bounds[id] = NativeBounds::Unknown,
            1 => f.bounds[id] = NativeBounds::Compatibility,
            2 => f.axes[id] = NativeAxis::Unknown,
            _ => f.axes[id] = NativeAxis::Compatibility,
        };
        assert!(!prove(&f));
    }
}
#[test]
fn stale_caption_projection_cannot_borrow_an_original_cell() {
    let mut f = fixture("変更", NativeAxis::Down, true, false);
    let id = f.labels[0][0];
    f.words[id].x0 += 1.;
    assert!(!prove(&f));
}
#[test]
fn duplicate_current_source_ownership_is_ambiguous() {
    let mut f = fixture("変更", NativeAxis::Down, true, false);
    let id = f.labels[0][0];
    f.regions[1].lines.push(Line::text("変".into(), vec![id]));
    assert!(!prove(&f));
}
#[test]
fn reversed_role_assignment_cannot_inherit_a_pair() {
    let mut f = fixture("変更", NativeAxis::Down, true, false);
    f.regions[0].role = Role::Before;
    f.regions[1].role = Role::After;
    assert!(!prove(&f));
}
#[test]
fn incompatible_caption_stems_do_not_form_a_publication_pair() {
    let mut f = fixture("変更", NativeAxis::Down, false, false);
    let id = f.labels[1][0];
    f.words[id].text = "改正前".into();
    f.words[id].raw = "改正前".into();
    assert!(!prove(&f));
}
#[test]
fn a_caption_mention_inside_body_is_not_a_leading_header() {
    let mut f = fixture("変更", NativeAxis::Down, true, false);
    let ids = f.labels.iter().flatten().copied().collect::<Vec<_>>();
    for i in ids {
        let mut b = f.words[i].source_box;
        b[0] = 20.;
        b[2] = 28.;
        set_box(&mut f, i, b);
    }
    assert!(!prove(&f));
}
#[test]
fn overlapping_native_role_bodies_are_not_a_source_pair() {
    let mut f = fixture("変更", NativeAxis::Down, true, false);
    let id = f.bodies[1];
    set_box(&mut f, id, [60., 10., 68., 80.]);
    assert!(!prove(&f));
}
#[test]
fn captions_from_an_earlier_unit_cannot_label_an_unrelated_last_pair() {
    for unowned in [false, true] {
        let mut f = fixture("変更", NativeAxis::Down, true, unowned);
        for r in &mut f.regions {
            r.unit = 0;
        }
        for k in 0..2 {
            let y = 10. + 100. * k as f32;
            let id = add(
                &mut f,
                [20., y, 28., y + 70.],
                "body",
                NativeAxis::Down,
                WordStatus::Used,
            );
            f.regions.push(Region {
                unit: 2,
                part: 0,
                role: if k == 0 { Role::After } else { Role::Before },
                lines: vec![Line::text("body".into(), vec![id])],
            });
        }
        assert!(!prove(&f));
    }
}
#[test]
fn incomplete_role_body_metadata_cannot_extend_a_scope() {
    for mode in 0..2 {
        let mut f = fixture("変更", NativeAxis::Down, true, false);
        let id = f.bodies[0];
        if mode == 0 {
            f.bounds[id] = NativeBounds::Unknown
        } else {
            f.axes[id] = NativeAxis::Unknown
        }
        assert!(!prove(&f));
    }
}
#[test]
fn slot_identity_and_valid_original_status_are_required() {
    for mode in 0..2 {
        let mut f = fixture("変更", NativeAxis::Down, true, false);
        let id = f.labels[0][0];
        if mode == 0 {
            f.words[id].id = 999
        } else {
            f.words[id].status = WordStatus::Noise
        }
        assert!(!prove(&f));
    }
}

#[test]
fn original_first_page_printed_captions_supply_explicit_source_roles() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let pdf =
        std::fs::read(root.join("tests/data/ruled-cell-flow/20261019h018100001.pdf")).unwrap();
    let inputs = crate::page_inputs(&pdf).unwrap();
    let a = crate::analyze_item(&inputs);
    let p = &a.pages[0];
    assert!(
        explicit_pair(RoleSource {
            regions: &p.regions,
            words: &p.words,
            bounds: &p.native_bounds,
            axes: &p.native_axes
        }),
        "The original printed change-after/change-before labels must be source witnesses"
    );
}
#[test]
fn the_following_page_cannot_mint_source_labels_from_legacy_inferred_roles() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut inputs = Vec::new();
    for name in ["20261019h018100001", "20261019h018100002"] {
        let pdf =
            std::fs::read(root.join(format!("tests/data/ruled-cell-flow/{name}.pdf"))).unwrap();
        inputs.extend(crate::page_inputs(&pdf).unwrap());
    }
    let a = crate::analyze_item(&inputs);
    let p = &a.pages[1];
    assert!(!explicit_pair(RoleSource{regions:&p.regions,words:&p.words,bounds:&p.native_bounds,axes:&p.native_axes}),"Legacy after/before assignments to ordinary coordinate lists are not original printed labels");
}

#[test]
fn an_opaque_merged_constituent_outside_the_target_scope_cannot_invalidate_printed_captions() {
    let mut f = fixture("変更", NativeAxis::Down, true, false);
    add(
        &mut f,
        [400., 20., 408., 60.],
        "opaque",
        NativeAxis::Down,
        WordStatus::Used,
    );
    assert!(prove(&f),"Legacy merges can retain Used constituents without a direct Region root; unrelated native support cannot invalidate a complete source-caption pair");
}
#[test]
fn an_unowned_source_occurrence_inside_a_role_support_is_incomplete_provenance() {
    let mut f = fixture("変更", NativeAxis::Down, true, false);
    add(
        &mut f,
        [70., 20., 78., 60.],
        "opaque",
        NativeAxis::Down,
        WordStatus::Used,
    );
    assert!(!prove(&f));
}
#[test]
fn unowned_used_constituents_are_not_declared_omitted_header_words() {
    let mut f = fixture("改正", NativeAxis::Down, true, true);
    for ids in &f.labels {
        for &id in ids {
            f.words[id].status = WordStatus::Used;
        }
    }
    assert!(!prove(&f));
}
#[test]
fn an_opaque_constituent_between_an_omitted_caption_and_its_field_prevents_binding() {
    let mut f = fixture("改正", NativeAxis::Down, true, true);
    add(
        &mut f,
        [80., 20., 88., 60.],
        "opaque",
        NativeAxis::Down,
        WordStatus::Used,
    );
    assert!(!prove(&f));
}

#[test]
fn omitted_whole_publication_captions_need_not_have_a_perfect_noise_tag() {
    for stem in ["改正", "変更"] {
        for axis in [NativeAxis::Down, NativeAxis::Right] {
            let mut f = fixture(stem, axis, false, true);
            for id in f.labels.iter().flatten() {
                f.words[*id].status = WordStatus::Noise;
            }
            assert!(prove(&f),"Complete original caption semantics and exclusive native scope must remain usable when the old diagnostic tagger omits whole words as Noise");
        }
    }
}
#[test]
fn mixed_omitted_caption_glyph_statuses_have_the_same_native_scope() {
    let mut f = fixture("変更", NativeAxis::Down, true, true);
    for ids in &f.labels {
        for &id in &ids[..2] {
            f.words[id].status = WordStatus::Noise;
        }
    }
    assert!(prove(&f));
}
#[test]
fn noise_without_native_caption_proof_cannot_extend_roles() {
    for mode in 0..4 {
        let mut f = fixture("変更", NativeAxis::Down, false, true);
        for id in f.labels.iter().flatten() {
            f.words[*id].status = WordStatus::Noise;
        }
        let id = f.labels[0][0];
        match mode {
            0 => f.bounds[id] = NativeBounds::Unknown,
            1 => f.axes[id] = NativeAxis::Unknown,
            2 => f.words[id].x0 += 1.,
            _ => {
                let b = [20., 20., 28., 68.];
                set_box(&mut f, id, b);
            }
        }
        assert!(!prove(&f));
    }
}
#[test]
fn valid_unowned_noise_inside_a_caption_support_is_an_opaque_obstacle() {
    let mut f = fixture("変更", NativeAxis::Down, true, false);
    add(
        &mut f,
        [80., 20., 88., 60.],
        "unknown",
        NativeAxis::Down,
        WordStatus::Noise,
    );
    assert!(!prove(&f));
}
#[test]
fn irrelevant_noise_without_geometry_cannot_erase_a_complete_printed_caption_pair() {
    let mut f = fixture("変更", NativeAxis::Down, true, false);
    let id = add(
        &mut f,
        [400., 20., 408., 60.],
        "unknown",
        NativeAxis::Unknown,
        WordStatus::Noise,
    );
    f.bounds[id] = NativeBounds::Unknown;
    assert!(prove(&f));
}

#[test]
fn an_unrelated_noise_axis_cannot_invalidate_known_source_caption_axes() {
    let mut f = fixture("変更", NativeAxis::Down, true, false);
    add(
        &mut f,
        [400., 20., 408., 60.],
        "unknown",
        NativeAxis::Unknown,
        WordStatus::Noise,
    );
    assert!(prove(&f));
}

#[test]
fn a_rewritten_caption_cannot_borrow_the_geometry_of_an_unrelated_raw_word() {
    for omitted in [false, true] {
        for split in [false, true] {
            let mut f = fixture("変更", NativeAxis::Down, split, omitted);
            let id = f.labels[0][0];
            f.words[id].raw = "unrelated".into();
            assert!(!prove(&f),"Semantic caption evidence must be backed by the same original source text, not only a current rewritten label and its rectangle");
        }
    }
}
#[test]
fn original_caption_whitespace_uses_the_same_normalization_as_the_text_layer() {
    let mut f = fixture("変更", NativeAxis::Down, false, false);
    for ids in &f.labels {
        for &id in ids {
            f.words[id].raw = f.words[id].raw.chars().map(|c| format!(" {c}　")).collect();
        }
    }
    assert!(prove(&f));
}
