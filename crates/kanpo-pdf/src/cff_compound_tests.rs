use super::tests::font;
use super::*;
fn number(n: i16) -> Vec<u8> {
    let mut b = vec![28];
    b.extend(n.to_be_bytes());
    b
}
fn paths(paths: &[Vec<[i16; 2]>]) -> Vec<u8> {
    let mut b = vec![];
    let mut p = [0, 0];
    for path in paths {
        for (i, &q) in path.iter().enumerate() {
            b.extend(number(q[0] - p[0]));
            b.extend(number(q[1] - p[1]));
            b.push(if i == 0 { 21 } else { 5 });
            p = q;
        }
    }
    b.push(14);
    b
}
fn box_path(x: i16, y: i16, w: i16, h: i16) -> Vec<[i16; 2]> {
    vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h]]
}
fn make(cid: u16, p: Vec<u8>) -> Vec<u8> {
    font(cid, p, "Arbitrary", "Other", "Random", &[], &[])
}
#[test]
fn nested_opposite_rectangles_keep_empty_interior_and_exact_painted_border() {
    for cid in [73,43021,65534] {for shift in [-4096,0,4096] {for swap in [false,true] {for reverse in [false,true] {
        let mut a=box_path(shift,shift,1000,800);
        let mut b=box_path(shift+30,shift+20,920,720);b.reverse();
        if reverse {a.reverse();b.reverse();}
        let contours=if swap{vec![b,a]}else{vec![a,b]};
        let data=make(cid,paths(&contours));
        let frame=rectangle_frames(&data).unwrap()[&cid];
        assert_eq!(frame.outer,[shift as f64,shift as f64,(shift+1000)as f64,(shift+800)as f64]);
        assert_eq!(frame.inner,[(shift+30)as f64,(shift+20)as f64,(shift+950)as f64,(shift+740)as f64]);
        let border=frame.painted_rectangles();
        let area=|r:[f64;4]|(r[2]-r[0])*(r[3]-r[1]);
        assert_eq!(border.into_iter().map(area).sum::<f64>(),area(frame.outer)-area(frame.inner));
        for (i,a) in border.iter().enumerate(){for b in &border[i+1..] {
            assert!(!(a[0]<b[2]&&b[0]<a[2]&&a[1]<b[3]&&b[1]<a[3]));
        }}
        assert!(!rectangles(&data).unwrap().contains_key(&cid));
        assert!(!separate_rectangles(&data).unwrap().contains_key(&cid));
    }}}}
}

