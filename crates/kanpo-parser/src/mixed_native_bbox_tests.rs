use super::*;
fn sample(body: &str, attrs: &str, quote: &str) -> (Vec<Col>, Vec<Word>, bool, bool) {
    let html = Html::parse_document(&format!("<page width=\"842\" height=\"595\"><word xmin=\"300\" ymin=\"100\" xmax=\"620\" ymax=\"108.184\" data-wmode=\"0\">{body}</word><word xmin=\"60\" ymin=\"478\" xmax=\"108\" ymax=\"486\" {attrs}>{quote}</word>{QUOTATION}</page>"));
    let page = html.select(&Selector::parse("page").unwrap()).next().unwrap();
    let (c, w, r, s, _, _) = page_words_traced(page, 842., 595., None);
    (c, w, r, s)
}
const HORIZONTAL: &str = "横組み本文が続く通常の横書きの段落である。";
const META: &str = "data-wmode=\"1\" data-advance-x=\"8\" data-advance-y=\"0\" data-native-xmin=\"68\" data-native-ymin=\"482\" data-native-xmax=\"116\" data-native-ymax=\"490\"";
const QUOTATION: &str = r#"<word xmin="116.2" ymin="482.8" xmax="148.2" ymax="491" data-wmode="0">項目名称</word><word xmin="140" ymin="478" xmax="196" ymax="486" data-wmode="1" data-advance-x="8" data-advance-y="0" data-native-xmin="148" data-native-ymin="482" data-native-xmax="204" data-native-ymax="490">﹂に改める︒</word>"#;
fn selected(words: &[Word]) -> &Word {
    words.iter().find(|w| w.raw == "別表第１中﹁").unwrap()
}
#[test]
fn horizontal_mixed_font_uses_declared_native_rectangle() {
    let (c, w, _, s) = sample(HORIZONTAL, META, "別表第１中﹁");
    assert!(!s);
    let w = selected(&w);
    assert_eq!((w.x0, w.y0, w.x1, w.y1), (68., 482., 116., 490.));
    let c = c.iter().find(|c| c.id == w.id).unwrap();
    assert_eq!((c.x, c.y, c.x1, c.y1), (68., 482., 116., 490.));
}
#[test]
fn no_metadata_keeps_legacy_poppler_rectangle() {
    let (_, w, _, _) = sample(HORIZONTAL, "", "別表第１中﹁");
    let w = selected(&w);
    assert_eq!((w.x0, w.y0), (60., 478.));
}
#[test]
fn horizontal_wmode_font_is_not_adjusted() {
    let attrs = META.replace("data-wmode=\"1\"", "data-wmode=\"0\"");
    let (_, w, _, _) = sample(HORIZONTAL, &attrs, "別表第１中﹁");
    assert_eq!((selected(&w).x0, selected(&w).y0), (60., 478.));
}
#[test]
fn vertically_advancing_font_is_not_adjusted() {
    let attrs = META.replace("data-advance-x=\"8\" data-advance-y=\"0\"", "data-advance-x=\"0\" data-advance-y=\"8\"");
    let (_, w, _, _) = sample(HORIZONTAL, &attrs, "別表第１中﹁");
    assert_eq!((selected(&w).x0, selected(&w).y0), (60., 478.));
}
#[test]
fn zero_advance_does_not_prove_a_horizontal_font_run() {
    let attrs = META.replace("data-advance-x=\"8\"", "data-advance-x=\"0\"");
    let (_, w, _, _) = sample(HORIZONTAL, &attrs, "別表第１中﹁");
    assert_eq!(selected(&w).x0, 60.);
}
#[test]
fn malformed_native_rectangle_is_ignored() {
    for attrs in [
        META.replace("data-native-xmax=\"116\"", "data-native-xmax=\"60\""),
        META.replace("data-native-xmin=\"68\"", "data-native-xmin=\"NaN\""),
        META.replace("data-native-ymin=\"482\"", ""),
    ] {
        let (_, w, _, _) = sample(HORIZONTAL, &attrs, "別表第１中﹁");
        assert_eq!((selected(&w).x0, selected(&w).y0), (60., 478.));
    }
}
#[test]
fn sideways_vertical_page_retains_legacy_coordinates() {
    let (_, w, _, s) = sample("縦書き︑縦書き︒﹁本文﹂︑︒のページ", META, "別表第１中﹁");
    assert!(s);
    assert_eq!((selected(&w).x0, selected(&w).y0), (60., 478.));
}
#[test]
fn vertical_majority_page_retains_legacy_coordinates() {
    let html=Html::parse_document(&format!("<page width=\"595\" height=\"842\"><word xmin=\"300\" ymin=\"100\" xmax=\"308\" ymax=\"420\">縦書きの長い本文が上下へ続く通常の縦組みの段落である。</word><word xmin=\"60\" ymin=\"478\" xmax=\"108\" ymax=\"486\" {META}>別表第１中﹁</word></page>"));
    let page = html.select(&Selector::parse("page").unwrap()).next().unwrap();
    let (_, w, _, _, _, _) = page_words_traced(page, 595., 842., None);
    assert_eq!((selected(&w).x0, selected(&w).y0), (60., 478.));
}
#[test]
fn mainly_wmode_one_horizontal_runs_keep_legacy_layout_without_horizontal_base_font() {
    let html = Html::parse_document(&format!(
        "<page width=\"842\" height=\"595\"><word xmin=\"60\" ymin=\"478\" xmax=\"108\" ymax=\"486\" {META}>別表第１中﹁</word></page>"
    ));
    let page = html.select(&Selector::parse("page").unwrap()).next().unwrap();
    let (_, w, _, _, _, _) = page_words_traced(page, 842., 595., None);
    assert_eq!((selected(&w).x0, selected(&w).y0), (60., 478.));
}
fn collision_sample(second_attrs: &str, second_text: &str) -> Vec<Word> {
    let word = |attrs: &str, text: &str| format!("<word xmin=\"60\" ymin=\"478\" xmax=\"108\" ymax=\"486\" {attrs}>{text}</word>");
    let html = Html::parse_document(&format!(
        "<page width=\"842\" height=\"595\"><word xmin=\"300\" ymin=\"100\" xmax=\"620\" ymax=\"108.184\" data-wmode=\"0\">{HORIZONTAL}</word>{}{}{QUOTATION}</page>",
        word(META, "別表第１中﹁"),
        word(second_attrs, second_text)
    ));
    let page = html.select(&Selector::parse("page").unwrap()).next().unwrap();
    page_words_traced(page, 842., 595., None).1
}
#[test]
fn an_unannotated_duplicate_makes_the_coordinate_text_key_ambiguous() {
    let w = collision_sample("", "別表第１中﹁");
    let matching: Vec<_> = w.iter().filter(|w| w.raw == "別表第１中﹁").collect();
    assert_eq!(matching.len(), 2);
    for w in matching {
        assert_eq!((w.x0, w.y0), (60., 478.));
    }
}
#[test]
fn a_horizontal_font_duplicate_does_not_inherit_vertical_metadata() {
    let attrs = META.replace("data-wmode=\"1\"", "data-wmode=\"0\"");
    for w in collision_sample(&attrs, "別表第１中﹁").iter().filter(|w| w.raw == "別表第１中﹁") {
        assert_eq!((w.x0, w.y0), (60., 478.));
    }
}
#[test]
fn conflicting_native_rectangles_are_not_applied() {
    let attrs = META
        .replace("data-native-xmin=\"68\"", "data-native-xmin=\"72\"")
        .replace("data-native-xmax=\"116\"", "data-native-xmax=\"120\"");
    let w = collision_sample(&attrs, "別表第１中﹁");
    for w in w.iter().filter(|w| w.raw == "別表第１中﹁") {
        assert_eq!((w.x0, w.y0), (60., 478.));
    }
}
#[test]
fn identical_annotated_duplicates_can_use_the_same_native_rectangle() {
    let w = collision_sample(META, "別表第１中﹁");
    let matching: Vec<_> = w.iter().filter(|w| w.raw == "別表第１中﹁").collect();
    assert_eq!(matching.len(), 2);
    for w in matching {
        assert_eq!((w.x0, w.y0), (68., 482.));
    }
}
#[test]
fn a_different_unannotated_word_at_the_same_rectangle_stays_independent() {
    let w = collision_sample("", "別表第２中﹁");
    assert_eq!((selected(&w).x0, selected(&w).y0), (68., 482.));
    let other = w.iter().find(|w| w.raw == "別表第２中﹁").unwrap();
    assert_eq!((other.x0, other.y0), (60., 478.));
}

