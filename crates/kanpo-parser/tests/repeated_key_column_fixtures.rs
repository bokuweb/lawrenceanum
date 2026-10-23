//! 見出しと全記録の同上記号で確認できる欄は、広い空白を越えて同じ表に保つ。
use kanpo_parser::trace::{PageTrace, Table, WordStatus};

fn cell(table: &Table, row: usize, col: usize) -> &str {
    table.cells.iter().find(|c| c.row == row && c.col == col).unwrap().text.as_str()
}

fn table_with<'a>(page: &'a PageTrace, value: &str) -> &'a Table {
    page.regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .find(|t| t.cells.iter().any(|c| c.text == value))
        .unwrap()
}

fn assert_each_used_word_once(page: &PageTrace) {
    let mut ids = std::collections::HashSet::new();
    for id in page
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .flat_map(|l| l.words.iter().chain(l.table.iter().flat_map(|t| &t.cells).flat_map(|c| &c.words)))
    {
        assert!(ids.insert(*id), "duplicate word {id}");
    }
    let used: std::collections::HashSet<_> = page.words.iter().filter(|w| matches!(w.status, WordStatus::Used)).map(|w| w.id).collect();
    assert_eq!(ids, used);
}

#[test]
fn eighteen_vehicle_records_keep_the_company_and_address_column() {
    let pdf = include_bytes!("archive/pages/20261015h01808/20261015h018080002.pdf");
    let out = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let page = &out.pages[0];
    let table = table_with(page, "2026－ＴＡ0006－１");
    assert_eq!((table.rows, table.cols), (19, 4));
    assert_eq!(cell(table, 0, 3), "認定を受けた者の氏名又は名称及び住所");
    assert_eq!(cell(table, 1, 1), "トヨタ　ＸＢ４－ＶＸＫＡ75Ｌ");
    assert_eq!(cell(table, 1, 2), "ボンネット");
    assert_eq!(cell(table, 1, 3), "トヨタ自動車株式会社　愛知県豊田市トヨタ町１番地");
    for r in 2..=18 {
        let number: String = r
            .to_string()
            .chars()
            .map(|c| match c {
                '1'..='9' => char::from_u32(c as u32 + 0xfee0).unwrap(),
                _ => c,
            })
            .collect();
        // PDF uses fullwidth single-digit suffixes and halfwidth two-digit suffixes.
        let suffix = if r < 10 { number } else { r.to_string() };
        assert_eq!(cell(table, r, 0), format!("2026－ＴＡ0006－{suffix}"));
        for c in 1..4 {
            assert_eq!(cell(table, r, c), "〃");
        }
    }
    let heading = out.text.find("〇国土交通省告示第千二百二十二号").unwrap();
    let date = out.text[heading..].find("令和八年十月十五日　国土交通大臣井林辰憲").unwrap() + heading;
    let first = out.text.find("2026－ＴＡ0006－１").unwrap();
    let last = out.text.find("2026－ＴＡ0006－18").unwrap();
    let next = out.text.find("〇国土交通省告示第千二百二十三号").unwrap();
    assert!(heading < date && date < first && first < last && last < next);
    assert_each_used_word_once(page);
}

#[test]
fn the_following_two_record_table_keeps_its_existing_narrow_recognition() {
    let pdf = include_bytes!("archive/pages/20261015h01808/20261015h018080002.pdf");
    let out = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let table = table_with(&out.pages[0], "2026－ＴＡ0007－１");
    assert_eq!((table.rows, table.cols), (3, 4));
    assert_eq!(cell(table, 1, 1), "トヨタ　７ＡＷ－ＡＸＵＨ78Ｒ");
    assert_eq!(cell(table, 1, 2), "ステーションワゴン");
    assert_eq!(cell(table, 2, 0), "2026－ＴＡ0007－２");
    assert_eq!(cell(table, 2, 3), "〃");
}

#[test]
fn three_outboard_type_changes_keep_manufacturer_and_change_details() {
    let pdf = include_bytes!("archive/pages/20260810h01765/20260810h017650004.pdf");
    let out = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let page = &out.pages[0];
    let table = table_with(page, "第4972号");
    assert_eq!((table.rows, table.cols), (4, 5));
    for (r, key, model) in [(1, "第4972号", "06002Ｆ"), (2, "第4997号", "04003Ｆ"), (3, "第4998号", "05003Ｆ")] {
        assert_eq!(cell(table, r, 0), key);
        assert_eq!(cell(table, r, 2), model);
    }
    assert_eq!(cell(table, 1, 3), "スズキ株式会社");
    assert_eq!(cell(table, 1, 4), "クランクシャフトの材質を変更");
    for r in 2..=3 {
        for c in 3..=4 {
            assert_eq!(cell(table, r, c), "〃");
        }
    }
    let first = out.text.find("第4972号").unwrap();
    assert!(out.text.find("〇国土交通省告示第千四十号").unwrap() < first);
    assert!(first < out.text.find("〇国土交通省告示第千四十一号").unwrap());
    assert!(table.cells.iter().all(|c| !c.text.contains('〇') && !c.text.contains('。')));
    assert_each_used_word_once(page);
}
