use super::*;
fn lexical155(label: &str) -> Fixture {
    source152(&[
        (label, &[Some("11"), None]),
        ("Unrelated", &[Some("22"), Some("33")]),
        ("Elsewhere", &[None, Some("44")]),
    ])
}
#[test]
fn lexical155_inline_ascii_digits_do_not_turn_a_source_label_into_an_amount() {
    positive152(lexical155("Class17alpha"), 3);
}
#[test]
fn lexical155_inline_wide_digits_do_not_turn_a_source_label_into_an_amount() {
    positive152(lexical155("第１７種の項目"), 3);
}
#[test]
fn lexical155_footnote_numbers_stay_inside_the_original_label() {
    for label in ["任意の項目※３", "Arbitrary†12345", "Other‡１２"] {
        positive152(lexical155(label), 3);
    }
}
#[test]
fn lexical155_balanced_parenthesized_ascii_enumerator_stays_with_its_source_label() {
    positive152(lexical155("(12) Arbitrary item"), 3);
}
#[test]
fn lexical155_balanced_parenthesized_wide_enumerator_stays_with_its_source_label() {
    positive152(lexical155("（１２） 任意の項目"), 3);
}
#[test]
fn lexical155_an_unpunctuated_leading_integer_has_no_enum_authority() {
    decline152(lexical155("123 Arbitrary item"));
}
#[test]
fn lexical155_an_unpunctuated_leading_wide_integer_has_no_enum_authority() {
    decline152(lexical155("１２３ 任意の項目"));
}
#[test]
fn lexical155_year_embedded_in_numbered_label_is_literal_source_content() {
    positive152(lexical155("（17） 年度23の任意項目"), 3);
}
#[test]
fn lexical155_a_nonleading_separate_number_cannot_be_hidden_inside_a_label() {
    decline152(lexical155("Arbitrary 123 item"));
}
#[test]
fn lexical155_a_trailing_separate_number_cannot_be_hidden_inside_a_label() {
    decline152(lexical155("Arbitrary 123"));
}
#[test]
fn lexical155_an_enumerator_does_not_authorize_another_bare_numeric_label_token() {
    decline152(lexical155("(12) Arbitrary 123 item"));
}
#[test]
fn lexical155_an_unbalanced_enumerator_has_no_label_authority() {
    decline152(lexical155("(12 Arbitrary item"));
}
#[test]
fn lexical155_a_decimal_fraction_prefix_is_not_an_enumerator() {
    decline152(lexical155("1.25 Arbitrary item"));
}
#[test]
fn lexical155_a_signed_amount_prefix_is_not_an_enumerator() {
    decline152(lexical155("-123 Arbitrary item"));
}
#[test]
fn lexical155_an_integer_without_a_label_cannot_supply_a_label() {
    decline152(lexical155("123"));
}
#[test]
fn lexical155_nearby_different_original_numeric_rails_still_decline() {
    let mut s = lexical155("(12) Arbitrary item");
    s.1.last_mut().unwrap().x1 += 0.125;
    decline152(s);
}
#[test]
fn lexical155_missing_original_baseline_still_declines() {
    let mut s = lexical155("Class17alpha");
    s.3[0] = None;
    decline152(s);
}
#[test]
fn lexical155_omitted_original_label_word_still_declines() {
    let mut s = lexical155("Class17alpha");
    if let Unit::Plain(ls) = &mut s.0[0] {
        ls[0].words.remove(0);
    }
    decline152(s);
}
