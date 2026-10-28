use super::*;
use crate::native_word_axes::NativeAxis;
use crate::trace::{ContentStyles, NativeBounds, NativeStyles, Table, TableCell, WordStatus};
#[derive(Clone)]
struct Fixture {
    units: Vec<Vec<Line>>,
    words: Vec<Word>,
    styles: Vec<Option<NativeStyles>>,
    content: Vec<ContentStyles>,
    bounds: Vec<NativeBounds>,
    axes: Vec<NativeAxis>,
    rulings: Vec<crate::native_word_rulings::NativeRuling>,
}
impl Fixture {
    fn put(&mut self, text: &str, b: Box4, font: usize, size: f64, axis: NativeAxis) -> Line {
        let id = self.words.len();
        self.words.push(Word {
            id,
            x0: b[0],
            y0: b[1],
            x1: b[2],
            y1: b[3],
            source_box: b,
            raw: text.into(),
            text: text.into(),
            status: WordStatus::Used,
        });
        let s = NativeStyles::from([(font, size.to_bits())]);
        self.styles.push(Some(s.clone()));
        self.content.push(ContentStyles::Glyphs(s));
        self.bounds.push(NativeBounds::Glyphs(b));
        self.axes.push(axis);
        self.rulings
            .push(crate::native_word_rulings::NativeRuling::Unknown);
        Line::text(text.into(), vec![id])
    }
    fn run(&self) -> Vec<Line> {
        let mut us = self
            .units
            .iter()
            .cloned()
            .map(Unit::Plain)
            .collect::<Vec<_>>();
        order_with_source(
            &mut us,
            &self.words,
            &[[0., 0., 500., 250.]],
            800.,
            1000.,
            &self.styles,
            &self.content,
            &self.bounds,
            &self.axes,
            &self.rulings,
            &[],
        );
        let Unit::Plain(ls) = us.remove(0) else {
            panic!()
        };
        ls
    }
    fn rename(&mut self, from: &str, to: &str) {
        for w in &mut self.words {
            w.raw = w.raw.replace(from, to);
            w.text = w.text.replace(from, to);
        }
        for l in self.units.iter_mut().flatten() {
            l.text = l.text.replace(from, to);
        }
    }
}
fn fixture() -> Fixture {
    let mut f = Fixture {
        units: vec![],
        words: vec![],
        styles: vec![],
        content: vec![],
        bounds: vec![],
        axes: vec![],
        rulings: vec![],
    };
    let mut lines = vec![];
    for (i, t, font, size) in [
        (0, "第123期決算公告", 3, 9.),
        (1, "令和八年七月二十二日", 4, 8.),
        (2, "任意の住所", 4, 8.),
        (3, "株式会社みどり", 3, 9.),
        (4, "代表取締役山田太郎", 4, 8.),
        (5, "貸借対照表の要旨", 3, 8.),
    ] {
        lines.push(f.put(
            t,
            [20., 10. + i as f32 * 15., 150., 19. + i as f32 * 15.],
            font,
            size,
            NativeAxis::Right,
        ));
    }
    let value = f.put("123", [30., 110., 50., 118.], 4, 8., NativeAxis::Right);
    let table = Table {
        x0: 20.,
        y0: 100.,
        x1: 150.,
        y1: 145.,
        rows: 1,
        cols: 1,
        cells: vec![TableCell {
            row: 0,
            col: 0,
            rowspan: 1,
            colspan: 1,
            x0: 20.,
            y0: 100.,
            x1: 150.,
            y1: 145.,
            text: "123".into(),
            words: value.words,
        }],
    };
    lines.push(Line {
        text: table.render(),
        words: vec![],
        table: Some(table),
    });
    lines.push(f.put(
        "独立した欄外",
        [600., 10., 700., 18.],
        4,
        8.,
        NativeAxis::Right,
    ));
    for (i, t, font) in [
        (0, "新設分割公告", 0),
        (1, "最終貸借対照表の要旨は左記のとおりです", 1),
        (2, "令和八年七月二十二日", 1),
        (3, "株式会社みどり", 1),
        (4, "代表取締役山田太郎", 1),
    ] {
        let x = 360. - i as f32 * 14.;
        lines.push(f.put(
            t,
            [x, 10., x + 8., if i == 1 { 200. } else { 106. }],
            font,
            8.,
            NativeAxis::Down,
        ));
    }
    f.units.push(lines);
    f
}
fn order(ls: &[Line]) -> Vec<Vec<usize>> {
    ls.iter().map(|l| l.words.clone()).collect()
}
#[test]
fn a_proven_native_parent_keeps_the_referenced_notice_before_its_complete_statement() {
    let f = fixture();
    let ls = f.run();
    assert_eq!(ls[0].text, "新設分割公告");
    assert_eq!(ls.last().unwrap().text, "独立した欄外");
    assert_eq!(ls.iter().filter(|l| l.table.is_some()).count(), 1);
}
#[test]
fn native_issuer_values_and_formal_titles_have_no_character_cap() {
    let f = fixture();
    let want = order(&f.run());
    for (from, to) in [
        (
            "株式会社みどり",
            format!("株式会社{}", "任意の名称".repeat(80)),
        ),
        (
            "新設分割公告",
            format!("{}新設分割公告", "複数の事業の再編に関する".repeat(80)),
        ),
    ] {
        let mut x = f.clone();
        x.rename(from, &to);
        assert_eq!(order(&x.run()), want);
    }
}
#[test]
fn merging_complete_source_lines_does_not_remove_declaration_roles() {
    let f = fixture();
    let want = order(&f.run()).into_iter().flatten().collect::<Vec<_>>();
    let mut x = f.clone();
    let signature = x.units[0].remove(12);
    x.units[0][11].text.push_str(&signature.text);
    x.units[0][11].words.extend(signature.words);
    let out = order(&x.run()).into_iter().flatten().collect::<Vec<_>>();
    assert_eq!(out, want);
    let company_and_signature = x.units[0].remove(11);
    x.units[0][10].text.push_str(&company_and_signature.text);
    x.units[0][10].words.extend(company_and_signature.words);
    assert_eq!(
        order(&x.run()).into_iter().flatten().collect::<Vec<_>>(),
        want
    );
}
#[test]
fn nominal_publisher_typography_does_not_authorize_a_prose_sentence() {
    let mut f = fixture();
    f.rename(
        "株式会社みどり",
        "資料に記載された株式会社みどりについて説明しています。",
    );
    let before = f.units[0].clone();
    assert_eq!(f.run(), before);
}
#[test]
fn a_body_mention_in_normal_type_is_not_a_publisher_byline() {
    let mut f = fixture();
    let id = f.units[0][3].words[0];
    let s = NativeStyles::from([(4, 8f64.to_bits())]);
    f.styles[id] = Some(s.clone());
    f.content[id] = ContentStyles::Glyphs(s);
    let before = f.units[0].clone();
    assert_eq!(f.run(), before);
}
#[test]
fn unknown_or_incomplete_native_source_does_not_fall_back_to_plausible_boxes() {
    for axis in [NativeAxis::Unknown, NativeAxis::Compatibility] {
        let mut f = fixture();
        f.axes[3] = axis;
        let before = f.units[0].clone();
        assert_eq!(f.run(), before);
    }
    let mut f = fixture();
    f.bounds[3] = NativeBounds::Unknown;
    let before = f.units[0].clone();
    assert_eq!(f.run(), before);
}
#[test]
fn native_reading_axes_survive_independent_coordinate_scaling() {
    let f = fixture();
    let want = order(&f.run());
    for (sx, sy) in [(0.125, 64.), (64., 0.125)] {
        let mut x = f.clone();
        for (w, b) in x.words.iter_mut().zip(&mut x.bounds) {
            w.x0 *= sx;
            w.x1 *= sx;
            w.y0 *= sy;
            w.y1 *= sy;
            w.source_box = [
                w.source_box[0] * sx,
                w.source_box[1] * sy,
                w.source_box[2] * sx,
                w.source_box[3] * sy,
            ];
            if let NativeBounds::Glyphs(r) = b {
                *r = [r[0] * sx, r[1] * sy, r[2] * sx, r[3] * sy];
            }
        }
        for l in x.units.iter_mut().flatten() {
            if let Some(t) = &mut l.table {
                t.x0 *= sx;
                t.x1 *= sx;
                t.y0 *= sy;
                t.y1 *= sy;
                for c in &mut t.cells {
                    c.x0 *= sx;
                    c.x1 *= sx;
                    c.y0 *= sy;
                    c.y1 *= sy;
                }
            }
        }
        let mut us = x.units.iter().cloned().map(Unit::Plain).collect::<Vec<_>>();
        order_with_source(
            &mut us,
            &x.words,
            &[[0., 0., 500. * sx, 250. * sy]],
            800. * sx,
            1000. * sy,
            &x.styles,
            &x.content,
            &x.bounds,
            &x.axes,
            &x.rulings,
            &[],
        );
        let Unit::Plain(ls) = &us[0] else { panic!() };
        assert_eq!(order(ls), want);
    }
}

