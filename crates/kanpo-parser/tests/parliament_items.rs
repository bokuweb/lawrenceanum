//! 2026-10-19 本紙第1810号7頁。項目の境界と片側だけの上下段を原本PDFで検証する。
use kanpo_parser::{analyze_item, page_inputs, pick_article};

#[test]
fn shared_page_preserves_each_toc_items_body() {
    let pages = page_inputs(include_bytes!("fixtures/parliament-personnel-honours.pdf")).unwrap();
    let a = analyze_item(&pages);
    insta::assert_snapshot!(a.articles.iter().map(|a| a.text.as_str()).collect::<Vec<_>>().join("\n\n---\n\n"));
    let item = |title| &a.articles[pick_article(title, &a.articles).expect(title)].text;
    let parliament = item("国会事項");
    assert!(parliament.contains("田村智子外三名提出"));
    assert!(!parliament.contains("財務大臣"));
    let cabinet = item("内閣");
    assert!(cabinet.contains("〇財務大臣臨時代理"));
    assert!(cabinet.contains("金融）事務代理を命ずる"));
    assert!(!cabinet.contains("叙位"));
    let honours = item("叙位・叙勲");
    assert!(honours.contains("旭日小綬章を授ける"));
    assert!(honours.contains("瑞宝単光章を授ける"));
    assert!(honours.contains("松尾秀保"));
    assert!(!honours.contains("皇室事項"));
    let imperial = item("皇室事項");
    assert!(imperial.contains("第八十回国民スポーツ大会"));
    assert!(imperial.contains("午後零時二十三分還幸啓になった。"));
    assert!(!imperial.contains("松元登美子"));
    assert!(!imperial.contains("水先人試験"));
    let exam = item("水先人試験の施行");
    assert!(exam.contains("受験申請書"));
    assert!(!exam.contains("行幸啓"));
}
