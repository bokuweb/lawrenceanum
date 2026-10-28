use super::*;
use crate::{
    native_word_axes::NativeAxis,
    trace::{ContentStyles, NativeBounds, Region, Word},
};
fn add(p: &mut PageTrace, t: &str, b: Rect, fonts: &[usize], size: f64) {
    let id = p.words.len();
    let s: NativeStyles = fonts.iter().map(|&f| (f, size.to_bits())).collect();
    p.words.push(Word {
        id,
        x0: b[0],
        y0: b[1],
        x1: b[2],
        y1: b[3],
        source_box: b,
        raw: t.into(),
        text: t.into(),
        status: WordStatus::Used,
    });
    p.native_styles.push(Some(s.clone()));
    p.native_content_styles.push(ContentStyles::Glyphs(s));
    p.native_bounds.push(NativeBounds::Glyphs(b));
    p.native_axes.push(NativeAxis::Right);
    p.regions[0].lines.push(Line::text(t.into(), vec![id]));
    sync(p);
}
fn sync(p: &mut PageTrace) {
    p.doc_text = p
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    p.text = p.doc_text.clone();
}
fn empty() -> PageTrace {
    PageTrace {native_comparison_labels:vec![],native_horizontal_baselines:vec![],native_atomic_columns:vec![],
        width: 600.,
        height: 800.,
        layout: "horizontal",
        words: vec![],
        native_styles: vec![],
        native_content_styles: vec![],
        native_bounds: vec![],
        native_axes: vec![],
        native_rulings: vec![],
        regions: vec![Region {
            role: Role::Body,
            unit: 0,
            part: 0,
            lines: vec![],
        }],
        text: String::new(),
        doc_text: String::new(),
        tables: vec![],
    }
}
fn fixture() -> PageTrace {
    let mut p = empty();
    add(
        &mut p,
        "前の法令の説明をここで終える。",
        [60., 20., 250., 28.],
        &[0],
        8.,
    );
    add(
        &mut p,
        "◇任意制度の整備に関する法律",
        [60., 50., 260., 58.],
        &[2],
        8.,
    );
    add(
        &mut p,
        "（法律第九百八十七号）（未知管轄）",
        [68., 61., 250., 69.],
        &[0],
        8.,
    );
    add(
        &mut p,
        "１　任意の説明項目",
        [60., 72., 250., 80.],
        &[0],
        8.,
    );
    p
}
fn change(p: &mut PageTrace, i: usize, t: &str) {
    p.words[i].text = t.into();
    p.words[i].raw = t.into();
    p.regions[0].lines[i].text = t.into();
    sync(p);
}
fn legal_cuts(p: &PageTrace) -> Vec<usize> {
    source_lines(p).map_or(vec![], |ls| {
        legal_summaries::starts(p, &Default::default(), &ls)
    })
}
#[test]
fn wrapped_typed_law_root_separates_from_previous_outline() {
    assert_eq!(legal_cuts(&fixture()), vec![1]);
}
#[test]
fn names_numbers_and_issuing_institutions_are_not_lookup_keys() {
    for (name, id) in [
        (
            "◇架空Alphaの手続に関する法律",
            "（法律第１２３４号）（未知庁）",
        ),
        (
            "◇未知機関令",
            "(政令第一万二千三百四十五号)(ForeignDepartment)",
        ),
        ("◇遠隔両国間の協定", "（条約第0000007号）（架空部局）"),
    ] {
        let mut p = fixture();
        change(&mut p, 1, name);
        change(&mut p, 2, id);
        assert_eq!(legal_cuts(&p), vec![1], "{name} {id}");
    }
}
#[test]
fn mixed_title_and_regular_qualifier_fonts_are_source_evidence() {
    let mut p = fixture();
    let s = p.native_styles[1].as_mut().unwrap();
    s.insert((0, 8f64.to_bits()));
    p.native_content_styles[1] = ContentStyles::Glyphs(s.clone());
    assert_eq!(legal_cuts(&p), vec![1]);
}
#[test]
fn an_arbitrarily_long_source_title_has_no_line_or_character_cap() {
    let mut p = empty();
    add(&mut p, "前の記事。", [60., 10., 260., 18.], &[0], 8.);
    for i in 0..24 {
        let t = if i == 0 {
            "◇任意の長い表題"
        } else {
            "さらに折り返した任意の表題"
        };
        add(
            &mut p,
            t,
            [
                60. + if i > 0 { 8. } else { 0. },
                30. + i as f32 * 11.,
                260.,
                38. + i as f32 * 11.,
            ],
            &[2],
            8.,
        );
    }
    add(
        &mut p,
        "（法律第百号）（任意機関）",
        [68., 294., 250., 302.],
        &[0],
        8.,
    );
    add(
        &mut p,
        "本文はここから続く。",
        [68., 305., 260., 313.],
        &[0],
        8.,
    );
    assert_eq!(legal_cuts(&p), vec![1]);
}
#[test]
fn source_scale_translation_and_font_permutation_preserve_the_proof() {
    for scale in [0.125f32, 1., 8.] {
        let mut p = fixture();
        for i in 0..p.words.len() {
            let w = &mut p.words[i];
            let b = w.source_box;
            let b = [
                b[0] * scale - 300.,
                b[1] * scale - 500.,
                b[2] * scale - 300.,
                b[3] * scale - 500.,
            ];
            w.source_box = b;
            w.x0 = b[0];
            w.y0 = b[1];
            w.x1 = b[2];
            w.y1 = b[3];
            p.native_bounds[i] = NativeBounds::Glyphs(b);
            let s: NativeStyles = p.native_styles[i]
                .as_ref()
                .unwrap()
                .iter()
                .map(|&(f, size)| (91 - f, (f64::from_bits(size) * scale as f64).to_bits()))
                .collect();
            p.native_styles[i] = Some(s.clone());
            p.native_content_styles[i] = ContentStyles::Glyphs(s);
        }
        assert_eq!(legal_cuts(&p), vec![1]);
    }
}
#[test]
fn complete_single_line_title_can_start_a_page() {
    let mut p = empty();
    add(
        &mut p,
        "◇独立した法（法律第九号）（未知機関）",
        [60., 50., 260., 58.],
        &[0, 2],
        8.,
    );
    add(&mut p, "１　任意の説明", [60., 61., 260., 69.], &[0], 8.);
    assert_eq!(legal_cuts(&p), vec![0]);
}
#[test]
fn ordinary_outline_numbers_and_legal_references_are_not_roots() {
    for t in [
        "第１　法律の一部改正",
        "１　任意の説明",
        "（法律第百号）",
        "任意制度の整備に関する法律",
        "◇任意の説明",
    ] {
        let mut p = fixture();
        change(&mut p, 1, t);
        change(&mut p, 2, "１　次の説明項目");
        assert!(legal_cuts(&p).is_empty());
    }
}
#[test]
fn zero_malformed_and_untyped_instrument_numbers_are_rejected() {
    for t in [
        "（法律第零号）（機関）",
        "（政令第0号）（機関）",
        "（法律第十二三号）（機関）",
        "（法律第十百号）（機関）",
        "（法律第1.5号）（機関）",
        "（任意第十二号）（機関）",
        "（法律第十二号）を説明する。",
        "（法律第十二号）（）",
        "（法律第十二号〕（機関）",
    ] {
        let mut p = fixture();
        change(&mut p, 2, t);
        assert!(legal_cuts(&p).is_empty(), "{t}");
    }
}
#[test]
fn quotes_prose_and_unclosed_delimiters_cannot_supply_a_root() {
    for t in [
        "「◇任意制度の法律",
        "◇任意制度を説明する。",
        "◇任意制度（閉じない",
    ] {
        let mut p = fixture();
        change(&mut p, 1, t);
        assert!(legal_cuts(&p).is_empty(), "{t}");
    }
}
#[test]
fn missing_native_axes_bounds_or_typography_defer() {
    for i in 1..4 {
        for kind in 0..5 {
            let mut p = fixture();
            match kind {
                0 => p.native_axes[i] = NativeAxis::Unknown,
                1 => p.native_bounds[i] = NativeBounds::Unknown,
                2 => p.native_styles[i] = None,
                3 => p.native_content_styles[i] = ContentStyles::Unknown,
                _ => p.native_bounds[i] = NativeBounds::Compatibility,
            }
            assert!(legal_cuts(&p).is_empty(), "{i} {kind}");
        }
    }
}
#[test]
fn incomplete_used_word_ownership_and_current_text_mismatch_defer() {
    let mut p = fixture();
    p.words[1].status = WordStatus::Noise;
    assert!(legal_cuts(&p).is_empty());
    let mut p = fixture();
    p.regions[0].lines[2].words = vec![1];
    assert!(legal_cuts(&p).is_empty());
    let mut p = fixture();
    p.words[1].text = "別の文字".into();
    assert!(legal_cuts(&p).is_empty());
}
#[test]
fn same_typography_or_an_unrelated_body_font_cannot_prove_emphasis() {
    for font in [0, 7] {
        let mut p = fixture();
        p.native_styles[1] = Some(BTreeSet::from([(font, 8f64.to_bits())]));
        p.native_content_styles[1] = ContentStyles::Glyphs(p.native_styles[1].clone().unwrap());
        if font == 7 {
            p.native_styles[3] = Some(BTreeSet::from([(9, 8f64.to_bits())]));
            p.native_content_styles[3] = ContentStyles::Glyphs(p.native_styles[3].clone().unwrap());
        }
        assert!(legal_cuts(&p).is_empty());
    }
}
#[test]
fn non_body_roles_and_cross_region_witnesses_do_not_authorize() {
    let mut p = fixture();
    p.regions[0].role = Role::Preamble;
    assert!(legal_cuts(&p).is_empty());
    let mut p = fixture();
    let lines = p.regions[0].lines.split_off(3);
    p.regions.push(Region {
        role: Role::Body,
        unit: 1,
        part: 0,
        lines,
    });
    assert!(legal_cuts(&p).is_empty());
}
#[test]
fn source_title_continuations_and_body_stay_in_their_column_and_order() {
    for i in [2, 3] {
        for b in [
            [300., 61., 490., 69.],
            [68., 40., 250., 48.],
            [40., 61., 240., 69.],
        ] {
            let mut p = fixture();
            p.native_bounds[i] = NativeBounds::Glyphs(b);
            assert!(legal_cuts(&p).is_empty(), "{i} {b:?}");
        }
    }
}
#[test]
fn a_title_at_page_end_without_its_body_witness_defers() {
    let mut p = fixture();
    p.regions[0].lines.pop();
    p.words.pop();
    p.native_axes.pop();
    p.native_bounds.pop();
    p.native_styles.pop();
    p.native_content_styles.pop();
    assert!(legal_cuts(&p).is_empty());
}
fn prefixed() -> PageTrace {
    let mut p = empty();
    add(&mut p, "任意の共通見出し", [80., 10., 220., 24.], &[5], 14.);
    add(&mut p, "折り返した見出し", [90., 25., 230., 39.], &[5], 14.);
    add(
        &mut p,
        "◇任意法（法律第九号）（未知機関）",
        [60., 50., 260., 58.],
        &[0, 2],
        8.,
    );
    add(
        &mut p,
        "１　最初の法令の説明",
        [60., 61., 260., 69.],
        &[0],
        8.,
    );
    add(
        &mut p,
        "本文をここで終える。",
        [68., 72., 260., 80.],
        &[0],
        8.,
    );
    add(
        &mut p,
        "◇別の法（法律第十号）（別の機関）",
        [60., 100., 260., 108.],
        &[0, 2],
        8.,
    );
    add(&mut p, "別の本文。", [68., 111., 260., 119.], &[0], 8.);
    p
}
#[test]
fn larger_shared_heading_stays_with_first_law_in_source_order() {
    let p = prefixed();
    assert_eq!(legal_cuts(&p), vec![0, 5]);
    let text = p.doc_text.clone();
    let original = vec![text.clone()];
    let g = groups(&original, &text, &[p], &[Some(Default::default())]);
    assert_eq!(g[0].len(), 2);
    assert!(g[0][0]
        .text
        .starts_with("任意の共通見出し\n折り返した見出し\n◇"));
    assert!(g[0][1].text.starts_with("◇別の法"));
}
#[test]
fn ordinary_previous_body_is_not_reclassified_as_a_shared_heading() {
    let mut p = fixture();
    let s = BTreeSet::from([(7, 14f64.to_bits())]);
    p.native_styles[0] = Some(s.clone());
    p.native_content_styles[0] = ContentStyles::Glyphs(s);
    assert_eq!(legal_cuts(&p), vec![1]);
}
#[test]
fn unknown_adjacent_shared_heading_typography_defers_the_first_root() {
    let mut p = prefixed();
    p.native_styles[1] = None;
    p.native_content_styles[1] = ContentStyles::Unknown;
    assert_eq!(legal_cuts(&p), vec![5]);
}
#[test]
fn a_qualifier_may_wrap_across_arbitrary_original_word_boundaries() {
    let mut p = fixture();
    change(&mut p, 1, "◇任意制度（法律第九");
    change(&mut p, 2, "百八十七号）（未知管轄）");
    p.native_styles[1]
        .as_mut()
        .unwrap()
        .insert((0, 8f64.to_bits()));
    p.native_content_styles[1] = ContentStyles::Glyphs(p.native_styles[1].clone().unwrap());
    assert_eq!(legal_cuts(&p), vec![1]);
}
#[test]
fn the_root_does_not_reach_across_a_later_diamond_for_a_valid_identifier() {
    let mut p = fixture();
    change(&mut p, 1, "◇番号が欠けた法令");
    change(&mut p, 2, "◇別の法令（法律第九号）（未知機関）");
    p.native_bounds[2] = NativeBounds::Glyphs([60., 61., 260., 69.]);
    p.words[2].source_box = [60., 61., 260., 69.];
    p.words[2].x0 = 60.;
    p.words[2].x1 = 260.;
    p.native_styles[2]
        .as_mut()
        .unwrap()
        .insert((2, 8f64.to_bits()));
    p.native_content_styles[2] = ContentStyles::Glyphs(p.native_styles[2].clone().unwrap());
    assert_eq!(legal_cuts(&p), vec![2]);
}

