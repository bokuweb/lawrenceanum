use super::*;
use std::collections::BTreeSet;
#[test]
fn actual_six_financial_publications_keep_complete_source_membership() {
    let input = crate::page_inputs(include_bytes!(
        "../tests/archive/pages/20260721g00161/20260721g001610162.pdf"
    ))
    .unwrap();
    let a = crate::analyze_item(&input);
    assert_eq!(
        a.articles.len(),
        6,
        "A closed financial header must remain with its whole source frame"
    );
    let text = crate::trace::join_page_texts(&a.pages, &|p| p.doc_text.as_str());
    let mut original = crate::segment_articles(&text);
    crate::wrapped_notice_titles::repair_wrapped_notice_titles(&mut original, &a.pages);
    let rules: Vec<_> = input.iter().map(|p| p.rules.clone()).collect();
    let parts = super::super::groups(&original, &text, &a.pages, &rules)
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let gold: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/expected-framed-publications/20260721g001610162-gold.json"
    ))
    .unwrap();
    let expected: BTreeSet<Vec<usize>> = gold["expected_source_groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| {
            g["source_word_ids"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_u64().unwrap() as usize)
                .collect()
        })
        .collect();
    let actual: BTreeSet<Vec<usize>> = parts
        .iter()
        .map(|p| {
            assert!(p.sources.iter().all(|(page, _)| *page == 0));
            let mut ids = p.sources.iter().map(|(_, id)| *id).collect::<Vec<_>>();
            ids.sort();
            assert!(ids.windows(2).all(|w| w[0] != w[1]));
            ids
        })
        .collect();
    assert_eq!(actual, expected);
}

