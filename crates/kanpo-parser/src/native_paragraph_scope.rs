//! Native baseline/capacity evidence only; paragraph grammar and record
//! ownership are checked separately by the table reconciler.
use crate::{native_word_flow::NativeFlow, trace::NativeBounds, vertical::Col};
pub(crate) fn scope(
    cols: &[Col],
    flows: &[Option<NativeFlow>],
    bounds: &[NativeBounds],
    rect: [f32; 4],
) -> Option<Vec<Vec<usize>>> {
    if flows.len() != bounds.len()
        || !rect.iter().all(|n| n.is_finite())
        || rect[0] >= rect[2]
        || rect[1] >= rect[3]
    {
        return None;
    }
    struct L {
        baseline: f64,
        start: f64,
        end: f64,
        leading: f64,
        trailing: f64,
        prefix: f64,
        y0: f32,
        y1: f32,
        ids: Vec<usize>,
    }
    let mut lines: Vec<L> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for c in cols {
        if !seen.insert(c.id) {
            return None;
        }
        let f = (*flows.get(c.id)?)?;
        let NativeBounds::Glyphs(b) = *bounds.get(c.id)? else {
            return None;
        };
        if !f.belongs(b)
            || b[0] < rect[0]
            || b[1] < rect[1]
            || rect[2] < b[2]
            || rect[3] < b[3]
            || f.start < f64::from(rect[0])
            || f.end > f64::from(rect[2])
        {
            return None;
        }
        match lines.last_mut() {
            Some(l) if l.baseline == f.baseline => {
                if f.start < l.end {
                    return None;
                }
                l.end = f.end;
                l.trailing = f.trailing;
                l.y0 = l.y0.min(b[1]);
                l.y1 = l.y1.max(b[3]);
                l.ids.push(c.id);
            }
            Some(l) if f.baseline <= l.baseline => return None,
            _ => lines.push(L {
                baseline: f.baseline,
                start: f.start,
                end: f.end,
                leading: f.leading,
                trailing: f.trailing,
                prefix: f.prefix,
                y0: b[1],
                y1: b[3],
                ids: vec![c.id],
            }),
        }
    }
    if lines.len() < 2 {
        return None;
    }
    let margin = lines[1].start;
    if lines[0].start < margin || lines[1..].iter().any(|l| l.start != margin) {
        return None;
    }
    for pair in lines.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if !(a.baseline < f64::from(b.y0) && f64::from(a.y1) < b.baseline)
            || !(a.end + a.trailing + b.prefix + b.leading).is_finite()
            || a.end + a.trailing + b.prefix + b.leading <= f64::from(rect[2])
        {
            return None;
        }
    }
    Some(lines.into_iter().map(|l| l.ids).collect())
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_word_flow::NativeFlow;
    struct F {
        cols: Vec<Col>,
        flows: Vec<Option<NativeFlow>>,
        bounds: Vec<NativeBounds>,
        rect: [f32; 4],
    }
    fn fixture(n: usize) -> F {
        let mut f = F {
            cols: vec![],
            flows: vec![],
            bounds: vec![],
            rect: [0., 0., 80., (40 + 12 * n) as f32],
        };
        for i in 0..n {
            let y = 20. + 12. * i as f64;
            let end = if i + 1 == n { 29. } else { 77. };
            let b = [5., (y - 8.) as f32, end as f32, (y + 3.) as f32];
            f.cols.push(Col {
                id: i,
                x: b[0],
                y: b[1],
                x1: b[2],
                y1: b[3],
                text: "任意の項目名".into(),
            });
            f.bounds.push(NativeBounds::Glyphs(b));
            f.flows.push(Some(NativeFlow{
                start: 5.,
                baseline: y,
                end,
                leading: 8., trailing:0., prefix:0. }));
        }
        f
    }
    fn prove(f: &F) -> Option<Vec<Vec<usize>>> {
        scope(&f.cols, &f.flows, &f.bounds, f.rect)
    }
    fn yes(f: F) {
        assert_eq!(prove(&f), Some(f.cols.iter().map(|c| vec![c.id]).collect()));
    }
    #[test]
    fn native_capacity_wraps_have_no_character_or_line_count_ceiling() {
        for n in [2, 3, 7, 29] {
            yes(fixture(n));
        }
    }
    #[test]
    fn body_vocabulary_does_not_supply_geometry() {
        for text in ["x", "任意の別文言", "unknown ordinary words"] {
            let mut f = fixture(3);
            for c in &mut f.cols {
                c.text = text.into();
            }
            yes(f);
        }
    }
    #[test]
    fn the_true_next_first_advance_decides_exact_capacity_not_average_word_width() {
        let mut f = fixture(2);
        f.cols[0].x1 = 75.;
        f.bounds[0] = NativeBounds::Glyphs([5., 12., 75., 23.]);
        f.flows[0].as_mut().unwrap().end = 75.;
        f.flows[1].as_mut().unwrap().leading = 5.;
        assert_eq!(prove(&f), None);
        f.flows[1].as_mut().unwrap().leading = 6.;
        yes(f);
    }
    #[test]
    fn unknown_missing_or_foreign_native_flow_is_not_inferred() {
        for k in 0..4 {
            let mut f = fixture(3);
            match k {
                0 => f.flows[1] = None,
                1 => {
                    f.flows.pop();
                }
                2 => f.flows[1].as_mut().unwrap().start += 1.,
                _ => f.flows[1].as_mut().unwrap().baseline = 1000.,
            };
            assert_eq!(prove(&f), None);
        }
    }
    #[test]
    fn unknown_nonfinite_or_crossing_original_bounds_are_not_a_paragraph() {
        for k in 0..4 {
            let mut f = fixture(3);
            f.bounds[1] = match k {
                0 => NativeBounds::Unknown,
                1 => NativeBounds::Glyphs([f32::NAN, 24., 77., 35.]),
                2 => NativeBounds::Glyphs([5., 24., 81., 35.]),
                _ => NativeBounds::Glyphs([5., 24., 5., 35.]),
            };
            assert_eq!(prove(&f), None);
        }
    }
    #[test]
    fn duplicate_reversed_or_missing_original_occurrences_are_not_reordered() {
        for k in 0..3 {
            let mut f = fixture(3);
            match k {
                0 => f.cols[1].id = 0,
                1 => f.cols.reverse(),
                _ => f.cols[1].id = 100,
            };
            assert_eq!(prove(&f), None);
        }
    }
    #[test]
    fn a_tiny_distinct_baseline_inside_the_same_font_box_is_not_a_new_printed_line() {
        let mut f = fixture(2);
        f.bounds[1] = NativeBounds::Glyphs([5., 12., 29., 23.]);
        f.cols[1].y = 12.;
        f.cols[1].y1 = 23.;
        f.flows[1].as_mut().unwrap().baseline = 20.0000000001;
        assert_eq!(prove(&f), None);
    }
    #[test]
    fn independent_row_starts_and_hanging_item_indents_do_not_form_one_wrap() {
        for k in 1..3 {
            let mut f = fixture(3);
            f.cols[k].x = 13.;
            if let NativeBounds::Glyphs(ref mut b) = f.bounds[k] {
                b[0] = 13.;
            }
            f.flows[k].as_mut().unwrap().start = 13.;
            assert_eq!(prove(&f), None);
        }
    }
    #[test]
    fn one_initial_paragraph_indent_uses_the_later_source_line_margin() {
        let mut f = fixture(3);
        f.cols[0].x = 13.;
        if let NativeBounds::Glyphs(ref mut b) = f.bounds[0] {
            b[0] = 13.;
        }
        f.flows[0].as_mut().unwrap().start = 13.;
        yes(f);
    }
    #[test]
    fn exact_original_baselines_group_split_tokens_without_guessing_a_line() {
        let mut f = fixture(2);
        f.cols[0].x1 = 35.;
        f.bounds[0] = NativeBounds::Glyphs([5., 12., 35., 23.]);
        f.flows[0].as_mut().unwrap().end = 35.;
        let c = Col {
            id: 2,
            x: 35.,
            y: 12.,
            x1: 77.,
            y1: 23.,
            text: "続き".into(),
        };
        f.cols.insert(1, c);
        f.bounds.push(NativeBounds::Glyphs([35., 12., 77., 23.]));
        f.flows.push(Some(NativeFlow{
            start: 35.,
            baseline: 20.,
            end: 77.,
            leading: 8., trailing:0., prefix:0. }));
        assert_eq!(prove(&f), Some(vec![vec![0, 2], vec![1]]));
        f.flows[2].as_mut().unwrap().baseline += 1e-10;
        assert_eq!(prove(&f), None);
    }
    #[test]
    fn an_atomic_source_line_is_not_a_wrapped_paragraph() {
        let f = fixture(1);
        assert_eq!(prove(&f), None);
    }
    #[test]
    fn independently_scaled_and_translated_source_geometry_preserves_capacity() {
        for (sx, sy, dx, dy) in [
            (1., 1., 0., 0.),
            (0.125, 64., 32., -64.),
            (64., 0.125, -32., 64.),
        ] {
            let mut f = fixture(7);
            for i in 0..f.cols.len() {
                let Some(n) = f.flows[i].as_mut() else {
                    unreachable!()
                };
                n.start = n.start * sx + dx;
                n.end = n.end * sx + dx;
                n.leading *= sx;
                n.trailing *= sx;
                n.prefix *= sx;
                n.baseline = n.baseline * sy + dy;
                let NativeBounds::Glyphs(ref mut b) = f.bounds[i] else {
                    unreachable!()
                };
                *b = [
                    (f64::from(b[0]) * sx + dx) as f32,
                    (f64::from(b[1]) * sy + dy) as f32,
                    (f64::from(b[2]) * sx + dx) as f32,
                    (f64::from(b[3]) * sy + dy) as f32,
                ];
                let c = &mut f.cols[i];
                c.x = b[0];
                c.y = b[1];
                c.x1 = b[2];
                c.y1 = b[3];
            }
            f.rect = [
                (f64::from(f.rect[0]) * sx + dx) as f32,
                (f64::from(f.rect[1]) * sy + dy) as f32,
                (f64::from(f.rect[2]) * sx + dx) as f32,
                (f64::from(f.rect[3]) * sy + dy) as f32,
            ];
            yes(f);
        }
    }
