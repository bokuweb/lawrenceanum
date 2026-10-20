//! 上下の本文が似た通常の段組に新旧対照表の欄見出しを補わない。

fn plain_page(pdf: &[u8]) -> String {
    let xhtml = kanpo_parser::bbox_xhtml(pdf).unwrap();
    let rules = kanpo_parser::page_rules(pdf).unwrap();
    let analysis = kanpo_parser::analyze_with_rules(&xhtml, &[Some(rules)]);
    assert_ne!(analysis.pages[0].layout, "headerless_shinkyu");
    assert!(!analysis.text.lines().any(|l| matches!(l.trim(), "改正後" | "改正前")), "{}", analysis.text);
    analysis.text
}

#[test]
fn investigation_report_keeps_continuing_kana_headings_in_plain_tiers() {
    let text = plain_page(include_bytes!("archive/pages/20260724g00164/20260724g001640062.pdf"));
    let text: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let korea = text.find("イ韓国").unwrap();
    let china = text.find("ロ中国").unwrap();
    assert!(korea < china);
    assert!(text.contains("サンプリング調査非対象韓国生産者"));
}

#[test]
fn fee_table_keeps_continuing_paragraph_and_item_references() {
    let text = plain_page(include_bytes!("archive/pages/20260729g00169/20260729g001690035.pdf"));
    assert!(text.find("第四項第七号").unwrap() < text.find("第四項第八号").unwrap());
    assert!(text.find("第四項第十四号").unwrap() < text.find("第五項第一号").unwrap());
}
