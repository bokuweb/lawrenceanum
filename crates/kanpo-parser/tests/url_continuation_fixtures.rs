//! 横倒しURLの末尾区切りU+0332も折返しと認め、次の和文見出しを混ぜない。
#[test]
fn wrapped_url_low_line_tail_precedes_the_next_numbered_heading() {
    let pdf = include_bytes!("archive/pages/20260821g00187/20260821g001870087.pdf");
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    assert!(
        a.text.contains("business/wind̲power/kankyo̲assessment/\n五、意見書の提出\n環境保全"),
        "{}",
        a.text
    );
    assert!(a.text.contains("電子縦覧https://www.tepco.co.jp/rp/"));
    assert!(!a.text.contains("五、意見書の提出assessment/"));
}