#[test]
fn reference_type_does_not_depend_on_one_description_of_the_statement() {
    let mut f = fixture();
    f.rename(
        "最終貸借対照表の要旨は左記のとおりです",
        "対象の貸借対照表については左記をご参照ください",
    );
    assert_eq!(f.run()[0].text, "新設分割公告");
}
#[test]
fn existing_whole_table_is_opaque_and_no_new_blank_cell_is_certified() {
    let mut f = fixture();
    let id = f.words.len();
    f.words.push(Word {
        id,
        x0: 30.,
        y0: 120.,
        x1: 40.,
        y1: 128.,
        source_box: [30., 120., 40., 128.],
        raw: "未解決の元符号".into(),
        text: String::new(),
        status: WordStatus::Empty,
    });
    f.styles.push(None);
    f.content.push(ContentStyles::Unknown);
    f.bounds.push(NativeBounds::Glyphs([30., 120., 40., 128.]));
    f.axes.push(NativeAxis::Unknown);
    let old = f.units[0].iter().find_map(|l| l.table.clone()).unwrap();
    let output = f.run();
    assert_eq!(output[0].text, "新設分割公告");
    assert_eq!(output.iter().find_map(|l| l.table.clone()).unwrap(), old);
    assert_eq!(f.words[id].text, "");
    assert_eq!(f.words[id].status, WordStatus::Empty);
}
#[test]
fn unassigned_source_outside_or_crossing_the_opaque_table_is_not_header_evidence() {
    for b in [[200., 120., 210., 128.], [145., 120., 155., 128.]] {
        let mut f = fixture();
        let id = f.words.len();
        f.words.push(Word {
            id,
            x0: b[0],
            y0: b[1],
            x1: b[2],
            y1: b[3],
            source_box: b,
            raw: "未解決".into(),
            text: String::new(),
            status: WordStatus::Empty,
        });
        f.styles.push(None);
        f.content.push(ContentStyles::Unknown);
        f.bounds.push(NativeBounds::Glyphs(b));
        f.axes.push(NativeAxis::Unknown);
        assert_eq!(f.run(), f.units[0]);
    }
}

#[test]
fn adjacent_header_baselines_can_have_overlapping_nominal_glyph_boxes() {
    let mut f = fixture();
    let id = f.units[0][3].words[0];
    f.words[id].y1 = 72.;
    f.words[id].source_box[3] = 72.;
    f.bounds[id] = NativeBounds::Glyphs([20., 55., 150., 72.]);
    assert_eq!(f.run()[0].text, "新設分割公告");
}
#[test]
fn date_word_fragments_with_a_common_row_need_not_have_nested_glyph_boxes() {
    let mut f = fixture();
    let id = f.units[0][1].words[0];
    f.words[id].raw = "令和八年".into();
    f.words[id].text = "令和八年".into();
    f.words[id].x1 = 80.;
    f.words[id].source_box[2] = 80.;
    f.bounds[id] = NativeBounds::Glyphs([20., 25., 80., 34.]);
    let next = f.put(
        "七月二十二日",
        [80., 25.5, 150., 34.5],
        4,
        8.,
        NativeAxis::Right,
    );
    f.units[0][1].words.extend(next.words);
    assert_eq!(f.run()[0].text, "新設分割公告");
}

