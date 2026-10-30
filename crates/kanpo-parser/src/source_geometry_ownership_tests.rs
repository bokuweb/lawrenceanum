use super::*;
use crate::trace::{Word, WordStatus};
fn fixture() -> (Vec<Col>, Vec<Word>) {
    let cols = vec![
        Col {
            id: 0,
            x: 70.,
            x1: 100.,
            y: 90.,
            y1: 98.,
            text: "任意".into(),
        },
        Col {
            id: 1,
            x: 70.,
            x1: 100.,
            y: 310.,
            y1: 318.,
            text: "別の語".into(),
        },
    ];
    let original = cols
        .iter()
        .map(|w| Word {
            id: w.id,
            x0: w.x,
            y0: w.y,
            x1: w.x1,
            y1: w.y1,
            source_box: [w.x, w.y, w.x1, w.y1],
            raw: w.text.clone(),
            text: w.text.clone(),
            status: WordStatus::Used,
        })
        .collect();
    (cols, original)
}
fn bound(cols: &[Col], original: &[Word]) -> bool {
    SourceGeometry {
            bands: &[],
        enclosures: &[],
        flows: &[],
        bounds: &[],
        axes: &[],
        words: original,
        styles: &[],
    }
    .sources_unchanged(cols)
}
#[test]
fn original_occurrences_are_independent_of_projection_order_or_vocabulary() {
    let (mut c, mut w) = fixture();
    assert!(bound(&c, &w));
    c.reverse();
    assert!(bound(&c, &w));
    for labels in [["x", "y"], ["甲乙丙丁", "（一般）"]] {
        for q in &mut c {
            q.text = labels[q.id].into();
            w[q.id].text = q.text.clone();
        }
        assert!(bound(&c, &w));
    }
}
#[test]
fn a_joined_or_replaced_projection_cannot_use_one_original_words_geometry() {
    let (mut c, w) = fixture();
    for text in ["任意別の語", "他の文字"] {
        c[0].text = text.into();
        assert!(!bound(&c, &w));
    }
}
#[test]
fn a_projection_moved_after_source_binding_cannot_certify_the_old_rectangle() {
    for field in 0..4 {
        let (mut c, w) = fixture();
        match field {
            0 => c[0].x += 1.,
            1 => c[0].y += 1.,
            2 => c[0].x1 += 1.,
            _ => c[0].y1 += 1.,
        };
        assert!(!bound(&c, &w));
    }
}
#[test]
fn the_original_occurrence_id_must_match_its_metadata_slot() {
    let (c, mut w) = fixture();
    w[0].id = 1;
    assert!(!bound(&c, &w));
}
#[test]
fn absent_original_occurrences_are_not_inferred_from_another_word() {
    let (c, w) = fixture();
    assert!(!bound(&c, &w[..1]));
    assert!(!bound(&c, &[]));
}
#[test]
fn dropped_or_margin_source_words_cannot_authorize_a_new_partition() {
    for status in [WordStatus::Empty, WordStatus::Margin] {
        let (c, mut w) = fixture();
        w[0].status = status;
        assert!(!bound(&c, &w));
    }
}
#[test]
fn a_reused_projection_id_does_not_certify_two_occurrences() {
    let (mut c, w) = fixture();
    c[1] = c[0].clone();
    assert!(!bound(&c, &w));
}
#[test]
fn nonfinite_projection_geometry_is_not_source_correspondence() {
    let (mut c, mut w) = fixture();
    c[0].x = f32::NAN;
    w[0].x0 = f32::NAN;
    assert!(!bound(&c, &w));
}
