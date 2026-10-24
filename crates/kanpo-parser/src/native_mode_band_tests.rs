use super::*;
use crate::{
    native_word_axes::NativeAxis,
    native_word_flow::{NativeFlow, NativeMode},
    trace::{NativeBounds, Word, WordStatus},
};
struct F {
    frames: Vec<[f32; 4]>,
    cols: Vec<Col>,
    words: Vec<Word>,
    bounds: Vec<NativeBounds>,
    axes: Vec<NativeAxis>,
    flows: Vec<Option<NativeFlow>>,
}
impl F {
    fn add(&mut self, x: f32, right: f32, y: f64, mode: NativeMode, text: &str) -> usize {
        let id = self.cols.len();
        let b = [x, y as f32 - 6., right, y as f32 + 3.];
        let c = Col {
            id,
            x,
            y: b[1],
            x1: right,
            y1: b[3],
            text: text.into(),
        };
        self.words.push(Word {
            id,
            x0: c.x,
            y0: c.y,
            x1: c.x1,
            y1: c.y1,
            source_box: b,
            raw: c.text.clone(),
            text: c.text.clone(),
            status: WordStatus::Used,
        });
        self.cols.push(c);
        self.bounds.push(NativeBounds::Glyphs(b));
        self.axes.push(NativeAxis::Right);
        self.flows.push(Some(NativeFlow {
            mode: Some(mode),
            start: x as f64,
            baseline: y,
            end: right as f64,
            leading: 1.,
            trailing: 0.,
            prefix: 0.,
        }));
        id
    }
    fn s(&self) -> SourceGeometry<'_> {
        SourceGeometry {
            enclosures: &self.frames,
            words: &self.words,
            bounds: &self.bounds,
            axes: &self.axes,
            flows: &self.flows,
        }
    }
    fn line(&self, ids: &[usize]) -> trace::Line {
        let cs: Vec<_> = ids.iter().map(|&i| self.cols[i].clone()).collect();
        trace::Line::text(render_line(&cs), ids.to_vec())
    }
    fn before(&self) -> Vec<trace::Line> {
        vec![self.line(&[0, 1]), self.line(&[2, 3])]
    }
    fn after(&self) -> Vec<trace::Line> {
        vec![self.line(&[0, 1, 2]), self.line(&[3])]
    }
}
fn fixture() -> F {
    let mut f = F {
        frames: vec![],
        cols: vec![],
        words: vec![],
        bounds: vec![],
        axes: vec![],
        flows: vec![],
    };
    f.add(20., 40., 100., NativeMode::Vertical, "任意の項目");
    f.add(40., 60., 100., NativeMode::Vertical, "修飾部");
    f.add(120., 140., 100., NativeMode::Vertical, "任意の語");
    f.add(155., 200., 102., NativeMode::Horizontal, "別の本文");
    f
}
#[test]
fn native_modes_supply_a_real_empty_band_without_a_guessed_page_coordinate() {
    let f = fixture();
    let x = cut(&f.cols, f.s(), &[], &[]).unwrap();
    assert!(140. < x && x < 155.);
}
#[test]
fn exact_source_flow_rejoins_a_group_and_preserves_the_other_source_line() {
    let f = fixture();
    assert!(preserves_source_lines(
        &f.cols,
        f.s(),
        &f.before(),
        &f.after(),
        &[],
        &[]
    ));
}
#[test]
fn arbitrary_words_and_punctuation_never_authorize_the_new_partition() {
    for labels in [
        ["x", "y", "z", "q"],
        ["元と無関係", "あらゆる語句", "非数字", "標題でもない"],
    ] {
        let mut f = fixture();
        for (i, t) in labels.iter().enumerate() {
            f.cols[i].text = t.to_string();
            f.words[i].text = t.to_string();
        }
        assert!(cut(&f.cols, f.s(), &[], &[]).is_some());
        assert!(preserves_source_lines(
            &f.cols,
            f.s(),
            &f.before(),
            &f.after(),
            &[],
            &[]
        ));
    }
}
#[test]
fn source_band_and_flow_proof_are_invariant_under_scale_and_translation() {
    for (scale, dx, dy) in [(0.125, -100., 2000.), (1., 0., 0.), (8., 170., -30.)] {
        let mut f = fixture();
        for i in 0..f.cols.len() {
            let c = &mut f.cols[i];
            c.x = c.x * scale + dx;
            c.x1 = c.x1 * scale + dx;
            c.y = c.y * scale + dy;
            c.y1 = c.y1 * scale + dy;
            let w = &mut f.words[i];
            w.x0 = c.x;
            w.y0 = c.y;
            w.x1 = c.x1;
            w.y1 = c.y1;
            w.source_box = [c.x, c.y, c.x1, c.y1];
            f.bounds[i] = NativeBounds::Glyphs(w.source_box);
            let flow = f.flows[i].as_mut().unwrap();
            flow.start = flow.start * scale as f64 + dx as f64;
            flow.end = flow.end * scale as f64 + dx as f64;
            flow.baseline = flow.baseline * scale as f64 + dy as f64;
            flow.leading *= scale as f64;
        }
        assert!(cut(&f.cols, f.s(), &[], &[]).is_some());
        assert!(preserves_source_lines(
            &f.cols,
            f.s(),
            &f.before(),
            &f.after(),
            &[],
            &[]
        ));
    }
}
#[test]
fn reversed_source_mode_bands_do_not_depend_on_vertical_being_left() {
    let mut f = fixture();
    for flow in &mut f.flows {
        let q = flow.as_mut().unwrap();
        q.mode = Some(if q.mode == Some(NativeMode::Vertical) {
            NativeMode::Horizontal
        } else {
            NativeMode::Vertical
        });
    }
    assert!(cut(&f.cols, f.s(), &[], &[]).is_some());
    assert!(preserves_source_lines(
        &f.cols,
        f.s(),
        &f.before(),
        &f.after(),
        &[],
        &[]
    ));
}
#[test]
fn absent_or_unknown_metadata_is_not_inferred_from_plausible_boxes() {
    for id in 0..4 {
        for what in 0..4 {
            let mut f = fixture();
            match what {
                0 => f.flows[id] = None,
                1 => f.flows[id].as_mut().unwrap().mode = None,
                2 => f.axes[id] = NativeAxis::Unknown,
                _ => f.bounds[id] = NativeBounds::Unknown,
            };
            assert!(cut(&f.cols, f.s(), &[], &[]).is_none());
            assert!(!preserves_source_lines(
                &f.cols,
                f.s(),
                &f.before(),
                &f.after(),
                &[],
                &[]
            ));
        }
    }
}
#[test]
fn all_compatibility_metadata_defers() {
    let mut f = fixture();
    f.axes.fill(NativeAxis::Compatibility);
    assert!(cut(&f.cols, f.s(), &[], &[]).is_none());
}
#[test]
fn nonfinite_and_foreign_original_flows_defer() {
    for what in 0..5 {
        let mut f = fixture();
        let q = f.flows[2].as_mut().unwrap();
        match what {
            0 => q.baseline = f64::NAN,
            1 => q.start += 1.,
            2 => q.end = f64::INFINITY,
            3 => q.leading = 0.,
            _ => q.prefix = f64::NAN,
        };
        assert!(cut(&f.cols, f.s(), &[], &[]).is_none());
    }
}
#[test]
fn a_f32_rounded_baseline_match_is_insufficient() {
    let mut f = fixture();
    f.flows[2].as_mut().unwrap().baseline += 1e-12;
    assert!(!preserves_source_lines(
        &f.cols,
        f.s(),
        &f.before(),
        &f.after(),
        &[],
        &[]
    ));
}
#[test]
fn source_projection_changes_and_replaced_occurrences_cannot_authorize_the_cut() {
    for what in 0..5 {
        let mut f = fixture();
        match what {
            0 => f.cols[2].text.push('他'),
            1 => f.cols[2].x += 1.,
            2 => f.words[2].id = 0,
            3 => f.words[2].status = WordStatus::Margin,
            _ => f.words[2].status = WordStatus::Empty,
        };
        assert!(cut(&f.cols, f.s(), &[], &[]).is_none());
    }
}
#[test]
fn duplicate_occurrences_and_short_metadata_defer() {
    let mut f = fixture();
    f.cols.push(f.cols[2].clone());
    assert!(cut(&f.cols, f.s(), &[], &[]).is_none());
    f.cols.pop();
    f.flows.pop();
    assert!(cut(&f.cols, f.s(), &[], &[]).is_none());
}
#[test]
fn a_single_source_mode_is_not_column_evidence() {
    let mut f = fixture();
    for q in f.flows.iter_mut().flatten() {
        q.mode = Some(NativeMode::Horizontal);
    }
    assert!(cut(&f.cols, f.s(), &[], &[]).is_none());
}
#[test]
fn inline_tatechuyoko_within_a_horizontal_source_band_is_preserved() {
    let mut f = fixture();
    f.add(160., 170., 120., NativeMode::Vertical, "12");
    assert!(cut(&f.cols, f.s(), &[], &[]).is_none());
}
#[test]
fn touching_or_overlapping_native_mode_bands_are_not_a_gap() {
    for x in [140., 139., 130.] {
        let mut f = fixture();
        f.cols[3].x = x;
        f.words[3].x0 = x;
        f.words[3].source_box[0] = x;
        if let NativeBounds::Glyphs(ref mut b) = f.bounds[3] {
            b[0] = x;
        }
        f.flows[3].as_mut().unwrap().start = x as f64;
        assert!(cut(&f.cols, f.s(), &[], &[]).is_none());
    }
}
#[test]
fn a_crossing_horizontal_rule_fences_the_source_components() {
    let f = fixture();
    assert!(cut(&f.cols, f.s(), &[], &[(100., 10., 210.)]).is_none());
}
#[test]
fn unknown_or_invalid_rule_geometry_cannot_supply_an_empty_band() {
    let f = fixture();
    for r in [
        [f32::NAN, 0., 200.],
        [100., 200., 0.],
        [100., 0., f32::INFINITY],
    ] {
        assert!(cut(&f.cols, f.s(), &[r.into()], &[]).is_none());
    }
}
#[test]
fn already_correct_homogeneous_source_lines_are_not_repartitioned() {
    let f = fixture();
    assert!(!preserves_source_lines(
        &f.cols,
        f.s(),
        &f.after(),
        &f.after(),
        &[],
        &[]
    ));
}
#[test]
fn ordinary_different_baselines_in_one_source_mode_do_not_prove_a_reassignment() {
    let mut f = fixture();
    f.flows[3].as_mut().unwrap().mode = Some(NativeMode::Vertical);
    assert!(!preserves_source_lines(
        &f.cols,
        f.s(),
        &f.before(),
        &f.after(),
        &[],
        &[]
    ));
}
#[test]
fn superscripts_are_preserved_when_no_source_mode_affinity_conflict_exists() {
    let mut f = fixture();
    f.flows[2].as_mut().unwrap().mode = Some(NativeMode::Horizontal);
    assert!(!preserves_source_lines(
        &f.cols,
        f.s(),
        &f.before(),
        &f.after(),
        &[],
        &[]
    ));
}
#[test]
fn splitting_an_existing_source_line_to_borrow_one_word_is_rejected() {
    let f = fixture();
    let a = vec![f.line(&[0]), f.line(&[1, 2]), f.line(&[3])];
    assert!(!preserves_source_lines(
        &f.cols,
        f.s(),
        &f.before(),
        &a,
        &[],
        &[]
    ));
}
#[test]
fn dropped_duplicated_or_reordered_source_words_are_rejected() {
    for ids in [vec![0, 2], vec![0, 1, 2, 2], vec![1, 0, 2]] {
        let f = fixture();
        let a = vec![f.line(&ids), f.line(&[3])];
        assert!(!preserves_source_lines(
            &f.cols,
            f.s(),
            &f.before(),
            &a,
            &[],
            &[]
        ));
    }
}
#[test]
fn rewritten_text_cannot_supply_an_original_line_witness() {
    let f = fixture();
    for side in [false, true] {
        let mut b = f.before();
        let mut a = f.after();
        if side {
            a[0].text.push('誤');
        } else {
            b[0].text.push('誤');
        }
        assert!(!preserves_source_lines(&f.cols, f.s(), &b, &a, &[], &[]));
    }
}
#[test]
fn source_tables_cannot_be_split_or_reassigned() {
    let f = fixture();
    let mut a = f.after();
    a[0].table = Some(trace::Table {
        rows: 0,
        cols: 0,
        cells: vec![],
        x0: 0.,
        y0: 0.,
        x1: 1.,
        y1: 1.,
    });
    assert!(!preserves_source_lines(
        &f.cols,
        f.s(),
        &f.before(),
        &a,
        &[],
        &[]
    ));
}
#[test]
fn a_new_line_may_only_combine_exactly_matching_original_source_flows() {
    let f = fixture();
    let a = vec![f.line(&[0, 1]), f.line(&[2]), f.line(&[3])];
    assert!(!preserves_source_lines(
        &f.cols,
        f.s(),
        &f.before(),
        &a,
        &[],
        &[]
    ));
}
#[test]
fn source_flow_both_sides_needs_an_actual_ownership_conflict() {
    let f = fixture();
    let b = vec![f.line(&[0]), f.line(&[1, 2]), f.line(&[3])];
    assert!(!preserves_source_lines(
        &f.cols,
        f.s(),
        &b,
        &f.after(),
        &[],
        &[]
    ));
}

