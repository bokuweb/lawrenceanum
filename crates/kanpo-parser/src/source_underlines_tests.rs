use super::*;

#[test]
fn original_forward_projections_have_complete_source_styles_in_all_reordered_cells() {
    let pdf=include_bytes!("../tests/data/workbook/case-17.pdf");
    let sources=kanpo_pdf::extract_pages_with_aj1_sources(pdf).unwrap();let inputs=crate::page_inputs_from_sources(&sources);let native=crate::analyze_item(&inputs);
    for pi in [0,1,4,5,9] {
        let page=&native.pages[pi];let html=scraper::Html::parse_document(&inputs[pi].xhtml);let node=html.select(&scraper::Selector::parse("page").unwrap()).next().unwrap();
        let projection=crate::source_line_projection::Page::new(node,sources[pi].page().width as f32,sources[pi].page().height as f32,&inputs[pi],&sources[pi]);
        let aux=projection.auxiliary_locations(page);let scene:Scene=serde_json::from_str(node.value().attr("data-native-scene").unwrap()).unwrap();
        let styles=word_styles(page,sources[pi].page(),&sources[pi],&scene,pi,&BTreeSet::new(),&aux,&mut vec![],&mut vec![],&mut BTreeMap::new());
        for (ri,r) in page.regions.iter().enumerate(){for (li,l) in r.lines.iter().enumerate(){
            let mut check=|text:&str,ids:&[usize],cell:Option<usize>,tokens:Option<Vec<crate::source_line_projection::Token>>|{
                let record=tokens.map(|tokens|crate::source_line_projection::Record{page:pi,region:ri,line:li,cell,field:Field::Paragraph,offset:0,text:compact(text),tokens});
                let mut s=Stream::new();s.projected_part(text,ids,page,&styles,pi,record);
                assert!(s.complete,"page {pi} region {ri} line {li} cell {cell:?}");
                assert_eq!(s.flags.len(),compact(text).chars().count());
            };
            if let Some(t)=&l.table{for (ci,c) in t.cells.iter().enumerate(){
                let ids=crate::source_line_projection::ordered_cell(page,c).unwrap();check(&c.text,&ids,Some(ci),projection.cell(c,page));
            }}else{check(&l.text,&l.words,None,projection.line(l,page));}
        }}
    }
}

#[test]
fn actual_font_axes_keep_all_cardinal_rotations_and_reflections_in_the_same_geometry() {
    for side in [[1, 0], [-1, 0], [0, 1], [0, -1]] {
        for flow in [[1, 0], [-1, 0], [0, 1], [0, -1]] {
            if side[0] * flow[0] + side[1] * flow[1] != 0 {
                continue;
            }
            let forward = Basis { side, flow };
            let physical = |p: [f64; 2]| {
                [
                    side[0] as f64 * p[0] + flow[0] as f64 * p[1],
                    side[1] as f64 * p[0] + flow[1] as f64 * p[1],
                ]
            };
            let inverse = Basis {
                side: [side[0], flow[0]],
                flow: [side[1], flow[1]],
            };
            for vertical in [true, false] {
                let (used, mut page, scene) = example(3);
                for g in &mut page.glyphs {
                    let a = physical([g.x, g.y]);
                    let b = physical([g.end_x, g.end_y]);
                    g.x = a[0];
                    g.y = a[1];
                    g.end_x = b[0];
                    g.end_y = b[1];
                    g.bbox = inverse.rect(g.bbox);
                    g.vertical = vertical;
                }
                let axes = if vertical {
                    kanpo_pdf::SourceTextAxes {
                        x: side,
                        y: flow.map(|n| -n),
                    }
                } else {
                    kanpo_pdf::SourceTextAxes {
                        x: flow,
                        y: side.map(|n| -n),
                    }
                };
                let actual = Basis::glyph(&page.glyphs[0], axes).unwrap();
                assert_eq!(actual, forward);
                let transformed = used
                    .iter()
                    .map(|p| {
                        let mut q = p.clone();
                        q.bounds = actual.rect(inverse.rect(p.bounds));
                        q
                    })
                    .collect::<Vec<_>>();
                let transformed_scene = actual.scene(&inverse.scene(&scene));
                assert_eq!(
                    decorations(&transformed, &page, &transformed_scene).len(),
                    3
                );
                // Same advance, opposite normal: a mirrored font has its
                // underline on the other side; advance alone cannot prove it.
                let mirrored = kanpo_pdf::SourceTextAxes {
                    x: if vertical { axes.x.map(|n| -n) } else { axes.x },
                    y: if vertical { axes.y } else { axes.y.map(|n| -n) },
                };
                assert_eq!(
                    Basis::glyph(&page.glyphs[0], mirrored).unwrap().side,
                    side.map(|n| -n)
                );
            }
        }
    }
}

