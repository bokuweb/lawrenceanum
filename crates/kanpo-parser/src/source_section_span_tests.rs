use super::*;
fn w(id: usize, y: f32, text: &str) -> Col {
    Col {
        id,
        x: 70.,
        x1: 100.,
        y,
        y1: y + 8.,
        text: text.into(),
    }
}
fn lines(ids: &[usize]) -> Vec<trace::Line> {
    ids.iter()
        .map(|&id| trace::Line::text(String::new(), vec![id]))
        .collect()
}
#[test]
fn source_ruled_sections_remove_only_foreign_section_interleaving() {
    let words = vec![
        w(0, 100., "（"),
        w(1, 200., "任意）"),
        w(2, 310., "別の領域"),
    ];
    let a = lines(&[0, 2, 1]);
    let b = lines(&[0, 1, 2]);
    assert!(!orthogonal_order_preserves_spans(&words, &a, &b));
    assert!(source_order_preserves_spans(&words, &a, &b, 300.));
}
#[test]
fn a_balanced_span_crossing_the_source_ruling_is_not_a_closed_section() {
    let words = vec![w(0, 100., "（"), w(1, 310., "任意）")];
    let a = lines(&[0, 1]);
    assert!(orthogonal_order_preserves_spans(&words, &a, &a));
    assert!(!source_order_preserves_spans(&words, &a, &a, 300.));
}
#[test]
fn within_section_citation_order_remains_protected() {
    let words = vec![
        w(0, 100., "（"),
        w(1, 150., "任意"),
        w(2, 200., "）"),
        w(3, 310., "次の領域"),
    ];
    assert!(!source_order_preserves_spans(
        &words,
        &lines(&[0, 1, 2, 3]),
        &lines(&[0, 2, 1, 3]),
        300.
    ));
}
#[test]
fn unbalanced_or_mismatched_section_spans_are_insufficient() {
    for (a, b) in [
        ("（", "任意"),
        ("任意", "）"),
        ("（", "］"),
        ("（［", "）］"),
    ] {
        let words = vec![w(0, 100., a), w(1, 200., b), w(2, 310., "次")];
        let x = lines(&[0, 1, 2]);
        assert!(!source_order_preserves_spans(&words, &x, &x, 300.));
    }
}
#[test]
fn a_next_section_cannot_be_inserted_inside_a_closed_span() {
    let words = vec![w(0, 100., "（"), w(1, 200., "任意）"), w(2, 310., "次")];
    assert!(!source_order_preserves_spans(
        &words,
        &lines(&[0, 1, 2]),
        &lines(&[0, 2, 1]),
        300.
    ));
}
#[test]
fn complete_unique_ownership_is_required_for_source_section_order() {
    let words = vec![w(0, 100., "（"), w(1, 200., "任意）"), w(2, 310., "次")];
    for ids in [&[0, 1][..], &[0, 1, 1, 2], &[0, 1, 2, 3]] {
        assert!(!source_order_preserves_spans(
            &words,
            &lines(&[0, 1, 2]),
            &lines(ids),
            300.
        ));
    }
}
