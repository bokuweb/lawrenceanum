use super::*;
use crate::{
    native_word_axes::NativeAxis,
    trace::{ContentStyles, NativeBounds, Region, Table, TableCell, Word},
};
fn add(p: &mut PageTrace, text: &str, b: Rect, font: usize, size: f64) -> usize {
    let id = p.words.len();
    p.words.push(Word {
        id,
        source_box: b,
        x0: b[0],
        y0: b[1],
        x1: b[2],
        y1: b[3],
        raw: text.into(),
        text: text.into(),
        status: WordStatus::Used,
    });
    let s = NativeStyles::from([(font, size.to_bits())]);
    p.native_styles.push(Some(s.clone()));
    p.native_content_styles.push(ContentStyles::Glyphs(s));
    p.native_bounds.push(NativeBounds::Glyphs(b));
    p.native_axes.push(NativeAxis::Right);
    p.regions[0].lines.push(Line::text(text.into(), vec![id]));
    id
}
fn empty() -> PageTrace {
    PageTrace {
        width: 800.,
        height: 800.,
        layout: "horizontal",
        words: vec![],
        native_styles: vec![],
        native_content_styles: vec![],
        native_bounds: vec![],
        native_axes: vec![],
        native_rulings: vec![],
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
fn refresh(p: &mut PageTrace) {
    p.doc_text = p
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    p.text = p.doc_text.clone();
}
struct F {
    p: PageTrace,
    first: usize,
    last: usize,
    date: usize,
    address: usize,
    issuer: usize,
    table: usize,
}
fn financial(prefix_lines: usize, next_column: bool) -> F {
    let mut p = empty();
    add(&mut p, "以前の公表の本文。", [50., 10., 350., 20.], 1, 8.);
    let first = p.words.len();
    let mut y = 40.;
    for _ in 0..prefix_lines {
        add(
            &mut p,
            "無名の団体の任意の期間の",
            [100., y, 250., y + 9.2],
            2,
            9.,
        );
        y += 9.;
    }
    let last = add(&mut p, "決算に関する公告", [100., y, 250., y + 9.2], 2, 9.);
    y += 20.;
    let date = add(&mut p, "2026年7月1日", [50., y, 150., y + 8.], 1, 8.);
    y += 12.;
    let address = add(&mut p, "任意の所在地", [200., y, 350., y + 8.], 1, 8.);
    y += 12.;
    let issuer = add(&mut p, "任意の機関", [270., y, 350., y + 8.], 1, 8.);
    y += 12.;
    let x = if next_column { 400. } else { 0. };
    if next_column {
        y = 40.;
    }
    add(&mut p, "１ 短期経理", [42. + x, y, 160. + x, y + 8.], 1, 8.);
    y += 12.;
    add(&mut p, "任意の表題", [150. + x, y, 250. + x, y + 8.], 1, 8.);
    y += 20.;
    let a = add(&mut p, "資産", [60. + x, y + 5., 100. + x, y + 13.], 1, 8.);
    let b = add(&mut p, "123", [250. + x, y + 5., 280. + x, y + 13.], 1, 8.);
    p.regions[0].lines.pop();
    p.regions[0].lines.pop();
    let t = Table {
        x0: 50. + x,
        y0: y,
        x1: 350. + x,
        y1: y + 30.,
        rows: 1,
        cols: 2,
        cells: vec![
            TableCell {
                row: 0,
                col: 0,
                rowspan: 1,
                colspan: 1,
                x0: 50. + x,
                y0: y,
                x1: 200. + x,
                y1: y + 30.,
                text: "資産".into(),
                words: vec![a],
            },
            TableCell {
                row: 0,
                col: 1,
                rowspan: 1,
                colspan: 1,
                x0: 200. + x,
                y0: y,
                x1: 350. + x,
                y1: y + 30.,
                text: "123".into(),
                words: vec![b],
            },
        ],
    };
    let table = p.regions[0].lines.len();
    p.regions[0].lines.push(Line {
        text: t.render(),
        words: vec![],
        table: Some(t.clone()),
    });
    p.tables.push(t);
    refresh(&mut p);
    F {
        p,
        first,
        last,
        date,
        address,
        issuer,
        table,
    }
}
fn replace(p: &mut PageTrace, id: usize, text: &str) {
    p.words[id].text = text.into();
    p.words[id].raw = text.into();
    let l = p.regions[0]
        .lines
        .iter_mut()
        .find(|l| l.words == vec![id])
        .unwrap();
    l.text = text.into();
    refresh(p);
}
#[test]
fn a_wrapped_dated_financial_header_owns_its_following_source_table() {
    let f = financial(1, false);
    assert_eq!(cuts(&f.p, &Default::default()), vec![f.first]);
}
#[test]
fn title_wrapping_has_no_source_line_count_ceiling() {
    for count in [0, 1, 5, 64] {
        let f = financial(count, false);
        assert_eq!(cuts(&f.p, &Default::default()), vec![f.first], "{count}");
    }
}
#[test]
fn a_financial_header_may_finish_in_the_preceding_source_column() {
    let f = financial(2, true);
    assert_eq!(cuts(&f.p, &Default::default()), vec![f.first]);
}
#[test]
fn unknown_content_font_on_an_opaque_address_is_not_date_or_issuer_font_evidence() {
    let mut f = financial(1, false);
    f.p.native_content_styles[f.address] = ContentStyles::Unknown;
    assert_eq!(cuts(&f.p, &Default::default()), vec![f.first]);
}
#[test]
fn arbitrary_institution_names_and_source_font_bijections_do_not_change_membership() {
    for name in ["X", "名前に依存しない機関", "Unknown Organization"] {
        let mut f = financial(1, false);
        replace(&mut f.p, f.issuer, name);
        for s in f.p.native_styles.iter_mut().flatten() {
            *s = s.iter().map(|&(i, z)| (99 - i, z)).collect();
        }
        for s in &mut f.p.native_content_styles {
            if let ContentStyles::Glyphs(s) = s {
                *s = s.iter().map(|&(i, z)| (99 - i, z)).collect();
            }
        }
        assert_eq!(cuts(&f.p, &Default::default()), vec![f.first]);
    }
}
#[test]
fn ordinary_body_font_or_missing_native_header_evidence_cannot_authorize_a_publication() {
    for id in [1, 2, 3, 5] {
        let mut f = financial(1, false);
        f.p.native_content_styles[id] = ContentStyles::Unknown;
        assert!(cuts(&f.p, &Default::default()).is_empty());
    }
    let mut f = financial(1, false);
    for id in f.first..=f.last {
        f.p.native_styles[id] = f.p.native_styles[f.date].clone();
        f.p.native_content_styles[id] = f.p.native_content_styles[f.date].clone();
    }
    assert!(cuts(&f.p, &Default::default()).is_empty());
}
#[test]
fn a_date_must_belong_to_the_header_and_not_a_later_table_note() {
    for t in ["日付未定", "2026年99月1日", "2026年7月0日"] {
        let mut f = financial(1, false);
        replace(&mut f.p, f.date, t);
        assert!(cuts(&f.p, &Default::default()).is_empty());
    }
    let mut f = financial(1, false);
    f.p.regions[0].lines.swap(f.date, f.issuer + 1);
    refresh(&mut f.p);
    assert!(cuts(&f.p, &Default::default()).is_empty());
}
#[test]
fn the_right_aligned_header_block_cannot_be_supplied_by_the_next_publication() {
    let mut f = financial(1, false);
    replace(&mut f.p, f.address, "他の独立した公告");
    assert!(cuts(&f.p, &Default::default()).is_empty());
}
#[test]
fn finance_without_its_own_following_complete_table_is_not_certified() {
    let mut f = financial(1, false);
    let ids: Vec<_> = f.p.regions[0].lines[f.table]
        .table
        .as_ref()
        .unwrap()
        .cells
        .iter()
        .flat_map(|c| &c.words)
        .copied()
        .collect();
    f.p.regions[0].lines.pop();
    for id in ids {
        f.p.words[id].status = WordStatus::Noise;
    }
    assert!(cuts(&f.p, &Default::default()).is_empty());
}
#[test]
fn explicit_unknown_bounds_axes_and_source_role_cannot_use_compatibility_geometry() {
    for id in [1, 2, 3, 4, 5] {
        let mut f = financial(1, false);
        f.p.native_bounds[id] = NativeBounds::Unknown;
        assert!(cuts(&f.p, &Default::default()).is_empty());
        for a in [
            NativeAxis::Unknown,
            NativeAxis::Compatibility,
            NativeAxis::Down,
        ] {
            let mut f = financial(1, false);
            f.p.native_axes[id] = a;
            assert!(cuts(&f.p, &Default::default()).is_empty());
        }
    }
    let mut f = financial(1, false);
    f.p.regions[0].role = Role::Before;
    assert!(cuts(&f.p, &Default::default()).is_empty());
}
#[test]
fn source_regions_tables_and_enclosures_are_not_transparent_to_the_header_proof() {
    let mut f = financial(1, false);
    let ls = f.p.regions[0].lines.split_off(f.date);
    f.p.regions.push(Region {
        role: Role::Body,
        unit: 1,
        part: 0,
        lines: ls,
    });
    assert!(cuts(&f.p, &Default::default()).is_empty());
    let f = financial(1, false);
    let mut r = PageRules::default();
    r.enclosures.push([40., 30., 360., 300.]);
    assert!(cuts(&f.p, &r).is_empty());
}
#[test]
fn quoted_or_rendered_text_mismatched_headers_are_not_certificates() {
    let mut f = financial(1, false);
    replace(&mut f.p, 0, "引用「");
    assert!(cuts(&f.p, &Default::default()).is_empty());
    let mut f = financial(1, false);
    f.p.regions[0].lines[f.last].text = "別の決算に関する公告".into();
    assert!(cuts(&f.p, &Default::default()).is_empty());
}
#[test]
fn publication_date_cannot_be_above_the_last_original_title_baseline() {
    for count in [1, 5] {
        let mut f = financial(count, false);
        let NativeBounds::Glyphs(mut b) = f.p.native_bounds[f.date] else {
            unreachable!()
        };
        let NativeBounds::Glyphs(last) = f.p.native_bounds[f.last] else {
            unreachable!()
        };
        b[1] = last[1] - 1.;
        b[3] = b[1] + 8.;
        f.p.native_bounds[f.date] = NativeBounds::Glyphs(b);
        assert!(cuts(&f.p, &Default::default()).is_empty());
    }
}
#[test]
fn unknown_font_on_inline_address_does_not_invalidate_complete_original_date_prefix() {
    let mut f = financial(1, false);
    let NativeBounds::Glyphs(mut b) = f.p.native_bounds[f.address] else {
        unreachable!()
    };
    let NativeBounds::Glyphs(d) = f.p.native_bounds[f.date] else {
        unreachable!()
    };
    b[1] = d[1];
    b[3] = d[3];
    f.p.native_bounds[f.address] = NativeBounds::Glyphs(b);
    f.p.native_content_styles[f.address] = ContentStyles::Unknown;
    let address = f.p.regions[0].lines.remove(f.address);
    f.p.regions[0].lines[f.date].words.extend(address.words);
    f.p.regions[0].lines[f.date].text.push(' ');
    f.p.regions[0].lines[f.date].text.push_str(&address.text);
    refresh(&mut f.p);
    assert_eq!(cuts(&f.p, &Default::default()), vec![f.first]);
}
fn signed(instrument: &str) -> PageTrace {
    let mut p = empty();
    add(&mut p, "以前の本文。", [50., 10., 350., 20.], 1, 8.);
    add(&mut p, "任意団体公告", [100., 40., 250., 49.2], 2, 9.);
    add(&mut p, instrument, [50., 60., 350., 68.], 2, 8.);
    add(
        &mut p,
        "本件の手続を公告する。",
        [50., 80., 350., 88.],
        1,
        8.,
    );
    add(&mut p, "2026年7月1日", [50., 100., 150., 108.], 1, 8.);
    add(&mut p, "任意団体", [260., 120., 350., 128.], 1, 8.);
    add(&mut p, "任意支局", [280., 140., 350., 148.], 1, 8.);
    let role = add(&mut p, "管理部長", [280., 160., 310., 168.], 1, 8.);
    let name = add(&mut p, "任意", [320., 160., 350., 168.], 1, 8.);
    p.regions[0].lines.pop();
    p.regions[0].lines.pop();
    p.regions[0]
        .lines
        .push(Line::text("管理部長 任意".into(), vec![role, name]));
    refresh(&mut p);
    p
}
#[test]
fn signed_numbered_publication_has_a_nominal_source_office_not_a_known_issuer() {
    assert_eq!(
        cuts(&signed("任意団体支局公告第七号"), &Default::default()),
        vec![1]
    );
}
#[test]
fn malformed_child_instrument_cannot_be_silently_treated_as_the_parents_header() {
    for t in [
        "任意団体支局公告第十十号",
        "任意団体支局公告第〇号",
        "他団体支局公告第七号",
    ] {
        assert!(cuts(&signed(t), &Default::default()).is_empty(), "{t}");
    }
}
#[test]
fn a_following_unknown_title_fences_the_first_notices_delayed_signature() {
    let mut p = signed("任意団体支局公告第七号");
    replace(&mut p, 3, "別の独立した公告");
    p.native_content_styles[3] = ContentStyles::Unknown;
    assert!(cuts(&p, &Default::default()).is_empty());
}
#[test]
fn an_office_role_in_running_prose_is_not_a_nominal_source_signature() {
    let mut p = signed("任意団体支局公告第七号");
    p.regions[0].lines.last_mut().unwrap().text = "管理部長 任意と協議する。".into();
    let last = p.words.len() - 1;
    p.words[last].text = "任意と協議する。".into();
    p.words[last].raw = p.words[last].text.clone();
    assert!(cuts(&p, &Default::default()).is_empty());
}
#[test]
fn splitting_a_person_name_does_not_hide_a_sentence_after_the_office_role() {
    let mut p = signed("任意団体支局公告第七号");
    let name = p.words.len() - 1;
    p.words[name].text = "甲".into();
    p.words[name].raw = "甲".into();
    let extra = add(&mut p, "乙と協議する。", [340., 160., 350., 168.], 1, 8.);
    p.regions[0].lines.pop();
    let l = p.regions[0].lines.last_mut().unwrap();
    l.words.push(extra);
    l.text = "管理部長 甲 乙と協議する。".into();
    refresh(&mut p);
    assert!(cuts(&p, &Default::default()).is_empty());
}
#[test]
fn positive_scaling_translation_and_table_geometry_preserve_the_publication_proof() {
    fn transform(b: Rect, k: f32) -> Rect {
        [
            b[0] * k - 100.,
            b[1] * k - 200.,
            b[2] * k - 100.,
            b[3] * k - 200.,
        ]
    }
    fn table(t: &mut Table, k: f32) {
        let b = transform([t.x0, t.y0, t.x1, t.y1], k);
        t.x0 = b[0];
        t.y0 = b[1];
        t.x1 = b[2];
        t.y1 = b[3];
        for c in &mut t.cells {
            let b = transform([c.x0, c.y0, c.x1, c.y1], k);
            c.x0 = b[0];
            c.y0 = b[1];
            c.x1 = b[2];
            c.y1 = b[3];
        }
    }
    for k in [0.125, 1., 8.] {
        let mut f = financial(5, true);
        for i in 0..f.p.words.len() {
            let w = &mut f.p.words[i];
            let b = transform(w.source_box, k);
            w.source_box = b;
            w.x0 = b[0];
            w.y0 = b[1];
            w.x1 = b[2];
            w.y1 = b[3];
            f.p.native_bounds[i] = NativeBounds::Glyphs(b);
            if let Some(s) = &mut f.p.native_styles[i] {
                *s = s
                    .iter()
                    .map(|&(font, z)| (font, (f64::from_bits(z) * k as f64).to_bits()))
                    .collect();
            }
            if let ContentStyles::Glyphs(s) = &mut f.p.native_content_styles[i] {
                *s = s
                    .iter()
                    .map(|&(font, z)| (font, (f64::from_bits(z) * k as f64).to_bits()))
                    .collect();
            }
        }
        for l in &mut f.p.regions[0].lines {
            if let Some(t) = &mut l.table {
                table(t, k);
            }
        }
        for t in &mut f.p.tables {
            table(t, k);
        }
        assert_eq!(cuts(&f.p, &Default::default()), vec![f.first]);
    }
}
fn document_stream(p: &mut PageTrace) {
    let mut table = 0;
    p.doc_text = p
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .map(|l| {
            if l.table.is_some() {
                let s = crate::trace::table_marker(table);
                table += 1;
                s
            } else {
                l.text.clone()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
}
#[test]
fn a_section_label_is_not_a_separate_publication_before_the_first_certified_title() {
    for label in ["諸事項", "官 庁 報 告", "公告"] {
        let mut f = financial(1, false);
        replace(&mut f.p, 0, label);
        document_stream(&mut f.p);
        let text = f.p.doc_text.clone();
        let original = vec![text.clone()];
        let result = groups(&original, &text, std::slice::from_ref(&f.p), &[None]);
        assert_eq!(result[0].len(), 1, "{label}");
        assert!(result[0][0].text.starts_with(label));
    }
}
#[test]
fn an_existing_publication_body_before_the_new_title_still_requires_a_boundary() {
    let mut f = financial(1, false);
    document_stream(&mut f.p);
    let text = f.p.doc_text.clone();
    let original = vec![text.clone()];
    let result = groups(&original, &text, std::slice::from_ref(&f.p), &[None]);
    assert_eq!(result[0].len(), 2);
    assert_eq!(result[0][0].text, "以前の公表の本文。");
}