use crate::lines::PageRules;
use crate::trace::{
    ContentStyles, Line, NativeBounds, NativeStyles, PageTrace, Region, Role, Table, TableCell,
    Unit, Word, WordStatus,
};
fn put(p: &mut PageTrace, text: &str, b: [f32; 4], font: usize) -> Line {
    let id = p.words.len();
    p.words.push(Word {
        source_box: [b[0], b[1], b[2], b[3]],
        id,
        raw: text.into(),
        text: text.into(),
        x0: b[0],
        y0: b[1],
        x1: b[2],
        y1: b[3],
        status: WordStatus::Used,
    });
    let styles = NativeStyles::from([(font, 8f64.to_bits())]);
    p.native_styles.push(Some(styles.clone()));
    p.native_content_styles.push(ContentStyles::Glyphs(styles));
    p.native_bounds.push(NativeBounds::Glyphs(b));
    Line::text(text.into(), vec![id])
}
fn table(p: &mut PageTrace, value: &str, b: [f32; 4]) -> Line {
    let cell = put(p, value, [b[0] + 5., b[1] + 5., b[0] + 65., b[1] + 15.], 2);
    Line {
        text: value.into(),
        words: vec![],
        table: Some(Table {
            x0: b[0],
            y0: b[1],
            x1: b[2],
            y1: b[3],
            rows: 1,
            cols: 1,
            cells: vec![TableCell {
                row: 0,
                col: 0,
                rowspan: 1,
                colspan: 1,
                x0: b[0],
                y0: b[1],
                x1: b[2],
                y1: b[3],
                text: value.into(),
                words: cell.words,
            }],
        }),
    }
}
fn finish(p: &mut PageTrace, start: usize) {
    let all = p
        .regions
        .iter()
        .flat_map(|r| r.lines.iter().cloned())
        .collect();
    let u = Unit::Plain(all);
    let mut tables = vec![];
    p.text = u.text();
    p.doc_text = u.doc_text(&mut tables, start);
    p.tables = tables;
}
fn fixture(rows: usize) -> (PageTrace, PageRules) {
    let mut p = PageTrace {native_axes:vec![],native_rulings:vec![],
        width: 400.,
        height: 600.,
        layout: "horizontal",
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
    };
    let mut a = vec![
        put(&mut p, "第1期決算公告", [10., 10., 180., 20.], 1),
        put(&mut p, "令和八年七月一日", [10., 30., 90., 40.], 2),
    ];
    for i in 0..rows {
        let y = 50. + i as f32 * 12.;
        a.push(put(&mut p, "任意の発行者欄", [100., y, 180., y + 10.], 2));
    }
    let mut b = vec![
        put(&mut p, "第2期決算公告", [210., 10., 380., 20.], 1),
        put(&mut p, "2026年7月1日", [210., 30., 290., 40.], 2),
    ];
    for i in 0..rows {
        let y = 50. + i as f32 * 12.;
        b.push(put(&mut p, "X", [310., y, 380., y + 10.], 2));
    }
    let top = 90. + rows as f32 * 12.;
    let ta = table(&mut p, "金額111", [10., top, 180., top + 30.]);
    let tb = table(&mut p, "金額222", [210., top, 380., top + 30.]);
    a.extend(b);
    a.push(tb);
    a.push(ta);
    p.regions[0].lines = a;
    finish(&mut p, 0);
    let mut r = PageRules::default();
    r.enclosures = vec![[0., 0., 190., top + 40.], [200., 0., 390., top + 40.]];
    (p, r)
}
fn ids(p: &PageTrace) -> Vec<Vec<usize>> {
    p.regions
        .iter()
        .flat_map(|r| &r.lines)
        .map(|l| {
            l.table.as_ref().map_or_else(
                || l.words.clone(),
                |t| {
                    t.cells
                        .iter()
                        .flat_map(|c| c.words.iter().copied())
                        .collect()
                },
            )
        })
        .collect()
}
fn run(p: &mut PageTrace, r: PageRules) {
    restore_framed_reading_order(std::slice::from_mut(p), &[Some(r)]);
}
fn snapshot(p: &PageTrace) -> serde_json::Value {
    serde_json::json!({"page":p,"doc":p.doc_text,"tables":p.tables})
}
fn conserved(before: &PageTrace, after: &PageTrace) {
    assert_eq!(
        serde_json::to_value(&before.words).unwrap(),
        serde_json::to_value(&after.words).unwrap()
    );
    assert_eq!(before.native_styles, after.native_styles);
    assert_eq!(before.native_bounds, after.native_bounds);
    assert_eq!(before.native_content_styles, after.native_content_styles);
    let lines = |p: &PageTrace| {
        let mut x = p
            .regions
            .iter()
            .flat_map(|r| &r.lines)
            .map(|l| serde_json::to_string(l).unwrap())
            .collect::<Vec<_>>();
        x.sort();
        x
    };
    assert_eq!(lines(before), lines(after));
    let chars = |p: &PageTrace| {
        let mut x = p.text.chars().collect::<Vec<_>>();
        x.sort();
        x
    };
    assert_eq!(chars(before), chars(after));
}
#[test]
fn complete_financial_frames_are_atomic_and_source_values_are_untouched() {
    let (mut p, r) = fixture(0);
    let before = p.clone();
    run(&mut p, r.clone());
    assert_eq!(
        ids(&p),
        vec![vec![0], vec![1], vec![4], vec![2], vec![3], vec![5]]
    );
    conserved(&before, &p);
    assert_eq!(p.tables[0].cells[0].text, "金額111");
    assert_eq!(p.tables[1].cells[0].text, "金額222");
    let once = snapshot(&p);
    run(&mut p, r);
    assert_eq!(snapshot(&p), once);
}
#[test]
fn frame_order_double_borders_and_duplicate_observations_do_not_change_the_result() {
    let (mut expected, r) = fixture(0);
    run(&mut expected, r);
    let (mut p, mut r) = fixture(0);
    r.enclosures.reverse();
    r.enclosures.extend([
        [0., 0., 190., 130.],
        [-1., -1., 191., 131.],
        [199., -1., 391., 131.],
    ]);
    run(&mut p, r);
    assert_eq!(snapshot(&p), snapshot(&expected));
}
#[test]
fn titles_issuer_values_and_wrapped_source_rows_have_no_length_or_count_caps() {
    for rows in [0, 1, 4, 25] {
        let (mut p, r) = fixture(rows);
        let old = p.clone();
        for w in &mut p.words {
            if w.text.ends_with("決算公告") {
                w.text = format!("{}決算公告", "任意の非常に長い標題".repeat(50));
                w.raw = w.text.clone();
            }
        }
        for l in &mut p.regions[0].lines {
            if l.table.is_none() {
                l.text = l.words.iter().map(|&i| p.words[i].text.as_str()).collect();
            }
        }
        finish(&mut p, 0);
        let before = p.clone();
        run(&mut p, r);
        let order = ids(&p);
        let split = rows + 3;
        assert!(order[..split]
            .iter()
            .flatten()
            .all(|&id| id < rows + 2 || id == rows * 2 + 4));
        conserved(&before, &p);
        assert_ne!(ids(&p), ids(&old));
    }
}
#[test]
fn positive_axis_scaling_translation_and_font_bijections_preserve_grouping() {
    for (sx, sy) in [
        (0.03125, 0.03125),
        (1., 1.),
        (32., 32.),
        (0.01, 25.),
        (25., 0.01),
    ] {
        let (mut p, mut r) = fixture(0);
        let transform = |b: &mut [f32; 4]| {
            b[0] = b[0] * sx - 600.;
            b[2] = b[2] * sx - 600.;
            b[1] = b[1] * sy - 800.;
            b[3] = b[3] * sy - 800.;
        };
        for w in &mut p.words {
            let mut b = [w.x0, w.y0, w.x1, w.y1];
            transform(&mut b);
            [w.x0, w.y0, w.x1, w.y1] = b;
        }
        for b in &mut p.native_bounds {
            if let NativeBounds::Glyphs(b) = b {
                transform(b);
            }
        }
        for r in &mut p.regions {
            for l in &mut r.lines {
                if let Some(t) = &mut l.table {
                    let mut b = [t.x0, t.y0, t.x1, t.y1];
                    transform(&mut b);
                    [t.x0, t.y0, t.x1, t.y1] = b;
                    for c in &mut t.cells {
                        let mut b = [c.x0, c.y0, c.x1, c.y1];
                        transform(&mut b);
                        [c.x0, c.y0, c.x1, c.y1] = b;
                    }
                }
            }
        }
        for b in &mut r.enclosures {
            transform(b);
        }
        let change = |s: &NativeStyles| {
            s.iter()
                .map(|&(f, b)| (17 - f, (f64::from_bits(b) * sy as f64).to_bits()))
                .collect()
        };
        for s in p.native_styles.iter_mut().flatten() {
            *s = change(s);
        }
        for s in &mut p.native_content_styles {
            if let ContentStyles::Glyphs(s) = s {
                *s = change(s);
            }
        }
        p.width *= sx;
        p.height *= sy;
        finish(&mut p, 0);
        let before = p.clone();
        run(&mut p, r);
        assert_eq!(
            ids(&p),
            vec![vec![0], vec![1], vec![4], vec![2], vec![3], vec![5]]
        );
        conserved(&before, &p);
    }
}
#[test]
fn every_monotone_interleaving_preserves_the_first_original_heading_order() {
    for a in 0..6 {
        for b in a + 1..6 {
            for c in b + 1..6 {
                let (mut p, r) = fixture(0);
                let lines = p.regions[0].lines.clone();
                let left = [lines[0].clone(), lines[1].clone(), lines[5].clone()];
                let right = [lines[2].clone(), lines[3].clone(), lines[4].clone()];
                let (mut i, mut j) = (0, 0);
                let mut shuffled = Vec::new();
                for n in 0..6 {
                    if [a, b, c].contains(&n) {
                        shuffled.push(left[i].clone());
                        i += 1;
                    } else {
                        shuffled.push(right[j].clone());
                        j += 1;
                    }
                }
                p.regions[0].lines = shuffled;
                finish(&mut p, 0);
                let before = p.clone();
                let first = ids(&p)[0][0];
                run(&mut p, r);
                let expected = if first == 0 {
                    vec![vec![0], vec![1], vec![4], vec![2], vec![3], vec![5]]
                } else {
                    vec![vec![2], vec![3], vec![5], vec![0], vec![1], vec![4]]
                };
                assert_eq!(ids(&p), expected, "shuffle={a},{b},{c}");
                conserved(&before, &p);
            }
        }
    }
}
#[test]
fn missing_frames_unknown_bounds_or_missing_header_witnesses_abstain() {
    for invalid in [
        "frame",
        "bounds",
        "compatibility",
        "title-style",
        "date-style",
        "date",
        "same-style",
        "render",
        "table-copy",
    ] {
        let (mut p, mut r) = fixture(0);
        match invalid {
            "frame" => r.enclosures.clear(),
            "bounds" => p.native_bounds[1] = NativeBounds::Unknown,
            "compatibility" => p.native_bounds[1] = NativeBounds::Compatibility,
            "title-style" => p.native_styles[0] = None,
            "date-style" => p.native_styles[1] = None,
            "date" => {
                p.words[1].text = "日付未定".into();
                p.words[1].raw = p.words[1].text.clone();
                p.regions[0].lines[1].text = p.words[1].text.clone();
                finish(&mut p, 0);
            }
            "same-style" => {
                p.native_styles[1] = p.native_styles[0].clone();
                p.native_content_styles[1] = p.native_content_styles[0].clone();
            }
            "render" => p.doc_text.push_str("未帰属"),
            "table-copy" => p.tables[0].x1 += 1.,
            _ => unreachable!(),
        }
        let before = snapshot(&p);
        run(&mut p, r);
        assert_eq!(snapshot(&p), before, "{invalid}");
    }
}
#[test]
fn crossing_lines_tables_or_inconsistent_occurrence_evidence_never_get_split() {
    for invalid in [
        "line",
        "table",
        "table-nonfinite",
        "conflict",
        "role",
        "frame-nonfinite",
    ] {
        let (mut p, mut r) = fixture(0);
        match invalid {
            "line" => {
                let b = p.regions[0].lines.remove(3);
                p.regions[0].lines[1].text.push_str(&b.text);
                p.regions[0].lines[1].words.extend(b.words);
                finish(&mut p, 0);
            }
            "table" => {
                p.regions[0].lines[5].table.as_mut().unwrap().x1 = 200.;
                finish(&mut p, 0);
            }
            "table-nonfinite" => {
                p.regions[0].lines[5].table.as_mut().unwrap().y1 = f32::INFINITY;
                finish(&mut p, 0);
            }
            "conflict" => {
                let mut w = p.words[0].clone();
                w.id = p.words.len();
                p.native_styles.push(p.native_styles[1].clone());
                p.native_content_styles
                    .push(p.native_content_styles[1].clone());
                p.native_bounds.push(p.native_bounds[0].clone());
                p.regions[0]
                    .lines
                    .push(Line::text(w.text.clone(), vec![w.id]));
                p.words.push(w);
                finish(&mut p, 0);
            }
            "role" => p.regions[0].role = Role::Outside,
            "frame-nonfinite" => r.enclosures[0][2] = f32::INFINITY,
            _ => unreachable!(),
        }
        let before = snapshot(&p);
        run(&mut p, r);
        assert_eq!(snapshot(&p), before, "{invalid}");
    }
}
#[test]
fn non_nested_overlapping_native_claims_are_ambiguous() {
    let (mut p, mut r) = fixture(0);
    r.enclosures.push([5., -5., 195., 145.]);
    let before = snapshot(&p);
    run(&mut p, r);
    assert_eq!(snapshot(&p), before);
}
#[test]
fn a_common_parent_preamble_fences_its_related_children() {
    let (mut p, mut r) = fixture(0);
    let pre = put(&mut p, "親の公告", [10., -15., 180., -5.], 1);
    p.regions[0].lines.insert(0, pre);
    finish(&mut p, 0);
    r.enclosures.push([-20., -20., 400., 160.]);
    let before = snapshot(&p);
    run(&mut p, r);
    assert_eq!(snapshot(&p), before);
}
#[test]
fn a_container_with_no_own_header_is_not_a_common_publication() {
    let (mut p, mut r) = fixture(0);
    r.enclosures.push([-20., -20., 400., 160.]);
    run(&mut p, r);
    assert_eq!(
        ids(&p),
        vec![vec![0], vec![1], vec![4], vec![2], vec![3], vec![5]]
    );
}
#[test]
fn moving_a_closed_quote_cannot_change_an_unrelated_publications_quote_scope() {
    let (mut p, r) = fixture(0);
    let open = put(&mut p, "「引用", [10., 50., 100., 60.], 2);
    let close = put(&mut p, "引用終わり」", [10., 70., 100., 80.], 2);
    p.regions[0].lines.insert(2, open);
    let last = p.regions[0].lines.len() - 1;
    p.regions[0].lines.insert(last, close);
    finish(&mut p, 0);
    let before = snapshot(&p);
    run(&mut p, r);
    assert_eq!(snapshot(&p), before);
}
#[test]
fn fully_quoted_publications_are_not_independent_layout_roots() {
    let (mut p, r) = fixture(0);
    let open = put(&mut p, "「", [-20., 0., -10., 10.], 2);
    let close = put(&mut p, "」", [-20., 140., -10., 150.], 2);
    p.regions[0].lines.insert(0, open);
    p.regions[0].lines.push(close);
    finish(&mut p, 0);
    let before = snapshot(&p);
    run(&mut p, r);
    assert_eq!(snapshot(&p), before);
}
#[test]
fn page_quotations_resume_and_table_markers_rebind_at_the_same_global_offset() {
    let (mut quoted, r) = fixture(0);
    let close = put(&mut quoted, "」", [-20., 140., -10., 150.], 2);
    quoted.regions[0].lines.push(close);
    finish(&mut quoted, 0);
    let quoted_before = snapshot(&quoted);
    let (mut lead, _) = fixture(0);
    lead.words.clear();
    lead.native_styles.clear();
    lead.native_bounds.clear();
    lead.native_content_styles.clear();
    let open = put(&mut lead, "「", [10., 10., 20., 20.], 2);
    lead.regions[0].lines = vec![open];
    finish(&mut lead, 0);
    let (mut after, r2) = fixture(0);
    finish(&mut after, 2);
    let mut pages = vec![lead, quoted, after];
    restore_framed_reading_order(&mut pages, &[None, Some(r), Some(r2)]);
    assert_eq!(snapshot(&pages[1]), quoted_before);
    assert_eq!(
        ids(&pages[2]),
        vec![vec![0], vec![1], vec![4], vec![2], vec![3], vec![5]]
    );
    assert_eq!(pages[2].tables[0].cells[0].text, "金額111");
    assert!(pages[2].doc_text.contains(&crate::trace::table_marker(2)));
    assert!(pages[2].doc_text.contains(&crate::trace::table_marker(3)));
}
#[test]
fn actual_source_word_emission_order_does_not_change_the_native_result() {
    let mut input = crate::page_inputs(include_bytes!(
        "../tests/archive/pages/20260721g00161/20260721g001610162.pdf"
    ))
    .unwrap();
    let before = crate::analyze_item(&input);
    let mut words = input[0]
        .xhtml
        .lines()
        .filter(|l| l.trim_start().starts_with("<word "))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    words.reverse();
    let mut next = words.into_iter();
    input[0].xhtml = input[0]
        .xhtml
        .lines()
        .map(|l| {
            if l.trim_start().starts_with("<word ") {
                next.next().unwrap()
            } else {
                l.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(next.next().is_none());
    let after = crate::analyze_item(&input);
    assert_eq!(
        serde_json::to_value(before).unwrap(),
        serde_json::to_value(after).unwrap()
    );
}

#[test]
fn unrelated_source_lines_preserve_their_order_and_entire_values() {
    let (mut p, r) = fixture(0);
    let pre = put(&mut p, "前置き", [-30., -15., -10., -5.], 2);
    let middle = put(&mut p, "別の本文", [-30., 50., -10., 60.], 2);
    let end = put(&mut p, "後置き", [-30., 140., -10., 150.], 2);
    p.regions[0].lines.insert(0, pre);
    p.regions[0].lines.insert(3, middle);
    p.regions[0].lines.push(end);
    finish(&mut p, 0);
    let before = p.clone();
    run(&mut p, r);
    assert_eq!(
        ids(&p),
        vec![
            vec![6],
            vec![0],
            vec![1],
            vec![4],
            vec![7],
            vec![2],
            vec![3],
            vec![5],
            vec![8]
        ]
    );
    conserved(&before, &p);
}

fn check_original_memberships(pdf: &[u8], gold: &str, baseline: &str, complete: bool) {
    let input = crate::page_inputs(pdf).unwrap();
    let a = crate::analyze_item(&input);
    let text = crate::trace::join_page_texts(&a.pages, &|p| p.doc_text.as_str());
    let mut original = crate::segment_articles(&text);
    crate::wrapped_notice_titles::repair_wrapped_notice_titles(&mut original, &a.pages);
    let rules: Vec<_> = input.iter().map(|p| p.rules.clone()).collect();
    let parts = super::super::groups(&original, &text, &a.pages, &rules)
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let gold: serde_json::Value = serde_json::from_str(gold).unwrap();
    let expected: BTreeSet<Vec<usize>> = gold["expected_source_groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| {
            g["source_word_ids"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_u64().unwrap() as usize)
                .collect()
        })
        .collect();
    let actual: BTreeSet<Vec<usize>> = parts
        .iter()
        .map(|p| {
            assert!(p.sources.iter().all(|(page, _)| *page == 0));
            let mut ids = p.sources.iter().map(|(_, id)| *id).collect::<Vec<_>>();
            ids.sort();
            assert!(ids.windows(2).all(|w| w[0] != w[1]));
            ids
        })
        .collect();

    let baseline: serde_json::Value = serde_json::from_str(baseline).unwrap();
    let expected_rows: BTreeSet<(String, usize)> = {
        let mut counts = std::collections::BTreeMap::new();
        for row in baseline["document_table_rows"].as_array().unwrap() {
            *counts
                .entry(row.as_str().unwrap().to_string())
                .or_insert(0usize) += 1;
        }
        counts.into_iter().collect()
    };
    let actual_rows: BTreeSet<(String, usize)> = {
        let mut counts = std::collections::BTreeMap::new();
        for article in &a.articles {
            let doc = serde_json::to_value(&article.document).unwrap();
            for block in doc["blocks"].as_array().unwrap() {
                assert_eq!(block["kind"], "paragraph");
                for run in block["runs"].as_array().unwrap() {
                    if let Some(table) = run.get("table") {
                        for row in table["rows"].as_array().unwrap() {
                            *counts
                                .entry(serde_json::to_string(row).unwrap())
                                .or_insert(0usize) += 1;
                        }
                    }
                }
            }
        }
        counts.into_iter().collect()
    };
    assert_eq!(actual_rows,expected_rows,"Every complete structured table row, including cell order and Run attributes, must be retained");
    let previous: BTreeSet<Vec<usize>> = baseline["source_groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| {
            g.as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_u64().unwrap() as usize)
                .collect()
        })
        .collect();
    // Baseline is a regression reference, not a correctness oracle. Correcting
    // a mixed old group can reassign lines between old groups, so refinement of
    // that wrong partition would be an invalid constraint. Original frame gold
    // instead forbids new cross-publication pairs among the annotated sources.
    let all =
        |groups: &BTreeSet<Vec<usize>>| groups.iter().flatten().copied().collect::<BTreeSet<_>>();
    assert_eq!(all(&previous), all(&actual));
    let together = |groups: &BTreeSet<Vec<usize>>, a: usize, b: usize| {
        groups
            .iter()
            .any(|g| g.binary_search(&a).is_ok() && g.binary_search(&b).is_ok())
    };
    let gold_groups = expected.iter().collect::<Vec<_>>();
    for i in 0..gold_groups.len() {
        for j in i + 1..gold_groups.len() {
            for &a in gold_groups[i] {
                for &b in gold_groups[j] {
                    assert!(!together(&actual,a,b)||together(&previous,a,b),"No new cross-publication pair may be introduced among original-annotated Words");
                }
            }
        }
    }
    for correct in expected.intersection(&previous) {
        assert!(
            actual.contains(correct),
            "Previously correct original membership must remain exact"
        );
    }
    assert!(expected.intersection(&actual).count()>expected.intersection(&previous).count(),"At least one additional original publication must become exact; unchanged errors are not improvements");

    if complete {
        assert_eq!(actual, expected);
        assert_eq!(a.articles.len(), expected.len());
    }
}

