use kanpo_parser::{Analysis, Block};
const PDF: &[u8] = include_bytes!("data/source-comparison-labels/20260730g001710030.pdf");
fn analyze() -> Analysis {
    kanpo_parser::analyze_item(&kanpo_parser::page_inputs(PDF).unwrap())
}
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
const CONDITION:&str="デジタルＭＣＡ陸上移動通信用とし、930-940MHz帯と対の二周波方式に限る。使用は令和11年５月31日までに限る。";
#[test]
fn original_old_condition_is_not_interrupted_by_neighboring_domestic_identifier() {
    let a = analyze();
    assert_eq!(a.articles.len(), 2);
    assert!(
        compact(&a.articles[1].text).contains(&compact(CONDITION)),
        "the complete old printed condition is a single cell; J77 belongs to domestic allocation"
    );
}
#[test]
fn original_change_labels_bind_both_typed_role_tables_and_complete_cells() {
    let a = analyze();
    assert_eq!(a.articles.len(), 2);
    let mut after = vec![];
    let mut before = vec![];
    for b in &a.articles[1].document.blocks {
        if let Block::Shinkyu { rows, labels } = b {
            assert_eq!(
                *labels,
                Some(kanpo_parser::ComparisonLabels {
                    after: "変更後",
                    before: "変更前"
                }),
                "typed cells preserve their printed source label family"
            );
            for r in rows {
                for run in &r.after {
                    if let Some(t) = &run.table {
                        after.push(t);
                    }
                }
                for run in &r.before {
                    if let Some(t) = &run.table {
                        before.push(t);
                    }
                }
                if let Some(t) = &r.after_table {
                    after.push(t);
                }
                if let Some(t) = &r.before_table {
                    before.push(t);
                }
            }
        }
    }
    let after: Vec<String> = after
        .iter()
        .flat_map(|t| t.rows.iter())
        .flat_map(|r| r.iter())
        .map(|rs| compact(&rs.iter().map(|r| r.text.as_str()).collect::<String>()))
        .collect();
    let before: Vec<String> = before
        .iter()
        .flat_map(|t| t.rows.iter())
        .flat_map(|r| r.iter())
        .map(|rs| compact(&rs.iter().map(|r| r.text.as_str()).collect::<String>()))
        .collect();
    assert!(
        !after.is_empty() && !before.is_empty(),
        "both source change roles must retain their own structured tables"
    );
    assert!(
        before.contains(&compact(CONDITION)),
        "the long old condition must remain one complete cell"
    );
    assert!(
        before.contains(&compact("850-860Ｊ77")),
        "the native identifier belongs to its own frequency cell"
    );
}

#[test]
fn original_change_label_surface_is_preserved_in_source_article_text() {
    let a = analyze();
    let text = &a.articles[1].text;
    for label in ["変更後", "変更前"] {
        assert_eq!(
            text.lines().filter(|l| l.trim() == label).count(),
            1,
            "retain the actual printed label {label}"
        );
    }
    assert!(
        !text
            .lines()
            .any(|l| matches!(l.trim(), "改正後" | "改正前")),
        "do not replace source labels with canonical amendment wording"
    );
}

#[test]
fn prior_notice_native146_article_and_note_are_unchanged() {
    let a = analyze();
    let old: serde_json::Value = serde_json::from_str(include_str!(
        "data/source-comparison-labels/prior-notice146.json"
    ))
    .unwrap();
    let current: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&a.articles[0]).unwrap()).unwrap();
    assert_eq!(
        current, old,
        "the preceding publication, including note spacing and structure, is unchanged"
    );
}

#[test]
fn original_confirmed_source_change_label_words_are_header_labels() {
    let a = analyze();
    for id in [51, 52, 53, 54, 55, 56] {
        assert_eq!(
            serde_json::to_value(&a.pages[0].words[id]).unwrap()["status"],
            "header_label",
            "confirmed source label Word {id}"
        );
    }
}

#[test]
fn preexisting_preamble_table_is_retained_as_one_exact_native_table() {
    let pdf = include_bytes!("data/source-comparison-labels/20260924g002110029.pdf");
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "data/source-comparison-labels/preamble-table146.json"
    ))
    .unwrap();
    let actual: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    let tables: Vec<_> = actual["pages"][0]["regions"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| r["lines"].as_array().unwrap())
        .filter_map(|l| l.get("table"))
        .collect();
    assert!(
        tables.contains(&&expected),
        "recognizing a later change pair must not flatten the preceding independent source table"
    );
}

