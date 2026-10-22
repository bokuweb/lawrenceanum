use super::*;
use crate::trace::{Line, PageTrace, Region, Role, Word, WordStatus};
use std::collections::{BTreeSet, HashSet};
fn style(font: usize) -> Option<crate::trace::NativeStyles> {
    Some(BTreeSet::from([(font, 8f64.to_bits())]))
}
fn push(p: &mut PageTrace, text: &str, b: [f32; 4], font: usize) {
    let id = p.words.len();
    p.words.push(Word {
        id,
        raw: text.into(),
        text: text.into(),
        x0: b[0],
        y0: b[1],
        x1: b[2],
        y1: b[3],
        status: WordStatus::Used,
    });
    p.native_styles.push(style(font));
    p.regions[0].lines.push(Line::text(text.into(), vec![id]));
    p.doc_text = p.regions[0]
        .lines
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    p.text = p.doc_text.clone();
}
fn empty() -> PageTrace {
    PageTrace {
        width: 600.,
        height: 800.,
        layout: "tiers",
        words: vec![],
        native_styles: vec![],
        native_bounds: vec![],
        native_content_styles: vec![],
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
fn notice(p: &mut PageTrace, x: f32, name: &str, date: &str) {
    push(p, "解散公告", [x, 50., x + 8., 82.], 1);
    push(
        p,
        "手続についてお知らせします。",
        [x - 11., 58., x - 3., 230.],
        2,
    );
    push(p, date, [x - 22., 60., x - 14., 140.], 2);
    push(p, name, [x - 33., 100., x - 25., 180.], 2);
    push(p, "代表清算人甲", [x - 44., 120., x - 36., 210.], 2);
}
fn two() -> PageTrace {
    let mut p = empty();
    notice(&mut p, 400., "不明な名称", "令和八年七月一日");
    notice(
        &mut p,
        300.,
        "Ｓｅｔｉａ Ｏｓａｋａ特定目的会社",
        "令和八年七月一日",
    );
    p
}
#[test]
fn distinct_styled_notice_boundaries_do_not_require_known_organization_names() {
    assert_eq!(cuts(&two(), &Default::default()), vec![0, 5]);
}
#[test]
fn arbitrary_name_values_do_not_change_boundary_evidence() {
    for name in [
        "X",
        "二百字相当の名称を想定した任意の値",
        "住所のように見える値",
        "引用された株式会社との関連は未確定",
    ] {
        let mut p = empty();
        notice(&mut p, 400., name, "令和八年七月一日");
        notice(&mut p, 300., name, "令和八年七月一日");
        assert_eq!(cuts(&p, &Default::default()), vec![0, 5]);
    }
}
#[test]
fn missing_or_identical_source_styles_do_not_authorize_boundaries() {
    let mut p = two();
    p.native_styles.fill(None);
    assert!(cuts(&p, &Default::default()).is_empty());
    p.native_styles.fill(style(1));
    assert!(cuts(&p, &Default::default()).is_empty());
}
#[test]
fn font_identity_bijection_is_irrelevant() {
    let mut p = two();
    for s in p.native_styles.iter_mut().flatten() {
        *s = s.iter().map(|&(f, b)| (19 - f, b)).collect();
    }
    assert_eq!(cuts(&p, &Default::default()), vec![0, 5]);
}
#[test]
fn positive_uniform_scaling_and_translation_do_not_add_point_or_count_caps() {
    for k in [0.125, 1., 8.] {
        let mut p = two();
        for w in &mut p.words {
            w.x0 = w.x0 * k - 100.;
            w.x1 = w.x1 * k - 100.;
            w.y0 = w.y0 * k - 200.;
            w.y1 = w.y1 * k - 200.;
        }
        for s in p.native_styles.iter_mut().flatten() {
            *s = s
                .iter()
                .map(|&(f, b)| (f, (f64::from_bits(b) * k as f64).to_bits()))
                .collect();
        }
        assert_eq!(cuts(&p, &Default::default()), vec![0, 5]);
    }
}
#[test]
fn a_day_character_is_not_a_publication_date() {
    for date in [
        "日付未定",
        "令和八年九十九月一日",
        "令和八年二月三十日",
        "令和零年七月一日",
    ] {
        let mut p = empty();
        notice(&mut p, 400., "任意", date);
        assert!(cuts(&p, &Default::default()).is_empty());
    }
}
#[test]
fn number_presentation_and_padding_are_not_date_length_limits() {
    for date in [
        "令和000008年000007月000001日",
        "令和八年七月一日",
        "2026年7月1日",
        "平成元年二月二十九日",
    ] {
        let mut p = empty();
        notice(&mut p, 400., "任意", date);
        assert_eq!(cuts(&p, &Default::default()), vec![0]);
    }
}
#[test]
fn quoted_heading_across_source_lines_is_not_an_independent_notice() {
    let mut p = empty();
    push(&mut p, "以下を引用「", [450., 40., 458., 200.], 2);
    notice(&mut p, 400., "任意", "令和八年七月一日");
    push(&mut p, "」引用終わり", [330., 40., 338., 200.], 2);
    assert!(cuts(&p, &Default::default()).is_empty());
}
#[test]
fn a_body_suffix_without_distinct_heading_style_is_not_a_boundary() {
    let mut p = two();
    p.regions[0].lines[5].text = "手続に関する公告".into();
    p.words[5].text = p.regions[0].lines[5].text.clone();
    p.words[5].raw = p.words[5].text.clone();
    p.native_styles[5] = style(2);
    assert_eq!(cuts(&p, &Default::default()), vec![0]);
}
#[test]
fn a_signature_before_the_publication_date_does_not_complete_the_notice() {
    let mut p = empty();
    notice(&mut p, 400., "任意", "令和八年七月一日");
    p.regions[0].lines.swap(2, 4);
    p.doc_text = p.regions[0]
        .lines
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(cuts(&p, &Default::default()).is_empty());
}
#[test]
fn source_text_or_owner_ambiguity_abstains() {
    let mut p = two();
    p.regions[0].lines[0].text = "別の公告".into();
    assert_eq!(cuts(&p, &Default::default()), vec![5]);
    let mut p = two();
    p.regions[0].lines[1].words.push(0);
    assert!(cuts(&p, &Default::default()).is_empty());
}
#[test]
fn invalid_geometry_is_not_ignored_when_deciding_scope() {
    let mut p = two();
    p.words[3].x0 = f32::NAN;
    assert!(cuts(&p, &Default::default()).is_empty());
    let mut rules = crate::lines::PageRules::default();
    rules.enclosures.push([0., 0., f32::INFINITY, 800.]);
    assert!(cuts(&two(), &rules).is_empty());
}
#[test]
fn a_related_financial_header_in_a_common_frame_is_not_an_extra_cut() {
    let mut p = empty();
    notice(&mut p, 400., "任意", "令和八年七月一日");
    push(&mut p, "第1期決算公告", [100., 50., 250., 60.], 3);
    push(&mut p, "令和八年七月一日", [100., 65., 250., 75.], 2);
    push(&mut p, "代表取締役乙", [100., 80., 250., 90.], 2);
    let mut r = crate::lines::PageRules::default();
    r.enclosures = vec![[80., 40., 420., 240.], [90., 45., 260., 100.]];
    assert_eq!(cuts(&p, &r), vec![0]);
}
#[test]
fn an_adjacent_financial_notice_cannot_supply_a_missing_date() {
    let mut p = empty();
    notice(&mut p, 400., "任意", "日付未定");
    push(&mut p, "第1期決算公告", [100., 50., 250., 60.], 3);
    push(&mut p, "令和八年七月一日", [100., 65., 250., 75.], 2);
    push(&mut p, "代表取締役乙", [100., 80., 250., 90.], 2);
    let mut r = crate::lines::PageRules::default();
    r.enclosures = vec![[90., 45., 260., 100.]];
    assert_eq!(cuts(&p, &r), vec![5]);
}
fn native_case(page: &str, expected: usize, gold: &serde_json::Value) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = page.get(..14).unwrap();
    let pdf = std::fs::read(
        root.join("tests/archive/pages")
            .join(dir)
            .join(format!("{page}.pdf")),
    )
    .unwrap();
    let inputs = crate::page_inputs(&pdf).unwrap();
    let a = crate::analyze_item(&inputs);
    assert_eq!(a.articles.len(), expected, "{page}");
    let text = crate::trace::join_page_texts(&a.pages, &|p| p.doc_text.as_str());
    let mut original = crate::segment_articles(&text);
    crate::wrapped_notice_titles::repair_wrapped_notice_titles(&mut original, &a.pages);
    let rules: Vec<_> = inputs.iter().map(|p| p.rules.clone()).collect();
    let parts = groups(&original, &text, &a.pages, &rules)
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    assert_eq!(parts.len(), a.articles.len());
    let all_tables: Vec<_> = a
        .pages
        .iter()
        .flat_map(|p| p.tables.iter().cloned())
        .collect();
    for (part, article) in parts.iter().zip(&a.articles) {
        assert_eq!(
            crate::render_table_markers(&part.text, &all_tables),
            article.text
        );
    }
    let expected_sets: Vec<HashSet<usize>> = if let Some(notices) = gold["notices"].as_array() {
        notices
            .iter()
            .map(|n| {
                n["all_word_ids"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_u64().unwrap() as usize)
                    .collect()
            })
            .collect()
    } else {
        gold["expected_source_groups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| {
                g["source_word_ids"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_u64().unwrap() as usize)
                    .collect()
            })
            .collect()
    };
    for (part, want) in parts.iter().zip(expected_sets) {
        assert!(part.sources.iter().all(|(pi, _)| *pi == 0));
        let got: HashSet<_> = part.sources.iter().map(|(_, id)| *id).collect();
        assert_eq!(got.len(), part.sources.len());
        assert_eq!(got, want, "source membership {page}");
    }
    let mut plain = inputs.clone();
    for p in &mut plain {
        let mut s = p.xhtml.clone();
        while let Some(i) = s.find(" data-native-styles=\"") {
            let e = i
                + " data-native-styles=\"".len()
                + s[i + " data-native-styles=\"".len()..].find('"').unwrap()
                + 1;
            s.replace_range(i..e, "");
        }
        p.xhtml = s;
    }
    let b = crate::analyze_item(&plain);
    assert_eq!(
        serde_json::to_value(&a.pages).unwrap(),
        serde_json::to_value(&b.pages).unwrap()
    );
    assert_eq!(a.text, b.text);
}
#[test]
fn actual_seventeen_unframed_notices_keep_exact_source_membership() {
    let g: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/expected-company-source-fields/20260818g001840038.json"
    ))
    .unwrap();
    native_case("20260818g001840038", 17, &g);
}
#[test]
fn actual_four_mergers_keep_their_eight_related_financial_statements() {
    let g: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/expected-company-article-sources/20260730g001710163.json"
    ))
    .unwrap();
    native_case("20260730g001710163", 4, &g);
}
#[test]
fn actual_seven_notices_preserve_independent_and_related_finance_groups() {
    let g: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/expected-company-article-sources/20260727g001670103.json"
    ))
    .unwrap();
    native_case("20260727g001670103", 7, &g);
}
#[test]
fn quoted_date_and_signature_do_not_supply_the_heading_witness() {
    let mut p = empty();
    notice(&mut p, 400., "任意", "令和八年七月一日");
    push(&mut p, "「", [430., 50., 438., 58.], 2);
    p.regions[0].lines.swap(2, 5);
    push(&mut p, "」", [320., 50., 328., 58.], 2);
    assert!(cuts(&p, &Default::default()).is_empty());
}
#[test]
fn body_region_is_required_for_date_and_signature_witnesses() {
    let mut p = empty();
    notice(&mut p, 400., "任意", "令和八年七月一日");
    let other = p.regions[0].lines.split_off(2);
    p.regions.push(Region {
        role: Role::Before,
        unit: 0,
        part: 0,
        lines: other,
    });
    assert!(cuts(&p, &Default::default()).is_empty());
}
#[test]
fn unrelated_source_font_cannot_complete_a_signature() {
    let mut p = empty();
    notice(&mut p, 400., "任意", "令和八年七月一日");
    p.native_styles[4] = style(9);
    assert!(cuts(&p, &Default::default()).is_empty());
}
#[test]
fn actual_word_emission_reversal_keeps_article_membership_and_all_analysis_fields() {
    for page in [
        "20260721g001610161",
        "20260818g001840038",
        "20260730g001710163",
        "20260727g001670103",
        "20260813g001810062",
        "20260925g002120067",
        "20261005g002190067",
        "20260731h017590032",
    ] {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let pdf = std::fs::read(
            root.join("tests/archive/pages")
                .join(&page[..14])
                .join(format!("{page}.pdf")),
        )
        .unwrap();
        let inputs = crate::page_inputs(&pdf).unwrap();
        let a = crate::analyze_item(&inputs);
        let mut reversed = inputs.clone();
        for p in &mut reversed {
            let mut words = vec![];
            let mut rest = p.xhtml.as_str();
            while let Some(start) = rest.find("<word ") {
                let end = start + rest[start..].find("</word>").unwrap() + "</word>".len();
                words.push(rest[start..end].to_string());
                rest = &rest[end..];
            }
            words.reverse();
            let doc = scraper::Html::parse_document(&p.xhtml);
            let s = scraper::Selector::parse("page").unwrap();
            let page = doc.select(&s).next().unwrap();
            p.xhtml = format!(
                "<doc><page width=\"{}\" height=\"{}\">{}</page></doc>",
                page.value().attr("width").unwrap(),
                page.value().attr("height").unwrap(),
                words.join("")
            );
        }
        let b = crate::analyze_item(&reversed);
        assert_eq!(
            serde_json::to_value(a).unwrap(),
            serde_json::to_value(b).unwrap()
        );
    }
}

