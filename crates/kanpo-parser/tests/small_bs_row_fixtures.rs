use kanpo_parser::{analyze_item, page_inputs};
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
#[test]
fn merged_category_preserves_two_asset_rows_and_values() {
    let input = page_inputs(include_bytes!(
        "archive/pages/20260813g00181/20260813g001810067.pdf"
    ))
    .unwrap();
    let a = analyze_item(&input);
    assert_eq!(
        a.text,
        include_str!("archive/expected-small-bs/20260813g001810067.txt")
    );
    // The new small table must preserve the following announcement's native period pair.
    let period: Vec<_> = a.pages[0].regions.iter().flat_map(|r| &r.lines).collect();
    let starts: Vec<_> = period.iter().enumerate().filter(|(_, l)| l.words.starts_with(&[115, 68])).map(|(i, _)| i).collect();
    assert_eq!(starts.len(), 1);
    let k = starts[0];
    assert_eq!(period[k].text, "（自　令和７年４月１日");
    assert_eq!(period[k + 1].text, "至　令和８年３月31日）");
    assert!(period[k + 1].words.starts_with(&[116, 69]));
    let section = a
        .text
        .split("一般社団法人日本食肉マイスター協会")
        .nth(1)
        .unwrap()
        .split("第36期決算公告")
        .next()
        .unwrap();
    assert!(
        section.lines().any(|l| compact(l).contains("流動資産43")),
        "missing first asset row: {section}"
    );
    assert!(
        section.lines().filter(|l| compact(l) == "合計43").count() >= 2,
        "asset/liability totals must each retain a separate row: {section}"
    );
    assert!(!section.contains("4343"), "two amounts merged: {section}");
}
#[test]
fn native_grid_keeps_category_span_and_both_asset_rows() {
    let input = page_inputs(include_bytes!(
        "archive/pages/20260813g00181/20260813g001810067.pdf"
    ))
    .unwrap();
    let a = analyze_item(&input);
    let tables: Vec<_> = a
        .pages
        .iter()
        .flat_map(|p| &p.regions)
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .filter(|t| (t.x0 - 56.75).abs() < 1. && (t.y0 - 333.75).abs() < 1.)
        .collect();
    assert_eq!(tables.len(), 1, "the original ruled BS must stay a table");
    let t = tables[0];
    assert_eq!(t.rows, 9);
    assert_eq!(t.cols, 3);
    for (r, label, value) in [
        (1, "流動資産", "43"),
        (2, "合計", "43"),
        (3, "流動負債", "56"),
        (4, "負債合計", "56"),
        (5, "剰余金", "△12"),
        (6, "(うち当期純損失)", "(12)"),
        (7, "純資産合計", "△12"),
        (8, "合計", "43"),
    ] {
        assert!(
            t.cells
                .iter()
                .any(|c| c.row == r && c.col == 1 && compact(&c.text) == label),
            "label row{r}: {t:?}"
        );
        assert!(
            t.cells
                .iter()
                .any(|c| c.row == r && c.col == 2 && compact(&c.text) == value),
            "value row{r}: {t:?}"
        );
    }
    assert!(t.cells.iter().any(|c| c.row == 3
        && c.col == 0
        && (c.y0 - 361.).abs() < 0.1
        && (c.y1 - 414.25).abs() < 0.1
        && compact(&c.text) == "負債及び純資産の部"
        && c.words == vec![645, 632]));
    assert!(t.cells.iter().any(|c| c.row == 1
        && c.col == 0
        && (c.y0 - 343.).abs() < 0.1
        && (c.y1 - 361.).abs() < 0.1
        && compact(&c.text) == "資産の部"
        && c.words == vec![652, 640]));
}

#[test]
fn existing_wide_amount_table_20260730g001710155_is_unchanged() {
    let a = analyze_item(
        &page_inputs(include_bytes!(
            "archive/pages/20260730g00171/20260730g001710155.pdf"
        ))
        .unwrap(),
    );
    assert_eq!(
        a.text,
        include_str!("archive/expected-small-bs/20260730g001710155.txt")
    );
}

#[test]
fn existing_wide_amount_table_20260803g001740142_is_unchanged() {
    let a = analyze_item(
        &page_inputs(include_bytes!(
            "archive/pages/20260803g00174/20260803g001740142.pdf"
        ))
        .unwrap(),
    );
    assert_eq!(
        a.text,
        include_str!("archive/expected-small-bs/20260803g001740142.txt")
    );
}

#[test]
fn adjacent_20260813g001810065_is_unchanged() {
    let a = analyze_item(
        &page_inputs(include_bytes!(
            "archive/pages/20260813g00181/20260813g001810065.pdf"
        ))
        .unwrap(),
    );
    assert_eq!(
        a.text,
        include_str!("archive/expected-small-bs/20260813g001810065.txt")
    );
}

#[test]
fn adjacent_20260813g001810066_is_unchanged() {
    let a = analyze_item(
        &page_inputs(include_bytes!(
            "archive/pages/20260813g00181/20260813g001810066.pdf"
        ))
        .unwrap(),
    );
    assert_eq!(
        a.text,
        include_str!("archive/expected-small-bs/20260813g001810066.txt")
    );
}

#[test]
fn adjacent_20260813g001810068_is_unchanged() {
    let a = analyze_item(
        &page_inputs(include_bytes!(
            "archive/pages/20260813g00181/20260813g001810068.pdf"
        ))
        .unwrap(),
    );
    assert_eq!(
        a.text,
        include_str!("archive/expected-small-bs/20260813g001810068.txt")
    );
}

#[test]
fn adjacent_20260813g001810069_is_unchanged() {
    let a = analyze_item(
        &page_inputs(include_bytes!(
            "archive/pages/20260813g00181/20260813g001810069.pdf"
        ))
        .unwrap(),
    );
    assert_eq!(
        a.text,
        include_str!("archive/expected-small-bs/20260813g001810069.txt")
    );
}

#[test]
fn second_original_small_bs_preserves_separate_asset_rows() {
    let a = analyze_item(
        &page_inputs(include_bytes!(
            "archive/pages/20260727g00167/20260727g001670074.pdf"
        ))
        .unwrap(),
    );
    assert_eq!(
        a.text,
        include_str!("archive/expected-small-bs/20260727g001670074.txt")
    );
    assert!(!a.text.contains("7777"));
    let rows: Vec<_> = a.text.lines().map(compact).collect();
    assert!(rows
        .windows(2)
        .any(|p| p[0] == "資産の部流動資産77" && p[1] == "合計77"));
}