#[test]
fn a_complete_previous_sentence_does_not_require_its_opaque_ordinal_font() {
    let mut p = fixture();
    p.native_content_styles[0] = ContentStyles::Unknown;
    assert_eq!(legal_cuts(&p), vec![1]);
}
#[test]
fn source_hanging_punctuation_is_not_limited_by_the_first_line_glyph_end() {
    for extra in [4., 40., 400.] {
        let mut p = fixture();
        let b = [68., 61., 260. + extra, 69.];
        p.native_bounds[2] = NativeBounds::Glyphs(b);
        p.words[2].source_box = b;
        p.words[2].x1 = b[2];
        assert_eq!(legal_cuts(&p), vec![1]);
    }
}
#[test]
fn inconsistent_hanging_left_margins_do_not_borrow_a_qualifier() {
    let mut p = fixture();
    change(&mut p, 2, "（法律第九号）（長い");
    change(&mut p, 3, "機関名）");
    p.native_bounds[3] = NativeBounds::Glyphs([80., 72., 250., 80.]);
    p.words[3].source_box = [80., 72., 250., 80.];
    p.words[3].x0 = 80.;
    add(&mut p, "１　説明", [60., 83., 260., 91.], &[0], 8.);
    assert!(legal_cuts(&p).is_empty());
}

