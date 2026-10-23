
use super::*;
fn values(a: &str, b: &str) -> Vec<Col> {
    [a, b]
        .iter()
        .enumerate()
        .map(|(id, t)| Col {
            id,
            x: 100.,
            x1: 108.,
            y: 20. + id as f32 * 8.,
            y1: 28.2 + id as f32 * 8.,
            text: (*t).into(),
        })
        .collect()
}
#[test]
fn decimal_values_and_canonical_minus_are_not_a_vertical_label() {
    for text in ["12", "51", "78"] {
        for sign in ["-", "－", "−", "—", "―"] {
            for scale in [0.125, 1., 8.] {
                let mut cs = values(text, sign);
                for c in &mut cs {
                    c.x = c.x * scale + 13.;
                    c.x1 = c.x1 * scale + 13.;
                    c.y = c.y * scale + 17.;
                    c.y1 = c.y1 * scale + 17.;
                }
                let refs: Vec<_> = cs.iter().collect();
                assert!(
                    !stacked(&refs),
                    "numeric records must remain separate: {text} / {sign}, scale{scale}"
                );
            }
        }
    }
}
#[test]
fn genuine_vertical_label_is_still_a_label() {
    let mut cs = values("資産", "部");
    cs[0].y1 = 36.;
    cs[1].y = 38.;
    cs[1].y1 = 46.;
    let refs: Vec<_> = cs.iter().collect();
    assert!(stacked(&refs));
}
