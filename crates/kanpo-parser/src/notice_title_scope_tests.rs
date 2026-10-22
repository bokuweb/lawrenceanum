use crate::notice_boundaries::{cuts, groups};
use crate::trace::{Line, PageTrace, Region, Role, Word, WordStatus};
use std::collections::{BTreeSet, HashSet};
fn put(p: &mut PageTrace, t: &str, b: [f32; 4], font: Option<usize>) {
    let id = p.words.len();
    p.words.push(Word {
        id,
        raw: t.into(),
        text: t.into(),
        x0: b[0],
        y0: b[1],
        x1: b[2],
        y1: b[3],
        status: WordStatus::Used,
    });
    p.native_styles
        .push(font.map(|f| BTreeSet::from([(f, 8f64.to_bits())])));
    p.regions[0].lines.push(Line::text(t.into(), vec![id]));
}
fn page() -> PageTrace {
    PageTrace {
        width: 600.,
        height: 800.,
        layout: "tiers",
        words: vec![],
        native_styles: vec![],
        native_bounds: vec![],
        regions: vec![Region {
            role: Role::Body,
            unit: 0,
            part: 0,
            lines: vec![],
        }],
        text: String::new(),
        doc_text: String::new(),
        tables: vec![],
    }
}
fn notice(p: &mut PageTrace, x: f32, title: &str, font: Option<usize>, publication: &str) {
    put(
        p,
        title,
        [x, 50., x + 8., 50. + title.chars().count() as f32 * 8.],
        font,
    );
    put(
        p,
        "手続に関する本文。",
        [x - 11., 58., x - 3., 250.],
        Some(2),
    );
    put(p, publication, [x - 33., 60., x - 25., 140.], Some(2));
    put(
        p,
        "Unknown organization",
        [x - 44., 100., x - 36., 180.],
        Some(2),
    );
    put(p, "清算人任意", [x - 55., 120., x - 47., 210.], Some(2));
}
fn reference(p: &mut PageTrace, texts: &[&str], font: usize) {
    let mut ids = vec![];
    let mut y = 58.;
    for text in texts {
        let id = p.words.len();
        let end = y + text.chars().count() as f32 * 8.;
        put(p, text, [378., y, 386., end], Some(font));
        ids.push(id);
        y = end;
        p.regions[0].lines.pop();
    }
    // This is a full prose column. The following date does not extend beyond it.
    let last = *ids.last().unwrap();
    p.words[last].y1 = 300.;
    p.regions[0]
        .lines
        .insert(2, Line::text(texts.concat(), ids));
}
#[test]
fn prose_references_are_not_title_fences_regardless_of_word_segmentation_or_font() {
    for font in [2, 9] {
        for texts in [
            vec!["対象の関連公告（対象Ａ）"],
            vec!["対象の関連公告", "（対象Ａ）"],
        ] {
            let mut p = page();
            notice(&mut p, 400., "訂正公告", Some(1), "2026年7月1日");
            reference(&mut p, &texts, font);
            assert_eq!(cuts(&p, &Default::default()), vec![0], "{font} {texts:?}");
        }
    }
}
#[test]
fn a_full_prose_column_ending_in_the_announcement_suffix_is_not_a_fence() {
    let mut p = page();
    notice(&mut p, 400., "訂正公告", Some(1), "2026年7月1日");
    reference(&mut p, &["対象の関連公告"], 2);
    assert_eq!(cuts(&p, &Default::default()), vec![0]);
}

