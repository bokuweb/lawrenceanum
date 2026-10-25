//! Regression requirements: a structural title qualifier must not depend on
//! ordinal wording, source Word segmentation, arbitrary length or font IDs.
use crate::notice_boundaries::cuts;
use crate::trace::{Line, PageTrace, Region, Role, Word, WordStatus};
use std::collections::BTreeSet;

fn push(p: &mut PageTrace, text: &str, bbox: [f32; 4], font: usize) {
    let id = p.words.len();
    p.words.push(Word {
        source_box: [bbox[0], bbox[1], bbox[2], bbox[3]],
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
        .push(Some(BTreeSet::from([(font, 8f64.to_bits())])));
    p.regions[0].lines.push(Line::text(text.into(), vec![id]));
}
fn sample(title: &str, title_font: usize) -> PageTrace {
    let mut p = PageTrace {native_atomic_columns:vec![],native_axes:vec![],native_rulings:vec![],
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
    let bottom = 50. + 8. * title.chars().count() as f32;
    p.height = p.height.max(bottom + 200.);
    push(&mut p, title, [400., 50., 408., bottom], title_font);
    push(
        &mut p,
        "手続をお知らせします。",
        [389., 58., 397., bottom + 64.],
        2,
    );
    push(&mut p, "2026年7月1日", [378., 60., 386., 140.], 2);
    push(&mut p, "Unknown entity", [367., 100., 375., 180.], 2);
    push(&mut p, "清算人任意", [356., 120., 364., 210.], 2);
    p.doc_text = p.regions[0]
        .lines
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    p.text = p.doc_text.clone();
    p
}
#[test]
fn closed_qualifiers_are_not_limited_to_a_known_ordinal_phrase() {
    for t in [
        "関連公告（対象Ａ）",
        "関連公告(更新対象)",
        "関連公告（補足（対象Ａ））",
        "関連公告（対象Ａ）（更新）",
    ] {
        assert_eq!(cuts(&sample(t, 1), &Default::default()), vec![0], "{t}");
    }
}
#[test]
fn qualifier_length_has_no_character_count_limit() {
    for n in [1, 20, 200] {
        let t = format!("関連公告（{}）", "任意".repeat(n));
        assert_eq!(cuts(&sample(&t, 1), &Default::default()), vec![0], "n={n}");
    }
}
#[test]
fn malformed_or_unframed_prose_tails_do_not_authorize_a_title() {
    for t in [
        "関連公告（対象Ａ",
        "関連公告対象Ａ）",
        "関連公告（対象Ａ)",
        "関連公告についてお知らせします。",
    ] {
        assert!(cuts(&sample(t, 1), &Default::default()).is_empty(), "{t}");
    }
}
#[test]
fn a_body_font_qualified_line_is_not_a_distinct_title() {
    assert!(cuts(&sample("関連公告（対象Ａ）", 2), &Default::default()).is_empty());
}
#[test]
fn qualified_titles_survive_font_bijection_and_geometric_scaling() {
    for k in [0.125, 1., 8.] {
        let mut p = sample("関連公告（対象Ａ）", 1);
        for w in &mut p.words {
            w.x0 = w.x0 * k - 100.;
            w.x1 = w.x1 * k - 100.;
            w.y0 = w.y0 * k - 200.;
            w.y1 = w.y1 * k - 200.;
        }
        for s in p.native_styles.iter_mut().flatten() {
            *s = s
                .iter()
                .map(|&(f, b)| (19 - f, (f64::from_bits(b) * k as f64).to_bits()))
                .collect();
        }
        assert_eq!(cuts(&p, &Default::default()), vec![0]);
    }
}
#[test]
fn punctuation_font_does_not_override_the_qualifier_content_font() {
    let mut p = sample("関連公告（対象Ａ）", 1);
    let originals = p.words[1..].to_vec();
    let styles = p.native_styles[1..].to_vec();
    let lines = p.regions[0].lines[1..].to_vec();
    p.words.clear();
    p.native_styles.clear();
    p.regions[0].lines.clear();
    let mut y = 50.;
    for (text, font) in [("関連公告", 1), ("（", 2), ("対象Ａ", 1), ("）", 2)] {
        let end = y + text.chars().count() as f32 * 8.;
        push(&mut p, text, [400., y, 408., end], font);
        y = end;
    }
    p.regions[0].lines = vec![Line::text("関連公告（対象Ａ）".into(), vec![0, 1, 2, 3])];
    for ((mut w, s), mut l) in originals.into_iter().zip(styles).zip(lines) {
        let id = p.words.len();
        w.id = id;
        l.words = vec![id];
        p.words.push(w);
        p.native_styles.push(s);
        p.regions[0].lines.push(l);
    }
    assert_eq!(cuts(&p, &Default::default()), vec![0]);
    p.native_styles[2] = Some(BTreeSet::from([(2, 8f64.to_bits())]));
    assert!(cuts(&p, &Default::default()).is_empty());
}

#[test]
fn a_marker_inside_an_unclosed_qualifier_is_not_a_main_title_suffix() {
    for title in [
        "関連公告（補足公告",
        "関連公告(補足公告",
        "関連公告）補足公告",
        "関連公告（補足））次公告",
    ] {
        assert!(
            cuts(&sample(title, 1), &Default::default()).is_empty(),
            "{title}"
        );
    }
}

#[test]
fn an_announcement_title_does_not_require_a_category_prefix() {
    for title in ["公告", "公告（任意）"] {
        assert_eq!(
            cuts(&sample(title, 1), &Default::default()),
            vec![0],
            "{title}"
        );
    }
}

#[test]
fn qualifier_content_is_not_restricted_to_letters_or_numbers() {
    for title in ["関連公告（★）", "関連公告（※）", "関連公告（…）"] {
        assert_eq!(
            cuts(&sample(title, 1), &Default::default()),
            vec![0],
            "{title}"
        );
    }
}
