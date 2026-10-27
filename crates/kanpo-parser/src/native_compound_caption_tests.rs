//! Original columns determine the whole title span; no connective vocabulary or column budget.
use crate::notice_boundaries::cuts;
use crate::trace::{Line, PageTrace, Region, Role, Word, WordStatus};
use std::collections::BTreeSet;
fn put(p: &mut PageTrace, text: &str, bbox: [f32; 4], font: Option<usize>) {
    let id = p.words.len();
    p.words.push(Word {
        source_box: bbox,
        id,
        raw: text.into(),
        text: text.into(),
        x0: bbox[0],
        y0: bbox[1],
        x1: bbox[2],
        y1: bbox[3],
        status: WordStatus::Used,
    });
    p.native_styles
        .push(font.map(|f| BTreeSet::from([(f, 8f64.to_bits())])));
    p.regions[0].lines.push(Line::text(text.into(), vec![id]));
}
fn sample(titles: &[&str]) -> PageTrace {
    let mut p = PageTrace {
        native_comparison_labels: vec![],
        native_horizontal_baselines: vec![],
        native_atomic_columns: vec![],
        native_axes: vec![],
        native_rulings: vec![],
        width: 600.,
        height: 800.,
        layout: "tiers",
        words: vec![],
        native_styles: vec![],
        native_bounds: vec![],
        native_content_styles: vec![],
        regions: vec![Region {
            role: Role::Body,
            unit: 0,
            part: 0,
            lines: vec![],
        }],
        text: String::new(),
        doc_text: String::new(),
        tables: vec![],
    };

    let start = 400. + titles.len() as f32 * 11.;
    p.width = start + 100.;
    for (i, title) in titles.iter().enumerate() {
        let x = start - i as f32 * 11.;
        put(
            &mut p,
            title,
            [x, 50., x + 8., 50. + title.chars().count() as f32 * 8.],
            Some(17),
        );
    }
    let x = start - titles.len() as f32 * 11.;
    put(
        &mut p,
        "本文の手続をお知らせする。",
        [x, 58., x + 8., 250.],
        Some(31),
    );
    put(
        &mut p,
        "2026年7月1日",
        [x - 11., 60., x - 3., 140.],
        Some(31),
    );
    put(
        &mut p,
        "任意の発行主体",
        [x - 22., 110., x - 14., 190.],
        Some(31),
    );
    put(
        &mut p,
        "清算人任意",
        [x - 33., 120., x - 25., 200.],
        Some(31),
    );
    p
}
#[test]
fn a_compound_source_caption_is_one_root_without_connective_vocabulary() {
    for titles in [
        vec!["任意公告、補足公告", "並びに関連公告"],
        vec!["任意の手続に関する", "複数の元列からなる", "関連公告"],
    ] {
        assert_eq!(
            cuts(&sample(&titles), &Default::default()),
            vec![0],
            "{titles:?}"
        );
    }
}
#[test]
fn source_title_run_has_no_column_count_budget() {
    for n in [2, 4, 17, 41] {
        let titles = vec!["任意の関連公告"; n];
        assert_eq!(cuts(&sample(&titles), &Default::default()), vec![0], "{n}");
    }
}
#[test]
fn a_multi_column_caption_needs_its_own_dated_body_and_nominal_signature() {
    for relative in [1, 3] {
        let mut p = sample(&["任意公告、関連公告", "補足公告"]);
        let id = 2 + relative;
        p.words[id].text = "証拠なし".into();
        p.words[id].raw = "証拠なし".into();
        p.regions[0].lines[id].text = "証拠なし".into();
        assert!(cuts(&p, &Default::default()).is_empty());
    }
}
#[test]
fn quoted_and_unclosed_nominal_runs_do_not_authorize_a_publication() {
    for t in [
        vec!["「任意公告、関連公告", "補足公告」"],
        vec!["任意公告（補足", "関連公告"],
        vec!["任意公告について説明する。", "関連公告"],
    ] {
        assert!(cuts(&sample(&t), &Default::default()).is_empty(), "{t:?}");
    }
}
#[test]
fn a_missing_or_conflicting_caption_style_does_not_join_the_columns() {
    for font in [None, Some(99)] {
        let mut p = sample(&["任意の手続に関する", "関連公告"]);
        p.native_styles[0] = font.map(|f| BTreeSet::from([(f, 8f64.to_bits())]));
        assert!(!cuts(&p, &Default::default()).contains(&0));
    }
}
#[test]
fn native_caption_columns_must_align_and_advance_left_in_one_source_region() {
    for delta in [0, 1] {
        let mut p = sample(&["任意公告、関連公告", "補足公告"]);
        let a = p.words[0].source_box;
        let b = if delta == 0 {
            [a[2] + 2., 50., a[2] + 10., 120.]
        } else {
            [a[0] - 11., 70., a[0] - 3., 140.]
        };
        let w = &mut p.words[1];
        w.x0 = b[0];
        w.y0 = b[1];
        w.x1 = b[2];
        w.y1 = b[3];
        w.source_box = b;
        assert!(!cuts(&p, &Default::default()).contains(&0), "{b:?}");
    }
    let mut p = sample(&["任意公告、関連公告", "補足公告"]);
    let rest = p.regions[0].lines.split_off(1);
    p.regions.push(Region {
        role: Role::Body,
        unit: 1,
        part: 0,
        lines: rest,
    });
    assert!(!cuts(&p, &Default::default()).contains(&0));
}
#[test]
fn source_title_run_is_invariant_under_scale_translation_and_font_id_renaming() {
    for k in [0.125, 1., 8.] {
        let mut p = sample(&["任意公告、関連公告", "補足公告"]);
        for w in &mut p.words {
            w.x0 = w.x0 * k - 100.;
            w.x1 = w.x1 * k - 100.;
            w.y0 = w.y0 * k - 200.;
            w.y1 = w.y1 * k - 200.;
            w.source_box = [w.x0, w.y0, w.x1, w.y1];
        }
        for s in &mut p.native_styles {
            if let Some(s) = s {
                *s = s.iter().map(|&(f, b)| (f + 1000, b)).collect();
            }
        }
        assert_eq!(cuts(&p, &Default::default()), vec![0]);
    }
}

