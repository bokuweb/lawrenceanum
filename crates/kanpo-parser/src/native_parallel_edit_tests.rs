use super::*;
use crate::lines::HRule;
use crate::trace::{NativeStyles, WordStatus};
struct F {
    words: Vec<Word>,
    cols: Vec<Col>,
    bounds: Vec<NativeBounds>,
    axes: Vec<NativeAxis>,
    flows: Vec<Option<AxisFlow>>,
    styles: Vec<ContentStyles>,
    rules: PageRules,
    expected: [Vec<usize>; 2],
}
impl F {
    fn source(&self) -> Source<'_> {
        Source {
            words: &self.words,
            cols: &self.cols,
            bounds: &self.bounds,
            axes: &self.axes,
            flows: &self.flows,
            styles: &self.styles,
            rules: &self.rules,
        }
    }
    fn add(&mut self, field: usize, x: f32, y: f32, text: &str) -> usize {
        let id = self.words.len();
        let b = [x - 4., y, x + 4., y + text.chars().count() as f32 * 8.];
        self.words.push(Word {
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
        self.cols.push(Col {
            id,
            x: b[0],
            y: b[1],
            x1: b[2],
            y1: b[3],
            text: text.into(),
        });
        self.bounds.push(NativeBounds::Glyphs(b));
        self.axes.push(NativeAxis::Down);
        self.styles.push(ContentStyles::Glyphs(NativeStyles::from([(
            91,
            8f64.to_bits(),
        )])));
        self.flows.push(source_flow(b, 8., text.chars().count()));
        assert!(self.flows[id].is_some());
        self.expected[field].push(id);
        id
    }
    fn number(&mut self, field: usize, x: f32, y: f32, label: &str, body: &str) {
        self.add(field, x, y, label);
        self.add(field, x, y + (label.chars().count() as f32 + 1.) * 8., body);
    }
}
fn fixture(body: &str) -> F {
    let h = (body.chars().count() as f32 + 40.) * 8.;
    let mut f = F {
        words: vec![],
        cols: vec![],
        bounds: vec![],
        axes: vec![],
        flows: vec![],
        styles: vec![],
        rules: PageRules {
            horizontal: [10., 10. + h, 10. + 2. * h]
                .into_iter()
                .map(|y| HRule {
                    y,
                    x0: 10.,
                    x1: 210.,
                })
                .collect(),
            vertical: vec![],
            ..Default::default()
        },
        expected: [vec![], vec![]],
    };
    for (field, y) in [(0, 20.), (1, 20. + h)] {
        f.number(
            field,
            190.,
            y,
            if field == 0 { "四十一" } else { "十七" },
            body,
        );
        f.add(
            field,
            150.,
            y,
            if field == 0 {
                "〔四十二〜四十四略〕"
            } else {
                "〔十八〜二十同上〕"
            },
        );
        f.number(
            field,
            110.,
            y,
            if field == 0 { "四十五" } else { "二十一" },
            body,
        );
        if field == 0 {
            f.number(field, 70., y, "四十六", body)
        } else {
            f.add(field, 70., y, "〔号を加える。〕");
        }
    }
    f
}
fn yes(f: F) {
    let old = f
        .words
        .iter()
        .map(|w| (w.id, w.raw.clone(), w.text.clone(), w.source_box))
        .collect::<Vec<_>>();
    let p = prove(f.source())
        .expect("native paired enumeration and typed edit operator must certify their fields");
    assert_eq!(p.after, f.expected[0]);
    assert_eq!(p.before, f.expected[1]);
    assert_eq!(
        old,
        f.words
            .iter()
            .map(|w| (w.id, w.raw.clone(), w.text.clone(), w.source_box))
            .collect::<Vec<_>>()
    );
}
fn no(f: F) {
    assert!(prove(f.source()).is_none());
}
#[test]
fn arbitrary_body_spelling_and_length_do_not_classify_the_fields() {
    for b in ["任意の説明".to_string(), "Ｚα甲乙".repeat(180)] {
        yes(fixture(&b));
    }
}
#[test]
fn ordinary_parallel_numbers_without_an_edit_operator_do_not_supply_roles() {
    let mut f = fixture("普通の説明");
    let id = *f.expected[1].last().unwrap();
    f.words[id].text = "（注記）".into();
    f.words[id].raw = "（注記）".into();
    f.cols[id].text = "（注記）".into();
    no(f);
}
#[test]
fn an_operator_mentioned_inside_body_does_not_become_a_source_cell() {
    let mut f = fixture("引用中の〔号を加える。〕を説明する本文");
    let id = *f.expected[1].last().unwrap();
    f.words[id].text = "この〔号を加える。〕は説明である。".into();
    f.words[id].raw = f.words[id].text.clone();
    f.cols[id].text = f.words[id].text.clone();
    no(f);
}
#[test]
fn unknown_native_axis_or_flow_does_not_certify_roles() {
    for unknown in [true, false] {
        let mut f = fixture("任意の本文");
        if unknown {
            f.axes[0] = NativeAxis::Unknown
        } else {
            f.flows[0] = None
        }
        no(f);
    }
}
#[test]
fn native_ruling_gaps_and_missing_boundaries_are_not_invented() {
    let mut f = fixture("任意の本文");
    f.rules.horizontal.pop();
    no(f);
    let mut f = fixture("任意の本文");
    let rail = f.rules.horizontal[0].clone();
    f.rules.horizontal[0].x1 = 100.;
    f.rules.horizontal.push(HRule {
        y: rail.y,
        x0: 100.01,
        x1: 210.,
    });
    no(f);
}
#[test]
fn exactly_touching_original_rail_fragments_are_one_source_boundary() {
    let mut f = fixture("任意の本文");
    let rail = f.rules.horizontal[0].clone();
    f.rules.horizontal[0].x1 = 100.;
    f.rules.horizontal.push(HRule {
        y: rail.y,
        x0: 100.,
        x1: 210.,
    });
    yes(f);
}
#[test]
fn current_column_text_cannot_borrow_a_different_original_word() {
    let mut f = fixture("任意の本文");
    f.cols[0].text = "別の見出し".into();
    no(f);
}
#[test]
fn duplicate_source_occurrences_are_ambiguous() {
    let mut f = fixture("任意の本文");
    f.cols.push(f.cols[0].clone());
    no(f);
}
#[test]
fn marker_unit_must_agree_with_the_numbered_opposite_field() {
    let mut f = fixture("任意の本文");
    let id = *f.expected[1].last().unwrap();
    f.words[id].text = "〔条を加える。〕".into();
    f.words[id].raw = "〔条を加える。〕".into();
    f.cols[id].text = "〔条を加える。〕".into();
    no(f);
}
#[test]
fn unmatched_physical_entry_columns_cannot_be_filled_from_body_similarity() {
    let mut f = fixture("同じ本文同じ本文");
    let id = *f.expected[1].last().unwrap();
    f.bounds[id] = NativeBounds::Glyphs([20., f.words[id].y0, 28., f.words[id].y1]);
    no(f);
}
#[test]
fn an_edit_operator_cannot_claim_an_additional_prose_column_as_an_empty_cell() {
    let mut f = fixture("任意の本文");
    let y = f.words[*f.expected[1].last().unwrap()].y0;
    f.add(1, 40., y, "これは実際の本文である。");
    no(f);
}
#[test]
fn an_operator_inside_a_continuing_quote_is_not_an_amendment_cell() {
    let mut f = fixture("引用「任意の本文");
    let y = f.words[*f.expected[1].last().unwrap()].y0;
    f.add(1, 40., y, "引用の終わり」");
    no(f);
}
#[test]
fn omitted_active_output_cannot_hide_a_foreign_native_occurrence_in_the_frame() {
    let mut f = fixture("任意の本文");
    let id = f.add(0, 30., 30., "未所属の原字");
    f.words[id].status = WordStatus::Noise;
    f.cols.retain(|c| c.id != id);
    no(f);
}
#[test]
fn ordinary_balanced_quotations_inside_the_body_keep_their_comparison_roles() {
    yes(fixture("任意の「名前」及び『呼称』を持つ本文"));
}
fn source_flow(b: [f32; 4], leading: f32, count: usize) -> Option<AxisFlow> {
    let x = (b[0] + b[2]) / 2.;
    let xml=format!("<word xmin='{}' ymin='{}' xmax='{}' ymax='{}' data-native-xmin='{}' data-native-ymin='{}' data-native-xmax='{}' data-native-ymax='{}' data-wmode='1' data-native-axis='down' data-native-flow-start='{}' data-native-flow-end='{}' data-native-flow-baseline='{}' data-native-flow-leading='{}' data-native-flow-trailing='0' data-native-flow-prefix='0' data-native-flow-first-glyph='0' data-native-flow-after-glyph='{}'>X</word>",b[0],b[1],b[2],b[3],b[0],b[1],b[2],b[3],b[1],b[3],-x,leading,count);
    let h = scraper::Html::parse_fragment(&xml);
    crate::native_column_flow::from_word(
        h.select(&scraper::Selector::parse("word").unwrap())
            .next()
            .unwrap(),
        Some(count),
    )
}
fn rebound(f: &mut F, id: usize, b: [f32; 4], advance: f32) {
    let w = &mut f.words[id];
    (w.x0, w.y0, w.x1, w.y1) = (b[0], b[1], b[2], b[3]);
    w.source_box = b;
    let c = f.cols.iter_mut().find(|c| c.id == id).unwrap();
    (c.x, c.y, c.x1, c.y1) = (b[0], b[1], b[2], b[3]);
    f.bounds[id] = NativeBounds::Glyphs(b);
    f.flows[id] = source_flow(b, advance, w.text.chars().count());
    assert!(f.flows[id].is_some());
}
fn patch(f: &mut F, id: usize, text: &str) {
    let NativeBounds::Glyphs(mut b) = f.bounds[id] else {
        panic!()
    };
    f.words[id].text = text.into();
    f.words[id].raw = text.into();
    f.cols.iter_mut().find(|c| c.id == id).unwrap().text = text.into();
    b[3] = b[1] + text.chars().count() as f32 * 8.;
    rebound(f, id, b, 8.);
}
fn prefix(f: &mut F, id: usize, text: &str) {
    let NativeBounds::Glyphs(old) = f.bounds[id] else {
        panic!()
    };
    let NativeBounds::Glyphs(mut body) = f.bounds[id + 1] else {
        panic!()
    };
    let gap = body[1] - old[3];
    let height = body[3] - body[1];
    patch(f, id, text);
    let NativeBounds::Glyphs(b) = f.bounds[id] else {
        panic!()
    };
    body[1] = b[3] + gap;
    body[3] = body[1] + height;
    rebound(f, id + 1, body, 8.);
}
#[test]
fn physical_upper_before_and_lower_after_have_the_same_role_grammar() {
    let mut f = fixture("任意の本文");
    let h = f.rules.horizontal[1].y - f.rules.horizontal[0].y;
    for id in 0..f.words.len() {
        let NativeBounds::Glyphs(mut b) = f.bounds[id] else {
            panic!()
        };
        let dy = if f.expected[0].contains(&id) { h } else { -h };
        b[1] += dy;
        b[3] += dy;
        rebound(&mut f, id, b, 8.);
    }
    yes(f);
}
#[test]
fn deletion_operators_identify_the_after_side_without_an_orientation_assumption() {
    let mut f = fixture("任意の本文");
    let id = *f.expected[1].last().unwrap();
    patch(&mut f, id, "〔号を削る。〕");
    f.expected.swap(0, 1);
    yes(f);
}
#[test]
fn paragraph_and_article_units_are_structural_types_not_specific_values() {
    for article in [false, true] {
        let mut f = fixture("任意の本文");
        let mapping = if article {
            vec![
                (0, "第四十一条"),
                (3, "第四十五条"),
                (5, "第四十六条"),
                (7, "第十七条"),
                (10, "第二十一条"),
            ]
        } else {
            vec![
                (0, "４１"),
                (3, "４５"),
                (5, "４６"),
                (7, "１７"),
                (10, "２１"),
            ]
        };
        for (id, text) in mapping {
            prefix(&mut f, id, text);
        }
        patch(
            &mut f,
            2,
            if article {
                "〔第四十二条〜第四十四条略〕"
            } else {
                "〔４２〜４４略〕"
            },
        );
        patch(
            &mut f,
            9,
            if article {
                "〔第十八条〜第二十条同上〕"
            } else {
                "〔１８〜２０同上〕"
            },
        );
        patch(
            &mut f,
            12,
            if article {
                "〔条を加える。〕"
            } else {
                "〔項を加える。〕"
            },
        );
        yes(f);
    }
}
#[test]
fn numbering_values_need_no_page_specific_or_character_count_cap() {
    let mut f = fixture("任意の本文");
    for (id, text) in [
        (0, "千二百三十四"),
        (3, "千二百三十八"),
        (5, "千二百三十九"),
        (7, "六千"),
        (10, "六千四"),
    ] {
        prefix(&mut f, id, text);
    }
    patch(&mut f, 2, "〔千二百三十五〜千二百三十七略〕");
    patch(&mut f, 9, "〔六千一〜六千三同上〕");
    yes(f);
}
#[test]
fn native_number_overflow_is_unknown_and_never_panics() {
    assert_eq!(number("18446744073709551615"), Some(u64::MAX));
    for text in [
        "18446744073709551616".to_string(),
        "九".repeat(500),
        "十百".to_string(),
    ] {
        assert!(number(&text).is_none());
    }
}
#[test]
fn scale_translation_and_opaque_font_identifiers_do_not_select_a_layout() {
    for scale in [0.25, 2., 11.] {
        let mut f = fixture("任意の本文");
        for id in 0..f.words.len() {
            let NativeBounds::Glyphs(b) = f.bounds[id] else {
                panic!()
            };
            let b = [
                37.25 + b[0] * scale,
                -21.75 + b[1] * scale,
                37.25 + b[2] * scale,
                -21.75 + b[3] * scale,
            ];
            rebound(&mut f, id, b, 8. * scale);
            f.styles[id] =
                ContentStyles::Glyphs(NativeStyles::from([(509, (8f64 * scale as f64).to_bits())]));
        }
        for h in &mut f.rules.horizontal {
            h.y = -21.75 + h.y * scale;
            h.x0 = 37.25 + h.x0 * scale;
            h.x1 = 37.25 + h.x1 * scale;
        }
        yes(f);
    }
}
#[test]
fn unknown_or_nonfinite_source_styles_do_not_authorize_an_edit_cell() {
    for value in [None, Some(f64::INFINITY), Some(f64::NAN)] {
        let mut f = fixture("任意の本文");
        let id = *f.expected[1].last().unwrap();
        f.styles[id] = value
            .map(|n| ContentStyles::Glyphs(NativeStyles::from([(3, n.to_bits())])))
            .unwrap_or(ContentStyles::Unknown);
        no(f);
    }
}
#[test]
fn source_numeric_text_cannot_be_borrowed_from_a_different_native_occurrence() {
    let mut f = fixture("任意の本文");
    f.words[0].raw = "本来の別の文字".into();
    no(f);
}
#[test]
fn a_nested_or_independent_vertical_cell_wall_is_not_erased() {
    let mut f = fixture("任意の本文");
    f.rules.vertical.push(crate::lines::VRule {
        x: 100.,
        y0: f.rules.horizontal[0].y,
        y1: f.rules.horizontal[2].y,
    });
    no(f);
}
#[test]
fn a_certificate_cannot_be_reused_after_a_column_or_its_native_text_changes() {
    let f = fixture("任意の本文");
    let p = prove(f.source()).unwrap();
    assert!(p.matches(&f.cols));
    let mut cols = f.cols.clone();
    cols[0].x += 1.;
    assert!(!p.matches(&cols));
    let mut cols = f.cols.clone();
    cols[0].text = "別の本文".into();
    assert!(!p.matches(&cols));
}