#[test]
fn incomplete_advance_or_axes_parallel_to_advance_do_not_prove_an_underline_side() {
    let axes = kanpo_pdf::SourceTextAxes {
        x: [1, 0],
        y: [0, -1],
    };
    let mut g = glyph(10., 20., 28.);
    for (x, y) in [(0., 0.), (1., 1.), (f64::INFINITY, 0.), (f64::NAN, 0.)] {
        g.end_x = g.x + x;
        g.end_y = g.y + y;
        assert!(Basis::glyph(&g, axes).is_none());
    }
    g.end_x = g.x + 8.;
    g.end_y = g.y;
    assert!(Basis::glyph(&g, axes).is_none());
}

fn glyph(x: f64, y: f64, end: f64) -> kanpo_pdf::Glyph {
    kanpo_pdf::Glyph {
        text: "arbitrary source".into(),
        code: 123456,
        cid: 98765,
        font: 0,
        x,
        y,
        end_x: x,
        end_y: end,
        size: 8.,
        vertical: true,
        bbox: [x - 4., y, x + 4., end],
        invisible: false,
        spacing: 0.,
        kern: 0.,
        show: 3,
    }
}
pub(super) fn example(count: usize) -> (Vec<Placed>, kanpo_pdf::Page, Scene) {
    let used = (0..count)
        .map(|i| Placed {
            id: i,
            word: i,
            location: (0, 0, None),
            baseline: 10.,
            center: 24. + i as f64 * 4.,
            bounds: [6., 20. + i as f64 * 4., 14., 28. + i as f64 * 4.],
        })
        .collect::<Vec<_>>();
    let rect = [15., 20., 15.5, 28. + count as f64 * 4.];
    let mut glyphs = used
        .iter()
        .map(|p| glyph(p.baseline, p.bounds[1], p.bounds[3]))
        .collect::<Vec<_>>();
    glyphs.push(glyph(15.25, rect[1], rect[3]));
    let page = kanpo_pdf::Page {
        width: 100.,
        height: 100.,
        rotate: 0,
        glyphs,
        fonts: vec![],
        paths: vec![],
    };
    let scene = Scene {
        glyphs: count + 1,
        rectangle_ids: vec![count],
        rectangles: vec![rect],
        rulings: vec![rect],
        unknown: vec![],
        images: vec![],
        lines: vec![],
        fills: vec![],
    };
    (used, page, scene)
}

