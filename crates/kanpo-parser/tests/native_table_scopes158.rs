use kanpo_parser::{Analysis, Block};
use kanpo_parser::model::NestedTable;
use std::path::PathBuf;
fn check(key: &str) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let gold: serde_json::Value =
        serde_json::from_str(include_str!("expected-native-table-scopes158.json")).unwrap();
    let case = gold["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["key"] == key)
        .unwrap();
    let pdf = std::fs::read(root.join(case["input"].as_str().unwrap())).unwrap();
    let a: Analysis = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(&pdf).unwrap());
    if case["kind"] == "parallel_original_parents" {
        let original = a
            .pages
            .iter()
            .flat_map(|p| &p.regions)
            .flat_map(|r| &r.lines)
            .filter_map(|l| l.table.as_ref())
            .map(|t| t.to_nested())
            .collect::<Vec<_>>();
        let docs = a
            .articles
            .iter()
            .flat_map(|a| &a.document.blocks)
            .flat_map(|b| match b {
                Block::Paragraph { runs } => runs
                    .iter()
                    .filter_map(|r| r.table.clone())
                    .collect::<Vec<NestedTable>>(),
                _ => vec![],
            })
            .collect::<Vec<_>>();
        assert_eq!(
            docs, original,
            "all original cells, rows and merges stay under their own printed parent: {key}"
        );
    }
    let mut wire: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    wire.as_object_mut().unwrap().remove("pages");
    assert_eq!(
        wire, case["expected"],
        "complete body/article metadata/document source gold: {key}"
    );
}
macro_rules! source_case {
    ($name:ident,$key:literal) => {
        #[test]
        fn $name() {
            check($key)
        }
    };
}
source_case!(parallel_original_20260717g001600065, "20260717g001600065");
source_case!(parallel_original_20260724g001650659, "20260724g001650659");
source_case!(parallel_original_20260724g001650660, "20260724g001650660");
source_case!(parallel_original_20260724g001650661, "20260724g001650661");
source_case!(parallel_original_20260724g001650662, "20260724g001650662");
source_case!(parallel_original_20260724g001650665, "20260724g001650665");
source_case!(parallel_original_20260724g001650667, "20260724g001650667");
source_case!(parallel_original_20260724g001650682, "20260724g001650682");
source_case!(parallel_original_20260724g001650683, "20260724g001650683");
source_case!(parallel_original_20260724g001650684, "20260724g001650684");
source_case!(parallel_original_20260724g001650685, "20260724g001650685");
source_case!(parallel_original_20260724g001650687, "20260724g001650687");
source_case!(parallel_original_20260724g001650688, "20260724g001650688");
source_case!(parallel_original_20260724g001650693, "20260724g001650693");
source_case!(parallel_original_20260724g001650694, "20260724g001650694");
source_case!(parallel_original_20260724g001650702, "20260724g001650702");
source_case!(parallel_original_20260724g001650704, "20260724g001650704");
source_case!(parallel_original_20260724g001650706, "20260724g001650706");
source_case!(parallel_original_20260724g001650709, "20260724g001650709");
source_case!(parallel_original_20260724g001650711, "20260724g001650711");
source_case!(parallel_original_20260724g001650713, "20260724g001650713");
source_case!(parallel_original_20260724g001650717, "20260724g001650717");
source_case!(parallel_original_20260724g001650718, "20260724g001650718");
source_case!(parallel_original_20260724g001650725, "20260724g001650725");
source_case!(parallel_original_20260724g001650727, "20260724g001650727");
source_case!(parallel_original_20260724g001650765, "20260724g001650765");
source_case!(
    continuation_original_20260717g001600099,
    "20260717g001600099"
);
source_case!(
    continuation_original_20260717g001600103,
    "20260717g001600103"
);
source_case!(
    continuation_original_20260731g001730066,
    "20260731g001730066"
);
source_case!(
    continuation_original_20261016g002280523,
    "20261016g002280523"
);
source_case!(
    continuation_original_20260717h017500003,
    "20260717h017500003"
);
source_case!(
    continuation_original_20260813h017670007,
    "20260813h017670007"
);
source_case!(
    continuation_original_20260813h017670008,
    "20260813h017670008"
);
source_case!(
    continuation_original_20260813h017670009,
    "20260813h017670009"
);
source_case!(
    continuation_original_20260818h017700004,
    "20260818h017700004"
);
source_case!(
    continuation_original_20260818h017700005,
    "20260818h017700005"
);
