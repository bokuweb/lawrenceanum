use super::*;
use crate::native_word_rulings::NativeRuling;
#[test]
fn equal_bbox_text_occurrences_keep_their_own_font_axis_and_ink_evidence() {
    let a="<word xmin=100 ymin=100 xmax=108 ymax=140 data-native-axis=right data-native-styles='11:4020000000000000' data-native-ruling=horizontal data-native-ink-xmin=100 data-native-ink-ymin=110 data-native-ink-xmax=108 data-native-ink-ymax=111>同</word>";
    let b="<word xmin=100 ymin=100 xmax=108 ymax=140 data-native-axis=down data-native-styles='29:4020000000000000' data-native-ruling=vertical data-native-ink-xmin=102 data-native-ink-ymin=100 data-native-ink-xmax=103 data-native-ink-ymax=140>同</word>";
    for xml in [
        format!("<page>{a}{b}</page>"),
        format!("<page>{b}{a}</page>"),
    ] {
        let h = Html::parse_document(&xml);
        let p = h.select(&Selector::parse("page").unwrap()).next().unwrap();
        let (_, w, _, _, _, _, styles, _, _, axes, rulings, _, _) =
            page_words_with_styles(p, 595., 842., None);
        assert_eq!(w.len(), 2);
        for i in 0..2 {
            if styles[i].as_ref().unwrap().contains(&(11, 8f64.to_bits())) {
                assert_eq!(axes[i], crate::native_word_axes::NativeAxis::Right);
                assert_eq!(
                    rulings[i],
                    NativeRuling::Horizontal([100., 110., 108., 111.])
                );
            } else {
                assert_eq!(axes[i], crate::native_word_axes::NativeAxis::Down);
                assert_eq!(rulings[i], NativeRuling::Vertical([102., 100., 103., 140.]));
            }
        }
    }
}