fn inline_example(
    count: usize,
    side: [i8; 2],
    flow: [i8; 2],
    scale: f64,
    shift: f64,
) -> (
    BTreeMap<Basis, Vec<Placed>>,
    BTreeMap<usize, Vec<usize>>,
    kanpo_pdf::Page,
    Scene,
) {
    let (_, mut page, mut scene) = example(2);
    page.glyphs = vec![glyph(10., 20., 28.), glyph(10., 36., 44.)];
    for i in 0..count {
        let left = 20. * i as f64 / count as f64;
        let right = 20. * (i + 1) as f64 / count as f64;
        let mut g = glyph(left, 32., 32.);
        g.vertical = false;
        g.end_x = right;
        g.bbox = [left, 28., right, 36.];
        page.glyphs.push(g);
    }
    let rule = count + 2;
    page.glyphs.push(glyph(21.25, 20., 44.));
    let physical = |p: [f64; 2]| {
        [
            shift + scale * (side[0] as f64 * p[0] + flow[0] as f64 * p[1]),
            shift + scale * (side[1] as f64 * p[0] + flow[1] as f64 * p[1]),
        ]
    };
    let transform = |b: Rect| {
        let a = physical([b[0], b[1]]);
        let c = physical([b[2], b[3]]);
        [
            a[0].min(c[0]),
            a[1].min(c[1]),
            a[0].max(c[0]),
            a[1].max(c[1]),
        ]
    };
    for g in &mut page.glyphs {
        let a = physical([g.x, g.y]);
        let b = physical([g.end_x, g.end_y]);
        g.x = a[0];
        g.y = a[1];
        g.end_x = b[0];
        g.end_y = b[1];
        g.bbox = transform(g.bbox);
    }
    let mut groups = BTreeMap::<Basis, Vec<Placed>>::new();
    for (id, g) in page.glyphs.iter().enumerate().take(rule) {
        let axes = kanpo_pdf::SourceTextAxes {
            x: side,
            y: flow.map(|n| -n),
        };
        let basis = Basis::glyph(g, axes).unwrap();
        let a = basis.point([g.x, g.y]);
        let b = basis.point([g.end_x, g.end_y]);
        groups.entry(basis).or_default().push(Placed {
            id,
            word: if id < 2 { id } else { 2 },
            location: (0, 0, Some(3)),
            baseline: a[0],
            center: (a[1] + b[1]) / 2.,
            bounds: basis.rect(g.bbox),
        });
    }
    scene.rectangle_ids = vec![rule];
    scene.rectangles = vec![transform([21., 20., 21.5, 44.])];
    scene.rulings = scene.rectangles.clone();
    scene.glyphs = page.glyphs.len();
    let mapping = [(0, vec![0]), (1, vec![1]), (2, (2..rule).collect())]
        .into_iter()
        .collect();
    (groups, mapping, page, scene)
}

#[test]
fn inline_words_use_unique_source_column_in_all_cardinal_rotations_reflections_and_scales() {
    for side in [[1, 0], [-1, 0], [0, 1], [0, -1]] {
        for flow in [[1, 0], [-1, 0], [0, 1], [0, -1]] {
            if side[0] * flow[0] + side[1] * flow[1] != 0 {
                continue;
            }
            for count in [1, 3, 7] {
                for scale in [0.125, 1., 32.] {
                    for shift in [-256., 0., 128.] {
                        let (mut groups, mapping, page, scene) =
                            inline_example(count, side, flow, scale, shift);
                        let original = groups.clone();
                        let additions = inline_placements(&groups, &mapping, &page);
                        assert_eq!(additions.len(), 1);
                        assert_eq!(additions[0].1.len(), count);
                        let parent = additions[0].0;
                        for (basis, placed) in additions {
                            groups.entry(basis).or_default().extend(placed);
                        }
                        // Keep every original font-axis observation as well.
                        for (basis, placed) in original {
                            assert!(placed
                                .iter()
                                .all(|p| groups[&basis].iter().any(|q| q.id == p.id)));
                        }
                        let marked = decorations(&groups[&parent], &page, &parent.scene(&scene));
                        assert_eq!(
                            marked.len(),
                            count + 2,
                            "{side:?} {flow:?} {count} {scale} {shift}"
                        );
                        assert!(marked.values().all(|ids| ids == &[count + 2]));
                    }
                }
            }
        }
    }
}

#[test]
fn inline_parent_requires_empty_adjacent_slot_one_column_and_complete_source_word() {
    for mutation in 0..7 {
        let (mut groups, mapping, mut page, _) = inline_example(3, [1, 0], [0, 1], 1., 0.);
        let parent = Basis {
            side: [1, 0],
            flow: [0, 1],
        };
        let horizontal = Basis {
            side: [0, 1],
            flow: [1, 0],
        };
        match mutation {
            0 => {
                groups.get_mut(&parent).unwrap().remove(1);
            } // missing following body glyph
            1 => {
                groups.get_mut(&parent).unwrap()[1].location = (0, 1, Some(3));
            }
            2 => {
                // A real body glyph already occupies the supposed slot.
                let id = page.glyphs.len();
                page.glyphs.push(glyph(10., 28., 36.));
                groups.get_mut(&parent).unwrap().push(Placed {
                    id,
                    word: 99,
                    location: (0, 0, Some(3)),
                    baseline: 10.,
                    center: 32.,
                    bounds: [6., 28., 14., 36.],
                });
            }
            3 => {
                // Both neighbouring columns geometrically fit: neither is chosen.
                let mut extra = groups[&parent].clone();
                for p in &mut extra {
                    let mut g = page.glyphs[p.id].clone();
                    g.x += 2.;
                    g.end_x += 2.;
                    g.bbox[0] += 2.;
                    g.bbox[2] += 2.;
                    p.id = page.glyphs.len();
                    p.baseline += 2.;
                    p.bounds[0] += 2.;
                    p.bounds[2] += 2.;
                    page.glyphs.push(g);
                }
                groups.get_mut(&parent).unwrap().extend(extra);
            }
            4 => {
                groups.get_mut(&horizontal).unwrap().remove(1);
            } // unmapped source glyph
            5 => {
                page.glyphs[3].vertical = true;
            } // mixed writing modes
            _ => {
                page.glyphs[1].y = 80.;
                page.glyphs[1].end_y = 88.;
            } // distant following body text
        }
        assert!(
            inline_placements(&groups, &mapping, &page).is_empty(),
            "{mutation}"
        );
    }
}

