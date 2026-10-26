use super::*;
use crate::{
    model::{Document, Run, ShinkyuRow},
    trace::{Line, Region, Unit, Word},
};
#[derive(Clone)]
struct F {
    pages: Vec<PageTrace>,
    articles: Vec<ArticleAnalysis>,
    marked: String,
    owned: Vec<Source>,
}
fn joined(r: &[Run]) -> String {
    r.iter().map(|r| r.text.as_str()).collect()
}
fn rows(a: &[ArticleAnalysis]) -> &[ShinkyuRow] {
    let Block::Shinkyu { rows ,..} = &a[0].document.blocks[0] else {
        panic!()
    };
    rows
}
impl F {
    fn sync(&mut self) {
        let p = &mut self.pages[0];
        let u = Unit::Shinkyu {labels:None,
            preamble: vec![],
            after: vec![p.regions[0].lines.clone()],
            before: vec![p.regions[1].lines.clone()],
            outside: vec![],
        };
        p.text = u.text();
        p.tables.clear();
        p.doc_text = u.doc_text(&mut p.tables, 0);
        self.marked = p.doc_text.clone();
        self.articles[0].text = p.text.clone();
    }
    fn run(&self) -> Vec<ArticleAnalysis> {
        let mut a = self.articles.clone();
        repair(
            &mut a,
            &[OwnedSources::new(&self.marked, self.owned.clone())],
            &self.pages,
        );
        a
    }
    fn unchanged(&self) {
        assert_eq!(self.run()[0].document, self.articles[0].document);
    }
    fn map_text(&mut self, from: &str, to: &str) {
        for p in &mut self.pages {
            for w in &mut p.words {
                w.raw = w.raw.replace(from, to);
                w.text = w.text.replace(from, to);
            }
            for r in &mut p.regions {
                for l in &mut r.lines {
                    l.text = l.text.replace(from, to);
                }
            }
        }
        for b in &mut self.articles[0].document.blocks {
            if let Block::Shinkyu { rows ,..} = b {
                for r in rows {
                    for run in r.after.iter_mut().chain(&mut r.before) {
                        run.text = run.text.replace(from, to);
                    }
                }
            }
        }
        self.sync();
    }
    fn add_line(&mut self, role: Role, text: &str, x: f32, at: usize) -> usize {
        let p = &mut self.pages[0];
        let id = p.words.len();
        let y = if role == Role::After { 40. } else { 400. };
        let b = [x, y, x + 8., y + 100.];
        p.words.push(Word {
            id,
            x0: b[0],
            y0: b[1],
            x1: b[2],
            y1: b[3],
            source_box: b,
            raw: text.into(),
            text: text.into(),
            status: WordStatus::Used,
        });
        p.native_bounds.push(NativeBounds::Glyphs(b));
        p.native_axes.push(NativeAxis::Down);
        p.native_styles.push(None);
        p.native_content_styles.push(ContentStyles::Unknown);
        p.native_rulings
            .push(crate::native_word_rulings::NativeRuling::Unknown);
        let ri = if role == Role::After { 0 } else { 1 };
        p.regions[ri]
            .lines
            .insert(at, Line::text(text.into(), vec![id]));
        id
    }
}
fn fixture() -> F {
    let p = PageTrace {native_comparison_labels:vec![],
        native_horizontal_baselines: vec![],
        native_atomic_columns: vec![],
        native_styles: vec![],
        native_bounds: vec![],
        native_content_styles: vec![],
        native_axes: vec![],
        native_rulings: vec![],
        width: 1000.,
        height: 900.,
        layout: "tiers",
        words: vec![],
        regions: vec![
            Region {
                role: Role::After,
                unit: 0,
                part: 0,
                lines: vec![],
            },
            Region {
                role: Role::Before,
                unit: 0,
                part: 0,
                lines: vec![],
            },
        ],
        text: String::new(),
        doc_text: String::new(),
        tables: vec![],
    };
    let mut f = F {
        pages: vec![p],
        articles: vec![ArticleAnalysis {
            text: String::new(),
            document: Document {
                format: "shinkyu".into(),
                blocks: vec![Block::Shinkyu {labels:None,
                    rows: vec![
                        ShinkyuRow {
                            after: vec![Run::plain("第三条　改正文。")],
                            before: vec![Run::plain("第三条　原文。\n（新設）")],
                            ..Default::default()
                        },
                        ShinkyuRow {
                            after: vec![
                                Run {
                                    text: "第二章　任意の見出し".into(),
                                    underline: true,
                                    table: None,
                                },
                                Run::plain("\n（任意の条見出し）\n第六条　独立の本文。"),
                            ],
                            before: vec![Run {
                                text: "（新設）".into(),
                                underline: true,
                                table: None,
                            }],
                            ..Default::default()
                        },
                    ],
                }],
            },
            notices: vec![],
        }],
        marked: String::new(),
        owned: vec![],
    };
    for (role, text, x, at) in [
        (Role::After, "第三条　改正文。", 430., 0),
        (Role::After, "第二章　任意の見出し", 390., 1),
        (Role::After, "（任意の条見出し）", 370., 2),
        (Role::After, "第六条　独立の本文。", 350., 3),
        (Role::Before, "第三条　原文。", 430., 0),
        (Role::Before, "（新設）", 390., 1),
        (Role::Before, "（新設）", 350., 2),
    ] {
        f.add_line(role, text, x, at);
    }
    f.sync();
    f
}
#[test]
fn outline143_native_column_pairs_split_an_independent_outline_row() {
    let f = fixture();
    let a = f.run();
    let rs = rows(&a);
    assert_eq!(rs.len(), 3);
    assert_eq!(joined(&rs[0].before), "第三条　原文。");
    assert_eq!(joined(&rs[1].after), "第二章　任意の見出し");
    assert_eq!(joined(&rs[1].before), "（新設）");
    assert_eq!(
        joined(&rs[2].after),
        "（任意の条見出し）\n第六条　独立の本文。"
    );
    assert_eq!(joined(&rs[2].before), "（新設）");
    assert!(rs[1].after[0].underline);
    assert!(!rs[1].before[0].underline);
    assert!(rs[2].before[0].underline);
    assert_eq!(a[0].text, f.articles[0].text);
}
#[test]
fn outline143_outline_numbers_titles_and_marker_spelling_are_not_dispatch_keys() {
    let mut f = fixture();
    f.map_text("第二章", "第107款の二");
    f.map_text("任意の見出し", "全く異なる長い見出し");
    f.map_text("第六条", "第九十八条の三");
    f.map_text("（新設）", "［新設］");
    assert_eq!(rows(&f.run()).len(), 3);
}
#[test]
fn outline143_scale_translation_and_distinct_intersecting_column_origins_keep_the_pair() {
    for (scale, dx, dy) in [(0.25, 17., 29.), (3., 100., 50.)] {
        let mut f = fixture();
        let p = &mut f.pages[0];
        for (i, w) in p.words.iter_mut().enumerate() {
            let off = if i == 6 { 0.008 } else { 0. };
            let b = [
                (w.x0 + off) * scale + dx,
                w.y0 * scale + dy,
                (w.x1 + off) * scale + dx,
                w.y1 * scale + dy,
            ];
            w.x0 = b[0];
            w.y0 = b[1];
            w.x1 = b[2];
            w.y1 = b[3];
            w.source_box = b;
            p.native_bounds[i] = NativeBounds::Glyphs(b);
        }
        assert_eq!(rows(&f.run()).len(), 3);
    }
}
#[test]
fn outline143_unknown_or_compatibility_geometry_cannot_certify_a_marker() {
    for bad in [NativeBounds::Unknown, NativeBounds::Compatibility] {
        let mut f = fixture();
        f.pages[0].native_bounds[5] = bad;
        f.unchanged();
    }
}
#[test]
fn outline143_unknown_or_opposite_axis_cannot_supply_the_same_column() {
    for bad in [
        NativeAxis::Unknown,
        NativeAxis::Compatibility,
        NativeAxis::Right,
    ] {
        let mut f = fixture();
        f.pages[0].native_axes[5] = bad;
        f.unchanged();
    }
}
#[test]
fn outline143_ambiguous_counterpart_columns_decline() {
    let mut f = fixture();
    f.add_line(Role::Before, "（新設）", 391., 2);
    if let Block::Shinkyu { rows ,..} = &mut f.articles[0].document.blocks[0] {
        rows[0].before[0].text.push_str("\n（新設）");
    }
    f.sync();
    f.unchanged();
}
#[test]
fn outline143_same_marker_text_in_another_source_unit_is_not_a_counterpart() {
    let mut f = fixture();
    f.pages[0].regions[1].unit = 1;
    f.sync();
    f.unchanged();
}
#[test]
fn outline143_another_continuation_part_cannot_supply_a_counterpart() {
    let mut f = fixture();
    f.pages[0].regions[1].part = 1;
    f.sync();
    f.unchanged();
}
#[test]
fn outline143_invalid_source_rectangle_or_nonfinite_native_geometry_declines() {
    for native in [false, true] {
        let mut f = fixture();
        if native {
            if let NativeBounds::Glyphs(b) = &mut f.pages[0].native_bounds[5] {
                b[0] = f32::NAN;
            }
        } else {
            f.pages[0].words[5].source_box[0] = f32::NAN;
        }
        f.unchanged();
    }
}
#[test]
fn outline143_native_line_and_typed_payload_must_bind_completely() {
    let mut f = fixture();
    if let Block::Shinkyu { rows ,..} = &mut f.articles[0].document.blocks[0] {
        rows[0].before[0].text = rows[0].before[0].text.replace("原文", "別文");
    }
    f.unchanged();
}
#[test]
fn outline143_article_owner_must_not_cut_a_source_line() {
    let mut f = fixture();
    f.owned = vec![(0, 1)];
    f.unchanged();
}
#[test]
fn outline143_identical_marks_are_matched_by_source_column_not_text_search() {
    let mut f = fixture();
    f.pages[0].regions[1].lines.swap(1, 2);
    f.sync();
    f.unchanged();
}
#[test]
fn outline143_an_unaligned_first_article_counterpart_declines() {
    let mut f = fixture();
    let w = &mut f.pages[0].words[6];
    w.x0 = 250.;
    w.x1 = 258.;
    w.source_box = [w.x0, w.y0, w.x1, w.y1];
    f.pages[0].native_bounds[6] = NativeBounds::Glyphs(w.source_box);
    f.unchanged();
}
#[test]
fn outline143_missing_or_sentence_like_outline_is_not_a_declaration() {
    for text in ["第二章に規定する見出し", "任意の普通の本文"] {
        let mut f = fixture();
        f.map_text("第二章　任意の見出し", text);
        f.unchanged();
    }
}
fn move_x(f: &mut F, id: usize, x: f32) {
    let p = &mut f.pages[0];
    let w = &mut p.words[id];
    w.x0 = x;
    w.x1 = x + 8.;
    w.source_box = [w.x0, w.y0, w.x1, w.y1];
    p.native_bounds[id] = NativeBounds::Glyphs(w.source_box);
}
#[test]
fn outline143_title_wrapping_has_no_line_count_ceiling() {
    let mut f = fixture();
    for (id, x) in [
        (0, 940.),
        (4, 940.),
        (1, 900.),
        (5, 900.),
        (2, 240.),
        (3, 220.),
        (6, 220.),
    ] {
        move_x(&mut f, id, x);
    }
    let mut text = String::new();
    for i in 0..64 {
        let t = format!("任意の続き{i}");
        f.add_line(Role::After, &t, 890. - 10. * i as f32, 2 + i);
        text.push_str(&t);
    }
    if let Block::Shinkyu { rows ,..} = &mut f.articles[0].document.blocks[0] {
        rows[1].after[0].text.push_str(&text);
    }
    f.sync();
    let a = f.run();
    assert_eq!(rows(&a).len(), 3);
    assert!(joined(&rows(&a)[1].after).ends_with(&text));
}
#[test]
fn outline143_multicolumn_article_caption_is_kept_with_its_article() {
    let mut f = fixture();
    f.pages[0].words[2].text = "（任意の".into();
    f.pages[0].words[2].raw = "（任意の".into();
    f.pages[0].regions[0].lines[2].text = "（任意の".into();
    f.add_line(Role::After, "条見出し）", 360., 3);
    f.sync();
    let a = f.run();
    assert_eq!(rows(&a).len(), 3);
    assert!(joined(&rows(&a)[2].after).starts_with("（任意の条見出し）"));
}
#[test]
fn outline143_an_article_without_caption_still_has_its_own_source_anchor() {
    let mut f = fixture();
    f.pages[0].regions[0].lines.remove(2);
    f.pages[0].words[2].status = WordStatus::Empty;
    if let Block::Shinkyu { rows ,..} = &mut f.articles[0].document.blocks[0] {
        rows[1].after[1].text = rows[1].after[1].text.replace("（任意の条見出し）\n", "");
    }
    f.sync();
    let a = f.run();
    assert_eq!(rows(&a).len(), 3);
    assert_eq!(joined(&rows(&a)[2].after), "第六条　独立の本文。");
}
#[test]
fn outline143_the_visible_outline_may_be_on_either_comparison_side() {
    let mut f = fixture();
    f.pages[0].regions.swap(0, 1);
    f.pages[0].regions[0].role = Role::After;
    f.pages[0].regions[1].role = Role::Before;
    if let Block::Shinkyu { rows ,..} = &mut f.articles[0].document.blocks[0] {
        for r in rows {
            std::mem::swap(&mut r.after, &mut r.before);
        }
    }
    f.sync();
    let a = f.run();
    assert_eq!(rows(&a).len(), 3);
    assert_eq!(joined(&rows(&a)[1].before), "第二章　任意の見出し");
    assert_eq!(joined(&rows(&a)[1].after), "（新設）");
}
#[test]
fn outline143_complete_opaque_neighbor_tables_are_copied_with_their_original_runs() {
    let mut f = fixture();
    for role in [Role::After, Role::Before] {
        let id = f.add_line(role, "保存する表", 470., 0);
        let b = f.pages[0].words[id].source_box;
        let t = crate::trace::Table {
            x0: b[0],
            y0: b[1],
            x1: b[2],
            y1: b[3],
            rows: 1,
            cols: 1,
            cells: vec![crate::trace::TableCell {
                row: 0,
                col: 0,
                rowspan: 1,
                colspan: 1,
                x0: b[0],
                y0: b[1],
                x1: b[2],
                y1: b[3],
                text: "保存する表".into(),
                words: vec![id],
            }],
        };
        let ri = if role == Role::After { 0 } else { 1 };
        let l = &mut f.pages[0].regions[ri].lines[0];
        l.words.clear();
        l.table = Some(t.clone());
        if let Block::Shinkyu { rows ,..} = &mut f.articles[0].document.blocks[0] {
            let runs = if role == Role::After {
                &mut rows[0].after
            } else {
                &mut rows[0].before
            };
            runs.insert(
                0,
                Run {
                    text: String::new(),
                    underline: true,
                    table: Some(t.to_nested()),
                },
            );
        }
    }
    f.sync();
    let a = f.run();
    assert_eq!(rows(&a).len(), 3);
    for role in [Role::After, Role::Before] {
        let get = |a: &[ArticleAnalysis]| {
            let r = &rows(a)[0];
            if role == Role::After {
                r.after[0].clone()
            } else {
                r.before[0].clone()
            }
        };
        assert_eq!(get(&a), get(&f.articles));
    }
}
#[test]
fn outline143_nested_levels_keep_each_of_their_own_native_markers() {
    let mut f = fixture();
    f.add_line(Role::After, "第九節　内側の見出し", 380., 2);
    f.add_line(Role::Before, "（新設）", 380., 2);
    if let Block::Shinkyu { rows ,..} = &mut f.articles[0].document.blocks[0] {
        rows[1]
            .after
            .insert(1, Run::plain("\n第九節　内側の見出し"));
        rows[0].before[0].text.push_str("\n（新設）");
    }
    f.sync();
    let a = f.run();
    assert_eq!(rows(&a).len(), 4);
    assert_eq!(joined(&rows(&a)[1].after), "第二章　任意の見出し");
    assert_eq!(joined(&rows(&a)[2].after), "第九節　内側の見出し");
    for r in &rows(&a)[1..] {
        assert_eq!(joined(&r.before), "（新設）");
    }
}
#[test]
fn outline143_another_change_marker_is_not_an_article_caption() {
    let mut f = fixture();
    f.map_text("（任意の条見出し）", "（新設）");
    f.unchanged();
}
#[test]
fn outline143_rotating_all_native_axes_and_envelopes_keeps_column_pairs() {
    for turn in 1..=3 {
        let mut f = fixture();
        let p = &mut f.pages[0];
        for (id, w) in p.words.iter_mut().enumerate() {
            let [x0, y0, x1, y1] = w.source_box;
            let b = match turn {
                1 => [y0, 1000. - x1, y1, 1000. - x0],
                2 => [1000. - x1, 1000. - y1, 1000. - x0, 1000. - y0],
                _ => [1000. - y1, x0, 1000. - y0, x1],
            };
            w.x0 = b[0];
            w.y0 = b[1];
            w.x1 = b[2];
            w.y1 = b[3];
            w.source_box = b;
            p.native_bounds[id] = NativeBounds::Glyphs(b);
            p.native_axes[id] = match turn {
                1 => NativeAxis::Right,
                2 => NativeAxis::Up,
                _ => NativeAxis::Left,
            };
        }
        assert_eq!(rows(&f.run()).len(), 3);
    }
}
#[test]
fn outline143_every_original_character_style_survives_arbitrary_run_boundaries() {
    let mut f = fixture();
    if let Block::Shinkyu { rows ,..} = &mut f.articles[0].document.blocks[0] {
        for row in rows {
            for rs in [&mut row.after, &mut row.before] {
                let text = joined(rs);
                *rs = text
                    .chars()
                    .enumerate()
                    .map(|(i, c)| Run {
                        text: c.to_string(),
                        underline: i % 2 == 0,
                        table: None,
                    })
                    .collect();
            }
        }
    }
    let expected: Vec<_> = rows(&f.articles)[1]
        .after
        .iter()
        .take_while(|r| r.text != "\n")
        .cloned()
        .collect();
    let a = f.run();
    assert_eq!(rows(&a).len(), 3);
    assert_eq!(rows(&a)[1].after, expected);
}
#[test]
fn outline143_an_already_correct_row_is_unchanged() {
    let mut f = fixture();
    f.articles = f.run();
    assert_eq!(rows(&f.articles).len(), 3);
    f.unchanged();
}
#[test]
fn outline143_duplicate_explicit_owner_words_are_not_a_certificate() {
    let mut f = fixture();
    f.owned = (0..f.pages[0].words.len()).map(|i| (0, i)).collect();
    f.owned.push((0, 1));
    f.unchanged();
}

