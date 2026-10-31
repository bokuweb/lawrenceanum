use super::*;
fn word(id: usize, x: f32, x1: f32, y: f32, text: &str) -> Col {
    Col { id, x, x1, y, y1: y + 8., text: text.into() }
}
fn glyphs(font: usize) -> trace::ContentStyles {
    trace::ContentStyles::Glyphs([(font, 8)].into_iter().collect())
}
// Two balanced columns (rows at 47, 60, 73), the left column's bottom line at 86 and a
// full-width tier from 99. Fonts: body 0, the bottom line `title`, the full-width tier 4.
fn page(title: usize) -> (Vec<Col>, Vec<trace::ContentStyles>) {
    let ws = vec![
        word(0, 61., 221., 47., "国土地理院防災業務計画の修正について"),
        word(1, 245., 421., 47., "二　修正年月日　令和８年７月27日"),
        word(2, 61., 117., 60., "一　修正の目的"),
        word(3, 245., 421., 60., "三　修正の要旨"),
        word(4, 69., 205., 73., "上を図るため、所要の修正を行った。"),
        word(5, 253., 405., 73., "害対策要領等で定めることを明確化した。"),
        word(6, 61., 133., 86., "四国地方整備局公示"),
        word(7, 69., 421., 99., "道路法（昭和二十七年法律第百八十号）第三十七条第一項の規定に基づき、"),
        word(8, 61., 357., 112., "区域を指定することとしたので、同条第三項の規定に基づき次のとおり公示する。"),
    ];
    let styles = (0..ws.len()).map(|i| glyphs(match i { 6 => title, 7 | 8 => 4, _ => 0 })).collect();
    (ws, styles)
}
#[test]
fn a_title_on_the_left_column_s_bottom_line_heads_the_full_width_tier_below() {
    let (ws, styles) = page(3);
    // band_cut alone cuts below the title, so the title would be read before the right column.
    let below_title = (94. + 99.) / 2.;
    let cut = bottom_title_cut(&ws, &[], below_title, &styles).expect("cut above the title");
    assert!(cut > 81. && cut < 86., "{cut}");
}
#[test]
fn a_body_line_left_over_in_the_left_column_stays_there() {
    // An odd number of body lines leaves one more on the left, in the body font.
    let (ws, styles) = page(0);
    assert_eq!(bottom_title_cut(&ws, &[], 96.5, &styles), None);
}
#[test]
fn a_line_in_the_full_width_tier_s_font_is_not_its_title() {
    let (ws, styles) = page(4);
    assert_eq!(bottom_title_cut(&ws, &[], 96.5, &styles), None);
}
#[test]
fn unknown_fonts_do_not_move_the_cut() {
    let (ws, mut styles) = page(3);
    styles[6] = trace::ContentStyles::Unknown;
    assert_eq!(bottom_title_cut(&ws, &[], 96.5, &styles), None);
    assert_eq!(bottom_title_cut(&ws, &[], 96.5, &[]), None);
}
#[test]
fn a_bottom_line_beside_the_right_column_is_not_below_it() {
    // The right column runs as far down as the left one.
    let (mut ws, styles) = page(3);
    ws[5].y = 86.;
    ws[5].y1 = 94.;
    assert_eq!(bottom_title_cut(&ws, &[], 96.5, &styles), None);
    // A glyph box reaching 2pt above the right column's bottom still lies below it.
    let (mut ws, styles) = page(3);
    ws[6].y = 79.;
    ws[6].y1 = 87.;
    assert!(bottom_title_cut(&ws, &[], 93., &styles).is_some());
    let (mut ws, styles) = page(3);
    ws[6].y = 76.;
    ws[6].y1 = 84.;
    assert_eq!(bottom_title_cut(&ws, &[], 96.5, &styles), None);
}
#[test]
fn an_indented_bottom_line_is_not_a_title() {
    let (mut ws, styles) = page(3);
    ws[6].x = 85.;
    assert_eq!(bottom_title_cut(&ws, &[], 96.5, &styles), None);
}
#[test]
fn a_narrow_tier_below_does_not_take_the_title() {
    let (mut ws, styles) = page(3);
    ws[7].x1 = 230.;
    ws[8].x1 = 230.;
    assert_eq!(bottom_title_cut(&ws, &[], 96.5, &styles), None);
}
#[test]
fn a_title_under_a_right_only_row_is_compared_with_the_nearest_left_line() {
    // 号外第218号25ページ: the left column ends in a table above the title, so the row right
    // above the title has only the right column's words.
    let (mut ws, mut styles) = page(3);
    ws.retain(|w| w.id != 4);
    ws.push(word(9, 253., 405., 78., "変更後　17.5過りん酸石灰"));
    styles.push(glyphs(0));
    let cut = bottom_title_cut(&ws, &[], 96.5, &styles).expect("cut above the title");
    assert!(cut <= 86. && cut > 80., "{cut}");
    // The nearest left line in the title's font: a body line left over, not a title.
    styles[2] = glyphs(3);
    assert_eq!(bottom_title_cut(&ws, &[], 96.5, &styles), None);
}
