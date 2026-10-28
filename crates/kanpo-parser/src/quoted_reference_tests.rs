use crate::Document;
fn check(input: &str, expected: &str) {
    assert_eq!(Document::from_text(input).to_text(), expected);
}
#[test]
fn a_complete_fullwidth_reference_branch_stays_one_identifier() {
    check("「第68条の\n２の４第３項」", "「第68条の２の４第３項」");
}
#[test]
fn an_ascii_reference_branch_is_not_a_new_list_item() {
    check("「第7条の\n2第3項」", "「第7条の2第3項」");
}
#[test]
fn a_kanji_branch_is_not_a_new_list_item() {
    check("「第一条の\n二第三項」", "「第一条の二第三項」");
}
#[test]
fn a_complete_reference_can_span_more_than_one_physical_boundary() {
    check("「第1条の\n２\n第3項」", "「第1条の２第3項」");
}
#[test]
fn repeated_same_level_references_remain_separate_items() {
    check("「第1条\n第2条」", "「第1条\n第2条」");
}
#[test]
fn an_incomplete_quotation_does_not_certify_a_reference() {
    check("「第1条の\n２第3項", "「第1条の\n２第3項");
}
#[test]
fn prose_after_a_number_is_not_a_complete_reference() {
    check("「第1条の\n２の説明」", "「第1条の\n２の説明」");
}
#[test]
fn descending_hierarchy_is_required() {
    check("「第1項\n第2条」", "「第1項\n第2条」");
}
#[test]
fn explicit_spaces_are_not_removed_to_parse_a_reference() {
    check("「第1条 の\n２第3項」", "「第1条 の\n２第3項」");
}
#[test]
fn a_number_without_a_legal_ordinal_is_not_a_reference() {
    check("「第の\n２第3項」", "「第の\n２第3項」");
}
#[test]
fn a_non_numeric_branch_stays_unproven() {
    check("「第1条の\nイ」", "「第1条の\nイ」");
}
#[test]
fn quoted_separate_paragraphs_do_not_form_one_identifier() {
    check(
        "「第1条の\n２の説明。\n第三項」",
        "「第1条の\n２の説明。\n第三項」",
    );
}

#[test]
fn split_numerals_remain_ambiguous_without_source_geometry() {
    check("「第1条の\n２\n３」", "「第1条の\n２\n３」");
}
#[test]
fn an_empty_physical_line_is_not_a_reference_continuation() {
    check("「第1条の\n\n２第3項」", "「第1条の\n２第3項」");
}
#[test]
fn a_complete_leaf_reference_keeps_its_genuine_outer_quotation() {
    check("『「第1条の\n２第3項」』", "『「第1条の２第3項」』");
}
#[test]
fn a_nested_quote_does_not_make_its_outer_prose_a_reference() {
    check(
        "「第一条の\n二の説明「第三項」」",
        "「第一条の\n二の説明「第三項」」",
    );
}

#[test]
fn explicit_ascii_space_before_a_physical_boundary_does_not_certify_a_reference() {
    check("「第1条の \n２第3項」", "「第1条の\n２第3項」");
}
#[test]
fn explicit_fullwidth_space_before_a_physical_boundary_does_not_certify_a_reference() {
    check("「第1条の　\n２第3項」", "「第1条の\n２第3項」");
}
#[test]
fn explicit_space_after_a_physical_boundary_does_not_certify_a_reference() {
    check("「第1条の\n ２第3項」", "「第1条の\n２第3項」");
}
#[test]
fn unquoted_identifiers_keep_the_existing_structural_boundary() {
    check("第1条の\n２第3項", "第1条の\n２第3項");
}
#[test]
fn reference_ordinals_are_syntax_not_specific_values_or_machine_sized_integers() {
    for n in [
        "7",
        "１７",
        "六十八",
        "99999999999999999999999999999999999999999999999999999999999999",
    ] {
        check(
            &format!("「第{n}条の\n２第3項」"),
            &format!("「第{n}条の２第3項」"),
        );
    }
}

#[test]
fn a_prior_page_closer_does_not_change_a_later_self_contained_reference() {
    check(
        "prior quote end」。\nnext「第1条の\n２第3項」",
        "prior quote end」。\nnext「第1条の２第3項」",
    );
}
#[test]
fn mismatched_delimiters_inside_the_candidate_cannot_certify_it() {
    check("『「第1条の\n２第3項』」", "『「第1条の\n２第3項』」");
}