#[test]
fn a_complete_date_and_signature_inside_a_multiline_quote_are_not_witnesses() {
    let mut p = empty();
    notice(&mut p, 400., "任意", "令和八年七月一日");
    push(&mut p, "「", [430., 50., 438., 58.], 2);
    let opening = p.regions[0].lines.pop().unwrap();
    p.regions[0].lines.insert(2, opening);
    push(&mut p, "」", [320., 50., 328., 58.], 2);
    assert!(cuts(&p, &Default::default()).is_empty());
}
#[test]
fn a_missing_style_on_the_next_title_still_fences_the_previous_witness_range() {
    let mut p = empty();
    notice(&mut p, 400., "任意", "日付未定");
    notice(&mut p, 300., "任意", "令和八年七月一日");
    p.native_styles[5] = None;
    assert!(cuts(&p, &Default::default()).is_empty());
}
#[test]
fn a_body_style_title_still_fences_the_previous_witness_range() {
    let mut p = empty();
    notice(&mut p, 400., "任意", "日付未定");
    notice(&mut p, 300., "任意", "令和八年七月一日");
    p.native_styles[5] = style(2);
    assert!(cuts(&p, &Default::default()).is_empty());
}
#[test]
fn an_unresolved_parent_does_not_release_its_related_financial_child() {
    let mut p = empty();
    notice(&mut p, 400., "任意", "日付未定");
    push(&mut p, "第1期決算公告", [100., 50., 250., 60.], 3);
    push(&mut p, "令和八年七月一日", [100., 65., 250., 75.], 2);
    push(&mut p, "代表取締役乙", [100., 80., 250., 90.], 2);
    let mut r = crate::lines::PageRules::default();
    r.enclosures = vec![[80., 40., 420., 240.], [90., 45., 260., 100.]];
    assert!(cuts(&p, &r).is_empty());
}