#[test]
fn outline143_original_projection_and_native_glyph_bounds_are_distinct_contracts() {
    let mut f = fixture();
    for b in &mut f.pages[0].native_bounds {
        if let NativeBounds::Glyphs(b) = b {
            b[0] -= 4.;
            b[2] -= 4.;
            b[1] += 8.;
            b[3] += 8.;
        }
    }
    let a = f.run();
    assert_eq!(rows(&a).len(), 3);
}
fn unrelated_invalid_page() -> PageTrace {
    let mut p = fixture().pages.remove(0);
    p.words.truncate(1);
    p.words[0].text = "別の頁の不完全な本文。".into();
    p.words[0].raw = p.words[0].text.clone();
    p.regions = vec![Region {
        role: Role::Body,
        unit: 0,
        part: 0,
        lines: vec![Line::text(p.words[0].text.clone(), vec![])],
    }];
    p.text = p.words[0].text.clone();
    p.doc_text = p.text.clone();
    p.tables.clear();
    p
}
#[test]
fn outline143_bounded_source_unrelated_invalid_page_does_not_poison_a_complete_unit() {
    let f = fixture();
    let bad = unrelated_invalid_page();
    let other = ArticleAnalysis {
        text: bad.text.clone(),
        document: Document::from_text(&bad.text),
        notices: vec![],
    };
    let mut a = vec![other.clone(), f.articles[0].clone()];
    let scopes = vec![
        OwnedSources::new(&bad.doc_text, vec![]),
        OwnedSources::new(&f.marked, vec![]),
    ];
    repair(&mut a, &scopes, &[bad, f.pages[0].clone()]);
    assert_eq!(a[0].document, other.document);
    assert_eq!(rows(&a[1..]).len(), 3);
    assert_eq!(joined(&rows(&a[1..])[1].before), "（新設）");
}
#[test]
fn outline143_bounded_source_conflicting_explicit_article_owner_declines() {
    let f = fixture();
    let bad = unrelated_invalid_page();
    let mut a = vec![
        ArticleAnalysis {
            text: bad.text.clone(),
            document: Document::from_text(&bad.text),
            notices: vec![],
        },
        f.articles[0].clone(),
    ];
    let scopes = vec![
        OwnedSources::new(&bad.doc_text, vec![(1, 1)]),
        OwnedSources::new(&f.marked, vec![]),
    ];
    repair(&mut a, &scopes, &[bad, f.pages[0].clone()]);
    assert_eq!(a[1].document, f.articles[0].document);
}
#[test]
fn outline143_bounded_source_identical_units_without_an_independent_owner_decline() {
    let f = fixture();
    let bad = unrelated_invalid_page();
    let mut a = vec![
        ArticleAnalysis {
            text: bad.text.clone(),
            document: Document::from_text(&bad.text),
            notices: vec![],
        },
        f.articles[0].clone(),
        f.articles[0].clone(),
    ];
    let scopes = vec![
        OwnedSources::new(&bad.doc_text, vec![]),
        OwnedSources::new(&f.marked, vec![]),
        OwnedSources::new(&f.marked, vec![]),
    ];
    repair(
        &mut a,
        &scopes,
        &[bad, f.pages[0].clone(), f.pages[0].clone()],
    );
    assert_eq!(a[1].document, f.articles[0].document);
    assert_eq!(a[2].document, f.articles[0].document);
}
#[test]
fn outline143_bounded_source_never_crosses_article_cuts_through_a_native_unit() {
    let f = fixture();
    let bad = unrelated_invalid_page();
    let mut a = vec![
        ArticleAnalysis {
            text: bad.text.clone(),
            document: Document::from_text(&bad.text),
            notices: vec![],
        },
        f.articles[0].clone(),
        f.articles[0].clone(),
    ];
    let cut = f.marked.find("改正前").unwrap();
    let scopes = vec![
        OwnedSources::new(&bad.doc_text, vec![]),
        OwnedSources::new(&f.marked[..cut], vec![]),
        OwnedSources::new(&f.marked[cut..], vec![]),
    ];
    repair(&mut a, &scopes, &[bad, f.pages[0].clone()]);
    assert_eq!(a[1].document, f.articles[0].document);
    assert_eq!(a[2].document, f.articles[0].document);
}

