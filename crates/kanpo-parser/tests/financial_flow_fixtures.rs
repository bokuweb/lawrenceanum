use kanpo_parser::{analyze_item, page_inputs};
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
#[test]
fn account_categories_keep_their_cashflow_columns_in_the_same_table_rows() {
    let a = analyze_item(&page_inputs(include_bytes!("archive/pages/20260902g00196/20260902g001960133.pdf")).unwrap());
    let tables: Vec<_> = a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .collect();
    for (label, values) in [
        ("一般会計", ["△3,090", "△136,810", "△123,796", "△113,355", "△38,445", "△151,800"]),
        ("財政投融資", ["△2,609", "18,314", "33,745", "7,336", "2,491", "9,827"]),
        ("外為", ["117,464", "△26,476", "91,008", "1,617", "8,191", "9,807"]),
        ("保険", ["△74,104", "80,891", "6,787", "6,787", "27,755", "34,542"]),
        ("その他", ["△34,211", "44,786", "△19,303", "67,814", "57,583", "125,397"]),
        ("小計", ["3,450", "△19,294", "△11,561", "△29,802", "57,575", "27,774"]),
    ] {
        let rows: Vec<_> = tables
            .iter()
            .flat_map(|t| t.cells.iter().filter(move |c| compact(&c.text) == label).map(move |c| (*t, c.row)))
            .collect();
        assert!(
            rows.iter().any(|(t, r)| {
                [2, 3, 5, 7, 8, 9].into_iter().zip(values).all(|(col, value)| {
                    let cells: Vec<_> = t.cells.iter().filter(|c| c.row == *r && c.col == col).collect();
                    cells.len() == 1 && compact(&cells[0].text) == value
                })
            }),
            "category {label} detached from its original values: {}",
            a.text
        );
    }
}
#[test]
fn neighboring_financial_pages_preserve_their_full_output() {
    for (pdf, expected) in [
        (
            include_bytes!("archive/pages/20260902g00196/20260902g001960132.pdf").as_slice(),
            include_str!("fixtures/financial_flow/196132-before.txt"),
        ),
        (
            include_bytes!("archive/pages/20260902g00196/20260902g001960134.pdf").as_slice(),
            include_str!("fixtures/financial_flow/196134-before.txt"),
        ),
    ] {
        assert_eq!(analyze_item(&page_inputs(pdf).unwrap()).text, expected);
    }
}

