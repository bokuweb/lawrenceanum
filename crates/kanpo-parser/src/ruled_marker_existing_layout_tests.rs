#[test]
fn previously_recognized_comparisons_keep_the_complete_analysis_and_inner_tables() {
    {
        let a = crate::analyze_item(
            &crate::page_inputs(include_bytes!(
                "../tests/data/ruled-marker-bands/20260724g001650782.pdf"
            ))
            .unwrap(),
        );
        let actual: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
        let expected: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/data/ruled-marker-bands/20260724g001650782-baseline99.json"
        ))
        .unwrap();
        assert!(
            actual == expected,
            "previously recognized source comparison must retain every Analysis field"
        );
    }
    {
        let a = crate::analyze_item(
            &crate::page_inputs(include_bytes!(
                "../tests/data/ruled-marker-bands/20260729g001690094.pdf"
            ))
            .unwrap(),
        );
        let actual: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
        let expected: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/data/ruled-marker-bands/20260729g001690094-baseline99.json"
        ))
        .unwrap();
        assert!(
            actual == expected,
            "previously recognized source comparison must retain every Analysis field"
        );
    }
}
