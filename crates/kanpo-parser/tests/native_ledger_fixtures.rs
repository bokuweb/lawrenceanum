const PDF: &[u8] = include_bytes!("archive/pages/20260724g00166/20260724g001660039.pdf");
fn assert_source_ids(a: &kanpo_parser::Analysis) {
    let p = &a.pages[0];
    let mut ids = std::collections::HashSet::new();
    for l in p.regions.iter().flat_map(|r| &r.lines) {
        if let Some(t) = &l.table {
            assert_eq!(l.text, t.render());
        }
        for &id in l.words.iter().chain(l.table.iter().flat_map(|t| t.cells.iter().flat_map(|c| &c.words))) {
            assert!(ids.insert(id), "重複語{id}");
        }
    }
    for w in p.words.iter().filter(|w| w.status == kanpo_parser::trace::WordStatus::Used) {
        assert!(ids.contains(&w.id), "未出力語{}", w.id);
    }
}
#[test]
fn native_balance_continuation_preserves_five_labelled_rows() {
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(PDF).unwrap());
    assert_source_ids(&a);
    let expected = [
        "２．一般正味財産　9,733,153,074　9,346,891,666　386,261,408",
        "（うち基本財産への充当額）",
        "（うち特定資産への充当額）",
        "正味財産合計　9,733,153,074　9,346,891,666　386,261,408",
        "負債及び正味財産合計　10,279,866,987　9,876,014,881　403,852,106",
    ];
    for (s, l) in expected.iter().zip(a.text.lines()) {
        assert!(l.starts_with(s), "{l}");
    }
    assert!(a.text.contains("有限会社トレジャー・イン・マインド"));
    assert!(a.text.contains("特定非営利活動法人かしま楽園倶楽部"));
}
#[test]
fn no_amount_on_an_inner_row_does_not_join_two_account_names() {
    let mut input = kanpo_parser::page_inputs(PDF).unwrap();
    let coord = |line: &str, key: &str| {
        line.split_once(key)
            .and_then(|(_, s)| s.split_once('"'))
            .and_then(|(s, _)| s.parse::<f32>().ok())
    };
    input[0].xhtml = input[0]
        .xhtml
        .lines()
        .filter(|line| {
            let x = coord(line, "xMin=\"");
            let y = coord(line, "yMin=\"");
            !x.zip(y).is_some_and(|(x, y)| x >= 172.5 && x < 412.5 && (y - 86.212).abs() < 0.1)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let a = kanpo_parser::analyze_item(&input);
    assert_source_ids(&a);
    assert_eq!(a.text.lines().nth(2), Some("（うち特定資産への充当額）"));
    assert!(a.text.lines().nth(3).unwrap().starts_with("正味財産合計　9,733,153,074"));
}
