use super::*;

fn fixture(
    side: [i8; 2],
    flow: [i8; 2],
    scale: f64,
    shift: f64,
    lowered: bool,
) -> (Vec<Part>, kanpo_pdf::Page) {
    let physical = |p: [f64; 2]| {
        [
            shift + scale * (side[0] as f64 * p[0] + flow[0] as f64 * p[1]),
            shift + scale * (side[1] as f64 * p[0] + flow[1] as f64 * p[1]),
        ]
    };
    let box_at = |b: Rect| {
        let a = physical([b[0], b[1]]);
        let z = physical([b[2], b[3]]);
        [
            a[0].min(z[0]),
            a[1].min(z[1]),
            a[0].max(z[0]),
            a[1].max(z[1]),
        ]
    };
    let (baseline, small) = if lowered {
        (13., [10.5, 14.5])
    } else {
        (6., [3., 7.])
    };
    let specs = [
        (0, 10., 0., 8., 8., [4., 0., 12., 8.]),
        (1, baseline, 7.9, 11.9, 4., [small[0], 7.9, small[1], 11.9]),
        (
            1,
            baseline,
            11.9,
            15.9,
            4.,
            [small[0], 11.9, small[1], 15.9],
        ),
        (2, 10.01, 15.8, 23.8, 8., [4.01, 15.8, 12.01, 23.8]),
    ];
    let mut parts = Vec::new();
    let mut glyphs = Vec::new();
    for (id, &(word, normal, start, end, size, bounds)) in specs.iter().enumerate() {
        let a = physical([normal, start]);
        let b = physical([normal, end]);
        let g = kanpo_pdf::Glyph {
            text: "arbitrary".into(),
            code: 12345,
            cid: 54321,
            font: id,
            x: a[0],
            y: a[1],
            end_x: b[0],
            end_y: b[1],
            size: size * scale,
            vertical: false,
            bbox: box_at(bounds),
            invisible: false,
            spacing: 0.,
            kern: 0.,
            show: id as u32,
        };
        let basis = Basis::glyph(
            &g,
            kanpo_pdf::SourceTextAxes {
                x: flow,
                y: side.map(|n| -n),
            },
        )
        .unwrap();
        let a = basis.point(a);
        let b = basis.point(b);
        parts.push(Part {
            id,
            word,
            location: (0, 0, Some(0)),
            basis,
            bounds: basis.rect(g.bbox),
            start: a[1],
            end: b[1],
            baseline: a[0],
            size: g.size,
        });
        glyphs.push(g);
    }
    (
        parts,
        kanpo_pdf::Page {
            width: 600.,
            height: 800.,
            rotate: 0,
            glyphs,
            fonts: vec![],
            paths: vec![],
        },
    )
}
fn observe(parts: &[Part], native: &kanpo_pdf::Page) -> Option<SourceRun> {
    shifted(&[&parts[1], &parts[2]], parts, native, &BTreeMap::new())
}
#[test]
fn bracketed_runs_use_font_axes_in_all_cardinal_transforms_and_scales() {
    for side in [[1, 0], [-1, 0], [0, 1], [0, -1]] {
        for flow in [[1, 0], [-1, 0], [0, 1], [0, -1]] {
            if side[0] * flow[0] + side[1] * flow[1] != 0 {
                continue;
            }
            for scale in [0.125, 1., 64.] {
                for shift in [-256., 0., 128.] {
                    for lower in [false, true] {
                        let (parts, native) = fixture(side, flow, scale, shift, lower);
                        let run = observe(&parts, &native).unwrap();
                        assert_eq!(
                            run.shift,
                            if lower { Shift::Lowered } else { Shift::Raised }
                        );
                        assert_eq!(run.glyphs, vec![1, 2]);
                        assert_eq!((run.previous_glyph, run.following_glyph), (0, 3));
                        assert_eq!(
                            run.bounds,
                            union(native.glyphs[1].bbox, native.glyphs[2].bbox)
                        );
                        assert_eq!(run.parent_baselines, [parts[0].baseline, parts[3].baseline]);
                    }
                }
            }
        }
    }
}
#[test]
fn separate_rows_occupied_ruby_slots_overprints_and_ambiguous_parents_are_refused() {
    for mutation in 0..9 {
        let (mut parts, mut native) = fixture([1, 0], [0, 1], 1., 0., false);
        match mutation {
            0 => parts[3].location = (0, 1, Some(0)),
            1 => parts[3].size = parts[1].size,
            2 => {
                parts[1].bounds[2] = 10.;
                parts[2].bounds[2] = 10.;
            }
            3 => parts[2].baseline += 0.5,
            4 => {
                parts[3].start += 32.;
                parts[3].end += 32.;
            }
            5 => {
                let mut p = parts[0].clone();
                p.id = 4;
                p.word = 9;
                p.start = 8.;
                p.end = 16.;
                parts.push(p);
                native.glyphs.push(native.glyphs[0].clone());
            }
            6 => {
                let mut p = parts[0].clone();
                p.id = 4;
                p.word = 9;
                parts.push(p);
                native.glyphs.push(native.glyphs[0].clone());
            }
            7 => {
                let mut g = native.glyphs[1].clone();
                g.text = "unmapped overprint".into();
                native.glyphs.push(g);
            }
            _ => {
                parts[2].start = parts[1].start;
                parts[2].end = parts[1].end;
            }
        }
        assert!(observe(&parts, &native).is_none(), "mutation {mutation}");
    }
}

#[test]
fn a_proved_paint_envelope_can_exclude_an_overlapping_em_box() {
    let (parts, mut native) = fixture([1, 0], [0, 1], 1., 0., false);
    let mut g = native.glyphs[1].clone();
    g.text = "another source glyph".into();
    native.glyphs.push(g);
    assert!(observe(&parts, &native).is_none());
    let mut paint = BTreeMap::from([(4, vec![[1., 7.9, 2., 15.9]])]);
    assert!(shifted(&[&parts[1], &parts[2]], &parts, &native, &paint).is_some());
    paint.get_mut(&4).unwrap().push(native.glyphs[1].bbox);
    assert!(shifted(&[&parts[1], &parts[2]], &parts, &native, &paint).is_none());
}

#[test]
fn duplicate_word_ids_cannot_overwrite_another_equal_text_occurrence() {
    let pdf = include_bytes!("../tests/data/workbook/case-09.pdf");
    let source = kanpo_pdf::extract_pages_with_aj1_sources(pdf).unwrap();
    let mut page = crate::analyze_item(&crate::page_inputs(pdf).unwrap())
        .pages
        .remove(3);
    assert_eq!(page.words[2].raw, page.words[39].raw);
    assert_ne!(page.words[2].source_box, page.words[39].source_box);
    assert_eq!(occurrences(&page, source[3].page())[&2], vec![1273, 1274]);
    page.words[39].id = 2;
    let mapping = occurrences(&page, source[3].page());
    assert!(!mapping.contains_key(&2));
    assert!(!mapping.contains_key(&39));
    assert_eq!(
        mapping[&40],
        occurrences(
            &crate::analyze_item(&crate::page_inputs(pdf).unwrap()).pages[3],
            source[3].page()
        )[&40]
    );
}
