//! Conservative reconstruction of a wide, unruled horizontal matrix with a
//! native shared-category grouping glyph and multiple nested detail columns.
//! Word coordinates and old ownership are witnesses; no captions are matched.
use crate::trace::{Line, Table, TableCell, Unit, Word, WordStatus};
use std::collections::{BTreeMap, BTreeSet, HashSet};

#[derive(Debug)]
struct Geometry {
    em: f32,
    top: f32,
    caption_bottom: f32,
    fields: Vec<(f32, f32)>,
    xs: Vec<f32>,
    bands: Vec<(f32, f32)>,
    ys: Vec<f32>,
    anchors: Vec<usize>,
    shared: Vec<usize>,
    shared_start: usize,
    shared_span: usize,
    shared_right: f32,
    header: Vec<usize>,
    body: Vec<usize>,
    dashes: HashSet<usize>,
}
#[derive(Debug)]
struct Run {
    row: usize,
    x0: f32,
    x1: f32,
}
fn cy(w: &Word) -> f32 {
    (w.y0 + w.y1) * 0.5
}
fn cjk(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| matches!(c, 'ぁ'..='ヿ' | '㐀'..='鿿' | '々'))
}
fn private(c: char) -> bool {
    matches!(c as u32, 0xe000..=0xf8ff | 0xf0000..=0xffffd | 0x100000..=0x10fffd)
}
fn numeric(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || c == ',')
}
fn merge(mut spans: Vec<(f32, f32)>, gap: f32) -> Vec<(f32, f32)> {
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out: Vec<(f32, f32)> = Vec::new();
    for (lo, hi) in spans {
        if let Some(last) = out.last_mut().filter(|last| lo - last.1 <= gap) {
            last.1 = last.1.max(hi);
        } else {
            out.push((lo, hi));
        }
    }
    out
}
fn rows<'a>(mut ws: Vec<&'a Word>, em: f32) -> Vec<Vec<&'a Word>> {
    ws.sort_by(|a, b| cy(a).total_cmp(&cy(b)));
    let mut out: Vec<Vec<&Word>> = Vec::new();
    for w in ws {
        if out.last().is_none_or(|r| (cy(w) - cy(r[0])).abs() > em * 0.2) {
            out.push(Vec::new());
        }
        out.last_mut().unwrap().push(w);
    }
    for r in &mut out {
        r.sort_by(|a, b| a.x0.total_cmp(&b.x0));
    }
    out
}
fn infer(words: &[Word], owners: &HashSet<usize>) -> Option<Geometry> {
    let used: Vec<&Word> = words.iter().filter(|w| owners.contains(&w.id) && !w.text.is_empty()).collect();
    let mut sizes: Vec<f32> = used.iter().filter(|w| w.text.chars().count() <= 3).map(|w| w.y1 - w.y0).collect();
    sizes.sort_by(f32::total_cmp);
    let em = *sizes.get(sizes.len() / 2)?;
    if !(2.0..30.0).contains(&em) {
        return None;
    }
    let numeric_rows = rows(used.iter().copied().filter(|w| numeric(&w.text)).collect(), em);
    let mut runs = Vec::new();
    for (row, ws) in numeric_rows.iter().enumerate() {
        let mut groups: Vec<Vec<&Word>> = Vec::new();
        for &w in ws {
            if groups
                .last()
                .is_none_or(|g| w.x0 - g.iter().map(|z| z.x1).fold(f32::NEG_INFINITY, f32::max) > em * 0.65)
            {
                groups.push(Vec::new());
            }
            groups.last_mut().unwrap().push(w);
        }
        for g in groups {
            runs.push(Run {
                row,
                x0: g.iter().map(|w| w.x0).fold(f32::INFINITY, f32::min),
                x1: g.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max),
            });
        }
    }
    runs.sort_by(|a, b| a.x1.total_cmp(&b.x1));
    let mut edges: Vec<Vec<Run>> = Vec::new();
    for r in runs {
        if edges.last().is_none_or(|e| r.x1 - e[0].x1 > em * 0.1) {
            edges.push(Vec::new());
        }
        edges.last_mut().unwrap().push(r);
    }
    let mut signatures: BTreeMap<Vec<usize>, Vec<usize>> = BTreeMap::new();
    for (index, rs) in edges.iter().enumerate() {
        let mut sig: Vec<usize> = rs.iter().map(|r| r.row).collect();
        sig.sort_unstable();
        if (4..=12).contains(&sig.len()) && sig.windows(2).all(|p| p[0] != p[1]) {
            signatures.entry(sig).or_default().push(index);
        }
    }
    let candidates: Vec<_> = signatures.iter().filter(|(_, v)| v.len() >= 6).collect();
    if candidates.len() != 1 {
        return None;
    }
    let (signature, edge_ids) = candidates[0];
    let master: Vec<f32> = signature.iter().map(|&i| cy(numeric_rows[i][0])).collect();
    if master.windows(2).any(|p| p[1] - p[0] < em * 3.0) {
        return None;
    }
    let mut shared_candidates = Vec::new();
    for b in words.iter().filter(|w| {
        w.status == WordStatus::Empty
            && w.text.is_empty()
            && !w.raw.is_empty()
            && w.raw.chars().all(private)
            && w.y1 - w.y0 >= 8.0 * em
            && w.x1 > w.x0
            && w.x1 - w.x0 < 1.5 * em
    }) {
        let mut letters: Vec<&Word> = used
            .iter()
            .copied()
            .filter(|w| {
                w.text.chars().count() == 1 && cjk(&w.text) && (w.x0 - (b.x0 - em * 0.5)).abs() < em * 0.2 && b.y0 < w.y0 && w.y1 < b.y1
            })
            .collect();
        letters.sort_by(|a, b| cy(a).total_cmp(&cy(b)));
        if letters.len() < 3
            || letters.iter().map(|w| w.x0).fold(f32::NEG_INFINITY, f32::max) - letters.iter().map(|w| w.x0).fold(f32::INFINITY, f32::min)
                > em * 0.1
        {
            continue;
        }
        let gaps: Vec<f32> = letters.windows(2).map(|p| cy(p[1]) - cy(p[0])).collect();
        let small = gaps.iter().copied().fold(f32::INFINITY, f32::min);
        let large = gaps.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        if small < 1.5 * em || large - small > em * 0.1 {
            continue;
        }
        let covered: Vec<usize> = master
            .iter()
            .enumerate()
            .filter_map(|(r, &y)| (b.y0 < y && y < b.y1).then_some(r))
            .collect();
        if covered.len() < 2 || covered.len() >= master.len() || covered.windows(2).any(|p| p[1] != p[0] + 1) {
            continue;
        }
        shared_candidates.push((b, letters, covered));
    }
    if shared_candidates.len() != 1 {
        return None;
    }
    let (brace, letters, covered) = shared_candidates.pop()?;
    let shared: Vec<usize> = letters.iter().map(|w| w.id).collect();
    let visible: Vec<&Word> = used.iter().copied().filter(|w| !shared.contains(&w.id)).collect();
    // These known Empty header references supply bounds only. Their text stays
    // unmodified; restoring reference glyphs requires a separate source proof.
    let refs: Vec<&Word> = words
        .iter()
        .filter(|w| {
            w.status == WordStatus::Empty
                && w.raw.chars().count() == 1
                && w.raw.chars().all(|c| matches!(c, '\u{e94f}'..='\u{e955}'))
                && (w.y1 - w.y0 - em).abs() < em * 0.2
        })
        .collect();
    let geometry: Vec<&Word> = visible.iter().copied().chain(refs).collect();
    let vertical = merge(
        geometry
            .iter()
            .filter(|w| w.x0 >= brace.x1 - em * 0.1)
            .map(|w| (w.y0, w.y1))
            .collect(),
        em * 0.4,
    );
    let blanks: Vec<_> = vertical
        .windows(2)
        .filter(|p| p[1].0 - p[0].1 >= em && p[1].0 < master[0])
        .collect();
    if blanks.len() < 2 {
        return None;
    }
    let top = (blanks[blanks.len() - 2][0].1 + blanks[blanks.len() - 2][1].0) * 0.5;
    let caption_bottom = (blanks[blanks.len() - 1][0].1 + blanks[blanks.len() - 1][1].0) * 0.5;
    if caption_bottom - top < em || caption_bottom - top > 6.0 * em {
        return None;
    }
    let header: Vec<&Word> = visible.iter().copied().filter(|w| top <= w.y0 && w.y1 <= caption_bottom).collect();
    let geometric_header: Vec<&Word> = geometry.iter().copied().filter(|w| top <= w.y0 && w.y1 <= caption_bottom).collect();
    let body: Vec<&Word> = visible.iter().copied().filter(|w| w.y0 >= caption_bottom).collect();
    if header.is_empty() || body.is_empty() {
        return None;
    }
    let fields = merge(geometric_header.iter().chain(&body).map(|w| (w.x0, w.x1)).collect(), em * 0.75);
    if !(9..=12).contains(&fields.len()) || fields.len() < edge_ids.len() + 3 {
        return None;
    }
    let mut anchors = Vec::new();
    for &edge in edge_ids {
        let rs = &edges[edge];
        let left = rs.iter().map(|r| r.x0).fold(f32::INFINITY, f32::min);
        let right = rs[0].x1;
        let slots: Vec<usize> = fields
            .iter()
            .enumerate()
            .filter_map(|(i, &(lo, hi))| (lo - em * 0.1 <= left && right <= hi + em * 0.1).then_some(i))
            .collect();
        if slots.len() != 1 {
            return None;
        }
        anchors.push(slots[0]);
    }
    if anchors.contains(&0) || anchors.iter().collect::<HashSet<_>>().len() != anchors.len() {
        return None;
    }
    if fields
        .iter()
        .any(|&(lo, hi)| !header.iter().any(|w| lo <= w.x0 && w.x1 <= hi + em * 0.1))
    {
        return None;
    }
    let mut xs = vec![fields[0].0];
    xs.extend(fields.windows(2).map(|p| (p[0].1 + p[1].0) * 0.5));
    xs.push(fields.last()?.1);
    let dashes: HashSet<usize> = words
        .iter()
        .filter(|w| {
            w.status == WordStatus::Empty
                && w.raw == "\u{e61c}"
                && w.text.is_empty()
                && caption_bottom <= w.y0
                && (w.y1 - w.y0 - em).abs() < em * 0.2
                && xs[0] <= w.x0
                && w.x0 < w.x1
                && w.x1 <= *xs.last().unwrap()
        })
        .map(|w| w.id)
        .collect();
    let allbody: Vec<&Word> = body
        .iter()
        .copied()
        .chain(words.iter().filter(|w| dashes.contains(&w.id)))
        .collect();
    let bands = merge(allbody.iter().map(|w| (w.y0, w.y1)).collect(), em * 0.4);
    if bands.len() != master.len() || bands.windows(2).any(|p| p[1].0 - p[0].1 < em) {
        return None;
    }
    for (i, &(lo, hi)) in bands.iter().enumerate() {
        if !(lo <= master[i] && master[i] <= hi) {
            return None;
        }
        if !body.iter().any(|w| {
            fields[0].0 <= w.x0
                && w.x1 <= fields[0].1 + em * 0.1
                && lo <= w.y0
                && w.y1 <= hi
                && cjk(&w.text)
                && (cy(w) - master[i]).abs() <= em * 0.2
        }) {
            return None;
        }
    }
    // Shared category and child category occupy disjoint logical columns.
    let shared_right = brace.x1;
    if shared_right <= xs[0] || shared_right >= xs[1] || letters.iter().any(|w| w.x1 > shared_right) {
        return None;
    }
    for &i in &covered {
        if body.iter().any(|w| {
            fields[0].0 <= w.x0 && w.x1 <= fields[0].1 + em * 0.1 && bands[i].0 <= w.y0 && w.y1 <= bands[i].1 && w.x0 < shared_right
        }) {
            return None;
        }
    }
    let mut ys = vec![caption_bottom];
    ys.extend(bands.windows(2).map(|p| (p[0].1 + p[1].0) * 0.5));
    ys.push(bands.last()?.1);
    Some(Geometry {
        em,
        top,
        caption_bottom,
        fields,
        xs,
        bands,
        ys,
        anchors,
        shared,
        shared_start: covered[0],
        shared_span: covered.len(),
        shared_right,
        header: header.iter().map(|w| w.id).collect(),
        body: allbody.iter().map(|w| w.id).collect(),
        dashes,
    })
}

