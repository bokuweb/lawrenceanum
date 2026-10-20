//! Issue #227 の原本から、隣の記事・列・式への混線を防ぐ。

fn text(pdf: &[u8]) -> String {
    kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap()).text
}

#[test]
fn previous_table_tail_stays_before_the_next_notice_heading() {
    let out = text(include_bytes!("archive/pages/20260907h01784/20260907h017840004.pdf"));
    assert!(out.contains("移転のため\n〇厚生労働省告示第三百三十五号"), "{out}");
}

#[test]
fn fullwidth_latin_company_name_does_not_mix_with_the_liquidator() {
    let out = text(include_bytes!("archive/pages/20260731g00173/20260731g001730101.pdf"));
    let compact: String = out.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(compact.contains("株式会社ＮＯＳＰＩＣＥ，ＮＯＬＩＦＥ．代表清算人他力定信"), "{out}");
}

#[test]
fn partial_rules_separate_court_personnel_and_public_works_notices() {
    let out = text(include_bytes!("archive/pages/20260730h01758/20260730h017580006.pdf"));
    assert!(out.contains("最高裁判所\n宮崎地方裁判所判事補兼宮崎"), "{out}");
    assert!(out.contains("官庁報告\n官庁事項\n国営土地改良事業の工事完了の公告"), "{out}");
    assert!(out.contains("下記に掲げる国営土地改良事業の工事は、令和\n８年２月25日"), "{out}");
    // 部分横線が縦組みの字の中心で終わっていても、近くの溝で分けて任免ごとの本文を保つ。
    assert!(out.contains("人事異動\n内閣\n（横浜地方裁判所判事・横浜簡"), "{out}");
    assert!(!out.contains("内閣　内閣事務官"), "{out}");
    // 反対側の国家試験の本文まで行ごとに混ぜない。
    assert!(out.contains("人事院規則８－12（職員の任免）第14条第１項\nの規定に基づき、下記に掲げる採用試験の結果に\n基づいて作成された採用候補者名簿の有効期間"), "{out}");
}

#[test]
fn interest_formulas_keep_numerators_and_denominators_together() {
    let out = text(include_bytes!("archive/pages/20260909g00201/20260909g002010004.pdf"));
    assert!(out.contains("額面金額の総額×(2.2/100)×(60/365)"), "{out}");
    assert!(out.contains("額面金額×(2.2/100)×(１/２)"), "{out}");
}

#[test]
fn two_changes_in_one_record_keep_their_separate_rows() {
    let out = text(include_bytes!("archive/pages/20260901g00195/20260901g001950021.pdf"));
    assert!(!out.contains("東京都港区芝四－一六－二吉川知行"), "{out}");
    assert!(!out.contains("一三二二中村岳夫"), "{out}");
    assert!(out.contains("会計責任者の吉川知行"), "{out}");
}
