//! Source-order witnesses are manually checked against original PDF pixels.
//! Complete Analysis JSON freezes reviewed behavior for regression; it does not
//! establish that every existing omission or table-role issue is resolved.
fn check(id: &str, pdf: &[u8], reviewed: &str) {
    let analysis = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let witnesses: serde_json::Value = serde_json::from_str(include_str!(
        "data/partial-rule-columns/source-order-witnesses130.json"
    ))
    .unwrap();
    let witness = witnesses["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["page"].as_str() == Some(id))
        .unwrap();
    if let Some(fragment) = witness["fragment"].as_str() {
        assert!(
            analysis.text.contains(fragment),
            "source-order witness: {id}"
        );
    }
    if let Some(ordered) = witness["ordered"].as_array() {
        let mut at = 0;
        for fragment in ordered {
            let fragment = fragment.as_str().unwrap();
            let offset = analysis.text[at..]
                .find(fragment)
                .unwrap_or_else(|| panic!("source-order witness: {id}, missing {fragment:?}"));
            at += offset + fragment.len();
        }
    }
    // Compare serialized JSON on both sides, preserving f32 serialization.
    let actual: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&analysis).unwrap()).unwrap();
    let expected: serde_json::Value = serde_json::from_str(reviewed).unwrap();
    assert_eq!(
        actual, expected,
        "complete reviewed Analysis regression: {id}"
    );
}
macro_rules! fixture {
    ($name:ident, $id:literal) => {
        #[test]
        fn $name() {
            check(
                $id,
                include_bytes!(concat!("data/partial-rule-columns/", $id, ".pdf")),
                include_str!(concat!(
                    "data/partial-rule-columns/",
                    $id,
                    "-reviewed130.json"
                )),
            );
        }
    };
}
fixture!(preface_finishes_before_lower_record, "20260819h017710004");
fixture!(
    personnel_procedures_finish_within_each_ruled_band,
    "20260805h017620008"
);
fixture!(
    toc_page_number_stays_above_the_original_divider,
    "20260817h017690001"
);
fixture!(
    long_correction_cells_do_not_alternate_wrapped_fragments,
    "20260813h017670032"
);
#[test]
fn appendix_order_and_intact_signature_spaces() {
    const PDF: &[u8] = include_bytes!("data/partial-rule-columns/20261015g002260005.pdf");
    check(
        "20261015g002260005",
        PDF,
        include_str!("data/partial-rule-columns/20261015g002260005-reviewed147.json"),
    );
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(PDF).unwrap());
    let actual: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    let old: serde_json::Value = serde_json::from_str(include_str!(
        "data/partial-rule-columns/20261015g002260005-reviewed130.json"
    ))
    .unwrap();
    assert_eq!(
        actual["pages"][0]["words"], old["pages"][0]["words"],
        "every original Word payload and status stays exact"
    );
    assert_eq!(
        actual["articles"].as_array().unwrap()[1..],
        old["articles"].as_array().unwrap()[1..]
    );
    let table = a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .find_map(|l| l.table.as_ref())
        .unwrap();
    assert_eq!(
        (table.rows, table.cols),
        (1, 4),
        "retain all printed columns, including empty outer cells"
    );
    assert_eq!(table.cells.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(), [
        "", "ラ　百ヘクタール以上の面積の土地の形状を変更する放水路の新築の事業であって、国土交通大臣、都道府県知事又は指定都市の長が河川工事として行うもの",
        "七十五ヘクタール以上百ヘクタール未満の面積の土地の形状を変更する放水路の新築の事業であって、国土交通大臣、都道府県知事又は指定都市の長が河川工事として行うもの", ""
    ], "the two printed thresholds must remain complete independent cells");
    assert_eq!(
        table
            .cells
            .iter()
            .flat_map(|c| c.words.iter().copied())
            .collect::<Vec<_>>(),
        [1, 2, 4, 7, 9, 13, 15, 3, 5, 8, 10, 14, 16]
    );
    let old_blocks = old["articles"][0]["document"]["blocks"].as_array().unwrap();
    let new_blocks = actual["articles"][0]["document"]["blocks"]
        .as_array()
        .unwrap();
    assert_eq!(
        new_blocks[1..],
        old_blocks[1..],
        "all later comparison rows remain exact"
    );
    let remaining = old_blocks[0]["runs"][0]["text"]
        .as_str()
        .unwrap()
        .split_once('\n')
        .unwrap()
        .1;
    assert_eq!(new_blocks[0]["runs"][1]["text"], remaining, "the appendix, signatures, and following notice retain their previous exact text and spacing");
}
