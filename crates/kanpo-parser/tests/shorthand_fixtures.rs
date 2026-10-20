#[test]
fn an_omitted_article_stays_with_its_original_shorthand() {
    let pdf = include_bytes!("archive/pages/20260911g00203/20260911g002030026.pdf");
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let matches: Vec<_> = a
        .text
        .lines()
        .filter(|l| l.contains("第三十七条の三"))
        .collect();
    assert_eq!(matches.len(), 2);
    assert!(matches.iter().all(|l| l.contains("（略）")), "{matches:?}");
    let p = &a.pages[0];
    let owners: Vec<_> = p
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter(|l| l.words.contains(&24) || l.words.contains(&25))
        .collect();
    assert_eq!(owners.len(), 1);
    assert_eq!(owners[0].words, vec![24, 25]);
}
#[test]
fn the_upper_unabridged_body_and_three_new_article_marks_remain() {
    let pdf = include_bytes!("archive/pages/20260911g00203/20260911g002030026.pdf");
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    assert_eq!(a.text.matches("（新設）").count(), 3);
    assert!(a.text.contains("第二十九条の三事業主は、"));
    assert!(a.text.contains("第三十七条の四法附則"));
    assert!(a.text.contains("第三十七条の五法附則"));
}
