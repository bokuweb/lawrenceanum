use super::*;
use crate::native_word_axes::NativeAxis;
fn bound(xml: &str) -> Vec<(String, NativeAxis, Option<crate::trace::NativeStyles>)> {
    let h = Html::parse_document(xml);
    let p = h.select(&Selector::parse("page").unwrap()).next().unwrap();
    let (_, w, _, _, _, _, s, _, _, a, _) = page_words_with_styles(p, 595., 842., None);
    w.into_iter()
        .enumerate()
        .map(|(i, w)| (w.raw, a[i], s[i].clone()))
        .collect()
}
fn word(x: i32, text: &str, axis: &str, font: usize) -> String {
    format!("<word xMin='{x}' yMin='100' xMax='{}' yMax='140' data-native-axis='{axis}' data-native-styles='{font}:4020000000000000'>{text}</word>",x+8)
}
#[test]
fn canonical_word_sort_preserves_each_source_axis_font_and_text_pair() {
    let a = word(100, "同", "down", 11);
    let b = word(200, "別", "right", 29);
    let first = bound(&format!("<page width=595 height=842>{a}{b}</page>"));
    assert_eq!(
        first[0],
        (
            "別".into(),
            NativeAxis::Right,
            Some(crate::trace::NativeStyles::from([(29, 8f64.to_bits())]))
        )
    );
    assert_eq!(first[1].1, NativeAxis::Down);
    assert_eq!(
        first,
        bound(&format!("<page width=595 height=842>{b}{a}</page>"))
    );
}
#[test]
fn exact_coordinate_and_text_ties_keep_their_own_axis_and_style() {
    let a = word(100, "同", "down", 11);
    let b = word(100, "同", "right", 29);
    for xml in [
        format!("<page>{a}{b}</page>"),
        format!("<page>{b}{a}</page>"),
    ] {
        let xs = bound(&xml);
        assert_eq!(xs.len(), 2);
        for (_, axis, style) in xs {
            assert_eq!(
                axis,
                if style.unwrap().contains(&(11, 8f64.to_bits())) {
                    NativeAxis::Down
                } else {
                    NativeAxis::Right
                }
            );
        }
    }
}
