//! Bind a continuation to a proven five-field source header and native band.
//! Record count and existing Word/Line/Unit partitions do not determine schema.
use crate::lines::PageRules;
use crate::trace::{Line, Unit, Word, WordStatus};
use std::collections::{HashMap, HashSet};
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn cx(w: &Word) -> f32 {
    (w.x0 + w.x1) * 0.5
}
fn code(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| "〇一二三四五六七八九０１２３４５６７８９0123456789".contains(c))
}
fn walk(u: &Unit, f: &mut impl FnMut(&Line)) {
    match u {
        Unit::Plain(ls) => ls.iter().for_each(f),
        Unit::Shinkyu {
            preamble,
            after,
            before,
            outside,
        ..} => {
            preamble.iter().for_each(&mut *f);
            for ls in after.iter().chain(before).chain(outside) {
                ls.iter().for_each(&mut *f)
            }
        }
    }
}
fn ownership(us: &[Unit]) -> HashMap<usize, usize> {
    let mut out = HashMap::new();
    for u in us {
        walk(u, &mut |l| {
            for id in l
                .words
                .iter()
                .chain(l.table.iter().flat_map(|t| &t.cells).flat_map(|c| &c.words))
            {
                *out.entry(*id).or_insert(0) += 1;
            }
        })
    }
    out
}
fn source<'a>(
    l: &Line,
    by: &HashMap<usize, &'a Word>,
    own: &HashMap<usize, usize>,
) -> Option<Vec<&'a Word>> {
    if l.table.is_some() || l.words.is_empty() {
        return None;
    }
    let ws = l
        .words
        .iter()
        .map(|id| by.get(id).copied())
        .collect::<Option<Vec<_>>>()?;
    if ws.iter().any(|w| {
        w.status != WordStatus::Used
            || w.text.trim().is_empty()
            || own.get(&w.id) != Some(&1)
            || ![w.x0, w.x1, w.y0, w.y1].iter().all(|v| v.is_finite())
            || w.x1 <= w.x0
            || w.y1 <= w.y0
    }) {
        return None;
    }
    (compact(&ws.iter().map(|w| w.text.as_str()).collect::<String>()) == compact(&l.text))
        .then_some(ws)
}
const LABELS: [&str; 5] = ["施設番号", "施設名", "所在地名", "所有関係", "摘要"];
#[derive(Clone)]
struct Header {
    starts: [f32; 5],
    em: f32,
    ids: HashSet<usize>,
    last: usize,
}
struct Flat<'a> {
    unit: usize,
    line: usize,
    value: &'a Line,
}
struct Plan {
    start: (usize, usize),
    end: (usize, usize),
    lines: Vec<Line>,
}
fn header(
    flat: &[Flat<'_>],
    by: &HashMap<usize, &Word>,
    own: &HashMap<usize, usize>,
    rules: &PageRules,
) -> Option<Header> {
    let all: Vec<_> = flat
        .iter()
        .filter_map(|p| source(p.value, by, own))
        .flatten()
        .collect();
    let mut candidates = vec![];
    for first in all
        .iter()
        .filter(|w| LABELS[0].starts_with(&compact(&w.text)))
    {
        let em = first.x1 - first.x0;
        if em <= 0. {
            continue;
        }
        let mut ws: Vec<_> = all
            .iter()
            .copied()
            .filter(|w| (cx(w) - cx(first)).abs() <= em * 0.35 && w.y0 >= first.y0)
            .collect();
        ws.sort_by(|a, b| a.y0.total_cmp(&b.y0));
        let mut at = 0;
        let mut starts = [0.; 5];
        let mut ids = HashSet::new();
        let mut last_end = f32::NEG_INFINITY;
        let mut valid = true;
        for (f, label) in LABELS.iter().enumerate() {
            let Some(first) = ws.get(at) else {
                valid = false;
                break;
            };
            starts[f] = first.y0;
            if first.y0 <= last_end {
                valid = false;
                break;
            }
            let mut s = String::new();
            while s.len() < label.len() {
                let Some(w) = ws.get(at) else {
                    valid = false;
                    break;
                };
                if (w.x1 - w.x0 - em).abs() > em * 0.25
                    || w.y0 < last_end - em * 0.1
                    || !ids.insert(w.id)
                {
                    valid = false;
                    break;
                }
                s.push_str(&compact(&w.text));
                last_end = w.y1;
                at += 1;
            }
            if s != *label {
                valid = false;
                break;
            }
        }
        if !valid {
            continue;
        }
        // The native separator closes this header domain. A later article at the
        // same x below that rule is unrelated; an extra field above it is unknown.
        let next_rule = rules
            .horizontal
            .iter()
            .filter(|r| r.y >= last_end && r.x0 <= first.x0 && r.x1 >= first.x1)
            .map(|r| r.y)
            .min_by(f32::total_cmp);
        if ws
            .get(at)
            .is_some_and(|w| next_rule.is_none_or(|y| w.y0 < y))
        {
            continue;
        }
        // A source line may be split, but must not mix header and body words.
        if flat.iter().any(|p| {
            p.value.words.iter().any(|id| ids.contains(id))
                && p.value.words.iter().any(|id| !ids.contains(id))
        }) {
            continue;
        }
        let Some(last) = flat
            .iter()
            .rposition(|p| p.value.words.iter().any(|id| ids.contains(id)))
        else {
            continue;
        };
        if !candidates.iter().any(|h: &Header| h.ids == ids) {
            candidates.push(Header {
                starts,
                em,
                ids,
                last,
            });
        }
    }
    (candidates.len() == 1).then(|| candidates.remove(0))
}
fn heading(l: &Line) -> bool {
    l.text.trim_start().starts_with(['〇', '○', '◎'])
}
fn propose(
    flat: &[Flat<'_>],
    all: &[Word],
    by: &HashMap<usize, &Word>,
    own: &HashMap<usize, usize>,
    rules: &PageRules,
    width: f32,
) -> Option<Plan> {
    let h = header(flat, by, own, rules)?;
    let e = h.em;
    let mut plans = vec![];
    for top in &rules.horizontal {
        for bottom in &rules.horizontal {
            if bottom.y <= top.y
                || top.y <= h.starts[4]
                || top.x0 > bottom.x1
                || bottom.x0 > top.x1
                || top.x1 - top.x0 < width * 0.6
                || bottom.x1 - bottom.x0 < width * 0.6
            {
                continue;
            }
            // Text em boxes can overhang the stroked cap endpoint. The later common
            // coverage check bounds that overhang; no center-only owner assignment is used.
            let in_band = |w: &&Word| {
                w.y0 >= top.y
                    && w.y1 <= bottom.y
                    && w.x0 >= top.x0.max(bottom.x0) - e * 0.5
                    && w.x1 <= top.x1.min(bottom.x1) + e * 0.5
            };
            let start = flat.iter().enumerate().skip(h.last + 1).find_map(|(i, p)| {
                source(p.value, by, own)
                    .filter(|ws| {
                        ws.iter()
                            .any(|w| in_band(&w) && code(&w.text) && w.y0 - top.y < e * 2.)
                    })
                    .map(|_| i)
            });
            let Some(start) = start else { continue };
            if flat[h.last + 1..start]
                .iter()
                .any(|p| p.value.table.is_some() || heading(p.value))
            {
                continue;
            }
            // The printed header must already govern an upper record, not an isolated legend.
            if !flat[h.last + 1..start]
                .iter()
                .filter_map(|p| source(p.value, by, own))
                .flatten()
                .any(|w| code(&w.text) && (w.y0 - h.starts[0]).abs() <= e * 0.5)
            {
                continue;
            }
            let mut end = start;
            let mut rows = vec![];
            while let Some(p) = flat.get(end) {
                if heading(p.value) {
                    break;
                }
                let Some(ws) = source(p.value, by, own) else {
                    break;
                };
                if !ws.iter().all(in_band) {
                    break;
                }
                rows.push(ws);
                end += 1;
            }
            if end == start {
                continue;
            }
            let ws: Vec<_> = rows.iter().flatten().copied().collect();
            let mut axes: Vec<f32> = ws
                .iter()
                .filter(|w| code(&w.text) && w.y0 - top.y < e * 2.)
                .map(|w| cx(w))
                .collect();
            axes.sort_by(|a, b| b.total_cmp(a));
            axes.dedup_by(|a, b| (*a - *b).abs() <= e * 0.35);
            if axes.is_empty() || axes.windows(2).any(|p| p[0] - p[1] <= e * 1.5) {
                continue;
            }
            let number_top = ws
                .iter()
                .filter(|w| code(&w.text) && w.y0 - top.y < e * 2.)
                .map(|w| w.y0)
                .fold(f32::INFINITY, f32::min);
            let shift = number_top - h.starts[0];
            let starts: Option<Vec<Vec<f32>>> = axes
                .iter()
                .map(|&x| {
                    (0..5)
                        .map(|f| {
                            ws.iter()
                                .filter(|w| {
                                    (cx(w) - x).abs() <= e * 0.35
                                        && (w.y0 - h.starts[f] - shift).abs() <= e * 2.
                                })
                                .map(|w| w.y0)
                                .min_by(f32::total_cmp)
                        })
                        .collect()
                })
                .collect();
            let Some(starts) = starts else { continue };
            if (0..5).any(|f| {
                starts
                    .iter()
                    .any(|s| (s[f] - starts[0][f]).abs() > e * 0.35)
            }) {
                continue;
            }
            // A title beginning in the same open remarks lane prevents a complete source
            // band from being certified. Do not repair only the prefix before that title.
            if flat.get(end).is_some_and(|p| {
                heading(p.value)
                    && p.value
                        .words
                        .iter()
                        .filter_map(|id| by.get(id))
                        .any(|w| in_band(&w) && (w.y0 - starts[0][4]).abs() <= e * 0.35)
            }) {
                continue;
            }
            let mut offsets: Vec<_> = (0..5).map(|f| starts[0][f] - h.starts[f]).collect();
            offsets.sort_by(f32::total_cmp);
            if offsets.iter().any(|v| (*v - offsets[2]).abs() > e * 1.25) {
                continue;
            }
            let mut fields: Vec<Vec<Vec<&Word>>> = vec![vec![vec![]; 5]; axes.len()];
            let mut assigned = HashMap::new();
            let mut last_note_row = 0;
            let mut last_note_x = f32::INFINITY;
            let mut valid = true;
            for w in &ws {
                let Some(f) = (0..5).rfind(|&f| w.y0 >= starts[0][f] - e * 0.35) else {
                    valid = false;
                    break;
                };
                if f < 4 && w.y1 > starts[0][f + 1] + e * 0.35 {
                    valid = false;
                    break;
                }
                let row = if f < 4 {
                    let rs: Vec<_> = axes
                        .iter()
                        .enumerate()
                        .filter(|(_, x)| (cx(w) - **x).abs() <= e * 0.35)
                        .map(|(i, _)| i)
                        .collect();
                    if rs.len() != 1 {
                        valid = false;
                        break;
                    }
                    rs[0]
                } else {
                    let Some(r) = axes.iter().rposition(|a| cx(w) <= *a + e * 0.35) else {
                        valid = false;
                        break;
                    };
                    if r < last_note_row || (r == last_note_row && cx(w) > last_note_x + e * 0.35) {
                        valid = false;
                        break;
                    }
                    if fields[r][4].is_empty() && (cx(w) - axes[r]).abs() > e * 0.35 {
                        valid = false;
                        break;
                    }
                    last_note_row = r;
                    last_note_x = cx(w);
                    r
                };
                if (w.x1 - w.x0 - e).abs() > e * 0.25 || assigned.insert(w.id, (row, f)).is_some() {
                    valid = false;
                    break;
                }
                fields[row][f].push(*w);
            }
            if !valid || fields.iter().flatten().any(Vec::is_empty) {
                continue;
            }
            // Within one non-wrapping field, source fragments advance down one axis.
            // Coincident alternatives are not successive digits or a longer label.
            if fields.iter().any(|fs| {
                fs[..4]
                    .iter()
                    .any(|part| part.windows(2).any(|p| p[1].y0 < p[0].y1 - e * 0.1))
            }) {
                continue;
            }
            let codes: Vec<_> = fields
                .iter()
                .map(|fs| fs[0].iter().map(|w| w.text.as_str()).collect::<String>())
                .collect();
            if codes.iter().any(|t| !code(t))
                || codes.iter().collect::<HashSet<_>>().len() != codes.len()
            {
                continue;
            }
            if fields.iter().any(|fs| {
                fs[1..4]
                    .iter()
                    .any(|f| f.iter().any(|w| w.text.contains(['。', '：'])))
            }) {
                continue;
            }
            // Names are field values. Distinct source-number axes and unique
            // original Word ownership establish records, even for equal names.
            let x0 = ws.iter().map(|w| w.x0).fold(f32::INFINITY, f32::min);
            let x1 = ws.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max);
            let y0 = ws.iter().map(|w| w.y0).fold(f32::INFINITY, f32::min);
            let y1 = ws.iter().map(|w| w.y1).fold(f32::NEG_INFINITY, f32::max);
            if top.x0 > x0 + e * 0.5
                || top.x1 < x1 - e * 0.5
                || bottom.x0 > x0 + e * 0.5
                || bottom.x1 < x1 - e * 0.5
                || bottom.y - y1 >= e * 2.
                || rules.horizontal.iter().any(|r| {
                    r.y > top.y + e * 0.5 && r.y < bottom.y - e * 0.5 && r.x0 < x1 && r.x1 > x0
                })
                || rules
                    .vertical
                    .iter()
                    .any(|r| r.x > x0 + e * 0.5 && r.x < x1 - e * 0.5 && r.y0 < y1 && r.y1 > y0)
            {
                continue;
            }
            // A record is complete only if every source box intersecting its
            // envelope is assigned. Empty/raw-empty and zero-dimensional Words
            // still carry source geometry. Normalize inverted boxes; a point on
            // an edge is an intersection. Unknown geometry cannot prove disjointness.
            if all.iter().any(|w| {
                if assigned.contains_key(&w.id) {
                    return false;
                }
                if ![w.x0, w.x1, w.y0, w.y1].iter().all(|v| v.is_finite()) {
                    return true;
                }
                w.x0.min(w.x1) <= x1
                    && w.x0.max(w.x1) >= x0
                    && w.y0.min(w.y1) <= y1
                    && w.y0.max(w.y1) >= y0
            }) {
                continue;
            }
            let mut text = vec![vec![String::new(); 5]; axes.len()];
            for (p, words) in flat[start..end].iter().zip(&rows) {
                let mut cursor = 0;
                let mut prev = None;
                for w in words {
                    let Some(rest) = p.value.text.get(cursor..) else {
                        valid = false;
                        break;
                    };
                    let Some(at) = rest.find(&w.text) else {
                        valid = false;
                        break;
                    };
                    if !rest[..at].chars().all(char::is_whitespace) {
                        valid = false;
                        break;
                    }
                    let target = assigned[&w.id];
                    if prev == Some(target) {
                        text[target.0][target.1].push_str(&rest[..at]);
                    }
                    text[target.0][target.1].push_str(&w.text);
                    prev = Some(target);
                    cursor += at + w.text.len();
                }
                if !p.value.text[cursor..].chars().all(char::is_whitespace) {
                    valid = false;
                    break;
                }
            }
            if !valid {
                continue;
            }
            let result: Vec<_> = fields
                .iter()
                .enumerate()
                .map(|(r, fs)| {
                    Line::text(
                        text[r].join("　"),
                        fs.iter().flatten().map(|w| w.id).collect(),
                    )
                })
                .collect();
            let mut before: Vec<_> = flat[start..end]
                .iter()
                .flat_map(|p| p.value.text.chars())
                .filter(|c| !c.is_whitespace())
                .collect();
            let mut after: Vec<_> = result
                .iter()
                .flat_map(|l| l.text.chars())
                .filter(|c| !c.is_whitespace())
                .collect();
            before.sort_unstable();
            after.sort_unstable();
            if before != after || result.iter().map(|l| l.words.len()).sum::<usize>() != ws.len() {
                continue;
            }
            if result.iter().eq(flat[start..end].iter().map(|p| p.value)) {
                continue;
            }
            plans.push(Plan {
                start: (flat[start].unit, flat[start].line),
                end: (flat[end - 1].unit, flat[end - 1].line + 1),
                lines: result,
            });
        }
    }
    (plans.len() == 1).then(|| plans.remove(0))
}
pub(crate) fn restore(
    units: &mut Vec<Unit>,
    words: &[Word],
    rules: Option<&PageRules>,
    width: f32,
    height: f32,
) {
    let Some(rules) = rules else { return };
    if !width.is_finite() || !height.is_finite() || width <= 0. || height <= width {
        return;
    }
    let by: HashMap<_, _> = words.iter().map(|w| (w.id, w)).collect();
    if by.len() != words.len() {
        return;
    }
    let own = ownership(units);
    let mut flat = vec![];
    let mut plans = vec![];
    for (ui, u) in units.iter().enumerate() {
        match u {
            Unit::Plain(ls) => {
                for (li, l) in ls.iter().enumerate() {
                    flat.push(Flat {
                        unit: ui,
                        line: li,
                        value: l,
                    });
                }
            }
            _ => {
                if let Some(p) = propose(&flat, words, &by, &own, rules, width) {
                    plans.push(p)
                }
                flat.clear();
            }
        }
    }
    if let Some(p) = propose(&flat, words, &by, &own, rules, width) {
        plans.push(p)
    }
    drop(flat);
    if plans.len() != 1 {
        return;
    }
    let p = plans.remove(0);
    let old = std::mem::take(units);
    let mut replacement = Some(p.lines);
    for (ui, u) in old.into_iter().enumerate() {
        if ui < p.start.0 || ui > p.end.0 {
            units.push(u);
            continue;
        }
        let Unit::Plain(ls) = u else { unreachable!() };
        let from = if ui == p.start.0 { p.start.1 } else { 0 };
        let to = if ui == p.end.0 { p.end.1 } else { ls.len() };
        if p.start.0 == p.end.0 {
            let mut out = ls[..from].to_vec();
            out.extend(replacement.take().unwrap());
            out.extend_from_slice(&ls[to..]);
            units.push(Unit::Plain(out));
        } else {
            if from > 0 {
                units.push(Unit::Plain(ls[..from].to_vec()))
            }
            if ui == p.start.0 {
                units.push(Unit::Plain(replacement.take().unwrap()))
            }
            if to < ls.len() {
                units.push(Unit::Plain(ls[to..].to_vec()))
            }
        }
    }
}
#[cfg(test)]
#[path = "facility_continuation_shape_tests.rs"]
mod tests;
