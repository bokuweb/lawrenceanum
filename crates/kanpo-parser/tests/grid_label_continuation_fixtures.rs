//! 左右に並ぶ貸借対照表の科目折返しを同じ科目へ戻す。
#[test]
fn balance_sheet_wrapped_accounts_keep_their_amounts() {
    let pdf = include_bytes!("archive/pages/20260831g00194/20260831g001940076.pdf");
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let t = a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .find(|t| t.cells.iter().any(|c| c.text.contains("未収介護給付費・")))
        .unwrap();
    for (label, amount) in [
        ("１　未払介護給付費交付金", "122,567,217"),
        ("２　未収介護給付費・地域支援事業支援納付金", "238,954,768"),
        ("２　未払地域支援事業支援交付金", "8,067,480"),
    ] {
        let c = t
            .cells
            .iter()
            .find(|c| c.text == label)
            .unwrap_or_else(|| panic!("missing {label}: {}", a.text));
        let value = t.cells.iter().filter(|v| v.row == c.row && v.col > c.col).min_by_key(|v| v.col).unwrap();
        assert_eq!(value.text, amount, "{label}: {}", a.text);
    }
}