#[test]
fn balanced_quotes_in_the_publication_body_are_not_external_quotation_scope() {
    let mut f = fixture();
    f.rename(
        "最終貸借対照表の要旨は左記のとおりです",
        "当社の「対象事業」に係る貸借対照表は左記のとおりです",
    );
    assert_eq!(f.run()[0].text, "新設分割公告");
}
#[test]
fn external_quotation_does_not_authorize_a_framed_example() {
    let mut f = fixture();
    let q = f.put("次の例「", [600., 30., 700., 38.], 4, 8., NativeAxis::Right);
    f.units[0].insert(0, q);
    assert_eq!(f.run(), f.units[0]);
}
#[test]
fn an_external_quote_opened_between_frame_members_still_owns_the_example() {
    let mut f = fixture();
    let q = f.put("次の例「", [600., 30., 700., 38.], 4, 8., NativeAxis::Right);
    f.units[0].insert(9, q);
    assert_eq!(f.run(), f.units[0]);
}
#[test]
fn an_unclosed_internal_quotation_is_not_a_publication_certificate() {
    let mut f = fixture();
    f.rename(
        "最終貸借対照表の要旨は左記のとおりです",
        "「貸借対照表について",
    );
    assert_eq!(f.run(), f.units[0]);
}
#[test]
fn restoration_is_idempotent_and_preserves_every_whole_line_and_table() {
    let f = fixture();
    let output = f.run();
    let mut x = f.clone();
    x.units[0] = output.clone();
    assert_eq!(x.run(), output);
    let serialized = |ls: &[Line]| {
        let mut s = ls
            .iter()
            .map(|l| serde_json::to_string(l).unwrap())
            .collect::<Vec<_>>();
        s.sort();
        s
    };
    assert_eq!(serialized(&output), serialized(&f.units[0]));
}
#[test]
fn duplicate_frame_borders_have_no_priority_or_repeated_effect() {
    let f = fixture();
    let wanted = f.run();
    for frames in [
        vec![[0., 0., 500., 250.], [1., 1., 499., 249.]],
        vec![[1., 1., 499., 249.], [0., 0., 500., 250.]],
    ] {
        let mut us = f.units.iter().cloned().map(Unit::Plain).collect::<Vec<_>>();
        order_with_source(
            &mut us,
            &f.words,
            &frames,
            800.,
            1000.,
            &f.styles,
            &f.content,
            &f.bounds,
            &f.axes,
            &f.rulings,
            &[],
        );
        let Unit::Plain(ls) = &us[0] else { panic!() };
        assert_eq!(*ls, wanted);
    }
}

#[test]
fn displaced_signature_is_read_from_native_columns_rather_than_input_order() {
    let mut f = fixture();
    let sig = f.units[0].remove(12);
    f.units[0].insert(0, sig);
    let output = f.run();
    assert_eq!(output[0].text, "新設分割公告");
    let mut x = f.clone();
    x.units[0] = output.clone();
    assert_eq!(x.run(), output);
}
#[test]
fn a_displaced_column_prefix_has_no_character_or_line_count_requirement() {
    for prefix in ["任意", "別の説明を含む任意の部分"] {
        let mut f = fixture();
        let id = f.units[0][9].words[0];
        f.words[id].y0 = 40.;
        f.words[id].source_box[1] = 40.;
        f.bounds[id] = NativeBounds::Glyphs([346., 40., 354., 200.]);
        let prefix = f.put(prefix, [346., 10., 354., 40.], 1, 8., NativeAxis::Down);
        let new = prefix.words[0];
        f.units[0].insert(7, prefix);
        let output = f.run();
        assert_eq!(output[0].text, "新設分割公告");
        let a = output.iter().position(|l| l.words.contains(&new)).unwrap();
        let b = output.iter().position(|l| l.words.contains(&id)).unwrap();
        assert_eq!(a + 1, b);
    }
}
#[test]
fn an_unrelated_rendered_line_does_not_supply_or_invalidate_frame_evidence() {
    let mut f = fixture();
    f.units[0][7].text = "（独立した欄外）".into();
    let output = f.run();
    assert_eq!(output[0].text, "新設分割公告");
    assert_eq!(output.last(), Some(&f.units[0][7]));
    let mut bad = fixture();
    bad.units[0][3].text = "（旧商号株式会社みどり）".into();
    assert_eq!(bad.run(), bad.units[0]);
}

fn wrapped_publisher_suffix(prefix: &str) -> Fixture {
    let mut f = fixture();
    let id = f.units[0][11].words[0];
    f.words[id].raw = "株式会社み".into();
    f.words[id].text = "株式会社み".into();
    f.units[0][11].text = "株式会社み".into();
    let sig = f.units[0][12].words[0];
    f.words[sig].y0 = 40.;
    f.words[sig].y1 = 136.;
    f.words[sig].source_box = [304., 40., 312., 136.];
    f.bounds[sig] = NativeBounds::Glyphs([304., 40., 312., 136.]);
    let suffix = f.put(prefix, [304., 10., 312., 30.], 1, 8., NativeAxis::Down);
    f.units[0].insert(12, suffix);
    f.rename("株式会社みどり", &format!("株式会社み{prefix}"));
    f
}
#[test]
fn a_wrapped_publisher_can_share_its_final_native_column_with_the_signature() {
    for tail in ["どり", "どりの多くの部門".repeat(80).as_str()] {
        let f = wrapped_publisher_suffix(tail);
        assert_eq!(f.run()[0].text, "新設分割公告");
    }
}
#[test]
fn a_prose_prefix_does_not_turn_a_last_column_into_a_signature_field() {
    let f = wrapped_publisher_suffix("別の会社について、");
    assert_eq!(f.run(), f.units[0]);
}

#[test]
fn a_body_mention_of_another_publisher_is_not_its_dated_attribution() {
    for reference in [
        "株式会社みどりの貸借対照表については左記です",
        "「株式会社みどり」の貸借対照表については左記です",
    ] {
        let mut f = fixture();
        let id = f.units[0][11].words[0];
        f.words[id].text = "株式会社あおぞら".into();
        f.words[id].raw = f.words[id].text.clone();
        f.units[0][11].text = f.words[id].text.clone();
        f.rename("最終貸借対照表の要旨は左記のとおりです", reference);
        assert_eq!(
            f.run(),
            f.units[0],
            "a reference in prose cannot replace the publication's own dated attribution"
        );
    }
}

