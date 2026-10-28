//! Full original-source oracles. Baseline128is preserved except one physically
//! reviewed LF inside a transcribed legal reference in each sourceDocument.
use kanpo_parser::{analyze_item, page_inputs};
use serde_json::Value;
macro_rules! original {
    ($name:ident,$pdf:literal,$expected:literal) => {
        #[test]
        fn $name() {
            let pages = page_inputs(include_bytes!($pdf)).unwrap();
            let actual: Value =
                serde_json::from_slice(&serde_json::to_vec(&analyze_item(&pages)).unwrap())
                    .unwrap();
            let expected: Value = serde_json::from_str(include_str!($expected)).unwrap();
            assert_eq!(actual, expected);
        }
    };
}
original!(
    source_20260723g001630016,
    "data/quoted-references/20260723g001630016.pdf",
    "data/quoted-references/20260723g001630016-expected129.json"
);
original!(
    source_20260723g001630035,
    "data/quoted-references/20260723g001630035.pdf",
    "data/quoted-references/20260723g001630035-expected129.json"
);
original!(
    source_20260723g001630081,
    "data/quoted-references/20260723g001630081.pdf",
    "data/quoted-references/20260723g001630081-expected129.json"
);
original!(
    source_20260729g001690025,
    "data/quoted-references/20260729g001690025.pdf",
    "data/quoted-references/20260729g001690025-expected129.json"
);
original!(
    source_20260729g001690064,
    "data/quoted-references/20260729g001690064.pdf",
    "data/quoted-references/20260729g001690064-expected129.json"
);
original!(
    source_20260930g002150131,
    "data/native-column-flow/20260930g002150131.pdf",
    "data/quoted-references/20260930g002150131-expected129.json"
);