#[test]
fn outline143_partial_claims_can_be_completed_only_by_an_independent_native_unit() {
    let f = fixture();
    let mut a = f.articles.clone();
    let scope = OwnedSources::from_partial_claims(&f.marked, vec![(0, 1)]);
    repair(&mut a, &[scope], &f.pages);
    assert_eq!(rows(&a).len(), 3);
    assert_eq!(joined(&rows(&a)[1].before), "（新設）");
}
#[test]
fn outline143_partial_claims_with_an_unknown_word_are_not_valid_evidence() {
    let f = fixture();
    let mut a = f.articles.clone();
    let scope = OwnedSources::from_partial_claims(&f.marked, vec![(0, 10000)]);
    repair(&mut a, &[scope], &f.pages);
    assert_eq!(a[0].document, f.articles[0].document);
}

#[test]
fn outline143_partial_claims_do_not_rehabilitate_unknown_complete_extras() {
    let f = fixture();
    let mut a = f.articles.clone();
    let mut words = (0..f.pages[0].words.len())
        .map(|id| (0, id))
        .collect::<Vec<_>>();
    words.push((0, 10000));
    let scope = OwnedSources::new(&f.marked, words);
    repair(&mut a, &[scope], &f.pages);
    assert_eq!(a[0].document, f.articles[0].document);
}