#[test]
fn a_new_join_cannot_cross_an_actual_vertical_separator() {
    let f = fixture();
    assert!(!preserves_source_lines(
        &f.cols,
        f.s(),
        &f.before(),
        &f.after(),
        &[(90., 80., 120.)],
        &[]
    ));
}
#[test]
fn same_baseline_separate_source_lines_are_ambiguous_targets() {
    let mut f = fixture();
    f.add(80., 90., 100., NativeMode::Vertical, "独立の別欄");
    let mut b = f.before();
    b.insert(1, f.line(&[4]));
    let a = vec![f.line(&[0, 1, 4, 2]), f.line(&[3])];
    assert!(!preserves_source_lines(&f.cols, f.s(), &b, &a, &[], &[]));
}
#[test]
fn differing_source_modes_on_the_same_native_baseline_are_valid_inline_text() {
    let mut f = fixture();
    f.flows[3].as_mut().unwrap().baseline = 100.;
    assert!(!preserves_source_lines(
        &f.cols,
        f.s(),
        &f.before(),
        &f.after(),
        &[],
        &[]
    ));
}
#[test]
fn an_explicit_separator_in_the_mode_gap_supports_rather_than_blocks_the_partition() {
    let f = fixture();
    assert!(cut(&f.cols, f.s(), &[(147., 80., 120.)], &[]).is_some());
    assert!(preserves_source_lines(
        &f.cols,
        f.s(),
        &f.before(),
        &f.after(),
        &[(147., 80., 120.)],
        &[]
    ));
}

