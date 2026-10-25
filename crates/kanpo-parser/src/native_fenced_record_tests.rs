use super::*;
use crate::{
    native_word_axes::NativeAxis,
    native_word_flow::{NativeFlow, NativeMode},
    trace::{NativeBounds, Word, WordStatus},
};
struct F {
    cols: Vec<Col>,
    words: Vec<Word>,
    bounds: Vec<NativeBounds>,
    axes: Vec<NativeAxis>,
    flows: Vec<Option<NativeFlow>>,
    v: Vec<(f32, f32, f32)>,
}
impl F {
    fn add(&mut self, text: String, b: [f32; 4]) -> usize {
        let id = self.cols.len();
        self.cols.push(Col {
            id,
            x: b[0],
            y: b[1],
            x1: b[2],
            y1: b[3],
            text: text.clone(),
        });
        self.words.push(Word {
            id,
            x0: b[0],
            y0: b[1],
            x1: b[2],
            y1: b[3],
            source_box: b,
            raw: text.clone(),
            text,
            status: WordStatus::Used,
        });
        self.bounds.push(NativeBounds::Glyphs(b));
        self.axes.push(NativeAxis::Right);
        self.flows.push(Some(NativeFlow {
            mode: Some(NativeMode::Horizontal),
            start: f64::from(b[0]),
            end: f64::from(b[2]),
            baseline: f64::from(b[1]) + 6.,
            leading: 8.,
            trailing: 0.,
            prefix: 0.,
        }));
        id
    }
    fn source(&self) -> SourceGeometry<'_> {
        SourceGeometry {
            enclosures: &[],
            flows: &self.flows,
            bounds: &self.bounds,
            axes: &self.axes,
            words: &self.words,
        }
    }
    fn run(&self) -> Option<Vec<trace::Table>> {
        tables(&self.cols, self.source(), &self.v, &[])
    }
}
fn fixture(panels: usize, wrap: usize) -> F {
    let mut f = F {
        cols: vec![],
        words: vec![],
        bounds: vec![],
        axes: vec![],
        flows: vec![],
        v: vec![],
    };
    for col in 0..panels {
        let x = col as f32 * 240.;
        for row in 0..2 {
            let y = 20. + row as f32 * 80.;
            f.add(
                format!("Arbitrary label {row}"),
                [x + 10., y, x + 120., y + 8.],
            );
            for i in 0..wrap {
                let yy = y + (i + 1) as f32 * 8.;
                f.add(
                    format!("continuation {i}"),
                    [x + 10., yy, x + 120., yy + 8.],
                );
            }
            f.add(
                format!("ABCDEFGHIJKLMNOP{}-12345-67890-12345", 123456789 + row),
                [x + 160., y, x + 220., y + 8.],
            );
        }
    }
    for i in 0..=panels {
        f.v.push((i as f32 * 240., 0., 180.));
    }
    f
}
#[test]
fn fenced_records_any_fenced_panel_count_and_long_code_or_wrap_are_structural() {
    for panels in [1, 2, 3] {
        for wrap in [0, 1, 6] {
            let f = fixture(panels, wrap);
            let ts = f.run().unwrap();
            assert_eq!(ts.len(), panels);
            for t in ts {
                assert_eq!(t.rows, 2);
                assert_eq!(t.cols, 2);
                for row in 0..2 {
                    let name = t.cells.iter().find(|c| c.row == row && c.col == 0).unwrap();
                    assert!(name.text.starts_with(&format!("Arbitrary label {row}")));
                    assert_eq!(name.words.len(), wrap + 1);
                }
            }
        }
    }
}
#[test]
fn fenced_records_complete_glyph_row_can_contain_a_mixed_font_word_without_full_flow() {
    let mut f = fixture(1, 1);
    f.flows[0] = None;
    assert!(f.run().is_some());
}
#[test]
fn fenced_records_missing_native_bounds_or_axis_does_not_use_compatibility() {
    for kind in 0..3 {
        let mut f = fixture(1, 1);
        match kind {
            0 => f.bounds[0] = NativeBounds::Unknown,
            1 => f.axes[0] = NativeAxis::Unknown,
            _ => f.bounds[0] = NativeBounds::Compatibility,
        };
        assert!(f.run().is_none());
    }
}
#[test]
fn fenced_records_anchor_requires_its_own_finite_horizontal_source_flow() {
    for kind in 0..4 {
        let mut f = fixture(1, 1);
        let i = 2;
        match kind {
            0 => f.flows[i] = None,
            1 => f.flows[i].as_mut().unwrap().baseline = f64::NAN,
            2 => f.flows[i].as_mut().unwrap().mode = Some(NativeMode::Vertical),
            _ => f.flows[i].as_mut().unwrap().end += 1.,
        };
        assert!(f.run().is_none());
    }
}
#[test]
fn fenced_records_projection_changes_and_duplicate_occurrences_are_not_owned() {
    let mut f = fixture(1, 1);
    f.cols[0].text.push('!');
    assert!(f.run().is_none());
    let mut f = fixture(1, 1);
    f.cols.push(f.cols[0].clone());
    assert!(f.run().is_none());
}
#[test]
fn fenced_records_missing_or_incomplete_source_fences_do_not_form_a_table() {
    let mut f = fixture(1, 1);
    f.v.pop();
    assert!(f.run().is_none());
    let mut f = fixture(1, 1);
    f.v[0].2 = 40.;
    assert!(f.run().is_none());
}
#[test]
fn fenced_records_internal_horizontal_ruling_is_a_different_structure() {
    let f = fixture(1, 1);
    assert!(tables(&f.cols, f.source(), &f.v, &[(60., 0., 240.)]).is_none());
}
#[test]
fn fenced_records_source_words_must_not_cross_a_panel_fence() {
    let mut f = fixture(1, 1);
    f.add("crossing".into(), [-4., 50., 20., 58.]);
    assert!(f.run().is_none());
}
#[test]
fn fenced_records_three_disconnected_lanes_are_not_guessed_as_two_cells() {
    let mut f = fixture(1, 0);
    for y in [20., 100.] {
        f.add("separate field".into(), [125., y, 145., y + 8.]);
    }
    assert!(f.run().is_none());
}
#[test]
fn fenced_records_unanchored_row_or_unlabelled_key_is_not_claimed() {
    let mut f = fixture(1, 0);
    f.add("prior body".into(), [10., 5., 120., 13.]);
    assert!(f.run().is_none());
    let mut f = fixture(1, 0);
    f.add("EXTRA123".into(), [160., 140., 220., 148.]);
    assert!(f.run().is_none());
}
#[test]
fn fenced_records_projection_order_and_source_geometry_transform_do_not_change_membership() {
    let mut f = fixture(2, 1);
    f.cols.reverse();
    assert_eq!(f.run().unwrap().len(), 2);
    for s in [0.5, 2.] {
        let mut f = fixture(1, 1);
        for c in &mut f.cols {
            c.x = c.x * s + 32.;
            c.x1 = c.x1 * s + 32.;
            c.y = c.y * s + 64.;
            c.y1 = c.y1 * s + 64.;
        }
        for w in &mut f.words {
            w.x0 = w.x0 * s + 32.;
            w.x1 = w.x1 * s + 32.;
            w.y0 = w.y0 * s + 64.;
            w.y1 = w.y1 * s + 64.;
            w.source_box = [w.x0, w.y0, w.x1, w.y1];
        }
        for b in &mut f.bounds {
            if let NativeBounds::Glyphs(b) = b {
                *b = [
                    b[0] * s + 32.,
                    b[1] * s + 64.,
                    b[2] * s + 32.,
                    b[3] * s + 64.,
                ];
            }
        }
        for flow in f.flows.iter_mut().flatten() {
            flow.start = flow.start * f64::from(s) + 32.;
            flow.end = flow.end * f64::from(s) + 32.;
            flow.baseline = flow.baseline * f64::from(s) + 64.;
            flow.leading *= f64::from(s);
        }
        for r in &mut f.v {
            r.0 = r.0 * s + 32.;
            r.1 = r.1 * s + 64.;
            r.2 = r.2 * s + 64.;
        }
        assert!(f.run().is_some());
    }
}

