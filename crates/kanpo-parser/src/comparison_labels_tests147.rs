use super::*;
fn label(cols: &mut Vec<Col>, parts: &[&str], x: f32, y: f32) {
    let mut y = y;
    for text in parts {
        let end = y + 8. * text.chars().count() as f32;
        cols.push(Col {
            id: cols.len(),
            x,
            x1: x + 8.,
            y,
            y1: end,
            text: (*text).into(),
        });
        y = end + 4.;
    }
}
fn fixture() -> Vec<Col> {
    let mut c = vec![];
    label(&mut c, &["変", "更", "後"], 600., 100.);
    label(&mut c, &["変", "更", "前"], 600., 500.);
    c.push(Col {
        id: 6,
        x: 500.,
        x1: 508.,
        y: 50.,
        y1: 750.,
        text: "欄の本文。".into(),
    });
    c
}
#[test]
fn labels147_fragmented_pair_has_complete_source_ids() {
    let c = fixture();
    let h = headers(&c);
    assert_eq!(h.len(), 1);
    assert_eq!(h[0].labels, ComparisonLabels::CHANGE);
    assert_eq!(h[0].words, vec![0, 1, 2, 3, 4, 5]);
    assert!(h[0].divider > 132. && h[0].divider < 500.);
}
#[test]
fn labels147_whole_and_mixed_word_fragmentations() {
    for (a, b) in [
        (vec!["変更後"], vec!["変更前"]),
        (vec!["変更", "後"], vec!["変", "更前"]),
    ] {
        let mut c = vec![];
        label(&mut c, &a, 300., 100.);
        label(&mut c, &b, 300., 500.);
        assert_eq!(headers(&c).len(), 1);
    }
}
#[test]
fn labels147_existing_amendment_family_uses_the_same_rule() {
    let mut c = fixture();
    for t in &mut c {
        t.text = t.text.replace('変', "改").replace('更', "正");
    }
    let h = headers(&c);
    assert_eq!(h.len(), 1);
    assert_eq!(h[0].labels, ComparisonLabels::AMENDMENT);
}
#[test]
fn labels147_pair_families_must_match() {
    let mut c = fixture();
    for t in &mut c[..3] {
        t.text = t.text.replace('変', "改").replace('更', "正");
    }
    assert!(headers(&c).is_empty());
}
#[test]
fn labels147_prose_containing_a_label_is_not_a_header() {
    let mut c = vec![];
    label(&mut c, &["変更後欄に掲げる規定"], 300., 100.);
    label(&mut c, &["変更前"], 300., 500.);
    assert!(headers(&c).is_empty());
}
#[test]
fn labels147_intervening_native_word_is_not_skipped() {
    let mut c = fixture();
    c.push(Col {
        id: 7,
        x: 600.,
        x1: 608.,
        y: 109.,
        y1: 111.,
        text: "の".into(),
    });
    assert!(headers(&c).is_empty());
}
#[test]
fn labels147_words_between_complete_labels_are_not_skipped() {
    let mut c = fixture();
    c.push(Col {
        id: 7,
        x: 600.,
        x1: 608.,
        y: 300.,
        y1: 320.,
        text: "説明文".into(),
    });
    assert!(headers(&c).is_empty());
}
#[test]
fn labels147_without_a_common_printed_column_declines() {
    let mut c = fixture();
    for t in &mut c[3..6] {
        t.x += 9.;
        t.x1 += 9.;
    }
    assert!(headers(&c).is_empty());
}
#[test]
fn labels147_intersecting_columns_need_no_fixed_origin_tolerance() {
    let mut c = fixture();
    for t in &mut c[3..6] {
        t.x += 7.5;
        t.x1 += 7.5;
    }
    assert_eq!(headers(&c).len(), 1);
}
#[test]
fn labels147_scale_and_translation_do_not_change_the_pair() {
    for (scale, dx, dy) in [(0.125, 71., 9.), (4., -300., 60.)] {
        let mut c = fixture();
        for w in &mut c {
            w.x = w.x * scale + dx;
            w.x1 = w.x1 * scale + dx;
            w.y = w.y * scale + dy;
            w.y1 = w.y1 * scale + dy;
        }
        assert_eq!(headers(&c).len(), 1);
    }
}
#[test]
fn labels147_duplicate_opposite_fragments_are_ambiguous() {
    let mut c = fixture();
    let mut duplicate = c[4].clone();
    duplicate.id = 7;
    c.push(duplicate);
    assert!(headers(&c).is_empty());
}
#[test]
fn labels147_invalid_rectangles_decline() {
    for invalid in [0., f32::NAN] {
        let mut c = fixture();
        c[1].x1 = c[1].x + invalid;
        assert!(headers(&c).is_empty());
    }
}
#[test]
fn labels147_overlapping_roles_are_not_a_band_divider() {
    let mut c = fixture();
    for t in &mut c[3..6] {
        t.y -= 390.;
        t.y1 -= 390.;
    }
    assert!(headers(&c).is_empty());
}
#[test]
fn labels147_reversed_role_order_is_not_silently_mislabeled() {
    let mut c = fixture();
    for t in &mut c[..3] {
        t.y += 400.;
        t.y1 += 400.;
    }
    for t in &mut c[3..6] {
        t.y -= 400.;
        t.y1 -= 400.;
    }
    assert!(headers(&c).is_empty());
}
#[test]
fn labels147_parallel_complete_header_columns_are_independent() {
    let mut c = fixture();
    label(&mut c, &["改正後"], 300., 100.);
    label(&mut c, &["改正前"], 300., 500.);
    let h = headers(&c);
    assert_eq!(h.len(), 2);
    assert!(h[0].x > h[1].x);
}
#[test]
fn labels147_word_id_bijection_does_not_authorize_literal_ids() {
    let mut c = fixture();
    for w in &mut c {
        w.id = w.id * 17 + 1000;
    }
    let h = headers(&c);
    assert_eq!(h.len(), 1);
    assert_eq!(h[0].words, vec![1000, 1017, 1034, 1051, 1068, 1085]);
}
#[test]
fn labels147_another_column_does_not_supply_a_suffix() {
    let mut c = fixture();
    c[2].x = 550.;
    c[2].x1 = 558.;
    assert!(headers(&c).is_empty());
}
#[test]
fn labels147_formal_surface_grammar_does_not_match_substrings() {
    assert_eq!(
        ComparisonLabels::from_after("変更後"),
        Some(ComparisonLabels::CHANGE)
    );
    for s in ["変更後欄", "変更後にする", "参考変更後", "（変更後）"] {
        assert_eq!(ComparisonLabels::from_after(s), None);
    }
}
#[test]
fn labels147_unit_renderer_retains_actual_source_surfaces() {
    use crate::trace::{Line, Unit};
    let u = Unit::Shinkyu {
        labels: Some(ComparisonLabels::CHANGE),
        preamble: vec![],
        after: vec![vec![Line::text("新しい本文。".into(), vec![1])]],
        before: vec![vec![Line::text("元の本文。".into(), vec![2])]],
        outside: vec![],
    };
    assert_eq!(u.text(), "変更後\n新しい本文。\n変更前\n元の本文。");
    let mut tables = vec![];
    let d = crate::Document::from_text(&u.doc_text(&mut tables, 0));
    assert_eq!(d.to_text(), u.text());
    let json = serde_json::to_value(d).unwrap();
    assert_eq!(json["blocks"][0]["labels"]["after"], "変更後");
}
#[test]
fn labels147_canonical_document_keeps_existing_json_shape() {
    let d = crate::Document::from_text("改正後\n新しい本文。\n改正前\n元の本文。");
    assert!(serde_json::to_value(d).unwrap()["blocks"][0]
        .get("labels")
        .is_none());
}
#[test]
fn labels147_different_families_do_not_merge_continuation_blocks() {
    let text="改正後\n最初の新しい本文。\n改正前\n最初の元の本文。\n変更後\n別の新しい本文。\n変更前\n別の元の本文。";
    let d = crate::Document::from_text(text);
    assert_eq!(d.blocks.len(), 2);
    assert_eq!(d.to_text(), text);
}
#[test]
fn labels147_mismatched_text_pair_stays_literal() {
    for text in [
        "変更後\n本文。\n改正前\n別の本文。",
        "改正後\n本文。\n変更前\n別の本文。",
    ] {
        let d = crate::Document::from_text(text);
        assert!(d
            .blocks
            .iter()
            .all(|b| matches!(b, crate::Block::Paragraph { .. })));
        assert!(d.to_text().contains(text.lines().next().unwrap()));
    }
}
#[test]
fn labels147_one_sided_cells_preserve_the_existing_marker_grammar_for_both_families() {
    for labels in [ComparisonLabels::AMENDMENT, ComparisonLabels::CHANGE] {
        let d = crate::Document::from_text(&format!("{}\n（新設）", labels.after));
        let [crate::Block::Shinkyu { rows, .. }] = d.blocks.as_slice() else {
            panic!("one-sided source marker cell was flattened")
        };
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].after[0].text, "（新設）");
        assert!(rows[0].before.is_empty());
    }
}
#[test]
fn labels147_an_after_label_cannot_borrow_a_before_from_beyond_an_end_marker() {
    for labels in [ComparisonLabels::AMENDMENT, ComparisonLabels::CHANGE] {
        let text = format!(
            "{}\n最初の本文。\n{}\n別の記事。\n{}\n別の本文。",
            labels.after,
            crate::trace::SHINKYU_END,
            labels.before
        );
        let d = crate::Document::from_text(&text);
        assert_eq!(d.blocks.len(), 2);
        let crate::Block::Shinkyu { rows, .. } = &d.blocks[0] else {
            panic!()
        };
        assert_eq!(rows[0].after[0].text, "最初の本文。");
        assert!(rows[0].before.is_empty());
        let crate::Block::Paragraph { runs } = &d.blocks[1] else {
            panic!()
        };
        assert_eq!(
            runs[0].text,
            format!("別の記事。\n{}\n別の本文。", labels.before)
        );
    }
}
#[test]
fn labels147_a_new_after_label_ends_the_previous_unpaired_cell() {
    for labels in [ComparisonLabels::AMENDMENT, ComparisonLabels::CHANGE] {
        let text = format!(
            "{}\n第一条\n{}\n第二条\n{}\n第二条旧",
            labels.after, labels.after, labels.before
        );
        let d = crate::Document::from_text(&text);
        let crate::Block::Shinkyu { rows, .. } = &d.blocks[0] else {
            panic!()
        };
        assert!(rows
            .iter()
            .all(|r| r.after.iter().all(|run| !run.text.contains(labels.after))));
    }
}
#[test]
fn labels147_split_joint_publication_heading_owns_its_complete_native_columns() {
    let mut c = fixture();
    label(&mut c, &["改正後"], 300., 100.);
    label(&mut c, &["改正前"], 300., 500.);
    c.push(Col {
        id: c.len(),
        x: 450.,
        x1: 458.,
        y: 100.,
        y1: 700.,
        text: "先の表に属する注記。".into(),
    });
    let note = c.last().unwrap().id;
    let ids = [c.len(), c.len() + 1, c.len() + 2, c.len() + 3];
    for (x, y, y1, t) in [
        (400., 50., 58., "〇"),
        (404., 58., 130., "試験省"),
        (396., 58., 130., "別試験省"),
        (400., 130., 170., "告示第一号"),
    ] {
        c.push(Col {
            id: c.len(),
            x,
            x1: x + 8.,
            y,
            y1,
            text: t.into(),
        });
    }
    let parts = crate::shinkyu::split_side_by_side_tables(c);
    assert_eq!(parts.len(), 2);
    assert!(parts[0].iter().any(|c| c.id == note));
    assert!(
        ids.iter().all(|id| parts[1].iter().any(|c| c.id == *id)),
        "all joint heading fragments belong to the next table's preamble"
    );
}