#[test]
fn an_owned_frame_is_independent_of_the_preceding_words_font() {
    for title in ["関連公告", "任意公告（補足）", "任意の表題"] {
        let mut p = page();
        put(&mut p, "別の文書の語", [50., 10., 200., 20.], Some(3));
        put(&mut p, title, [300., 50., 450., 60.], Some(3));
        put(&mut p, "2026年7月1日", [300., 65., 450., 75.], Some(2));
        put(&mut p, "清算人任意", [300., 80., 450., 90.], Some(2));
        let mut rules = crate::lines::PageRules::default();
        rules.enclosures = vec![[290., 45., 460., 100.]];
        assert_eq!(cuts(&p, &rules), vec![1], "{title}");
    }
}
#[test]
fn independently_placed_qualified_titles_still_fence_when_source_style_is_ambiguous() {
    for font in [None, Some(2)] {
        let mut p = page();
        notice(&mut p, 400., "訂正公告", Some(1), "日付未定");
        notice(&mut p, 250., "関連公告（対象Ａ）", font, "2026年7月1日");
        assert!(cuts(&p, &Default::default()).is_empty(), "{font:?}");
    }
}
fn actual(page: &str) -> (crate::Analysis, Vec<crate::PageInput>) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = root
        .join("tests/archive/pages")
        .join(&page[..14])
        .join(format!("{page}.pdf"));
    let inputs = crate::page_inputs(&std::fs::read(path).unwrap()).unwrap();
    (crate::analyze_item(&inputs), inputs)
}
pub(crate) fn parts(a: &crate::Analysis, inputs: &[crate::PageInput]) -> Vec<Vec<usize>> {
    let text = crate::trace::join_page_texts(&a.pages, &|p| p.doc_text.as_str());
    let mut original = crate::segment_articles(&text);
    crate::wrapped_notice_titles::repair_wrapped_notice_titles(&mut original, &a.pages);
    let rules: Vec<_> = inputs.iter().map(|p| p.rules.clone()).collect();
    let parts: Vec<_> = groups(&original, &text, &a.pages, &rules)
        .into_iter()
        .flatten()
        .collect();
    assert_eq!(parts.len(), a.articles.len());
    let tables: Vec<_> = a
        .pages
        .iter()
        .flat_map(|p| p.tables.iter().cloned())
        .collect();
    for (part, article) in parts.iter().zip(&a.articles) {
        assert_eq!(
            crate::render_table_markers(&part.text, &tables),
            article.text
        );
    }
    parts
        .into_iter()
        .map(|part| {
            part.sources
                .into_iter()
                .map(|(pi, id)| {
                    assert_eq!(pi, 0);
                    id
                })
                .collect()
        })
        .collect()
}
#[test]
fn original_medical_page_owns_all_sixteen_notices_once() {
    let (a, inputs) = actual("20261005g002190067");
    let gold: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/expected-company-article-sources/20261005g002190067.json"
    ))
    .unwrap();
    assert_eq!(a.articles.len(), 16);
    let got = parts(&a, &inputs);
    assert_eq!(got.len(), 16);
    let wanted = gold["expected_source_groups"].as_array().unwrap();
    assert_eq!(wanted.len(), 16);
    let mut seen = HashSet::new();
    for (ids, g) in got.iter().zip(wanted) {
        let set: HashSet<_> = ids.iter().copied().collect();
        assert_eq!(set.len(), ids.len());
        let expected: HashSet<_> = g["source_word_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as usize)
            .collect();
        assert_eq!(set, expected);
        for id in ids {
            assert!(seen.insert(*id));
        }
    }
    assert_eq!(
        seen,
        a.pages[0]
            .words
            .iter()
            .filter(|w| matches!(w.status, WordStatus::Used))
            .map(|w| w.id)
            .collect()
    );
}
#[test]
fn original_correction_notices_do_not_merge_at_a_qualified_prose_reference() {
    let (a, inputs) = actual("20260731h017590032");
    let got = parts(&a, &inputs);
    // Manual original-image tracing: three separate correction notices.
    // Other units on this page remain unsupported, so this is a partial control.
    let gold: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/expected-company-article-sources/20260731h017590032-corrections.json"
    ))
    .unwrap();
    let expected: Vec<HashSet<usize>> = gold["expected_correction_groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| {
            g["source_word_ids"]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| id.as_u64().unwrap() as usize)
                .collect()
        })
        .collect();
    assert_eq!(expected.len(), 3);
    for ids in expected {
        assert!(
            got.iter()
                .any(|part| part.iter().copied().collect::<HashSet<_>>() == ids),
            "correction notice source ownership was merged or split"
        );
    }
}

#[test]
fn actual_source_font_id_bijection_preserves_all_analysis_fields() {
    for page in [
        "20260818g001840038",
        "20260730g001710163",
        "20260727g001670103",
        "20260813g001810062",
        "20260925g002120067",
        "20261005g002190067",
        "20260731h017590032",
    ] {
        let (a, mut inputs) = actual(page);
        let baseline = serde_json::to_value(a).unwrap();
        for p in &mut inputs {
            let mut output = String::new();
            let mut rest = p.xhtml.as_str();
            while let Some(start) = rest.find(" data-native-styles=\"") {
                let from = start + " data-native-styles=\"".len();
                let end = from + rest[from..].find('"').unwrap();
                output.push_str(&rest[..from]);
                let styles: Vec<_> = rest[from..end]
                    .split_whitespace()
                    .map(|t| {
                        let (f, b) = t.split_once(':').unwrap();
                        format!("{}:{b}", usize::MAX - f.parse::<usize>().unwrap())
                    })
                    .collect();
                output.push_str(&styles.join(" "));
                rest = &rest[end..];
            }
            output.push_str(rest);
            p.xhtml = output;
        }
        assert!(
            baseline == serde_json::to_value(crate::analyze_item(&inputs)).unwrap(),
            "font identity renaming changed Analysis for {page}"
        );
    }
}
