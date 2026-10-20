use super::*;
type Raw = (f32, f32, f32, f32, String);
fn fixture() -> (
    Vec<Raw>,
    std::collections::HashSet<([u32; 4], String)>,
    std::collections::HashMap<[u32; 4], Vec<(String, [f32; 4])>>,
) {
    let mut raw = Vec::new();
    let mut h = std::collections::HashSet::new();
    let mut v = std::collections::HashMap::new();
    for (t, b) in [
        ("所在地中﹁", [68., 482., 116., 490.]),
        ("﹂を﹁", [148., 482., 172., 490.]),
        ("﹂に改める︒", [108., 492., 156., 500.]),
    ] {
        let r: Raw = (b[0] - 8., b[1] - 4., b[2] - 8., b[3] - 4., t.to_string());
        let k = [r.0.to_bits(), r.1.to_bits(), r.2.to_bits(), r.3.to_bits()];
        v.insert(k, vec![(t.to_string(), b)]);
        raw.push(r);
    }
    for (t, b) in [
        ("旧住所中央", [116.2, 482.8, 148.2, 491.]),
        ("新住所長島", [172.2, 482.8, 220.2, 491.]),
        ("丁目十番", [60., 492.8, 108., 501.]),
    ] {
        let r: Raw = (b[0], b[1], b[2], b[3], t.to_string());
        h.insert(([r.0.to_bits(), r.1.to_bits(), r.2.to_bits(), r.3.to_bits()], r.4.clone()));
        raw.push(r);
    }
    (raw, h, v)
}
fn kept(
    mut raw: Vec<Raw>,
    h: std::collections::HashSet<([u32; 4], String)>,
    mut v: std::collections::HashMap<[u32; 4], Vec<(String, [f32; 4])>>,
) -> usize {
    retain_inline_native_quotes(&mut raw, &h, &mut v);
    v.values().map(Vec::len).sum()
}
#[test]
fn a_proven_first_quote_can_close_its_second_quote_on_the_next_line() {
    let (r, h, v) = fixture();
    assert_eq!(kept(r, h, v), 3);
}
#[test]
fn an_unproven_first_quote_cannot_anchor_a_wrapped_second_quote() {
    let (r, mut h, v) = fixture();
    h.retain(|(_, t)| t != "旧住所中央");
    assert_eq!(kept(r, h, v), 0);
}
#[test]
fn the_next_line_must_use_an_explicit_horizontal_font() {
    let (r, mut h, v) = fixture();
    h.retain(|(_, t)| t != "丁目十番");
    assert_eq!(kept(r, h, v), 2);
}
#[test]
fn a_missing_first_line_quoted_fragment_does_not_enable_a_wrap() {
    let (r, mut h, v) = fixture();
    h.retain(|(_, t)| t != "新住所長島");
    assert_eq!(kept(r, h, v), 2);
}
#[test]
fn a_farther_closing_run_does_not_attach_across_multiple_lines() {
    let (r, h, mut v) = fixture();
    for vals in v.values_mut() {
        for (t, b) in vals {
            if t == "﹂に改める︒" {
                b[1] += 8.;
                b[3] += 8.;
            }
        }
    }
    assert_eq!(kept(r, h, v), 2);
}
#[test]
fn a_small_closing_run_does_not_complete_a_prose_quote() {
    let (r, h, mut v) = fixture();
    for vals in v.values_mut() {
        for (t, b) in vals {
            if t == "﹂に改める︒" {
                b[3] -= 3.;
            }
        }
    }
    assert_eq!(kept(r, h, v), 2);
}
#[test]
fn an_unrelated_indented_next_line_is_not_a_continuation() {
    let (mut r, mut h, v) = fixture();
    h.retain(|(_, t)| t != "丁目十番");
    let x = r.iter_mut().find(|x| x.4 == "丁目十番").unwrap();
    x.0 += 24.;
    h.insert(([x.0.to_bits(), x.1.to_bits(), x.2.to_bits(), x.3.to_bits()], x.4.clone()));
    assert_eq!(kept(r, h, v), 2);
}
#[test]
fn a_next_line_without_prose_in_the_closing_run_is_not_joined() {
    let (r, h, mut v) = fixture();
    for vals in v.values_mut() {
        for (t, _) in vals {
            if t == "﹂に改める︒" {
                *t = "﹂".into();
            }
        }
    }
    assert_eq!(kept(r, h, v), 2);
}