#[test]
fn labels147_publication_heading_fence_leaves_prior_spanning_note_in_prior_table() {
    let mut c = fixture();
    label(&mut c, &["改正後"], 300., 100.);
    label(&mut c, &["改正前"], 300., 500.);
    c.push(Col {
        id: c.len(),
        x: 450.,
        x1: 458.,
        y: 100.,
        y1: 700.,
        text: "先の表に属する注記。".into(),
    });
    let note = c.last().unwrap().id;
    c.push(Col {
        id: c.len(),
        x: 400.,
        x1: 408.,
        y: 50.,
        y1: 150.,
        text: "〇試験省告示第一号".into(),
    });
    let heading = c.last().unwrap().id;
    let parts = crate::shinkyu::split_side_by_side_tables(c);
    assert_eq!(parts.len(), 2);
    assert!(parts[0].iter().any(|c| c.id == note));
    assert!(!parts[0].iter().any(|c| c.id == heading));
    assert!(parts[1].iter().any(|c| c.id == heading));
}

#[test]
fn labels147_a_heading_prefix_with_body_is_not_a_complete_publication_boundary() {
    let mut base = fixture();
    label(&mut base, &["改正後"], 300., 100.);
    label(&mut base, &["改正前"], 300., 500.);
    base.push(Col {
        id: base.len(),
        x: 450.,
        x1: 458.,
        y: 100.,
        y1: 700.,
        text: "先の表に属する注記。".into(),
    });
    base.push(Col {
        id: base.len(),
        x: 400.,
        x1: 408.,
        y: 50.,
        y1: 200.,
        text: "通常本文。".into(),
    });
    let owners = |cols| {
        crate::shinkyu::split_side_by_side_tables(cols)
            .iter()
            .map(|part| part.iter().map(|c| c.id).collect::<Vec<_>>())
            .collect::<Vec<_>>()
    };
    let expected = owners(base.clone());
    for text in [
        "〇試験省告示第一号　隣の本文。",
        "〇試験省告示第一号　別の告示第二号",
    ] {
        let mut c = base.clone();
        c.last_mut().unwrap().text = text.into();
        assert_eq!(
            owners(c),
            expected,
            "a recognized prefix cannot authorize unrelated source columns"
        );
    }
}

