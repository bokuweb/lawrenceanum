use super::*;
use crate::{trace::WordStatus, Block};
fn fixture() -> (Document, String, Vec<Part>, Vec<PageTrace>) {
    let a = crate::analyze(
        r#"<html><body><doc><page width="595" height="842"><word xMin="500" xMax="508" yMin="100" yMax="148">第一の証拠。</word><word xMin="470" xMax="478" yMin="100" yMax="148">第二の証拠。</word></page></doc></body></html>"#,
    );
    assert!(a.pages[0]
        .words
        .iter()
        .all(|w| w.status == WordStatus::Used));
    let first = "変更後\n新しい本文。\n変更前\n元の本文。".to_string();
    let second = "会社の独立した公告。".to_string();
    let segment = format!("{first}\n{second}");
    let original = Document::from_text(&segment);
    let parts = vec![
        Part {
            text: first,
            sources: vec![(0, 0)],
        },
        Part {
            text: second,
            sources: vec![(0, 1)],
        },
    ];
    (original, segment, parts, a.pages)
}
#[test]
fn source_parts147_complete_proven_partitions_bound_modeling_before_company_notices() {
    let (d, s, p, pages) = fixture();
    assert!(authorized(&d, &s, &p, &pages));
    let doc = Document::from_text(&p[0].text);
    let Block::Shinkyu { rows, .. } = &doc.blocks[0] else {
        panic!()
    };
    assert_eq!(rows[0].before[0].text, "元の本文。");
    assert!(!doc.to_text().contains("会社"));
}
#[test]
fn source_parts147_all_formal_families_use_the_same_boundary_contract() {
    let (mut d, mut s, mut p, pages) = fixture();
    for text in [&mut s, &mut p[0].text] {
        *text = text.replace("変更", "改正");
    }
    d = Document::from_text(&s);
    assert!(authorized(&d, &s, &p, &pages));
}
fn reject(change: impl FnOnce(&mut Document, &mut String, &mut Vec<Part>, &mut Vec<PageTrace>)) {
    let (mut d, mut s, mut p, mut pages) = fixture();
    change(&mut d, &mut s, &mut p, &mut pages);
    assert!(!authorized(&d, &s, &p, &pages));
}
#[test]
fn source_parts147_unstructured_run_alignment_failure_is_not_bypassed() {
    reject(|d, _, _, _| *d = Document::from_text("会社の通常本文。"));
}
#[test]
fn source_parts147_missing_a_complete_source_part_declines() {
    reject(|_, _, p, _| {
        p.pop();
    });
}
#[test]
fn source_parts147_changed_source_text_declines() {
    reject(|_, _, p, _| p[1].text.push('別'));
}
#[test]
fn source_parts147_ascii_or_ideographic_spaces_cannot_be_deleted() {
    for space in [" ", "　"] {
        reject(|_, s, _, _| s.push_str(space));
    }
}
#[test]
fn source_parts147_new_article_requires_positive_source_claims() {
    reject(|_, _, p, _| p[1].sources.clear());
}
#[test]
fn source_parts147_repeated_native_word_ownership_declines() {
    reject(|_, _, p, _| p[1].sources = vec![(0, 0)]);
}
#[test]
fn source_parts147_unknown_native_word_declines() {
    reject(|_, _, p, _| p[1].sources = vec![(0, 999)]);
}
#[test]
fn source_parts147_source_word_identity_must_bind() {
    reject(|_, _, _, pages| pages[0].words[1].id = 999);
}
#[test]
fn source_parts147_source_claims_cannot_borrow_margin_or_header_words() {
    for status in [
        WordStatus::Noise,
        WordStatus::Margin,
        WordStatus::HeaderLabel,
    ] {
        reject(|_, _, _, pages| pages[0].words[1].status = status);
    }
}
#[test]
fn source_parts147_reversed_parts_cannot_supply_the_same_surface_set() {
    reject(|_, _, p, _| p.reverse());
}

#[test]
fn source_parts147_unrelated_paragraphs_keep_original_trailing_lf() {
    let (_, _, mut parts, pages) = fixture();
    parts[0].sources.clear();
    parts[1].text = "第一の会社。".into();
    parts[1].sources = vec![(0, 0)];
    parts.push(Part {
        text: "第二の会社。".into(),
        sources: vec![(0, 1)],
    });
    let source = parts
        .iter()
        .map(|p| p.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let original = Document::from_text(&source);
    let wanted = parts
        .iter()
        .map(|p| Document::from_text(&p.text))
        .collect::<Vec<_>>();
    let got = documents(&original, &source, &parts, &pages, &wanted, &[]).unwrap();
    let Block::Paragraph { runs } = &got[1].blocks[0] else {
        panic!()
    };
    assert_eq!(
        runs[0].text, "第一の会社。\n",
        "source-owned model scoping cannot trim unrelated original Run bytes"
    );
}
