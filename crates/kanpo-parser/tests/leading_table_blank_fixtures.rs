use kanpo_parser::lines::{HRule, PageRules, VRule};
fn numeric_input(filled_first: bool) -> kanpo_parser::PageInput {
    let mut x = String::from("<html><body><doc><page width=\"842\" height=\"595\"><word xMin=\"44\" yMin=\"260\" xMax=\"54\" yMax=\"270\">官</word>");
    let mut data = vec![
        (1, 0, "10,000"),
        (2, 0, "20,000"),
        (1, 1, "30,000"),
        (2, 1, "40,000"),
        (0, 2, "50,000"),
        (1, 2, "60,000"),
        (2, 2, "70,000"),
    ];
    if filled_first {
        data.push((0, 0, "5,000"));
    }
    for (c, r, t) in data {
        let px = 75 + c * 70;
        let y = 105 + r * 20;
        x += &format!("<word xMin=\"{px}\" yMin=\"{y}\" xMax=\"{}\" yMax=\"{}\">{t}</word>", px + 45, y + 8);
    }
    x += "</page></doc></body></html>";
    let rules = PageRules {
        vertical: [70., 140., 210., 280.].into_iter().map(|x| VRule { x, y0: 100., y1: 160. }).collect(),
        horizontal: [100., 120., 140., 160.].into_iter().map(|y| HRule { y, x0: 70., x1: 280. }).collect(),
        enclosures: vec![],
        short_fraction_bars: vec![],
    };
    kanpo_parser::PageInput { xhtml: x, rules: Some(rules) }
}
fn prose_input() -> kanpo_parser::PageInput {
    kanpo_parser::PageInput{xhtml:"<html><body><doc><page width=\"842\" height=\"595\"><word xMin=\"44\" yMin=\"260\" xMax=\"54\" yMax=\"270\">官</word><word xMin=\"75\" yMin=\"100\" xMax=\"195\" yMax=\"108\">  Ordinary prose.  </word></page></doc></body></html>".into(),rules:None}
}
fn actual() -> kanpo_parser::Analysis {
    kanpo_parser::analyze_item(&kanpo_parser::page_inputs(include_bytes!("archive/pages/20261016g00228/20261016g002280473.pdf")).unwrap())
}
#[test]
fn actual_numeric_continuation_retains_first_empty_column_in_page_text() {
    let a = actual();
    assert!(a.pages[0].text.starts_with("\u{3000}177,738,338"));
}
#[test]
fn actual_numeric_continuation_retains_first_empty_column_in_analysis_text() {
    let a = actual();
    assert!(a.text.starts_with("\u{3000}177,738,338"));
}
#[test]
fn another_first_row_with_two_values_keeps_its_initial_column() {
    let a = kanpo_parser::analyze_item(&[numeric_input(false)]);
    assert_eq!(
        a.text,
        "\u{3000}10,000\u{3000}20,000\n\u{3000}30,000\u{3000}40,000\n50,000\u{3000}60,000\u{3000}70,000"
    );
}
#[test]
fn table_beginning_a_later_page_keeps_its_own_first_column() {
    let a = kanpo_parser::analyze_item(&[prose_input(), numeric_input(false)]);
    assert!(a.text.contains("Ordinary prose.\n\u{3000}10,000\u{3000}20,000"));
}
#[test]
fn ordinary_prose_still_trims_boundary_whitespace() {
    let a = kanpo_parser::analyze_item(&[prose_input()]);
    assert_eq!(a.text, "Ordinary prose.");
    assert_eq!(a.pages[0].text, "Ordinary prose.");
}
#[test]
fn second_and_later_table_rows_already_keep_blank_columns() {
    let a = actual();
    assert!(a.text.lines().nth(1).unwrap().starts_with("\u{3000}\u{3000}△177,738,338"));
    let a = kanpo_parser::analyze_item(&[numeric_input(false)]);
    assert_eq!(a.text.lines().nth(1), Some("\u{3000}30,000\u{3000}40,000"));
}
#[test]
fn a_populated_first_cell_does_not_acquire_a_blank_column() {
    let a = kanpo_parser::analyze_item(&[numeric_input(true)]);
    assert_eq!(a.text.lines().next(), Some("5,000\u{3000}10,000\u{3000}20,000"));
}
#[test]
fn structural_render_and_article_text_already_preserve_the_original_separator() {
    let a = actual();
    let line = &a.pages[0].regions[0].lines[0];
    let t = line.table.as_ref().unwrap();
    assert_eq!(t.cols, 6);
    assert_eq!(t.render(), line.text);
    assert!(line.text.starts_with("\u{3000}177,738,338"));
    assert!(a.articles[0].text.starts_with("\u{3000}177,738,338"));
    assert_eq!(
        t.cells.iter().filter(|c| c.row == 0).map(|c| c.col).collect::<std::collections::BTreeSet<_>>(),
        [1, 4, 5].into_iter().collect()
    );
}