#[test]fn negative_trailing_tracking_can_make_the_next_glyph_fit(){let mut f=fixture(2);f.cols[0].x1=75.;f.bounds[0]=NativeBounds::Glyphs([5.,12.,75.,23.]);f.flows[0].as_mut().unwrap().end=75.;assert!(prove(&f).is_some());f.flows[0].as_mut().unwrap().trailing=-4.;assert_eq!(prove(&f),None);}
#[test]fn negative_first_TJ_adjustment_can_make_the_next_glyph_fit(){let mut f=fixture(2);f.cols[0].x1=75.;f.bounds[0]=NativeBounds::Glyphs([5.,12.,75.,23.]);f.flows[0].as_mut().unwrap().end=75.;assert!(prove(&f).is_some());f.flows[1].as_mut().unwrap().prefix=-4.;assert_eq!(prove(&f),None);}
#[test]fn positive_native_tracking_or_TJ_adjustment_can_force_a_wrap(){for prefix in [false,true]{let mut f=fixture(2);f.cols[0].x1=70.;f.bounds[0]=NativeBounds::Glyphs([5.,12.,70.,23.]);f.flows[0].as_mut().unwrap().end=70.;assert_eq!(prove(&f),None);if prefix{f.flows[1].as_mut().unwrap().prefix=3.;}else{f.flows[0].as_mut().unwrap().trailing=3.;}yes(f);}}
}