#[derive(Debug)]
struct Detail<'a> {
    label: Vec<&'a Word>,
    value: Vec<&'a Word>,
    y1: f32,
}
fn wt<'a>(w: &'a Word, g: &Geometry) -> &'a str {
    if g.dashes.contains(&w.id) {
        "－"
    } else {
        &w.text
    }
}
fn value(w: &Word, g: &Geometry) -> bool {
    numeric(&w.text) || g.dashes.contains(&w.id)
}
// An occupied amount slot is either one original dash or one complete numeric
// group. Never combine a dash with digits or duplicate ambiguous dash sources.
fn complete_amount(ws: &[&Word], g: &Geometry) -> bool {
    let dashes = ws.iter().filter(|w| g.dashes.contains(&w.id)).count();
    if dashes > 0 {
        return dashes == 1 && ws.len() == 1;
    }
    let mut ordered = ws.to_vec();
    ordered.sort_by(|a, b| a.x0.total_cmp(&b.x0));
    let number: String = ordered.iter().map(|w| w.text.as_str()).collect();
    let groups: Vec<&str> = number.split(',').collect();
    !number.is_empty()
        && groups.iter().all(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
        && (groups.len() == 1 || groups[0].len() <= 3 && groups[1..].iter().all(|s| s.len() == 3))
}
fn detail_text(ws: Vec<&Word>, field_words: &[&Word], g: &Geometry) -> Option<(String, Vec<usize>)> {
    let label_right = field_words.iter().filter(|w| !value(w, g)).map(|w| w.x1).reduce(f32::max)?;
    let value_left = field_words.iter().filter(|w| value(w, g)).map(|w| w.x0).reduce(f32::min)?;
    if value_left - label_right <= g.em * 0.2 {
        return None;
    }
    let mut records: Vec<Detail> = Vec::new();
    for line in rows(ws, g.em) {
        let mut record = Detail {
            label: line.iter().copied().filter(|w| !value(w, g)).collect(),
            value: line.iter().copied().filter(|w| value(w, g)).collect(),
            y1: line.iter().map(|w| w.y1).fold(f32::NEG_INFINITY, f32::max),
        };
        if let Some(previous) = records.last_mut() {
            let wraps = (!previous.value.is_empty() && record.value.is_empty() || previous.value.is_empty() && !record.value.is_empty())
                && !previous.label.is_empty()
                && !record.label.is_empty()
                && record.label[0].y0 - previous.y1 < g.em * 0.3
                && (previous.label[0].x0 - record.label[0].x0).abs() < g.em * 0.2
                && label_right - previous.label.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max) < g.em * 0.25
                // A second full-width empty detail can be an independent entry.
                // Support only a demonstrably shorter final continuation line.
                && label_right - record.label.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max) >= g.em * 0.25;
            if wraps {
                previous.label.append(&mut record.label);
                previous.value.append(&mut record.value);
                previous.y1 = record.y1;
                continue;
            }
        }
        records.push(record);
    }
    if records.iter().any(|r| !complete_amount(&r.value, g)) {
        return None;
    }
    let mut text = Vec::new();
    let mut ids = Vec::new();
    for record in records {
        let label: String = record.label.iter().map(|w| wt(w, g)).collect();
        let number: String = record.value.iter().map(|w| wt(w, g)).collect();
        text.push(if label.is_empty() { number } else { format!("{label}　{number}") });
        ids.extend(record.label.iter().chain(&record.value).map(|w| w.id));
    }
    Some((text.join("\n"), ids))
}
fn plain_text(ws: Vec<&Word>, g: &Geometry) -> (String, Vec<usize>) {
    let mut ids = Vec::new();
    let mut text = Vec::new();
    for line in rows(ws, g.em) {
        text.push(line.iter().map(|w| wt(w, g)).collect::<String>());
        ids.extend(line.iter().map(|w| w.id));
    }
    (text.join("\n"), ids)
}
fn build(words: &[Word], g: &Geometry) -> Option<Table> {
    let get = |ids: &[usize]| -> Option<Vec<&Word>> { ids.iter().map(|id| words.iter().find(|w| w.id == *id)).collect() };
    let header = get(&g.header)?;
    let body = get(&g.body)?;
    let shared = get(&g.shared)?;
    let mut cells = Vec::new();
    let mut cell = |row, col, rowspan, colspan, x0, y0, x1, y1, text, words| {
        cells.push(TableCell {
            row,
            col,
            rowspan,
            colspan,
            x0,
            y0,
            x1,
            y1,
            text,
            words,
        })
    };
    for f in 0..g.fields.len() {
        let ws: Vec<&Word> = header.iter().copied().filter(|w| g.xs[f] <= w.x0 && w.x1 <= g.xs[f + 1]).collect();
        if ws.is_empty() {
            return None;
        }
        let (text, ids) = plain_text(ws, g);
        cell(
            0,
            if f == 0 { 0 } else { f + 1 },
            1,
            if f == 0 { 2 } else { 1 },
            g.xs[f],
            g.top,
            g.xs[f + 1],
            g.caption_bottom,
            text,
            ids,
        );
    }
    let shared_text = shared.iter().map(|w| w.text.as_str()).collect();
    cell(
        g.shared_start + 1,
        0,
        g.shared_span,
        1,
        g.xs[0],
        g.ys[g.shared_start],
        g.shared_right,
        g.ys[g.shared_start + g.shared_span],
        shared_text,
        g.shared.clone(),
    );
    for (r, &(lo, hi)) in g.bands.iter().enumerate() {
        for f in 0..g.fields.len() {
            let allfield: Vec<&Word> = body.iter().copied().filter(|w| g.xs[f] <= w.x0 && w.x1 <= g.xs[f + 1]).collect();
            let ws: Vec<&Word> = allfield.iter().copied().filter(|w| lo <= w.y0 && w.y1 <= hi + 0.01).collect();
            if ws.is_empty() {
                return None;
            }
            let (text, ids) = if f == 0 {
                plain_text(ws, g)
            } else if g.anchors.contains(&f) {
                if !complete_amount(&ws, g) || rows(ws.clone(), g.em).len() != 1 {
                    return None;
                }
                plain_text(ws, g)
            } else {
                detail_text(ws, &allfield, g)?
            };
            let within_shared = f == 0 && r >= g.shared_start && r < g.shared_start + g.shared_span;
            cell(
                r + 1,
                if f == 0 { usize::from(within_shared) } else { f + 1 },
                1,
                if f == 0 && !within_shared { 2 } else { 1 },
                if within_shared { g.shared_right } else { g.xs[f] },
                g.ys[r],
                g.xs[f + 1],
                g.ys[r + 1],
                text,
                ids,
            );
        }
    }
    let ids: Vec<usize> = cells.iter().flat_map(|c| c.words.iter().copied()).collect();
    let expected: BTreeSet<usize> = g.header.iter().chain(&g.body).chain(&g.shared).copied().collect();
    if ids.len() != expected.len() || ids.iter().copied().collect::<BTreeSet<_>>() != expected {
        return None;
    }
    Some(Table {
        x0: g.xs[0],
        y0: g.top,
        x1: *g.xs.last()?,
        y1: *g.ys.last()?,
        rows: g.bands.len() + 1,
        cols: g.fields.len() + 1,
        cells,
    })
}
/// Horizontal-only hook, before the existing prefix_minus operation. No word or
/// owner is modified unless all geometry, cell contents and ID checks succeed.
pub(crate) fn restore(units: &mut [Unit], words: &mut [Word]) {
    let [Unit::Plain(lines)] = units else {
        return;
    };
    if lines.iter().any(|l| l.table.is_some()) {
        return;
    }
    let old: Vec<usize> = lines.iter().flat_map(|l| l.words.iter().copied()).collect();
    let owners: HashSet<usize> = old.iter().copied().collect();
    if old.len() != owners.len() {
        return;
    }
    let Some(g) = infer(words, &owners) else {
        return;
    };
    let Some(table) = build(words, &g) else {
        return;
    };
    let target: HashSet<usize> = table.cells.iter().flat_map(|c| c.words.iter().copied()).collect();
    // Never cut a pre-existing line in half or steal another article's words.
    if lines
        .iter()
        .any(|l| l.words.iter().any(|id| target.contains(id)) && l.words.iter().any(|id| !target.contains(id)))
    {
        return;
    }
    if words
        .iter()
        .any(|w| owners.contains(&w.id) && !target.contains(&w.id) && w.y1 > g.top)
    {
        return;
    }
    let added: HashSet<usize> = target.difference(&owners).copied().collect();
    if added != g.dashes {
        return;
    }
    lines.retain(|l| l.words.iter().all(|id| !target.contains(id)));
    lines.push(Line {
        text: table.render(),
        words: Vec::new(),
        table: Some(table),
    });
    for w in words.iter_mut().filter(|w| g.dashes.contains(&w.id)) {
        w.text = "－".to_string();
        w.status = WordStatus::Used;
    }
}

#[cfg(test)]
#[path = "financial_flow_tests.rs"]
mod tests;