#[test]
fn actual_vertical_separators_remain_barriers_to_title_and_body() {
    for id in [1, 2, 3] {
        let mut p = fixture();
        let b = if id == 1 {
            [60., 50., 320., 58.]
        } else if id == 2 {
            [68., 61., 320., 69.]
        } else {
            [60., 72., 320., 80.]
        };
        p.native_bounds[id] = NativeBounds::Glyphs(b);
        let mut rules = PageRules::default();
        rules.vertical.push(crate::lines::VRule {
            x: 300.,
            y0: 0.,
            y1: 200.,
        });
        let ls = source_lines(&p).unwrap();
        assert!(legal_summaries::starts(&p, &rules, &ls).is_empty());
    }
}
#[test]
fn a_short_first_title_line_does_not_invent_a_column_boundary() {
    let mut p = fixture();
    let b = [68., 61., 280., 69.];
    p.native_bounds[2] = NativeBounds::Glyphs(b);
    let mut rules = PageRules::default();
    rules.vertical.push(crate::lines::VRule {
        x: 280.,
        y0: 0.,
        y1: 200.,
    });
    let ls = source_lines(&p).unwrap();
    assert_eq!(legal_summaries::starts(&p, &rules, &ls), vec![1]);
}

fn table_line(p: &mut PageTrace, i: usize) {
    let w = &p.words[i];
    let b = w.source_box;
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
            text: w.text.clone(),
            words: vec![i],
        }],
    };
    p.regions[0].lines[i].table = Some(t);
}
#[test]
fn an_existing_table_cell_cannot_supply_a_legal_root() {
    let mut p = fixture();
    table_line(&mut p, 1);
    assert!(legal_cuts(&p).is_empty());
}
#[test]
fn an_existing_table_cannot_supply_the_root_body_typography() {
    let mut p = fixture();
    table_line(&mut p, 3);
    assert!(legal_cuts(&p).is_empty());
}
#[test]
fn a_later_marked_caption_is_never_a_body_witness_for_the_previous_root() {
    let mut p = fixture();
    change(&mut p, 3, "◇別の法（法律第十号）（未知部局）");
    add(&mut p, "後の本文。", [60., 83., 260., 91.], &[0], 8.);
    assert!(legal_cuts(&p).is_empty());
}
#[test]
fn parenthesized_outline_number_with_prose_is_not_a_title_qualifier() {
    let mut p = fixture();
    change(&mut p, 3, "（１）独立した説明項目");
    assert_eq!(legal_cuts(&p), vec![1]);
}

