use super::*;
use crate::{
    model::{Block, Document, Run},
    native_word_axes::NativeAxis,
    trace::{
        ContentStyles, Line, NativeBounds, NativeStyles, PageTrace, Region, Role, Word, WordStatus,
    },
};
#[derive(Clone)]
struct Fixture {
    articles: Vec<ArticleAnalysis>,
    sources: Vec<Vec<(usize, usize)>>,
    source_text: Vec<String>,
    pages: Vec<PageTrace>,
    rules: Vec<Option<PageRules>>,
}
impl Fixture {
    fn run(&self) -> Vec<ArticleAnalysis> {
        let mut a = self.articles.clone();
        let mut s = self
            .sources
            .iter()
            .zip(&self.source_text)
            .map(|(words, text)| OwnedSources::new(text, words.clone()))
            .collect::<Vec<_>>();
        repair(&mut a, &mut s, &self.pages, &self.rules);
        assert_eq!(a.len(), s.len());
        a
    }
    fn replace(&mut self, from: &str, to: &str) {
        for source in &mut self.source_text {
            *source = source.replace(from, to);
        }
        for w in &mut self.pages[0].words {
            w.raw = w.raw.replace(from, to);
            w.text = w.text.replace(from, to);
        }
        for l in &mut self.pages[0].regions[0].lines {
            l.text = l.text.replace(from, to);
        }
        for a in &mut self.articles {
            a.text = a.text.replace(from, to);
            for b in &mut a.document.blocks {
                if let Block::Paragraph { runs } = b {
                    for r in runs {
                        r.text = r.text.replace(from, to);
                    }
                }
            }
        }
    }
}
fn fixture() -> Fixture {
    let texts = ["官庁報告", "官庁事項", "任意の標題", "独立した本文。"];
    let rects = [
        [400., 40., 414., 110.],
        [370., 35., 379., 80.],
        [350., 30., 358., 100.],
        [330., 25., 338., 180.],
    ];
    let fonts = [(17, 14.), (17, 9.), (61, 8.), (62, 8.)];
    let words = texts
        .iter()
        .enumerate()
        .map(|(id, t)| {
            let [x0, y0, x1, y1] = rects[id];
            Word {
                id,
                x0,
                y0,
                x1,
                y1,
                source_box: rects[id],
                raw: (*t).into(),
                text: (*t).into(),
                status: WordStatus::Used,
            }
        })
        .collect::<Vec<_>>();
    let styles = fonts
        .iter()
        .map(|&(f, s)| Some(NativeStyles::from([(f, f64::to_bits(s))])))
        .collect::<Vec<_>>();
    let content = styles
        .iter()
        .map(|s| ContentStyles::Glyphs(s.clone().unwrap()))
        .collect();
    let ls = texts
        .iter()
        .enumerate()
        .map(|(i, t)| Line::text((*t).into(), vec![i]))
        .collect::<Vec<_>>();
    let text = texts.join("\n");
    let page = PageTrace {
        width: 800.,
        height: 1000.,
        layout: "tiers",
        words,
        native_styles: styles,
        native_content_styles: content,
        native_bounds: rects.into_iter().map(NativeBounds::Glyphs).collect(),
        native_axes: vec![NativeAxis::Down; 4],
        native_rulings: vec![crate::native_word_rulings::NativeRuling::Unknown; 4],
        regions: vec![Region {
            role: Role::Body,
            unit: 0,
            part: 0,
            lines: ls,
        }],
        text: text.clone(),
        doc_text: text,
        tables: vec![],
    };
    let mut context = Document::from_text(texts[0]);
    if let Block::Paragraph { runs } = &mut context.blocks[0] {
        runs[0].underline = true;
    }
    let articles = vec![
        ArticleAnalysis {
            text: texts[0].into(),
            document: context,
            notices: vec![],
        },
        ArticleAnalysis {
            text: texts[1..].join("\n"),
            document: Document::from_text(&texts[1..].join("\n")),
            notices: vec![],
        },
    ];
    Fixture {
        articles,
        sources: vec![vec![(0, 0)], vec![(0, 1), (0, 2), (0, 3)]],
        source_text: vec![texts[0].into(), texts[1..].join("\n")],
        pages: vec![page],
        rules: vec![Some(PageRules::default())],
    }
}
#[test]
fn a_source_parent_section_context_belongs_to_its_child_field_body() {
    let f = fixture();
    let a = f.run();
    assert_eq!(a.len(), 1);
    assert_eq!(
        a[0].text,
        f.articles
            .iter()
            .map(|a| a.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
}
#[test]
fn existing_runs_styles_paragraphs_and_format_are_copied() {
    let f = fixture();
    let a = f.run();
    assert_eq!(a.len(), 1);
    let blocks = f
        .articles
        .iter()
        .flat_map(|a| a.document.blocks.clone())
        .collect::<Vec<_>>();
    assert_eq!(a[0].document.blocks, blocks);
    assert_eq!(a[0].document.format, f.articles[1].document.format);
}
#[test]
fn a_numbered_root_keeps_the_existing_separate_lookup_contract() {
    let mut f = fixture();
    f.replace("官庁事項", "〇任意省告示第一号");
    assert_eq!(f.run().len(), 2);
    assert_eq!(
        crate::segment_articles("告示\n〇総務省告示第一号\n本文")[1],
        "〇総務省告示第一号\n本文"
    );
}
#[test]
fn unknown_headings_are_not_recognized_from_their_font_alone() {
    let mut f = fixture();
    f.replace("官庁報告", "任意の大見出し");
    assert_eq!(f.run().len(), 2);
}
#[test]
fn a_section_with_its_own_body_is_not_just_context() {
    let mut f = fixture();
    f.articles[0].text.push_str("\n自身の手続を公告します。");
    assert_eq!(f.run().len(), 2);
}
#[test]
fn an_unproven_source_word_owner_cannot_be_filled_by_text_matching() {
    let mut f = fixture();
    f.sources[0].clear();
    let certify = |f: &Fixture| {
        let sources = f
            .sources
            .iter()
            .zip(&f.source_text)
            .map(|(words, text)| OwnedSources::new(text, words.clone()))
            .collect::<Vec<_>>();
        certified(
            &f.articles[0],
            &f.articles[1],
            &sources[0],
            &sources[1],
            &f.pages,
            &f.rules,
            &locations(&f.pages),
            &table_sources(&f.pages),
        )
        .is_some()
    };
    assert!(!certify(&f));
    f = fixture();
    f.sources[1].retain(|&s| s != (0, 1));
    assert!(!certify(&f));
}

#[test]
fn duplicate_source_ownership_does_not_authorize_joining() {
    let mut f = fixture();
    f.sources[0].push((0, 0));
    assert_eq!(f.run().len(), 2);
}
#[test]
fn different_source_regions_cannot_lend_hierarchy() {
    let mut f = fixture();
    let lines = f.pages[0].regions[0].lines.split_off(1);
    let mut other = f.pages[0].regions[0].clone();
    other.lines = lines;
    other.part = 1;
    f.pages[0].regions.push(other);
    assert_eq!(f.run().len(), 2);
}
#[test]
fn unknown_source_flow_does_not_infer_a_heading_from_its_box() {
    let mut f = fixture();
    f.pages[0].native_axes[0] = NativeAxis::Unknown;
    assert_eq!(f.run().len(), 2);
}
#[test]
fn unknown_source_glyph_bounds_do_not_infer_a_heading_from_compatibility_boxes() {
    let mut f = fixture();
    f.pages[0].native_bounds[0] = NativeBounds::Compatibility;
    assert_eq!(f.run().len(), 2);
}
#[test]
fn unknown_or_unrelated_source_fonts_cannot_supply_the_hierarchy() {
    let mut f = fixture();
    f.pages[0].native_content_styles[0] = ContentStyles::Unknown;
    assert_eq!(f.run().len(), 2);
    f = fixture();
    let s = NativeStyles::from([(999, 14f64.to_bits())]);
    f.pages[0].native_styles[0] = Some(s.clone());
    f.pages[0].native_content_styles[0] = ContentStyles::Glyphs(s);
    assert_eq!(f.run().len(), 2);
}
#[test]
fn typography_must_express_a_parent_child_hierarchy() {
    let mut f = fixture();
    let s = NativeStyles::from([(17, 9f64.to_bits())]);
    f.pages[0].native_styles[0] = Some(s.clone());
    f.pages[0].native_content_styles[0] = ContentStyles::Glyphs(s);
    assert_eq!(f.run().len(), 2);
}
#[test]
fn an_incomplete_field_heading_without_its_body_cannot_borrow_later_material() {
    let mut f = fixture();
    f.articles[1].text = "官庁事項".into();
    f.sources[1] = vec![(0, 1)];
    assert_eq!(f.run().len(), 2);
}
#[test]
fn an_unknown_rule_inventory_is_not_an_empty_native_inventory() {
    let mut f = fixture();
    f.rules[0] = None;
    assert_eq!(f.run().len(), 2);
}
#[test]
fn a_closed_source_frame_around_only_one_heading_fences_the_relationship() {
    let mut f = fixture();
    f.rules[0]
        .as_mut()
        .unwrap()
        .enclosures
        .push([390., 20., 420., 130.]);
    assert_eq!(f.run().len(), 2);
}
#[test]
fn a_complete_cross_band_ruling_fences_the_relationship() {
    let mut f = fixture();
    f.rules[0]
        .as_mut()
        .unwrap()
        .vertical
        .push(crate::lines::VRule {
            x: 390.,
            y0: 0.,
            y1: 200.,
        });
    assert_eq!(f.run().len(), 2);
}

#[test]
fn the_child_body_keeps_its_own_explicit_source_axis() {
    for axis in [
        NativeAxis::Right,
        NativeAxis::Left,
        NativeAxis::Up,
        NativeAxis::Down,
    ] {
        let mut f = fixture();
        f.pages[0].native_axes[2] = axis;
        assert_eq!(f.run().len(), 1, "{axis:?}");
    }
}
#[test]
fn a_short_field_label_does_not_need_to_overlap_the_centered_masthead() {
    let mut f = fixture();
    f.replace("官庁事項", "法務");
    f.pages[0].native_bounds[0] = NativeBounds::Glyphs([400., 60., 414., 130.]);
    f.pages[0].native_bounds[1] = NativeBounds::Glyphs([370., 35., 379., 53.]);
    assert_eq!(f.run().len(), 1);
}
#[test]
fn a_full_rule_between_staggered_source_heading_bands_is_a_boundary() {
    let mut f = fixture();
    f.replace("官庁事項", "法務");
    f.pages[0].native_bounds[0] = NativeBounds::Glyphs([400., 60., 414., 130.]);
    f.pages[0].native_bounds[1] = NativeBounds::Glyphs([370., 35., 379., 53.]);
    f.rules[0]
        .as_mut()
        .unwrap()
        .horizontal
        .push(crate::lines::HRule {
            y: 55.,
            x0: 360.,
            x1: 420.,
        });
    assert_eq!(f.run().len(), 2);
}
#[test]
fn a_partial_masthead_border_is_not_a_full_domain_separator() {
    let mut f = fixture();
    f.rules[0]
        .as_mut()
        .unwrap()
        .vertical
        .push(crate::lines::VRule {
            x: 390.,
            y0: 39.,
            y1: 150.,
        });
    assert_eq!(f.run().len(), 1);
}
#[test]
fn a_shared_closed_frame_owns_both_heading_levels() {
    let mut f = fixture();
    f.rules[0]
        .as_mut()
        .unwrap()
        .enclosures
        .push([300., 10., 430., 200.]);
    assert_eq!(f.run().len(), 1);
}
#[test]
fn a_disjoint_closed_frame_does_not_change_the_heading_hierarchy() {
    let mut f = fixture();
    f.rules[0]
        .as_mut()
        .unwrap()
        .enclosures
        .push([600., 500., 700., 700.]);
    assert_eq!(f.run().len(), 1);
}
fn prior_quote(f: &mut Fixture, text: &str) {
    let mut p = f.pages[0].clone();
    p.words.truncate(1);
    p.words[0].text = text.into();
    p.words[0].raw = text.into();
    p.native_styles.truncate(1);
    p.native_content_styles.truncate(1);
    p.native_bounds.truncate(1);
    p.native_axes.truncate(1);
    p.native_rulings.truncate(1);
    p.regions[0].lines = vec![Line::text(text.into(), vec![0])];
    p.doc_text = text.into();
    p.text = text.into();
    f.pages.insert(0, p);
    f.rules.insert(0, None);
    for source in f.sources.iter_mut().flatten() {
        source.0 = 1;
    }
}
#[test]
fn an_external_open_quote_cannot_authorize_a_section() {
    let mut f = fixture();
    prior_quote(&mut f, "別の本文が引用を開始「");
    assert_eq!(f.run().len(), 2);
}
#[test]
fn a_closed_earlier_quote_does_not_own_the_following_section() {
    let mut f = fixture();
    prior_quote(&mut f, "別の本文の引用「終了」");
    assert_eq!(f.run().len(), 1);
}
#[test]
fn unknown_body_flow_cannot_lend_evidence_to_the_field() {
    let mut f = fixture();
    f.pages[0].native_axes[2] = NativeAxis::Compatibility;
    assert_eq!(f.run().len(), 2);
}
fn extend_heading(f: &mut Fixture, parent: bool) {
    let p = &mut f.pages[0];
    let original = if parent { 0 } else { 1 };
    let id = p.words.len();
    let prefix = "官庁";
    let suffix = if parent { "報告" } else { "事項" };
    p.words[original].text = prefix.into();
    p.words[original].raw = prefix.into();
    let mut w = p.words[original].clone();
    w.id = id;
    w.text = suffix.into();
    w.raw = suffix.into();
    let shift = if parent { 15. } else { 10. };
    w.x0 -= shift;
    w.x1 -= shift;
    w.source_box = [w.x0, w.y0, w.x1, w.y1];
    let bounds = NativeBounds::Glyphs(w.source_box);
    p.words.push(w);
    p.native_styles.push(p.native_styles[original].clone());
    p.native_content_styles
        .push(p.native_content_styles[original].clone());
    p.native_bounds.push(bounds);
    p.native_axes.push(NativeAxis::Down);
    p.native_rulings
        .push(crate::native_word_rulings::NativeRuling::Unknown);
    p.regions[0].lines[original].text = prefix.into();
    p.regions[0]
        .lines
        .insert(original + 1, Line::text(suffix.into(), vec![id]));
    f.sources[if parent { 0 } else { 1 }].push((0, id));
    f.articles[if parent { 0 } else { 1 }].text =
        f.articles[if parent { 0 } else { 1 }].text.replace(
            if parent {
                "官庁報告"
            } else {
                "官庁事項"
            },
            &format!("{prefix}\n{suffix}"),
        );
    p.text = p.regions[0]
        .lines
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    p.doc_text = p.text.clone();
    f.source_text[if parent { 0 } else { 1 }] = f.articles[if parent { 0 } else { 1 }].text.clone();
}
#[test]
fn a_wrapped_parent_heading_remains_one_source_heading_without_a_line_cap() {
    let mut f = fixture();
    extend_heading(&mut f, true);
    assert_eq!(f.run().len(), 1);
}
#[test]
fn a_wrapped_child_heading_remains_one_source_field_without_a_line_cap() {
    let mut f = fixture();
    extend_heading(&mut f, false);
    assert_eq!(f.run().len(), 1);
}

#[test]
fn all_existing_field_labels_use_the_same_source_hierarchy() {
    for name in [
        "官庁事項",
        "人事異動",
        "労働",
        "法務",
        "国家試験",
        "公聴会",
        "諸事項",
    ] {
        let mut f = fixture();
        f.replace("官庁事項", name);
        assert_eq!(f.run().len(), 1, "{name}");
    }
}
#[test]
fn native_font_identifiers_are_opaque_values() {
    let mut f = fixture();
    for s in f.pages[0].native_styles.iter_mut().flatten() {
        *s = s.iter().map(|&(font, size)| (font + 50000, size)).collect();
    }
    for s in &mut f.pages[0].native_content_styles {
        if let ContentStyles::Glyphs(s) = s {
            *s = s.iter().map(|&(font, size)| (font + 50000, size)).collect();
        }
    }
    assert_eq!(f.run().len(), 1);
}
#[test]
fn source_translation_scaling_and_rotation_do_not_select_a_page_shape() {
    for axis in [
        NativeAxis::Right,
        NativeAxis::Down,
        NativeAxis::Left,
        NativeAxis::Up,
    ] {
        for (sx, sy, dx, dy) in [
            (0.125, 64., -1000., 3000.),
            (64., 0.125, 2000., -500.),
            (1., 1., 0., 0.),
        ] {
            let mut f = fixture();
            let p = &mut f.pages[0];
            let tr = |b: [f32; 4]| {
                let point = |x: f32, y: f32| {
                    let (x, y) = match axis {
                        NativeAxis::Right => (y, -x),
                        NativeAxis::Down => (x, y),
                        NativeAxis::Left => (-y, x),
                        NativeAxis::Up => (-x, -y),
                        _ => unreachable!(),
                    };
                    (x * sx + dx, y * sy + dy)
                };
                let (a, b0) = point(b[0], b[1]);
                let (c, d) = point(b[2], b[3]);
                [a.min(c), b0.min(d), a.max(c), b0.max(d)]
            };
            for w in &mut p.words {
                let b = tr([w.x0, w.y0, w.x1, w.y1]);
                [w.x0, w.y0, w.x1, w.y1] = b;
                w.source_box = tr(w.source_box);
            }
            for b in &mut p.native_bounds {
                if let NativeBounds::Glyphs(b) = b {
                    *b = tr(*b);
                }
            }
            p.native_axes.fill(axis);
            assert_eq!(f.run().len(), 1, "{axis:?}: {sx},{sy},{dx},{dy}");
        }
    }
}
#[test]
fn native_body_values_and_lengths_are_not_an_authority_switch() {
    for value in ["一".into(), "任意の長い本文".repeat(1000)] {
        let mut f = fixture();
        f.replace("任意の標題", &value);
        assert_eq!(f.run().len(), 1);
    }
}
#[test]
fn an_unknown_child_glyph_box_cannot_borrow_the_legacy_box() {
    let mut f = fixture();
    f.pages[0].native_bounds[1] = NativeBounds::Unknown;
    assert_eq!(f.run().len(), 2);
}
#[test]
fn non_body_source_roles_are_not_section_context() {
    let mut f = fixture();
    f.pages[0].regions[0].role = Role::Outside;
    assert_eq!(f.run().len(), 2);
}
#[test]
fn native_source_text_and_line_ownership_must_agree() {
    let mut f = fixture();
    f.pages[0].words[0].text = "異なる原文".into();
    assert_eq!(f.run().len(), 2);
}
#[test]
fn a_visible_unowned_source_word_between_the_heads_blocks_the_join() {
    let mut f = fixture();
    let p = &mut f.pages[0];
    let id = p.words.len();
    let mut w = p.words[2].clone();
    w.id = id;
    w.raw = "別の可視項目".into();
    w.text = w.raw.clone();
    w.status = WordStatus::Margin;
    [w.x0, w.y0, w.x1, w.y1] = [385., 50., 392., 80.];
    w.source_box = [w.x0, w.y0, w.x1, w.y1];
    p.words.push(w);
    p.native_styles.push(p.native_styles[2].clone());
    p.native_content_styles
        .push(p.native_content_styles[2].clone());
    p.native_bounds
        .push(NativeBounds::Glyphs([385., 50., 392., 80.]));
    p.native_axes.push(NativeAxis::Down);
    p.native_rulings
        .push(crate::native_word_rulings::NativeRuling::Unknown);
    assert_eq!(f.run().len(), 2);
}

#[test]
fn a_boxed_masthead_pair_cannot_borrow_an_outside_body() {
    let mut f = fixture();
    f.rules[0]
        .as_mut()
        .unwrap()
        .enclosures
        .push([360., 10., 430., 200.]);
    assert_eq!(f.run().len(), 2);
}
#[test]
fn a_boxed_heading_and_title_cannot_borrow_the_rest_of_an_outside_body() {
    let mut f = fixture();
    f.rules[0]
        .as_mut()
        .unwrap()
        .enclosures
        .push([340., 10., 430., 200.]);
    assert_eq!(f.run().len(), 2);
}
fn wordless_table(f: &mut Fixture, b: [f32; 4]) {
    use crate::model::NestedTable;
    use crate::trace::{Table, TableCell};
    let table = Table {
        x0: b[0],
        y0: b[1],
        x1: b[2],
        y1: b[3],
        rows: 1,
        cols: 1,
        cells: vec![TableCell {
            row: 0,
            col: 0,
            rowspan: 1,
            colspan: 1,
            x0: b[0],
            y0: b[1],
            x1: b[2],
            y1: b[3],
            text: String::new(),
            words: vec![],
        }],
    };
    let marker = f.pages[0].tables.len();
    f.pages[0].tables.push(table.clone());
    f.pages[0].regions[0].lines.push(Line {
        text: table.render(),
        words: vec![],
        table: Some(table),
    });
    f.source_text[1].push('\n');
    f.source_text[1].push_str(&crate::trace::table_marker(marker));
    f.articles[1].document.blocks.push(Block::Paragraph {
        runs: vec![Run {
            text: String::new(),
            underline: false,
            table: Some(NestedTable {
                rows: vec![vec![vec![]]],
                merges: vec![],
            }),
        }],
    });
}
#[test]
fn a_wordless_table_outside_the_shared_frame_is_still_source_material() {
    let mut f = fixture();
    wordless_table(&mut f, [500., 20., 600., 100.]);
    f.rules[0]
        .as_mut()
        .unwrap()
        .enclosures
        .push([300., 10., 430., 200.]);
    assert_eq!(f.run().len(), 2);
}
#[test]
fn a_whole_wordless_table_inside_the_shared_frame_is_retained() {
    let mut f = fixture();
    wordless_table(&mut f, [310., 20., 320., 100.]);
    f.rules[0]
        .as_mut()
        .unwrap()
        .enclosures
        .push([300., 10., 430., 200.]);
    let a = f.run();
    assert_eq!(a.len(), 1);
    assert_eq!(
        a[0].document.blocks,
        f.articles
            .iter()
            .flat_map(|a| a.document.blocks.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn identical_wordless_table_values_do_not_substitute_for_their_source_ids() {
    let mut f = fixture();
    wordless_table(&mut f, [310., 20., 320., 100.]);
    wordless_table(&mut f, [500., 20., 600., 100.]);
    f.rules[0]
        .as_mut()
        .unwrap()
        .enclosures
        .push([300., 10., 430., 200.]);
    assert_eq!(f.run().len(), 2);
}
#[test]
fn independently_owned_identical_table_values_remain_two_whole_tables() {
    let mut f = fixture();
    wordless_table(&mut f, [310., 20., 320., 100.]);
    wordless_table(&mut f, [305., 110., 315., 150.]);
    f.rules[0]
        .as_mut()
        .unwrap()
        .enclosures
        .push([300., 10., 430., 200.]);
    let a = f.run();
    assert_eq!(a.len(), 1);
    assert_eq!(
        a[0].document.blocks,
        f.articles
            .iter()
            .flat_map(|a| a.document.blocks.clone())
            .collect::<Vec<_>>()
    );
}
#[test]
fn a_duplicate_source_table_marker_is_not_two_independent_tables() {
    let mut f = fixture();
    wordless_table(&mut f, [310., 20., 320., 100.]);
    f.source_text[1].push_str("\n\u{e000}0\u{e001}");
    assert_eq!(f.run().len(), 2);
}
#[test]
fn a_conflicting_native_table_catalog_does_not_fall_back_to_matching_values() {
    let mut f = fixture();
    wordless_table(&mut f, [310., 20., 320., 100.]);
    f.pages[0].tables[0].x0 = 300.;
    assert_eq!(f.run().len(), 2);
}
fn opaque_table(f: &mut Fixture) {
    wordless_table(f, [310., 20., 320., 100.]);
    let p = &mut f.pages[0];
    let id = p.words.len();
    let b = [311., 30., 319., 50.];
    let mut w = p.words[2].clone();
    w.id = id;
    w.raw = "\u{f0000}".into();
    w.text = w.raw.clone();
    [w.x0, w.y0, w.x1, w.y1] = b;
    w.source_box = b;
    p.words.push(w);
    p.native_styles
        .push(Some(NativeStyles::from([(99, 8f64.to_bits())])));
    p.native_content_styles.push(ContentStyles::Unknown);
    p.native_bounds.push(NativeBounds::Compatibility);
    p.native_axes.push(NativeAxis::Unknown);
    p.native_rulings
        .push(crate::native_word_rulings::NativeRuling::Unknown);
    p.tables[0].cells[0].text = "\u{f0000}".into();
    p.tables[0].cells[0].words = vec![id];
    let t = p.tables[0].clone();
    let l = p.regions[0].lines.last_mut().unwrap();
    l.text = t.render();
    l.table = Some(t);
    f.sources[1].push((0, id));
    f.articles[1].text.push_str("\n\u{f0000}");
    let Block::Paragraph { runs } = f.articles[1].document.blocks.last_mut().unwrap() else {
        panic!()
    };
    runs[0].table.as_mut().unwrap().rows[0][0].push(Run::plain("\u{f0000}"));
}
#[test]
fn a_complete_opaque_source_table_keeps_unknown_glyphs_without_lending_header_authority() {
    let mut f = fixture();
    opaque_table(&mut f);
    f.rules[0]
        .as_mut()
        .unwrap()
        .enclosures
        .push([300., 10., 430., 200.]);
    let a = f.run();
    assert_eq!(a.len(), 1);
    assert_eq!(
        a[0].document.blocks,
        f.articles
            .iter()
            .flat_map(|a| a.document.blocks.clone())
            .collect::<Vec<_>>()
    );
}
#[test]
fn an_opaque_table_cannot_hide_its_outside_source_word_box() {
    let mut f = fixture();
    opaque_table(&mut f);
    f.pages[0].words.last_mut().unwrap().source_box = [600., 30., 608., 50.];
    f.rules[0]
        .as_mut()
        .unwrap()
        .enclosures
        .push([300., 10., 430., 200.]);
    assert_eq!(f.run().len(), 2);
}

// Open, parallel strips belong to the complete source masthead only when
// independent visible source material and native table extents are absent.
fn open_masthead(f: &mut Fixture) {
    for x in [394., 396., 418., 420.] {
        f.rules[0]
            .as_mut()
            .unwrap()
            .vertical
            .push(crate::lines::VRule {
                x,
                y0: 20.,
                y1: 200.,
            });
    }
}
fn foreign_masthead_word(f: &mut Fixture) {
    let p = &mut f.pages[0];
    let id = p.words.len();
    let mut w = p.words[2].clone();
    w.id = id;
    w.raw = "別項".into();
    w.text = w.raw.clone();
    w.status = WordStatus::Margin;
    let b = [416., 50., 417., 70.];
    [w.x0, w.y0, w.x1, w.y1] = b;
    w.source_box = b;
    p.words.push(w);
    p.native_styles.push(p.native_styles[2].clone());
    p.native_content_styles
        .push(p.native_content_styles[2].clone());
    p.native_bounds.push(NativeBounds::Glyphs(b));
    p.native_axes.push(NativeAxis::Down);
    p.native_rulings
        .push(crate::native_word_rulings::NativeRuling::Unknown);
}
#[test]
fn the_mastheads_own_open_parallel_strip_is_not_an_article_boundary() {
    let mut f = fixture();
    open_masthead(&mut f);
    assert_eq!(f.run().len(), 1);
}
#[test]
fn one_unpaired_separator_cannot_claim_to_be_a_masthead() {
    let mut f = fixture();
    open_masthead(&mut f);
    f.rules[0]
        .as_mut()
        .unwrap()
        .vertical
        .retain(|r| r.x == 394.);
    assert_eq!(f.run().len(), 2);
}
#[test]
fn an_independent_separator_still_fences_a_ruled_masthead() {
    let mut f = fixture();
    open_masthead(&mut f);
    f.rules[0]
        .as_mut()
        .unwrap()
        .vertical
        .push(crate::lines::VRule {
            x: 390.,
            y0: 0.,
            y1: 210.,
        });
    assert_eq!(f.run().len(), 2);
}
#[test]
fn foreign_text_inside_the_open_strip_blocks_the_masthead_role() {
    let mut f = fixture();
    open_masthead(&mut f);
    foreign_masthead_word(&mut f);
    assert_eq!(f.run().len(), 2);
}
#[test]
fn unknown_ruling_glyphs_inside_the_strip_do_not_become_decoration() {
    let mut f = fixture();
    open_masthead(&mut f);
    foreign_masthead_word(&mut f);
    let p = &mut f.pages[0];
    let w = p.words.last_mut().unwrap();
    w.text.clear();
    w.raw = "─".into();
    w.status = WordStatus::Empty;
    assert_eq!(f.run().len(), 2);
}
#[test]
fn a_wordless_native_table_inside_the_open_strip_is_not_empty_space() {
    let mut f = fixture();
    open_masthead(&mut f);
    wordless_table(&mut f, [415., 45., 417., 70.]);
    assert_eq!(f.run().len(), 2);
}
#[test]
fn a_closed_frame_does_not_turn_into_open_masthead_decoration() {
    let mut f = fixture();
    open_masthead(&mut f);
    f.rules[0]
        .as_mut()
        .unwrap()
        .enclosures
        .push([393., 19., 421., 201.]);
    assert_eq!(f.run().len(), 2);
}
#[test]
fn crossing_source_rules_do_not_prove_an_open_parallel_strip() {
    let mut f = fixture();
    open_masthead(&mut f);
    f.rules[0]
        .as_mut()
        .unwrap()
        .horizontal
        .push(crate::lines::HRule {
            y: 30.,
            x0: 392.,
            x1: 422.,
        });
    assert_eq!(f.run().len(), 2);
}
fn rotate_ruled_fixture(f: &mut Fixture, axis: NativeAxis, sx: f32, sy: f32, dx: f32, dy: f32) {
    let pt = |x: f32, y: f32| {
        let (x, y) = match axis {
            NativeAxis::Right => (y, -x),
            NativeAxis::Down => (x, y),
            NativeAxis::Left => (-y, x),
            NativeAxis::Up => (-x, -y),
            _ => unreachable!(),
        };
        (x * sx + dx, y * sy + dy)
    };
    let tr = |b: Rect| {
        let (a, b0) = pt(b[0], b[1]);
        let (c, d) = pt(b[2], b[3]);
        [a.min(c), b0.min(d), a.max(c), b0.max(d)]
    };
    let p = &mut f.pages[0];
    for w in &mut p.words {
        [w.x0, w.y0, w.x1, w.y1] = tr([w.x0, w.y0, w.x1, w.y1]);
        w.source_box = tr(w.source_box);
    }
    for b in &mut p.native_bounds {
        if let NativeBounds::Glyphs(b) = b {
            *b = tr(*b);
        }
    }
    p.native_axes.fill(axis);
    let r = f.rules[0].as_mut().unwrap();
    let old = std::mem::take(&mut r.vertical);
    for v in old {
        let (a, b) = pt(v.x, v.y0);
        let (c, d) = pt(v.x, v.y1);
        if a == c {
            r.vertical.push(crate::lines::VRule {
                x: a,
                y0: b.min(d),
                y1: b.max(d),
            });
        } else {
            assert_eq!(b, d);
            r.horizontal.push(crate::lines::HRule {
                y: b,
                x0: a.min(c),
                x1: a.max(c),
            });
        }
    }
}
#[test]
fn the_open_masthead_role_is_invariant_under_native_axis_scale_and_translation() {
    for axis in [
        NativeAxis::Right,
        NativeAxis::Down,
        NativeAxis::Left,
        NativeAxis::Up,
    ] {
        for (sx, sy, dx, dy) in [
            (0.125, 64., -1000., 3000.),
            (64., 0.125, 2000., -500.),
            (1., 1., 0., 0.),
        ] {
            let mut f = fixture();
            open_masthead(&mut f);
            rotate_ruled_fixture(&mut f, axis, sx, sy, dx, dy);
            assert_eq!(f.run().len(), 1, "{axis:?} {sx},{sy},{dx},{dy}");
        }
    }
}

#[test]
fn a_masthead_strip_has_no_border_count_threshold() {
    for before in [1, 2, 7] {
        for after in [1, 3, 9] {
            let mut f = fixture();
            for i in 0..before {
                f.rules[0]
                    .as_mut()
                    .unwrap()
                    .vertical
                    .push(crate::lines::VRule {
                        x: 418. + i as f32,
                        y0: 20.,
                        y1: 200.,
                    });
            }
            for i in 0..after {
                f.rules[0]
                    .as_mut()
                    .unwrap()
                    .vertical
                    .push(crate::lines::VRule {
                        x: 399. - i as f32,
                        y0: 20.,
                        y1: 200.,
                    });
            }
            assert_eq!(f.run().len(), 1, "{before} {after}");
        }
    }
}

fn source_shinkyu_before(f: &mut Fixture) {
    let p = &mut f.pages[0];
    let mut regions = vec![];
    let mut prefix = vec![];
    for (role, text) in [
        (Role::After, "反復する原文"),
        (Role::Before, "反復する原文"),
    ] {
        let id = p.words.len();
        let mut w = p.words[2].clone();
        w.id = id;
        w.raw = text.into();
        w.text = text.into();
        let b = [50. + id as f32, 300., 51. + id as f32, 350.];
        [w.x0, w.y0, w.x1, w.y1] = b;
        w.source_box = b;
        p.words.push(w);
        p.native_styles.push(p.native_styles[2].clone());
        p.native_content_styles
            .push(p.native_content_styles[2].clone());
        p.native_bounds.push(NativeBounds::Glyphs(b));
        p.native_axes.push(NativeAxis::Down);
        p.native_rulings
            .push(crate::native_word_rulings::NativeRuling::Unknown);
        regions.push(Region {
            role,
            unit: 0,
            part: 0,
            lines: vec![Line::text(text.into(), vec![id])],
        });
        prefix.push((0, id));
    }
    p.regions[0].unit = 1;
    regions.append(&mut p.regions);
    p.regions = regions;
    let text = format!(
        "改正後\n反復する原文\n改正前\n反復する原文\n{}",
        crate::trace::SHINKYU_END
    );
    f.articles.insert(
        0,
        ArticleAnalysis {
            text: text.clone(),
            document: Document::from_text(&text),
            notices: vec![],
        },
    );
    f.source_text.insert(0, text);
    f.sources.insert(0, prefix);
    p.doc_text = f.source_text.join("\n");
    for source in &mut f.sources {
        source.clear();
    }
}
#[test]
fn source_offsets_survive_synthetic_shinkyu_labels_without_a_text_search() {
    let mut f = fixture();
    source_shinkyu_before(&mut f);
    let a = f.run();
    assert_eq!(a.len(), 2);
    assert_eq!(a[0].text, f.articles[0].text);
    assert_eq!(a[0].document.blocks, f.articles[0].document.blocks);
    assert_eq!(
        a[1].document.blocks,
        f.articles[1..]
            .iter()
            .flat_map(|a| a.document.blocks.clone())
            .collect::<Vec<_>>()
    );
}
#[test]
fn structural_source_projection_and_open_masthead_rules_compose() {
    let mut f = fixture();
    source_shinkyu_before(&mut f);
    open_masthead(&mut f);
    assert_eq!(f.run().len(), 2);
}
#[test]
fn an_inconsistent_whole_document_cannot_supply_missing_source_owners() {
    let mut f = fixture();
    source_shinkyu_before(&mut f);
    f.pages[0].doc_text.push_str("別の本文");
    assert_eq!(f.run().len(), 3);
}
#[test]
fn native_pua_is_expanded_once_and_never_decoded_as_a_source_template() {
    let mut f = fixture();
    f.replace("任意の標題", "任意\u{e003}0\u{e004}本文");
    source_shinkyu_before(&mut f);
    assert_eq!(f.run().len(), 2);
}
#[test]
fn repeated_text_in_separate_source_columns_does_not_select_the_source_owner() {
    let mut f = fixture();
    source_shinkyu_before(&mut f);
    let mut sources = f
        .source_text
        .iter()
        .map(|text| OwnedSources::new(text, vec![]))
        .collect::<Vec<_>>();
    stream::restore(&mut sources, &f.pages);
    assert_eq!(sources[0].words, vec![(0, 4), (0, 5)]);
    assert_eq!(sources[1].words, vec![(0, 0)]);
    assert_eq!(sources[2].words, vec![(0, 1), (0, 2), (0, 3)]);
}
#[test]
fn a_conflicting_preexisting_word_owner_cannot_be_replaced_by_a_fallback() {
    let mut f = fixture();
    source_shinkyu_before(&mut f);
    f.sources[1] = vec![(0, 4)];
    assert_eq!(f.run().len(), 3);
}
#[test]
fn duplicate_original_line_ownership_invalidates_source_projection() {
    let mut f = fixture();
    source_shinkyu_before(&mut f);
    f.pages[0].regions[1].lines[0].words = vec![4];
    assert_eq!(f.run().len(), 3);
}
#[test]
fn malformed_region_role_order_cannot_reconstruct_a_source_unit() {
    let mut f = fixture();
    source_shinkyu_before(&mut f);
    f.pages[0].regions.swap(0, 1);
    assert_eq!(f.run().len(), 3);
}
#[test]
fn an_unrepresented_used_word_invalidates_the_source_stream() {
    let mut f = fixture();
    source_shinkyu_before(&mut f);
    f.pages[0].regions[1].lines.clear();
    assert_eq!(f.run().len(), 3);
}
#[test]
fn whole_line_ownership_cannot_cross_an_article_boundary_in_a_fallback() {
    let mut f = fixture();
    source_shinkyu_before(&mut f);
    f.source_text[1] = "官庁".into();
    f.source_text[2] = format!("報告\n{}", f.source_text[2]);
    f.pages[0].doc_text = f.source_text.join("\n");
    assert_eq!(f.run().len(), 3);
}

#[test]
fn an_existing_child_body_word_between_the_headers_is_not_heading_evidence() {
    let mut f = fixture();
    f.pages[0].native_bounds[2] = NativeBounds::Glyphs([385., 50., 392., 80.]);
    assert_eq!(f.run().len(), 2);
}
#[test]
fn a_whole_wordless_table_between_the_headers_is_not_empty_heading_space() {
    let mut f = fixture();
    wordless_table(&mut f, [382., 50., 395., 80.]);
    assert_eq!(f.run().len(), 2);
}

fn native_root_fixture() -> Fixture {
    let mut f = fixture();
    f.replace("官庁報告", "資料");
    f.replace("官庁事項", "任意の統計題名");
    let style = NativeStyles::from([(5, 8f64.to_bits())]);
    f.pages[0].native_styles[1] = Some(style.clone());
    f.pages[0].native_content_styles[1] = ContentStyles::Glyphs(style);
    open_masthead(&mut f);
    refresh_root_stream(&mut f);
    f
}
fn refresh_root_stream(f: &mut Fixture) {
    let text = f.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    f.pages[0].text = text.clone();
    f.pages[0].doc_text = text;
}
fn native_roots(f: &Fixture) -> Vec<usize> {
    crate::notice_boundaries::cuts(&f.pages[0], f.rules[0].as_ref().unwrap())
}
#[test]
fn an_open_native_primary_masthead_starts_a_section_before_its_own_title() {
    assert_eq!(native_roots(&native_root_fixture()), vec![0]);
}
#[test]
fn a_primary_masthead_and_its_title_do_not_have_to_use_the_same_font() {
    let f = native_root_fixture();
    assert_ne!(f.pages[0].native_styles[0], f.pages[0].native_styles[1]);
    assert_eq!(native_roots(&f), vec![0]);
}
#[test]
fn ordinary_label_words_keep_the_existing_plain_text_segmentation() {
    assert_eq!(
        crate::segment_articles("先行の本文。\n資料\n後続する任意の本文。").len(),
        1
    );
    let mut f = native_root_fixture();
    f.rules[0] = Some(PageRules::default());
    assert!(native_roots(&f).is_empty());
}
#[test]
fn unknown_titles_do_not_gain_a_primary_category_from_font_and_rules_alone() {
    let mut f = native_root_fixture();
    f.replace("資料", "任意の大見出し");
    assert!(native_roots(&f).is_empty());
}
#[test]
fn all_known_category_labels_use_the_same_native_masthead_proof() {
    for label in [
        "告示",
        "公告",
        "公示",
        "法規的告示",
        "その他告示",
        "条約",
        "庁令",
        "訓令",
        "国会事項",
        "官庁報告",
        "皇室事項",
        "叙位・叙勲",
        "資料",
        "官庁事項",
        "人事異動",
        "労働",
        "法務",
        "国家試験",
        "公聴会",
        "諸事項",
    ] {
        let mut f = native_root_fixture();
        f.replace("資料", label);
        assert_eq!(native_roots(&f), vec![0], "{label}");
    }
}
#[test]
fn category_words_with_plain_field_typography_do_not_start_new_sections() {
    for label in [
        "官庁事項",
        "人事異動",
        "労働",
        "法務",
        "国家試験",
        "公聴会",
        "諸事項",
    ] {
        let mut f = native_root_fixture();
        f.replace("資料", label);
        f.rules[0] = Some(PageRules::default());
        assert!(native_roots(&f).is_empty(), "{label}");
    }
}
#[test]
fn an_equal_sized_body_does_not_certify_a_higher_primary_heading() {
    let mut f = native_root_fixture();
    let style = NativeStyles::from([(5, 14f64.to_bits())]);
    f.pages[0].native_styles[1] = Some(style.clone());
    f.pages[0].native_content_styles[1] = ContentStyles::Glyphs(style);
    assert!(native_roots(&f).is_empty());
}
#[test]
fn native_font_zero_is_a_valid_opaque_identifier_for_a_section_masthead() {
    let mut f = native_root_fixture();
    let style = NativeStyles::from([(0, 14f64.to_bits())]);
    f.pages[0].native_styles[0] = Some(style.clone());
    f.pages[0].native_content_styles[0] = ContentStyles::Glyphs(style);
    assert_eq!(native_roots(&f), vec![0]);
}
#[test]
fn an_independent_rule_still_fences_the_section_from_the_next_title() {
    let mut f = native_root_fixture();
    f.rules[0]
        .as_mut()
        .unwrap()
        .vertical
        .push(crate::lines::VRule {
            x: 390.,
            y0: 0.,
            y1: 210.,
        });
    assert!(native_roots(&f).is_empty());
}
#[test]
fn a_closed_frame_is_not_reclassified_as_an_open_primary_masthead() {
    let mut f = native_root_fixture();
    f.rules[0]
        .as_mut()
        .unwrap()
        .enclosures
        .push([393., 19., 421., 201.]);
    assert!(native_roots(&f).is_empty());
}
#[test]
fn foreign_visible_source_inside_the_masthead_strip_prevents_a_section_cut() {
    let mut f = native_root_fixture();
    foreign_masthead_word(&mut f);
    assert!(native_roots(&f).is_empty());
}
#[test]
fn a_wordless_table_inside_the_masthead_strip_is_still_source_material() {
    let mut f = native_root_fixture();
    wordless_table(&mut f, [415., 45., 417., 70.]);
    assert!(native_roots(&f).is_empty());
}
#[test]
fn a_body_word_between_the_primary_heading_and_its_title_prevents_a_cut() {
    let mut f = native_root_fixture();
    f.pages[0].native_bounds[2] = NativeBounds::Glyphs([385., 50., 392., 80.]);
    assert!(native_roots(&f).is_empty());
}
#[test]
fn a_wordless_table_between_the_primary_heading_and_its_title_prevents_a_cut() {
    let mut f = native_root_fixture();
    wordless_table(&mut f, [382., 50., 395., 80.]);
    assert!(native_roots(&f).is_empty());
}
#[test]
fn a_section_root_requires_known_native_glyphs_content_styles_and_axes() {
    for id in [0, 1] {
        let mut f = native_root_fixture();
        f.pages[0].native_bounds[id] = NativeBounds::Compatibility;
        assert!(native_roots(&f).is_empty());
        let mut f = native_root_fixture();
        f.pages[0].native_content_styles[id] = ContentStyles::Unknown;
        assert!(native_roots(&f).is_empty());
        let mut f = native_root_fixture();
        f.pages[0].native_axes[id] = NativeAxis::Unknown;
        assert!(native_roots(&f).is_empty());
    }
}
#[test]
fn an_empty_primary_heading_cannot_borrow_a_following_category_as_body() {
    let mut f = native_root_fixture();
    f.replace("任意の統計題名", "国家試験");
    assert!(native_roots(&f).is_empty());
}
#[test]
fn primary_header_domains_keep_axis_scale_translation_and_border_count_invariance() {
    for axis in [
        NativeAxis::Right,
        NativeAxis::Down,
        NativeAxis::Left,
        NativeAxis::Up,
    ] {
        for (sx, sy, dx, dy) in [
            (0.125, 64., -1000., 3000.),
            (64., 0.125, 2000., -500.),
            (1., 1., 0., 0.),
        ] {
            let mut f = native_root_fixture();
            rotate_ruled_fixture(&mut f, axis, sx, sy, dx, dy);
            assert_eq!(native_roots(&f), vec![0], "{axis:?} {sx},{sy},{dx},{dy}");
        }
    }
}
fn root_prefix(f: &mut Fixture, text: &str) {
    let p = &mut f.pages[0];
    let id = p.words.len();
    let mut w = p.words[3].clone();
    w.id = id;
    w.raw = text.into();
    w.text = text.into();
    let b = [600., 40., 608., 180.];
    [w.x0, w.y0, w.x1, w.y1] = b;
    w.source_box = b;
    p.words.push(w);
    p.native_styles.push(p.native_styles[3].clone());
    p.native_content_styles
        .push(p.native_content_styles[3].clone());
    p.native_bounds.push(NativeBounds::Glyphs(b));
    p.native_axes.push(NativeAxis::Down);
    p.native_rulings
        .push(crate::native_word_rulings::NativeRuling::Unknown);
    p.regions[0]
        .lines
        .insert(0, Line::text(text.into(), vec![id]));
    refresh_root_stream(f);
}
#[test]
fn an_open_preceding_quote_does_not_authorize_a_primary_section() {
    let mut f = native_root_fixture();
    root_prefix(&mut f, "別の本文から引用「");
    assert!(native_roots(&f).is_empty());
}
#[test]
fn a_closed_preceding_quote_does_not_own_the_next_primary_section() {
    let mut f = native_root_fixture();
    root_prefix(&mut f, "別の本文の引用「終了」");
    assert_eq!(native_roots(&f), vec![1]);
}
#[test]
fn an_unmatched_preceding_quote_closure_is_not_valid_section_evidence() {
    let mut f = native_root_fixture();
    root_prefix(&mut f, "先行する未知の引用」");
    assert!(native_roots(&f).is_empty());
}
#[test]
fn primary_root_parts_keep_original_source_offsets_in_the_existing_group_pipeline() {
    let mut f = native_root_fixture();
    root_prefix(&mut f, "先行する別の本文。");
    let original = crate::segment_articles(&f.pages[0].doc_text);
    assert_eq!(original.len(), 1);
    let grouped =
        crate::notice_boundaries::groups(&original, &f.pages[0].doc_text, &f.pages, &f.rules);
    assert_eq!(grouped.len(), 1);
    assert_eq!(grouped[0].len(), 2);
    assert_eq!(grouped[0][0].text, "先行する別の本文。");
    assert_eq!(
        grouped[0][1].text,
        "資料\n任意の統計題名\n任意の標題\n独立した本文。"
    );
    assert_eq!(grouped[0][0].sources, vec![(0, 4)]);
    assert_eq!(grouped[0][1].sources, vec![(0, 0), (0, 1), (0, 2), (0, 3)]);
}

#[test]
fn blank_source_rows_before_a_masthead_do_not_duplicate_its_cut() {
    let mut f = native_root_fixture();
    f.pages[0].regions[0]
        .lines
        .insert(0, Line::text(String::new(), vec![]));
    refresh_root_stream(&mut f);
    assert_eq!(native_roots(&f), vec![1]);
}
#[test]
fn a_wrapped_source_category_has_no_fixed_line_count_boundary() {
    let mut f = native_root_fixture();
    let p = &mut f.pages[0];
    p.words[0].raw = "資".into();
    p.words[0].text = "資".into();
    p.regions[0].lines[0].text = "資".into();
    let id = p.words.len();
    let mut w = p.words[0].clone();
    w.id = id;
    w.raw = "料".into();
    w.text = "料".into();
    let b = [385., 40., 399., 110.];
    [w.x0, w.y0, w.x1, w.y1] = b;
    w.source_box = b;
    p.words.push(w);
    p.native_styles.push(p.native_styles[0].clone());
    p.native_content_styles
        .push(p.native_content_styles[0].clone());
    p.native_bounds.push(NativeBounds::Glyphs(b));
    p.native_axes.push(NativeAxis::Down);
    p.native_rulings
        .push(crate::native_word_rulings::NativeRuling::Unknown);
    p.regions[0]
        .lines
        .insert(1, Line::text("料".into(), vec![id]));
    let rules = f.rules[0].as_mut().unwrap();
    rules.vertical.clear();
    for x in [381., 383., 418., 420.] {
        rules.vertical.push(crate::lines::VRule {
            x,
            y0: 20.,
            y1: 200.,
        });
    }
    refresh_root_stream(&mut f);
    assert_eq!(native_roots(&f), vec![0]);
}

#[test]
fn categorical_root_certification_has_no_border_count_threshold() {
    for before in [1, 2, 7] {
        for after in [1, 3, 9] {
            let mut f = native_root_fixture();
            f.rules[0].as_mut().unwrap().vertical.clear();
            for i in 0..before {
                f.rules[0]
                    .as_mut()
                    .unwrap()
                    .vertical
                    .push(crate::lines::VRule {
                        x: 418. + i as f32,
                        y0: 20.,
                        y1: 200.,
                    });
            }
            for i in 0..after {
                f.rules[0]
                    .as_mut()
                    .unwrap()
                    .vertical
                    .push(crate::lines::VRule {
                        x: 399. - i as f32,
                        y0: 20.,
                        y1: 200.,
                    });
            }
            assert_eq!(native_roots(&f), vec![0], "{before} {after}");
        }
    }
}
#[test]
fn categorical_roots_do_not_branch_on_the_following_title_value_or_length() {
    for title in ["一".into(), "任意の長い標題".repeat(1000)] {
        let mut f = native_root_fixture();
        f.replace("任意の統計題名", &title);
        assert_eq!(native_roots(&f), vec![0]);
    }
}