#[test]
fn fenced_records_rule_endpoints_on_source_baselines_cover_rows_without_ink_padding() {
    let mut f = fixture(2, 1);
    for rule in &mut f.v {
        rule.1 = 26.;
        rule.2 = 114.;
    }
    assert_eq!(f.run().unwrap().len(), 2);
    // The last continuation baseline is 114; a rule ending before it is incomplete.
    f.v[1].2 = 113.;
    assert!(f.run().is_none());
}
#[test]
fn fenced_records_invalid_label_flow_cannot_hide_behind_a_matching_glyph_row() {
    let mut f = fixture(1, 0);
    f.flows[0].as_mut().unwrap().baseline = f64::NAN;
    assert!(f.run().is_none());
}

#[test]
fn fenced_records_source_baselines_can_share_a_physical_row_without_numeric_equality() {
    let mut f = fixture(1, 1);
    // Labels and keys use distinct source text origins; both original baselines
    // must lie within the other field's complete native glyph interval.
    for id in [0, 3] {
        f.cols[id].y += 1.;
        f.cols[id].y1 += 1.;
        f.words[id].y0 += 1.;
        f.words[id].y1 += 1.;
        f.words[id].source_box = [f.cols[id].x, f.cols[id].y, f.cols[id].x1, f.cols[id].y1];
        f.bounds[id] = NativeBounds::Glyphs(f.words[id].source_box);
        f.flows[id].as_mut().unwrap().baseline += 1.;
    }
    assert!(f.run().is_some());
}

#[test]
fn fenced_records_mixed_font_glyph_cell_can_witness_exactly_one_explicit_anchor_row() {
    let mut f = fixture(1, 1);
    f.flows[0] = None;
    f.cols[0].y += 1.;
    f.cols[0].y1 += 1.;
    f.words[0].y0 += 1.;
    f.words[0].y1 += 1.;
    f.words[0].source_box = [f.cols[0].x, f.cols[0].y, f.cols[0].x1, f.cols[0].y1];
    f.bounds[0] = NativeBounds::Glyphs(f.words[0].source_box);
    assert!(f.run().is_some());
    // A glyph union containing two anchors does not prove one source row.
    f.cols[0].y1 = 108.;
    f.words[0].y1 = 108.;
    f.words[0].source_box[3] = 108.;
    f.bounds[0] = NativeBounds::Glyphs(f.words[0].source_box);
    assert!(f.run().is_none());
}

#[test]
fn fenced_records_one_name_field_keeps_its_text_across_physical_wraps() {
    let f = fixture(1, 1);
    let ts = f.run().unwrap();
    assert_eq!(ts[0].cells[0].text, "Arbitrary label 0continuation 0");
}
