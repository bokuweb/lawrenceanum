// Source order was fixed from full original renders before the runtime change.
// Bitmap-only body omissions remain outside these native text regressions.
use crate::{analyze_item, page_inputs, Analysis};
fn original(bytes: &[u8]) -> Analysis {
    analyze_item(&page_inputs(bytes).unwrap())
}
fn lines(a: &Analysis) -> Vec<Vec<usize>> {
    a.pages[0]
        .regions
        .iter()
        .flat_map(|r| r.lines.iter())
        .map(|l| l.words.clone())
        .collect()
}
fn anchors(a: &Analysis, expected: &[usize]) -> Vec<usize> {
    lines(a)
        .into_iter()
        .flatten()
        .filter(|id| expected.contains(id))
        .collect()
}
#[test]
fn source_centered_form_titles_precede_the_lower_left_section() {
    let a = original(include_bytes!(
        "../tests/archive/pages/20260731g00172/20260731g001720133.pdf"
    ));
    assert_eq!(&lines(&a)[..3], &[vec![3], vec![14, 4, 1, 0], vec![47]]);
}
#[test]
fn source_vertical_divider_preserves_the_complete_column_order() {
    let a = original(include_bytes!(
        "../tests/data/orthogonal-gutters/20260717h017500021.pdf"
    ));
    let expected = [12, 13, 11, 10, 0, 1, 2];
    assert_eq!(anchors(&a, &expected), expected);
}
#[test]
fn source_framed_title_precedes_its_lower_subheading() {
    let a = original(include_bytes!(
        "../tests/data/orthogonal-gutters/20260821g001870026.pdf"
    ));
    let expected = [47, 41, 53, 51, 49];
    assert_eq!(anchors(&a, &expected), expected);
}
#[test]
fn source_titles_precede_both_lower_fields_in_each_comparison_half() {
    let a = original(include_bytes!(
        "../tests/data/orthogonal-gutters/20260930g002150052.pdf"
    ));
    let expected = [98, 103, 71, 30, 35, 4];
    assert_eq!(anchors(&a, &expected), expected);
}
#[test]
fn source_wall_columns_do_not_merge_their_headings() {
    let a = original(include_bytes!(
        "../tests/data/orthogonal-gutters/20260717h017500011.pdf"
    ));
    assert_eq!(lines(&a), vec![vec![4], vec![1], vec![3], vec![2], vec![0]]);
}
#[test]
fn source_wall_column_finishes_before_the_right_column() {
    let a = original(include_bytes!(
        "../tests/data/orthogonal-gutters/20260717h017500027.pdf"
    ));
    assert_eq!(
        lines(&a),
        vec![vec![4, 5], vec![6, 7], vec![0, 1], vec![2, 3]]
    );
}

#[test]
fn source_multi_issuer_citation_finishes_before_shared_suffix_32() {
    let a = original(include_bytes!(
        "../tests/data/orthogonal-gutters/20260824g001880032.pdf"
    ));
    let expected = [69, 67, 64, 70, 68, 65, 71, 63];
    assert_eq!(anchors(&a, &expected), expected);
}
#[test]
fn source_multi_issuer_citation_finishes_before_shared_suffix_36() {
    let a = original(include_bytes!(
        "../tests/data/orthogonal-gutters/20260824g001880036.pdf"
    ));
    let expected = [50, 48, 43, 51, 49, 44, 52, 42];
    assert_eq!(anchors(&a, &expected), expected);
}
