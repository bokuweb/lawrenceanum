use kanpo_parser::{analyze_item, page_inputs};
fn check(pdf: &[u8], id: usize, row: usize, expected: &str, body: &str) {
    let a = analyze_item(&page_inputs(pdf).unwrap());
    assert_eq!(
        a.text
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '\u{25b3}')
            .collect::<String>(),
        body.trim_end()
    );
    let cells: Vec<_> = a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .flat_map(|t| &t.cells)
        .collect();
    let target: Vec<_> = cells.iter().filter(|c| c.words.contains(&id)).collect();
    assert_eq!(target.len(), 1);
    assert_eq!((target[0].row, target[0].col), (row, 1));
    assert_eq!(target[0].text, "\u{25b3}");
    assert_eq!(target[0].words, vec![id]);
    let other: Vec<serde_json::Value> = cells
        .iter()
        .filter(|c| !c.words.contains(&id))
        .map(|c| serde_json::from_str(&serde_json::to_string(c).unwrap()).unwrap())
        .collect();
    assert_eq!(
        other,
        serde_json::from_str::<Vec<serde_json::Value>>(expected).unwrap()
    );
}
#[test]
fn native_triangle_actual_20260731g001720098() {
    check(
        include_bytes!("archive/pages/20260731g00172/20260731g001720098.pdf"),
        113,
        36,
        include_str!("expected/owned_triangles/20260731g001720098-other-cells.json"),
        include_str!("expected/owned_triangles/20260731g001720098-nontriangle-body.txt"),
    );
}
#[test]
fn native_triangle_actual_20260731g001720102() {
    check(
        include_bytes!("archive/pages/20260731g00172/20260731g001720102.pdf"),
        37,
        24,
        include_str!("expected/owned_triangles/20260731g001720102-other-cells.json"),
        include_str!("expected/owned_triangles/20260731g001720102-nontriangle-body.txt"),
    );
}
