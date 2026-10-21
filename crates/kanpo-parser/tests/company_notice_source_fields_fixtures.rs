//! Source evidence for the still-open company notice segmentation problem.
//!
//! The correct source fields are supplied by a manual original-PDF annotation.
//! These tests protect every source Word and manually annotated role. Article
//! boundary and exact group membership assertions live in notice_boundaries_tests.

use std::collections::HashSet;

const PDF: &[u8] = include_bytes!("archive/pages/20260818g00184/20260818g001840038.pdf");
const GOLD: &str = include_str!("expected-company-source-fields/20260818g001840038.json");

fn assert_source_fields(a: &kanpo_parser::Analysis) {
    let gold: serde_json::Value = serde_json::from_str(GOLD).unwrap();
    assert_eq!(a.pages.len(), 1);
    let page = &a.pages[0];
    let expected = gold["words"].as_array().unwrap();
    assert_eq!(page.words.len(), expected.len());
    for (w, e) in page.words.iter().zip(expected) {
        assert_eq!(w.id, e["id"].as_u64().unwrap() as usize);
        assert_eq!(w.raw, e["raw"].as_str().unwrap(), "raw Word {}", w.id);
        assert_eq!(w.text, e["text"].as_str().unwrap(), "text Word {}", w.id);
        assert_eq!(serde_json::to_value(w.status).unwrap(), e["status"]);
        for (value, bits) in [w.x0, w.y0, w.x1, w.y1]
            .iter()
            .zip(e["bbox_f32_bits"].as_array().unwrap())
        {
            assert_eq!(value.to_bits(), bits.as_u64().unwrap() as u32);
        }
    }

    let roles: HashSet<_> = [
        "notice_title",
        "prose",
        "date",
        "address",
        "organization",
        "representative",
    ]
    .into_iter()
    .collect();
    let notices = gold["notices"].as_array().unwrap();
    assert_eq!(
        notices.len(),
        gold["source_ownership_notice_count"].as_u64().unwrap() as usize
    );
    let mut assigned = HashSet::new();
    for notice in notices {
        let fields = notice["fields"].as_object().unwrap();
        assert_eq!(
            fields.keys().map(String::as_str).collect::<HashSet<_>>(),
            roles
        );
        let mut field_ids = HashSet::new();
        for field in fields.values() {
            let ids = field.as_array().unwrap();
            assert!(!ids.is_empty());
            for id in ids {
                let id = id.as_u64().unwrap() as usize;
                assert!(field_ids.insert(id), "duplicate field Word {id}");
                assert!(
                    assigned.insert(id),
                    "Word {id} shared by two manual notices"
                );
                assert_eq!(page.words[id].status, kanpo_parser::trace::WordStatus::Used);
            }
        }
        let whole: HashSet<_> = notice["all_word_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| id.as_u64().unwrap() as usize)
            .collect();
        assert_eq!(field_ids, whole);
    }
    let used: HashSet<_> = page
        .words
        .iter()
        .filter(|w| w.status == kanpo_parser::trace::WordStatus::Used)
        .map(|w| w.id)
        .collect();
    assert_eq!(assigned, used);
    assert_eq!(
        assigned.len(),
        gold["used_Words_assigned_once"].as_u64().unwrap() as usize
    );
}

#[test]
fn all_native_words_and_manually_bound_notice_fields_are_preserved() {
    let pages = kanpo_parser::page_inputs(PDF).unwrap();
    assert_source_fields(&kanpo_parser::analyze_item(&pages));
}

#[test]
fn source_fields_do_not_depend_on_word_element_emission_order() {
    let mut pages = kanpo_parser::page_inputs(PDF).unwrap();
    let before = kanpo_parser::analyze_item(&pages);
    let mut words: Vec<_> = pages[0]
        .xhtml
        .lines()
        .filter(|line| line.trim_start().starts_with("<word "))
        .map(str::to_owned)
        .collect();
    assert_eq!(words.len(), before.pages[0].words.len());
    words.reverse();
    let mut next = words.into_iter();
    pages[0].xhtml = pages[0]
        .xhtml
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("<word ") {
                next.next().unwrap()
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(next.next().is_none());
    let after = kanpo_parser::analyze_item(&pages);
    assert_source_fields(&after);
    assert_eq!(
        serde_json::to_value(&after).unwrap(),
        serde_json::to_value(&before).unwrap()
    );
}