#[test]
fn nearer_body_occludes_foreign_inline_unit_even_when_body_center_precedes_short_rule() {
    let (mut used, mut page, mut scene) = example(2);
    used[0].bounds = [6., 12., 14., 20.];
    used[0].center = 16.;
    used[1].bounds = [6., 20., 14., 28.];
    used[1].center = 24.;
    // The wider object is in another column. Its centre is under the first
    // glyph, whose centre falls outside this particular short rule segment.
    used.push(Placed {
        id: 3,
        word: 3,
        location: (1, 0, None),
        baseline: -10.,
        center: 19.,
        bounds: [-20., 15., -2., 23.],
    });
    page.glyphs.push(glyph(-10., 15., 23.));
    scene.rectangles = vec![[15., 18., 15.5, 26.]];
    scene.rulings = scene.rectangles.clone();
    let marked = decorations(&used, &page, &scene);
    assert_eq!(marked.keys().copied().collect::<Vec<_>>(), vec![1]);
}

#[test]
fn original_rectangle_can_cover_arbitrary_packed_punctuation_without_font_or_cid_names() {
    for count in [1, 2, 11, 37] {
        for scale in [0.125, 1., 64.] {
            for origin in [-256., 0., 128.] {
                let (mut used, mut page, mut scene) = example(count);
                let bounds = |b: Rect| b.map(|n| n * scale + origin);
                for p in &mut used {
                    p.baseline = p.baseline * scale + origin;
                    p.center = p.center * scale + origin;
                    p.bounds = bounds(p.bounds);
                }
                for g in &mut page.glyphs {
                    g.x = g.x * scale + origin;
                    g.end_x = g.end_x * scale + origin;
                    g.y = g.y * scale + origin;
                    g.end_y = g.end_y * scale + origin;
                    g.bbox = bounds(g.bbox);
                }
                scene.rectangles = scene.rectangles.into_iter().map(bounds).collect();
                scene.rulings = scene.rulings.into_iter().map(bounds).collect();
                let marked = decorations(&used, &page, &scene);
                assert_eq!(marked.len(), count);
                assert!(marked.values().all(|v| v == &[count]));
            }
        }
    }
}

#[test]
fn mixed_origins_need_an_overlapping_anchor_in_each_original_word() {
    for scale in [0.125, 1., 64.] {
        for shift in [-256., 0., 128.] {
            let (mut used, mut page, mut scene) = example(3);
            // The shifted glyph is the only part of its Word under this
            // segment. Its unmarked neighbour supplies source row identity.
            used[1].word = 0;
            used[1].baseline += 1.;
            used[1].bounds[0] += 1.;
            used[1].bounds[2] += 1.;
            scene.rectangles = vec![[16., 25., 16.5, 36.]];
            scene.rulings = scene.rectangles.clone();
            let bounds = |b: Rect| b.map(|v| v * scale + shift);
            for p in &mut used {
                p.baseline = p.baseline * scale + shift;
                p.center = p.center * scale + shift;
                p.bounds = bounds(p.bounds);
            }
            for g in &mut page.glyphs { g.bbox = bounds(g.bbox); }
            scene.rectangles = scene.rectangles.into_iter().map(bounds).collect();
            scene.rulings = scene.rulings.into_iter().map(bounds).collect();
            let marked = decorations(&used, &page, &scene);
            assert_eq!(marked.keys().copied().collect::<Vec<_>>(), vec![1, 2]);
            // Independent original Words with different baselines cannot be
            // joined just because the rule passes both centres in one cell.
            let mut separate = used.clone();
            separate[1].word = 1;
            assert!(decorations(&separate, &page, &scene).is_empty());
            let mut foreign = used.clone();
            foreign[1].location = (0, 1, None);
            assert!(decorations(&foreign, &page, &scene).is_empty());
            let mut detached = used.clone();
            detached[0].bounds[0] -= 32. * scale;
            detached[0].bounds[2] -= 32. * scale;
            assert!(decorations(&detached, &page, &scene).is_empty());
        }
    }
}

