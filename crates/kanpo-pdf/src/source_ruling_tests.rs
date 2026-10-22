use super::*;
use crate::{FontInfo, Glyph};
fn fixture(n: usize) -> (Page, Vec<Option<Rect>>) {
    let glyphs = (0..n)
        .map(|i| {
            let x = i as f64 * 8.;
            Glyph {
                text: "\u{e005}".into(),
                code: 53,
                cid: 81,
                font: 0,
                x,
                y: 100.,
                end_x: x + 8.,
                end_y: 100.,
                size: 8.,
                vertical: false,
                bbox: [x, 94., x + 8., 102.],
                invisible: false,
                spacing: 0.,
                kern: 0.,
                show: 1,
            }
        })
        .collect();
    let p = Page {
        width: 800.,
        height: 600.,
        rotate: 0,
        glyphs,
        fonts: vec![FontInfo {
            name: "Arbitrary".into(),
            vertical: false,
            ordering: Some("Any".into()),
            has_to_unicode: true,
        }],
        paths: vec![],
    };
    let r = (0..n)
        .map(|i| {
            Some([
                i as f64 * 8. - 0.015625,
                96.,
                i as f64 * 8. + 8.015625,
                96.5,
            ])
        })
        .collect();
    (p, r)
}
fn word(p: &Page) -> Word {
    let words = crate::bbox::words(p);
    assert_eq!(words.len(), 1);
    words.into_iter().next().unwrap()
}
#[test]
fn a_joined_outline_chain_has_no_character_font_name_or_CID_identity_rule() {
    let (p, r) = fixture(3);
    let original = serde_json::to_string(&p).unwrap();
    let w = word(&p);
    assert_eq!(
        word_ruling(&p, &w, &r),
        Some(("horizontal", [-0.015625, 96., 24.015625, 96.5]))
    );
    assert_eq!(serde_json::to_string(&p).unwrap(), original);
    for (text, name, cid) in [("任意", "OtherFamily", 7000), ("━", "NoRulerName", 3)] {
        let mut x = p.clone();
        x.fonts[0].name = name.into();
        for g in &mut x.glyphs {
            g.text = text.into();
            g.cid = cid;
            g.code = cid;
        }
        assert_eq!(word_ruling(&x, &word(&x), &r), word_ruling(&p, &w, &r));
    }
}
#[test]
fn disconnected_outlines_and_single_glyphs_are_not_rule_chains() {
    let (p, mut r) = fixture(3);
    for (i, b) in r.iter_mut().enumerate() {
        *b = Some([i as f64 * 8. + 1., 96., i as f64 * 8. + 7., 96.5]);
    }
    assert!(word_ruling(&p, &word(&p), &r).is_none());
    let (p, r) = fixture(1);
    assert!(word_ruling(&p, &word(&p), &r).is_none());
}
#[test]
fn every_original_occurrence_and_complete_finite_source_rectangle_is_required() {
    let (p, r) = fixture(3);
    let w = word(&p);
    for i in 0..3 {
        let mut x = r.clone();
        x[i] = None;
        assert!(word_ruling(&p, &w, &x).is_none());
    }
    assert!(word_ruling(&p, &w, &r[..2]).is_none());
    for b in [[0., f64::NAN, 8., 1.], [9., 96., 8., 96.5]] {
        let mut x = r.clone();
        x[1] = Some(b);
        assert!(word_ruling(&p, &w, &x).is_none());
    }
    let mut x = p.clone();
    x.glyphs[1].invisible = true;
    assert!(word_ruling(&x, &w, &r).is_none());
    let mut x = p.clone();
    x.glyphs[1].end_y += 1.;
    assert!(word_ruling(&x, &w, &r).is_none());
    let mut x = w.clone();
    x.glyphs = vec![0, 0, 1];
    assert!(word_ruling(&p, &x, &r).is_none());
    let mut x = w.clone();
    x.text.push('別');
    assert!(word_ruling(&p, &x, &r).is_none());
}
#[test]
fn touching_is_connected_but_even_a_small_gap_or_overprint_is_not() {
    let (p, mut r) = fixture(3);
    r[1].as_mut().unwrap()[0] = r[0].unwrap()[2];
    assert!(word_ruling(&p, &word(&p), &r).is_some());
    r[1].as_mut().unwrap()[0] += 2f64.powi(-20);
    assert!(word_ruling(&p, &word(&p), &r).is_none());
    let (p, mut r) = fixture(3);
    r[1] = r[0];
    assert!(word_ruling(&p, &word(&p), &r).is_none());
}
#[test]
fn independent_scaling_translation_and_axis_reversal_preserve_source_geometry() {
    let (p, r) = fixture(3);
    let original_word = word(&p);
    for (sx, sy, swap) in [
        (0.125, 64., false),
        (64., 0.125, false),
        (-8., 4., false),
        (2., -4., true),
    ] {
        let mut x = p.clone();
        let point = |x: f64, y: f64| {
            let q = [sx * x + 128., sy * y + 2048.];
            if swap {
                [q[1], q[0]]
            } else {
                q
            }
        };
        let rect = |b: Rect| {
            let a = point(b[0], b[1]);
            let z = point(b[2], b[3]);
            [
                a[0].min(z[0]),
                a[1].min(z[1]),
                a[0].max(z[0]),
                a[1].max(z[1]),
            ]
        };
        for g in &mut x.glyphs {
            let a = point(g.x, g.y);
            let b = point(g.end_x, g.end_y);
            g.x = a[0];
            g.y = a[1];
            g.end_x = b[0];
            g.end_y = b[1];
            g.bbox = rect(g.bbox);
            g.size *= sy.abs();
        }
        let rs = r.iter().map(|b| b.map(rect)).collect::<Vec<_>>(); // Preserve the declared native Word occurrences: legacy word splitting has its own aspect-dependent duplicate heuristics.
        let mut w = original_word.clone();
        let b = rect([w.x_min, w.y_min, w.x_max, w.y_max]);
        w.x_min = b[0];
        w.y_min = b[1];
        w.x_max = b[2];
        w.y_max = b[3];
        assert_eq!(w.glyphs, vec![0, 1, 2]);
        let got = word_ruling(&x, &w, &rs).unwrap();
        assert_eq!(got.0, if swap { "vertical" } else { "horizontal" });
        assert_eq!(got.1, rect([-0.015625, 96., 24.015625, 96.5]));
    }
}