// Inserted articles are separate legal rows even when their legacy cell and
// the opposite cell each contain several complete printed article anchors.
fn article146_fixture() -> F {
    let mut f = fixture();
    let p = &mut f.pages[0];
    p.words.clear();
    p.native_bounds.clear();
    p.native_axes.clear();
    p.native_styles.clear();
    p.native_content_styles.clear();
    p.native_rulings.clear();
    for r in &mut p.regions {
        r.lines.clear();
    }
    let after = "（前の条見出し）\n第十三条の二　前の本文。\n２　続く本文。\n（次の条見出し）\n第二百条の三　次の本文。\n一　項目。";
    f.articles[0].document.blocks = vec![Block::Shinkyu {labels:None,
        rows: vec![ShinkyuRow {
            after: vec![Run {
                text: after.into(),
                underline: true,
                table: None,
            }],
            before: vec![
                Run::plain("（新設）\n"),
                Run {
                    text: "（新設）".into(),
                    underline: true,
                    table: None,
                },
            ],
            ..Default::default()
        }],
    }];
    for (i, text) in after.lines().enumerate() {
        f.add_line(Role::After, text, 900. - 20. * i as f32, i);
    }
    f.add_line(Role::Before, "（新設）", 880., 0);
    f.add_line(Role::Before, "（新設）", 820., 1);
    f.sync();
    f
}
fn article146_check(f: &F) {
    let a = f.run();
    let rs = rows(&a);
    assert_eq!(rs.len(), 2);
    assert_eq!(
        joined(&rs[0].after),
        "（前の条見出し）\n第十三条の二　前の本文。\n２　続く本文。"
    );
    assert_eq!(
        joined(&rs[1].after),
        "（次の条見出し）\n第二百条の三　次の本文。\n一　項目。"
    );
    assert_eq!(joined(&rs[0].before), "（新設）");
    assert_eq!(joined(&rs[1].before), "（新設）");
    assert!(rs[0].after[0].underline && rs[1].after[0].underline);
    assert!(!rs[0].before[0].underline && rs[1].before[0].underline);
    for side in [true, false] {
        assert_eq!(
            material(&f.articles[0].document, side),
            material(&a[0].document, side)
        );
    }
    assert_eq!(a[0].text, f.articles[0].text);
}
#[test]
fn article146_native_columns_split_a_merged_inserted_article_cell() {
    article146_check(&article146_fixture());
}
#[test]
fn article146_a_continuation_can_have_an_empty_opposite_prefix() {
    let mut f = article146_fixture();
    if let Block::Shinkyu { rows ,..} = &mut f.articles[0].document.blocks[0] {
        rows[0].before.remove(0);
    }
    f.pages[0].regions[1].lines.remove(0);
    let id = 6;
    f.pages[0].words[id].status = WordStatus::Margin;
    f.sync();
    let a = f.run();
    let rs = rows(&a);
    assert_eq!(rs.len(), 2);
    assert!(rs[0].before.is_empty());
    assert_eq!(joined(&rs[1].before), "（新設）");
}
#[test]
fn article146_distinct_numbers_titles_scale_and_translation_keep_the_proof() {
    let mut f = article146_fixture();
    f.map_text("第十三条の二", "第５条の４");
    f.map_text("第二百条の三", "第六百七条の八");
    f.map_text("次の条見出し", "任意の長さに変更した条文の見出し");
    for p in &mut f.pages {
        for (i, w) in p.words.iter_mut().enumerate() {
            let b = w.source_box.map(|v| v * 3. + 71.);
            w.source_box = b;
            w.x0 = b[0];
            w.y0 = b[1];
            w.x1 = b[2];
            w.y1 = b[3];
            p.native_bounds[i] = NativeBounds::Glyphs(b);
        }
    }
    f.sync();
    assert_eq!(rows(&f.run()).len(), 2);
}
#[test]
fn article146_before_side_can_own_inserted_article_columns() {
    let mut f = article146_fixture();
    for r in &mut f.pages[0].regions {
        r.role = if r.role == Role::After {
            Role::Before
        } else {
            Role::After
        };
    }
    f.pages[0].regions.swap(0, 1);
    if let Block::Shinkyu { rows ,..} = &mut f.articles[0].document.blocks[0] {
        let r = &mut rows[0];
        std::mem::swap(&mut r.after, &mut r.before);
    }
    f.sync();
    assert_eq!(rows(&f.run()).len(), 2);
}
#[test]
fn article146_missing_native_bounds_declines() {
    let mut f = article146_fixture();
    f.pages[0].native_bounds[4] = NativeBounds::Unknown;
    f.unchanged();
}
#[test]
fn article146_nonintersecting_printed_marker_declines() {
    let mut f = article146_fixture();
    f.pages[0].native_bounds[7] = NativeBounds::Glyphs([810., 400., 818., 500.]);
    f.unchanged();
}
#[test]
fn article146_ambiguous_opposite_marker_declines() {
    let mut f = article146_fixture();
    f.add_line(Role::Before, "（新設）", 821., 2);
    if let Block::Shinkyu { rows ,..} = &mut f.articles[0].document.blocks[0] {
        rows[0].before.push(Run::plain("\n（新設）"));
    }
    f.sync();
    f.unchanged();
}
#[test]
fn article146_markers_from_another_unit_decline() {
    let mut f = article146_fixture();
    f.pages[0].regions[1].unit = 1;
    f.unchanged();
}
#[test]
fn article146_arbitrary_opposite_text_is_not_an_insertion_symbol() {
    let mut f = article146_fixture();
    f.map_text("（新設）", "（参考）");
    f.unchanged();
}
#[test]
fn article146_citation_without_article_number_separator_declines() {
    let mut f = article146_fixture();
    f.map_text("第二百条の三　", "第二百条の三の規定により");
    f.unchanged();
}
#[test]
fn article146_malformed_caption_does_not_stay_with_the_previous_article() {
    let mut f = article146_fixture();
    f.map_text("（次の条見出し）", "（次の条見出し");
    f.unchanged();
}
#[test]
fn article146_duplicate_explicit_word_claims_decline() {
    let mut f = article146_fixture();
    f.owned = vec![(0, 4), (0, 4)];
    f.unchanged();
}
#[test]
fn article146_no_caption_is_required_by_the_article_column_proof() {
    let mut f = article146_fixture();
    f.map_text("（次の条見出し）\n", "");
    f.pages[0].regions[0].lines.remove(3);
    f.pages[0].words[3].status = WordStatus::Margin;
    f.sync();
    let a = f.run();
    let rs = rows(&a);
    assert_eq!(rs.len(), 2);
    assert!(joined(&rs[1].after).starts_with("第二百条の三　"));
}
#[test]
fn article146_complete_wrapped_nested_caption_moves_with_its_article() {
    let mut f = article146_fixture();
    f.map_text("（次の条見出し）", "（次の（内側）");
    f.add_line(Role::After, "条見出し）", 830., 4);
    if let Block::Shinkyu { rows ,..} = &mut f.articles[0].document.blocks[0] {
        rows[0].after[0].text = rows[0].after[0]
            .text
            .replace("（次の（内側）\n", "（次の（内側）\n条見出し）\n");
    }
    f.sync();
    let a = f.run();
    let rs = rows(&a);
    assert_eq!(rs.len(), 2);
    assert!(joined(&rs[1].after).starts_with("（次の（内側）\n条見出し）\n第二百条の三　"));
}
fn article146_add_table(f: &mut F, x: f32, line_at: usize, trailing: bool) {
    let id = f.add_line(Role::After, "保存する表", x, line_at);
    let b = f.pages[0].words[id].source_box;
    let table = crate::trace::Table {
        x0: b[0],
        y0: b[1],
        x1: b[2],
        y1: b[3],
        rows: 1,
        cols: 1,
        cells: vec![crate::trace::TableCell {
            row: 0,
            col: 0,
            rowspan: 1,
            colspan: 1,
            x0: b[0],
            y0: b[1],
            x1: b[2],
            y1: b[3],
            text: "保存する表".into(),
            words: vec![id],
        }],
    };
    let l = &mut f.pages[0].regions[0].lines[line_at];
    l.words.clear();
    l.table = Some(table.clone());
    if let Block::Shinkyu { rows ,..} = &mut f.articles[0].document.blocks[0] {
        let r = &mut rows[0];
        if trailing {
            r.after_table = Some(table.to_nested());
        } else {
            let (a, b) = r.after[0].text.split_once("（次の条見出し）").unwrap();
            let a = a.to_owned();
            let b = format!("（次の条見出し）{b}");
            r.after = vec![
                Run {
                    text: a,
                    underline: true,
                    table: None,
                },
                Run {
                    text: String::new(),
                    underline: true,
                    table: Some(table.to_nested()),
                },
                Run {
                    text: format!("\n{b}"),
                    underline: true,
                    table: None,
                },
            ];
        }
    }
    f.sync();
}
#[test]
fn article146_inline_opaque_table_remains_with_its_original_article() {
    let mut f = article146_fixture();
    article146_add_table(&mut f, 850., 3, false);
    let a = f.run();
    let rs = rows(&a);
    assert_eq!(rs.len(), 2);
    assert!(rs[0].after.iter().any(|r| r.table.is_some() && r.underline));
    assert!(rs[1].after.iter().all(|r| r.table.is_none()));
    for side in [true, false] {
        assert_eq!(
            material(&f.articles[0].document, side),
            material(&a[0].document, side)
        );
    }
}
#[test]
fn article146_trailing_table_field_follows_the_last_article() {
    let mut f = article146_fixture();
    article146_add_table(&mut f, 780., 6, true);
    let a = f.run();
    let rs = rows(&a);
    assert_eq!(rs.len(), 2);
    assert!(rs[0].after_table.is_none());
    assert_eq!(
        rs[1].after_table,
        f.articles[0]
            .document
            .blocks
            .iter()
            .find_map(|b| if let Block::Shinkyu { rows ,..} = b {
                rows[0].after_table.clone()
            } else {
                None
            })
    );
    for side in [true, false] {
        assert_eq!(
            material(&f.articles[0].document, side),
            material(&a[0].document, side)
        );
    }
}
#[test]
fn article146_ascii_and_ideographic_spaces_keep_their_styles_and_positions() {
    let mut f = article146_fixture();
    f.map_text("次の本文。", " 次 の　本文。 ");
    let a = f.run();
    assert_eq!(rows(&a).len(), 2);
    for side in [true, false] {
        assert_eq!(
            material(&f.articles[0].document, side),
            material(&a[0].document, side)
        );
    }
}
#[test]
fn article146_unknown_explicit_source_declines() {
    let mut f = article146_fixture();
    f.owned = vec![(0, 999)];
    f.unchanged();
}
#[test]
fn article146_incomplete_marked_unit_does_not_supply_source_authority() {
    let mut f = article146_fixture();
    f.marked = f.marked.replacen("２　続く本文。\n", "", 1);
    f.unchanged();
}
#[test]
fn article146_marker_intersecting_multiple_article_columns_declines() {
    let mut f = article146_fixture();
    f.pages[0].native_bounds[7] = NativeBounds::Glyphs([799., 400., 829., 500.]);
    f.unchanged();
}
#[test]
fn article146_caption_in_reverse_column_order_declines() {
    let mut f = article146_fixture();
    f.pages[0].native_bounds[3] = NativeBounds::Glyphs([810., 40., 818., 140.]);
    f.unchanged();
}
#[test]
fn article146_partial_collector_claims_can_use_a_complete_independent_unit() {
    let f = article146_fixture();
    let mut a = f.articles.clone();
    repair(
        &mut a,
        &[OwnedSources::from_partial_claims(&f.marked, vec![(0, 0)])],
        &f.pages,
    );
    assert_eq!(rows(&a).len(), 2);
}
fn article146_with_internal_outline() -> F {
    let mut f = article146_fixture();
    f.map_text("（次の条見出し）", "第五節　別の任意見出し");
    f.add_line(Role::After, "（次の条見出し）", 830., 4);
    f.add_line(Role::Before, "（新設）", 840., 1);
    if let Block::Shinkyu { rows ,..} = &mut f.articles[0].document.blocks[0] {
        let r = &mut rows[0];
        r.after[0].text = r.after[0].text.replace(
            "第五節　別の任意見出し\n",
            "第五節　別の任意見出し\n（次の条見出し）\n",
        );
        r.before.insert(1, Run::plain("（新設）\n"));
    }
    f.sync();
    f
}
#[test]
fn article146_an_internal_outline_does_not_borrow_its_previous_article_row() {
    let f = article146_with_internal_outline();
    let a = f.run();
    let rs = rows(&a);
    assert_eq!(rs.len(), 3);
    assert_eq!(joined(&rs[1].after), "第五節　別の任意見出し");
    for r in rs {
        assert_eq!(joined(&r.before), "（新設）");
    }
    for side in [true, false] {
        assert_eq!(
            material(&f.articles[0].document, side),
            material(&a[0].document, side)
        );
    }
}
#[test]
fn article146_an_internal_outline_on_the_before_side_keeps_its_own_marker() {
    let mut f = article146_with_internal_outline();
    for r in &mut f.pages[0].regions {
        r.role = if r.role == Role::After {
            Role::Before
        } else {
            Role::After
        };
    }
    f.pages[0].regions.swap(0, 1);
    if let Block::Shinkyu { rows ,..} = &mut f.articles[0].document.blocks[0] {
        let r = &mut rows[0];
        std::mem::swap(&mut r.after, &mut r.before);
    }
    f.sync();
    let a = f.run();
    let rs = rows(&a);
    assert_eq!(rs.len(), 3);
    assert_eq!(joined(&rs[1].before), "第五節　別の任意見出し");
    for r in rs {
        assert_eq!(joined(&r.after), "（新設）");
    }
}
fn article146_repeated_marker_pages(
    gap: bool,
    reverse: bool,
    foreign: bool,
) -> Vec<ArticleAnalysis> {
    let f = article146_fixture();
    let mut g = article146_fixture();
    g.map_text("前の本文", "別頁の前の本文");
    g.map_text("次の本文", "別頁の次の本文");
    g.map_text("第十三条の二", "第五十条");
    g.map_text("第二百条の三", "第六十条");
    let bad = unrelated_invalid_page();
    let other = ArticleAnalysis {
        text: bad.text.clone(),
        document: Document::from_text(&bad.text),
        notices: vec![],
    };
    let mut a = vec![f.articles[0].clone(), other];
    if let Block::Shinkyu { rows: r ,..} = &mut a[0].document.blocks[0] {
        r.extend(rows(&g.articles).iter().cloned());
    }
    let sep = if gap {
        "\n証明されない別の本文\n"
    } else {
        "\n"
    };
    let marked = if reverse {
        format!("{}{}{}", g.marked, sep, f.marked)
    } else {
        format!("{}{}{}", f.marked, sep, g.marked)
    };
    let scope = OwnedSources::from_partial_claims(&marked, vec![(0, 0)]);
    let mut other_scope = OwnedSources::new(&bad.doc_text, vec![]);
    if foreign {
        other_scope.words.push((0, 4));
    }
    repair(
        &mut a,
        &[scope, other_scope],
        &[f.pages[0].clone(), g.pages[0].clone(), bad],
    );
    a
}
#[test]
fn article146_repeated_markers_are_bound_by_contiguous_certified_source_units() {
    let a = article146_repeated_marker_pages(false, false, false);
    assert_eq!(rows(&a).len(), 4);
    for r in rows(&a) {
        assert_eq!(joined(&r.before), "（新設）");
    }
}
#[test]
fn article146_a_gap_in_raw_article_scope_does_not_certify_a_chain() {
    assert_eq!(
        rows(&article146_repeated_marker_pages(true, false, false)).len(),
        2
    );
}
#[test]
fn article146_reversed_raw_article_units_do_not_certify_a_chain() {
    assert_eq!(
        rows(&article146_repeated_marker_pages(false, true, false)).len(),
        2
    );
}
#[test]
fn article146_conflicting_article_word_owner_breaks_a_chain() {
    assert_eq!(
        rows(&article146_repeated_marker_pages(false, false, true)).len(),
        2
    );
}
#[test]
fn article146_an_unvalidated_physical_page_cannot_be_skipped_by_a_chain() {
    let f = article146_fixture();
    let mut g = article146_fixture();
    g.map_text("前の本文", "別頁の前の本文");
    g.map_text("次の本文", "別頁の次の本文");
    let bad = unrelated_invalid_page();
    let other = ArticleAnalysis {
        text: bad.text.clone(),
        document: Document::from_text(&bad.text),
        notices: vec![],
    };
    let mut a = vec![f.articles[0].clone(), other];
    if let Block::Shinkyu { rows: r ,..} = &mut a[0].document.blocks[0] {
        r.extend(rows(&g.articles).iter().cloned());
    }
    let marked = format!("{}\n{}", f.marked, g.marked);
    let scope = OwnedSources::from_partial_claims(&marked, vec![]);
    let other = OwnedSources::new(&bad.doc_text, vec![]);
    repair(
        &mut a,
        &[scope, other],
        &[f.pages[0].clone(), bad, g.pages[0].clone()],
    );
    assert_eq!(rows(&a).len(), 2);
}
#[test]
fn article146_an_unrelated_typed_mismatch_does_not_poison_a_complete_neighboring_frame() {
    let f = article146_fixture();
    let mut g = article146_fixture();
    g.map_text("前の本文", "別頁の前の本文");
    g.map_text("次の本文", "別頁の次の本文");
    let mut third = article146_fixture();
    third.map_text("前の本文", "第三頁の前の本文");
    third.map_text("次の本文", "第三頁の次の本文");
    third.map_text("（新設）", "第三十条　独立した原文。");
    let bad = unrelated_invalid_page();
    let other = ArticleAnalysis {
        text: bad.text.clone(),
        document: Document::from_text(&bad.text),
        notices: vec![],
    };
    let mut a = vec![f.articles[0].clone(), other];
    let mut unrelated = rows(&third.articles)[0].clone();
    unrelated.after[0].text = unrelated.after[0]
        .text
        .replace("第三頁の次の本文", "照合しない既存の別出力");
    if let Block::Shinkyu { rows: r ,..} = &mut a[0].document.blocks[0] {
        r.extend(rows(&g.articles).iter().cloned());
        r.push(unrelated.clone());
    }
    let marked = format!("{}\n{}\n{}", f.marked, g.marked, third.marked);
    let scope = OwnedSources::from_partial_claims(&marked, vec![]);
    let other = OwnedSources::new(&bad.doc_text, vec![]);
    repair(
        &mut a,
        &[scope, other],
        &[
            f.pages[0].clone(),
            g.pages[0].clone(),
            third.pages[0].clone(),
            bad,
        ],
    );
    let rs = rows(&a);
    assert_eq!(rs.len(), 5);
    assert_eq!(rs[4], unrelated);
    for r in &rs[..4] {
        assert_eq!(joined(&r.before), "（新設）");
    }
}
