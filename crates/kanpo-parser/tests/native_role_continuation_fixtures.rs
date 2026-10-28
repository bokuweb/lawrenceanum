use kanpo_parser::{analyze_item, page_inputs};
fn original_context() -> kanpo_parser::Analysis {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut inputs = Vec::new();
    for name in ["20261019h018100001", "20261019h018100002"] {
        let pdf =
            std::fs::read(root.join(format!("tests/data/ruled-cell-flow/{name}.pdf"))).unwrap();
        inputs.extend(page_inputs(&pdf).unwrap());
    }
    analyze_item(&inputs)
}
fn source_page(a: &kanpo_parser::Analysis) -> serde_json::Value {
    serde_json::from_str(&serde_json::to_string(&a.pages[1]).unwrap()).unwrap()
}
fn role(p: &serde_json::Value, id: u64) -> &str {
    let owners: Vec<_> = p["regions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| {
            r["lines"].as_array().unwrap().iter().any(|l| {
                l["words"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|w| w.as_u64() == Some(id))
                    || l["table"]["cells"].as_array().is_some_and(|cs| {
                        cs.iter().any(|c| {
                            c["words"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .any(|w| w.as_u64() == Some(id))
                        })
                    })
            })
        })
        .collect();
    assert_eq!(
        owners.len(),
        1,
        "Every source occurrence has exactly one region owner"
    );
    owners[0]["role"].as_str().unwrap()
}
#[test]
fn the_complete_left_comparison_cell_precedes_the_right_cell_under_page_context() {
    let a = original_context();
    let p = source_page(&a);
    let ids: Vec<_> = p["regions"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| r["lines"].as_array().unwrap())
        .flat_map(|l| l["words"].as_array().unwrap())
        .filter_map(|id| id.as_u64())
        .filter(|id| [1086, 1040].contains(id))
        .collect();
    assert_eq!(
        ids,
        [1086, 1040],
        "Finish the left printed comparison cell before reading the right cell's heading"
    );
}
#[test]
fn local_comparison_roles_do_not_cover_the_following_notice_or_ordinary_coordinate_frames() {
    let a = original_context();
    let p = source_page(&a);
    assert_eq!(role(&p, 1100), "after");
    assert_eq!(
        role(&p, 1040),
        "before",
        "The right comparison cell belongs to the prior page's change-before field"
    );
    for id in [1123, 1021, 507] {
        assert_eq!(
            role(&p, id),
            "body",
            "A following notice and its consecutive1..60 coordinate list are ordinary body"
        );
    }
    assert!(
        !["after", "before"].contains(&role(&p, 1113)),
        "The supplementary provisions start below the comparison frame's closing rule"
    );
}
#[test]
fn the_new_notice_and_both_consecutive_coordinate_frames_have_ordinary_body_owners() {
    let a = original_context();
    let p = source_page(&a);
    for id in [1123, 1021, 507] {
        assert_eq!(
            role(&p, id),
            "body",
            "The new notice and its consecutive point records cannot inherit amendment roles"
        );
    }
}
#[test]
fn the_supplementary_provisions_are_outside_the_closed_comparison_bottom() {
    let a = original_context();
    let p = source_page(&a);
    assert!(
        !["after", "before"].contains(&role(&p, 1113)),
        "The supplementary provisions start below the comparison frame's closing rule"
    );
}
#[test]
fn both_local_comparison_fields_retain_their_complete_original_occurrence_order() {
    let a = original_context();
    let p = source_page(&a);
    let source: serde_json::Value =
        serde_json::from_str(include_str!("data/native-role-continuation/references.json")).unwrap();
    for name in ["after", "before"] {
        let actual: Vec<_> = p["regions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["role"] == name)
            .flat_map(|r| r["lines"].as_array().unwrap())
            .flat_map(|l| l["words"].as_array().unwrap())
            .cloned()
            .collect();
        assert_eq!(actual, *source["all_used_comparison_memberships"][name].as_array().unwrap(), "Keep all source comparison occurrences within their own field, with no next-article members");
    }
}
