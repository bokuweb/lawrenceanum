//! 項目の前の頁の官庁報告の欄見出しを文脈にして、欄の公示の標題で記事を分ける（号外第195号）。

use kanpo_parser::PageInput;

fn page(n: u32) -> Vec<PageInput> {
    let path = format!("{}/tests/data/report-context263af/20260901g0019500{n}.pdf", env!("CARGO_MANIFEST_DIR"));
    kanpo_parser::page_inputs(&std::fs::read(path).unwrap()).unwrap()
}

fn pages(range: std::ops::RangeInclusive<u32>) -> Vec<PageInput> {
    range.flat_map(page).collect()
}

#[test]
fn a_report_field_on_an_earlier_page_splits_the_notices_of_the_item() {
    // 目次「あん摩マッサージ指圧師国家試験の施行（同）」は41〜42ページ。欄見出し「国家試験」は34ページにある。
    let title = "あん摩マッサージ指圧師国家試験の施行（同）";
    let items = pages(41..=42);
    let alone = kanpo_parser::analyze_item(&items);
    let i = kanpo_parser::pick_article(title, &alone.articles).unwrap();
    assert!(!alone.articles[i].text.starts_with("あん摩マッサージ指圧師国家試験の施行"), "項目の頁だけでは欄が分からず、前の公示の記事に続く");

    let a = kanpo_parser::analyze_item_in_context(&pages(34..=40), &items);
    let i = kanpo_parser::pick_article(title, &a.articles).unwrap();
    assert!(a.articles[i].text.starts_with("あん摩マッサージ指圧師国家試験の施行"), "{}", a.articles[i].text);
    let j = kanpo_parser::pick_article("視能訓練士国家試験の施行（同）", &a.articles).unwrap();
    assert_ne!(i, j);
    // 頁の全文は項目の頁だけのもの。
    assert_eq!(a.text, alone.text);
}

#[test]
fn context_is_not_used_when_the_item_pages_carry_a_section_label() {
    // 項目の頁（34ページ）が欄見出し「国家試験」を持つので、ほかの頁は足さない。
    let items = pages(34..=35);
    let alone = kanpo_parser::analyze_item(&items);
    let a = kanpo_parser::analyze_item_in_context(&pages(38..=40), &items);
    let texts = |a: &kanpo_parser::Analysis| a.articles.iter().map(|x| x.text.clone()).collect::<Vec<_>>();
    assert_eq!(texts(&a), texts(&alone));
}
