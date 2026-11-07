//! Keep independent comparison items apart when native line geometry rules out
//! an ordinary wrap. Bind by complete forward field positions, never spelling.
use kanpo_parser::{
    media::{Block, Document, Field, Inline, Table},
    trace::{Line, PageTrace, Role, WordStatus},
    Analysis,
};
use std::collections::{BTreeMap, BTreeSet};

fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

#[derive(Clone, Copy)]
struct Geometry {
    bounds: [f32; 4],
    em: f32,
    axis: usize,
}

fn geometry(page: &PageTrace, line: &Line, counts: &BTreeMap<usize, usize>) -> Option<Geometry> {
    if line.table.is_some() || line.words.is_empty() {
        return None;
    }
    let mut text = String::new();
    let mut boxes = vec![];
    for &id in &line.words {
        let w = page.words.get(id)?;
        if w.id != id || w.status != WordStatus::Used || counts.get(&id) != Some(&1) {
            return None;
        }
        text.push_str(&compact(&w.text));
        boxes.push(page.original_glyph_bounds(id)?);
    }
    if text != compact(&line.text) {
        return None;
    }
    let mut choices = vec![];
    for axis in [0, 1] {
        let cross = 1 - axis;
        let em = boxes
            .iter()
            .map(|b| b[cross + 2] - b[cross])
            .reduce(f32::min)?;
        let center = (boxes[0][cross] + boxes[0][cross + 2]) / 2.;
        if !boxes.iter().any(|b| b[axis + 2] - b[axis] > 1.5 * em)
            || boxes.iter().any(|b| {
                b[cross + 2] - b[cross] > 1.2 * em
                    || ((b[cross] + b[cross + 2]) / 2. - center).abs() > 0.25 * em
            })
            || boxes
                .windows(2)
                .any(|b| b[1][axis] < b[0][axis + 2] - 0.25 * em)
        {
            continue;
        }
        let mut bounds = boxes[0];
        for b in &boxes[1..] {
            for i in 0..2 {
                bounds[i] = bounds[i].min(b[i]);
                bounds[i + 2] = bounds[i + 2].max(b[i + 2]);
            }
        }
        choices.push(Geometry { bounds, em, axis });
    }
    (choices.len() == 1).then(|| choices[0])
}

fn independent(a: Geometry, b: Geometry, region: &[Option<Geometry>]) -> bool {
    if a.axis != b.axis || a.em > 1.2 * b.em || b.em > 1.2 * a.em {
        return false;
    }
    let axis = a.axis;
    let cross = 1 - axis;
    let em = a.em.min(b.em);
    let center = |g: Geometry| (g.bounds[cross] + g.bounds[cross + 2]) / 2.;
    // Several independent cells can occupy different portions of the same
    // physical column, even when the trace has not represented them as a table.
    // A longer line in another lane cannot certify this cell's available width.
    if region.iter().flatten().any(|left| {
        region.iter().flatten().any(|right| {
            left.axis == axis
                && right.axis == axis
                && (center(*left) - center(*right)).abs() <= 0.25 * left.em.min(right.em)
                && left.bounds[axis + 2] <= right.bounds[axis]
        })
    }) {
        return false;
    }
    let pitch = if axis == 1 {
        center(a) - center(b)
    } else {
        center(b) - center(a)
    };
    // Same leading anchor, adjacent physical lines, and enough unused space
    // to fit the entire next line plus one em (at least three em). Wrapping
    // an indivisible long token does not qualify. A longer line in this region proves
    // available flow space; page size or a desired Word width supplies none.
    let end = region
        .iter()
        .flatten()
        .filter(|g| g.axis == axis && g.em <= 1.2 * em && em <= 1.2 * g.em)
        .map(|g| g.bounds[axis + 2])
        .reduce(f32::max)
        .unwrap_or(a.bounds[axis + 2]);
    (a.bounds[axis] - b.bounds[axis]).abs() <= 0.25 * em
        && (0.8 * em..=2. * em).contains(&pitch)
        && end - a.bounds[axis + 2] >= (b.bounds[axis + 2] - b.bounds[axis] + em).max(3. * em)
}

pub(crate) fn apply(analysis: &Analysis, documents: &mut [Document]) {
    let mut streams = [String::new(), String::new()];
    let mut positions = [0, 0];
    let mut cuts: [BTreeSet<usize>; 2] = Default::default();
    for page in &analysis.pages {
        let mut counts = BTreeMap::new();
        for region in &page.regions {
            for line in &region.lines {
                for &id in &line.words {
                    *counts.entry(id).or_insert(0) += 1;
                }
            }
        }
        for region in &page.regions {
            let fi = match region.role {
                Role::After => 0,
                Role::Before => 1,
                _ => continue,
            };
            let geometry = region
                .lines
                .iter()
                .map(|l| geometry(page, l, &counts))
                .collect::<Vec<_>>();
            for (i, line) in region.lines.iter().enumerate() {
                if let Some((a, b)) = i
                    .checked_sub(1)
                    .and_then(|j| Some((geometry[j]?, geometry[i]?)))
                {
                    if independent(a, b, &geometry) {
                        cuts[fi].insert(positions[fi]);
                    }
                }
                let text = compact(&line.text);
                positions[fi] += text.chars().count();
                streams[fi].push_str(&text);
            }
        }
    }
    for (fi, field) in [Field::After, Field::Before].into_iter().enumerate() {
        if cuts[fi].is_empty()
            || streams[fi] != kanpo_parser::source_underlines::field_text(analysis, field)
            || streams[fi]
                != documents
                    .iter()
                    .map(|d| d.field_text(field))
                    .collect::<String>()
        {
            continue;
        }
        let mut cursor = 0;
        for doc in &mut *documents {
            for block in &mut doc.blocks {
                if let Block::Shinkyu { rows, .. } = block {
                    for row in rows {
                        let (runs, table) = if fi == 0 {
                            (&mut row.after, &mut row.after_table)
                        } else {
                            (&mut row.before, &mut row.before_table)
                        };
                        insert(runs, &cuts[fi], &mut cursor);
                        if let Some(table) = table {
                            insert_table(table, &cuts[fi], &mut cursor);
                        }
                    }
                }
            }
        }
    }
}