#[test]
fn identical_source_occurrences_with_conflicting_styles_abstain() {
    let mut p = two();
    let mut w = p.words[0].clone();
    w.id = p.words.len();
    let id = w.id;
    p.words.push(w);
    p.native_styles.push(style(9));
    p.regions[0]
        .lines
        .push(Line::text("解散公告".into(), vec![id]));
    assert!(cuts(&p, &Default::default()).is_empty());
}

#[test]
fn an_independent_financial_frame_with_date_and_whole_table_does_not_require_a_signature() {
    let mut p = empty();
    push(&mut p, "第1期決算公告", [100., 50., 250., 60.], 3);
    push(&mut p, "令和八年七月一日", [100., 65., 250., 75.], 2);
    push(&mut p, "資産合計1", [100., 90., 200., 100.], 2);
    let l = p.regions[0].lines.last_mut().unwrap();
    let id = l.words[0];
    l.words.clear();
    l.table = Some(crate::trace::Table {
        x0: 95.,
        y0: 85.,
        x1: 255.,
        y1: 110.,
        rows: 1,
        cols: 1,
        cells: vec![crate::trace::TableCell {
            row: 0,
            col: 0,
            rowspan: 1,
            colspan: 1,
            x0: 95.,
            y0: 85.,
            x1: 255.,
            y1: 110.,
            text: "資産合計1".into(),
            words: vec![id],
        }],
    });
    let mut r = crate::lines::PageRules::default();
    r.enclosures = vec![[90., 45., 260., 120.]];
    assert_eq!(cuts(&p, &r), vec![0]);
    p.regions[0].lines[1].text = "日付未定".into();
    p.words[1].text = "日付未定".into();
    p.words[1].raw = "日付未定".into();
    assert!(cuts(&p, &r).is_empty());
    p.words[1].text = "令和八年七月一日".into();
    p.words[1].raw = p.words[1].text.clone();
    p.regions[0].lines[1].text = p.words[1].text.clone();
    p.regions[0].lines[0].text = "合併公告".into();
    p.words[0].text = "合併公告".into();
    p.words[0].raw = "合併公告".into();
    assert!(cuts(&p, &r).is_empty());
}
#[test]
fn actual_twelve_independent_financial_notices_include_a_signatureless_issuer() {
    let g: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/expected-company-article-sources/20260813g001810062.json"
    ))
    .unwrap();
    native_case("20260813g001810062", 12, &g);
}

