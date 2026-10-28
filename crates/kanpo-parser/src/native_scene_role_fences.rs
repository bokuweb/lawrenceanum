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