#[test]
fn independent_native_table_20261007h018030003() {
    let a = kanpo_parser::analyze_item(
        &kanpo_parser::page_inputs(include_bytes!(
            "data/source-comparison-labels/20261007h018030003.pdf"
        ))
        .unwrap(),
    );
    let a: serde_json::Value = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "data/source-comparison-labels/20261007h018030003-independent-table146.json"
    ))
    .unwrap();
    assert!(a["pages"][0]["regions"].as_array().unwrap().iter().flat_map(|r|r["lines"].as_array().unwrap()).any(|l|l.get("table")==Some(&expected)),"a separate publication's ruled table must retain complete cells, geometry, and native source ownership");
}
#[test]
fn independent_native_table_20261020h018110006() {
    let a = kanpo_parser::analyze_item(
        &kanpo_parser::page_inputs(include_bytes!(
            "data/source-comparison-labels/20261020h018110006.pdf"
        ))
        .unwrap(),
    );
    let a: serde_json::Value = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "data/source-comparison-labels/20261020h018110006-independent-table146.json"
    ))
    .unwrap();
    assert!(a["pages"][0]["regions"].as_array().unwrap().iter().flat_map(|r|r["lines"].as_array().unwrap()).any(|l|l.get("table")==Some(&expected)),"a separate publication's ruled table must retain complete cells, geometry, and native source ownership");
}

#[test]
fn native_continuation_inherits_the_confirmed_change_family() {
    let mut pages = vec![];
    for pdf in [
        include_bytes!("data/source-comparison-labels/20260730g001710030.pdf").as_slice(),
        include_bytes!("data/source-comparison-labels/20260730g001710031.pdf").as_slice(),
    ] {
        pages.extend(kanpo_parser::page_inputs(pdf).unwrap());
    }
    let a = kanpo_parser::analyze_item(&pages);
    assert_eq!(a.pages.len(), 2);
    let next = &a.pages[1].text;
    for label in ["変更後", "変更前"] {
        assert_eq!(
            next.lines().filter(|l| *l == label).count(),
            1,
            "a certified comparison continuation retains its source family's role name"
        );
    }
    assert!(!next.lines().any(|l| matches!(l, "改正後" | "改正前")));
}

fn source_value(a: &Analysis) -> serde_json::Value {
    serde_json::from_str(&serde_json::to_string(a).unwrap()).unwrap()
}
#[test]
fn original_joint_headings_keep_complete_source_ownership_and_every_article_exact() {
    for (pdf, baseline) in [
        (
            include_bytes!("data/source-comparison-labels/20260724h017540002.pdf").as_slice(),
            include_str!("data/source-comparison-labels/20260724h017540002-baseline146.json"),
        ),
        (
            include_bytes!("data/source-comparison-labels/20260805g001760048.pdf").as_slice(),
            include_str!("data/source-comparison-labels/20260805g001760048-baseline146.json"),
        ),
        (
            include_bytes!("data/source-comparison-labels/20260824g001880052.pdf").as_slice(),
            include_str!("data/source-comparison-labels/20260824g001880052-baseline146.json"),
        ),
    ] {
        let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
        let old: serde_json::Value = serde_json::from_str(baseline).unwrap();
        assert_eq!(source_value(&a),old,"a heading split into joint-agency source columns must remain entirely in its own preamble");
    }
}
#[test]
fn original_partial_marker_streams_keep_existing_complete_documents() {
    for (pdf, baseline) in [
        (
            include_bytes!("data/source-comparison-labels/20260803h017600004.pdf").as_slice(),
            include_str!("data/source-comparison-labels/20260803h017600004-baseline146.json"),
        ),
        (
            include_bytes!("data/source-comparison-labels/20260908h017850007.pdf").as_slice(),
            include_str!("data/source-comparison-labels/20260908h017850007-baseline146.json"),
        ),
        (
            include_bytes!("data/source-comparison-labels/20260807g001780064.pdf").as_slice(),
            include_str!("data/source-comparison-labels/20260807g001780064-baseline146.json"),
        ),
    ] {
        let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
        let old: serde_json::Value = serde_json::from_str(baseline).unwrap();
        assert_eq!(source_value(&a),old,"missing a role marker after article segmentation cannot flatten or change a previously represented cell");
    }
}

