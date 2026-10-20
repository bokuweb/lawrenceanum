use super::*;
fn atom(id: usize, b: [f32; 4], text: &str) -> Atom {
    Atom {
        id,
        x0: b[0],
        y0: b[1],
        x1: b[2],
        y1: b[3],
        raw: text.into(),
        text: text.into(),
        owner: Some(3),
        eligible: true,
    }
}
fn fixture() -> (Vec<Atom>, Vec<Bar>) {
    let mut atoms = vec![
        atom(10, [595., 458.002, 687., 466.186], "A＋B×C"),
        atom(20, [553., 469.002, 705., 477.186], "D＋E"),
        atom(30, [527., 474.503, 543., 482.687], "１＋"),
        atom(40, [623., 480.003, 635., 488.187], "100"),
        atom(50, [715., 474.503, 755., 482.687], "×F"),
    ];
    for (id, b, raw) in [
        (
            60,
            [543.8597, 469.6429, 552.0437, 485.6429],
            "\u{eae9}\u{eaea}",
        ),
        (
            70,
            [707.8597, 469.6429, 716.0437, 485.6429],
            "\u{eaeb}\u{eaec}",
        ),
    ] {
        let mut p = atom(id, b, raw);
        p.text.clear();
        p.owner = None;
        atoms.push(p);
    }
    (
        atoms,
        vec![
            Bar {
                y: 466.643,
                x0: 525.,
                x1: 757.,
            },
            Bar {
                y: 477.643,
                x0: 551.,
                x1: 707.,
            },
        ],
    )
}
fn reject(a: Vec<Atom>, b: Vec<Bar>) {
    assert!(recognize(&a, &b).is_empty());
}
#[test]
fn exact_tree_has_suffix_inside_outer_denominator() {
    let (a, b) = fixture();
    let f = recognize(&a, &b);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].text, "(A＋B×C)/(１＋((D＋E)/100)×F)");
    assert_eq!(f[0].source_ids, vec![10, 20, 30, 50, 40]);
    assert_eq!(f[0].bracket_ids, [60, 70]);
    assert_eq!(f[0].owner, 3);
}
#[test]
fn translations_and_font_scale_do_not_change_tree() {
    let (mut a, mut b) = fixture();
    for x in &mut a {
        x.x0 = x.x0 * 1.2 - 100.;
        x.x1 = x.x1 * 1.2 - 100.;
        x.y0 = x.y0 * 1.2 - 400.;
        x.y1 = x.y1 * 1.2 - 400.;
    }
    for x in &mut b {
        x.x0 = x.x0 * 1.2 - 100.;
        x.x1 = x.x1 * 1.2 - 100.;
        x.y = x.y * 1.2 - 400.;
    }
    assert_eq!(recognize(&a, &b).len(), 1);
}
#[test]
fn arbitrary_source_variables_are_serialized_without_literal_equation() {
    let (mut a, b) = fixture();
    a[0].text = "7＋残高×率".into();
    a[0].raw = a[0].text.clone();
    a[1].text = "利回り＋格差".into();
    a[1].raw = a[1].text.clone();
    a[4].text = "×期間".into();
    a[4].raw = a[4].text.clone();
    assert_eq!(
        recognize(&a, &b)[0].text,
        "(7＋残高×率)/(１＋((利回り＋格差)/100)×期間)"
    );
}
#[test]
fn ordinary_two_parallel_bars_are_not_nested() {
    let (a, mut b) = fixture();
    b[0].y = b[1].y;
    b[0].x0 = 100.;
    b[0].x1 = 250.;
    reject(a, b);
}
#[test]
fn absent_outer_bar_is_rejected() {
    let (a, mut b) = fixture();
    b.remove(0);
    reject(a, b);
}
#[test]
fn absent_inner_bar_is_rejected() {
    let (a, mut b) = fixture();
    b.pop();
    reject(a, b);
}
#[test]
fn uncontained_inner_bar_is_rejected() {
    let (a, mut b) = fixture();
    b[1].x0 = 520.;
    reject(a, b);
}
#[test]
fn inner_bar_above_parent_is_rejected() {
    let (a, mut b) = fixture();
    b[1].y = 455.;
    reject(a, b);
}
#[test]
fn missing_left_components_are_rejected() {
    let (mut a, b) = fixture();
    a.remove(5);
    reject(a, b);
}
#[test]
fn missing_right_components_are_rejected() {
    let (mut a, b) = fixture();
    a.pop();
    reject(a, b);
}
#[test]
fn unknown_bracket_component_is_not_inferred() {
    let (mut a, b) = fixture();
    a[5].raw = "\u{e0aa}\u{eaea}".into();
    reject(a, b);
}
#[test]
fn reversed_composite_parts_are_rejected() {
    let (mut a, b) = fixture();
    a[5].raw = "\u{eaea}\u{eae9}".into();
    reject(a, b);
}
#[test]
fn bracket_with_an_existing_owner_is_rejected() {
    let (mut a, b) = fixture();
    a[5].owner = Some(8);
    reject(a, b);
}
#[test]
fn operands_must_have_one_common_owner() {
    let (mut a, b) = fixture();
    a[1].owner = Some(4);
    reject(a, b);
}
#[test]
fn multiply_owned_operand_is_rejected() {
    let (mut a, b) = fixture();
    a[1].owner = None;
    reject(a, b);
}
#[test]
fn a_third_bar_inside_the_expression_is_ambiguous() {
    let (a, mut b) = fixture();
    b.push(Bar {
        y: 482.,
        x0: 600.,
        x1: 650.,
    });
    reject(a, b);
}
#[test]
fn extra_text_inside_the_fraction_is_not_dropped() {
    let (mut a, b) = fixture();
    a.push(atom(80, [640., 483., 650., 491.], "注"));
    reject(a, b);
}
#[test]
fn outer_numerator_must_be_centered() {
    let (mut a, b) = fixture();
    a[0].x0 += 20.;
    a[0].x1 += 20.;
    reject(a, b);
}
#[test]
fn inner_numerator_must_be_centered() {
    let (mut a, b) = fixture();
    a[1].x0 -= 20.;
    a[1].x1 -= 20.;
    reject(a, b);
}
#[test]
fn denominator_must_be_numeric_and_not_a_complex_expression() {
    let (mut a, b) = fixture();
    a[3].text = "100＋2".into();
    a[3].raw = a[3].text.clone();
    reject(a, b);
}
#[test]
fn suffix_addition_cannot_be_simplified_to_multiplication() {
    let (mut a, b) = fixture();
    a[4].text = "×F＋G".into();
    a[4].raw = a[4].text.clone();
    reject(a, b);
}
#[test]
fn a_preexisting_division_or_parenthesis_is_not_reinterpreted() {
    let (mut a, b) = fixture();
    a[1].text = "D/E".into();
    a[1].raw = a[1].text.clone();
    reject(a, b);
}
#[test]
fn a_normalized_away_native_symbol_is_not_silently_ignored() {
    let (mut a, b) = fixture();
    a.push(atom(80, [650., 474., 658., 482.], "\u{e0aa}"));
    a.last_mut().unwrap().text.clear();
    reject(a, b);
}
#[test]
fn mismatched_font_sizes_are_rejected() {
    let (mut a, b) = fixture();
    a[1].y0 -= 5.;
    reject(a, b);
}
#[test]
fn duplicate_source_ids_are_rejected() {
    let (mut a, b) = fixture();
    a[1].id = a[0].id;
    reject(a, b);
}