fn ignored_marker(p: &mut PageTrace, b: [f32; 4], raw: &str) {
    let id = p.words.len();
    p.words.push(Word {
        id,
        source_box: b,
        x0: b[0],
        y0: b[1],
        x1: b[2],
        y1: b[3],
        raw: raw.into(),
        text: String::new(),
        status: WordStatus::Empty,
    });
    p.native_styles
        .push(Some(BTreeSet::from([(99, 8f64.to_bits())])));
}
#[test]
fn unresolved_original_ink_between_caption_columns_cannot_be_skipped() {
    let mut p = sample(&["任意公告、関連公告", "補足公告"]);
    let a = p.words[0].source_box;
    ignored_marker(&mut p, [a[0] - 2., 60., a[0] - 1., 68.], "\u{e123}");
    assert!(!cuts(&p, &Default::default()).contains(&0));
}
#[test]
fn a_known_ignored_word_outside_the_caption_band_does_not_poison_it() {
    let mut p = sample(&["任意公告、関連公告", "補足公告"]);
    ignored_marker(&mut p, [100., 60., 108., 68.], "\u{e123}");
    assert_eq!(cuts(&p, &Default::default()), vec![0]);
}
#[test]
fn decoded_whitespace_is_not_an_unresolved_caption_obstacle() {
    let mut p = sample(&["任意公告、関連公告", "補足公告"]);
    let a = p.words[0].source_box;
    ignored_marker(&mut p, [a[0] - 2., 60., a[0] - 1., 68.], " \t");
    assert_eq!(cuts(&p, &Default::default()), vec![0]);
}
#[test]
fn unlocated_ignored_glyphs_cannot_use_display_bounds_to_prove_caption_adjacency() {
    let mut p = sample(&["任意公告、関連公告", "補足公告"]);
    ignored_marker(&mut p, [100., 60., 108., 68.], "\u{e123}");
    p.native_bounds = vec![crate::trace::NativeBounds::Compatibility; p.words.len()];
    *p.native_bounds.last_mut().unwrap() = crate::trace::NativeBounds::Unknown;
    assert!(!cuts(&p, &Default::default()).contains(&0));
}
#[test]
fn a_partial_enclosure_on_a_later_caption_column_is_not_a_shared_title() {
    let p = sample(&["任意公告、関連公告", "補足公告"]);
    let b = p.words[1].source_box;
    let mut rules = crate::lines::PageRules::default();
    rules
        .enclosures
        .push([b[0] - 1., b[1] + 8., b[2] + 1., b[3] - 8.]);
    assert!(!cuts(&p, &rules).contains(&0));
}

#[test]
fn a_foreign_owned_word_inside_the_caption_band_blocks_the_run() {
    let mut p = sample(&["任意公告、関連公告", "補足公告"]);
    let a = p.words[0].source_box;
    put(
        &mut p,
        "外部の原語",
        [a[0] - 2., 60., a[0] - 1., 68.],
        Some(99),
    );
    assert!(!cuts(&p, &Default::default()).contains(&0));
}
