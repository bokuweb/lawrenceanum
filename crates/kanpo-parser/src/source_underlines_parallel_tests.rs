use super::*;

#[test]
fn separate_parallel_paints_count_lanes_in_all_cardinal_bases() {
    for side in [[1, 0], [-1, 0], [0, 1], [0, -1]] {
        for flow in [[1, 0], [-1, 0], [0, 1], [0, -1]] {
            if side[0] * flow[0] + side[1] * flow[1] != 0 {
                continue;
            }
            let basis = Basis { side, flow };
            let inverse = Basis {
                side: [side[0], flow[0]],
                flow: [side[1], flow[1]],
            };
            for count in [2, 3, 7] {
                let (placed, native, mut original) = super::tests::example(1);
                original.rectangle_ids = vec![1; count];
                original.rectangles = (0..count)
                    .map(|i| [15. + i as f64 * 0.75, 20., 15.25 + i as f64 * 0.75, 28.])
                    .collect();
                original.rulings = original.rectangles.clone();
                let physical = inverse.scene(&original);
                let oriented = basis.scene(&physical);
                let marked = decoration_paints(&placed, &native, &oriented);
                let hits = marked[&0]
                    .iter()
                    .map(|&(index, full_primary_support)| PaintHit {
                        index,
                        basis,
                        full_primary_support,
                    })
                    .collect::<Vec<_>>();
                assert_eq!(hits.len(), count);
                assert_eq!(parallel_strokes(&hits, &physical), Some(count));
                assert_eq!(
                    physical.rectangle_ids[hits[0].index],
                    physical.rectangle_ids[hits[1].index]
                );
                let mut framed = oriented.clone();
                framed
                    .lines
                    .push(([0., 20., 100., 20.], [0., 19.5, 100., 20.5]));
                assert!(decoration_paints(&placed, &native, &framed).is_empty());
                let mut hidden = oriented;
                hidden.images.push([6., 20., 17., 28.]);
                assert!(decoration_paints(&placed, &native, &hidden).is_empty());
            }
        }
    }
}

#[test]
fn orthogonal_cff_frame_strokes_and_foreign_paint_remain_obstacles_after_component_capture() {
    let (placed, mut native, mut scene) = super::tests::example(1);
    native.glyphs.push(native.glyphs[1].clone());
    scene.rectangle_ids = vec![1, 2];
    for rect in [[15., 20., 18., 20.5], [12., 22., 14.5, 22.5]] {
        scene.rectangles = vec![[15., 20., 15.5, 28.], rect];
        scene.rulings.clear();
        assert!(decoration_paints(&placed, &native, &scene).is_empty());
    }
}

#[test]
fn overlapping_segments_partial_marks_and_multiple_directions_do_not_invent_double_lines() {
    let (placed, mut native, mut scene) = super::tests::example(1);
    native.glyphs.push(native.glyphs[1].clone());
    scene.rectangle_ids = vec![1, 2];
    scene.rectangles = vec![[15., 20., 15.5, 28.], [15., 22., 15.5, 30.]];
    scene.rulings = scene.rectangles.clone();
    let basis = Basis {
        side: [1, 0],
        flow: [0, 1],
    };
    let hits = decoration_paints(&placed, &native, &scene)[&0]
        .iter()
        .map(|&(index, full_primary_support)| PaintHit {
            index,
            basis,
            full_primary_support,
        })
        .collect::<Vec<_>>();
    assert_eq!(hits.len(), 2);
    assert_eq!(parallel_strokes(&hits, &scene), Some(1));
    let mut crossed = hits.clone();
    crossed[1].basis = Basis {
        side: [0, 1],
        flow: [1, 0],
    };
    assert_eq!(parallel_strokes(&crossed, &scene), None);
    scene.rectangles = vec![[15., 22., 15.5, 26.], [16., 22., 16.5, 26.]];
    scene.rulings.clear();
    let short = decoration_paints(&placed, &native, &scene)[&0]
        .iter()
        .map(|&(index, full_primary_support)| PaintHit {
            index,
            basis,
            full_primary_support,
        })
        .collect::<Vec<_>>();
    assert_eq!(short.len(), 2);
    assert_eq!(parallel_strokes(&short, &scene), None);
}

#[test]
fn scalar_style_mapping_keeps_compound_tokens_uniform_and_requires_complete_mixed_mapping() {
    let make = |raw: String, text: &str| crate::trace::Word {
        id: 0,
        x0: 0.,
        y0: 0.,
        x1: 1.,
        y1: 1.,
        source_box: [0., 0., 1., 1.],
        raw,
        text: text.into(),
        status: WordStatus::Used,
    };
    let word = make("𠀀︑乙".into(), "𠀀、乙");
    assert_eq!(
        canonical_styles(&word, &[Some(2), Some(1), Some(3)]),
        Some(vec![Some(2), Some(1), Some(3)])
    );
    let raw = [0x661d, 0x662e]
        .map(|cid| char::from_u32(kanpo_pdf::bbox::IDENTITY_CID_BASE + cid).unwrap())
        .into_iter()
        .collect();
    let compound = make(raw, "（１８）");
    assert_eq!(
        canonical_styles(&compound, &[Some(2), Some(2)]),
        Some(vec![Some(2); 4])
    );
    assert_eq!(canonical_styles(&compound, &[Some(1), Some(2)]), None);
    let changed = make("𠀀︑乙".into(), "𠀀、丙");
    assert_eq!(
        canonical_styles(&changed, &[Some(2), Some(1), Some(3)]),
        None
    );
}

#[test]
fn source_annotation_positions_use_scalar_word_offsets_and_fail_closed_on_partial_streams() {
    let word = SourceWord { page: 7, word: 9 };
    let mut stream = Stream {
        projections:vec![],
        text: "前𠀀甲乙丙丁".into(),
        flags: vec![true; 6],
        complete: true,
        words: vec![(word, 2, 4)],
    };
    let counts = [((7, 9), Some(vec![Some(2), Some(2), Some(1), Some(3)]))]
        .into_iter()
        .collect();
    let a = annotations(Field::After, &stream, &counts);
    assert_eq!(
        a.iter()
            .map(|a| (a.offset, a.length, a.parallel_strokes))
            .collect::<Vec<_>>(),
        vec![(2, 2, 2), (5, 1, 3)]
    );
    assert!(a.iter().all(|a| a.source_word == word));
    stream.complete = false;
    assert!(annotations(Field::After, &stream, &counts).is_empty());
    stream.complete = true;
    stream.words[0].1 = 3;
    assert!(annotations(Field::After, &stream, &counts).is_empty());
}
