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
    PageTrace {native_comparison_labels:vec![],native_horizontal_baselines:vec![],native_atomic_columns:vec![],
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

// A printed date/address row may be two logical Lines in the same source scope.
fn split_header140(address: Rect) -> PageTrace {
    let mut p = signed("任意団体支局公告第七号");
    add(&mut p, "opaque address", address, 1, 8.);
    let address = p.regions[0].lines.pop().unwrap();
    p.regions[0].lines.insert(5, address);
    p.native_horizontal_baselines=vec![None;p.words.len()];
    p.native_horizontal_baselines[4]=Some(104.);
    let id=p.words.len()-1;p.native_horizontal_baselines[id]=Some(104.);
    refresh(&mut p);
    p
}
#[test]
fn shared_header140_two_disjoint_fields_on_one_native_row_are_not_a_reversal() {
    assert_eq!(cuts(&split_header140([200.,100.,350.,108.]), &Default::default()), vec![1]);
}
#[test]
fn shared_header140_overlapping_or_leftward_fields_are_not_one_ordered_row() {
    for b in [[120.,100.,350.,108.],[10.,100.,40.,108.]] {
        assert!(cuts(&split_header140(b), &Default::default()).is_empty());
    }
}
#[test]
fn shared_header140_source_baseline_handles_different_glyph_extents() {
    for b in [[200.,100.,350.,109.],[200.,99.,350.,108.]] {
        assert_eq!(cuts(&split_header140(b), &Default::default()),vec![1]);
    }
}
#[test]
fn shared_header140_missing_source_axis_or_bounds_cannot_use_legacy_coordinates() {
    for b in [true,false] {
        let mut p=split_header140([200.,100.,350.,108.]);let id=p.words.len()-1;
        if b {p.native_bounds[id]=NativeBounds::Unknown;}else{p.native_axes[id]=NativeAxis::Unknown;}
        assert!(cuts(&p,&Default::default()).is_empty());
    }
}
#[test]
fn shared_header140_opaque_address_content_font_is_not_date_or_signature_evidence() {
    let mut p=split_header140([200.,100.,350.,108.]);let id=p.words.len()-1;p.native_content_styles[id]=ContentStyles::Unknown;
    assert_eq!(cuts(&p,&Default::default()),vec![1]);
}
#[test]
fn shared_header140_cannot_borrow_a_signature_across_a_source_region_boundary() {
    let mut p=split_header140([200.,100.,350.,108.]);let lines=p.regions[0].lines.split_off(5);
    p.regions.push(Region{role:Role::Body,unit:1,part:0,lines});refresh(&mut p);
    assert!(cuts(&p,&Default::default()).is_empty());
}

#[test]
fn shared_header140_absent_nonfinite_or_foreign_baseline_is_not_legacy_row_evidence() {
    for baseline in [None,Some(f64::NAN),Some(103.)] {
        let mut p=split_header140([200.,100.,350.,108.]);let id=p.words.len()-1;
        p.native_horizontal_baselines[id]=baseline;
        assert!(cuts(&p,&Default::default()).is_empty());
    }
    let mut p=split_header140([200.,100.,350.,108.]);p.native_horizontal_baselines.clear();
    assert!(cuts(&p,&Default::default()).is_empty());
}
#[test]
fn shared_header140_every_word_of_the_fragment_needs_the_same_source_baseline() {
    let mut p=split_header140([200.,100.,350.,108.]);
    let extra=add(&mut p,"extra",[351.,100.,370.,108.],1,8.);
    p.native_horizontal_baselines.push(Some(103.));
    let extra_line=p.regions[0].lines.pop().unwrap();
    p.regions[0].lines[5].text.push_str(" extra");p.regions[0].lines[5].words.extend(extra_line.words);
    assert_eq!(extra,p.words.len()-1);refresh(&mut p);
    assert!(cuts(&p,&Default::default()).is_empty());
}

fn header141_fixture() -> (PageTrace, PageRules) {
    let mut f = financial(2, false);
    let original = f.p.regions[0].lines.clone();
    let date = f.p.native_bounds[f.date].clone();
    let NativeBounds::Glyphs(date) = date else { unreachable!() };
    let NativeBounds::Glyphs(mut address) = f.p.native_bounds[f.address].clone() else { unreachable!() };
    address[1] = date[1]; address[3] = date[3];
    for (id, b) in [(f.address, address)] {
        let w = &mut f.p.words[id];
        w.x0=b[0]; w.y0=b[1]; w.x1=b[2]; w.y1=b[3]; w.source_box=b;
        f.p.native_bounds[id]=NativeBounds::Glyphs(b);
    }
    let cap = original.iter().position(|l| l.text == "１ 短期経理").unwrap();
    for &id in &original[cap].words {
        let w=&mut f.p.words[id]; w.x0=50.; w.source_box[0]=50.;
        let NativeBounds::Glyphs(b)=&mut f.p.native_bounds[id] else {unreachable!()};b[0]=50.;
    }
    let date_line=original.iter().position(|l|l.words==vec![f.date]).unwrap();
    let mut order=vec![0,date_line,cap];
    order.extend(1..date_line);
    order.extend((date_line+1..original.len()).filter(|i|*i!=cap));
    f.p.regions[0].lines=order.into_iter().map(|i|original[i].clone()).collect();
    f.p.native_horizontal_baselines=f.p.native_bounds.iter().map(|b|match b {NativeBounds::Glyphs(b)=>Some((f64::from(b[1])+f64::from(b[3]))/2.),_=>None}).collect();
    let table=&f.p.tables[0];
    let rules=PageRules {
        horizontal:vec![crate::lines::HRule{y:table.y0,x0:table.x0,x1:table.x1}],
        vertical:vec![crate::lines::VRule{x:table.x0,y0:table.y0,y1:table.y1},crate::lines::VRule{x:table.x1,y0:table.y0,y1:table.y1}],
        ..Default::default()
    };
    header141_seal(&mut f.p);
    (f.p,rules)
}
fn header141_seal(p:&mut PageTrace) {
    let unit=crate::trace::Unit::Plain(p.regions.iter().flat_map(|r|r.lines.iter().cloned()).collect());
    let mut tables=vec![];p.text=unit.text();p.doc_text=unit.doc_text(&mut tables,0);p.tables=tables;
}
fn header141_restore(p:&mut PageTrace,r:&PageRules) {
    framed_publications::restore_framed_reading_order(std::slice::from_mut(p),&[Some(r.clone())]);
}
fn header141_first_id(p:&PageTrace,text:&str)->usize {
    p.words.iter().find(|w|w.text==text).unwrap().id
}
fn header141_position(p:&PageTrace,id:usize)->usize {
    p.regions.iter().flat_map(|r|&r.lines).position(|l|l.words.contains(&id)).unwrap()
}
#[test]
fn header141_restores_source_parent_header_before_its_statement() {
    let (mut p,r)=header141_fixture();
    let title=header141_first_id(&p,"無名の団体の任意の期間の");
    let date=header141_first_id(&p,"2026年7月1日");
    let issuer=header141_first_id(&p,"任意の機関");
    let caption=header141_first_id(&p,"１ 短期経理");
    header141_restore(&mut p,&r);
    assert!(header141_position(&p,title)<header141_position(&p,date));
    assert!(header141_position(&p,issuer)<header141_position(&p,caption));
    assert_eq!(cuts(&p,&r),vec![1]);
}
#[test]
fn header141_missing_source_or_baseline_keeps_the_original() {
    for kind in 0..4 {
        let(mut p,r)=header141_fixture();let id=header141_first_id(&p,"任意の所在地");
        match kind {0=>p.native_bounds[id]=NativeBounds::Unknown,1=>p.native_axes[id]=NativeAxis::Unknown,2=>p.native_horizontal_baselines[id]=None,_=>p.native_content_styles[id]=ContentStyles::Unknown}
        let old=serde_json::to_string(&p).unwrap();header141_restore(&mut p,&r);
        assert_eq!(serde_json::to_string(&p).unwrap(),old);
    }
}
#[test]
fn header141_native_table_top_and_both_outer_edges_are_required() {
    for kind in 0..3 {
        let(mut p,mut r)=header141_fixture();
        if kind==0 {r.horizontal.clear();}else{r.vertical.remove(kind-1);}
        let old=serde_json::to_string(&p).unwrap();header141_restore(&mut p,&r);
        assert_eq!(serde_json::to_string(&p).unwrap(),old);
    }
}
#[test]
fn header141_real_column_separator_fences_the_header() {
    let(mut p,mut r)=header141_fixture();r.vertical.push(crate::lines::VRule{x:175.,y0:30.,y1:p.tables[0].y0});
    let old=serde_json::to_string(&p).unwrap();header141_restore(&mut p,&r);assert_eq!(serde_json::to_string(&p).unwrap(),old);
}
#[test]
fn header141_cannot_reorder_a_header_across_regions() {
    let(mut p,r)=header141_fixture();let rest=p.regions[0].lines.split_off(3);
    p.regions.push(Region{role:Role::Body,unit:1,part:0,lines:rest});header141_seal(&mut p);
    let old=serde_json::to_string(&p).unwrap();header141_restore(&mut p,&r);assert_eq!(serde_json::to_string(&p).unwrap(),old);
}
#[test]
fn header141_mismatched_document_rendering_is_not_repaired() {
    let(mut p,r)=header141_fixture();p.doc_text.push_str(" unmatched source");
    let old=serde_json::to_string(&p).unwrap();header141_restore(&mut p,&r);assert_eq!(serde_json::to_string(&p).unwrap(),old);
}
#[test]
fn header141_quoted_header_is_not_an_independent_publication() {
    let(mut p,r)=header141_fixture();
    let first=header141_first_id(&p,"無名の団体の任意の期間の");
    replace(&mut p,first,"「無名の団体の任意の期間の");
    let last=header141_first_id(&p,"任意の表題");replace(&mut p,last,"任意の表題」");header141_seal(&mut p);
    let old=serde_json::to_string(&p).unwrap();header141_restore(&mut p,&r);assert_eq!(serde_json::to_string(&p).unwrap(),old);
}
#[test]
fn header141_source_geometry_is_invariant_under_scale_and_translation() {
    for (s,dx,dy) in [(0.5,0.,0.),(2.,128.,64.)] {
        let(mut p,mut rules)=header141_fixture();
        let rect=|b:Rect|[b[0]*s+dx,b[1]*s+dy,b[2]*s+dx,b[3]*s+dy];
        for w in &mut p.words {let b=rect([w.x0,w.y0,w.x1,w.y1]);w.x0=b[0];w.y0=b[1];w.x1=b[2];w.y1=b[3];w.source_box=rect(w.source_box);}
        for b in &mut p.native_bounds {if let NativeBounds::Glyphs(b)=b{*b=rect(*b);}}
        for b in &mut p.native_horizontal_baselines {if let Some(b)=b{*b=*b*f64::from(s)+f64::from(dy);}}
        for r in &mut p.regions {for l in &mut r.lines {if let Some(t)=&mut l.table {let b=rect([t.x0,t.y0,t.x1,t.y1]);t.x0=b[0];t.y0=b[1];t.x1=b[2];t.y1=b[3];for c in &mut t.cells {let b=rect([c.x0,c.y0,c.x1,c.y1]);c.x0=b[0];c.y0=b[1];c.x1=b[2];c.y1=b[3];}}}}
        for r in &mut rules.vertical{r.x=r.x*s+dx;r.y0=r.y0*s+dy;r.y1=r.y1*s+dy;}
        for r in &mut rules.horizontal{r.y=r.y*s+dy;r.x0=r.x0*s+dx;r.x1=r.x1*s+dx;}
        header141_seal(&mut p);header141_restore(&mut p,&rules);assert_eq!(cuts(&p,&rules),vec![1]);
    }
}

#[test]
fn header141_inset_date_remains_inside_its_own_ruled_table_parent() {
    let(mut p,r)=header141_fixture();let id=header141_first_id(&p,"2026年7月1日");
    let NativeBounds::Glyphs(mut b)=p.native_bounds[id] else {unreachable!()};b[0]+=48.;b[2]+=48.;
    p.native_bounds[id]=NativeBounds::Glyphs(b);let w=&mut p.words[id];w.x0=b[0];w.x1=b[2];w.source_box=b;
    header141_seal(&mut p);header141_restore(&mut p,&r);assert_eq!(cuts(&p,&r),vec![1]);
    let old=serde_json::to_string(&p).unwrap();header141_restore(&mut p,&r);assert_eq!(serde_json::to_string(&p).unwrap(),old,"restoration must be idempotent");
}
fn header141_overhang(amount:f32,dividers:bool)->(PageTrace,PageRules) {
    let(mut p,mut r)=header141_fixture();let id=header141_first_id(&p,"任意の表題");
    let NativeBounds::Glyphs(mut b)=p.native_bounds[id] else {unreachable!()};b[0]=300.;b[2]=p.tables[0].x1+amount;
    p.native_bounds[id]=NativeBounds::Glyphs(b);let w=&mut p.words[id];w.x0=b[0];w.x1=b[2];w.source_box=b;
    if dividers {r.vertical.extend([crate::lines::VRule{x:40.,y0:0.,y1:p.tables[0].y1},crate::lines::VRule{x:b[2]+8.,y0:0.,y1:p.tables[0].y1}]);}
    header141_seal(&mut p);(p,r)
}
#[test]
fn header141_overhanging_caption_uses_original_column_dividers_without_fixed_tolerance() {
    for overhang in [0.5,16.,64.] {
        let(mut p,r)=header141_overhang(overhang,true);header141_restore(&mut p,&r);assert_eq!(cuts(&p,&r),vec![1]);
    }
}
#[test]
fn header141_overhang_without_a_source_column_boundary_is_not_guessed() {
    for overhang in [0.5,16.,64.] {
        let(mut p,r)=header141_overhang(overhang,false);let old=serde_json::to_string(&p).unwrap();header141_restore(&mut p,&r);assert_eq!(serde_json::to_string(&p).unwrap(),old);
    }
}
#[test]
fn header141_another_publication_inside_the_same_parent_column_is_ambiguous() {
    let(mut p,r)=header141_fixture();let id=header141_first_id(&p,"１ 短期経理");
    replace(&mut p,id,"別の機関の決算に関する公告");
    let st=NativeStyles::from([(7,9f64.to_bits())]);p.native_styles[id]=Some(st.clone());p.native_content_styles[id]=ContentStyles::Glyphs(st);
    header141_seal(&mut p);let old=serde_json::to_string(&p).unwrap();header141_restore(&mut p,&r);assert_eq!(serde_json::to_string(&p).unwrap(),old);
}
#[test]
fn header141_overlapping_same_baseline_fields_are_not_reordered() {
    let(mut p,r)=header141_fixture();let id=header141_first_id(&p,"任意の所在地");
    let NativeBounds::Glyphs(mut b)=p.native_bounds[id]else{unreachable!()};b[0]=120.;
    p.native_bounds[id]=NativeBounds::Glyphs(b);let w=&mut p.words[id];w.x0=b[0];w.source_box=b;
    let old=serde_json::to_string(&p).unwrap();header141_restore(&mut p,&r);assert_eq!(serde_json::to_string(&p).unwrap(),old);
}
#[test]
fn header141_font_bijection_and_word_id_permutation_keep_the_same_source_order() {
    let(mut p,r)=header141_fixture();let n=p.words.len();let remap=|id:usize|n-1-id;
    p.words.reverse();for w in &mut p.words {w.id=remap(w.id);}
    p.native_bounds.reverse();p.native_axes.reverse();p.native_styles.reverse();p.native_content_styles.reverse();p.native_horizontal_baselines.reverse();
    for s in p.native_styles.iter_mut().flatten(){*s=s.iter().map(|&(i,z)|(99-i,z)).collect();}
    for s in &mut p.native_content_styles {if let ContentStyles::Glyphs(s)=s{*s=s.iter().map(|&(i,z)|(99-i,z)).collect();}}
    for r in &mut p.regions {for l in &mut r.lines{for id in &mut l.words{*id=remap(*id);}if let Some(t)=&mut l.table{for c in &mut t.cells{for id in &mut c.words{*id=remap(*id);}}}}}
    header141_seal(&mut p);header141_restore(&mut p,&r);
    let title=header141_first_id(&p,"無名の団体の任意の期間の");let date=header141_first_id(&p,"2026年7月1日");
    assert!(header141_position(&p,title)<header141_position(&p,date));assert_eq!(cuts(&p,&r),vec![1]);
}

// A source-typed instrument need not use an announcement noun in its title.
fn native_instrument(title: &str, office: &str) -> PageTrace {
    let mut p = empty();
    add(&mut p, "以前の別件の本文。", [50., 10., 350., 18.], 1, 8.);
    add(&mut p, title, [50., 40., 220., 49.], 2, 9.);
    add(
        &mut p,
        "根拠となる制度に基づいて本件を定める。",
        [50., 60., 350., 68.],
        1,
        8.,
    );
    add(&mut p, "2026年7月1日", [60., 80., 160., 88.], 1, 8.);
    let role = add(&mut p, office, [230., 100., 290., 108.], 1, 8.);
    let name = add(&mut p, "任意", [310., 100., 350., 108.], 1, 8.);
    p.regions[0].lines.pop();
    p.regions[0].lines.pop();
    p.regions[0]
        .lines
        .push(Line::text(format!("{office} 任意"), vec![role, name]));
    add(&mut p, "１ 手続", [50., 120., 150., 128.], 1, 8.);
    p.native_horizontal_baselines = p
        .native_bounds
        .iter()
        .map(|b| match b {
            NativeBounds::Glyphs(b) => Some(f64::from(b[1] + b[3]) / 2.),
            _ => None,
        })
        .collect();
    refresh(&mut p);
    p
}
#[test]
fn native_instrument_roots_use_own_typography_date_and_office_for_arbitrary_titles() {
    for title in ["名のない措置", "制度の開始", "審査手続の実施"] {
        for office in ["任意機関長", "任意担当大臣"] {
            assert_eq!(
                cuts(&native_instrument(title, office), &Default::default()),
                vec![1],
                "{title}/{office}"
            );
        }
    }
}
#[test]
fn native_instrument_roots_do_not_borrow_a_following_headings_signature() {
    let mut p = native_instrument("最初の見出し", "任意機関長");
    let last = add(&mut p, "次の見出し", [50., 75., 220., 84.], 2, 9.);
    let l = p.regions[0].lines.pop().unwrap();
    p.regions[0].lines.insert(3, l);
    p.native_horizontal_baselines.push(Some(79.5));
    assert_eq!(p.words[last].text, "次の見出し");
    refresh(&mut p);
    assert!(cuts(&p, &Default::default()).is_empty());
}

#[test]
fn native_instrument_roots_require_their_complete_original_date_and_sentence() {
    for (id, text) in [
        (2, "名詞の断片"),
        (3, "日付不明"),
        (4, "大臣は本件を述べる。"),
        (5, "氏名について述べる。"),
    ] {
        let mut p = native_instrument("任意の手続", "任意機関長");
        p.words[id].raw = text.into();
        p.words[id].text = text.into();
        for line in &mut p.regions[0].lines {
            if line.words.contains(&id) {
                line.text = line
                    .words
                    .iter()
                    .map(|&i| p.words[i].text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
            }
        }
        refresh(&mut p);
        assert!(cuts(&p, &Default::default()).is_empty(), "{id}/{text}");
    }
}
#[test]
fn native_instrument_roots_refuse_missing_or_foreign_source_evidence() {
    for id in 1..=5 {
        for kind in 0..4 {
            let mut p = native_instrument("任意の手続", "任意機関長");
            match kind {
                0 => p.native_bounds[id] = NativeBounds::Unknown,
                1 => p.native_axes[id] = NativeAxis::Compatibility,
                2 => p.native_content_styles[id] = ContentStyles::Unknown,
                _ => p.words[id].status = WordStatus::Noise,
            }
            assert!(cuts(&p, &Default::default()).is_empty(), "{id}/{kind}");
        }
    }
}
#[test]
fn native_instrument_roots_require_a_distinct_original_heading_style() {
    let mut p = native_instrument("任意の手続", "任意機関長");
    p.native_styles[1] = p.native_styles[2].clone();
    p.native_content_styles[1] = p.native_content_styles[2].clone();
    assert!(cuts(&p, &Default::default()).is_empty());
}
#[test]
fn native_instrument_roots_cannot_take_unrelated_region_role_or_enclosed_material() {
    let p = native_instrument("任意の手続", "任意機関長");
    let mut r = PageRules::default();
    r.enclosures.push([40., 30., 360., 115.]);
    assert!(cuts(&p, &r).is_empty());
    let mut p = native_instrument("任意の手続", "任意機関長");
    p.regions[0].role = Role::Before;
    assert!(cuts(&p, &Default::default()).is_empty());
    let mut p = native_instrument("任意の手続", "任意機関長");
    let tail = p.regions[0].lines.split_off(3);
    p.regions.push(Region {
        role: Role::Body,
        unit: 1,
        part: 0,
        lines: tail,
    });
    assert!(cuts(&p, &Default::default()).is_empty());
}
#[test]
fn native_instrument_roots_cannot_use_a_quoted_or_mismatched_rendered_heading() {
    let mut p = native_instrument("任意の手続", "任意機関長");
    replace(&mut p, 0, "引用「");
    assert!(cuts(&p, &Default::default()).is_empty());
    let mut p = native_instrument("任意の手続", "任意機関長");
    p.regions[0].lines[1].text = "描画と異なる見出し".into();
    refresh(&mut p);
    assert!(cuts(&p, &Default::default()).is_empty());
}
#[test]
fn native_instrument_roots_keep_scale_and_translation_independent() {
    for (scale, dx, dy) in [(0.25, 300., 40.), (3., -120., 90.), (1., 0., 0.)] {
        let mut p = native_instrument("任意の手続", "任意機関長");
        let project = |b: Rect| {
            [
                b[0] * scale + dx,
                b[1] * scale + dy,
                b[2] * scale + dx,
                b[3] * scale + dy,
            ]
        };
        for w in &mut p.words {
            let b = project(w.source_box);
            w.source_box = b;
            [w.x0, w.y0, w.x1, w.y1] = b;
        }
        for b in &mut p.native_bounds {
            if let NativeBounds::Glyphs(r) = b {
                *r = project(*r);
            }
        }
        for b in &mut p.native_horizontal_baselines {
            if let Some(y) = b {
                *y = *y * f64::from(scale) + f64::from(dy);
            }
        }
        assert_eq!(cuts(&p, &Default::default()), vec![1]);
    }
}

#[test]
fn native_instrument_roots_allow_balanced_inline_definitions_in_their_own_prose() {
    let mut p = native_instrument("任意の手続", "任意機関長");
    replace(
        &mut p,
        2,
        "制度（以下「任意」という。）に基づいて本件を定める。",
    );
    assert_eq!(cuts(&p, &Default::default()), vec![1]);
}
#[test]
fn native_instrument_roots_allow_a_balanced_definition_across_source_lines() {
    let mut p = native_instrument("任意の手続", "任意機関長");
    replace(&mut p, 2, "制度（以下「任");
    add(
        &mut p,
        "意」という。）に基づいて本件を定める。",
        [50., 70., 350., 78.],
        1,
        8.,
    );
    let l = p.regions[0].lines.pop().unwrap();
    p.regions[0].lines.insert(3, l);
    p.native_horizontal_baselines.push(Some(74.));
    refresh(&mut p);
    assert_eq!(cuts(&p, &Default::default()), vec![1]);
}
#[test]
fn native_instrument_roots_cannot_release_an_unclosed_or_malformed_body_quote() {
    for text in [
        "引用「本件を定める。",
        "閉じられていない引用』本件を定める。",
    ] {
        let mut p = native_instrument("任意の手続", "任意機関長");
        replace(&mut p, 2, text);
        assert!(cuts(&p, &Default::default()).is_empty());
    }
}

#[test]
fn native_instrument_roots_do_not_turn_an_indented_prose_wrap_into_a_heading() {
    let mut p = native_instrument("任意の手続", "任意機関長");
    replace(&mut p, 2, "根拠となる制度の番号を示す");
    // A paragraph's first line can have its own indentation, while subsequent
    // lines share the source column margin and the same original body style.
    p.words[2].x0 = 60.;
    p.words[2].source_box[0] = 60.;
    if let NativeBounds::Glyphs(b) = &mut p.native_bounds[2] {
        b[0] = 60.;
    }
    add(
        &mut p,
        "（以下「任意」という。）によって本件を定める。",
        [50., 70., 350., 78.],
        1,
        8.,
    );
    let l = p.regions[0].lines.pop().unwrap();
    p.regions[0].lines.insert(3, l);
    p.native_horizontal_baselines.push(Some(74.));
    refresh(&mut p);
    assert_eq!(cuts(&p, &Default::default()), vec![1]);
}

#[test]
fn native_instrument_roots_include_a_larger_original_category_prefix() {
    let mut p = native_instrument("任意の手続", "任意機関長");
    add(&mut p, "任意分類", [80., 25., 260., 35.], 3, 10.);
    let l = p.regions[0].lines.pop().unwrap();
    p.regions[0].lines.insert(1, l);
    p.native_horizontal_baselines.push(Some(30.));
    refresh(&mut p);
    assert_eq!(cuts(&p, &Default::default()), vec![1]);
}
#[test]
fn native_instrument_roots_do_not_borrow_another_columns_larger_prefix() {
    let mut p = native_instrument("任意の手続", "任意機関長");
    add(&mut p, "別の分類", [400., 25., 550., 35.], 3, 10.);
    let l = p.regions[0].lines.pop().unwrap();
    p.regions[0].lines.insert(1, l);
    p.native_horizontal_baselines.push(Some(30.));
    refresh(&mut p);
    assert_eq!(cuts(&p, &Default::default()), vec![2]);
}

#[test]
fn native_instrument_roots_keep_source_font_ids_opaque() {
    let mut p = native_instrument("任意の手続", "任意機関長");
    for styles in p.native_styles.iter_mut().filter_map(Option::as_mut) {
        *styles = styles
            .iter()
            .map(|&(font, size)| (font ^ 0x105, size))
            .collect();
    }
    for content in &mut p.native_content_styles {
        if let ContentStyles::Glyphs(styles) = content {
            *styles = styles
                .iter()
                .map(|&(font, size)| (font ^ 0x105, size))
                .collect();
        }
    }
    assert_eq!(cuts(&p, &Default::default()), vec![1]);
}
#[test]
fn native_instrument_roots_follow_original_word_ownership_after_id_relabeling() {
    let mut p = native_instrument("任意の手続", "任意機関長");
    let n = p.words.len();
    p.words.reverse();
    for (i, w) in p.words.iter_mut().enumerate() {
        w.id = i;
    }
    p.native_styles.reverse();
    p.native_content_styles.reverse();
    p.native_bounds.reverse();
    p.native_axes.reverse();
    p.native_horizontal_baselines.reverse();
    for region in &mut p.regions {
        for line in &mut region.lines {
            for id in &mut line.words {
                *id = n - 1 - *id;
            }
        }
    }
    assert_eq!(cuts(&p, &Default::default()), vec![1]);
}
#[test]
fn native_instrument_roots_have_no_heading_line_count_budget() {
    for extra in [1, 7, 40] {
        let mut p = native_instrument("任意の手続", "任意機関長");
        let original = p.words.len();
        let dy = 10. * extra as f32;
        for id in 2..original {
            p.words[id].y0 += dy;
            p.words[id].y1 += dy;
            p.words[id].source_box[1] += dy;
            p.words[id].source_box[3] += dy;
            if let NativeBounds::Glyphs(b) = &mut p.native_bounds[id] {
                b[1] += dy;
                b[3] += dy;
            }
            if let Some(y) = &mut p.native_horizontal_baselines[id] {
                *y += f64::from(dy);
            }
        }
        for i in 0..extra {
            let y = 50. + 10. * i as f32;
            add(&mut p, "題名の任意の続き", [50., y, 220., y + 9.], 2, 9.);
            let line = p.regions[0].lines.pop().unwrap();
            p.regions[0].lines.insert(2 + i, line);
            p.native_horizontal_baselines.push(Some(f64::from(y) + 4.5));
        }
        refresh(&mut p);
        assert_eq!(cuts(&p, &Default::default()), vec![1]);
    }
}

#[test]
fn native_instrument_roots_do_not_trust_a_legacy_signature_line_without_one_source_baseline() {
    for baseline in [None, Some(f64::NAN), Some(105.)] {
        let mut p = native_instrument("任意の手続", "任意機関長");
        p.native_horizontal_baselines[5] = baseline;
        assert!(cuts(&p, &Default::default()).is_empty());
    }
}
#[test]
fn native_instrument_roots_require_the_date_to_have_a_finite_original_baseline() {
    for baseline in [None, Some(f64::INFINITY)] {
        let mut p = native_instrument("任意の手続", "任意機関長");
        p.native_horizontal_baselines[3] = baseline;
        assert!(cuts(&p, &Default::default()).is_empty());
    }
}

#[test]
fn native_instrument_roots_do_not_join_different_source_date_baselines() {
    let mut p = native_instrument("任意の手続", "任意機関長");
    p.words[3].raw = "2026年7月".into();
    p.words[3].text = "2026年7月".into();
    p.words[3].x1 = 140.;
    p.words[3].source_box[2] = 140.;
    if let NativeBounds::Glyphs(b) = &mut p.native_bounds[3] {
        b[2] = 140.;
    }
    let id = add(&mut p, "1日", [144., 80., 160., 88.], 1, 8.);
    p.regions[0].lines.pop();
    p.regions[0].lines[3].words.push(id);
    p.regions[0].lines[3].text = "2026年7月1日".into();
    p.native_horizontal_baselines.push(Some(85.));
    refresh(&mut p);
    assert!(cuts(&p, &Default::default()).is_empty());
}

#[test]
fn complete_signed_announcements_use_own_native_prose_date_and_office() {
    for title in ["任意の公告","別名公告","任意の公告（対象補足）"] {
        for office in ["任意機関長","任意担当大臣"] {
            assert_eq!(cuts(&native_instrument(title,office),&Default::default()),vec![1],"{title}/{office}");
        }
    }
}
#[test]
fn a_signed_certificate_does_not_repair_incomplete_announcement_grammar() {
    for title in ["公告対象の説明","公告に関する参考","任意公告（未閉鎖","任意公告（補足）について"] {
        assert!(cuts(&native_instrument(title,"任意機関長"),&Default::default()).is_empty(),"{title}");
    }
}
#[test]
fn signed_announcement_roots_require_their_actual_complete_body_and_date() {
    for case in 0..6 {
        let mut p=native_instrument("任意の公告","任意機関長");
        match case {
            0=>replace(&mut p,2,"名詞の断片"),
            1=>replace(&mut p,3,"別の記載"),
            2=>p.native_horizontal_baselines[3]=None,
            3=>p.native_content_styles[2]=ContentStyles::Unknown,
            4=>p.native_bounds[2]=NativeBounds::Unknown,
            _=>replace(&mut p,2,"引用「未完の本文。"),
        }
        assert!(cuts(&p,&Default::default()).is_empty(),"case{case}");
    }
}
#[test]
fn a_signed_announcement_cannot_borrow_an_independent_following_heading_date() {
    let mut p=native_instrument("任意の公告","任意機関長");
    add(&mut p,"次の独立した見出し",[50.,75.,220.,84.],2,9.);
    let l=p.regions[0].lines.pop().unwrap();p.regions[0].lines.insert(3,l);
    p.native_horizontal_baselines.push(Some(79.5));refresh(&mut p);
    assert!(cuts(&p,&Default::default()).is_empty());
}
#[test]
fn source_fonts_remain_opaque_for_a_signed_announcement() {
    let mut p=native_instrument("任意の公告","任意機関長");
    for s in p.native_styles.iter_mut().filter_map(Option::as_mut) {*s=s.iter().map(|&(f,n)|(f^0x511,n)).collect();}
    for c in &mut p.native_content_styles {if let ContentStyles::Glyphs(s)=c {*s=s.iter().map(|&(f,n)|(f^0x511,n)).collect();}}
    assert_eq!(cuts(&p,&Default::default()),vec![1]);
}