fn foreign_issuer(reference: &str) -> Fixture {
    let mut f = fixture();
    let id = f.units[0][11].words[0];
    f.words[id].text = "株式会社あおぞら".into();
    f.words[id].raw = f.words[id].text.clone();
    f.units[0][11].text = f.words[id].text.clone();
    f.rename("最終貸借対照表の要旨は左記のとおりです", reference);
    f
}
#[test]
fn an_explicit_counterparty_can_own_an_annex_without_signing_the_notice() {
    for (a, b) in [
        ("甲".to_owned(), "乙".to_owned()),
        ("事業を承継する側".repeat(30), "事業を移転する側".repeat(40)),
    ] {
        let f = foreign_issuer(&format!(
            "当社（{a}）は株式会社みどり（{b}）との手続を行います。貸借対照表は左記のとおりです"
        ));
        assert_eq!(f.run()[0].text, "新設分割公告");
    }
}
#[test]
fn quotations_separate_sentences_and_conflicting_actor_labels_do_not_bind_a_counterparty() {
    for text in [
        "「当社（甲）は株式会社みどり（乙）と手続します」を引用します。貸借対照表は左記です",
        "当社（甲）は手続を行います。別の例は株式会社みどり（乙）です。貸借対照表は左記です",
        "当社（甲）は株式会社みどり（甲）と手続します。貸借対照表は左記です",
    ] {
        let f = foreign_issuer(text);
        assert_eq!(f.run(), f.units[0]);
    }
}

#[test]
fn party_role_records_can_carry_arbitrary_balanced_annotations() {
    for note in ["任意の属性".to_owned(), "補足（別の任意の注記）".repeat(60)] {
        let f=foreign_issuer(&format!("当社（取得する側）は株式会社みどり（移転する側、{note}）との手続を行います。貸借対照表は左記です"));
        assert_eq!(f.run()[0].text, "新設分割公告");
    }
}
#[test]
fn missing_unbalanced_or_conflicting_role_record_heads_are_not_participants() {
    for text in [
        "当社（甲）は株式会社みどり（、任意の属性）と手続します。貸借対照表は左記です",
        "当社（甲）は株式会社みどり（乙、）と手続します。貸借対照表は左記です",
        "当社（甲）は株式会社みどり（乙、注記（途中）と手続します。貸借対照表は左記です",
        "当社（甲、注記）は株式会社みどり（甲、別の注記）と手続します。貸借対照表は左記です",
    ] {
        let f = foreign_issuer(text);
        assert_eq!(f.run(), f.units[0]);
    }
}

fn unused_outline(f: &mut Fixture, ink: [f64; 4]) -> usize {
    let l = f.put(
        &"\u{e101}".repeat(7),
        [200., 130., 230., 138.],
        42,
        8.,
        NativeAxis::Right,
    );
    let id = l.words[0];
    f.words[id].text.clear();
    f.words[id].status = WordStatus::Empty;
    f.content[id] = ContentStyles::Unknown;
    f.rulings[id] = crate::native_word_rulings::NativeRuling::Horizontal(ink);
    id
}
#[test]
fn certified_unused_outline_chains_do_not_block_an_owned_publication() {
    let mut f = fixture();
    unused_outline(&mut f, [199.984375, 134., 230.015625, 134.5]);
    assert_eq!(f.run()[0].text, "新設分割公告");
}
#[test]
fn ink_crossing_the_parent_visible_source_or_incomplete_provenance_remain_blockers() {
    for mode in 0..6 {
        let mut f = fixture();
        let id = unused_outline(&mut f, [199.984375, 134., 230.015625, 134.5]);
        match mode {
            0 => {
                f.rulings[id] =
                    crate::native_word_rulings::NativeRuling::Horizontal([-1., 134., 230., 134.5])
            }
            1 => {
                f.words[id].status = WordStatus::Used;
                f.words[id].text = "資料".into();
            }
            2 => f.bounds[id] = NativeBounds::Compatibility,
            3 => f.styles[id] = None,
            4 => f.axes[id] = NativeAxis::Down,
            _ => {
                f.rulings[id] = crate::native_word_rulings::NativeRuling::Horizontal([
                    f64::NAN,
                    134.,
                    230.,
                    134.5,
                ])
            }
        }
        assert_eq!(f.run(), f.units[0], "{mode}");
    }
}
#[test]
fn one_certified_ruling_never_authorizes_other_unresolved_source() {
    let mut f = fixture();
    unused_outline(&mut f, [199.984375, 134., 230.015625, 134.5]);
    let l = f.put(
        "別の未解決字形",
        [250., 130., 260., 138.],
        43,
        8.,
        NativeAxis::Right,
    );
    let id = l.words[0];
    f.words[id].text.clear();
    f.words[id].status = WordStatus::Empty;
    assert_eq!(f.run(), f.units[0]);
}

#[test]
fn unlabelled_named_transfer_operands_can_supply_the_related_statement() {
    let f=foreign_issuer("当社は新設分割により株式会社みどり（任意の所在地と注記）の事業に関して有する権利義務を承継します。貸借対照表は左記です");
    assert_eq!(f.run()[0].text, "新設分割公告");
}
#[test]
fn an_incidental_name_inside_another_actors_property_is_not_its_own_attribution() {
    let f=foreign_issuer("当社は新設分割により株式会社あおぞら（株式会社みどりへの言及）の株式会社みどりに関する資料の権利義務を承継します。貸借対照表は左記です");
    assert_eq!(f.run(), f.units[0]);
}

#[test]
fn an_issuer_name_prefix_is_not_a_complete_signed_publisher_field() {
    let mut f = foreign_issuer("貸借対照表は左記です");
    let id = f.units[0][11].words[0];
    let text = "株式会社みどり商事";
    f.words[id].raw = text.into();
    f.words[id].text = text.into();
    f.units[0][11].text = text.into();
    assert_eq!(f.run(), f.units[0]);
}
#[test]
fn a_publisher_mentioned_inside_an_attribution_address_does_not_sign_it() {
    let mut f = foreign_issuer("貸借対照表は左記です");
    let address = f.put(
        "東京都株式会社みどりビル",
        [325., 110., 333., 210.],
        1,
        8.,
        NativeAxis::Down,
    );
    f.units[0].insert(11, address);
    assert_eq!(f.run(), f.units[0]);
}
#[test]
fn publisher_identity_does_not_depend_on_internal_native_word_partition() {
    let mut f = fixture();
    let id = f.units[0][11].words[0];
    f.words[id].raw = "株式会社".into();
    f.words[id].text = "株式会社".into();
    f.words[id].y1 = 45.;
    f.words[id].source_box[3] = 45.;
    f.bounds[id] = NativeBounds::Glyphs([318., 10., 326., 45.]);
    f.units[0][11].text = "株式会社".into();
    let suffix = f.put("みどり", [318., 45., 326., 106.], 1, 8., NativeAxis::Down);
    f.units[0][11].text.push_str(&suffix.text);
    f.units[0][11].words.extend(suffix.words);
    assert_eq!(f.run()[0].text, "新設分割公告");
}

