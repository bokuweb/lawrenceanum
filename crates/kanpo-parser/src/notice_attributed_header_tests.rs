use super::*;
use crate::trace::{Line, PageTrace, Region, Table, TableCell, Word, WordStatus};

fn push(page: &mut PageTrace, fields: &[(&str, Rect, usize)]) {
    let mut ids = Vec::new();
    for &(text, b, font) in fields {
        let id = page.words.len();
        page.words.push(Word {
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
        page.native_styles
            .push(Some(NativeStyles::from([(font, 8f64.to_bits())])));
        ids.push(id);
    }
    page.regions[0].lines.push(Line::text(
        fields.iter().map(|f| f.0).collect::<Vec<_>>().join("　"),
        ids,
    ));
}

fn publication(
    title: &str,
    issuer: &str,
    address_rows: usize,
    issuer_rows: usize,
) -> (PageTrace, PageRules) {
    let mut page = PageTrace {native_comparison_labels:vec![],native_horizontal_baselines:vec![],native_atomic_columns:vec![],native_axes:vec![],native_rulings:vec![],
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
    };
    push(&mut page, &[(title, [130., 50., 230., 60.], 1)]);
    push(
        &mut page,
        &[
            ("令和八年七月一日", [50., 70., 110., 80.], 2),
            ("任意の所在地", [170., 70., 260., 80.], 2),
        ],
    );
    let mut y = 82.;
    for _ in 0..address_rows {
        push(&mut page, &[("所在地の続き", [170., y, 260., y + 10.], 2)]);
        y += 12.;
    }
    for _ in 0..issuer_rows {
        push(&mut page, &[(issuer, [190., y, 260., y + 10.], 1)]);
        y += 12.;
    }
    push(
        &mut page,
        &[("対象の記録を公開します。", [50., y, 260., y + 10.], 2)],
    );
    y += 20.;
    push(&mut page, &[("結果1", [50., y, 260., y + 10.], 2)]);
    let line = page.regions[0].lines.last_mut().unwrap();
    let ids = std::mem::take(&mut line.words);
    line.table = Some(Table {
        x0: 45.,
        y0: y - 2.,
        x1: 265.,
        y1: y + 12.,
        rows: 1,
        cols: 1,
        cells: vec![TableCell {
            row: 0,
            col: 0,
            rowspan: 1,
            colspan: 1,
            x0: 45.,
            y0: y - 2.,
            x1: 265.,
            y1: y + 12.,
            text: "結果1".into(),
            words: ids,
        }],
    });
    let mut rules = PageRules::default();
    rules.enclosures = vec![[40., 40., 280., y + 20.]];
    (page, rules)
}

fn change(page: &mut PageTrace, id: usize, text: &str) {
    page.words[id].raw = text.into();
    page.words[id].text = text.into();
    for line in &mut page.regions[0].lines {
        if line.words.contains(&id) {
            line.text = line
                .words
                .iter()
                .map(|&i| page.words[i].text.as_str())
                .collect::<Vec<_>>()
                .join("　");
        }
    }
}

#[test]
fn attributed_publication_uses_field_topology_without_title_or_issuer_vocabulary() {
    for title in [
        "別の記録",
        "A different publication",
        "123",
        "任意のお知らせ",
    ] {
        for issuer in [
            "X",
            "A different issuer",
            "きょうかい",
            "任意の非常に長い名前".repeat(40).as_str(),
        ] {
            let (p, r) = publication(title, issuer, 0, 1);
            assert_eq!(cuts(&p, &r), vec![0], "title={title} issuer={issuer}");
        }
    }
}

#[test]
fn wrapped_address_and_attribution_have_no_line_count_caps() {
    for address_rows in [0, 1, 3, 12] {
        for issuer_rows in [1, 2, 8] {
            let (p, r) = publication("記録", "X", address_rows, issuer_rows);
            assert_eq!(cuts(&p, &r), vec![0]);
        }
    }
}

#[test]
fn attributed_header_is_invariant_under_positive_scale_translation_and_font_bijection() {
    for scale in [0.03125, 1., 32.] {
        let (mut p, mut r) = publication("記録", "X", 2, 3);
        for w in &mut p.words {
            w.x0 = w.x0 * scale - 600.;
            w.x1 = w.x1 * scale - 600.;
            w.y0 = w.y0 * scale - 800.;
            w.y1 = w.y1 * scale - 800.;
        }
        let transform = |b: &mut Rect| {
            b[0] = b[0] * scale - 600.;
            b[2] = b[2] * scale - 600.;
            b[1] = b[1] * scale - 800.;
            b[3] = b[3] * scale - 800.;
        };
        for b in &mut r.enclosures {
            transform(b);
        }
        for line in &mut p.regions[0].lines {
            if let Some(t) = &mut line.table {
                let mut b = [t.x0, t.y0, t.x1, t.y1];
                transform(&mut b);
                [t.x0, t.y0, t.x1, t.y1] = b;
                for cell in &mut t.cells {
                    let mut b = [cell.x0, cell.y0, cell.x1, cell.y1];
                    transform(&mut b);
                    [cell.x0, cell.y0, cell.x1, cell.y1] = b;
                }
            }
        }
        for s in p.native_styles.iter_mut().flatten() {
            *s = s
                .iter()
                .map(|&(f, b)| (29 - f, (f64::from_bits(b) * scale as f64).to_bits()))
                .collect();
        }
        assert_eq!(cuts(&p, &r), vec![0]);
    }
}

#[test]
fn missing_dated_address_attribution_or_table_does_not_authorize_a_publication() {
    for omitted in ["date", "address", "issuer", "table", "frame", "body"] {
        let (mut p, mut r) = publication("記録", "X", 0, 1);
        match omitted {
            "date" => change(&mut p, 1, "日付未定"),
            "address" => {
                p.regions[0].lines[1].words.pop();
                p.words[2].status = WordStatus::Noise;
                p.regions[0].lines[1].text = p.words[1].text.clone();
            }
            "issuer" => {
                p.native_styles[3] = Some(NativeStyles::from([(2, 8f64.to_bits())]));
            }
            "table" => {
                let line = p.regions[0].lines.last_mut().unwrap();
                line.words = line.table.take().unwrap().cells[0].words.clone();
            }
            "frame" => r.enclosures.clear(),
            "body" => {
                p.words[4].x0 = 190.;
            }
            _ => unreachable!(),
        }
        assert!(cuts(&p, &r).is_empty(), "missing {omitted}");
    }
}

#[test]
fn attribution_sentence_or_missing_native_evidence_is_not_a_byline() {
    for text in [
        "これは組織が行う手続です。",
        "Please publish!",
        "発行者は未定？",
    ] {
        let (p, r) = publication("記録", text, 0, 1);
        assert!(cuts(&p, &r).is_empty());
    }
    let (p, r) = publication("記録", "X", 0, 1);
    for id in 0..p.words.len() {
        let mut q = p.clone();
        q.native_styles[id] = None;
        assert!(cuts(&q, &r).is_empty(), "missing style {id}");
    }
}

#[test]
fn source_fields_must_be_separate_ordered_and_inside_their_columns() {
    for invalid in [
        "overlap",
        "byline-outside",
        "before-title",
        "address-cross-column",
        "body-first",
    ] {
        let (mut p, r) = publication("記録", "X", 1, 1);
        match invalid {
            "overlap" => p.words[2].x0 = 100.,
            "byline-outside" => p.words[4].x0 = 160.,
            "before-title" => {
                p.words[1].y0 = 45.;
                p.words[1].y1 = 55.;
            }
            "address-cross-column" => p.words[3].x0 = 90.,
            "body-first" => p.regions[0].lines.swap(2, 4),
            _ => unreachable!(),
        }
        assert!(cuts(&p, &r).is_empty(), "{invalid}");
    }
}

#[test]
fn date_prefix_requires_an_original_word_boundary() {
    let (mut p, r) = publication("記録", "X", 0, 1);
    change(&mut p, 1, "令和八年七月一日任意の所在地");
    assert!(cuts(&p, &r).is_empty());
}

#[test]
fn address_punctuation_is_not_content_typography_for_the_date_or_byline() {
    let (mut p, r) = publication("記録", "X", 0, 1);
    p.native_styles[2] = Some(NativeStyles::from([
        (2, 8f64.to_bits()),
        (5, 8f64.to_bits()),
    ]));
    p.native_content_styles = vec![crate::trace::ContentStyles::Compatibility; p.words.len()];
    p.native_content_styles[2] = crate::trace::ContentStyles::Unknown;
    assert_eq!(cuts(&p, &r), vec![0]);
    p.native_content_styles[1] = crate::trace::ContentStyles::Unknown;
    assert!(cuts(&p, &r).is_empty());
    p.native_content_styles[1] = crate::trace::ContentStyles::Compatibility;
    p.native_content_styles[3] = crate::trace::ContentStyles::Unknown;
    assert!(cuts(&p, &r).is_empty());
}

#[test]
fn overlapping_nominal_glyph_boxes_follow_both_ordered_row_endpoints() {
    for overlap in [0.25, 1., 4.] {
        let (mut p, mut r) = publication("記録", "X", 0, 1);
        // A regular baseline progression with tall nominal glyph boxes.
        for w in &mut p.words {
            w.y1 += 12. + overlap;
        }
        let table = p.regions[0]
            .lines
            .last_mut()
            .unwrap()
            .table
            .as_mut()
            .unwrap();
        table.y1 += 20.;
        table.cells[0].y1 += 20.;
        r.enclosures[0][3] += 20.;
        assert_eq!(cuts(&p, &r), vec![0]);
        // Containment of one row in another is not a baseline progression.
        p.words[3].y1 = p.words[1].y1;
        assert!(cuts(&p, &r).is_empty());
    }
}

#[test]
fn wrapped_address_punctuation_does_not_claim_emphasis_or_a_byline() {
    let (mut p, r) = publication("記録", "X", 2, 1);
    p.native_content_styles = vec![crate::trace::ContentStyles::Compatibility; p.words.len()];
    for id in [2, 3, 4] {
        p.native_styles[id] = Some(NativeStyles::from([
            (2, 8f64.to_bits()),
            (5, 8f64.to_bits()),
        ]));
        p.native_content_styles[id] = crate::trace::ContentStyles::Unknown;
    }
    assert_eq!(cuts(&p, &r), vec![0]);
    p.native_styles[5] = p.native_styles[4].clone();
    p.native_content_styles[5] = crate::trace::ContentStyles::Unknown;
    assert!(cuts(&p, &r).is_empty());
}

#[test]
fn an_unresolved_common_parent_does_not_release_an_attributed_child_table() {
    let (mut p, mut r) = publication("記録", "X", 0, 1);
    push(&mut p, &[("親の任意の記録", [300., 45., 500., 55.], 3)]);
    p.regions[0].lines.rotate_right(1);
    r.enclosures.push([30., 30., 550., 180.]);
    assert!(cuts(&p, &r).is_empty());
}

#[test]
fn quoted_or_nonbody_headers_cannot_supply_attribution_evidence() {
    for id in [0, 1, 2, 3] {
        let (mut p, r) = publication("記録", "X", 0, 1);
        let text = format!("「{}」", p.words[id].text);
        change(&mut p, id, &text);
        assert!(cuts(&p, &r).is_empty(), "quoted Word {id}");
    }
    let (mut p, r) = publication("記録", "X", 0, 1);
    p.regions[0].role = Role::Outside;
    assert!(cuts(&p, &r).is_empty());
}

#[test]
fn a_later_table_must_be_whole_inside_the_same_frame_and_below_the_header() {
    for invalid in ["outside", "above", "nonfinite"] {
        let (mut p, r) = publication("記録", "X", 0, 1);
        let table = p.regions[0]
            .lines
            .last_mut()
            .unwrap()
            .table
            .as_mut()
            .unwrap();
        match invalid {
            "outside" => table.x1 = 300.,
            "above" => {
                table.y0 = 60.;
                table.y1 = 75.;
            }
            "nonfinite" => table.y1 = f32::INFINITY,
            _ => unreachable!(),
        }
        assert!(cuts(&p, &r).is_empty(), "{invalid}");
    }
}

#[test]
fn one_complete_table_cannot_hide_a_second_table_crossing_the_enclosure() {
    for invalid in ["outside", "nonfinite", "child-outside", "child-nonfinite"] {
        let (mut p, mut r) = publication("任意の公告", "X", 0, 1);
        let mut second = p.regions[0].lines.last().unwrap().clone();
        r.enclosures[0][3] = 240.;
        if invalid.starts_with("child-") {
            push(&mut p, &[("第2期決算公告", [100., 145., 250., 155.], 3)]);
            push(&mut p, &[("令和八年七月一日", [50., 160., 260., 170.], 2)]);
        }
        let mut word = p.words[second.table.as_ref().unwrap().cells[0].words[0]].clone();
        word.id = p.words.len();
        word.y0 = 180.;
        word.y1 = 190.;
        let table = second.table.as_mut().unwrap();
        table.cells[0].words = vec![word.id];
        table.y0 = 178.;
        table.y1 = 194.;
        table.cells[0].y0 = 178.;
        table.cells[0].y1 = 194.;
        p.native_styles
            .push(p.native_styles.last().unwrap().clone());
        p.words.push(word);
        p.regions[0].lines.push(second);
        assert_eq!(cuts(&p, &r), vec![0], "complete control for {invalid}");
        let table = p.regions[0]
            .lines
            .last_mut()
            .unwrap()
            .table
            .as_mut()
            .unwrap();
        match invalid {
            "outside" | "child-outside" => table.x1 = 300.,
            "nonfinite" | "child-nonfinite" => table.y1 = f32::INFINITY,
            _ => unreachable!(),
        }
        assert!(cuts(&p, &r).is_empty(), "{invalid}");
    }
}

#[test]
fn actual_six_closed_publications_keep_all_source_memberships_including_unsigned_issuer() {
    let gold: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/expected-company-article-sources/20260903g001970080.json"
    ))
    .unwrap();
    super::tests::native_case("20260903g001970080", 6, &gold);
}
