//! Reconstruct a source-ruled location / wrong / correct table.
//! Caption values and record/word counts are not routing inputs.
use super::*;
use crate::trace::{ContentStyles, NativeBounds, Table, TableCell, Unit, Word};
use std::collections::{BTreeMap, BTreeSet};
const FIELDS: [&str; 5] = ["ページ", "段", "行", "誤", "正"];
fn squash(t: &str) -> String {
    t.chars().filter(|c| !c.is_whitespace()).collect()
}
fn union(a: Rect, b: Rect) -> Rect {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}
fn mid(a: f32, b: f32) -> f32 {
    ((a as f64 + b as f64) / 2.) as f32
}
fn line_ids(l: &Line) -> Vec<usize> {
    l.table.as_ref().map_or_else(
        || l.words.clone(),
        |t| {
            t.cells
                .iter()
                .flat_map(|c| c.words.iter().copied())
                .collect()
        },
    )
}
fn unit_lines(u: &Unit) -> Vec<&Line> {
    match u {
        Unit::Plain(ls) => ls.iter().collect(),
        Unit::Shinkyu {
            preamble,
            after,
            before,
            outside,
        } => preamble
            .iter()
            .chain(after.iter().flatten())
            .chain(before.iter().flatten())
            .chain(outside.iter().flatten())
            .collect(),
    }
}
struct Source<'a> {
    words: &'a [Word],
    styles: &'a [Option<NativeStyles>],
    content: &'a [ContentStyles],
    bounds: &'a [NativeBounds],
    boxes: BTreeMap<usize, Rect>,
}
impl<'a> Source<'a> {
    fn new(
        words: &'a [Word],
        styles: &'a [Option<NativeStyles>],
        content: &'a [ContentStyles],
        bounds: &'a [NativeBounds],
        ids: impl IntoIterator<Item = usize>,
    ) -> Option<Self> {
        let mut boxes = BTreeMap::new();
        for id in ids {
            let w = words.get(id)?;
            if w.id != id || !matches!(w.status, WordStatus::Used) || boxes.contains_key(&id) {
                return None;
            }
            let NativeBounds::Glyphs(b) = bounds.get(id)? else {
                return None;
            };
            if !valid(*b) {
                return None;
            }
            boxes.insert(id, *b);
        }
        Some(Self {
            words,
            styles,
            content,
            bounds,
            boxes,
        })
    }
    // A blank reconstructed cell must not conceal raw source that the earlier
    // parser could not read or did not assign. Compatibility boxes are used
    // only to reject a claim when explicit native geometry is unavailable.
    fn unaccounted(&self, scope: Rect, owned: &BTreeSet<usize>) -> bool {
        self.words.iter().enumerate().any(|(slot, w)| {
            if owned.contains(&w.id) {
                return false;
            }
            if slot != w.id {
                return true;
            }
            let b = match self.bounds.get(slot) {
                Some(NativeBounds::Glyphs(b)) => *b,
                _ => [w.x0, w.y0, w.x1, w.y1],
            };
            if !valid(b) {
                return true;
            }
            if !intersects(scope, b) {
                return false;
            }
            let explicit_space = !w.raw.is_empty()
                && w.raw.chars().all(char::is_whitespace)
                && matches!(self.content.get(slot),Some(ContentStyles::Glyphs(s)) if s.is_empty());
            !explicit_space
        })
    }
    fn style(&self, ids: &[usize]) -> Option<NativeStyles> {
        let mut out = NativeStyles::new();
        for &id in ids {
            let all = self.styles.get(id)?.as_ref()?;
            if all.is_empty()
                || all.iter().any(|&(_, bits)| {
                    let size = f64::from_bits(bits);
                    !size.is_finite() || size <= 0.
                })
            {
                return None;
            }
            let ContentStyles::Glyphs(s) = self.content.get(id)? else {
                return None;
            };
            if !s.is_subset(all)
                || self.words[id].text.chars().any(char::is_alphanumeric) == s.is_empty()
            {
                return None;
            }
            out.extend(s.iter().copied());
        }
        Some(out)
    }
    fn rect(&self, ids: &[usize]) -> Option<Rect> {
        ids.iter().map(|id| self.boxes[id]).reduce(union)
    }
    fn columns(&self, ids: &[usize]) -> Option<Vec<Vec<usize>>> {
        if ids.is_empty() {
            return Some(vec![]);
        }
        let mut components: Vec<Vec<usize>> = ids.iter().map(|&id| vec![id]).collect();
        loop {
            let mut pair = None;
            for i in 0..components.len() {
                for j in i + 1..components.len() {
                    if components[i].iter().any(|a| {
                        components[j].iter().any(|b| {
                            let (a, b) = (self.boxes[a], self.boxes[b]);
                            (a[0] <= b[0] && b[2] <= a[2]) || (b[0] <= a[0] && a[2] <= b[2])
                        })
                    }) {
                        pair = Some((i, j));
                        break;
                    }
                }
                if pair.is_some() {
                    break;
                }
            }
            let Some((i, j)) = pair else { break };
            let other = components.remove(j);
            components[i].extend(other);
        }
        for col in &mut components {
            let left = col.iter().map(|id| self.boxes[id][0]).reduce(f32::max)?;
            let right = col.iter().map(|id| self.boxes[id][2]).reduce(f32::min)?;
            if left >= right {
                return None;
            }
            col.sort_by(|a, b| self.boxes[a][1].total_cmp(&self.boxes[b][1]));
            if col
                .windows(2)
                .any(|p| self.boxes[&p[0]][3] > self.boxes[&p[1]][1])
            {
                return None;
            }
        }
        components.sort_by(|a, b| self.rect(b).unwrap()[0].total_cmp(&self.rect(a).unwrap()[0]));
        for pair in components.windows(2) {
            let (a, b) = (self.rect(&pair[0])?, self.rect(&pair[1])?);
            if !(b[0] < a[0] && b[2] < a[2]) || mid(b[0], b[2]) >= a[0] || mid(a[0], a[2]) <= b[2] {
                return None;
            }
        }
        Some(components)
    }
    fn read(&self, ids: &[usize]) -> Option<(String, Vec<usize>)> {
        let order = self.columns(ids)?.into_iter().flatten().collect::<Vec<_>>();
        let text = order
            .iter()
            .map(|&id| self.words[id].text.as_str())
            .collect::<String>();
        Some((text, order))
    }
}
#[derive(Clone)]
struct Slab {
    x0: f32,
    x1: f32,
    ys: Vec<f32>,
}
fn slabs(rules: &PageRules) -> Option<Vec<Slab>> {
    let mut hs = rules.horizontal.clone();
    if hs
        .iter()
        .any(|r| ![r.x0, r.x1, r.y].iter().all(|v| v.is_finite()) || r.x0 >= r.x1)
    {
        return None;
    }
    hs.sort_by(|a, b| {
        a.y.total_cmp(&b.y)
            .then(a.x0.total_cmp(&b.x0))
            .then(a.x1.total_cmp(&b.x1))
    });
    let mut joined: Vec<crate::lines::HRule> = vec![];
    for h in hs {
        if let Some(last) = joined.last_mut() {
            if last.y == h.y && h.x0 <= last.x1 {
                last.x1 = last.x1.max(h.x1);
                continue;
            }
        }
        joined.push(h);
    }
    let mut out: Vec<Slab> = vec![];
    for h in joined {
        if let Some(s) = out.iter_mut().find(|s| s.x0 == h.x0 && s.x1 == h.x1) {
            s.ys.push(h.y);
        } else {
            out.push(Slab {
                x0: h.x0,
                x1: h.x1,
                ys: vec![h.y],
            });
        }
    }
    Some(out)
}
fn citation(text: &str) -> bool {
    let t = compact(text);
    for (i, c) in t.char_indices() {
        if c != '日' {
            continue;
        }
        let end = i + c.len_utf8();
        if !date(&t[..end]) {
            continue;
        }
        let rest = &t[end..];
        let Some(open) = rest.chars().next() else {
            return false;
        };
        let close = match open {
            '(' => ')',
            '（' => '）',
            _ => return false,
        };
        let mut stack = vec![close];
        for (i, c) in rest.char_indices().skip(1) {
            match c {
                '(' => stack.push(')'),
                '（' => stack.push('）'),
                ')' | '）' => {
                    if stack.pop() != Some(c) {
                        return false;
                    }
                    if stack.is_empty() {
                        return !rest[i + c.len_utf8()..].is_empty();
                    }
                }
                _ => {}
            }
        }
        return false;
    }
    false
}
struct Plan {
    caption: Line,
    table: Table,
    ids: BTreeSet<usize>,
    rect: Rect,
}
fn plans(s: &Source<'_>, rules: &PageRules) -> Vec<Plan> {
    let Some(slabs) = slabs(rules) else {
        return vec![];
    };
    let mut vs = rules.vertical.clone();
    if vs
        .iter()
        .any(|v| ![v.x, v.y0, v.y1].iter().all(|n| n.is_finite()) || v.y0 >= v.y1)
    {
        return vec![];
    }
    vs.sort_by(|a, b| {
        a.x.total_cmp(&b.x)
            .then(a.y0.total_cmp(&b.y0))
            .then(a.y1.total_cmp(&b.y1))
    });
    vs.dedup();
    let mut found = vec![];
    for pair in vs.windows(2) {
        let (l, r) = (pair[0], pair[1]);
        if l.x >= r.x || l.y0 != r.y0 || l.y1 != r.y1 {
            continue;
        }
        let caption_rect = [l.x, l.y0, r.x, r.y1];
        let cap: Vec<_> = s
            .boxes
            .iter()
            .filter_map(|(&id, &b)| contains(caption_rect, b).then_some(id))
            .collect();
        if cap.is_empty()
            || s.boxes
                .values()
                .any(|&b| intersects(caption_rect, b) && !contains(caption_rect, b))
        {
            continue;
        }
        let Some((title, title_ids)) = s.read(&cap) else {
            continue;
        };
        if title.is_empty() || title.contains(['。', '！', '？', '!', '?', '「', '」', '『', '』'])
        {
            continue;
        }
        let Some(title_styles) = s.style(&cap).filter(|s| !s.is_empty()) else {
            continue;
        };
        let top = slabs
            .iter()
            .filter(|h| h.x0 <= l.x && r.x <= h.x1)
            .flat_map(|h| h.ys.iter().copied())
            .filter(|&y| y <= l.y0)
            .max_by(f32::total_cmp);
        let bottom = slabs
            .iter()
            .filter(|h| h.x0 <= l.x && r.x <= h.x1)
            .flat_map(|h| h.ys.iter().copied())
            .filter(|&y| y >= l.y1)
            .min_by(f32::total_cmp);
        let (Some(top), Some(bottom)) = (top, bottom) else {
            continue;
        };
        for header in &slabs {
            if header.x1 > l.x
                || header.ys.len() + 1 != FIELDS.len()
                || header.ys.iter().any(|&y| y <= top || y >= bottom)
            {
                continue;
            }
            let mut edges = vec![top];
            edges.extend(&header.ys);
            edges.push(bottom);
            let get_field = |b: Rect| edges.windows(2).position(|p| p[0] <= b[1] && b[3] <= p[1]);
            let header_ids: Vec<_> = s
                .boxes
                .iter()
                .filter_map(|(&id, &b)| {
                    (header.x0 <= mid(b[0], b[2])
                        && mid(b[0], b[2]) <= header.x1
                        && top <= b[1]
                        && b[3] <= bottom)
                        .then_some(id)
                })
                .collect();
            let mut labels = vec![vec![]; FIELDS.len()];
            let mut ok = true;
            for &id in &header_ids {
                if let Some(c) = get_field(s.boxes[&id]) {
                    labels[c].push(id);
                } else {
                    ok = false;
                }
            }
            if !ok
                || labels
                    .iter()
                    .zip(FIELDS)
                    .any(|(ids, label)| s.read(ids).is_none_or(|(t, _)| squash(&t) != label))
            {
                continue;
            }
            let Some(header_styles) = s
                .style(&header_ids)
                .filter(|x| !x.is_empty() && x.is_disjoint(&title_styles))
            else {
                continue;
            };
            let _ = header_styles;
            let data_slabs: Vec<_> = slabs
                .iter()
                .filter(|b| b.x1 < header.x0 && b.ys == header.ys)
                .collect();
            if data_slabs.len() != 1 {
                continue;
            }
            let data = data_slabs[0];
            let mut ownership = BTreeSet::new();
            let mut field_ids = vec![vec![]; FIELDS.len()];
            let mut refs = vec![];
            let mut bad = false;
            for (&id, &b) in &s.boxes {
                let x = mid(b[0], b[2]);
                if x < data.x0 || x > r.x || b[3] <= top || b[1] >= bottom {
                    continue;
                }
                if b[1] < top || b[3] > bottom {
                    bad = true;
                    break;
                }
                ownership.insert(id);
                if cap.contains(&id) || header_ids.contains(&id) {
                    continue;
                }
                if x <= data.x1 {
                    if b[2] > data.x1 {
                        bad = true;
                        break;
                    }
                    let Some(c) = get_field(b) else {
                        bad = true;
                        break;
                    };
                    field_ids[c].push(id);
                } else if data.x1 < x && x < header.x0 {
                    if b[0] < data.x1 || b[2] > header.x0 {
                        bad = true;
                        break;
                    }
                    refs.push(id);
                } else {
                    bad = true;
                    break;
                }
            }
            if bad || field_ids[0].is_empty() || refs.is_empty() {
                continue;
            }
            let Some((reference, reference_ids)) = s.read(&refs) else {
                continue;
            };
            if !citation(&reference)
                || s.style(&refs)
                    .is_none_or(|x| x.is_empty() || !x.is_disjoint(&title_styles))
                || ownership
                    .iter()
                    .any(|&id| s.words[id].text.contains(['「', '」', '『', '』']))
                || s.style(&ownership.iter().copied().collect::<Vec<_>>())
                    .is_none()
            {
                continue;
            }
            let mut data_ids: Vec<_> = field_ids.iter().flatten().copied().collect();
            data_ids.sort_by(|a, b| s.boxes[a][0].total_cmp(&s.boxes[b][0]));
            let mut components: Vec<Vec<usize>> = vec![];
            for id in data_ids {
                if let Some(last) = components.last_mut() {
                    if s.boxes[&id][0] <= s.rect(last).unwrap()[2] {
                        last.push(id);
                        continue;
                    }
                }
                components.push(vec![id]);
            }
            components.reverse();
            let mut records: Vec<Vec<usize>> = vec![];
            for component in components {
                let pages: Vec<_> = component
                    .iter()
                    .filter(|id| field_ids[0].contains(id))
                    .copied()
                    .collect();
                if !pages.is_empty() {
                    if s.columns(&pages).is_none_or(|c| c.len() != 1) {
                        bad = true;
                        break;
                    }
                    records.push(component);
                } else if component
                    .iter()
                    .any(|id| field_ids[1].contains(id) || field_ids[2].contains(id))
                {
                    // An unanchored column/line locator can be a new record or
                    // wrapped locator content. Source geometry does not decide.
                    bad = true;
                    break;
                } else if let Some(last) = records.last_mut() {
                    last.extend(component);
                } else {
                    bad = true;
                    break;
                }
            }
            if bad || records.is_empty() {
                continue;
            }
            let mut cells = vec![];
            for (c, ids) in labels.iter().enumerate() {
                let Some((text, words)) = s.read(ids) else {
                    bad = true;
                    break;
                };
                cells.push(TableCell {
                    row: 0,
                    col: c,
                    rowspan: 1,
                    colspan: 1,
                    x0: header.x0,
                    y0: edges[c],
                    x1: header.x1,
                    y1: edges[c + 1],
                    text,
                    words,
                });
            }
            cells.push(TableCell {
                row: 1,
                col: 0,
                rowspan: 1,
                colspan: FIELDS.len(),
                x0: data.x1,
                y0: top,
                x1: header.x0,
                y1: bottom,
                text: reference,
                words: reference_ids,
            });
            for (row, record) in records.iter().enumerate() {
                let b = s.rect(record).unwrap();
                for (c, field) in field_ids.iter().enumerate() {
                    let ids: Vec<_> = record
                        .iter()
                        .filter(|id| field.contains(id))
                        .copied()
                        .collect();
                    let Some((text, words)) = s.read(&ids) else {
                        bad = true;
                        break;
                    };
                    cells.push(TableCell {
                        row: row + 2,
                        col: c,
                        rowspan: 1,
                        colspan: 1,
                        x0: b[0],
                        y0: edges[c],
                        x1: b[2],
                        y1: edges[c + 1],
                        text,
                        words,
                    });
                }
            }
            if bad {
                continue;
            }
            let table = Table {
                x0: s
                    .rect(&ownership.iter().copied().collect::<Vec<_>>())
                    .unwrap()[0],
                y0: top,
                x1: header.x1,
                y1: bottom,
                rows: records.len() + 2,
                cols: FIELDS.len(),
                cells,
            };
            let rect = union(
                [table.x0, table.y0, table.x1, table.y1],
                s.rect(&cap).unwrap(),
            );
            // The source claim includes caption padding inside its native rails.
            let rect = [rect[0], rect[1], r.x, rect[3]];
            if rules
                .enclosures
                .iter()
                .any(|&b| !valid(b) || intersects(b, rect))
            {
                continue;
            }
            if s.unaccounted(rect, &ownership) {
                continue;
            }
            let caption = Line::text(title.clone(), title_ids.clone());
            if found.iter().any(|p: &Plan| p.ids == ownership) {
                continue;
            }
            found.push(Plan {
                caption,
                table,
                ids: ownership,
                rect,
            });
        }
    }
    let conflicts: BTreeSet<_> = found
        .iter()
        .enumerate()
        .filter(|(i, p)| {
            found
                .iter()
                .enumerate()
                .any(|(j, q)| i != &j && (!p.ids.is_disjoint(&q.ids) || intersects(p.rect, q.rect)))
        })
        .map(|(i, _)| i)
        .collect();
    found
        .into_iter()
        .enumerate()
        .filter_map(|(i, p)| (!conflicts.contains(&i)).then_some(p))
        .collect()
}
fn quotes(text: &str, state: &mut Vec<char>) -> Option<()> {
    for c in text.chars() {
        match c {
            '「' => state.push('」'),
            '『' => state.push('』'),
            '」' | '』' => {
                if state.pop() != Some(c) {
                    return None;
                }
            }
            _ => {}
        }
    }
    Some(())
}
fn chars(text: &str) -> BTreeMap<char, usize> {
    let mut out = BTreeMap::new();
    for c in text.chars().filter(|c| !c.is_whitespace()) {
        *out.entry(c).or_insert(0) += 1;
    }
    out
}
fn header_alphabet_present(words: &[Word]) -> bool {
    let alphabet: BTreeSet<_> = words.iter().flat_map(|w| w.text.chars()).collect();
    FIELDS
        .iter()
        .flat_map(|s| s.chars())
        .all(|c| alphabet.contains(&c))
}
pub(crate) fn restore_ruled_corrections(
    units: &mut [Unit],
    words: &[Word],
    styles: &[Option<NativeStyles>],
    content: &[ContentStyles],
    bounds: &[NativeBounds],
    rules: &PageRules,
    previous: &[PageTrace],
) {
    if !header_alphabet_present(words) {
        return;
    }

    let ids = units.iter().flat_map(unit_lines).flat_map(line_ids);
    let Some(source) = Source::new(words, styles, content, bounds, ids) else {
        return;
    };
    let plans = plans(&source, rules);
    let mut replacements = vec![];
    for p in plans {
        let mut locations = vec![];
        let mut bad = false;
        for (ui, u) in units.iter().enumerate() {
            for (li, l) in unit_lines(u).into_iter().enumerate() {
                let ids = line_ids(l);
                if ids.iter().any(|id| p.ids.contains(id)) {
                    if !matches!(u, Unit::Plain(_))
                        || l.table.is_some()
                        || ids.iter().any(|id| !p.ids.contains(id))
                    {
                        bad = true;
                    }
                    locations.push((ui, li));
                }
            }
        }
        if bad || locations.is_empty() {
            continue;
        }
        let ui = locations[0].0;
        if locations.iter().any(|p| p.0 != ui) {
            continue;
        }
        let Unit::Plain(lines) = &units[ui] else {
            continue;
        };
        let first = locations[0].1;
        let end = locations.last().unwrap().1 + 1;
        if (first..end).any(|i| !locations.contains(&(ui, i)) && !lines[i].text.trim().is_empty()) {
            continue;
        }
        let mut state = vec![];
        let mut valid_quote = true;
        for t in previous
            .iter()
            .map(|p| p.text.clone())
            .chain(units[..ui].iter().map(Unit::text))
            .chain(lines[..first].iter().map(|l| l.text.clone()))
        {
            if quotes(&t, &mut state).is_none() {
                valid_quote = false;
                break;
            }
        }
        if !valid_quote || !state.is_empty() {
            continue;
        }
        let before = lines[first..end]
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let after = format!("{}\n{}", p.caption.text, p.table.render());
        if chars(&before) != chars(&after) {
            continue;
        }
        let table = Line {
            text: p.table.render(),
            words: vec![],
            table: Some(p.table),
        };
        replacements.push((ui, first, end, vec![p.caption, table]));
    }
    replacements.sort_by_key(|(u, start, _, _)| (std::cmp::Reverse(*u), std::cmp::Reverse(*start)));
    for (ui, first, end, new) in replacements {
        if let Unit::Plain(lines) = &mut units[ui] {
            lines.splice(first..end, new);
        }
    }
}
pub(super) fn section_starts(
    page: &PageTrace,
    rules: &PageRules,
    lines: &[SourceLine<'_>],
) -> Vec<usize> {
    if !header_alphabet_present(&page.words) {
        return vec![];
    }

    let Some(s) = Source::new(
        &page.words,
        &page.native_styles,
        &page.native_content_styles,
        &page.native_bounds,
        lines.iter().flat_map(|l| l.ids.iter().copied()),
    ) else {
        return vec![];
    };
    let mut out = vec![];
    for p in plans(&s, rules) {
        for i in 0..lines.len().saturating_sub(1) {
            if lines[i].role == Role::Body
                && lines[i + 1].role == Role::Body
                && lines[i].unquoted
                && lines[i + 1].unquoted
                && *lines[i].line == p.caption
                && lines[i + 1].line.table.as_ref() == Some(&p.table)
            {
                let ids: BTreeSet<_> = lines[i]
                    .ids
                    .iter()
                    .chain(&lines[i + 1].ids)
                    .copied()
                    .collect();
                if ids == p.ids {
                    out.push(i);
                }
            }
        }
    }
    out
}
#[cfg(test)]
#[path = "ruled_corrections_tests.rs"]
mod tests;
