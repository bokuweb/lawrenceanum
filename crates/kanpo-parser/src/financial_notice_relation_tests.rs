use super::*;
use crate::notice_sources::{NoticeScope, SourceSpan, StatementReference};
fn span(a: usize, b: usize) -> Vec<SourceSpan> {
    vec![SourceSpan {
        page: 0,
        region: 0,
        start_line: a,
        end_line: b,
    }]
}
fn expected(a: usize, b: usize, c: usize, d: usize) -> NoticeScope {
    NoticeScope {
        source: span(a, b),
        financial_statements: vec![StatementReference { source: span(c, d) }],
    }
}
fn graph(p: &crate::trace::PageTrace, r: &crate::lines::PageRules) -> Vec<NoticeScope> {
    super::super::source_scopes::notice_scopes(0, p, r, &[])
}
fn replace(p: &mut crate::trace::PageTrace, from: &str, to: &str) {
    for w in &mut p.words {
        w.text = w.text.replace(from, to);
        w.raw = w.raw.replace(from, to);
    }
    for l in p.regions.iter_mut().flat_map(|r| &mut r.lines) {
        l.text = l.text.replace(from, to);
    }
}
fn shared() -> (crate::trace::PageTrace, crate::lines::PageRules) {
    let (mut p, r) = side_by_side_publications();
    replace(&mut p, "株式会社あお", "株式会社みどり");
    let removed = p.regions[0].lines.drain(17..24).collect::<Vec<_>>();
    let removed = removed
        .into_iter()
        .flat_map(|l| {
            l.table
                .map(|t| {
                    t.cells
                        .into_iter()
                        .flat_map(|c| c.words)
                        .collect::<Vec<_>>()
                })
                .unwrap_or(l.words)
        })
        .collect::<std::collections::HashSet<_>>();
    let mut map = std::collections::BTreeMap::new();
    for old in 0..p.words.len() {
        if !removed.contains(&old) {
            map.insert(old, map.len());
        }
    }
    fn retain<T>(v: &mut Vec<T>, removed: &std::collections::HashSet<usize>) {
        let mut i = 0;
        v.retain(|_| {
            let keep = !removed.contains(&i);
            i += 1;
            keep
        });
    }
    retain(&mut p.words, &removed);
    retain(&mut p.native_styles, &removed);
    retain(&mut p.native_content_styles, &removed);
    retain(&mut p.native_bounds, &removed);
    retain(&mut p.native_axes, &removed);
    retain(&mut p.native_rulings, &removed);
    for w in &mut p.words {
        w.id = map[&w.id];
    }
    for l in &mut p.regions[0].lines {
        for id in &mut l.words {
            *id = map[id];
        }
        if let Some(t) = &mut l.table {
            for id in t.cells.iter_mut().flat_map(|c| &mut c.words) {
                *id = map[id];
            }
        }
    }
    p.tables = p.regions[0]
        .lines
        .iter()
        .filter_map(|l| l.table.clone())
        .collect();
    (p, r)
}
#[test]
fn two_unique_notices_reference_their_own_whole_statements() {
    let (p, r) = stacked_publications(false);
    assert_eq!(
        graph(&p, &r),
        vec![expected(0, 5, 5, 12), expected(12, 17, 17, 24)]
    );
}
#[test]
fn equal_publishers_do_not_join_separately_dated_statements() {
    let (p, r) = stacked_publications(true);
    assert_eq!(
        graph(&p, &r),
        vec![expected(0, 5, 5, 12), expected(12, 17, 17, 24)]
    );
}
#[test]
fn two_procedures_can_reference_one_source_statement() {
    let (p, r) = shared();
    assert_eq!(p.tables.len(), 1);
    assert_eq!(
        graph(&p, &r),
        vec![expected(0, 5, 5, 12), expected(12, 17, 5, 12)]
    );
}
#[test]
fn equal_publishers_in_an_ambiguous_shared_band_do_not_make_all_to_all_links() {
    let (mut p, r) = side_by_side_publications();
    replace(&mut p, "株式会社あお", "株式会社みどり");
    assert!(graph(&p, &r).is_empty());
}
#[test]
fn unknown_flow_cannot_supply_notice_ownership() {
    let (mut p, r) = stacked_publications(false);
    p.native_axes[0] = NativeAxis::Unknown;
    assert!(graph(&p, &r).is_empty());
}
#[test]
fn incomplete_source_styles_cannot_supply_notice_ownership() {
    let (mut p, r) = stacked_publications(false);
    p.native_content_styles[0] = ContentStyles::Unknown;
    assert!(graph(&p, &r).is_empty());
}
#[test]
fn an_external_open_quote_prevents_new_notice_authority() {
    let (p, r) = stacked_publications(false);
    let mut prior = p.clone();
    prior.text = "別の手続による引用「".into();
    assert!(super::super::source_scopes::notice_scopes(1, &p, &r, &[prior]).is_empty());
}
#[test]
fn local_disclosure_vocabulary_is_document_grammar() {
    for direction in ["左記", "下記", "右記", "上記", "次", "以下"] {
        let (mut p, r) = shared();
        replace(&mut p, "左記", direction);
        assert_eq!(graph(&p, &r).len(), 2, "{direction}");
    }
}
#[test]
fn quoted_disclosure_cannot_claim_a_local_statement() {
    let (mut p, r) = shared();
    replace(
        &mut p,
        "最終貸借対照表の要旨は左記のとおりです",
        "「最終貸借対照表の要旨は左記のとおりです」",
    );
    assert!(graph(&p, &r).is_empty());
}
#[test]
fn negated_local_disclosure_is_not_a_positive_reference() {
    let (mut p, r) = shared();
    replace(&mut p, "左記のとおりです", "左記のとおりではありません");
    assert!(graph(&p, &r).is_empty());
}
#[test]
fn incidental_reference_after_an_unrelated_subject_is_not_disclosure() {
    let (mut p, r) = shared();
    replace(
        &mut p,
        "最終貸借対照表の要旨は左記のとおりです",
        "説明例では最終貸借対照表の要旨は左記のとおりです",
    );
    assert!(graph(&p, &r).is_empty());
}
#[test]
fn conflicting_disclosure_is_not_silently_resolved() {
    let (mut p, r) = shared();
    replace(
        &mut p,
        "最終貸借対照表の要旨は左記のとおりです",
        "最終貸借対照表の要旨は左記のとおりです。最終貸借対照表の要旨は掲載官報のとおりです",
    );
    assert!(graph(&p, &r).is_empty());
}
#[test]
fn disclosure_requires_full_issuer_identity() {
    let (mut p, r) = shared();
    for w in &mut p.words {
        if p.native_axes[w.id] == NativeAxis::Down {
            w.text = w.text.replace("株式会社みどり", "株式会社みどり商事");
            w.raw = w.text.clone();
        }
    }
    assert!(graph(&p, &r).is_empty());
}
#[test]
fn publisher_values_have_no_character_cap() {
    for name in ["一".into(), format!("株式会社{}", "任意の名称".repeat(100))] {
        let (mut p, r) = shared();
        replace(&mut p, "株式会社みどり", &name);
        assert_eq!(graph(&p, &r).len(), 2);
    }
}

