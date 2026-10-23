use super::*;
fn atom(id: usize, t: &str, x: f32, y: f32, w: f32) -> Atom {
    Atom {
        id,
        owner: Some(7),
        eligible: true,
        raw: t.into(),
        text: t.into(),
        x0: x,
        y0: y,
        x1: x + w,
        y1: y + 8.184,
    }
}
fn good() -> (Vec<Atom>, Vec<Bar>) {
    (
        vec![
            atom(54, "利率", 631., 272.336, 80.),
            atom(37, "１", 723., 272.364, 8.),
            atom(96, "元金×", 525., 277.832, 104.),
            atom(45, "×", 713., 277.86, 8.),
            atom(50, "100", 665., 283.336, 12.),
            atom(38, "２", 723., 283.364, 8.),
        ],
        vec![
            Bar {
                y: 281.,
                x0: 629.,
                x1: 713.,
            },
            Bar {
                y: 281.,
                x0: 721.,
                x1: 733.,
            },
        ],
    )
}
fn reject(mutator: impl FnOnce(&mut Vec<Atom>, &mut Vec<Bar>)) {
    let (mut a, mut b) = good();
    mutator(&mut a, &mut b);
    assert!(parse(&a, &b).is_none());
}
#[test]
fn source_ast_order_and_both_bars() {
    let (a, b) = good();
    let f = parse(&a, &b).unwrap();
    assert_eq!(f.tree.render(), "元金×(利率/100)×(１/２)");
    assert_eq!(f.source_ids, vec![54, 37, 96, 45, 50, 38]);
    assert_eq!(f.owner, 7);
    let Expr::Sequence(xs) = f.tree else { panic!() };
    assert!(matches!(&xs[1],Expr::Fraction{bar,..} if *bar==b[0]));
    assert!(matches!(&xs[3],Expr::Fraction{bar,..} if *bar==b[1]));
}
#[test]
fn missing_short_bar() {
    reject(|_, b| {
        b.pop();
    });
}
#[test]
fn missing_long_bar() {
    reject(|_, b| {
        b.remove(0);
    });
}
#[test]
fn duplicate_bar_ambiguous() {
    reject(|_, b| b.push(b[1]));
}
#[test]
fn underline_has_no_lower_operand() {
    reject(|a, _| a[4].y0 = 300.);
}
#[test]
fn dash_without_upper_operand() {
    reject(|a, _| {
        a[1].y0 = 260.;
        a[1].y1 = 268.184;
    });
}
#[test]
fn single_number_not_expression() {
    reject(|a, _| {
        a.clear();
        a.push(atom(1, "１", 723., 272.364, 8.));
    });
}
#[test]
fn numeric_only_left_not_this_pass() {
    reject(|a, _| {
        a[0].text = "123".into();
        a[0].raw = a[0].text.clone();
    });
}
#[test]
fn no_source_prefix_times() {
    reject(|a, _| {
        a[2].text = "元金".into();
        a[2].raw = a[2].text.clone();
    });
}
#[test]
fn no_source_middle_times() {
    reject(|a, _| {
        a[3].text = "＋".into();
        a[3].raw = a[3].text.clone();
    });
}
#[test]
fn compound_numerator_preserved() {
    reject(|a, _| {
        a[0].text = "利率/２".into();
        a[0].raw = a[0].text.clone();
    });
}
#[test]
fn unknown_pua_preserved() {
    reject(|a, _| {
        a[0].text = "利率\u{e0aa}".into();
        a[0].raw = a[0].text.clone();
    });
}
#[test]
fn normalized_unknown_not_silently_dropped() {
    reject(|a, _| a[0].raw.push('\u{e0aa}'));
}
#[test]
fn different_cell_owner() {
    reject(|a, _| a[1].owner = Some(8));
}
#[test]
fn unowned_operand() {
    reject(|a, _| a[1].owner = None);
}
#[test]
fn duplicate_source_id() {
    reject(|a, _| a[1].id = a[0].id);
}
#[test]
fn suppressed_operand() {
    reject(|a, _| a[1].eligible = false);
}
#[test]
fn extra_word_not_ignored() {
    reject(|a, _| a.push(atom(55, "注", 600., 270., 8.)));
}
#[test]
fn wrong_denominator_center() {
    reject(|a, _| {
        a[4].x0 += 8.;
        a[4].x1 += 8.;
    });
}
#[test]
fn superscript_wrong_size() {
    reject(|a, _| a[1].y1 = a[1].y0 + 4.);
}
#[test]
fn unrelated_second_bar() {
    reject(|_, b| b[1].y += 10.);
}
#[test]
fn overlapping_bars() {
    reject(|_, b| b[1].x0 -= 10.);
}
#[test]
fn denominator_zero_rejected() {
    reject(|a, _| {
        a[5].text = "０".into();
        a[5].raw = a[5].text.clone();
    });
}
