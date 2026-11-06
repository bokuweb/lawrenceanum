//! An unruled instrument can have a full-height column beside short columns.
//! Native paths order these columns without the legacy tier height heuristic.
use crate::{
    native_column_flow::AxisFlow,
    native_word_axes::NativeAxis,
    trace::{Line, NativeBounds, Unit, Word, WordStatus},
};
use std::collections::BTreeSet;

fn compact(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

fn ordered(
    lines: &[Line],
    words: &[Word],
    bounds: &[NativeBounds],
    axes: &[NativeAxis],
    flows: &[Option<AxisFlow>],
    scene: &crate::native_source_scene::Scene,
) -> Option<Vec<Line>> {
    if words.len() != bounds.len()
        || words.len() != axes.len()
        || words.len() != flows.len()
        || !scene.matches(words)
    {
        return None;
    }
    let mut seen = BTreeSet::new();
    let mut rows = Vec::new();
    let mut envelope = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    for (index, line) in lines.iter().enumerate() {
        if line.table.is_some() || line.words.is_empty() {
            return None;
        }
        let mut baseline = None;
        let mut start = None;
        let mut end = None;
        let mut cross = [f32::INFINITY, f32::NEG_INFINITY];
        let mut source_text = String::new();
        for &id in &line.words {
            let w = words.get(id)?;
            if w.id != id
                || w.status != WordStatus::Used
                || !seen.insert(id)
                || axes[id] != NativeAxis::Down
            {
                return None;
            }
            let NativeBounds::Glyphs(b) = bounds[id] else {
                return None;
            };
            let [path, lo, hi] = flows[id]?.vertical_path(b)?;
            if baseline.is_some_and(|p| p != path) || end.is_some_and(|p| lo < p) {
                return None;
            }
            baseline = Some(path);
            start.get_or_insert(lo);
            end = Some(hi);
            cross[0] = cross[0].min(b[0]);
            cross[1] = cross[1].max(b[2]);
            source_text.push_str(&w.text);
            for k in 0..2 {
                envelope[k] = envelope[k].min(b[k]);
                envelope[k + 2] = envelope[k + 2].max(b[k + 2]);
            }
        }
        if compact(&source_text) != compact(&line.text) {
            return None;
        }
        rows.push((baseline?, start?, end?, index, cross));
    }
    if words
        .iter()
        .any(|w| w.status == WordStatus::Used && !seen.contains(&w.id))
        || !scene.unobstructed(envelope)
    {
        return None;
    }
    if words.iter().enumerate().any(|(id, w)| {
        if seen.contains(&id) {
            return false;
        }
        let b = match bounds[id] {
            NativeBounds::Glyphs(b) => b,
            _ => w.source_box,
        };
        b[0] < envelope[2] && envelope[0] < b[2] && b[1] < envelope[3] && envelope[1] < b[3]
    }) {
        return None;
    }
    rows.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.total_cmp(&b.1)));
    for pair in rows.windows(2) {
        if pair[0].0 == pair[1].0 && pair[1].1 < pair[0].2 {
            return None;
        }
        if pair[0].0 != pair[1].0 && pair[0].4[0] < pair[1].4[1] {
            return None;
        }
    }
    let roots = rows
        .iter()
        .filter(|r| crate::shinkyu::complete_numbered_publication_heading(&lines[r.3].text))
        .collect::<Vec<_>>();
    let [root] = roots.as_slice() else {
        return None;
    };
    if root.3 != rows.first()?.3 {
        return None;
    }
    if rows.iter().map(|r| r.3).eq(0..lines.len()) {
        return None;
    }
    Some(rows.into_iter().map(|r| lines[r.3].clone()).collect())
}