fn insert_table(table: &mut Table, cuts: &BTreeSet<usize>, cursor: &mut usize) {
    for row in &mut table.rows {
        for cell in row {
            insert(cell, cuts, cursor);
        }
    }
}
fn insert(runs: &mut [Inline], cuts: &BTreeSet<usize>, cursor: &mut usize) {
    let mut at_start = true;
    for run in runs {
        match run {
            Inline::Text { text, .. } => {
                let mut result = String::new();
                for c in text.chars() {
                    if !c.is_whitespace() {
                        if cuts.contains(cursor) && !at_start {
                            result.push('\n');
                        }
                        *cursor += 1;
                        at_start = false;
                    } else if c == '\n' {
                        at_start = true;
                    }
                    result.push(c);
                }
                *text = result;
            }
            Inline::Table { table } => {
                insert_table(table, cuts, cursor);
                at_start = true;
            }
            Inline::Image { .. } => {
                at_start = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spare_flow_space_and_same_anchor_work_in_both_axes_at_arbitrary_scale() {
        for axis in [0, 1] {
            for (scale, dx, dy) in [(0.6, 42., 71.), (1., 0., 0.), (1.8, 93., 23.)] {
                let g = |start: f32, end: f32, cross: f32| {
                    let b = if axis == 0 {
                        [start, cross, end, cross + 8.]
                    } else {
                        [cross, start, cross + 8., end]
                    };
                    Geometry {
                        bounds: [
                            b[0] * scale + dx,
                            b[1] * scale + dy,
                            b[2] * scale + dx,
                            b[3] * scale + dy,
                        ],
                        em: 8. * scale,
                        axis,
                    }
                };
                let a = g(16., 72., 40.);
                let b = g(16., 112., if axis == 0 { 52. } else { 28. });
                let region = [Some(g(0., 320., 80.)), Some(a), Some(b)];
                assert!(independent(a, b, &region));
                let parallel = [region.as_slice(), &[Some(g(128., 320., 40.))]].concat();
                assert!(
                    !independent(a, b, &parallel),
                    "another cell in the same physical column has an unknown separator"
                );
                assert!(
                    !independent(g(16., 312., 40.), b, &region),
                    "filled line wraps normally"
                );
                assert!(
                    !independent(g(16., 224., 40.), b, &region),
                    "next line would not fit the remaining space"
                );
                assert!(
                    !independent(a, g(32., 112., if axis == 0 { 52. } else { 28. }), &region),
                    "changed leading anchor"
                );
                assert!(
                    !independent(
                        a,
                        b,
                        &[
                            Some(a),
                            Some(g(16., 72., if axis == 0 { 52. } else { 28. }))
                        ]
                    ),
                    "no proof of unused flow space"
                );
                assert!(
                    !independent(a, g(16., 112., 80.), &region),
                    "not an adjacent physical line"
                );
            }
        }
    }

    #[test]
    fn unknown_occurrences_and_changed_complete_fields_cannot_supply_breaks() {
        let pdf = include_bytes!("../../kanpo-parser/tests/data/workbook/case-10-listed-3.pdf");
        let a = crate::analyze(pdf, &[]).unwrap();
        let docs = || {
            a.articles
                .iter()
                .map(|a| Document::from(&a.document))
                .collect::<Vec<_>>()
        };
        let mut documents = docs();
        apply(&a, &mut documents);
        assert_ne!(
            serde_json::to_value(&documents).unwrap(),
            serde_json::to_value(docs()).unwrap()
        );
        let once = serde_json::to_value(&documents).unwrap();
        apply(&a, &mut documents);
        assert_eq!(
            serde_json::to_value(&documents).unwrap(),
            once,
            "existing paragraphs must not acquire extra empty lines"
        );
        let mut unknown = a.clone();
        for page in &mut unknown.pages {
            for word in &mut page.words {
                word.id += 100000;
            }
        }
        let mut documents = docs();
        let before = serde_json::to_value(&documents).unwrap();
        apply(&unknown, &mut documents);
        assert_eq!(serde_json::to_value(&documents).unwrap(), before);
        let mut documents = docs();
        for doc in &mut documents {
            for block in &mut doc.blocks {
                if let Block::Shinkyu { rows, .. } = block {
                    for row in rows {
                        row.after.push(Inline::Text {
                            text: "異なる原文".into(),
                            underline: false,
                        });
                        row.before.push(Inline::Text {
                            text: "異なる原文".into(),
                            underline: false,
                        });
                    }
                }
            }
        }
        let before = serde_json::to_value(&documents).unwrap();
        apply(&a, &mut documents);
        assert_eq!(serde_json::to_value(&documents).unwrap(), before);
    }

    #[test]
    fn narrow_parallel_cells_keep_their_wrapped_sentences_together() {
        let pdf = include_bytes!("../../kanpo-parser/tests/data/workbook/case-15.pdf");
        let a = crate::analyze(pdf, &[]).unwrap();
        let mut documents = a
            .articles
            .iter()
            .map(|a| Document::from(&a.document))
            .collect::<Vec<_>>();
        let before = serde_json::to_value(&documents).unwrap();
        apply(&a, &mut documents);
        assert_eq!(serde_json::to_value(&documents).unwrap(), before);
    }
}