#[test]
fn independently_styled_framed_blocks_do_not_require_a_title_keyword() {
    for title in [
        "任意のお知らせ",
        "任意の記録",
        "A completely different title",
        "123",
        "任意公告に関するお知らせ",
        "任意公告（対象未定",
    ] {
        let mut p = empty();
        push(&mut p, "第1期決算公告", [100., 50., 250., 60.], 3);
        push(&mut p, "令和八年七月一日", [100., 65., 250., 75.], 2);
        push(&mut p, "代表取締役乙", [100., 80., 250., 90.], 2);
        push(&mut p, title, [300., 50., 450., 60.], 3);
        push(&mut p, "令和八年七月一日", [300., 65., 450., 75.], 2);
        push(&mut p, "代表取締役甲", [300., 80., 450., 90.], 2);
        let mut r = crate::lines::PageRules::default();
        r.enclosures = vec![[90., 45., 260., 100.], [290., 45., 460., 100.]];
        assert_eq!(cuts(&p, &r), vec![0, 3]);
        r.enclosures.pop();
        assert_eq!(cuts(&p, &r), vec![0]);
    }
}
#[test]
fn an_unclassified_frame_does_not_borrow_the_previous_publication_date() {
    let mut p = empty();
    push(&mut p, "第1期決算公告", [100., 50., 250., 60.], 3);
    push(&mut p, "令和八年七月一日", [100., 65., 250., 75.], 2);
    push(&mut p, "代表取締役乙", [100., 80., 250., 90.], 2);
    push(&mut p, "任意の記録", [300., 50., 450., 60.], 3);
    push(&mut p, "代表取締役甲", [300., 80., 450., 90.], 2);
    let mut r = crate::lines::PageRules::default();
    r.enclosures = vec![[90., 45., 260., 100.], [290., 45., 460., 100.]];
    assert_eq!(cuts(&p, &r), vec![0]);
}
#[test]
fn an_unclassified_framed_table_without_its_own_signature_is_not_an_article() {
    let mut p = empty();
    push(&mut p, "第1期決算公告", [100., 50., 250., 60.], 3);
    push(&mut p, "令和八年七月一日", [100., 65., 250., 75.], 2);
    push(&mut p, "代表取締役乙", [100., 80., 250., 90.], 2);
    push(&mut p, "補足の表", [300., 50., 450., 60.], 3);
    push(&mut p, "令和八年七月一日", [300., 65., 450., 75.], 2);
    push(&mut p, "資産合計1", [300., 80., 450., 90.], 2);
    let mut r = crate::lines::PageRules::default();
    r.enclosures = vec![[90., 45., 260., 100.], [290., 45., 460., 100.]];
    assert_eq!(cuts(&p, &r), vec![0]);
}
#[test]
fn actual_independent_unknown_frame_does_not_leak_into_the_financial_article() {
    let g: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/expected-company-article-sources/20260925g002120067.json"
    ))
    .unwrap();
    native_case("20260925g002120067", 3, &g);
}