#[test]
fn a_new_source_join_cannot_cross_or_straddle_a_closed_native_frame() {
    for frame in [
        [10., 90., 70., 110.],
        [80., 90., 100., 110.],
        [30., 90., 150., 110.],
    ] {
        let mut f = fixture();
        f.frames.push(frame);
        assert!(!preserves_source_lines(
            &f.cols,
            f.s(),
            &f.before(),
            &f.after(),
            &[],
            &[]
        ));
    }
}
#[test]
fn a_complete_join_inside_its_own_native_frame_is_supported() {
    let mut f = fixture();
    f.frames.push([10., 90., 145., 110.]);
    assert!(preserves_source_lines(
        &f.cols,
        f.s(),
        &f.before(),
        &f.after(),
        &[],
        &[]
    ));
}
#[test]
fn an_unrelated_native_frame_does_not_block_a_source_affinity_proof() {
    let mut f = fixture();
    f.frames.push([300., 90., 400., 110.]);
    assert!(preserves_source_lines(
        &f.cols,
        f.s(),
        &f.before(),
        &f.after(),
        &[],
        &[]
    ));
}
#[test]
fn unknown_or_invalid_native_frames_defer_the_new_source_partition() {
    for frame in [
        [f32::NAN, 0., 1., 1.],
        [0., 0., f32::INFINITY, 1.],
        [10., 0., 0., 1.],
        [0., 0., 1., 0.],
    ] {
        let mut f = fixture();
        f.frames.push(frame);
        assert!(cut(&f.cols, f.s(), &[], &[]).is_none());
        assert!(!preserves_source_lines(
            &f.cols,
            f.s(),
            &f.before(),
            &f.after(),
            &[],
            &[]
        ));
    }
}
