//! Source line moves plus exclusive native ownership and row counterevidence.
use crate::{
    native_cell_origin::Origin,
    trace::{Line, NativeBounds, TableCell, Unit, Word, WordStatus},
};
use std::collections::{BTreeMap, BTreeSet};
type Edge = (usize, usize, usize);
struct Record {
    raw: String,
    legacy: [f32; 4],
    first: usize,
    after: usize,
    moves: BTreeSet<Edge>,
}
struct Ink {
    center: Option<[f32; 4]>,
    paint: [f32; 4],
}
pub(crate) struct Scene {
    ink: Vec<Ink>,
    obstacles: Vec<[f64; 4]>,
    total: usize,
    ignored: BTreeSet<usize>,
    records: Vec<Record>,
    edges: BTreeSet<Edge>,
}
fn valid(b: [f32; 4]) -> bool {
    b.iter().all(|n| n.is_finite()) && b[0] < b[2] && b[1] < b[3]
}
fn within(b: [f32; 4], o: [f32; 4]) -> bool {
    o[0] <= b[0] && o[1] <= b[1] && b[2] <= o[2] && b[3] <= o[3]
}
fn overlaps(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}
fn hex(raw: &str) -> Option<String> {
    if raw.len() % 2 != 0 || !raw.is_ascii() {
        return None;
    }
    String::from_utf8(
        (0..raw.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&raw[i..i + 2], 16).ok())
            .collect::<Option<Vec<_>>>()?,
    )
    .ok()
}
pub(crate) fn for_page(page: scraper::ElementRef) -> Option<Scene> {
    let a = page.value();
    let total = a.attr("data-native-glyph-count")?.parse::<usize>().ok()?;
    let c = a
        .attr("data-native-lineage-coverage")?
        .split_whitespace()
        .collect::<Vec<_>>();
    if c.len() != 3 || c[0] != "1" || c[1].parse::<usize>().ok() != Some(total) {
        return None;
    }
    let obs = a
        .attr("data-native-lineage-obstacles")?
        .split_whitespace()
        .collect::<Vec<_>>();
    if obs.len() < 2 || obs[0] != "1" {
        return None;
    }
    let mut obstacles = vec![];
    if obs[1] != "-" {
        for s in &obs[1..] {
            let b = s
                .split(',')
                .map(|x| x.parse::<f64>().ok())
                .collect::<Option<Vec<_>>>()?;
            if b.len() != 4 || !b.iter().all(|x| x.is_finite()) || b[0] >= b[2] || b[1] >= b[3] {
                return None;
            }
            obstacles.push([b[0], b[1], b[2], b[3]])
        }
    } else if obs.len() != 2 {
        return None;
    }
    let raw_ink = a
        .attr("data-native-lineage-ink")?
        .split_whitespace()
        .collect::<Vec<_>>();
    if raw_ink.len() < 2 || raw_ink[0] != "1" {
        return None;
    }
    let mut ink = vec![];
    if raw_ink[1] != "-" {
        for item in &raw_ink[1..] {
            let (kind, values) = item.split_once(':')?;
            let v = values
                .split(',')
                .map(|x| x.parse::<f32>().ok())
                .collect::<Option<Vec<_>>>()?;
            if v.iter().any(|n| !n.is_finite()) {
                return None;
            }
            let (center, paint) = match (kind, v.as_slice()) {
                ("B", [a, b, c, d]) => (None, [*a, *b, *c, *d]),
                ("L", [a, b, c, d, e, f, g, h]) => {
                    let center = [*a, *b, *c, *d];
                    if !((*a == *c && *b < *d) || (*b == *d && *a < *c)) {
                        return None;
                    }
                    (Some(center), [*e, *f, *g, *h])
                }
                _ => return None,
            };
            if !valid(paint) || center.is_some_and(|b| !within(b, paint)) {
                return None;
            }
            ink.push(Ink { center, paint });
        }
    } else if raw_ink.len() != 2 {
        return None;
    }
    let mut ignored = BTreeSet::new();
    if c[2] != "-" {
        for s in c[2].split(',') {
            let id = s.parse::<usize>().ok()?;
            if id >= total || !ignored.insert(id) {
                return None;
            }
        }
    }
    let mut intervals = ignored.iter().map(|&id| (id, id + 1)).collect::<Vec<_>>();
    let mut records = vec![];
    let mut edges = BTreeSet::new();
    for w in page.select(&scraper::Selector::parse("word").ok()?) {
        let a = w.value();
        let first = a
            .attr("data-native-flow-first-glyph")?
            .parse::<usize>()
            .ok()?;
        let after = a
            .attr("data-native-flow-after-glyph")?
            .parse::<usize>()
            .ok()?;
        if first >= after || after > total {
            return None;
        }
        intervals.push((first, after));
        let n = |k| a.attr(k)?.parse::<f32>().ok();
        let legacy = [n("xmin")?, n("ymin")?, n("xmax")?, n("ymax")?];
        if !valid(legacy) {
            return None;
        }
        let raw = w.text().collect::<String>();
        let mut moves = BTreeSet::new();
        if let Some(s) = a.attr("data-native-line-moves") {
            let v = s.split_whitespace().collect::<Vec<_>>();
            if v.len() < 9
                || v[0] != "1"
                || v[1].parse::<usize>().ok() != Some(first)
                || v[2].parse::<usize>().ok() != Some(after)
            {
                return None;
            }
            let b = v[3..7]
                .iter()
                .map(|s| s.parse::<f32>().ok())
                .collect::<Option<Vec<_>>>()?;
            if b.as_slice() != legacy || hex(v[7])? != raw {
                return None;
            }
            let o = crate::native_cell_origin::from_word(w, Some(total))?;
            if o.range != (first, after, total) || o.legacy != legacy {
                return None;
            }
            for s in &v[8..] {
                let e = s
                    .split(':')
                    .map(|s| s.parse::<usize>().ok())
                    .collect::<Option<Vec<_>>>()?;
                if e.len() != 3
                    || e[0] >= e[1]
                    || e[1] >= e[2]
                    || e[2] > total
                    || !((e[0] <= first && after <= e[1]) || (e[1] <= first && after <= e[2]))
                {
                    return None;
                }
                let e = (e[0], e[1], e[2]);
                if !moves.insert(e) {
                    return None;
                }
                edges.insert(e);
            }
        }
        records.push(Record {
            raw,
            legacy,
            first,
            after,
            moves,
        });
    }
    intervals.sort_unstable();
    let mut cursor = 0;
    for (first, after) in intervals {
        if first != cursor {
            return None;
        }
        cursor = after
    }
    if cursor != total || edges.is_empty() {
        return None;
    }
    Some(Scene {
        ink,
        obstacles,
        total,
        ignored,
        records,
        edges,
    })
}
fn key(raw: &str, b: [f32; 4]) -> (String, [u32; 4]) {
    (raw.into(), b.map(f32::to_bits))
}
fn bind<'a>(
    scene: &'a Scene,
    words: &[Word],
    origins: &[Option<Origin>],
) -> Option<Vec<&'a Record>> {
    if scene.records.len() != words.len() || origins.len() != words.len() {
        return None;
    }
    let mut source = BTreeMap::new();
    for r in &scene.records {
        if source.insert(key(&r.raw, r.legacy), r).is_some() {
            return None;
        }
    }
    let mut out = vec![];
    for (id, w) in words.iter().enumerate() {
        if w.id != id {
            return None;
        }
        let r = source.remove(&key(&w.raw, w.source_box))?;
        if let Some(o) = origins[id] {
            if o.range != (r.first, r.after, scene.total) || o.legacy != r.legacy {
                return None;
            }
        }
        if !r.moves.is_empty() && origins[id].is_none() {
            return None;
        }
        out.push(r);
    }
    Some(out)
}
fn visit(units: &[Unit], mut f: impl FnMut(&[Line])) {
    for u in units {
        match u {
            Unit::Plain(l) => f(l),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
                ..
            } => {
                f(preamble);
                for l in after.iter().chain(before).chain(outside) {
                    f(l)
                }
            }
        }
    }
}
fn material(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
// Preserve exact old per-word spans and separators whenever their adjacency stays.
fn spans(cell: &TableCell, words: &[Word]) -> Option<(String, BTreeMap<usize, (String, String)>)> {
    let chars = cell.text.chars().collect::<Vec<_>>();
    let mut pos = 0;
    let mut prefix = String::new();
    while chars.get(pos).is_some_and(|c| c.is_whitespace()) {
        prefix.push(chars[pos]);
        pos += 1
    }
    let mut spans = BTreeMap::new();
    for &id in &cell.words {
        let wanted = material(&words.get(id)?.text).chars().collect::<Vec<_>>();
        if wanted.is_empty() {
            return None;
        }
        let start = pos;
        for expected in wanted {
            while chars.get(pos).is_some_and(|c| c.is_whitespace()) {
                pos += 1
            }
            if chars.get(pos) != Some(&expected) {
                return None;
            }
            pos += 1
        }
        let body = chars[start..pos].iter().collect();
        let start = pos;
        while chars.get(pos).is_some_and(|c| c.is_whitespace()) {
            pos += 1
        }
        let gap = chars[start..pos].iter().collect();
        if spans.insert(id, (body, gap)).is_some() {
            return None;
        }
    }
    (pos == chars.len()).then_some((prefix, spans))
}
fn evidence(
    group: &BTreeSet<usize>,
    cell: &TableCell,
    origins: &[Option<Origin>],
    words: &[Word],
    bounds: &[NativeBounds],
    scene: &Scene,
) -> Option<bool> {
    let mut b = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    let mut baselines = vec![];
    for &id in group {
        let o = origins.get(id)?.as_ref()?;
        b[0] = b[0].min(o.bounds[0]);
        b[1] = b[1].min(o.bounds[1]);
        b[2] = b[2].max(o.bounds[2]);
        b[3] = b[3].max(o.bounds[3]);
        baselines.push(o.flow.baseline)
    }
    // Another cell may legitimately overlap this parent rectangle below the
    // header. Only the complete source band of this candidate needs exclusive
    // ownership, across the full parent width. Never infer a missing partner
    // from the target cell's membership alone.
    let band = [cell.x0, b[1], cell.x1, b[3]];
    let outer = [cell.x0, cell.y0, cell.x1, cell.y1];
    for ink in &scene.ink {
        // A complete source side of this existing cell is a table boundary.
        // A shorter line, including a value dash, cannot borrow that role.
        let side = ink.center.is_some_and(|s| {
            (s[0] == s[2]
                && (s[0] == outer[0] || s[0] == outer[2])
                && s[1] <= outer[1]
                && outer[3] <= s[3])
                || (s[1] == s[3]
                    && (s[1] == outer[1] || s[1] == outer[3])
                    && s[0] <= outer[0]
                    && outer[2] <= s[2])
        });
        if !side && overlaps(ink.paint, band) {
            return Some(false);
        }
    }

    for (id, _) in words.iter().enumerate() {
        if cell.words.contains(&id) {
            continue;
        }
        let NativeBounds::Glyphs(rect) = *bounds.get(id)? else {
            return None;
        };
        // Empty normalization does not erase the original source mark: a
        // dash can still be a value in an adjacent printed row.
        if overlaps(rect, band) {
            return Some(false);
        }
    }
    baselines.sort_by(f64::total_cmp);
    baselines.dedup();
    if baselines.len() < 2 {
        return Some(false);
    }
    let mut interleaved = false;
    for &id in &cell.words {
        if group.contains(&id) {
            continue;
        }
        let o = origins.get(id)?.as_ref()?;
        let disjoint = o.bounds[2] <= b[0] || b[2] <= o.bounds[0];
        // Independent paint sequences can put the same printed row at
        // different exact baselines. Both original baselines lying in the
        // other's original glyph envelope is positive row counterevidence;
        // no distance tolerance or font-size multiple is introduced.
        if disjoint
            && group.iter().any(|&member| {
                let Some(g) = origins[member] else {
                    return false;
                };
                let a = g.flow.baseline as f32;
                let b = o.flow.baseline as f32;
                g.bounds[1] <= b && b <= g.bounds[3] && o.bounds[1] <= a && a <= o.bounds[3]
            })
        {
            return Some(false);
        }
        if !disjoint && overlaps(b, o.bounds) {
            return Some(false);
        }
        if disjoint && baselines[0] < o.flow.baseline && o.flow.baseline < *baselines.last()? {
            interleaved = true
        }
    }
    Some(interleaved)
}
fn repair(
    cell: &TableCell,
    words: &[Word],
    bounds: &[NativeBounds],
    origins: &[Option<Origin>],
    counts: &BTreeMap<usize, usize>,
    records: &[&Record],
    scene: &Scene,
) -> Option<(String, Vec<usize>)> {
    if !cell.words.iter().any(|&id| !records[id].moves.is_empty()) {
        return None;
    }
    let outer = [cell.x0, cell.y0, cell.x1, cell.y1];
    if scene.obstacles.iter().any(|b| {
        b[0] < (outer[2] as f64)
            && (outer[0] as f64) < b[2]
            && b[1] < (outer[3] as f64)
            && (outer[1] as f64) < b[3]
    }) {
        return None;
    }
    if !valid(outer) || cell.words.is_empty() {
        return None;
    }
    let ids = cell.words.iter().copied().collect::<BTreeSet<_>>();
    if ids.len() != cell.words.len() {
        return None;
    }
    for (id, w) in words.iter().enumerate() {
        let NativeBounds::Glyphs(b) = *bounds.get(id)? else {
            return None;
        };
        if !valid(b) {
            return None;
        }
        if ids.contains(&id) {
            if w.status != WordStatus::Used || counts.get(&id) != Some(&1) || !within(b, outer) {
                return None;
            }
            let o = origins.get(id)?.as_ref()?;
            o.horizontal_source(w.source_box)?;
            if !within(o.bounds, outer) {
                return None;
            }
        }
    }
    let mut groups: Vec<BTreeSet<usize>> = vec![];
    for &edge in &scene.edges {
        let (mut members, mut parts) = (BTreeSet::new(), [vec![], vec![]]);
        for (id, r) in records.iter().enumerate() {
            if edge.0 < r.after && r.first < edge.2 {
                if !ids.contains(&id) || !r.moves.contains(&edge) {
                    members.clear();
                    break;
                }
                let part = if edge.0 <= r.first && r.after <= edge.1 {
                    0
                } else if edge.1 <= r.first && r.after <= edge.2 {
                    1
                } else {
                    members.clear();
                    break;
                };
                members.insert(id);
                parts[part].push(id);
            }
        }
        if members.is_empty() || parts.iter().any(Vec::is_empty) {
            continue;
        }
        let mut complete = true;
        for (p, (first, after)) in parts.iter_mut().zip([(edge.0, edge.1), (edge.1, edge.2)]) {
            p.sort_by_key(|&id| records[id].first);
            let mut cursor = first;
            let mut baseline = None;
            let mut end = None;
            for &id in p.iter() {
                let r = records[id];
                while cursor < r.first && scene.ignored.contains(&cursor) {
                    cursor += 1
                }
                if cursor != r.first {
                    complete = false;
                    break;
                }
                cursor = r.after;
                let f = origins[id]?.flow;
                if baseline.is_some_and(|y| y != f.baseline) || end.is_some_and(|x| x > f.start) {
                    complete = false;
                    break;
                }
                baseline = Some(f.baseline);
                end = Some(f.end);
            }
            while cursor < after && scene.ignored.contains(&cursor) {
                cursor += 1
            }
            if cursor != after {
                complete = false
            }
        }
        if !complete || origins[parts[0][0]]?.flow.baseline >= origins[parts[1][0]]?.flow.baseline {
            continue;
        }
        let mut i = 0;
        while i < groups.len() {
            if !groups[i].is_disjoint(&members) {
                members.extend(groups.remove(i));
                i = 0
            } else {
                i += 1
            }
        }
        groups.push(members);
    }
    groups.retain(|g| evidence(g, cell, origins, words, bounds, scene) == Some(true));
    if groups.is_empty() {
        return None;
    }
    let mut order = vec![];
    let mut emitted: BTreeSet<usize> = BTreeSet::new();
    for &id in &cell.words {
        if let Some(g) = groups.iter().find(|g| g.contains(&id)) {
            if g.iter().all(|id| !emitted.contains(id)) {
                let mut members = g.iter().copied().collect::<Vec<_>>();
                members.sort_by_key(|&id| records[id].first);
                order.extend(members);
                emitted.extend(g.iter().copied())
            }
        } else {
            order.push(id)
        }
    }
    if order == cell.words || order.len() != cell.words.len() {
        return None;
    }
    let (mut text, spans) = spans(cell, words)?;
    let mut previous = None;
    for &id in &order {
        if let Some(prev) = previous {
            let old = cell.words.iter().position(|&i| i == prev)?;
            if cell.words.get(old + 1) == Some(&id) {
                text.push_str(&spans.get(&prev)?.1)
            } else {
                text.push('\n')
            }
        }
        text.push_str(&spans.get(&id)?.0);
        previous = Some(id)
    }
    text.push_str(&spans.get(cell.words.last()?)?.1);
    Some((text, order))
}
pub(crate) fn restore(
    units: &mut [Unit],
    words: &[Word],
    bounds: &[NativeBounds],
    origins: &[Option<Origin>],
    scene: Option<&Scene>,
) {
    let Some(scene) = scene else { return };
    if bounds.len() != words.len() {
        return;
    }
    let Some(records) = bind(scene, words, origins) else {
        return;
    };
    let mut counts = BTreeMap::new();
    visit(units, |lines| {
        for l in lines {
            for &id in &l.words {
                *counts.entry(id).or_default() += 1
            }
            if let Some(t) = &l.table {
                for c in &t.cells {
                    for &id in &c.words {
                        *counts.entry(id).or_default() += 1
                    }
                }
            }
        }
    });
    let rewrite = |lines: &mut [Line]| {
        for l in lines {
            let Some(t) = &l.table else { continue };
            if l.text != t.render() {
                continue;
            }
            let changes = t
                .cells
                .iter()
                .enumerate()
                .filter_map(|(i, c)| {
                    repair(c, words, bounds, origins, &counts, &records, scene).map(|v| (i, v))
                })
                .collect::<Vec<_>>();
            if changes.is_empty() {
                continue;
            }
            let t = l.table.as_mut().unwrap();
            for (i, (text, ids)) in changes {
                t.cells[i].text = text;
                t.cells[i].words = ids
            }
            l.text = t.render()
        }
    };
    for u in units {
        match u {
            Unit::Plain(l) => rewrite(l),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
                ..
            } => {
                rewrite(preamble);
                for l in after.iter_mut().chain(before).chain(outside) {
                    rewrite(l)
                }
            }
        }
    }
}
#[cfg(test)]
#[path = "native_cell_lineage_tests.rs"]
mod tests;
