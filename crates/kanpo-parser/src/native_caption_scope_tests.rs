//! A native caption need not be shorter than its following body column.
use crate::notice_boundaries::{cuts, signature_office};
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
fn sample(preface: &str, title: &str, signature: &str) -> PageTrace {
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
    put(&mut p, title, [400., 50., 408., 240.], Some(17));
    put(&mut p, preface, [389., 50., 397., 90.], Some(31));
    put(
        &mut p,
        "ここに本手続の本文を記載する。",
        [378., 58., 386., 240.],
        Some(31),
    );
    put(&mut p, "2026年7月1日", [367., 58., 375., 138.], Some(31));
    put(&mut p, "任意の発行主体", [356., 130., 364., 210.], Some(31));
    put(&mut p, signature, [345., 130., 353., 240.], Some(31));
    p
}
#[test]
fn arbitrary_short_prefaces_before_the_body_do_not_need_a_salutation_dictionary() {
    for text in ["各関係者", "任意", "御通知", "Some preface"] {
        assert_eq!(
            cuts(&sample(text, "関連公告", "清算人任意"), &Default::default()),
            vec![0],
            "{text}"
        );
    }
}
#[test]
fn captions_may_reach_the_same_bottom_as_the_body() {
    let mut p = sample("本文を記載する。", "任意の長い関連公告", "清算人任意");
    p.words[1].y1 = 240.;
    p.words[1].source_box[3] = 240.;
    assert_eq!(cuts(&p, &Default::default()), vec![0]);
}
#[test]
fn caption_and_body_order_has_no_absolute_coordinate_or_scale_budget() {
    for k in [0.125, 1., 8.] {
        let mut p = sample("任意", "関連公告", "清算人任意");
        for w in &mut p.words {
            w.x0 = w.x0 * k - 100.;
            w.x1 = w.x1 * k - 100.;
            w.y0 = w.y0 * k - 200.;
            w.y1 = w.y1 * k - 200.;
            w.source_box = [w.x0, w.y0, w.x1, w.y1];
        }
        assert_eq!(cuts(&p, &Default::default()), vec![0], "{k}");
    }
}
#[test]
fn a_short_caption_scope_still_needs_its_own_date_and_nominal_signature() {
    for id in [3, 5] {
        let mut p = sample("任意", "関連公告", "清算人任意");
        p.words[id].text = "証拠なし".into();
        p.words[id].raw = "証拠なし".into();
        p.regions[0].lines[id].text = "証拠なし".into();
        assert!(cuts(&p, &Default::default()).is_empty(), "{id}");
    }
}
#[test]
fn equal_or_unknown_source_typography_does_not_authorize_a_short_body_scope() {
    for font in [None, Some(17)] {
        let mut p = sample("任意", "関連公告", "清算人任意");
        p.native_styles[1] = font.map(|f| BTreeSet::from([(f, 8f64.to_bits())]));
        assert!(cuts(&p, &Default::default()).is_empty(), "{font:?}");
    }
}
#[test]
fn date_and_signature_lines_cannot_supply_the_missing_body() {
    for text in ["2026年7月1日", "清算人任意"] {
        let mut p = sample(text, "関連公告", "清算人任意");
        p.regions[0].lines.remove(2);
        p.words[2].status = WordStatus::Noise;
        let owned = p.regions[0]
            .lines
            .iter()
            .flat_map(|l| l.words.iter().copied())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            owned,
            p.words
                .iter()
                .filter(|w| matches!(w.status, WordStatus::Used))
                .map(|w| w.id)
                .collect()
        );
        assert!(cuts(&p, &Default::default()).is_empty(), "{text}");
    }
}
#[test]
fn body_geometry_still_has_to_follow_the_caption_in_its_native_band() {
    for b in [
        [410., 50., 418., 90.],
        [389., 260., 397., 300.],
        [389., 10., 397., 40.],
    ] {
        let mut p = sample("任意", "関連公告", "清算人任意");
        let w = &mut p.words[1];
        w.x0 = b[0];
        w.y0 = b[1];
        w.x1 = b[2];
        w.y1 = b[3];
        w.source_box = b;
        assert!(cuts(&p, &Default::default()).is_empty(), "{b:?}");
    }
}
#[test]
fn a_body_in_a_different_source_region_cannot_prove_caption_ownership() {
    let mut p = sample("任意", "関連公告", "清算人任意");
    let rest = p.regions[0].lines.split_off(1);
    p.regions.push(Region {
        role: Role::Body,
        unit: 1,
        part: 0,
        lines: rest,
    });
    assert!(cuts(&p, &Default::default()).is_empty());
}
#[test]
fn a_header_region_cannot_supply_the_short_body_proof() {
    let mut p = sample("任意", "関連公告", "清算人任意");
    p.regions[0].role = Role::Preamble;
    assert!(cuts(&p, &Default::default()).is_empty());
}
#[test]
fn a_full_quoted_caption_does_not_start_an_independent_scope() {
    let p = sample("任意", "「関連公告」", "清算人任意");
    assert!(cuts(&p, &Default::default()).is_empty());
}
#[test]
fn canonical_foreign_company_representative_is_a_nominal_signature() {
    assert!(signature_office("日本における代表者任意"));
    assert_eq!(
        cuts(
            &sample("任意", "関連公告", "日本における代表者任意"),
            &Default::default()
        ),
        vec![0]
    );
}
#[test]
fn bare_roles_embedded_mentions_and_sentences_are_not_signatures() {
    for text in [
        "日本における代表者",
        "当社の日本における代表者任意",
        "日本における代表者任意が退任します。",
        "代表者任意",
    ] {
        assert!(!signature_office(text), "{text}");
    }
}

#[test]
fn an_empty_normalized_column_cannot_supply_the_body_proof() {
    for text in ["", "　", " \t"] {
        let p = sample(text, "関連公告", "清算人任意");
        assert!(cuts(&p, &Default::default()).is_empty(), "{text:?}");
    }
}
