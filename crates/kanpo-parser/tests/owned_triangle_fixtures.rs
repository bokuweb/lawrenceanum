#[path = "support/legacy_analysis.rs"]
mod legacy_analysis;
use kanpo_parser::{analyze_item, page_inputs};
fn source(p: &[u8]) -> kanpo_parser::Analysis {
    analyze_item(&page_inputs(p).unwrap())
}
#[test]
fn actual_revenue_triangle_and_unsigned_expenditure() {
    let a = source(include_bytes!(
        "archive/pages/20260831g00193/20260831g001930183.pdf"
    ));
    let rows: Vec<_> = a.text.lines().collect();
    assert_eq!(
        a.text
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '△')
            .collect::<String>(),
        include_str!("expected/owned_triangles/193183-nontriangle-body.txt")
    );
    let own: Vec<_> = a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter(|l| l.words.contains(&268))
        .collect();
    assert_eq!(own.len(), 1);
    assert!(own[0].words.contains(&231));
    assert!(!own[0].words.contains(&261));
    assert_eq!(
        a.pages[0].words[268].status,
        kanpo_parser::trace::WordStatus::Used
    );
    assert!(
        rows.iter().any(|l| l.starts_with("歳入224,828,567,066")
            && l.chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>()
                .contains("△128,613,201,090")),
        "revenue must own native triangle"
    );
    assert!(
        rows.iter().any(|l| l.starts_with("歳出227,871,441,806")
            && l.contains("　165,844,651,302　")
            && !l.contains("△")),
        "expenditure must remain positive"
    );
}
#[test]
fn actual_blank_amount_triangles_stay_in_their_own_rows() {
    let a = source(include_bytes!(
        "archive/pages/20260916g00206/20260916g002060248.pdf"
    ));
    let rows: Vec<_> = a.text.lines().collect();
    assert_eq!(
        a.text
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '△')
            .collect::<String>(),
        include_str!("expected/owned_triangles/206248-nontriangle-body.txt")
    );
    let cells: Vec<_> = a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .flat_map(|t| &t.cells)
        .collect();
    for (id, row) in [(124, 24), (125, 25)] {
        let c: Vec<_> = cells.iter().filter(|c| c.words.contains(&id)).collect();
        assert_eq!(c.len(), 1);
        assert_eq!((c[0].row, c[0].col), (row, 1));
        assert_eq!(c[0].text, "△");
        assert_eq!(c[0].words, vec![id]);
    }
    let others: Vec<_> = cells
        .iter()
        .filter(|c| !c.words.contains(&124) && !c.words.contains(&125))
        .map(|c| {
            serde_json::from_str::<serde_json::Value>(&serde_json::to_string(c).unwrap()).unwrap()
        })
        .collect();
    let expected: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "expected/owned_triangles/206248-other-cells.json"
    ))
    .unwrap();
    assert_eq!(others, expected);
    assert!(rows
        .iter()
        .any(|l| l.starts_with("支払承諾見返") && !l.contains("△")));
    assert!(rows
        .iter()
        .any(|l| l.starts_with("貸倒引当金") && l.contains("△")));
    assert!(rows
        .iter()
        .any(|l| l.starts_with("投資損失引当金") && l.contains("△")));
}

fn as_json(a: kanpo_parser::Analysis) -> serde_json::Value {
    legacy_analysis::value(&a)
}
#[test]
fn actual_explanatory_triangle_must_not_negate_positive_amount() {
    let a = source(include_bytes!(
        "archive/pages/20260724g00166/20260724g001660033.pdf"
    ));
    assert!(
        as_json(a)
            == serde_json::from_str::<serde_json::Value>(include_str!(
                "expected/owned_triangles/20260724g001660033-before-analysis.json"
            ))
            .unwrap(),
        "complete Analysis must remain unchanged"
    );
}
#[test]
fn actual_multiline_label_groups_are_not_resegmented_099() {
    let a = source(include_bytes!(
        "archive/pages/20260731g00172/20260731g001720099.pdf"
    ));
    assert!(
        as_json(a)
            == serde_json::from_str::<serde_json::Value>(include_str!(
                "expected/owned_triangles/20260731g001720099-before-analysis.json"
            ))
            .unwrap(),
        "complete Analysis must remain unchanged"
    );
}
#[test]
fn actual_multiline_label_groups_are_not_resegmented_104() {
    let a = source(include_bytes!(
        "archive/pages/20260731g00172/20260731g001720104.pdf"
    ));
    assert!(
        as_json(a)
            == serde_json::from_str::<serde_json::Value>(include_str!(
                "expected/owned_triangles/20260731g001720104-before-analysis.json"
            ))
            .unwrap(),
        "complete Analysis must remain unchanged"
    );
}
#[test]
fn actual_multiline_label_groups_are_not_resegmented_105() {
    let a = source(include_bytes!(
        "archive/pages/20260731g00172/20260731g001720105.pdf"
    ));
    assert!(
        as_json(a)
            == serde_json::from_str::<serde_json::Value>(include_str!(
                "expected/owned_triangles/20260731g001720105-before-analysis.json"
            ))
            .unwrap(),
        "complete Analysis must remain unchanged"
    );
}
