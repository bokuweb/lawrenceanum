use kanpo_parser::analyze_item;
fn analyze(page: &str) -> kanpo_parser::Analysis {
    let pdf: &[u8] = match page {
        "20261020g002300062" => {
            include_bytes!("archive/pages/20261020g00230/20261020g002300062.pdf")
        }
        "20261020g002300061" => {
            include_bytes!("archive/pages/20261020g00230/20261020g002300061.pdf")
        }
        "20261020g002300063" => {
            include_bytes!("archive/pages/20261020g00230/20261020g002300063.pdf")
        }
        "20261016g002280540" => {
            include_bytes!("archive/pages/20261016g00228/20261016g002280540.pdf")
        }
        _ => panic!("unknown fixture"),
    };
    analyze_item(&kanpo_parser::page_inputs(pdf).unwrap())
}
fn compact(t: &str) -> String {
    t.chars().filter(|c| !c.is_whitespace()).collect()
}
#[test]
fn framed_continuation_amounts_return_to_their_accounts() {
    let a = analyze("20261020g002300062");
    for row in [
        "経常収益合計829,726,151",
        "経常損失△38,778,714",
        "臨時損失合計172,559",
        "臨時利益合計172,559",
        "当期純損失△38,778,714",
        "当期総損失△38,778,714",
    ] {
        assert!(
            a.text.lines().any(|l| compact(l) == row),
            "missing {row}: {}",
            a.text
        )
    }
    let tail = a.text.find("損失の処理に関する書類").unwrap();
    assert!(a.text[..tail].contains("当期総損失"));
    assert!(!a.text[tail..].contains("829,726,151"));
}
#[test]
fn loss_statement_amounts_keep_their_own_table_and_company_boundary() {
    let a = analyze("20261020g002300062");
    for row in [
        "Ⅰ当期未処理損失38,778,714",
        "当期総損失38,778,714",
        "Ⅱ損失処理額",
        "積立金取崩額38,778,71438,778,714",
    ] {
        assert!(
            a.text.lines().any(|l| compact(l) == row),
            "missing {row}: {}",
            a.text
        )
    }
    assert!(a
        .text
        .lines()
        .any(|l| compact(l).starts_with("Ⅲ次期繰越欠損金")));
    let stop = a.text.find("会社その他の公告").unwrap();
    assert!(a.text[..stop].contains("Ⅲ次期繰越欠損金"));
    for company in [
        "やまびこ北海道地区農機商業協同組合",
        "株式会社アズ水沢",
        "内外計画株式会社",
        "有限会社ヤマヤ水産",
        "特定非営利活動法人泉里会",
    ] {
        assert!(a.text[stop..].contains(company));
    }
}
#[test]
fn neighboring_financial_pages_are_unchanged() {
    for page in [
        "20261020g002300061",
        "20261020g002300063",
        "20261016g002280540",
    ] {
        let a = analyze(page);
        let expected = match page {
            "20261020g002300061" => {
                include_str!("archive/expected-framed-amount-rows/20261020g002300061.txt")
            }
            "20261020g002300063" => {
                include_str!("archive/expected-framed-amount-rows/20261020g002300063.txt")
            }
            "20261016g002280540" => {
                include_str!("archive/expected-framed-amount-rows/20261016g002280540.txt")
            }
            _ => unreachable!(),
        };
        assert_eq!(a.text, expected, "{page}")
    }
}
