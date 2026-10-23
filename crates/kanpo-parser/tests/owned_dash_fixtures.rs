use kanpo_parser::analyze_item;
fn extract(stem: &str) -> String {
    let (pdf, expected): (&[u8], &str) = match stem {
        "20260728g001680055" => (
            include_bytes!("archive/pages/20260728g00168/20260728g001680055.pdf"),
            include_str!("archive/expected-owned-dashes/20260728g001680055.txt"),
        ),
        "20261016g002280338" => (
            include_bytes!("archive/pages/20261016g00228/20261016g002280338.pdf"),
            include_str!("archive/expected-owned-dashes/20261016g002280338.txt"),
        ),
        "20261016g002280006" => (
            include_bytes!("archive/pages/20261016g00228/20261016g002280006.pdf"),
            include_str!("archive/expected-owned-dashes/20261016g002280006.txt"),
        ),
        "20260728g001680054" => (
            include_bytes!("archive/pages/20260728g00168/20260728g001680054.pdf"),
            include_str!("archive/expected-owned-dashes/20260728g001680054.txt"),
        ),
        "20260728g001680056" => (
            include_bytes!("archive/pages/20260728g00168/20260728g001680056.pdf"),
            include_str!("archive/expected-owned-dashes/20260728g001680056.txt"),
        ),
        "20261016g002280337" => (
            include_bytes!("archive/pages/20261016g00228/20261016g002280337.pdf"),
            include_str!("archive/expected-owned-dashes/20261016g002280337.txt"),
        ),
        "20261016g002280339" => (
            include_bytes!("archive/pages/20261016g00228/20261016g002280339.pdf"),
            include_str!("archive/expected-owned-dashes/20261016g002280339.txt"),
        ),
        "20261016g002280005" => (
            include_bytes!("archive/pages/20261016g00228/20261016g002280005.pdf"),
            include_str!("archive/expected-owned-dashes/20261016g002280005.txt"),
        ),
        "20261016g002280007" => (
            include_bytes!("archive/pages/20261016g00228/20261016g002280007.pdf"),
            include_str!("archive/expected-owned-dashes/20261016g002280007.txt"),
        ),
        _ => panic!("unknown fixture"),
    };
    let input = kanpo_parser::page_inputs(pdf).unwrap();
    let text = analyze_item(&input).text;
    assert_eq!(text, expected, "full page text changed: {stem}");
    text
}
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
#[test]
fn one_amount_column_retains_fixed_asset_zero() {
    let t = extract("20260728g001680055");
    assert!(t.lines().any(|l| compact(l) == "固定資産－"), "{t}");
    assert!(t.contains("11,820"));
}
#[test]
fn unitless_zero_rows_follow_yen_anchors() {
    let t = extract("20261016g002280338");
    for label in [
        "新規取得による増加額",
        "見積りの変更による減少額",
        "過去勤務費用の当期発生額",
        "過去勤務費用の当期の費用処理額",
        "臨時に支払った割増退職金",
    ] {
        assert!(
            t.lines().any(|l| compact(l) == format!("{label}－")),
            "missing {label}: {t}"
        );
    }
}
#[test]
fn middle_profit_amount_column_retains_zero() {
    let t = extract("20261016g002280006");
    assert!(t.lines().any(|l| compact(l) == "積立金－"), "{t}");
    assert!(t
        .lines()
        .any(|l| compact(l) == "教育研究向上及び組織運営改善積立金1,4611,4611,461"));
}

#[test]
fn adjacent_20260728g001680054_is_unchanged() {
    extract("20260728g001680054");
}

#[test]
fn adjacent_20260728g001680056_is_unchanged() {
    extract("20260728g001680056");
}

#[test]
fn adjacent_20261016g002280337_is_unchanged() {
    extract("20261016g002280337");
}

#[test]
fn adjacent_20261016g002280339_is_unchanged() {
    extract("20261016g002280339");
}

#[test]
fn adjacent_20261016g002280005_is_unchanged() {
    extract("20261016g002280005");
}

#[test]
fn adjacent_20261016g002280007_is_unchanged() {
    extract("20261016g002280007");
}
