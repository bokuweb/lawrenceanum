use super::*;
use crate::page::{FontInfo, Glyph};
fn sample() -> (Page, Aj1PageSources, crate::bbox::Word) {
    let cid = 17518;
    let p = Page {
        width: 800.,
        height: 600.,
        rotate: 0,
        glyphs: vec![Glyph {
            text: char::from_u32(cid).unwrap().to_string(),
            code: cid,
            cid,
            font: 0,
            x: 100.,
            y: 100.,
            end_x: 108.,
            end_y: 100.,
            size: 8.,
            vertical: false,
            bbox: [100., 94., 108., 102.],
            invisible: false,
            spacing: 0.,
            kern: 0.,
            show: 1,
        }],
        paths: vec![],
        fonts: vec![FontInfo {
            name: "Source".into(),
            vertical: false,
            ordering: Some("Identity".into()),
            has_to_unicode: false,
        }],
    };
    let s = Aj1PageSources { ink_rects:vec![],
        original: p.clone(),
        resolved: vec![Some('嬴')],
    };
    let w = crate::bbox::words(&p).remove(0);
    (p, s, w)
}
#[test]
fn original_raw_stays_tagged_while_candidate_is_separate() {
    let (p, s, w) = sample();
    assert_eq!(
        word_candidate(&p, &w, s.matched(&p).unwrap()),
        Some("嬴".into())
    );
    assert_eq!(w.text, char::from_u32(0xF0000 + 17518).unwrap().to_string());
}
#[test]
fn entire_page_Glyph_or_font_changes_invalidate_evidence() {
    let (p, s, _) = sample();
    let mut q = p.clone();
    q.glyphs[0].x += 1.;
    assert!(s.matched(&q).is_none());
    let mut q = p.clone();
    q.fonts[0].name.push('x');
    assert!(s.matched(&q).is_none());
}
#[test]
fn missing_or_extra_source_metadata_refused() {
    let (p, mut s, w) = sample();
    s.resolved.push(None);
    assert!(s.matched(&p).is_none());
    assert!(word_candidate(&p, &w, &[]).is_none());
}
#[test]
fn duplicate_and_unknown_GlyphIDs_refused() {
    let (p, s, mut w) = sample();
    w.glyphs = vec![0, 0];
    w.text = w.text.repeat(2);
    assert!(word_candidate(&p, &w, &s.resolved).is_none());
    w.glyphs = vec![9];
    assert!(word_candidate(&p, &w, &s.resolved).is_none());
}
#[test]
fn partial_Word_unresolved_extra_Glyph_refuses_all() {
    let (mut p, _, mut w) = sample();
    let mut g = p.glyphs[0].clone();
    g.cid = 65535;
    g.code = g.cid;
    g.text = char::from_u32(g.cid).unwrap().to_string();
    g.x += 8.;
    g.end_x += 8.;
    p.glyphs.push(g);
    w.glyphs.push(1);
    w.text.push(char::from_u32(0xF0000 + 65535).unwrap());
    assert!(word_candidate(&p, &w, &[Some('嬴'), None]).is_none());
}
#[test]
fn ToUnicode_and_ordinary_raw_are_not_new_candidates() {
    let (mut p, s, mut w) = sample();
    p.fonts[0].has_to_unicode = true;
    assert!(word_candidate(&p, &w, &s.resolved).is_none());
    p.fonts[0].has_to_unicode = false;
    w.text = "嬴".into();
    assert!(word_candidate(&p, &w, &s.resolved).is_none());
}
#[test]
fn mismatching_standard_character_is_not_trusted() {
    let (p, _, w) = sample();
    assert!(word_candidate(&p, &w, &[Some('瀀')]).is_none());
}
#[test]
fn multi_character_Glyph_and_cardinality_mismatch_refused() {
    let (mut p, s, mut w) = sample();
    p.glyphs[0].text = "外字".into();
    assert!(word_candidate(&p, &w, &s.resolved).is_none());
    w.text.push('外');
    assert!(word_candidate(&p, &w, &s.resolved).is_none());
}