#[test]
fn numeric_prefix_and_denominator_are_source_data_not_constants() {
    let (mut a, b) = fixture();
    a[2].raw = "２.５＋".into();
    a[2].text = a[2].raw.clone();
    a[3].raw = "365".into();
    a[3].text = a[3].raw.clone();
    assert_eq!(
        recognize(&a, &b)[0].text,
        "(A＋B×C)/(２.５＋((D＋E)/365)×F)"
    );
}
#[test]
fn duplicate_parenthesis_pair_is_an_ambiguous_native_partition() {
    let (mut a, b) = fixture();
    let mut p = a[5].clone();
    p.id = 81;
    a.push(p);
    reject(a, b);
}
#[test]
fn an_operand_crossing_the_outer_rule_is_not_clipped() {
    let (mut a, b) = fixture();
    a[0].x0 = 510.;
    reject(a, b);
}
#[test]
fn a_vertical_gap_does_not_turn_another_paragraph_into_a_fraction() {
    let (mut a, b) = fixture();
    a[0].y0 -= 15.;
    a[0].y1 -= 15.;
    reject(a, b);
}

#[test]
fn multiplication_has_an_explicit_denominator_tree_position() {
    let (a, b) = fixture();
    let f = recognize(&a, &b);
    let Expr::Fraction {
        bar,
        numerator,
        denominator,
    } = &f[0].tree
    else {
        panic!()
    };
    assert_eq!(*bar, b[0]);
    assert!(matches!(numerator.as_ref(),Expr::Source{word_ids,..} if word_ids==&vec![10]));
    let Expr::Sequence(parts) = denominator.as_ref() else {
        panic!()
    };
    assert_eq!(parts.len(), 3);
    assert!(matches!(&parts[2],Expr::Source{word_ids,text} if word_ids==&vec![50]&&text=="×F"));
    let Expr::NativeParentheses { word_ids, child } = &parts[1] else {
        panic!()
    };
    assert_eq!(*word_ids, [60, 70]);
    assert!(matches!(child.as_ref(),Expr::Fraction{bar,..} if *bar==b[1]));
}
