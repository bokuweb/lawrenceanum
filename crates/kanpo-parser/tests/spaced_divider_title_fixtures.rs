//! 字間を空けた全幅標題の上は、段を分けてから損益表の科目と金額を読む。
#[test]
fn unruled_profit_and_loss_keeps_amounts_beside_their_accounts() {
    let pdf = include_bytes!("archive/pages/20261016g00228/20261016g002280540.pdf");
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let expected = [
        "職員人件費　1,027,547　5,540,293",
        "一般管理費　192,585", "支払利息　1,032　1,032", "雑損　1,141",
        "経常費用合計　5,735,052", "運営費交付金収益（注１）　3,404,265",
        "授業料収益（注１）　1,416,687", "入学金収益（注１）　197,245",
        "検定料収益　41,435", "受託研究収益（注１）　1,226",
        "共同研究収益（注１）　500", "受託事業等収益（注１）　8,170",
        "寄附金収益（注１）　64,959", "施設費収益（注１）　78,160",
        "補助金等収益（注１）　539,129", "その他　13,643　17,570",
        "有価証券利息　980　4,237",
    ];
    for row in expected { assert!(a.text.lines().any(|l|l==row), "missing {row}: {}", a.text); }
    assert!(a.text.find("有価証券利息").unwrap()<a.text.find("雑益").unwrap());
    assert!(a.text.find("当期支出額").unwrap()<a.text.find("純資産変動計算書").unwrap());
}
