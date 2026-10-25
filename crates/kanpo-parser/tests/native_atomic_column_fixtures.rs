//! Original caption witnesses and reviewed regression goldens are distinct.
//! Raster bodies and pre-existing unresolved amendment roles are not graded.
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn check(id: &str, pdf: &[u8], golden: &str) {
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let witnesses: serde_json::Value = serde_json::from_str(include_str!(
        "data/native-atomic-columns/source-caption-witnesses132.json"
    ))
    .unwrap();
    let expected = witnesses["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["page"] == id)
        .unwrap()["caption"]
        .as_str()
        .unwrap();
    let owners = a
        .articles
        .iter()
        .filter(|a| compact(&a.text).contains(expected))
        .collect::<Vec<_>>();
    assert_eq!(owners.len(), 1);
    assert_eq!(
        compact(&owners[0].text),
        expected,
        "caption's original column: {id}"
    );
    let doc: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&owners[0].document).unwrap()).unwrap();
    fn texts(v: &serde_json::Value, out: &mut String) {
        match v {
            serde_json::Value::Object(o) => {
                if let Some(t) = o.get("text").and_then(|x| x.as_str()) {
                    out.push_str(t)
                }
                for (k, v) in o {
                    if k != "text" {
                        texts(v, out)
                    }
                }
            }
            serde_json::Value::Array(v) => {
                for x in v {
                    texts(x, out)
                }
            }
            _ => {}
        }
    }
    let mut text = String::new();
    texts(&doc, &mut text);
    assert_eq!(compact(&text), expected, "caption Document: {id}");
    // Review freezes this candidate for regression; it does not establish
    // independent full-page correctness or native coverage of raster bodies.
    let actual: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    let expected: serde_json::Value = serde_json::from_str(golden).unwrap();
    assert_eq!(actual, expected);
}
macro_rules! original {
    ($name:ident,$id:literal) => { original!($name,$id,concat!("data/native-atomic-columns/",$id,"-reviewed132.json")); };
    ($name:ident,$id:literal,$golden:expr) => {
        #[test]
        fn $name() {
            check(
                $id,
                include_bytes!(concat!("data/native-atomic-columns/", $id, ".pdf")),
                include_str!($golden),
            )
        }
    };
}
original!(
    caption_does_not_interrupt_another_court_case,
    "20260728h017560022",
    "data/native-court-categories/20260728h017560022-reviewed133.json"
);
original!(
    first_physical_column_keeps_its_own_caption,
    "20260824h017740012"
);
original!(
    last_physical_column_keeps_its_own_caption,
    "20260825h017750019"
);
original!(
    adjacent_procedure_titles_are_separate_source_blocks,
    "20260826h017760023",
    "data/native-court-categories/20260826h017760023-reviewed133.json"
);
original!(
    first_column_caption_does_not_name_the_second_column,
    "20260917h017920023",
    "data/native-court-categories/20260917h017920023-reviewed133.json"
);
original!(
    trustee_change_caption_does_not_name_a_liquidation,
    "20260925h017950020"
);
original!(
    a_later_issue_repeats_the_original_column_contract,
    "20261020h018110016"
);
#[test]
fn existing_comparison_fields_do_not_gain_unrelated_article_roots() {
    let a = kanpo_parser::analyze_item(
        &kanpo_parser::page_inputs(include_bytes!(
            "data/native-atomic-columns/20260831g001930134.pdf"
        ))
        .unwrap(),
    );
    let actual: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    let baseline: serde_json::Value = serde_json::from_str(include_str!(
        "data/native-atomic-columns/20260831g001930134-baseline131.json"
    ))
    .unwrap();
    assert_eq!(
        actual, baseline,
        "preserve the existing unresolved comparison roles"
    );
}
fn page_end(first: &[u8], next: &[u8], reviewed: &str) {
    let mut pages = vec![];
    for bytes in [first, next] {
        pages.extend(kanpo_parser::page_inputs(bytes).unwrap())
    }
    let a = kanpo_parser::analyze_item(&pages);
    let caption = "書面による計算報告";
    let owner = a
        .articles
        .iter()
        .find(|a| compact(&a.text).contains(caption))
        .unwrap();
    assert_eq!(
        compact(&owner.text),
        caption,
        "original column must close before the next physical page"
    );
    let actual: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    let expected: serde_json::Value = serde_json::from_str(reviewed).unwrap();
    assert_eq!(actual, expected);
}
#[test]
fn august_column_end_closes_before_the_next_original_page() {
    page_end(
        include_bytes!("data/native-atomic-columns/20260825h017750019.pdf"),
        include_bytes!("data/native-atomic-columns/20260825h017750020.pdf"),
        include_str!("data/native-atomic-columns/20260825h017750019-20-reviewed132.json"),
    )
}
#[test]
fn october_column_end_closes_before_the_next_original_page() {
    page_end(
        include_bytes!("data/native-atomic-columns/20261020h018110016.pdf"),
        include_bytes!("data/native-atomic-columns/20261020h018110017.pdf"),
        include_str!("data/native-court-categories/20261020h018110016-17-reviewed133.json"),
    )
}