#[test]
fn original_20260803g001740154_publication_source_memberships() {
    check_original_memberships(
        include_bytes!("../tests/archive/pages/20260803g00174/20260803g001740154.pdf"),
        include_str!("../tests/expected-framed-publications/20260803g001740154-gold.json"),
        include_str!(
            "../tests/expected-framed-publications/20260803g001740154-baseline-memberships.json"
        ),
        true,
    );
}

#[test]
fn original_20260807g001780124_publication_source_memberships() {
    check_original_memberships(
        include_bytes!("../tests/archive/pages/20260807g00178/20260807g001780124.pdf"),
        include_str!("../tests/expected-framed-publications/20260807g001780124-gold.json"),
        include_str!(
            "../tests/expected-framed-publications/20260807g001780124-baseline-memberships.json"
        ),
        true,
    );
}

#[test]
fn original_20260826g001900051_publication_source_memberships() {
    check_original_memberships(
        include_bytes!("../tests/archive/pages/20260826g00190/20260826g001900051.pdf"),
        include_str!("../tests/expected-framed-publications/20260826g001900051-gold.json"),
        include_str!(
            "../tests/expected-framed-publications/20260826g001900051-baseline-memberships.json"
        ),
        false,
    );
}

#[test]
fn original_20260930g002160119_publication_source_memberships() {
    check_original_memberships(
        include_bytes!("../tests/archive/pages/20260930g00216/20260930g002160119.pdf"),
        include_str!("../tests/expected-framed-publications/20260930g002160119-gold.json"),
        include_str!(
            "../tests/expected-framed-publications/20260930g002160119-baseline-memberships.json"
        ),
        false,
    );
}

