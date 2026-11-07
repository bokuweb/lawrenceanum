//! Two native quotation lanes continued across a physical column break.
//! The replacement operator, closed delimiters, complete original glyph ranges,
//! paired wrap edges and a central entry/exit are all mandatory. Ordinary nearby
//! columns and short agency-name annotations do not authorize this repair.
use crate::{
    native_column_flow::AxisFlow,
    native_word_axes::NativeAxis,
    trace::{Line, NativeBounds, Unit, Word, WordStatus},
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
struct Atom {
    id: usize,
    x: f64,
    start: f64,
    end: f64,
    em: f64,
    b: [f32; 4],
    range: (usize, usize),
}
#[derive(Clone)]
struct Ribbon {
    upper: Vec<usize>,
    lower: Vec<usize>,
    b: [f32; 4],
}
pub(crate) struct Plan {
    entry: usize,
    exit: usize,
    upper: Vec<usize>,
    lower: Vec<usize>,
}
impl Plan {
    pub(crate) fn members(&self) -> impl Iterator<Item = usize> + '_ {
        self.upper.iter().chain(&self.lower).copied()
    }
}
fn near(a: f64, b: f64, em: f64) -> bool {
    (a - b).abs() <= em * 0.05
}
fn chain(atoms: &[Atom], x: f64, start: f64, em: f64) -> Option<Vec<usize>> {
    let mut candidates = atoms
        .iter()
        .filter(|a| near(a.x, x, em) && near(a.em, em, em) && a.start >= start - em * 0.05)
        .collect::<Vec<_>>();
    candidates.sort_by(|a, b| a.start.total_cmp(&b.start));
    if !near(candidates.first()?.start, start, em) {
        return None;
    }
    let mut out = Vec::new();
    let mut end = start;
    for a in candidates {
        if a.start < end - em * 0.05 {
            return None;
        }
        if a.start - end > em * 1.25 {
            break;
        }
        out.push(a.id);
        end = a.end;
    }
    Some(out)
}
fn balanced(s: &str) -> bool {
    let mut stack = Vec::new();
    for c in s.chars() {
        match c {
            '「' => stack.push('」'),
            '『' => stack.push('』'),
            '（' => stack.push('）'),
            '(' => stack.push(')'),
            '［' => stack.push('］'),
            '[' => stack.push(']'),
            '」' | '』' | '）' | ')' | '］' | ']' => {
                if stack.pop() != Some(c) {
                    return false;
                }
            }
            _ => {}
        }
    }
    stack.is_empty()
}
fn atomize(
    words: &[Word],
    bounds: &[NativeBounds],
    axes: &[NativeAxis],
    flows: &[Option<AxisFlow>],
) -> Vec<Atom> {
    if [bounds.len(), axes.len(), flows.len()]
        .into_iter()
        .any(|n| n != words.len())
    {
        return Vec::new();
    }
    words
        .iter()
        .enumerate()
        .filter_map(|(id, w)| {
            if w.id != id
                || w.status != WordStatus::Used
                || w.text.is_empty()
                || axes[id] != NativeAxis::Down
            {
                return None;
            }
            let NativeBounds::Glyphs(b) = bounds[id] else {
                return None;
            };
            let f = flows[id]?;
            let [x, start, end] = f.vertical_path(b)?;
            let em = (b[2] - b[0]) as f64;
            (em.is_finite() && em > 0.).then_some(Atom {
                id,
                x,
                start,
                end,
                em,
                b,
                range: f.source_range(),
            })
        })
        .collect()
}
fn prove_one(
    entry: &Atom,
    atoms: &[Atom],
    words: &[Word],
    bounds: &[NativeBounds],
) -> Option<Plan> {
    if !words[entry.id].text.ends_with('を') {
        return None;
    }
    let em = entry.em;
    let get = |id: usize| atoms.iter().find(|a| a.id == id);
    let heads = atoms
        .iter()
        .filter(|a| {
            near(a.x, entry.x + em * 0.5, em)
                && near(a.start, entry.end, em)
                && words[a.id].text.starts_with('「')
        })
        .collect::<Vec<_>>();
    if heads.len() != 1 {
        return None;
    }
    let (mut base, mut upper_start, mut lower_start) =
        (entry.x, heads[0].start, heads[0].start + em);
    let mut ribbons = Vec::new();
    let mut seen = BTreeSet::new();
    let exit = loop {
        let upper = chain(atoms, base + em * 0.5, upper_start, em)?;
        let lower = chain(atoms, base - em * 0.5, lower_start, em)?;
        if upper.iter().chain(&lower).any(|id| !seen.insert(*id)) {
            return None;
        }
        let upper_end = get(*upper.last()?)?.end;
        let lower_end = get(*lower.last()?)?.end;
        let mut b = [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ];
        for id in upper.iter().chain(&lower) {
            let a = get(*id)?;
            b[0] = b[0].min(a.b[0]);
            b[1] = b[1].min(a.b[1]);
            b[2] = b[2].max(a.b[2]);
            b[3] = b[3].max(a.b[3]);
        }
        ribbons.push(Ribbon { upper, lower, b });
        let exits = atoms
            .iter()
            .filter(|a| {
                near(a.x, base, em)
                    && near(a.em, em, em)
                    && a.start >= upper_start
                    && a.start >= lower_start
                    && (upper_end.max(lower_end) - a.start).abs() <= em * 0.6
                    && words[a.id].text.starts_with('に')
            })
            .filter(|a| {
                chain(atoms, base, a.start, em).is_some_and(|ids| {
                    ids.iter()
                        .map(|&id| words[id].text.as_str())
                        .collect::<String>()
                        .contains("改め")
                })
            })
            .collect::<Vec<_>>();
        if exits.len() == 1 {
            break exits[0].id;
        }
        if !exits.is_empty() || !near(upper_end, lower_end, em) {
            return None;
        }
        // Both lanes finish at the same original edge and resume at the
        // original column top. No sentence or guessed reading role supplies it.
        let next = atoms
            .iter()
            .filter(|a| near(a.start, entry.start, em) && near(a.em, em, em))
            .filter_map(|a| {
                let center = a.x - em * 0.5;
                let step = base - center;
                if step < em * 1.8 || step > em * 3.5 {
                    return None;
                }
                chain(atoms, center - em * 0.5, entry.start, em)?;
                Some(center)
            })
            .collect::<Vec<_>>();
        if next.len() != 1 {
            return None;
        }
        base = next[0];
        upper_start = entry.start;
        lower_start = entry.start;
    };
    if ribbons.len() < 2 {
        return None;
    }
    let upper = ribbons
        .iter()
        .flat_map(|r| r.upper.iter().copied())
        .collect::<Vec<_>>();
    let lower = ribbons
        .iter()
        .flat_map(|r| r.lower.iter().copied())
        .collect::<Vec<_>>();
    let text = upper
        .iter()
        .chain(&lower)
        .map(|&id| words[id].text.as_str())
        .collect::<String>();
    if !text.starts_with('「') || !text.ends_with('」') || !balanced(&text) {
        return None;
    }
    // Every native glyph belongs to exactly one participating original Word.
    let mut ranges = seen
        .iter()
        .map(|&id| get(id).map(|a| a.range))
        .collect::<Option<Vec<_>>>()?;
    ranges.sort_unstable();
    if ranges.windows(2).any(|w| w[0].1 > w[1].0) {
        return None;
    }
    for r in &ribbons {
        for (id, w) in words.iter().enumerate() {
            if seen.contains(&id) || id == entry.id || id == exit {
                continue;
            }
            let b = match bounds.get(id) {
                Some(NativeBounds::Glyphs(b)) => *b,
                _ => w.source_box,
            };
            if !b.iter().all(|x| x.is_finite()) {
                return None;
            }
            if b[0] < r.b[2] && r.b[0] < b[2] && b[1] < r.b[3] && r.b[1] < b[3] {
                return None;
            }
        }
    }
    Some(Plan {
        entry: entry.id,
        exit,
        upper,
        lower,
    })
}
pub(crate) fn prove(
    words: &[Word],
    bounds: &[NativeBounds],
    axes: &[NativeAxis],
    flows: &[Option<AxisFlow>],
) -> Vec<Plan> {
    let atoms = atomize(words, bounds, axes, flows);
    let plans = atoms
        .iter()
        .filter_map(|entry| prove_one(entry, &atoms, words, bounds))
        .collect::<Vec<_>>();
    let mut counts = BTreeMap::new();
    for p in &plans {
        for id in p.members() {
            *counts.entry(id).or_insert(0usize) += 1;
        }
    }
    plans
        .into_iter()
        .filter(|p| p.members().all(|id| counts[&id] == 1))
        .collect()
}
fn line(ids: Vec<usize>, words: &[Word]) -> Line {
    Line::text(ids.iter().map(|&id| words[id].text.as_str()).collect(), ids)
}
pub(crate) fn restore(
    units: &mut [Unit],
    words: &[Word],
    bounds: &[NativeBounds],
    axes: &[NativeAxis],
    flows: &[Option<AxisFlow>],
) {
    for plan in prove(words, bounds, axes, flows) {
        let members = plan.members().collect::<BTreeSet<_>>();
        let mut owned = BTreeMap::new();
        for (ui, u) in units.iter().enumerate() {
            let ls = match u {
                Unit::Plain(ls) => ls.iter().collect::<Vec<_>>(),
                Unit::Shinkyu {
                    preamble,
                    after,
                    before,
                    outside,
                    ..
                } => preamble
                    .iter()
                    .chain(after.iter().flatten())
                    .chain(before.iter().flatten())
                    .chain(outside.iter().flatten())
                    .collect(),
            };
            for l in ls {
                for id in l.words.iter().copied().chain(
                    l.table
                        .iter()
                        .flat_map(|t| t.cells.iter().flat_map(|c| c.words.iter().copied())),
                ) {
                    owned.entry(id).or_insert_with(Vec::new).push(ui);
                }
            }
        }
        let all = members
            .iter()
            .copied()
            .chain([plan.entry, plan.exit])
            .collect::<Vec<_>>();
        if all
            .iter()
            .any(|id| owned.get(id).is_none_or(|owners| owners.len() != 1))
        {
            continue;
        }
        let ui = owned[&plan.entry][0];
        if all.iter().any(|id| owned[id][0] != ui) {
            continue;
        }
        let Unit::Plain(lines) = &mut units[ui] else {
            continue;
        };
        if lines
            .iter()
            .any(|l| l.table.is_some() && l.words.iter().any(|id| members.contains(id)))
        {
            continue;
        }
        let original = lines
            .iter()
            .flat_map(|l| l.words.iter().copied())
            .collect::<Vec<_>>();
        let mut rebuilt = Vec::new();
        for old in lines.iter() {
            if old.table.is_some() {
                rebuilt.push(old.clone());
                continue;
            }
            let mut pending = Vec::new();
            for &id in &old.words {
                if members.contains(&id) {
                    continue;
                }
                pending.push(id);
                if id == plan.entry {
                    rebuilt.push(line(std::mem::take(&mut pending), words));
                    rebuilt.push(line(plan.upper.clone(), words));
                    rebuilt.push(line(plan.lower.clone(), words));
                }
            }
            if !pending.is_empty() {
                if pending == old.words {
                    rebuilt.push(old.clone());
                } else {
                    rebuilt.push(line(pending, words));
                }
            }
        }
        let mut before = original;
        let mut after = rebuilt
            .iter()
            .flat_map(|l| l.words.iter().copied())
            .collect::<Vec<_>>();
        before.sort_unstable();
        after.sort_unstable();
        if before == after {
            *lines = rebuilt;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(scale: f64, dx: f64, dy: f64) -> (Vec<Word>, Vec<NativeBounds>, Vec<Atom>) {
        let data = [
            (100., 0., 32., "任意を"),
            (104., 32., 64., "「第一の条件（甲"),
            (96., 40., 64., "第二の条件（乙"),
            (84., 0., 16., "）"),
            (76., 0., 32., "）」"),
            (80., 28., 80., "に改める。"),
        ];
        let mut words = Vec::new();
        let mut bounds = Vec::new();
        let mut atoms = Vec::new();
        for (id, (x, y, end, text)) in data.into_iter().enumerate() {
            let b = [
                ((x - 4.) * scale + dx) as f32,
                (y * scale + dy) as f32,
                ((x + 4.) * scale + dx) as f32,
                (end * scale + dy) as f32,
            ];
            words.push(Word {
                id,
                x0: b[0],
                y0: b[1],
                x1: b[2],
                y1: b[3],
                source_box: b,
                raw: text.into(),
                text: text.into(),
                status: WordStatus::Used,
            });
            bounds.push(NativeBounds::Glyphs(b));
            atoms.push(Atom {
                id,
                x: x * scale + dx,
                start: y * scale + dy,
                end: end * scale + dy,
                em: 8. * scale,
                b,
                range: (id * 20, id * 20 + 10),
            });
        }
        (words, bounds, atoms)
    }
    #[test]
    fn arbitrary_quoted_words_and_affine_source_coordinates_keep_the_two_lanes() {
        for (scale, dx, dy) in [(1., 0., 0.), (0.6, 23., 41.), (1.7, 100., 72.)] {
            let (words, bounds, atoms) = fixture(scale, dx, dy);
            let p = prove_one(&atoms[0], &atoms, &words, &bounds).unwrap();
            assert_eq!(p.upper, [1, 3]);
            assert_eq!(p.lower, [2, 4]);
            assert_eq!(p.exit, 5);
            assert!(
                prove(
                    &words,
                    &bounds,
                    &vec![NativeAxis::Down; words.len()],
                    &vec![None; words.len()]
                )
                .is_empty(),
                "geometry alone is not native provenance"
            );
        }
    }
    #[test]
    fn incomplete_wraps_unclosed_delimiters_and_unowned_intersections_are_rejected() {
        let (mut words, bounds, mut atoms) = fixture(1., 0., 0.);
        atoms[2].end -= 8.;
        assert!(prove_one(&atoms[0], &atoms, &words, &bounds).is_none());
        let (_, _, atoms) = fixture(1., 0., 0.);
        words[4].text = "）".into();
        assert!(prove_one(&atoms[0], &atoms, &words, &bounds).is_none());
        let (mut words, mut bounds, atoms) = fixture(1., 0., 0.);
        let mut ghost = words[2].clone();
        ghost.id = words.len();
        ghost.source_box = [96., 48., 100., 56.];
        words.push(ghost);
        bounds.push(NativeBounds::Unknown);
        assert!(prove_one(&atoms[0], &atoms, &words, &bounds).is_none());
    }
    #[test]
    fn original_wrapped_replacement_keeps_both_quotation_lanes_in_reading_order() {
        let pdf = include_bytes!("../tests/data/workbook/case-13.pdf");
        let inputs = crate::page_inputs(pdf).unwrap();
        let a = crate::analyze_item(&inputs[..1]);
        let text = a.articles[0].text.split_whitespace().collect::<String>();
        assert!(
            text.contains("第二十五条の八－第二十五条の十五）"),
            "{text}"
        );
        assert!(text.contains("特定暗号資産の譲渡による所得の課税の特例等（第二十五条の十五の二・第二十五条の十五の三）」に、"),"{text}");
        assert!(!text.contains("五）　に、"));
    }
}