#[test]
fn labels147_before_first_prose_records_cannot_borrow_opposite_roles_from_the_next_record() {
    for labels in [ComparisonLabels::AMENDMENT, ComparisonLabels::CHANGE] {
        let text=format!("第一の手続。\n{}\n元の期限。\n{}\n新しい期限。\n第二の手続。\n{}\n別の元の期限。\n{}\n別の新しい期限。",labels.before,labels.after,labels.before,labels.after);
        let d = crate::Document::from_text(&text);
        assert!(
            d.blocks
                .iter()
                .all(|b| matches!(b, crate::Block::Paragraph { .. })),
            "a reversed ordinary notice sequence is not an After-first marker stream"
        );
        for marker in [labels.after, labels.before] {
            assert_eq!(d.to_text().matches(marker).count(), 2);
        }
    }
}
#[test]
fn labels147_an_explicit_end_resets_a_leading_unpaired_before_scope() {
    for labels in [ComparisonLabels::AMENDMENT, ComparisonLabels::CHANGE] {
        let text = format!(
            "{}\n先の断片。\n{}\n{}\n次の新しい本文。\n{}\n次の元の本文。",
            labels.before,
            crate::trace::SHINKYU_END,
            labels.after,
            labels.before
        );
        let d = crate::Document::from_text(&text);
        assert_eq!(d.blocks.len(), 2);
        assert!(matches!(d.blocks[0], crate::Block::Paragraph { .. }));
        let crate::Block::Shinkyu { rows, .. } = &d.blocks[1] else {
            panic!()
        };
        assert_eq!(rows[0].after[0].text, "次の新しい本文。");
        assert_eq!(rows[0].before[0].text, "次の元の本文。");
    }
}

#[test]
fn labels147_format_uses_the_same_after_first_scope_as_the_document() {
    for labels in [ComparisonLabels::AMENDMENT, ComparisonLabels::CHANGE] {
        let reversed = format!(
            "{}\n元の本文。\n{}\n新しい本文。",
            labels.before, labels.after
        );
        assert_eq!(crate::format::detect_format(&reversed), "unknown");
        let reset = format!(
            "{}\n{}\n{}\n新しい本文。\n{}\n元の本文。",
            labels.before,
            crate::trace::SHINKYU_END,
            labels.after,
            labels.before
        );
        assert_eq!(crate::format::detect_format(&reset), "shinkyu");
    }
}