#[test]
fn the_rest_of_a_split_longer_issuer_is_not_discarded_before_its_signature() {
    let mut f = fixture();
    let suffix = f.put("商事", [318., 106., 326., 130.], 1, 8., NativeAxis::Down);
    f.units[0].insert(12, suffix);
    assert_eq!(f.run(), f.units[0]);
}

fn stacked_publications(
    same_publisher: bool,
) -> (crate::trace::PageTrace, crate::lines::PageRules) {
    use crate::trace::{PageTrace, Region, Role};
    let mut a = fixture();
    let mut b = fixture();
    if !same_publisher {
        b.rename("株式会社みどり", "株式会社あお");
    }
    let mut first = a.run();
    first.pop();
    let mut second = b.run();
    second.pop();
    let shift = a.words.len();
    for w in &mut b.words {
        w.id += shift;
        w.y0 += 300.;
        w.y1 += 300.;
        w.source_box[1] += 300.;
        w.source_box[3] += 300.;
    }
    for bound in &mut b.bounds {
        if let NativeBounds::Glyphs(r) = bound {
            r[1] += 300.;
            r[3] += 300.;
        } else {
            panic!();
        }
    }
    for l in &mut second {
        for id in &mut l.words {
            *id += shift;
        }
        if let Some(t) = &mut l.table {
            t.y0 += 300.;
            t.y1 += 300.;
            for c in &mut t.cells {
                c.y0 += 300.;
                c.y1 += 300.;
                for id in &mut c.words {
                    *id += shift;
                }
            }
        }
    }
    for w in a.words.iter_mut().chain(&mut b.words) {
        if w.x0 >= 600. {
            w.status = WordStatus::Margin;
        }
    }
    first.extend(second);
    a.words.extend(b.words);
    a.styles.extend(b.styles);
    a.content.extend(b.content);
    a.bounds.extend(b.bounds);
    a.axes.extend(b.axes);
    a.rulings.extend(b.rulings);
    let tables = first
        .iter()
        .filter_map(|l| l.table.clone())
        .collect::<Vec<_>>();
    let text = first
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let mut ti = 0;
    let doc_text = first
        .iter()
        .map(|l| {
            if l.table.is_some() {
                let s = crate::trace::table_marker(ti);
                ti += 1;
                s
            } else {
                l.text.clone()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let page = PageTrace {native_comparison_labels:vec![],native_horizontal_baselines:vec![],native_atomic_columns:vec![],
        width: 800.,
        height: 1000.,
        layout: "tiers",
        words: a.words,
        native_styles: a.styles,
        native_content_styles: a.content,
        native_bounds: a.bounds,
        native_axes: a.axes,
        native_rulings: a.rulings,
        regions: vec![Region {
            role: Role::Body,
            unit: 0,
            part: 0,
            lines: first,
        }],
        tables,
        text,
        doc_text,
    };
    let mut rules = crate::lines::PageRules::default();
    rules.enclosures = vec![[0., 0., 500., 550.]];
    (page, rules)
}
#[test]
fn inner_certificates_keep_distinct_dated_notices_and_their_complete_statements() {
    let (p, r) = stacked_publications(false);
    let cuts = super::source_scopes::ranges(&p, &r);
    assert_eq!(cuts, vec![(0, 12), (12, 24)]);
}
#[test]
fn inner_certificates_do_not_merge_independent_notices_by_equal_publisher_names() {
    let (p, r) = stacked_publications(true);
    assert_eq!(
        super::source_scopes::ranges(&p, &r),
        vec![(0, 12), (12, 24)]
    );
}
#[test]
fn inner_certificates_are_invariant_under_positive_scaling_translation_and_font_renaming() {
    for k in [0.0625, 1., 32.] {
        let (mut p, mut r) = stacked_publications(true);
        let tr = |b: &mut [f32; 4]| {
            b[0] = b[0] * k - 100.;
            b[2] = b[2] * k - 100.;
            b[1] = b[1] * k + 55.;
            b[3] = b[3] * k + 55.;
        };
        for w in &mut p.words {
            let mut b = [w.x0, w.y0, w.x1, w.y1];
            tr(&mut b);
            [w.x0, w.y0, w.x1, w.y1] = b;
            tr(&mut w.source_box);
        }
        for b in &mut p.native_bounds {
            if let NativeBounds::Glyphs(b) = b {
                tr(b);
            }
        }
        for l in &mut p.regions[0].lines {
            if let Some(t) = &mut l.table {
                let mut b = [t.x0, t.y0, t.x1, t.y1];
                tr(&mut b);
                [t.x0, t.y0, t.x1, t.y1] = b;
                for c in &mut t.cells {
                    let mut b = [c.x0, c.y0, c.x1, c.y1];
                    tr(&mut b);
                    [c.x0, c.y0, c.x1, c.y1] = b;
                }
            }
        }
        for b in &mut r.enclosures {
            tr(b);
        }
        for s in p.native_styles.iter_mut().flatten() {
            *s = s
                .iter()
                .map(|&(f, b)| (f + 29, (f64::from_bits(b) * k as f64).to_bits()))
                .collect();
        }
        for s in &mut p.native_content_styles {
            if let ContentStyles::Glyphs(s) = s {
                *s = s
                    .iter()
                    .map(|&(f, b)| (f + 29, (f64::from_bits(b) * k as f64).to_bits()))
                    .collect();
            }
        }
        assert_eq!(
            super::source_scopes::ranges(&p, &r),
            vec![(0, 12), (12, 24)]
        );
    }
}
#[test]
fn inner_certificates_do_not_split_a_statement_spanning_other_notice_bands() {
    let (mut p, r) = stacked_publications(false);
    p.regions[0]
        .lines
        .iter_mut()
        .find_map(|l| l.table.as_mut())
        .unwrap()
        .y1 = 350.;
    assert!(super::source_scopes::ranges(&p, &r).len() <= 1);
}
#[test]
fn inner_certificates_require_complete_source_styles_and_word_ownership() {
    for case in 0..2 {
        let (mut p, r) = stacked_publications(false);
        if case == 0 {
            p.native_styles[0] = None;
        } else {
            let id = p.regions[0].lines[0].words[0];
            p.regions[0].lines[12].words.push(id);
        }
        assert!(super::source_scopes::ranges(&p, &r).len() <= 1);
    }
}

fn opaque_statement_rows() -> (crate::trace::PageTrace, crate::lines::PageRules) {
    let (mut p, mut r) = stacked_publications(true);
    for l in &mut p.regions[0].lines {
        if let Some(t) = l.table.take() {
            l.words = t
                .cells
                .iter()
                .flat_map(|c| c.words.iter().copied())
                .collect();
            l.text = format!("△{}", t.render());
        }
    }
    r.horizontal = vec![crate::lines::HRule {
        y: 275.,
        x0: 0.,
        x1: 500.,
    }];
    (p, r)
}
#[test]
fn independent_source_bands_preserve_opaque_statement_rows_without_interpreting_added_symbols() {
    let (p, r) = opaque_statement_rows();
    assert_eq!(crate::notice_boundaries::cuts(&p, &r), vec![0, 12]);
}
#[test]
fn independent_source_bands_need_a_real_divider_and_both_own_signed_dates() {
    let (mut p, mut r) = opaque_statement_rows();
    r.horizontal.clear();
    assert_eq!(crate::notice_boundaries::cuts(&p, &r), vec![0]);
    r.horizontal = vec![crate::lines::HRule {
        y: 275.,
        x0: 0.,
        x1: 500.,
    }];
    p.regions[0].lines[2].text = "出所未確定".into();
    assert!(crate::notice_boundaries::cuts(&p, &r).is_empty());
}
#[test]
fn a_source_body_finance_divider_does_not_make_a_financial_annex_independent() {
    let (mut p, r) = opaque_statement_rows();
    let l = &mut p.regions[0].lines[12];
    l.text = "第456期決算公告".into();
    let id = l.words[0];
    p.words[id].text = l.text.clone();
    p.words[id].raw = l.text.clone();
    assert_eq!(crate::notice_boundaries::cuts(&p, &r), vec![0]);
}
#[test]
fn a_transverse_stroke_cannot_split_a_complete_table_or_borrow_incomplete_native_bounds() {
    let (mut p, r) = opaque_statement_rows();
    let id = p.regions[0].lines[13].words[0];
    p.native_bounds[id] = NativeBounds::Unknown;
    assert!(crate::notice_boundaries::cuts(&p, &r).is_empty());
    let (mut p, mut r) = stacked_publications(false);
    r.horizontal = vec![crate::lines::HRule {
        y: 275.,
        x0: 0.,
        x1: 500.,
    }];
    p.regions[0]
        .lines
        .iter_mut()
        .find_map(|l| l.table.as_mut())
        .unwrap()
        .y1 = 350.;
    assert!(!crate::notice_boundaries::cuts(&p, &r).contains(&12));
}

#[test]
fn source_order_preserves_each_independent_notice_with_its_own_complete_statement_block() {
    for same in [false, true] {
        let (p, r) = stacked_publications(same);
        let expected = p.regions[0].lines.clone();
        let mut units = vec![Unit::Plain(expected.clone())];
        order_with_source(
            &mut units,
            &p.words,
            &r.enclosures,
            p.width,
            p.height,
            &p.native_styles,
            &p.native_content_styles,
            &p.native_bounds,
            &p.native_axes,
            &p.native_rulings,
            &[],
        );
        let Unit::Plain(actual) = &units[0] else {
            panic!();
        };
        assert_eq!(actual,&expected,"the full source-order pass must keep each own dated operation and annex together, not move every statement after every body");
    }
}

#[test]
fn source_dividers_are_invariant_under_independent_axis_scales_and_font_ids() {
    for (kx, ky) in [(0.125, 8.), (8., 0.125), (1., 1.)] {
        let (mut p, mut r) = opaque_statement_rows();
        let tr = |b: &mut [f32; 4]| {
            b[0] = b[0] * kx - 100.;
            b[2] = b[2] * kx - 100.;
            b[1] = b[1] * ky + 55.;
            b[3] = b[3] * ky + 55.;
        };
        p.width *= kx;
        p.height *= ky;
        for w in &mut p.words {
            let mut b = [w.x0, w.y0, w.x1, w.y1];
            tr(&mut b);
            [w.x0, w.y0, w.x1, w.y1] = b;
            tr(&mut w.source_box);
        }
        for b in &mut p.native_bounds {
            if let NativeBounds::Glyphs(b) = b {
                tr(b);
            }
        }
        for b in &mut r.enclosures {
            tr(b);
        }
        for h in &mut r.horizontal {
            h.x0 = h.x0 * kx - 100.;
            h.x1 = h.x1 * kx - 100.;
            h.y = h.y * ky + 55.;
        }
        for s in p.native_styles.iter_mut().flatten() {
            *s = s
                .iter()
                .map(|&(f, b)| (f + 41, (f64::from_bits(b) * ky as f64).to_bits()))
                .collect();
        }
        for s in &mut p.native_content_styles {
            if let ContentStyles::Glyphs(s) = s {
                *s = s
                    .iter()
                    .map(|&(f, b)| (f + 41, (f64::from_bits(b) * ky as f64).to_bits()))
                    .collect();
            }
        }
        assert_eq!(
            crate::notice_boundaries::cuts(&p, &r),
            vec![0, 12],
            "independent axis scales {kx} {ky}"
        );
        p.regions[0].lines[2].text = "出所未確定".into();
        assert!(!crate::notice_boundaries::cuts(&p,&r).contains(&12),"a financial header's later date cannot supply the operative publication at scales {kx} {ky}");
    }
}
#[test]
fn a_transversal_must_meet_every_word_not_only_an_opaque_rows_union_box() {
    let (mut p, mut r) = opaque_statement_rows();
    r.horizontal[0].x0 = 60.;
    let rows = p.regions[0]
        .lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.text.starts_with('△'))
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    let id = p.regions[0].lines[rows[0]].words[0];
    let other = p.regions[0].lines[rows[1]].words[0];
    p.words[other].x0 += 50.;
    p.words[other].x1 += 50.;
    p.words[other].source_box[0] += 50.;
    p.words[other].source_box[2] += 50.;
    if let NativeBounds::Glyphs(b) = &mut p.native_bounds[other] {
        b[0] += 50.;
        b[2] += 50.;
    }
    let new = p.words.len();
    let mut word = p.words[id].clone();
    word.id = new;
    word.x0 = 200.;
    word.x1 = 210.;
    word.source_box = [word.x0, word.y0, word.x1, word.y1];
    p.native_bounds.push(NativeBounds::Glyphs(word.source_box));
    p.words.push(word);
    p.native_styles.push(p.native_styles[id].clone());
    p.native_content_styles
        .push(p.native_content_styles[id].clone());
    p.native_axes.push(NativeAxis::Right);
    p.native_rulings
        .push(crate::native_word_rulings::NativeRuling::Unknown);
    p.regions[0].lines[rows[0]].words.push(new);
    p.regions[0].lines[rows[0]].text.push_str(" 123");
    assert_eq!(crate::notice_boundaries::cuts(&p, &r), vec![0]);
}
#[test]
fn a_quotation_opened_outside_the_divided_frame_does_not_release_inner_notices() {
    let (mut p, r) = opaque_statement_rows();
    let ids = p
        .words
        .iter()
        .filter(|w| w.status == WordStatus::Margin)
        .map(|w| w.id)
        .collect::<Vec<_>>();
    for (&id, text) in ids.iter().zip(["「", "」"]) {
        p.words[id].status = WordStatus::Used;
        p.words[id].text = text.into();
        p.words[id].raw = text.into();
    }
    p.regions[0]
        .lines
        .insert(0, Line::text("「".into(), vec![ids[0]]));
    p.regions[0]
        .lines
        .push(Line::text("」".into(), vec![ids[1]]));
    assert!(crate::notice_boundaries::cuts(&p, &r).is_empty());
}
#[test]
fn compatibility_bounds_cannot_authorize_a_new_native_divider_certificate() {
    let (mut p, r) = opaque_statement_rows();
    let id = p.regions[0]
        .lines
        .iter()
        .find(|l| l.text.starts_with('△'))
        .unwrap()
        .words[0];
    p.native_bounds[id] = NativeBounds::Compatibility;
    assert_eq!(crate::notice_boundaries::cuts(&p, &r), vec![0]);
}

#[test]
fn unresolved_source_words_cannot_cross_a_new_notice_divider_even_when_not_rendered() {
    for case in 0..3 {
        let (mut p, r) = opaque_statement_rows();
        let id = p
            .words
            .iter()
            .find(|w| w.status == WordStatus::Margin)
            .unwrap()
            .id;
        let b = if case == 1 {
            [210., 270., 230., 280.]
        } else {
            [210., 210., 230., 218.]
        };
        p.words[id].status = WordStatus::Empty;
        p.words[id].raw = "\u{e000}".into();
        p.words[id].text.clear();
        [
            p.words[id].x0,
            p.words[id].y0,
            p.words[id].x1,
            p.words[id].y1,
        ] = b;
        p.words[id].source_box = b;
        p.native_styles[id] = None;
        p.native_content_styles[id] = ContentStyles::Unknown;
        p.native_bounds[id] = if case == 2 {
            NativeBounds::Unknown
        } else {
            NativeBounds::Glyphs(b)
        };
        assert_eq!(
            crate::notice_boundaries::cuts(&p, &r),
            if case == 0 { vec![0, 12] } else { vec![0] },
            "unresolved original glyph geometry still belongs to the source scope"
        );
    }
}

fn with_publisher_note(text: &str, size: f64) -> Fixture {
    let mut f = fixture();
    let note = f.put(text, [20., 65., 150., 68.], 3, size, NativeAxis::Right);
    f.units[0].insert(4, note);
    f
}
#[test]
fn former_designation_notes_are_not_identity_aliases() {
    for text in [
        "（旧商号任意の補足）".to_owned(),
        format!("(旧社名{})", "任意（入れ子）の補足".repeat(25)),
    ] {
        let f = with_publisher_note(&text, 6.);
        let out = f.run();
        assert_eq!(out[0].text, "新設分割公告");
        assert!(out.iter().any(|l| l.text == text));
    }
}
#[test]
fn annotation_does_not_lend_a_foreign_publisher() {
    let mut f = with_publisher_note("（旧商号株式会社みどり）", 6.);
    f.rename("株式会社みどり", "株式会社別会社");
    // Restore only the body actor and annotation to the other issuer.
    for index in [4, 12] {
        let old = f.units[0][index].text.clone();
        let new = old.replace("株式会社別会社", "株式会社みどり");
        f.units[0][index].text = new.clone();
        for &id in &f.units[0][index].words {
            f.words[id].text = f.words[id].text.replace("株式会社別会社", "株式会社みどり");
            f.words[id].raw = f.words[id].text.clone();
        }
    }
    assert_eq!(f.run(), f.units[0]);
}
#[test]
fn unbalanced_prose_or_equal_size_suffix_is_not_a_separate_note() {
    for (text, size) in [
        ("（任意の補足", 6.),
        ("補足です。", 6.),
        ("（旧商号任意の補足）", 9.),
        ("（旧商号任意の補足）別会社", 6.),
        ("（任意の補足)", 6.),
    ] {
        let f = with_publisher_note(text, size);
        assert_eq!(f.run(), f.units[0]);
    }
}
#[test]
fn unknown_address_content_can_stop_but_cannot_supply_publisher_identity() {
    let mut f = fixture();
    let address = f.units[0][2].words[0];
    f.content[address] = ContentStyles::Unknown;
    assert_eq!(f.run()[0].text, "新設分割公告");
    let name = f.units[0][3].words[0];
    f.content[name] = ContentStyles::Unknown;
    assert_eq!(f.run(), f.units[0]);
}
fn side_by_side_publications() -> (crate::trace::PageTrace, crate::lines::PageRules) {
    let (mut p, r) = stacked_publications(false);
    let half = p.words.len() / 2;
    // Two body columns above a disjoint shared financial band; the names
    // alone, rather than x proximity, assign each complete financial annex.
    let tr = |id: usize, b: &mut [f32; 4]| {
        let (dx, dy) = match (id >= half, p.native_axes[id]) {
            (false, NativeAxis::Down) => (0., 0.),
            (true, NativeAxis::Down) => (-180., -300.),
            (false, _) => (0., 300.),
            (true, _) => (170., 0.),
        };
        b[0] += dx;
        b[2] += dx;
        b[1] += dy;
        b[3] += dy;
    };
    for w in &mut p.words {
        let mut b = [w.x0, w.y0, w.x1, w.y1];
        tr(w.id, &mut b);
        [w.x0, w.y0, w.x1, w.y1] = b;
        tr(w.id, &mut w.source_box);
    }
    for (i, b) in p.native_bounds.iter_mut().enumerate() {
        if let NativeBounds::Glyphs(b) = b {
            tr(i, b);
        }
    }
    for l in &mut p.regions[0].lines {
        if let Some(t) = &mut l.table {
            let id = t.cells[0].words[0];
            let mut b = [t.x0, t.y0, t.x1, t.y1];
            tr(id, &mut b);
            [t.x0, t.y0, t.x1, t.y1] = b;
            for c in &mut t.cells {
                let mut b = [c.x0, c.y0, c.x1, c.y1];
                tr(c.words[0], &mut b);
                [c.x0, c.y0, c.x1, c.y1] = b;
            }
        }
    }
    (p, r)
}
#[test]
fn unique_parties_bind_disjoint_shared_band_annexes() {
    let (p, r) = side_by_side_publications();
    assert_eq!(
        super::source_scopes::ranges(&p, &r),
        vec![(0, 12), (12, 24)]
    );
}
#[test]
fn shared_band_ambiguous_publishers_do_not_partition() {
    let (mut p, r) = side_by_side_publications();
    for w in &mut p.words {
        w.text = w.text.replace("株式会社あお", "株式会社みどり");
        w.raw = w.text.clone();
    }
    for l in &mut p.regions[0].lines {
        l.text = l.text.replace("株式会社あお", "株式会社みどり");
    }
    assert!(super::source_scopes::ranges(&p, &r).len() <= 1);
}

#[test]
fn annotations_survive_axis_scaling_and_font_renaming() {
    let f = with_publisher_note("（旧商号任意（入れ子）の補足）", 6.);
    let want = order(&f.run());
    for (sx, sy) in [(0.125, 64.), (64., 0.125)] {
        let mut x = f.clone();
        for s in x.styles.iter_mut().flatten() {
            *s = s
                .iter()
                .map(|&(f, b)| (f + 53, (f64::from_bits(b) * 7.).to_bits()))
                .collect();
        }
        for s in &mut x.content {
            if let ContentStyles::Glyphs(s) = s {
                *s = s
                    .iter()
                    .map(|&(f, b)| (f + 53, (f64::from_bits(b) * 7.).to_bits()))
                    .collect();
            }
        }
        for (w, b) in x.words.iter_mut().zip(&mut x.bounds) {
            w.x0 *= sx;
            w.x1 *= sx;
            w.y0 *= sy;
            w.y1 *= sy;
            w.source_box = [
                w.source_box[0] * sx,
                w.source_box[1] * sy,
                w.source_box[2] * sx,
                w.source_box[3] * sy,
            ];
            if let NativeBounds::Glyphs(r) = b {
                *r = [r[0] * sx, r[1] * sy, r[2] * sx, r[3] * sy];
            }
        }
        for l in x.units.iter_mut().flatten() {
            if let Some(t) = &mut l.table {
                t.x0 *= sx;
                t.x1 *= sx;
                t.y0 *= sy;
                t.y1 *= sy;
                for c in &mut t.cells {
                    c.x0 *= sx;
                    c.x1 *= sx;
                    c.y0 *= sy;
                    c.y1 *= sy;
                }
            }
        }
        let mut us = x.units.iter().cloned().map(Unit::Plain).collect::<Vec<_>>();
        order_with_source(
            &mut us,
            &x.words,
            &[[0., 0., 500. * sx, 250. * sy]],
            800. * sx,
            1000. * sy,
            &x.styles,
            &x.content,
            &x.bounds,
            &x.axes,
            &x.rulings,
            &[],
        );
        let Unit::Plain(ls) = &us[0] else { panic!() };
        assert_eq!(order(ls), want);
    }
}

#[test]
fn annotation_roles_survive_source_word_and_presentation_line_partition() {
    let mut f = with_publisher_note("（旧商号任意の補足）", 6.);
    let note = f.units[0].remove(4);
    f.units[0][3].text.push_str(&note.text);
    f.units[0][3].words.extend(note.words);
    assert_eq!(f.run()[0].text, "新設分割公告");
    let mut f = fixture();
    let a = f.put(
        "（旧商号任意の",
        [20., 65., 70., 66.],
        3,
        6.,
        NativeAxis::Right,
    );
    let b = f.put("補足）", [20., 67., 70., 68.], 3, 6., NativeAxis::Right);
    f.units[0].splice(4..4, [a, b]);
    assert_eq!(f.run()[0].text, "新設分割公告");
}
#[test]
fn unknown_annotation_or_publisher_style_cannot_supply_name_proof() {
    for index in [3, 4] {
        let mut f = with_publisher_note("（旧商号任意の補足）", 6.);
        let id = f.units[0][index].words[0];
        f.content[id] = ContentStyles::Unknown;
        assert_eq!(f.run(), f.units[0]);
    }
}

#[test]
fn unlabelled_small_parentheses_can_be_a_legal_name_and_are_not_removed() {
    for note in [
        "（任意の補足）",
        "（東日本）",
        "（現商号株式会社任意）",
        "（新商号株式会社任意）",
        "（商号株式会社任意）",
        "（旧商号）",
        "（旧商号：）",
        "（旧商号株式会社任意）（別の欄）",
    ] {
        let f = with_publisher_note(note, 6.);
        assert_eq!(f.run(), f.units[0], "{note}");
    }
}
#[test]
fn former_designation_fields_use_their_role_not_their_company_value() {
    for label in ["旧商号", "旧社名", "旧名称"] {
        for value in [
            "任意の名称".to_owned(),
            "異なる任意（内部の括弧）の名称".repeat(80),
        ] {
            let f = with_publisher_note(&format!("（{label}：{value}）"), 6.);
            assert_eq!(f.run()[0].text, "新設分割公告");
        }
    }
}

#[test]
fn a_typed_former_designation_does_not_depend_on_its_font_family() {
    for family in [4, 79] {
        let mut f = with_publisher_note("（旧商号任意の別名称）", 6.);
        let id = f.units[0][4].words[0];
        let s = NativeStyles::from([(family, 6f64.to_bits())]);
        f.styles[id] = Some(s.clone());
        f.content[id] = ContentStyles::Glyphs(s);
        assert_eq!(f.run()[0].text, "新設分割公告", "metadata family {family}");
    }
}
#[test]
fn other_font_notes_do_not_make_current_or_unlabelled_names_historical() {
    for text in ["（現商号任意の名称）", "（任意の名称）", "（旧商号）"] {
        let mut f = with_publisher_note(text, 6.);
        let id = f.units[0][4].words[0];
        let s = NativeStyles::from([(79, 6f64.to_bits())]);
        f.styles[id] = Some(s.clone());
        f.content[id] = ContentStyles::Glyphs(s);
        assert_eq!(f.run(), f.units[0]);
    }
}

#[path="financial_notice_relation_tests.rs"]
mod notice_relation_tests;
