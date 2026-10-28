// Original stroke centers can support material on either page axis. Each
// material interval needs actual continuous source segments; paint hulls and
// point tolerances never bridge a missing centerline interval.
impl Scene {
    pub(crate) fn parallel_fences(
        &self,
        axis: crate::native_word_axes::NativeAxis,
        supports: &[[f64; 2]],
    ) -> Vec<f64> {
        use crate::native_word_axes::NativeAxis;
        let primary = match axis {
            NativeAxis::Down => 1,
            NativeAxis::Right => 0,
            _ => return vec![],
        };
        let cross = 1 - primary;
        if supports.is_empty()
            || supports
                .iter()
                .any(|s| !s.iter().all(|v| v.is_finite()) || s[0] > s[1])
        {
            return vec![];
        }
        let mut positions = self
            .lines
            .iter()
            .filter_map(|&(c, _, _)| {
                (c[primary] == c[primary + 2] && c[cross] < c[cross + 2]).then_some(c[primary])
            })
            .collect::<Vec<_>>();
        positions.sort_by(f64::total_cmp);
        positions.dedup();
        positions.retain(|&q| {
            let mut segments = self
                .lines
                .iter()
                .filter_map(|&(c, _, _)| {
                    (c[primary] == q && c[primary + 2] == q && c[cross] < c[cross + 2])
                        .then_some([c[cross], c[cross + 2]])
                })
                .collect::<Vec<_>>();
            segments.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
            supports.iter().all(|&[lo, hi]| {
                let mut end = lo;
                for &[a, z] in &segments {
                    if a <= end && end <= z {
                        end = z;
                        if end >= hi {
                            return true;
                        }
                    }
                }
                false
            })
        });
        positions
    }
}

impl Scene {
    // Missing support is not an article boundary. A real perpendicular source
    // cap must meet all three fences and separate every retained glyph from
    // every glyph in the next line, including opaque physical envelopes.
    pub(crate) fn cross_cap_separates(
        &self,
        axis: crate::native_word_axes::NativeAxis,
        q: [f64; 3],
        retained: &[Rect],
        next: &[Rect],
    ) -> bool {
        use crate::native_word_axes::NativeAxis::{Down, Right};
        if !matches!(axis, Down | Right)
            || retained.is_empty()
            || next.is_empty()
            || !q.iter().all(|v| v.is_finite())
            || !(q[0] < q[1] && q[1] < q[2])
            || !retained.iter().chain(next).all(|&b| valid(b))
        {
            return false;
        }
        let cross = usize::from(axis == Right);
        let other = if axis == Right { Down } else { Right };
        let lo = retained
            .iter()
            .map(|m| m[cross])
            .fold(f64::INFINITY, f64::min);
        let hi = retained
            .iter()
            .map(|m| m[cross + 2])
            .fold(f64::NEG_INFINITY, f64::max);
        self.parallel_fences(other, &[[q[0], q[2]]])
            .into_iter()
            .any(|cap| {
                if !((hi <= cap && next.iter().all(|m| m[cross + 2] > cap))
                    || (cap <= lo && next.iter().all(|m| m[cross] < cap)))
                {
                    return false;
                }
                let fences = self.parallel_fences(axis, &[[cap, cap]]);
                q.iter().all(|cut| fences.contains(cut))
            })
    }
}

impl Scene {
    pub(crate) fn opaque_crosses_fence(
        &self,
        axis: crate::native_word_axes::NativeAxis,
        q: f64,
        supports: &[[f64; 2]],
    ) -> bool {
        let primary = usize::from(axis == crate::native_word_axes::NativeAxis::Down);
        let cross = 1 - primary;
        let lo = supports.iter().map(|s| s[0]).fold(f64::INFINITY, f64::min);
        let hi = supports
            .iter()
            .map(|s| s[1])
            .fold(f64::NEG_INFINITY, f64::max);
        self.obstacles
            .iter()
            .any(|b| b[primary] < q && q < b[primary + 2] && b[cross] <= hi && lo <= b[cross + 2])
    }
}

