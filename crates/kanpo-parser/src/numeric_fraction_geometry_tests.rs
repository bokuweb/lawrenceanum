use super::*;
pub(crate) fn two() -> (Vec<Atom>, Vec<Bar>) {
    (
        vec![
            Atom {
                id: 28,
                owner: Some(0),
                eligible: true,
                raw: "1".into(),
                text: "1".into(),
                x0: 567.0f32,
                y0: 130.3149f32,
                x1: 571.0f32,
                y1: 138.4989f32,
            },
            Atom {
                id: 26,
                owner: Some(0),
                eligible: true,
                raw: "".into(),
                text: ".".into(),
                x0: 568.0f32,
                y0: 130.3149f32,
                x1: 576.0f32,
                y1: 138.4989f32,
            },
            Atom {
                id: 24,
                owner: Some(0),
                eligible: true,
                raw: "87".into(),
                text: "87".into(),
                x0: 573.0f32,
                y0: 130.3149f32,
                x1: 581.0f32,
                y1: 138.4989f32,
            },
            Atom {
                id: 21,
                owner: Some(0),
                eligible: true,
                raw: "１".into(),
                text: "１".into(),
                x0: 593.0f32,
                y0: 130.2923f32,
                x1: 601.0f32,
                y1: 138.4763f32,
            },
            Atom {
                id: 61,
                owner: Some(0),
                eligible: true,
                raw: "額面金額×".into(),
                text: "額面金額×".into(),
                x0: 525.0f32,
                y0: 135.8109f32,
                x1: 565.0f32,
                y1: 143.9949f32,
            },
            Atom {
                id: 23,
                owner: Some(0),
                eligible: true,
                raw: "×".into(),
                text: "×".into(),
                x0: 583.0f32,
                y0: 135.7883f32,
                x1: 591.0f32,
                y1: 143.9723f32,
            },
            Atom {
                id: 27,
                owner: Some(0),
                eligible: true,
                raw: "100".into(),
                text: "100".into(),
                x0: 568.0f32,
                y0: 141.3149f32,
                x1: 580.0f32,
                y1: 149.4989f32,
            },
            Atom {
                id: 22,
                owner: Some(0),
                eligible: true,
                raw: "２".into(),
                text: "２".into(),
                x0: 593.0f32,
                y0: 141.2923f32,
                x1: 601.0f32,
                y1: 149.4763f32,
            },
        ],
        vec![
            Bar {
                y: 138.929f32,
                x0: 565f32,
                x1: 583f32,
            },
            Bar {
                y: 138.929f32,
                x0: 591f32,
                x1: 603f32,
            },
        ],
    )
}
fn grouped() -> (Vec<Atom>, Vec<Bar>) {
    (
        vec![
            Atom {
                id: 100,
                owner: Some(0),
                eligible: true,
                raw: "1".into(),
                text: "1".into(),
                x0: 200.0f32,
                y0: 82.9637f32,
                x1: 204.0f32,
                y1: 91.1477f32,
            },
            Atom {
                id: 99,
                owner: Some(0),
                eligible: true,
                raw: "".into(),
                text: ".".into(),
                x0: 201.0f32,
                y0: 82.9637f32,
                x1: 209.0f32,
                y1: 91.1477f32,
            },
            Atom {
                id: 98,
                owner: Some(0),
                eligible: true,
                raw: "8".into(),
                text: "8".into(),
                x0: 206.0f32,
                y0: 82.9637f32,
                x1: 210.0f32,
                y1: 91.1477f32,
            },
            Atom {
                id: 94,
                owner: Some(0),
                eligible: true,
                raw: "１".into(),
                text: "１".into(),
                x0: 231.0f32,
                y0: 82.9669f32,
                x1: 239.0f32,
                y1: 91.1509f32,
            },
            Atom {
                id: 91,
                owner: Some(0),
                eligible: true,
                raw: "２".into(),
                text: "２".into(),
                x0: 253.0f32,
                y0: 82.9709f32,
                x1: 261.0f32,
                y1: 91.1549f32,
            },
            Atom {
                id: 126,
                owner: Some(0),
                eligible: true,
                raw: "額面金額×".into(),
                text: "額面金額×".into(),
                x0: 157.0f32,
                y0: 88.4597f32,
                x1: 197.0f32,
                y1: 96.6437f32,
            },
            Atom {
                id: 97,
                owner: Some(0),
                eligible: true,
                raw: "×".into(),
                text: "×".into(),
                x0: 213.0f32,
                y0: 88.4669f32,
                x1: 221.0f32,
                y1: 96.6509f32,
            },
            Atom {
                id: 93,
                owner: Some(0),
                eligible: true,
                raw: "−".into(),
                text: "−".into(),
                x0: 241.0f32,
                y0: 88.4669f32,
                x1: 249.0f32,
                y1: 96.6509f32,
            },
            Atom {
                id: 101,
                owner: Some(0),
                eligible: true,
                raw: "100".into(),
                text: "100".into(),
                x0: 199.0f32,
                y0: 93.9637f32,
                x1: 211.0f32,
                y1: 102.1477f32,
            },
            Atom {
                id: 95,
                owner: Some(0),
                eligible: true,
                raw: "２".into(),
                text: "２".into(),
                x0: 231.0f32,
                y0: 93.9669f32,
                x1: 239.0f32,
                y1: 102.1509f32,
            },
            Atom {
                id: 92,
                owner: Some(0),
                eligible: true,
                raw: "365".into(),
                text: "365".into(),
                x0: 251.0f32,
                y0: 93.9709f32,
                x1: 263.0f32,
                y1: 102.1549f32,
            },
            Atom {
                id: 96,
                owner: None,
                eligible: true,
                raw: "".into(),
                text: "".into(),
                x0: 221.8597f32,
                y0: 83.6071f32,
                x1: 230.0437f32,
                y1: 99.6071f32,
            },
            Atom {
                id: 90,
                owner: None,
                eligible: true,
                raw: "".into(),
                text: "".into(),
                x0: 265.8597f32,
                y0: 83.6071f32,
                x1: 274.0437f32,
                y1: 99.6071f32,
            },
        ],
        vec![
            Bar {
                y: 91.607f32,
                x0: 197f32,
                x1: 213f32,
            },
            Bar {
                y: 91.607f32,
                x0: 229f32,
                x1: 241f32,
            },
            Bar {
                y: 91.607f32,
                x0: 249f32,
                x1: 265f32,
            },
        ],
    )
}