fn signed_frames()->(PageTrace,PageRules) {
 let(mut p,r)=fixture(1);
 for(id,value)in[(0,"任意の手続公告"),(2,"代表取締役任意"),(3,"別の手続公告"),(5,"代表取締役別")]{p.words[id].raw=value.into();p.words[id].text=value.into();let line=p.regions[0].lines.iter_mut().find(|l|l.words==vec![id]).unwrap();line.text=value.into();}
 finish(&mut p,0);(p,r)
}
#[test] fn a_signed_publication_with_a_whole_table_is_atomic_without_a_financial_title() {
 let(mut p,r)=signed_frames();let before=p.clone();run(&mut p,r.clone());
 assert_eq!(ids(&p),vec![vec![0],vec![1],vec![2],vec![6],vec![3],vec![4],vec![5],vec![7]]);
 conserved(&before,&p);let once=snapshot(&p);run(&mut p,r);assert_eq!(snapshot(&p),once);
}
#[test] fn signed_atomic_publications_do_not_depend_on_their_formal_title_value_or_length() {
 let(mut p,r)=signed_frames();let value=format!("{}公告","複数の事業に関する任意の手続".repeat(80));p.words[0].raw=value.clone();p.words[0].text=value.clone();p.regions[0].lines[0].text=value;finish(&mut p,0);
 run(&mut p,r);assert_eq!(ids(&p)[3],vec![6]);
}
#[test] fn a_generic_closed_frame_still_needs_its_own_date_and_signature() {
 for id in [1,2] {
  let(mut p,r)=signed_frames();p.words[id].raw="任意の通常文".into();p.words[id].text="任意の通常文".into();p.regions[0].lines.iter_mut().find(|l|l.words==vec![id]).unwrap().text="任意の通常文".into();finish(&mut p,0);
  let before=snapshot(&p);run(&mut p,r);assert_eq!(snapshot(&p),before);
 }
}