#[cfg(test)]
mod role_fence_tests {
    use super::*;
    use crate::native_word_axes::NativeAxis::{Down, Right, Unknown};
    fn scene(lines: Vec<Rect>) -> Scene {
        Scene {
            words: vec![],
            obstacles: vec![],
            lines: lines
                .into_iter()
                .map(|c| {
                    let paint = [c[0] - 1., c[1] - 1., c[2] + 1., c[3] + 1.];
                    (c, paint, false)
                })
                .collect(),
        }
    }
    fn cap_scene(axis: crate::native_word_axes::NativeAxis) -> (Scene, impl Fn(Rect) -> Rect) {
        let project = move |b: Rect| {
            if axis == Down {
                [b[1], b[0], b[3], b[2]]
            } else {
                b
            }
        };
        let lines = vec![
            [2., 0., 2., 20.],
            [7., 0., 7., 20.],
            [13., 0., 13., 20.],
            [2., 10., 13., 10.],
        ];
        (scene(lines.into_iter().map(project).collect()), project)
    }
    #[test]
    fn a_real_cross_cap_separates_all_material_on_both_axes_and_reading_directions() {
        for axis in [Down, Right] {
            let (s, p) = cap_scene(axis);
            for (retained, next) in [
                ([3., 1., 5., 2.], [3., 11., 5., 12.]),
                ([3., 11., 5., 12.], [3., 1., 5., 2.]),
            ] {
                assert!(s.cross_cap_separates(axis, [2., 7., 13.], &[p(retained)], &[p(next)]));
            }
        }
    }
    #[test]
    fn source_caps_are_invariant_under_finite_scale_translation_and_material_counts() {
        for axis in [Down, Right] {
            for scale in [0.125, 1., 64.] {
                for offset in [-32., 0., 256.] {
                    let (s, p) = cap_scene(axis);
                    let t = |v: f64| v * scale + offset;
                    let s = scene(s.lines.iter().map(|(b, _, _)| b.map(t)).collect());
                    for count in [1, 2, 17, 1009] {
                        assert!(s.cross_cap_separates(
                            axis,
                            [2., 7., 13.].map(t),
                            &vec![p([3., 1., 5., 2.]).map(t); count],
                            &vec![p([3., 11., 5., 12.]).map(t); count]
                        ));
                    }
                }
            }
        }
    }
    #[test]
    fn a_cross_cap_cannot_hide_old_material_in_the_next_mixed_line() {
        for axis in [Down, Right] {
            let (s, p) = cap_scene(axis);
            let old = p([3., 1., 5., 2.]);
            let new = p([3., 11., 5., 12.]);
            assert!(!s.cross_cap_separates(axis, [2., 7., 13.], &[old], &[new, old]));
            assert!(!s.cross_cap_separates(axis, [2., 7., 13.], &[old, new], &[new]));
            assert!(!s.cross_cap_separates(axis, [2., 7., 13.], &[old], &[p([3., 9., 5., 10.])]));
            assert!(s.cross_cap_separates(
                axis,
                [2., 7., 13.],
                &[old],
                &[p([3., 9., 5., f64::from_bits(10f64.to_bits() + 1)])]
            ));
        }
    }
    #[test]
    fn a_cap_needs_all_fence_intersections_and_a_continuous_original_cross_span() {
        for axis in [Down, Right] {
            let (s, p) = cap_scene(axis);
            let old = p([3., 1., 5., 2.]);
            let new = p([3., 11., 5., 12.]);
            for missing in 0..4 {
                let mut bad = scene(s.lines.iter().map(|(b, _, _)| *b).collect());
                bad.lines.remove(missing);
                assert!(!bad.cross_cap_separates(axis, [2., 7., 13.], &[old], &[new]));
            }
            let mut bad = scene(s.lines.iter().map(|(b, _, _)| *b).collect());
            bad.lines.pop();
            for b in [
                [2., 10., 7., 10.],
                [f64::from_bits(7f64.to_bits() + 1), 10., 13., 10.],
            ] {
                let b = p(b);
                bad.lines.push((b, b, false));
            }
            assert!(!bad.cross_cap_separates(axis, [2., 7., 13.], &[old], &[new]));
        }
    }
    #[test]
    fn absent_invalid_or_unknown_material_never_certifies_a_cap() {
        let (s, p) = cap_scene(Right);
        let old = p([3., 1., 5., 2.]);
        let new = p([3., 11., 5., 12.]);
        assert!(!s.cross_cap_separates(Unknown, [2., 7., 13.], &[old], &[new]));
        for q in [[2., 7., f64::INFINITY], [2., 7., 7.], [2., f64::NAN, 13.]] {
            assert!(!s.cross_cap_separates(Right, q, &[old], &[new]));
        }
        for material in [vec![], vec![[0., 0., 0., 1.]], vec![[0., 0., f64::NAN, 1.]]] {
            assert!(!s.cross_cap_separates(Right, [2., 7., 13.], &material, &[new]));
            assert!(!s.cross_cap_separates(Right, [2., 7., 13.], &[old], &material));
        }
    }
    #[test]
    fn genuine_segments_support_each_material_without_claiming_the_gap_is_empty() {
        let missing = f64::from_bits(2f64.to_bits() + 1);
        let s = scene(vec![[0., 3., 2., 3.], [missing, 3., 5., 3.]]);
        assert_eq!(s.parallel_fences(Down, &[[0.5, 1.5], [3., 4.]]), vec![3.]);
        assert!(s.parallel_fences(Down, &[[1., 3.]]).is_empty());
        assert!(s.parallel_fences(Down, &[[2., missing]]).is_empty());
        assert_eq!(s.parallel_fences(Down, &[[2., 2.]]), vec![3.]);
        assert_eq!(s.parallel_fences(Down, &[[missing, missing]]), vec![3.]);
    }
    #[test]
    fn touching_segments_are_continuous_on_either_axis_under_source_transformations() {
        for scale in [0.5, 1., 8.] {
            for offset in [-100., 0., 70.] {
                for axis in [Down, Right] {
                    let c = |v| v * scale + offset;
                    let mut lines =
                        vec![[c(0.), c(3.), c(2.), c(3.)], [c(2.), c(3.), c(5.), c(3.)]];
                    if axis == Right {
                        for b in &mut lines {
                            *b = [b[1], b[0], b[3], b[2]];
                        }
                    }
                    assert_eq!(
                        scene(lines).parallel_fences(axis, &[[c(1.), c(4.)]]),
                        vec![c(3.)]
                    );
                }
            }
        }
    }
    #[test]
    fn source_support_has_no_material_or_neighbor_count_budget() {
        let s = scene(vec![[0., 3., 5., 3.]]);
        for count in [1, 2, 7, 1009] {
            let support = (0..count)
                .map(|i| {
                    let x = (i % 5) as f64;
                    [x, x]
                })
                .collect::<Vec<_>>();
            assert_eq!(s.parallel_fences(Down, &support), vec![3.]);
        }
    }
    #[test]
    fn no_axis_no_material_or_invalid_material_never_proves_a_fence() {
        let s = scene(vec![[0., 3., 5., 3.]]);
        for intervals in [
            vec![],
            vec![[2., 1.]],
            vec![[0., f64::INFINITY]],
            vec![[f64::NAN, 1.]],
        ] {
            assert!(s.parallel_fences(Down, &intervals).is_empty());
        }
        assert!(s.parallel_fences(Unknown, &[[0., 1.]]).is_empty());
        assert!(s.parallel_fences(Right, &[[0., 1.]]).is_empty());
        assert!(s.parallel_fences(Down, &[[-1., 1.]]).is_empty());
    }
    #[test]
    fn opaque_paint_across_a_role_divider_vetoes_the_common_material_span() {
        let mut s = scene(vec![]);
        s.obstacles.push([1., 2., 2., 4.]);
        assert!(s.opaque_crosses_fence(Down, 3., &[[0., 0.], [5., 5.]]));
        assert!(!s.opaque_crosses_fence(Down, 2., &[[0., 5.]]));
        assert!(!s.opaque_crosses_fence(Down, 3., &[[3., 5.]]));
        s.obstacles = vec![[2., 1., 4., 2.]];
        assert!(s.opaque_crosses_fence(Right, 3., &[[0., 5.]]));
    }
}
