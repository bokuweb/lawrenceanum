use super::*;
fn signature(
    order: &[(usize, f64, usize)],
) -> std::collections::BTreeMap<usize, (u64, u64, usize, usize)> {
    let ws=order.iter().map(|&(font,low,first)|format!("<word xmin='10' ymin='90' xmax='30' ymax='102' data-wmode='0' data-native-axis='right' data-native-styles='{font}:4020000000000000' data-native-advance-band='1 {first} {} 10 {low} 100 30 8 0 0 10.000000 90.000000 30.000000 102.000000 10 89 30 102'>same</word>",first+2)).collect::<String>();
    let d = Html::parse_document(&format!(
        "<page width='595' height='842' data-native-glyph-count='4'>{ws}</page>"
    ));
    let p = d.select(&Selector::parse("page").unwrap()).next().unwrap();
    let ((_, words, _, _, _, _, styles, _, _, _, _, _, _, _, _), bands) =
        page_words_with_styles_and_bands(p, 595., 842., None);
    assert_eq!(words.len(), bands.len());
    styles
        .into_iter()
        .zip(bands)
        .map(|(s, b)| {
            let b = b.unwrap();
            (
                s.unwrap().iter().next().unwrap().0,
                (
                    b.min_baseline.to_bits(),
                    b.max_baseline.to_bits(),
                    b.first,
                    b.after,
                ),
            )
        })
        .collect()
}
#[test]
fn band144_stays_bound_to_its_own_exact_bbox_text_tie_and_source_font() {
    let mut input = vec![(71, 99.992, 0), (300, 99.8, 2)];
    let expected = signature(&input);
    input.reverse();
    assert_eq!(signature(&input), expected);
    assert_eq!(
        expected[&71],
        (99.992_f64.to_bits(), 100_f64.to_bits(), 0, 2)
    );
    assert_eq!(
        expected[&300],
        (99.8_f64.to_bits(), 100_f64.to_bits(), 2, 4)
    );
}
