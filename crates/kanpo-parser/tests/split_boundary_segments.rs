use std::collections::HashSet;
use kanpo_parser::trace::Role;
#[test]
fn split_boundary_segments_preserve_old_only_and_deleted_rows() {
    for (n,pdf) in [(86, include_bytes!("archive/pages/20261009g00223/20261009g002230086.pdf").as_slice()), (89, include_bytes!("archive/pages/20261009g00223/20261009g002230089.pdf").as_slice())] {
        let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
        let p = &a.pages[0];
        let first_before = p.regions.iter().find(|r| r.unit == 0 && matches!(r.role, Role::Before)).unwrap();
        let first_after = p.regions.iter().find(|r| r.unit == 0 && matches!(r.role, Role::After)).unwrap();
        let before: String = first_before.lines.iter().map(|l| l.text.as_str()).collect();
        let after: String = first_after.lines.iter().map(|l| l.text.as_str()).collect();
        if n == 89 {
            assert!(before.contains("神奈川県逗子市池子"));
            assert!(before.contains("神奈川県横浜市磯子区"));
            assert!(!after.contains("神奈川県逗子市池子"));
            assert!(first_before.lines.iter().flat_map(|l| l.table.iter()).flat_map(|t| &t.cells).any(|c| c.text == "10"));
            assert!(a.text.find("神奈川県逗子市池子").unwrap() < a.text.find("備考").unwrap());
        } else {
            assert!(after.contains("12　削除"));
            assert!(after.contains("［13〜130　略］"));
            assert!(!before.contains("12　削除"));
            assert!(before.contains("北海道千歳市"));
            assert!(!after.contains("電気通信業務用伝搬障害防止区域　１"));
        }
        let mut seen = HashSet::new();
        for id in p.regions.iter().flat_map(|r| &r.lines).flat_map(|l| &l.words) { assert!(seen.insert(id)); }
    }
}