#[test]
fn an_open_quote_from_the_previous_page_does_not_authorize_new_article_cuts() {
    let mut first = empty();
    push(&mut first, "引用「", [400., 50., 408., 100.], 2);
    let mut second = two();
    push(&mut second, "」引用終わり", [200., 50., 208., 100.], 2);
    let pages = vec![first, second];
    let text = pages
        .iter()
        .map(|p| p.doc_text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let got = groups(&[text.clone()], &text, &pages, &[]);
    assert_eq!(got[0].len(), 1);
}
#[test]
fn page_quote_scope_resumes_after_the_matching_closure() {
    let mut first = empty();
    push(&mut first, "引用「", [400., 50., 408., 100.], 2);
    let mut second = empty();
    push(&mut second, "」引用終わり", [400., 50., 408., 100.], 2);
    let pages = vec![first, second, two()];
    let text = pages
        .iter()
        .map(|p| p.doc_text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let got = groups(&[text.clone()], &text, &pages, &[]);
    assert_eq!(got[0].len(), 3);
    assert_eq!(got[0][1].sources.len(), 5);
    assert_eq!(got[0][2].sources.len(), 5);
}

#[test]
fn a_parent_cannot_borrow_a_horizontal_childs_date_without_a_separate_child_frame() {
    for title in ["第1期決算公告", "第1期決算公告（任意）"] {
        for font in [Some(3), None, Some(2)] {
            let mut p = empty();
            notice(&mut p, 400., "任意の親組織", "日付未定");
            push(&mut p, title, [100., 50., 250., 60.], 3);
            push(&mut p, "2026年7月1日", [100., 65., 250., 75.], 2);
            push(&mut p, "代表取締役乙", [100., 80., 250., 90.], 2);
            p.native_styles[5] = font.map(style).flatten();
            let mut r = crate::lines::PageRules::default();
            r.enclosures = vec![[90., 40., 420., 240.]];
            assert!(cuts(&p, &r).is_empty(), "{title} {font:?}");
        }
    }
}

#[test]
fn an_announcement_noun_prefix_does_not_make_an_unparsed_compound_a_suffix_title() {
    for compound in ["公告対象の説明", "公告に関する参考", "公告方式の案内"] {
        let mut p = empty();
        push(&mut p, "任意決算公告", [100., 50., 250., 60.], 3);
        push(&mut p, "2026年7月1日", [100., 65., 250., 75.], 2);
        push(&mut p, "代表取締役任意", [100., 80., 250., 90.], 2);
        push(&mut p, compound, [400., 50., 408., 200.], 4);
        push(&mut p, "任意の参考本文です。", [389., 58., 397., 260.], 2);
        let mut rules = crate::lines::PageRules::default();
        rules.enclosures = vec![[90., 40., 420., 280.]];
        assert_eq!(cuts(&p, &rules), vec![0], "{compound}");
    }
}

fn director_signatures(p: &mut PageTrace, signature: &str) {
    for id in [4, 9] {
        p.words[id].text = signature.into();
        p.words[id].raw = signature.into();
        p.regions[0].lines[id].text = signature.into();
    }
    p.text = p.regions[0]
        .lines
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    p.doc_text = p.text.clone();
}
#[test]
fn director_office_prefix_is_independent_of_organization_and_post_labels() {
    for signature in [
        "取締役甲",
        "取締役社長甲",
        "取締役会長甲",
        "取締役頭取甲",
        "取締役副頭取甲",
    ] {
        let mut p = two();
        director_signatures(&mut p, signature);
        assert_eq!(cuts(&p, &Default::default()), vec![0, 5]);
    }
}
#[test]
fn director_signatures_do_not_authorize_an_undated_scope() {
    let mut p = two();
    director_signatures(&mut p, "取締役甲");
    for id in [2, 7] {
        p.words[id].text = "任意本文".into();
        p.words[id].raw = "任意本文".into();
        p.regions[0].lines[id].text = "任意本文".into();
    }
    assert!(cuts(&p, &Default::default()).is_empty());
}
#[test]
fn director_signatures_must_not_share_the_headings_typography() {
    let mut p = two();
    director_signatures(&mut p, "取締役甲");
    for id in [4, 9] {
        p.native_styles[id] = style(1);
    }
    assert!(cuts(&p, &Default::default()).is_empty());
}

#[test]
fn role_sentence_prose_cannot_be_a_signature_witness(){
 for role in ["取締役","代表取締役","代表清算人","清算人","代表社員","代表理事","理事長"]{
  for terminator in ["。","！","？","!","?"]{
   let mut p=two();director_signatures(&mut p,&format!("{role}が行う任意の手続について通知します{terminator}"));
   assert!(cuts(&p,&Default::default()).is_empty(),"a role mention in prose is not a signed witness: {role}/{terminator}");
  }
 }
}

#[test]
fn signature_names_keep_initials_middle_dots_and_hiragana(){
 for signature in ["取締役J.Smith","取締役Ｊ．スミス","代表取締役ベルナール・パトリック","代表理事佐藤あすか"] {let mut p=two();director_signatures(&mut p,signature);assert_eq!(cuts(&p,&Default::default()),vec![0,5],"{signature}");}
}

#[test]
fn legal_administrator_signature_requires_its_own_date_and_non_sentence_tail(){
 for name in ["甲","あすか","J.Smith","ベルナール・パトリック"]{let mut p=two();director_signatures(&mut p,&format!("破産管財人{name}"));assert_eq!(cuts(&p,&Default::default()),vec![0,5]);}
 for end in ["。","！","？","!","?"]{let mut p=two();director_signatures(&mut p,&format!("破産管財人が行う手続を案内します{end}"));assert!(cuts(&p,&Default::default()).is_empty());}
 let mut p=two();director_signatures(&mut p,"破産管財人甲");for id in [2,7]{p.words[id].text="任意本文".into();p.words[id].raw="任意本文".into();p.regions[0].lines[id].text="任意本文".into();}assert!(cuts(&p,&Default::default()).is_empty());
}

#[test]
fn a_line_outside_a_complete_closed_frame_cannot_make_its_root_a_prose_continuation() {
    fn fixture() -> PageTrace {
        let mut p = empty();
        push(&mut p, "任意の別領域の科目", [350., 30., 450., 40.], 1);
        push(&mut p, "任意決算公告", [100., 50., 180., 60.], 1);
        push(&mut p, "2026年7月1日", [200., 50., 300., 60.], 2);
        push(&mut p, "任意の本文です。", [100., 65., 300., 75.], 2);
        push(&mut p, "代表取締役あすか", [200., 80., 300., 90.], 2);
        p.regions[0].lines[1] = Line::text("任意決算公告 2026年7月1日".into(), vec![1, 2]);
        p.regions[0].lines.remove(2);
        p.text = p.regions[0]
            .lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        p.doc_text = p.text.clone();
        p
    }
    for scale in [0.125, 1., 8.] {
        let mut p = fixture();
        for w in &mut p.words {
            w.x0 = w.x0 * scale - 100.;
            w.x1 = w.x1 * scale - 100.;
            w.y0 = w.y0 * scale - 200.;
            w.y1 = w.y1 * scale - 200.;
        }
        let mut r = crate::lines::PageRules::default();
        r.enclosures = vec![[
            90. * scale - 100.,
            45. * scale - 200.,
            310. * scale - 100.,
            100. * scale - 200.,
        ]];
        assert_eq!(cuts(&p, &r), vec![1], "complete own frame at scale={scale}");
    }
    let p = fixture();
    assert!(
        cuts(&p, &Default::default()).is_empty(),
        "without independent ownership the same-style continuation still abstains"
    );
    let mut r = crate::lines::PageRules::default();
    r.enclosures = vec![[90., 20., 470., 100.]];
    assert!(
        cuts(&p, &r).is_empty(),
        "a previous line in the same enclosure cannot be discarded"
    );
}
