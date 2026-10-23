//! Keep one complete closed frame on one source-coordinate basis.
use crate::trace::{NativeBounds, Word, WordStatus};
use crate::vertical::Col;
use std::collections::{HashMap, HashSet};
type Rect = [f32; 4];
fn valid(b: Rect) -> bool {
    b.iter().all(|v| v.is_finite()) && b[0] < b[2] && b[1] < b[3]
}
fn contains(a: Rect, b: Rect) -> bool {
    a[0] <= b[0] && a[1] <= b[1] && b[2] <= a[2] && b[3] <= a[3]
}
fn intersects(a: Rect, b: Rect) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}
pub(crate) fn restore(
    cols: &mut [Col],
    words: &[Word],
    bounds: &[NativeBounds],
    top_down: &[bool],
    frames: &[Rect],
) {
    if frames.iter().any(|b| !valid(*b)) {
        return;
    }
    let used: Vec<_> = words
        .iter()
        .filter(|w| matches!(w.status, WordStatus::Used))
        .collect();
    let mut source = HashMap::new();
    let mut declarations = HashMap::new();
    for w in &used {
        let Some(NativeBounds::Glyphs(b)) = bounds.get(w.id) else {
            return;
        };
        if !valid(*b) {
            return;
        }
        let old = [w.x0, w.y0, w.x1, w.y1];
        if !valid(old) {
            return;
        }
        let key = (
            [
                w.x0.to_bits(),
                w.y0.to_bits(),
                w.x1.to_bits(),
                w.y1.to_bits(),
            ],
            w.raw.as_str(),
        );
        if declarations.insert(key, *b).is_some_and(|v| v != *b) {
            return;
        }
        if source.insert(w.id, (*b, old)).is_some() {
            return;
        }
    }
    let mut candidates = Vec::new();
    for &frame in frames {
        if frames
            .iter()
            .any(|&other| other != frame && contains(other, frame))
        {
            continue;
        }
        if frames.iter().any(|&other| {
            intersects(frame, other) && !contains(frame, other) && !contains(other, frame)
        }) {
            continue;
        }
        if source.values().any(|&(b, old)| {
            intersects(frame, b) && !contains(frame, b)
                || contains(frame, old) && !contains(frame, b)
        }) {
            continue;
        }
        let ids: HashSet<_> = source
            .iter()
            .filter_map(|(&id, &(b, _))| contains(frame, b).then_some(id))
            .collect();
        if ids.iter().any(|id| top_down.get(*id) != Some(&true)) {
            continue;
        }
        if !ids.iter().any(|id| !contains(frame, source[id].1)) {
            continue;
        }
        if cols.iter().filter(|c| ids.contains(&c.id)).count() != ids.len() {
            continue;
        }
        if !candidates.iter().any(|old: &HashSet<usize>| *old == ids) {
            candidates.push(ids);
        }
    }
    let accepted: Vec<_> = candidates
        .iter()
        .enumerate()
        .filter(|(i, ids)| {
            !candidates
                .iter()
                .enumerate()
                .any(|(j, other)| *i != j && !ids.is_disjoint(other))
        })
        .map(|(_, v)| v)
        .collect();
    for ids in accepted {
        for c in cols.iter_mut().filter(|c| ids.contains(&c.id)) {
            let b = source[&c.id].0;
            (c.x, c.y, c.x1, c.y1) = (b[0], b[1], b[2], b[3]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture {
        cols: Vec<Col>,
        words: Vec<Word>,
        bounds: Vec<NativeBounds>,
        top_down: Vec<bool>,
        frames: Vec<Rect>,
    }
    fn fixture() -> Fixture {
        let mut f = Fixture {
            cols: vec![],
            words: vec![],
            bounds: vec![],
            top_down: vec![],
            frames: vec![[0., 0., 100., 100.]],
        };
        for (id, (old, native)) in [
            ([80., -5., 90., 15.], [76., 5., 86., 25.]),
            ([60., 10., 70., 80.], [56., 18., 66., 88.]),
            ([40., 20., 50., 60.], [36., 28., 46., 68.]),
            ([20., 25., 30., 70.], [16., 33., 26., 78.]),
        ]
        .into_iter()
        .enumerate()
        {
            let text = format!("任意{id}");
            f.cols.push(Col {
                id,
                x: old[0],
                y: old[1],
                x1: old[2],
                y1: old[3],
                text: text.clone(),
            });
            f.words.push(Word {
                source_box: [old[0], old[1], old[2], old[3]],
                id,
                x0: old[0],
                y0: old[1],
                x1: old[2],
                y1: old[3],
                raw: text.clone(),
                text,
                status: WordStatus::Used,
            });
            f.bounds.push(NativeBounds::Glyphs(native));
            f.top_down.push(true);
        }
        f
    }
    impl Fixture {
        fn restore(&mut self) {
            restore(
                &mut self.cols,
                &self.words,
                &self.bounds,
                &self.top_down,
                &self.frames,
            );
        }
        fn snapshot(&self) -> Vec<(usize, [u32; 4], String)> {
            self.cols
                .iter()
                .map(|c| {
                    (
                        c.id,
                        [c.x.to_bits(), c.y.to_bits(), c.x1.to_bits(), c.y1.to_bits()],
                        c.text.clone(),
                    )
                })
                .collect()
        }
        fn assert_source_columns(&self) {
            for c in &self.cols {
                let NativeBounds::Glyphs(b) = self.bounds[c.id] else {
                    panic!()
                };
                assert_eq!([c.x, c.y, c.x1, c.y1], b);
            }
        }
    }
    #[test]
    fn one_original_ownership_discrepancy_moves_the_complete_vertical_frame_on_one_basis() {
        let mut f = fixture();
        let words = serde_json::to_value(&f.words).unwrap();
        f.restore();
        f.assert_source_columns();
        assert_eq!(serde_json::to_value(&f.words).unwrap(), words);
    }
    #[test]
    fn scaling_translation_and_arbitrary_text_preserve_original_frame_ownership() {
        for k in [0.125, 1., 8.] {
            let mut f = fixture();
            let tr = |b: Rect| {
                [
                    b[0] * k - 100.,
                    b[1] * k - 200.,
                    b[2] * k - 100.,
                    b[3] * k - 200.,
                ]
            };
            for w in &mut f.words {
                let b = tr([w.x0, w.y0, w.x1, w.y1]);
                (w.x0, w.y0, w.x1, w.y1) = (b[0], b[1], b[2], b[3]);
            }
            for c in &mut f.cols {
                let b = tr([c.x, c.y, c.x1, c.y1]);
                (c.x, c.y, c.x1, c.y1) = (b[0], b[1], b[2], b[3]);
            }
            for b in &mut f.bounds {
                if let NativeBounds::Glyphs(b) = b {
                    *b = tr(*b);
                }
            }
            for b in &mut f.frames {
                *b = tr(*b);
            }
            f.restore();
            f.assert_source_columns();
        }
    }
    #[test]
    fn a_mixed_or_unknown_advance_direction_prevents_the_entire_frame_change() {
        for n in 0..4 {
            let mut f = fixture();
            f.top_down[n] = false;
            let old = f.snapshot();
            f.restore();
            assert_eq!(f.snapshot(), old);
        }
    }
    #[test]
    fn missing_original_direction_evidence_does_not_gain_authority() {
        let mut f = fixture();
        f.top_down.pop();
        let old = f.snapshot();
        f.restore();
        assert_eq!(f.snapshot(), old);
    }
    #[test]
    fn unknown_original_bounds_do_not_gain_compatibility_geometry() {
        let mut f = fixture();
        f.bounds[1] = NativeBounds::Unknown;
        let old = f.snapshot();
        f.restore();
        assert_eq!(f.snapshot(), old);
    }
    #[test]
    fn an_unrepresented_original_word_prevents_a_partial_coordinate_change() {
        let mut f = fixture();
        f.cols.pop();
        let old = f.snapshot();
        f.restore();
        assert_eq!(f.snapshot(), old);
    }
    #[test]
    fn a_source_word_crossing_the_frame_edge_is_ambiguous() {
        let mut f = fixture();
        f.bounds[1] = NativeBounds::Glyphs([56., 18., 66., 108.]);
        let old = f.snapshot();
        f.restore();
        assert_eq!(f.snapshot(), old);
    }
    #[test]
    fn nested_duplicate_borders_do_not_change_the_result_or_idempotence() {
        let mut f = fixture();
        f.frames.extend([[1., 1., 99., 99.], [0., 0., 100., 100.]]);
        f.restore();
        f.assert_source_columns();
        let old = f.snapshot();
        f.restore();
        assert_eq!(f.snapshot(), old);
    }
    #[test]
    fn nonnested_intersecting_enclosures_do_not_select_a_winner() {
        let mut f = fixture();
        f.frames.push([5., -1., 105., 99.]);
        let old = f.snapshot();
        f.restore();
        assert_eq!(f.snapshot(), old);
    }
    #[test]
    fn a_frame_already_owning_all_original_compatibility_words_does_not_rebase() {
        let mut f = fixture();
        f.words[0].y0 = 1.;
        f.words[0].y1 = 21.;
        f.cols[0].y = 1.;
        f.cols[0].y1 = 21.;
        let old = f.snapshot();
        f.restore();
        assert_eq!(f.snapshot(), old);
    }
    #[test]
    fn conflicting_source_occurrence_aliases_abstain_in_either_emission_order() {
        for reverse in [false, true] {
            let mut f = fixture();
            let mut w = f.words[0].clone();
            w.id = 4;
            f.words.push(w);
            let mut c = f.cols[0].clone();
            c.id = 4;
            f.cols.push(c);
            f.bounds.push(NativeBounds::Glyphs([77., 5., 87., 25.]));
            f.top_down.push(true);
            if reverse {
                f.words.reverse();
            }
            let old = f.snapshot();
            f.restore();
            assert_eq!(f.snapshot(), old);
        }
    }
}