fn physical(b: Rect, axis: NativeAxis) -> Rect {
    match axis {
        NativeAxis::Right => b,
        NativeAxis::Down => [-b[3], b[0], -b[1], b[2]],
        NativeAxis::Left => [-b[2], -b[3], -b[0], -b[1]],
        NativeAxis::Up => [b[1], -b[2], b[3], -b[0]],
        _ => unreachable!(),
    }
}
fn rotate(p: &mut PageTrace, r: &mut PageRules, axis: NativeAxis) {
    for i in 0..p.words.len() {
        let NativeBounds::Glyphs(b) = p.native_bounds[i] else {
            panic!("test source bounds")
        };
        let b = physical(b, axis);
        p.native_bounds[i] = NativeBounds::Glyphs(b);
        p.native_axes[i] = axis;
        let w = &mut p.words[i];
        w.source_box = b;
        w.x0 = b[0];
        w.y0 = b[1];
        w.x1 = b[2];
        w.y1 = b[3];
    }
    let v = std::mem::take(&mut r.vertical);
    for q in v {
        match axis {
            NativeAxis::Right => r.vertical.push(q),
            NativeAxis::Down => r.horizontal.push(crate::lines::HRule {
                y: q.x,
                x0: -q.y1,
                x1: -q.y0,
            }),
            NativeAxis::Left => r.vertical.push(crate::lines::VRule {
                x: -q.x,
                y0: -q.y1,
                y1: -q.y0,
            }),
            NativeAxis::Up => r.horizontal.push(crate::lines::HRule {
                y: -q.x,
                x0: q.y0,
                x1: q.y1,
            }),
            _ => unreachable!(),
        }
    }
}
fn with_rules(p: &PageTrace, r: &PageRules) -> Vec<usize> {
    let l = source_lines(p).unwrap();
    legal_summaries::starts(p, r, &l)
}
const AXES: [NativeAxis; 4] = [
    NativeAxis::Right,
    NativeAxis::Down,
    NativeAxis::Left,
    NativeAxis::Up,
];
#[test]
fn the_same_source_proof_applies_along_each_explicit_native_axis() {
    for axis in AXES {
        let mut p = fixture();
        let mut r = PageRules::default();
        rotate(&mut p, &mut r, axis);
        assert_eq!(with_rules(&p, &r), vec![1], "{axis:?}");
    }
}
#[test]
fn mixed_native_axes_never_form_one_legal_root() {
    for i in 1..4 {
        let mut p = fixture();
        p.native_axes[i] = NativeAxis::Down;
        assert!(legal_cuts(&p).is_empty());
    }
}
#[test]
fn column_barriers_rotate_with_the_source_writing_axis() {
    for axis in AXES {
        let mut p = fixture();
        p.native_bounds[2] = NativeBounds::Glyphs([68., 61., 320., 69.]);
        let mut r = PageRules::default();
        r.vertical.push(crate::lines::VRule {
            x: 300.,
            y0: 0.,
            y1: 200.,
        });
        rotate(&mut p, &mut r, axis);
        assert!(with_rules(&p, &r).is_empty(), "{axis:?}");
    }
}
fn column_wrap() -> (PageTrace, PageRules) {
    let mut p = fixture();
    for (i, b) in [
        (1, [60., 175., 230., 183.]),
        (2, [68., 186., 230., 194.]),
        (3, [260., 10., 430., 18.]),
    ] {
        p.native_bounds[i] = NativeBounds::Glyphs(b);
        let w = &mut p.words[i];
        w.source_box = b;
        w.x0 = b[0];
        w.y0 = b[1];
        w.x1 = b[2];
        w.y1 = b[3];
    }
    let mut r = PageRules::default();
    r.vertical = vec![
        crate::lines::VRule {
            x: 240.,
            y0: 0.,
            y1: 200.,
        },
        crate::lines::VRule {
            x: 440.,
            y0: 0.,
            y1: 200.,
        },
    ];
    (p, r)
}
#[test]
fn a_complete_title_can_own_its_immediately_next_source_column_body() {
    let (p, r) = column_wrap();
    assert_eq!(with_rules(&p, &r), vec![1]);
}
#[test]
fn next_column_proof_uses_all_source_axes_without_a_gap_threshold() {
    for axis in AXES {
        let (mut p, mut r) = column_wrap();
        rotate(&mut p, &mut r, axis);
        assert_eq!(with_rules(&p, &r), vec![1], "{axis:?}");
    }
}
#[test]
fn missing_incomplete_or_skipped_column_boundaries_defer() {
    for kind in 0..3 {
        let (mut p, mut r) = column_wrap();
        match kind {
            0 => r.vertical.clear(),
            1 => {
                p.native_bounds[3] = NativeBounds::Glyphs([460., 10., 630., 18.]);
            }
            _ => {
                r.vertical[0].y0 = 50.;
            }
        }
        assert!(with_rules(&p, &r).is_empty(), "{kind}");
    }
}
#[test]
fn shared_heading_affinity_is_independent_of_the_printed_axis() {
    for axis in AXES {
        let mut p = prefixed();
        let mut r = PageRules::default();
        rotate(&mut p, &mut r, axis);
        assert_eq!(with_rules(&p, &r), vec![0, 5], "{axis:?}");
    }
}