#[test]
fn native_heading_prefix_cannot_claim_neighboring_table_words() {
    let pdf = include_bytes!("data/source-comparison-labels/20260803g001740011.pdf");
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let old: serde_json::Value = serde_json::from_str(include_str!(
        "data/source-comparison-labels/20260803g001740011-baseline146.json"
    ))
    .unwrap();
    assert_eq!(source_value(&a), old, "a rendered prefix sharing a line with another cell is not a complete publication-heading witness");
}

#[test]
fn original_before_first_notices_preserve_all_records_and_native_article_boundaries() {
    for (pdf, baseline) in [
        (
            include_bytes!("data/source-comparison-labels/20261005h018010004.pdf").as_slice(),
            include_str!("data/source-comparison-labels/20261005h018010004-baseline146.json"),
        ),
        (
            include_bytes!("data/source-comparison-labels/20261005h018010005.pdf").as_slice(),
            include_str!("data/source-comparison-labels/20261005h018010005-baseline146.json"),
        ),
    ] {
        let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
        let old: serde_json::Value = serde_json::from_str(baseline).unwrap();
        assert_eq!(source_value(&a),old,"ordinary Before/After records stay complete literal notices without cross-record comparison roles");
    }
}

fn mixed_scopes(pdf1: &[u8], pdf2: &[u8], baseline: &str, count: usize) {
    let mut pages = kanpo_parser::page_inputs(pdf1).unwrap();
    pages.extend(kanpo_parser::page_inputs(pdf2).unwrap());
    let a = kanpo_parser::analyze_item(&pages);
    let now = source_value(&a);
    let old: serde_json::Value = serde_json::from_str(baseline).unwrap();
    assert_eq!(
        a.articles.len(),
        count,
        "an earlier comparison cannot disable native article boundaries later in the stream"
    );
    assert_eq!(
        now["pages"], old["pages"],
        "every native source page, cell, and owner remains exact"
    );
    assert_eq!(now["text"], old["text"]);
    for (i, (x, y)) in now["articles"]
        .as_array()
        .unwrap()
        .iter()
        .zip(old["articles"].as_array().unwrap())
        .enumerate()
    {
        assert_eq!(
            x["text"], y["text"],
            "complete original Article {i} surface"
        );
        let metadata = |v: &serde_json::Value| {
            v.as_object()
                .unwrap()
                .iter()
                .filter(|(k, _)| !matches!(k.as_str(), "document" | "text"))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<serde_json::Map<_, _>>()
        };
        assert_eq!(metadata(x), metadata(y));
        if i > 0 {
            assert_eq!(x["document"]["blocks"],y["document"]["blocks"],"all unrelated announcement Runs, styles, spaces, newlines and opaque tables remain exact");
            assert_eq!(
                x["document"]["format"], "unknown",
                "ordinary financial announcements do not inherit an amendment's prose format"
            );
        }
    }
    assert!(
        a.articles[0].document.blocks.iter().any(|b| matches!(
            b,
            Block::Shinkyu {
                labels: Some(_),
                ..
            }
        )),
        "the complete first comparison remains structured inside its own native article scope"
    );
}
#[test]
fn comparison_scope_then_independent_announcements_20260724g001660034() {
    mixed_scopes(
        include_bytes!("data/source-comparison-labels/20260724g001660034.pdf"),
        include_bytes!("data/source-comparison-labels/20260724g001660036.pdf"),
        include_str!("data/source-comparison-labels/20260724g001660034-mixed-baseline146.json"),
        3,
    );
}
#[test]
fn comparison_scope_then_independent_announcements_20260805g001760106() {
    mixed_scopes(
        include_bytes!("data/source-comparison-labels/20260805g001760106.pdf"),
        include_bytes!("data/source-comparison-labels/20260805g001760108.pdf"),
        // baseline146 cut at the 「会社その他の公告」 label line (native263v,
        // `data/company-section263v/split.py`): one more native article.
        include_str!("data/source-comparison-labels/20260805g001760106-mixed-baseline146-company263v.json"),
        17,
    );
}
#[test]
fn comparison_scope_then_independent_announcements_20260806g001770048() {
    mixed_scopes(
        include_bytes!("data/source-comparison-labels/20260806g001770048.pdf"),
        include_bytes!("data/source-comparison-labels/20260806g001770049.pdf"),
        include_str!("data/source-comparison-labels/20260806g001770048-mixed-baseline146.json"),
        3,
    );
}
