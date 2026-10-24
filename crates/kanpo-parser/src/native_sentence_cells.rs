//! Keep source sentence scope across inferred logical row cuts.
use crate::{
    grid::Grid,
    lines::PageRules,
    native_word_axes::NativeAxis,
    native_word_rulings::NativeRuling,
    trace::{NativeBounds, Table, TableCell, Unit, Word, WordStatus},
    vertical::Col,
};
pub(crate) struct Source<'a> {
    pub(crate) flows: Option<&'a[Option<crate::native_word_flow::NativeFlow>]>,
    pub(crate) words: &'a [Word],
    pub(crate) originals: &'a [Col],
    pub(crate) bounds: &'a [NativeBounds],
    pub(crate) axes: &'a [NativeAxis],
    pub(crate) rulings: &'a [NativeRuling],
    pub(crate) grids: &'a [Grid],
    pub(crate) rules: &'a PageRules,
}
include!("native_sentence_plan.rs");
pub(crate) fn restore(units: &mut [Unit], source: Source<'_>) {
    let mut counts = std::collections::HashMap::new();
    fn visit(units: &[Unit], mut f: impl FnMut(&[crate::trace::Line])) {
        for u in units {
            match u {
                Unit::Plain(ls) => f(ls),
                Unit::Shinkyu {
                    preamble,
                    after,
                    before,
                    outside,
                } => {
                    f(preamble);
                    for ls in after.iter().chain(before).chain(outside) {
                        f(ls);
                    }
                }
            }
        }
    }
    visit(units, |ls| {
        for l in ls {
            for id in &l.words {
                *counts.entry(*id).or_insert(0usize) += 1;
            }
            if let Some(t) = &l.table {
                for id in t.cells.iter().flat_map(|c| &c.words) {
                    *counts.entry(*id).or_insert(0usize) += 1;
                }
            }
        }
    });
    let repair = |ls: &mut Vec<crate::trace::Line>| {
        for l in ls {
            let Some(t) = &mut l.table else { continue };
            if !l.words.is_empty()
                || t.cells
                    .iter()
                    .flat_map(|c| &c.words)
                    .any(|id| counts.get(id) != Some(&1))
            {
                continue;
            }
            if let Some(next) = source_plan(
                t,
                Source {
                    flows: source.flows,
                    words: source.words,
                    originals: source.originals,
                    bounds: source.bounds,
                    axes: source.axes,
                    rulings: source.rulings,
                    grids: source.grids,
                    rules: source.rules,
                },
            ) {
                *t = next;
                l.text = t.render();
            }
        }
    };
    for u in units {
        match u {
            Unit::Plain(ls) => repair(ls),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            } => {
                repair(preamble);
                for ls in after.iter_mut().chain(before).chain(outside) {
                    repair(ls);
                }
            }
        }
    }
}