pub(crate) fn restore(
    units: &mut Vec<Unit>,
    words: &[Word],
    bounds: &[NativeBounds],
    axes: &[NativeAxis],
    flows: &[Option<AxisFlow>],
    page: scraper::ElementRef<'_>,
    rules: Option<&crate::lines::PageRules>,
) {
    let Some(lines) = units
        .iter()
        .map(|u| match u {
            Unit::Plain(l) => Some(l.as_slice()),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()
    else {
        return;
    };
    let lines = lines.into_iter().flatten().cloned().collect::<Vec<_>>();
    if !lines
        .iter()
        .any(|l| crate::shinkyu::complete_numbered_publication_heading(&l.text))
    {
        return;
    }
    let Some(scene) = rules.and_then(|r| crate::native_source_scene::for_page(page, r)) else {
        return;
    };
    if let Some(lines) = ordered(&lines, words, bounds, axes, flows, &scene) {
        *units = vec![Unit::Plain(lines)];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture {
        lines: Vec<Line>,
        words: Vec<Word>,
        bounds: Vec<NativeBounds>,
        axes: Vec<NativeAxis>,
        flows: Vec<Option<AxisFlow>>,
    }
    fn fixture(count: usize, scale: f64, offset: f64, step: f64) -> Fixture {
        let mut f = Fixture {
            lines: vec![],
            words: vec![],
            bounds: vec![],
            axes: vec![],
            flows: vec![],
        };
        let mut first = 0;
        for id in 0..count {
            let text = if id == 0 {
                "〇甲庁告示第一号".into()
            } else if id == 1 {
                "β".repeat(55)
            } else {
                format!("列{id}")
            };
            let after = first + text.chars().count();
            let x = (100. - id as f64 * step) * scale + offset;
            let lo = 10. * scale + offset;
            let hi = (10. + text.chars().count() as f64 * 2.) * scale + offset;
            let b = [x - scale, lo, x + scale, hi].map(|v| v as f32);
            let html=format!("<word data-wmode='1' data-native-axis='down' data-native-xmin='{}' data-native-ymin='{}' data-native-xmax='{}' data-native-ymax='{}' data-native-flow-start='{lo}' data-native-flow-end='{hi}' data-native-flow-baseline='{}' data-native-flow-leading='{}' data-native-flow-trailing='0' data-native-flow-prefix='0' data-native-flow-first-glyph='{first}' data-native-flow-after-glyph='{after}'></word>",b[0],b[1],b[2],b[3],-x,2.*scale);
            let dom = scraper::Html::parse_fragment(&html);
            let node = dom
                .select(&scraper::Selector::parse("word").unwrap())
                .next()
                .unwrap();
            f.flows.push(crate::native_column_flow::from_word(
                node,
                Some(count * 200),
            ));
            f.lines.push(Line::text(text.clone(), vec![id]));
            f.words.push(Word {
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
            f.bounds.push(NativeBounds::Glyphs(b));
            f.axes.push(NativeAxis::Down);
            first = after;
        }
        f.lines.rotate_left(1);
        f
    }
    fn prove(f: &Fixture) -> Option<Vec<Line>> {
        ordered(
            &f.lines,
            &f.words,
            &f.bounds,
            &f.axes,
            &f.flows,
            &crate::native_source_scene::Scene::for_test(&f.words),
        )
    }
    #[test]
    fn source_order_is_independent_of_tier_storage_height_column_count_scale_and_origin() {
        for count in [2, 3, 17, 37] {
            for scale in [0.125, 1., 64.] {
                for offset in [-256., 0., 1024.] {
                    let mut f = fixture(count, scale, offset, 4.);
                    let rows = prove(&f).unwrap();
                    assert_eq!(
                        rows.iter()
                            .flat_map(|l| l.words.iter().copied())
                            .collect::<Vec<_>>(),
                        (0..count).collect::<Vec<_>>()
                    );
                    let expected = rows;
                    f.lines.reverse();
                    if count > 2 {
                        assert_eq!(prove(&f).unwrap(), expected);
                    } // count2 is already physically ordered after reversal
                }
            }
        }
    }
    #[test]
    fn unknown_material_ownership_and_source_paths_cannot_choose_a_plausible_order() {
        for mode in 0..8 {
            let mut f = fixture(3, 1., 0., 4.);
            match mode {
                0 => f.axes[1] = NativeAxis::Unknown,
                1 => f.flows[1] = None,
                2 => f.bounds[1] = NativeBounds::Unknown,
                3 => {
                    let id = f.lines[0].words[0];
                    f.lines[0].words.push(id);
                }
                4 => f.lines[0].text.push('誤'),
                5 => f.words[1].status = WordStatus::Noise,
                6 => {
                    f.lines.pop();
                }
                _ => {
                    f.lines[0].text = "〇乙庁告示第二号".into();
                    f.words[1].text = f.lines[0].text.clone();
                }
            }
            assert!(prove(&f).is_none(), "mode {mode}");
        }
    }
    #[test]
    fn overlapping_parallel_columns_and_overprinted_flow_have_no_unique_physical_order() {
        for step in [0., 0.125, 1.] {
            let f = fixture(3, 1., 0., step);
            assert!(prove(&f).is_none());
        }
    }
    #[test]
    fn a_dropped_foreign_occurrence_inside_the_support_is_opaque() {
        for x in [94., 1000.] {
            let mut f = fixture(3, 1., 0., 4.);
            let b = [x, 40., x + 0.5, 41.];
            let id = f.words.len();
            f.words.push(Word {
                id,
                x0: b[0],
                y0: b[1],
                x1: b[2],
                y1: b[3],
                source_box: b,
                raw: "foreign".into(),
                text: "foreign".into(),
                status: WordStatus::Noise,
            });
            f.bounds.push(NativeBounds::Glyphs(b));
            f.axes.push(NativeAxis::Unknown);
            f.flows.push(None);
            assert_eq!(prove(&f).is_none(), x == 94.);
        }
    }
}