#[test]
fn unrelated_rectangles_touching_holes_and_unsupported_contours_do_not_prove_a_frame() {
    for other in [box_path(0,20,920,720),box_path(1000,0,100,100),box_path(500,400,1000,800),box_path(100,100,800,600)] {
        // Same orientation is deliberately not a strict hollow-frame proof.
        let data=make(43021,paths(&[box_path(0,0,1000,800),other]));
        assert!(!rectangle_frames(&data).unwrap().contains_key(&43021));
    }
    let mut hole=box_path(0,20,920,720);hole.reverse();
    let data=make(73,paths(&[box_path(0,0,1000,800),hole]));
    assert!(!rectangle_frames(&data).unwrap().contains_key(&73));
    let mut hole=box_path(30,20,920,720);hole.reverse();
    let data=make(73,paths(&[box_path(0,0,1000,800),hole,box_path(100,100,20,20)]));
    assert!(!rectangle_frames(&data).unwrap().contains_key(&73));
}
#[test]
fn original_two_contour_paint_has_an_independently_traced_opaque_envelope() {
    let data = include_bytes!("../tests/data/native-compound-scene/original.cff");
    assert_eq!(
        compound_envelopes(data).unwrap().get(&23115),
        Some(&[408., -122., 590., 882.])
    );
    assert!(
        !rectangles(data).unwrap().contains_key(&23115),
        "two contours must not become one continuous ruling rectangle"
    );
}
#[test]
fn counts_font_names_and_cids_do_not_supply_compound_geometry() {
    for cid in [73, 43021, 65534] {
        for n in [2, 3, 17, 65] {
            let p = (0..n)
                .map(|i| box_path(i * 128, -32, 64, 96))
                .collect::<Vec<_>>();
            let data = make(cid, paths(&p));
            assert_eq!(
                compound_envelopes(&data).unwrap().get(&cid),
                Some(&[0., -32., ((n - 1) * 128 + 64) as f64, 64.])
            );
            assert!(!rectangles(&data).unwrap().contains_key(&cid));
            let expected=(0..n).map(|i|[(i*128)as f64,-32.,(i*128+64)as f64,64.]).collect::<Vec<_>>();
            assert_eq!(separate_rectangles(&data).unwrap().get(&cid),Some(&expected));
        }
    }
}
#[test]
fn opaque_bounds_do_not_claim_a_hole_or_overlap_is_empty() {
    for (mut a, mut b) in [
        (box_path(0, 0, 100, 100), box_path(20, 20, 60, 60)),
        (box_path(0, 0, 100, 100), box_path(50, 50, 100, 100)),
    ] {
        b.reverse();
        let hi = if b.iter().any(|p| p[0] == 150) {
            150.
        } else {
            100.
        };
        let data = make(51, paths(&[a.clone(), b]));
        assert_eq!(
            compound_envelopes(&data).unwrap().get(&51),
            Some(&[0., 0., hi, hi])
        );
        assert!(!separate_rectangles(&data).unwrap().contains_key(&51));
        a.reverse();
    }
}
#[test]
fn implicit_path_closing_keeps_the_previous_current_point_for_the_next_move() {
    let data = make(
        82,
        paths(&[box_path(100, 200, 40, 60), box_path(-200, 500, 40, 60)]),
    );
    assert_eq!(
        compound_envelopes(&data).unwrap().get(&82),
        Some(&[-200., 200., 140., 560.])
    );
}
#[test]
fn one_contour_and_a_nonrectangular_contour_do_not_get_compound_authority() {
    for p in [
        vec![box_path(0, 0, 100, 100)],
        vec![
            box_path(0, 0, 100, 100),
            vec![[200, 200], [250, 200], [225, 250]],
        ],
    ] {
        assert!(!compound_envelopes(&make(66, paths(&p)))
            .unwrap()
            .contains_key(&66));
    }
}

#[test]
fn touching_contours_never_claim_separate_rectangles() {
    for other in [box_path(100,0,100,100),box_path(100,100,100,100),box_path(20,20,40,40)] {
        let data=make(43021,paths(&[box_path(0,0,100,100),other]));
        assert!(compound_envelopes(&data).unwrap().contains_key(&43021));
        assert!(!separate_rectangles(&data).unwrap().contains_key(&43021));
    }
}
#[test]
fn unsupported_operators_and_trailing_material_keep_unknown_paint() {
    let good = paths(&[box_path(0, 0, 100, 100), box_path(200, 200, 100, 100)]);
    for op in [1, 10, 19, 29, 30] {
        let mut p = good.clone();
        p.insert(p.len() - 1, op);
        assert!(!compound_envelopes(&make(66, p)).unwrap().contains_key(&66));
    }
    for end in [0, 1, good.len() - 1] {
        assert!(compound_envelopes(&make(66, good[..end].to_vec()))
            .is_none_or(|m| !m.contains_key(&66)));
    }
    let mut p = good;
    p.push(14);
    assert!(!compound_envelopes(&make(66, p)).unwrap().contains_key(&66));
}
#[test]
fn only_the_first_moveto_can_have_an_optional_width() {
    let mut first = vec![28, 0, 80];
    first.extend(paths(&[
        box_path(0, 0, 100, 100),
        box_path(200, 200, 100, 100),
    ]));
    assert_eq!(
        compound_envelopes(&make(44, first)).unwrap().get(&44),
        Some(&[0., 0., 300., 300.])
    );
    let mut p = paths(&[box_path(0, 0, 100, 100)]);
    p.pop();
    p.extend(number(80));
    p.extend(number(200));
    p.extend(number(100));
    p.push(21);
    p.extend(number(0));
    p.extend(number(100));
    p.push(5);
    p.push(14);
    assert!(!compound_envelopes(&make(44, p)).unwrap().contains_key(&44));
}
