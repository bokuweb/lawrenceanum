//! Original judicial case boundaries certified by native column scope.
//! Historical149 JSON anchors material; it is not independent accuracy gold.
#[path = "support/native_court_article_contracts.rs"]
mod court_contract;
use serde_json::Value;
use std::path::PathBuf;

fn data(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/native-court-columns")
        .join(name)
}
fn baseline(name: &str) -> Value {
    serde_json::from_slice(&std::fs::read(data(name)).unwrap()).unwrap()
}
fn analyze(keys: &[&str]) -> Value {
    let inputs: Vec<_> = keys
        .iter()
        .flat_map(|key| {
            kanpo_parser::page_inputs(&std::fs::read(data(&format!("{key}.pdf"))).unwrap()).unwrap()
        })
        .collect();
    serde_json::from_str(&serde_json::to_string(&kanpo_parser::analyze_item(&inputs)).unwrap())
        .unwrap()
}
fn source(key: &str, count: usize) -> Value {
    let old = baseline(&format!("{key}-baseline149.json"));
    let new = analyze(&[key]);
    court_contract::preserved(&old, &new);
    assert_eq!(new["articles"].as_array().unwrap().len(), count);
    new
}
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn owners(a: &Value, name: &str) -> Vec<usize> {
    a["articles"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            compact(p["text"].as_str().unwrap())
                .contains(name)
                .then_some(i)
        })
        .collect()
}
fn separate(a: &Value, names: &[&str]) {
    let ids: Vec<_> = names
        .iter()
        .map(|name| {
            let ids = owners(a, name);
            assert_eq!(ids.len(), 1, "one original case owner for {name}");
            ids[0]
        })
        .collect();
    assert_eq!(
        ids.iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        names.len()
    );
}

#[test]
fn seven_original_liquidation_cases_have_independent_source_owners() {
    let a = source("20260831h017790021", 7);
    separate(
        &a,
        &[
            "株式会社エルエルエム",
            "シグマ薬品株式会社",
            "株式会社ＭＩ",
            "株式会社ＳＳ",
            "株式会社Psエージェント",
            "株式会社シャクナゲ",
            "株式会社Ｋ商事",
        ],
    );
}
#[test]
fn the_same_court_does_not_merge_distinct_companies_across_native_columns() {
    let a = source("20260724h017540018", 6);
    separate(
        &a,
        &[
            "株式会社Ｔ・Ｂ・Ｃ",
            "株式会社ジェイ・エフ・シィ",
            "株式会社ｃｅｎｔｕｒｙ",
        ],
    );
}
#[test]
fn a_multicolumn_agreement_and_following_reorganization_keep_their_own_clauses() {
    let a = source("20260929h017970020", 2);
    separate(
        &a,
        &["株式会社Ｔ・Ｂ・Ｃ", "株式会社トーシンホールディングス"],
    );
}
#[test]
fn repeated_case_numbers_do_not_merge_different_proceedings_or_companies() {
    let a = source("20261016h018090024", 10);
    let names = [
        "株式会社ＪＯＢｃｏｎｎｅｃｔ",
        "株式会社絆ホールディングス",
        "株式会社レーヴ",
        "株式会社リベラーラ",
    ];
    for name in names {
        let ids = owners(&a, name);
        assert_eq!(
            ids.len(),
            2,
            "separate protection/collective prohibition source records for {name}"
        );
        for id in ids {
            let text = compact(a["articles"][id]["text"].as_str().unwrap());
            assert_eq!(
                names.iter().filter(|n| text.contains(**n)).count(),
                1,
                "unrelated company in the same source record"
            );
        }
    }
}
#[test]
fn adjacent_multicolumn_agreements_keep_their_original_material_and_parties() {
    let a = source("20261007h018030023", 7);
    separate(
        &a,
        &[
            "株式会社ＨＡＲ",
            "株式会社ＳＧ商会",
            "株式会社プランナー商事",
        ],
    );
}
#[test]
fn the_three_actual_consecutive_pages_preserve_all_native_material() {
    let old = baseline("consecutive-3pages-baseline149.json");
    let new = analyze(&[
        "20260831h017790020",
        "20260831h017790021",
        "20260831h017790022",
    ]);
    court_contract::preserved(&old, &new);
    assert_eq!(
        new["articles"].as_array().unwrap().len(),
        old["articles"].as_array().unwrap().len() + 5
    );
}
