//! Source-caption provenance and native leading comparison-frame continuation.
use crate::{
    grid::Grid,
    lines::PageRules,
    native_word_axes::NativeAxis,
    trace::{Line, NativeBounds, PageTrace, Region, Role, Unit, Word, WordStatus},
    vertical::Col,
};
use std::collections::{BTreeMap, HashSet};
pub(crate) struct Plan {
    pub(crate) after: Vec<usize>,
    pub(crate) before: Vec<usize>,
}
#[derive(Clone, Copy)]
pub(crate) struct Source<'a> {
    pub(crate) inherited_explicit_roles: bool,
    pub(crate) words: &'a [Word],
    pub(crate) cols: &'a [Col],
    pub(crate) bounds: &'a [NativeBounds],
    pub(crate) axes: &'a [NativeAxis],
    pub(crate) grids: &'a [Grid],
    pub(crate) rules: &'a PageRules,
}
pub(crate) enum SceneAtom {
    Pair(Plan),
    Body(Vec<usize>),
}
pub(crate) struct RoleSource<'a> {
    pub(crate) regions: &'a [Region],
    pub(crate) words: &'a [Word],
    pub(crate) bounds: &'a [NativeBounds],
    pub(crate) axes: &'a [NativeAxis],
}
include!("native_role_plan.rs");
include!("native_role_scene.rs");
include!("native_role_captions.rs");
pub(crate) fn explicit_page(p: &PageTrace) -> bool {
    explicit_pair(RoleSource {
        regions: &p.regions,
        words: &p.words,
        bounds: &p.native_bounds,
        axes: &p.native_axes,
    })
}
#[derive(Default)]
pub(crate) struct Restored {
    pub(crate) applied: bool,
    pub(crate) continues: bool,
}
fn visit(units: &[Unit], mut f: impl FnMut(&[Line])) {
    for u in units {
        match u {
            Unit::Plain(ls) => f(ls),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            ..} => {
                f(preamble);
                for ls in after.iter().chain(before).chain(outside) {
                    f(ls);
                }
            }
        }
    }
}
fn line_ids(lines: &[Line]) -> Vec<usize> {
    let mut ids = Vec::new();
    for l in lines {
        ids.extend_from_slice(&l.words);
        if let Some(t) = &l.table {
            for c in &t.cells {
                ids.extend_from_slice(&c.words);
            }
        }
    }
    ids
}
fn material_chars(lines: &[Line]) -> BTreeMap<char, usize> {
    let mut result = BTreeMap::new();
    for c in lines
        .iter()
        .flat_map(|l| l.text.chars())
        .filter(|c| !c.is_whitespace())
    {
        *result.entry(c).or_default() += 1;
    }
    result
}
fn source_chars(cols: &[Col]) -> BTreeMap<char, usize> {
    let mut result = BTreeMap::new();
    for c in cols
        .iter()
        .flat_map(|l| l.text.chars())
        .filter(|c| !c.is_whitespace())
    {
        *result.entry(c).or_default() += 1;
    }
    result
}
fn same_scope(lines: &[Line], ids: &[usize], source: Source<'_>) -> bool {
    let mut actual = line_ids(lines);
    actual.sort_unstable();
    let mut expected = ids.to_vec();
    expected.sort_unstable();
    if actual != expected || actual.windows(2).any(|w| w[0] == w[1]) {
        return false;
    }
    let wanted: HashSet<_> = ids.iter().copied().collect();
    let cols: Vec<_> = source
        .cols
        .iter()
        .filter(|c| wanted.contains(&c.id))
        .cloned()
        .collect();
    material_chars(lines) == source_chars(&cols)
}
pub(crate) fn restore(
    source: Source<'_>,
    units: &mut Vec<Unit>,
    mut ordinary: impl FnMut(Vec<Col>) -> Vec<Line>,
) -> Restored {
    let Some(pair) = source_plan(source) else {
        return Restored::default();
    };
    let Some(scene) = source_scene(source, pair) else {
        return Restored::default();
    };
    let continues = scene.len() == 1;
    let mut candidate = Vec::new();
    for atom in scene {
        match atom {
            SceneAtom::Pair(p) => {
                let fields = [p.after, p.before];
                let mut lines = Vec::new();
                for ids in &fields {
                    let wanted: HashSet<_> = ids.iter().copied().collect();
                    let cols: Vec<_> = source
                        .cols
                        .iter()
                        .filter(|c| wanted.contains(&c.id))
                        .cloned()
                        .collect();
                    let ls = crate::horizontal::row_lines(cols);
                    if !same_scope(&ls, ids, source)
                        || ls
                            .iter()
                            .flat_map(|l| &l.words)
                            .copied()
                            .collect::<Vec<_>>()
                            != *ids
                    {
                        return Restored::default();
                    }
                    lines.push(ls);
                }
                candidate.push(Unit::Shinkyu {labels:None,
                    preamble: vec![],
                    after: vec![lines.remove(0)],
                    before: vec![lines.remove(0)],
                    outside: vec![],
                });
            }
            SceneAtom::Body(ids) => {
                let wanted: HashSet<_> = ids.iter().copied().collect();
                let cols: Vec<_> = source
                    .cols
                    .iter()
                    .filter(|c| wanted.contains(&c.id))
                    .cloned()
                    .collect();
                let lines = ordinary(cols);
                if !same_scope(&lines, &ids, source) {
                    return Restored::default();
                }
                candidate.push(Unit::Plain(lines));
            }
        }
    }
    let mut ids = Vec::new();
    visit(&candidate, |ls| ids.extend(line_ids(ls)));
    ids.sort_unstable();
    let mut original: Vec<_> = source.cols.iter().map(|c| c.id).collect();
    original.sort_unstable();
    if ids != original || ids.windows(2).any(|w| w[0] == w[1]) {
        return Restored::default();
    }
    *units = candidate;
    Restored {
        applied: true,
        continues,
    }
}