#[test]
fn source_region_boundaries_preserve_whole_notice_and_table_membership() {
    let (mut p, r) = stacked_publications(false);
    let second = p.regions[0].lines.split_off(12);
    let mut region = p.regions[0].clone();
    region.lines = second;
    region.part += 1;
    p.regions.push(region);
    let mut want = vec![expected(0, 5, 5, 12), expected(0, 5, 5, 12)];
    want[1].source[0].region = 1;
    want[1].financial_statements[0].source[0].region = 1;
    assert_eq!(graph(&p, &r), want);
}
#[test]
fn source_frame_path_order_and_duplicate_frames_do_not_duplicate_notices() {
    let (p, mut r) = stacked_publications(false);
    let want = graph(&p, &r);
    r.enclosures.extend(r.enclosures.clone());
    r.enclosures.reverse();
    assert_eq!(graph(&p, &r), want);
    assert_eq!(want.len(), 2);
}
#[test]
fn straddling_source_frames_cannot_authorize_partial_notices() {
    let (p, mut r) = stacked_publications(false);
    r.enclosures[0][2] = 363.;
    assert!(graph(&p, &r).is_empty());
}
#[test]
fn non_body_regions_cannot_authorize_signed_notice_scopes() {
    let (mut p, r) = stacked_publications(false);
    p.regions[0].role = crate::trace::Role::Outside;
    assert!(graph(&p, &r).is_empty());
}
#[test]
fn scope_page_indices_are_explicit_in_every_reference() {
    let (p, r) = stacked_publications(false);
    let mut want = graph(&p, &r);
    for n in &mut want {
        for span in n.source.iter_mut().chain(
            n.financial_statements
                .iter_mut()
                .flat_map(|s| &mut s.source),
        ) {
            span.page = 37;
        }
    }
    assert_eq!(
        super::super::source_scopes::notice_scopes(37, &p, &r, &[]),
        want
    );
    assert_eq!(want.len(), 2);
}
#[test]
fn typed_source_geometry_and_font_values_do_not_select_the_result() {
    let (p, r) = shared();
    let want = graph(&p, &r);
    assert_eq!(want.len(), 2);
    for (sx, sy, dx, dy) in [
        (0.125, 64., -1000., 3000.),
        (64., 0.125, 200., -500.),
        (1., 1., 0., 0.),
    ] {
        let mut p = p.clone();
        let mut r = r.clone();
        let tr = |b: &mut [f32; 4]| {
            b[0] = b[0] * sx + dx;
            b[2] = b[2] * sx + dx;
            b[1] = b[1] * sy + dy;
            b[3] = b[3] * sy + dy;
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
        for styles in p.native_styles.iter_mut().flatten() {
            *styles = styles
                .iter()
                .map(|&(font, size)| (font + 500, size))
                .collect();
        }
        for content in &mut p.native_content_styles {
            if let ContentStyles::Glyphs(styles) = content {
                *styles = styles
                    .iter()
                    .map(|&(font, size)| (font + 500, size))
                    .collect();
            }
        }
        for t in p
            .regions
            .iter_mut()
            .flat_map(|r| &mut r.lines)
            .filter_map(|l| l.table.as_mut())
        {
            let mut b = [t.x0, t.y0, t.x1, t.y1];
            tr(&mut b);
            [t.x0, t.y0, t.x1, t.y1] = b;
            for c in &mut t.cells {
                let mut b = [c.x0, c.y0, c.x1, c.y1];
                tr(&mut b);
                [c.x0, c.y0, c.x1, c.y1] = b;
            }
        }
        for b in &mut r.enclosures {
            tr(b);
        }
        assert_eq!(graph(&p, &r), want, "sx={sx},sy={sy}");
    }
}