#[test]
fn two_real_numeric_fractions() {
    let (a, b) = two();
    let f = parse(&a, &b);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].tree.render(), "額面金額×(1.87/100)×(１/２)");
    assert_eq!(f[0].original, "1.87１額面金額××100２");
}
#[test]
fn native_group_preserves_minus_precedence() {
    let (a, b) = grouped();
    let f = parse(&a, &b);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].tree.render(), "額面金額×(1.8/100)×((１/２)−(２/365))");
    assert_eq!(f[0].bracket_ids, vec![96, 90]);
}
#[test]
fn missing_one_bar() {
    let (a, mut b) = two();
    b.pop();
    assert!(parse(&a, &b).is_empty());
}
#[test]
fn missing_group_bar() {
    let (a, mut b) = grouped();
    b.remove(1);
    assert!(parse(&a, &b).is_empty());
}
#[test]
fn duplicate_bar_ambiguous() {
    let (a, mut b) = two();
    b.push(b[0]);
    assert!(parse(&a, &b).is_empty());
}
#[test]
fn wrong_bar_baseline() {
    let (a, mut b) = two();
    b[1].y += 2.;
    assert!(parse(&a, &b).is_empty());
}
#[test]
fn unknown_decimal_pua() {
    let (mut a, b) = two();
    a[1].raw = "\u{eaff}".into();
    assert!(parse(&a, &b).is_empty());
}
#[test]
fn decimal_text_mismatch() {
    let (mut a, b) = two();
    a[1].text = "0".into();
    assert!(parse(&a, &b).is_empty());
}
#[test]
fn duplicate_word_id() {
    let (mut a, b) = two();
    a[1].id = a[0].id;
    assert!(parse(&a, &b).is_empty());
}
#[test]
fn foreign_owner() {
    let (mut a, b) = two();
    a[2].owner = Some(1);
    assert!(parse(&a, &b).is_empty());
}
#[test]
fn denominator_has_operator() {
    let (mut a, b) = two();
    a[6].text = "100×２".into();
    a[6].raw = a[6].text.clone();
    assert!(parse(&a, &b).is_empty());
}
#[test]
fn unknown_atom_inside() {
    let (mut a, b) = two();
    let mut x = a[0].clone();
    x.id = 999;
    x.raw = "?".into();
    x.text = "?".into();
    x.owner = None;
    a.push(x);
    assert!(parse(&a, &b).is_empty());
}
#[test]
fn incomplete_native_brackets() {
    let (mut a, b) = grouped();
    a.pop();
    assert!(parse(&a, &b).is_empty());
}
#[test]
fn bracket_group_not_owned_text() {
    let (mut a, b) = grouped();
    a.last_mut().unwrap().owner = Some(0);
    assert!(parse(&a, &b).is_empty());
}
#[test]
fn bracket_wrong_height() {
    let (mut a, b) = grouped();
    a.last_mut().unwrap().y1 += 5.;
    assert!(parse(&a, &b).is_empty());
}
#[test]
fn minus_without_native_group() {
    let (mut a, b) = grouped();
    a.truncate(11);
    assert!(parse(&a, &b).is_empty());
}
#[test]
fn bracket_group_wrong_span() {
    let (mut a, b) = grouped();
    a.last_mut().unwrap().x0 -= 24.;
    a.last_mut().unwrap().x1 -= 24.;
    assert!(parse(&a, &b).is_empty());
}
#[test]
fn textual_numerator_unchanged() {
    let (mut a, b) = two();
    a[0].text = "適用利率".into();
    a[0].raw = a[0].text.clone();
    assert!(parse(&a, &b).is_empty());
}

#[test]
fn operator_wrong_size() {
    let (mut a, b) = two();
    a[5].y0 -= 2.;
    a[5].y1 += 2.;
    assert!(parse(&a, &b).is_empty());
}
#[test]
fn native_bracket_overlaps_source_prefix() {
    let (mut a, b) = grouped();
    a[11].x0 = 189.;
    a[11].x1 = 197.;
    assert!(parse(&a, &b).is_empty());
}
