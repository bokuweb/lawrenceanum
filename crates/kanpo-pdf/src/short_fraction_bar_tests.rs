#[test]
fn old_json_defaults_and_empty_is_omitted() {
    let s = r#"{"vertical":[],"horizontal":[]}"#;
    let r: super::PageRules = serde_json::from_str(s).unwrap();
    assert!(r.short_fraction_bars.is_empty());
    assert_eq!(serde_json::to_string(&r).unwrap(), s);
}
#[test]
fn json_nonempty_roundtrip() {
    let s =
        r#"{"vertical":[],"horizontal":[],"short_fraction_bars":[{"y":2.0,"x0":3.0,"x1":11.0}]}"#;
    let r: super::PageRules = serde_json::from_str(s).unwrap();
    assert_eq!(r.short_fraction_bars.len(), 1);
    assert_eq!(
        serde_json::from_str::<super::PageRules>(&serde_json::to_string(&r).unwrap()).unwrap(),
        r
    );
}
fn one() -> crate::page::Page {
    crate::page::Page {
        width: 100.,
        height: 100.,
        rotate: 0,
        glyphs: vec![],
        fonts: vec![],
        paths: vec![crate::page::PathItem {
            subpaths: vec![crate::page::SubPath {
                points: vec![[10., 20.], [22., 20.]],
                closed: false,
                curved: false,
                rect: false,
            }],
            stroke: true,
            fill: false,
            line_width: 0.36,
            clip: None,
        }],
    }
}
#[test]
fn valid_short_is_separate() {
    let r = super::page_rules(&one());
    assert_eq!(r.short_fraction_bars.len(), 1);
    assert!(r.horizontal.is_empty());
    assert!(r.vertical.is_empty());
}
#[test]
fn nonstroked_rejected() {
    let mut p = one();
    p.paths[0].stroke = false;
    assert!(super::page_rules(&p).short_fraction_bars.is_empty());
}
#[test]
fn filled_rejected() {
    let mut p = one();
    p.paths[0].fill = true;
    assert!(super::page_rules(&p).short_fraction_bars.is_empty());
}
#[test]
fn curved_rejected() {
    let mut p = one();
    p.paths[0].subpaths[0].curved = true;
    assert!(super::page_rules(&p).short_fraction_bars.is_empty());
}
#[test]
fn closed_rejected() {
    let mut p = one();
    p.paths[0].subpaths[0].closed = true;
    assert!(super::page_rules(&p).short_fraction_bars.is_empty());
}
#[test]
fn thick_rejected() {
    let mut p = one();
    p.paths[0].line_width = 2.;
    assert!(super::page_rules(&p).short_fraction_bars.is_empty());
}
#[test]
fn bent_rejected() {
    let mut p = one();
    p.paths[0].subpaths[0].points = vec![[10., 20.], [15., 20.2], [22., 20.]];
    assert!(super::page_rules(&p).short_fraction_bars.is_empty());
}
#[test]
fn clipped_away_rejected() {
    let mut p = one();
    p.paths[0].clip = Some([0., 21., 100., 100.]);
    assert!(super::page_rules(&p).short_fraction_bars.is_empty());
}
#[test]
fn clip_changes_actual_short_extent() {
    let mut p = one();
    p.paths[0].clip = Some([15., 0., 20., 100.]);
    assert_eq!(
        super::page_rules(&p).short_fraction_bars,
        vec![super::HRule {
            y: 20.,
            x0: 15.,
            x1: 20.
        }]
    );
}
#[test]
fn long_never_duplicated() {
    let mut p = one();
    p.paths[0].subpaths[0].points[1][0] = 30.;
    let r = super::page_rules(&p);
    assert!(r.short_fraction_bars.is_empty());
    assert_eq!(r.horizontal.len(), 1);
}
