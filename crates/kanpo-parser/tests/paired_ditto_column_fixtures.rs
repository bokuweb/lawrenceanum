#[test]
fn map_kind_and_level_stay_with_every_repeated_area() {
    let pdf = include_bytes!("archive/pages/20260831h01779/20260831h017790010.pdf");
    let out = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let lines: Vec<_> = out.text.lines().collect();
    assert!(lines[1].contains("地理院タイル（数値地図25000（土地条件））"));
    assert!(lines[1].contains("ズームレベル10〜16"));
    for (i, area) in ["小郡", "矢田", "山口", "仁保", "那古", "保田", "金束", "上総湊", "鬼泪山", "浅虫"].iter().enumerate() {
        let compact: String = lines[i + 1].chars().filter(|c| !c.is_whitespace()).collect();
        assert!(compact.ends_with(area));
        if i > 0 { assert_eq!(compact.matches('〃').count(), 3); }
    }
    assert!(lines[11].starts_with("備考"));
    let mut ids = std::collections::HashSet::new();
    for id in out.pages[0].regions.iter().flat_map(|r| &r.lines).flat_map(|l| l.words.iter().chain(l.table.iter().flat_map(|t| &t.cells).flat_map(|c| &c.words))) { assert!(ids.insert(id)); }
}
