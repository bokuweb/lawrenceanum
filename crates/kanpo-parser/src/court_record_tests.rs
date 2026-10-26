use super::*;
use crate::{
    native_word_axes::NativeAxis,
    trace::{ContentStyles, NativeBounds, Region, Word},
};

fn add(p: &mut PageTrace, text: &str, b: Rect, font: usize) {
    let id = p.words.len();
    p.words.push(Word {
        id,
        source_box: b,
        x0: b[0],
        y0: b[1],
        x1: b[2],
        y1: b[3],
        raw: text.into(),
        text: text.into(),
        status: WordStatus::Used,
    });
    p.native_styles
        .push(Some(BTreeSet::from([(font, 8f64.to_bits())])));
    p.native_content_styles
        .push(ContentStyles::Glyphs(BTreeSet::from([(
            font,
            8f64.to_bits(),
        )])));
    p.native_bounds.push(NativeBounds::Glyphs(b));
    p.native_axes.push(NativeAxis::Right);
    p.regions[0].lines.push(Line::text(text.into(), vec![id]));
    p.doc_text = p.regions[0]
        .lines
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    p.text = p.doc_text.clone();
}
fn fixture(case: &str, court: &str) -> PageTrace {
    let mut p = PageTrace {native_comparison_labels:vec![],native_horizontal_baselines:vec![],native_atomic_columns:vec![],
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
    };
    add(&mut p, "共通の欄見出し", [120., 40., 240., 50.], 3);
    add(&mut p, "令和元年（ヒ）第123号", [60., 60., 220., 70.], 2);
    add(&mut p, "任意の当事者", [68., 80., 220., 90.], 0);
    add(&mut p, "本文の末尾。", [68., 100., 400., 110.], 0);
    add(&mut p, court, [280., 120., 400., 130.], 0);
    add(&mut p, case, [60., 140., 220., 150.], 2);
    add(&mut p, "別の任意の当事者", [68., 160., 240., 170.], 0);
    p
}
fn valid() -> PageTrace {
    fixture("平成三十年（フ）第987654号", "架空地方裁判所民事第七部")
}
#[test]
fn signed_court_record_starts_at_its_case_number_without_orphaning_the_shared_heading() {
    assert_eq!(cuts(&valid(), &Default::default()), vec![5]);
}
#[test]
fn court_case_values_and_institution_locations_are_not_dispatch_keys() {
    for case in [
        "昭和六十二年(ワ)第一万二千三百四十五号",
        "令和000008年（家イ）第0000001号",
        "平成元年(レ)第9号",
    ] {
        for court in [
            "未知家庭裁判所任意支部",
            "遠隔簡易裁判所",
            "最高裁判所第一小法廷",
        ] {
            assert_eq!(
                cuts(&fixture(case, court), &Default::default()),
                vec![5],
                "{case} {court}"
            );
        }
    }
}
#[test]
fn source_content_styles_allow_shared_parenthesis_fonts() {
    let mut p = valid();
    p.native_styles[5]
        .as_mut()
        .unwrap()
        .insert((0, 8f64.to_bits()));
    assert_eq!(cuts(&p, &Default::default()), vec![5]);
}
#[test]
fn court_record_proof_is_invariant_under_scale_translation_and_font_renaming() {
    for scale in [0.125f32, 1., 8.] {
        let mut p = valid();
        for i in 0..p.words.len() {
            let b = p.words[i].source_box;
            let b = [
                b[0] * scale - 300.,
                b[1] * scale - 150.,
                b[2] * scale - 300.,
                b[3] * scale - 150.,
            ];
            p.words[i].x0 = b[0];
            p.words[i].y0 = b[1];
            p.words[i].x1 = b[2];
            p.words[i].y1 = b[3];
            p.words[i].source_box = b;
            p.native_bounds[i] = NativeBounds::Glyphs(b);
            if let Some(s) = &mut p.native_styles[i] {
                *s = s
                    .iter()
                    .map(|&(f, b)| (100 - f, (f64::from_bits(b) * scale as f64).to_bits()))
                    .collect();
            }
            if let ContentStyles::Glyphs(s) = &mut p.native_content_styles[i] {
                *s = s
                    .iter()
                    .map(|&(f, b)| (100 - f, (f64::from_bits(b) * scale as f64).to_bits()))
                    .collect();
            }
        }
        assert_eq!(cuts(&p, &Default::default()), vec![5]);
    }
}
#[test]
fn a_court_mention_or_case_reference_in_prose_is_not_a_record_header() {
    for case in [
        "令和八年（ヒ）第３号について",
        "前記令和八年（ヒ）第３号",
        "令和八年（ヒ）第３号、同第４号",
        "令和八年（ヒ）第号",
        "令和八年（ヒ）第〇号",
        "令和八年（ヒ）第3号。",
        "令和八年（ヒ)第3号",
    ] {
        assert!(
            cuts(&fixture(case, "未知地方裁判所"), &Default::default()).is_empty(),
            "{case}"
        );
    }
    for court in [
        "未知地方裁判所による判断",
        "未知地方裁判所。",
        "未知地方裁判所と協議する",
        "裁判所名未定",
    ] {
        assert!(
            cuts(&fixture("令和八年（ヒ）第3号", court), &Default::default()).is_empty(),
            "{court}"
        );
    }
}
#[test]
fn missing_or_shared_typography_cannot_authorize_court_records() {
    for id in [4, 5, 6] {
        let mut p = valid();
        p.native_styles[id] = None;
        assert!(cuts(&p, &Default::default()).is_empty());
        let mut p = valid();
        p.native_content_styles[id] = ContentStyles::Unknown;
        assert!(cuts(&p, &Default::default()).is_empty());
    }
    for id in [4, 6] {
        let mut p = valid();
        p.native_styles[id] = p.native_styles[5].clone();
        p.native_content_styles[id] = p.native_content_styles[5].clone();
        assert!(cuts(&p, &Default::default()).is_empty());
    }
}
#[test]
fn explicit_unknown_bounds_or_directions_do_not_use_compatibility_coordinates() {
    for id in [4, 5, 6] {
        let mut p = valid();
        p.native_bounds[id] = NativeBounds::Unknown;
        assert!(cuts(&p, &Default::default()).is_empty());
        for axis in [
            NativeAxis::Unknown,
            NativeAxis::Compatibility,
            NativeAxis::Down,
        ] {
            let mut p = valid();
            p.native_axes[id] = axis;
            assert!(cuts(&p, &Default::default()).is_empty());
        }
    }
}
#[test]
fn judicial_identifiers_inside_a_quotation_or_nonbody_role_are_not_boundaries() {
    let mut p = valid();
    p.regions[0].lines[3].text = "引用「".into();
    p.words[3].text = "引用「".into();
    assert!(cuts(&p, &Default::default()).is_empty());
    let mut p = valid();
    p.regions[0].role = Role::Before;
    assert!(cuts(&p, &Default::default()).is_empty());
}
#[test]
fn court_record_geometry_requires_a_terminal_signature_and_a_following_body() {
    for (id, b) in [
        (4, [60., 120., 180., 130.]),
        (4, [280., 145., 400., 155.]),
        (6, [50., 160., 240., 170.]),
        (6, [68., 145., 240., 155.]),
        (6, [410., 160., 500., 170.]),
    ] {
        let mut p = valid();
        p.native_bounds[id] = NativeBounds::Glyphs(b);
        assert!(cuts(&p, &Default::default()).is_empty(), "{id} {b:?}");
    }
    let mut p = valid();
    p.regions[0].lines.pop();
    p.words.pop();
    assert!(cuts(&p, &Default::default()).is_empty());
}
#[test]
fn court_record_source_ownership_must_be_unique_and_match_rendered_text() {
    let mut p = valid();
    p.regions[0].lines[5].text = "平成三十年（フ）第1号".into();
    assert!(cuts(&p, &Default::default()).is_empty());
    let mut p = valid();
    p.regions[0].lines[6].words.push(5);
    assert!(cuts(&p, &Default::default()).is_empty());
}
#[test]
fn a_case_inside_an_enclosed_source_scope_is_not_a_separate_article() {
    let mut r = PageRules::default();
    r.enclosures.push([50., 135., 410., 180.]);
    assert!(cuts(&valid(), &r).is_empty());
}
#[test]
fn a_footer_in_another_source_region_cannot_authorize_the_new_header() {
    let mut p = valid();
    let lines = p.regions[0].lines.split_off(5);
    p.regions.push(Region {
        role: Role::Body,
        unit: 1,
        part: 0,
        lines,
    });
    assert!(cuts(&p, &Default::default()).is_empty());
}
#[test]
fn whole_table_cells_cannot_supply_a_court_header_or_its_witnesses() {
    for index in [4, 5, 6] {
        let mut p = valid();
        let l = &mut p.regions[0].lines[index];
        let id = l.words[0];
        let b = p.words[id].source_box;
        l.words.clear();
        l.table = Some(crate::trace::Table {
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
                text: l.text.clone(),
                words: vec![id],
            }],
        });
        assert!(cuts(&p, &Default::default()).is_empty());
    }
}

include!("court_category_tests.rs");

#[path = "court_record_column_tests150.rs"]
mod columns150;

#[path="court_body_style_tests151.rs"]
mod body151;
