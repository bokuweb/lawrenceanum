use super::*;
use crate::native_word_flow::NativeMode;
fn modes(words: &[String]) -> std::collections::BTreeMap<usize, Option<NativeMode>> {
    let x = format!(
        "<doc><page width='595' height='842'>{}</page></doc>",
        words.join("")
    );
    let d = Html::parse_document(&x);
    let p = d.select(&Selector::parse("page").unwrap()).next().unwrap();
    let (_, words, _, _, _, _, styles, _, _, _, _, _, flows , _) =
        page_words_with_styles(p, 595., 842., None);
    assert_eq!(words.len(), flows.len());
    styles
        .iter()
        .zip(flows)
        .map(|(style, f)| {
            (
                style.as_ref().unwrap().iter().next().unwrap().0,
                f.and_then(|f| f.mode),
            )
        })
        .collect()
}
fn word(font: usize, mode: &str) -> String {
    format!("<word xmin='10' ymin='93' xmax='18' ymax='102' {mode} data-native-xmin='10' data-native-ymin='93' data-native-xmax='18' data-native-ymax='102' data-native-axis='right' data-native-styles='{font}:4020000000000000' data-native-flow-start='10' data-native-flow-baseline='100' data-native-flow-end='18' data-native-flow-leading='8' data-native-flow-trailing='0' data-native-flow-prefix='0'>任意</word>")
}
#[test]
fn distinct_native_modes_stay_with_their_original_equal_box_and_text_occurrences() {
    let words = vec![word(11, "data-wmode='0'"), word(27, "data-wmode='1'")];
    let a = modes(&words);
    let mut b = words;
    b.reverse();
    assert_eq!(a, modes(&b));
    assert_eq!(a[&11], Some(NativeMode::Horizontal));
    assert_eq!(a[&27], Some(NativeMode::Vertical));
}
#[test]
fn absent_invalid_and_noncanonical_modes_are_not_filled_from_the_source_axis_or_box() {
    for attr in [
        "",
        "data-wmode='unknown'",
        "data-wmode='2'",
        "data-wmode='00'",
        "data-wmode='01'",
        "data-wmode='NaN'",
    ] {
        let a = modes(&[word(1, "data-wmode='0'"), word(2, attr)]);
        assert_eq!(a[&1], Some(NativeMode::Horizontal));
        assert_eq!(a[&2], None);
    }
}
#[test]
fn font_identity_and_emission_order_do_not_recognize_a_writing_mode() {
    for (a, b) in [(1, 2), (80, 7), (999, 3)] {
        let words = vec![word(a, "data-wmode='1'"), word(b, "data-wmode='0'")];
        let before = modes(&words);
        let mut after = words;
        after.reverse();
        assert_eq!(before, modes(&after));
        assert_eq!(before[&a], Some(NativeMode::Vertical));
        assert_eq!(before[&b], Some(NativeMode::Horizontal));
    }
}
