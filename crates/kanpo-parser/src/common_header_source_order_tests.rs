#[test]
fn all_original_common_header_fields_precede_both_parallel_statements() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let bytes =
        std::fs::read(root.join("tests/archive/pages/20260721g00161/20260721g001610161.pdf"))
            .unwrap();
    let inputs = crate::page_inputs(&bytes).unwrap();
    let a = crate::analyze_item(&inputs);
    let order: Vec<_> = a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .flat_map(|l| {
            l.words.iter().chain(
                l.table
                    .iter()
                    .flat_map(|t| t.cells.iter().flat_map(|c| &c.words)),
            )
        })
        .copied()
        .collect();
    let at = |id| {
        let indices: Vec<_> = order
            .iter()
            .enumerate()
            .filter(|(_, v)| **v == id)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(indices.len(), 1, "original source must occur once: {id}");
        indices[0]
    };
    let header = [239, 409, 280, 265, 66, 136, 78, 33, 7];
    for ids in header.windows(2) {
        assert!(
            at(ids[0]) < at(ids[1]),
            "common header internal source order: {ids:?}"
        );
    }
    for id in header {
        for child in [336, 273, 122] {
            assert!(at(id)<at(child),"an original common header field is still after a statement heading: {id} after {child}");
        }
    }
}