fn quote_case(a: &str, q: &str, b: &str, close_attrs: &str) -> Vec<Word> {
    let quoted = QUOTATION
        .replace("項目名称", q)
        .replace("﹂に改める︒", b)
        .replace("data-wmode=\"1\"", close_attrs);
    let html = Html::parse_document(&format!(
        r#"<page width="842" height="595"><word xmin="300" ymin="100" xmax="620" ymax="108.184" data-wmode="0">{HORIZONTAL}</word><word xmin="60" ymin="478" xmax="108" ymax="486" {META}>{a}</word>{quoted}</page>"#
    ));
    let page = html.select(&Selector::parse("page").unwrap()).next().unwrap();
    page_words_traced(page, 842., 595., None).1
}
#[test]
fn isolated_native_heading_and_margin_symbols_keep_legacy_bounds() {
    for t in ["〇〇", "改正後", "単位円", "全角文字コード表"] {
        let (_, w, _, _) = sample(HORIZONTAL, META, t);
        let x = w.iter().find(|w| w.raw == t).unwrap();
        assert_eq!((x.x0, x.y0), (60., 478.));
    }
}
#[test]
fn open_quote_without_a_native_closing_run_is_insufficient() {
    for attrs in ["", "data-wmode=\"0\""] {
        let w = quote_case("別表第１中﹁", "項目名称", "﹂に改める︒", attrs);
        assert_eq!(selected(&w).x0, 60.);
    }
}
#[test]
fn mathematical_or_latin_quoted_symbols_do_not_prove_japanese_prose() {
    for q in ["x+y", "kg", "１２３４", "表"] {
        assert_eq!(selected(&quote_case("別表第１中﹁", q, "﹂に改める︒", "data-wmode=\"1\"")).x0, 60.);
    }
}
#[test]
fn brackets_without_a_japanese_prose_anchor_are_insufficient() {
    let w = quote_case("﹁", "項目名称", "﹂", "data-wmode=\"1\"");
    assert_eq!(w.iter().find(|w| w.raw == "﹁").unwrap().x0, 60.);
}
#[test]
fn ordinary_font_metadata_and_both_geometric_connections_are_required() {
    for q in [
        QUOTATION.replace("data-wmode=\"0\"", ""),
        QUOTATION.replace("xmin=\"116.2\"", "xmin=\"120\""),
        QUOTATION.replace("xmax=\"148.2\"", "xmax=\"144\""),
        QUOTATION.replace("ymin=\"482.8\"", "ymin=\"484.8\""),
        QUOTATION.replace("ymax=\"491\"", "ymax=\"495\""),
    ] {
        let html = Html::parse_document(&format!(
            r#"<page width="842" height="595"><word xmin="300" ymin="100" xmax="620" ymax="108.184" data-wmode="0">{HORIZONTAL}</word><word xmin="60" ymin="478" xmax="108" ymax="486" {META}>別表第１中﹁</word>{q}</page>"#
        ));
        let page = html.select(&Selector::parse("page").unwrap()).next().unwrap();
        let w = page_words_traced(page, 842., 595., None).1;
        assert_eq!(selected(&w).x0, 60.);
    }
}