// Expected rows read against the complete original PDF, including all nested
// detail records. A master-row-only assertion would miss the original failure.
const DETAILS: &[(&str, &[&str], &[&str])] = &[
    (
        "一般会計",
        &["法人税等△2,242", "納付金18,301", "その他45", "計16,103"],
        &["建設国債・特例国債発行45,808", "その他△35,367", "計10,441"],
    ),
    (
        "財政投融資",
        &["長期国債19,530", "その他△1,491", "計18,039"],
        &[
            "財投債発行9,390",
            "〃償還△35,798",
            "財政融資資金証券発行－",
            "〃償還－",
            "国庫余裕金繰替－",
            "計△26,408",
        ],
    ),
    (
        "外為",
        &["基金証券償還－", "その他19", "計19"],
        &[
            "外国為替資金証券発行2,273,001",
            "〃償還△2,326,392",
            "国庫余裕金繰替△36,000",
            "計△89,391",
        ],
    ),
    ("保険", &["－"], &["財政融資借入－", "国庫余裕金繰替－", "計－"]),
    (
        "その他",
        &[
            "長期国債(国整)△22,963",
            "割引料－",
            "国債利子△8,647",
            "貨幣△145",
            "その他1,875",
            "計△29,879",
        ],
        &[
            "借換債発行364,620",
            "復興債発行－",
            "脱炭素成長型経済構造移行債発行1,182",
            "子ども・子育て支援特例公債発行4,378",
            "半導体・ＡＩ債発行7,872",
            "普通国債償還△363,049",
            "財務省証券発行539,000",
            "〃償還△465,000",
            "食糧証券発行－",
            "〃償還－",
            "石油証券発行9,304",
            "〃償還△11,604",
            "原子力損害賠償支援証券発行2,690",
            "〃償還－",
            "育児休業等給付証券発行－",
            "〃償還－",
            "国庫余裕金繰替－",
            "その他△2,277",
            "計87,117",
        ],
    ),
    ("小計", &["4,283"], &["△18,241"]),
];
fn target() -> kanpo_parser::Analysis {
    analyze_item(&page_inputs(include_bytes!("archive/pages/20260902g00196/20260902g001960133.pdf")).unwrap())
}
#[test]
fn all_nested_details_stay_with_their_own_master_category_and_amount() {
    let a = target();
    for (label, expected_c, expected_e) in DETAILS {
        let matches: Vec<_> = a.pages[0]
            .regions
            .iter()
            .flat_map(|r| &r.lines)
            .filter_map(|l| l.table.as_ref())
            .filter_map(|t| t.cells.iter().find(|c| compact(&c.text) == *label).map(|c| (t, c.row)))
            .collect();
        assert_eq!(matches.len(), 1, "one owned category for {label}");
        let (t, row) = matches[0];
        // The narrow shared category occupies logical col 0; native content has nine
        // fields, with the ordinary category spanning cols 0..2 when no shared label.
        assert_eq!(t.cols, 10);
        for (col, expected) in [(4, *expected_c), (6, *expected_e)] {
            let cells: Vec<_> = t.cells.iter().filter(|c| c.row == row && c.col == col).collect();
            assert_eq!(cells.len(), 1, "{label} detail col {col}");
            let actual: Vec<_> = cells[0].text.lines().map(compact).filter(|s| !s.is_empty()).collect();
            assert_eq!(actual, expected, "{label} detail col {col}");
        }
    }
}
#[test]
fn shared_category_keeps_all_five_original_ids_and_spans_only_four_accounts() {
    let a = target();
    let found: Vec<_> = a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .flat_map(|t| t.cells.iter().filter(|c| compact(&c.text) == "特別会計等").map(move |c| (t, c)))
        .collect();
    assert_eq!(found.len(), 1);
    let (t, c) = found[0];
    assert_eq!((c.row, c.col, c.rowspan, c.colspan), (2, 0, 4, 1));
    assert_eq!(c.words, [457, 458, 459, 460, 461]);
    for (id, ch) in [(457, "特"), (458, "別"), (459, "会"), (460, "計"), (461, "等")] {
        let w = &a.pages[0].words[id];
        assert_eq!(w.id, id);
        assert_eq!(w.raw, ch);
        assert_eq!(w.text, ch);
        assert!((w.x0 - 73.).abs() < 0.02 && (w.x1 - 81.).abs() < 0.02);
    }
    let covered: Vec<_> = t
        .cells
        .iter()
        .filter(|z| z.row >= c.row && z.row < c.row + c.rowspan && z.col == 1)
        .map(|z| compact(&z.text))
        .collect();
    assert_eq!(covered, ["財政投融資", "外為", "保険", "その他"]);
    assert!(a.pages[0].words[455].text.is_empty(), "native brace is geometry, never body text");
}
#[test]
fn native_dash_ids_are_restored_once_without_replacing_existing_words() {
    use std::collections::BTreeSet;
    let a = target();
    let p = &a.pages[0];
    let tables: Vec<_> = p.regions.iter().flat_map(|r| &r.lines).filter_map(|l| l.table.as_ref()).collect();
    let t = tables
        .iter()
        .find(|t| t.cells.iter().any(|c| compact(&c.text) == "一般会計"))
        .expect("cashflow matrix");
    let ids: Vec<_> = t.cells.iter().flat_map(|c| c.words.iter().copied()).collect();
    assert_eq!(ids.len(), ids.iter().collect::<BTreeSet<_>>().len(), "no duplicate word owner");
    let expected: BTreeSet<usize> = include_str!("fixtures/financial_flow/196133-table-word-ids.txt")
        .split_whitespace()
        .map(|s| s.parse().unwrap())
        .collect();
    assert_eq!(
        ids.iter().copied().collect::<BTreeSet<_>>(),
        expected,
        "all original table words retained"
    );
    for id in [68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 295, 298, 299] {
        let w = &p.words[id];
        assert_eq!(w.raw, "\u{e61c}");
        assert_eq!(w.text, "－");
        let owners: Vec<_> = t.cells.iter().filter(|c| c.words.contains(&id)).collect();
        assert_eq!(owners.len(), 1);
        assert!(owners[0].x0 <= w.x0 && w.x1 <= owners[0].x1 && owners[0].y0 <= w.y0 && w.y1 <= owners[0].y1);
    }
    assert_eq!(t.cells.iter().map(|c| c.text.matches('－').count()).sum::<usize>(), 16);
}