#[test]
fn grid_junction_unknown_paint_images_and_foreign_overprints_are_not_underlines() {
    let (used, page, mut scene) = example(2);
    scene
        .lines
        .push(([0., 20., 100., 20.], [0., 19.5, 100., 20.5]));
    assert!(decorations(&used, &page, &scene).is_empty());
    let (used, page, mut scene) = example(2);
    scene.unknown.push(999);
    assert!(decorations(&used, &page, &scene).is_empty());
    let (used, page, mut scene) = example(2);
    scene.images.push([6., 20., 16., 100.]);
    assert!(decorations(&used, &page, &scene).is_empty());
    let (mut used, page, scene) = example(2);
    used[1].location = (1, 0, None);
    // Both centres share one decoration. It cannot own two original lines.
    assert!(decorations(&used, &page, &scene).is_empty());
    let (mut used, page, scene) = example(1);
    let mut overprint = used[0].clone();
    overprint.id = 99;
    overprint.bounds = [14., 20., 22., 28.];
    used.push(overprint);
    assert!(decorations(&used, &page, &scene).is_empty());
}

#[test]
fn underline_runs_preserve_nested_tables_all_text_and_existing_styles() {
    let mut runs = vec![
        Run::plain("甲 乙"),
        Run {
            text: String::new(),
            underline: false,
            table: Some(NestedTable {
                rows: vec![vec![vec![Run::plain("丙丁")]]],
                merges: vec![],
            }),
        },
        Run::plain("戊"),
    ];
    runs[2].underline = true;
    let mut before = String::new();
    runs_text(&runs, &mut before);
    let mut cursor = 0;
    style_runs(&mut runs, &[true, false, false, true, false], &mut cursor);
    let mut after = String::new();
    runs_text(&runs, &mut after);
    assert_eq!(before, after);
    assert_eq!(cursor, 5);
    assert!(runs[0].underline);
    assert!(!runs[1].underline);
    let table = runs.iter().find_map(|r| r.table.as_ref()).unwrap();
    assert!(!table.rows[0][0][0].underline);
    assert!(table.rows[0][0][1].underline);
    assert!(runs.last().unwrap().underline);
}

#[test]
fn complete_style_transfer_keeps_unchanged_fields_and_rejects_changed_fields() {
    let text = "改正後\n甲乙\n改正前\n丙丁";
    let mut source = crate::Analysis {
        pages: vec![],
        text: text.into(),
        format: "shinkyu".into(),
        articles: vec![crate::ArticleAnalysis {
            text: text.into(),
            document: crate::Document::from_text(text),
            notices: vec![],
        }],
    };
    let mut target = source.clone();
    let Block::Shinkyu { rows, .. } = &mut source.articles[0].document.blocks[0] else {
        panic!()
    };
    rows[0].after[0].underline = true;
    rows[0].before[0].underline = true;
    let Block::Shinkyu { rows, .. } = &mut target.articles[0].document.blocks[0] else {
        panic!()
    };
    rows[0].before[0].text.push('戊');
    assert!(transfer_field(&source, &mut target, Field::After));
    assert!(!transfer_field(&source, &mut target, Field::Before));
    let Block::Shinkyu { rows, .. } = &target.articles[0].document.blocks[0] else {
        panic!()
    };
    assert!(rows[0].after[0].underline);
    assert!(!rows[0].before[0].underline);
    assert_eq!(rows[0].before[0].text, "丙丁戊");
}
