//! 本文がアウトラインの区画でも、文字層のある見出しを隣の公告へ混ぜない。

const PDF: &[u8] = include_bytes!("archive/pages/20260722h01752/20260722h017520029.pdf");

#[test]
fn a_rule_in_a_large_empty_gap_separates_the_court_heading_from_a_merger() {
    let out = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(PDF).unwrap());
    let lines: Vec<_> = out.text.lines().collect();
    let heading = lines.iter().position(|l| *l == "所有者不明土地管理命令に関").unwrap();
    let merger = lines.iter().position(|l| *l == "合併公告").unwrap();
    let date = lines.iter().position(|l| *l == "令和八年七月二十二日").unwrap();
    let address = lines.iter().position(|l| *l == "秋田県秋田市大町二丁目五番一号").unwrap();
    assert!(heading < merger && merger < date && date < address, "{}", out.text);
    assert!(lines.contains(&"（甲）特定非営利活動法人きらら"));
}

#[test]
fn empty_space_without_a_real_rule_does_not_create_a_ruled_tier() {
    let mut pages = kanpo_parser::page_inputs(PDF).unwrap();
    pages[0].rules.as_mut().unwrap().vertical.clear();
    let out = kanpo_parser::analyze_item(&pages);
    assert!(out.text.lines().any(|l| l.contains("所有者不明土地管理命令に関") && l.contains("秋田県")));
}
