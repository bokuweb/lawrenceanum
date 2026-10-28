//! Source typography, baseline rows and a native ruled table delimit a parent
//! publication header. Only complete original Lines are reordered.
use super::super::framed_publications::render;
use super::*;

fn outer_edge(rules: &PageRules, x: f32, top: f32, bottom: f32) -> bool {
    let mut intervals = rules
        .vertical
        .iter()
        .filter(|v| v.x == x && v.y0.is_finite() && v.y1.is_finite() && v.y0 < v.y1)
        .map(|v| (v.y0, v.y1))
        .collect::<Vec<_>>();
    intervals.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut end = top;
    for (a, b) in intervals {
        if a > end {
            break;
        }
        end = end.max(b);
        if end >= bottom {
            return true;
        }
    }
    false
}
fn ruled_top(rules: &PageRules, t: Rect) -> bool {
    valid(t)
        && rules
            .horizontal
            .iter()
            .any(|r| r.y == t[1] && r.x0 <= t[0] && r.x1 >= t[2])
        && outer_edge(rules, t[0], t[1], t[3])
        && outer_edge(rules, t[2], t[1], t[3])
}
fn header_fence(rules: &PageRules, r: Rect) -> bool {
    rules
        .vertical
        .iter()
        .any(|v| r[0] < v.x && v.x < r[2] && v.y0 <= r[1] && v.y1 >= r[3])
        || rules
            .horizontal
            .iter()
            .any(|h| r[1] < h.y && h.y < r[3] && h.x0 <= r[0] && h.x1 >= r[2])
}
struct Header {
    chosen: Vec<usize>,
    order: Vec<usize>,
}
impl Header {
    fn ordered(&self) -> bool {
        self.order.iter().copied().eq(0..self.order.len())
    }
}
fn source_header(
    page: &PageTrace,
    rules: &PageRules,
    lines: &[SourceLine<'_>],
    h: &Head,
    ti: usize,
) -> Option<Header> {
    let table_line = lines.get(ti)?;
    let table = table_line.line.table.as_ref()?;
    let t = [table.x0, table.y0, table.x1, table.y1];
    if table_line.scope != lines[h.first].scope
        || native(page, table_line).is_none()
        || !ruled_top(rules, t)
        || h.rect[3] > t[1]
    {
        return None;
    }
    // A header belongs to the original containing column, when full-height
    // source dividers certify one. Captions can overhang the inner table edge;
    // absent such dividers the table edge remains the conservative boundary.
    let left = rules
        .vertical
        .iter()
        .filter(|v| v.x <= t[0] && outer_edge(rules, v.x, h.rect[1], t[3]))
        .map(|v| v.x)
        .max_by(f32::total_cmp)
        .unwrap_or(t[0]);
    let right = rules
        .vertical
        .iter()
        .filter(|v| v.x >= t[2] && outer_edge(rules, v.x, h.rect[1], t[3]))
        .map(|v| v.x)
        .min_by(f32::total_cmp)
        .unwrap_or(t[2]);
    let rect = [left, h.rect[1], right, t[1]];
    if !valid(rect)
        || !contains(rect, h.rect)
        || header_fence(rules, rect)
        || heads(page, rules, lines)
            .iter()
            .any(|other| other.first != h.first && intersects(rect, other.rect))
    {
        return None;
    }
    let mut chosen = Vec::new();
    let mut keys = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        let Some(b) = l.bbox else { continue };
        if !intersects(rect, b) {
            continue;
        }
        if !contains(rect, b) || l.scope != lines[h.first].scope || content(page, l).is_none() {
            return None;
        }
        let baseline = source_baseline(page, l)?;
        chosen.push(i);
        keys.push((baseline, b[0], b[2]));
    }
    if chosen.is_empty()
        || !chosen.windows(2).all(|p| p[1] == p[0] + 1)
        || !(h.first..h.end).all(|i| chosen.contains(&i))
    {
        return None;
    }
    let mut order = (0..chosen.len()).collect::<Vec<_>>();
    order.sort_by(|&a, &b| {
        keys[a]
            .0
            .total_cmp(&keys[b].0)
            .then(keys[a].1.total_cmp(&keys[b].1))
    });
    if order
        .windows(2)
        .any(|p| keys[p[0]].0 == keys[p[1]].0 && keys[p[0]].2 > keys[p[1]].1)
    {
        return None;
    }
    Some(Header { chosen, order })
}
// A date may be indented from the table edge. The complete, ordered native
// parent header and real outer table rules must supply that alternative proof.
pub(super) fn below_own_source_table(
    page: &PageTrace,
    rules: &PageRules,
    lines: &[SourceLine<'_>],
    h: &Head,
    ti: usize,
) -> bool {
    source_header(page, rules, lines, h, ti).is_some_and(|s| {
        s.ordered()
            && s.chosen[0] == h.first
            && s.chosen.last().copied().is_some_and(|last| last + 1 == ti)
    })
}
fn candidate(page: &PageTrace, rules: &PageRules, table_start: usize) -> Option<PageTrace> {
    let (text, doc, tables) = render(page, table_start);
    if text != page.text || doc != page.doc_text || tables != page.tables {
        return None;
    }
    let lines = source_lines(page)?;
    for h in heads(page, rules, &lines) {
        if !financial_title(&h) {
            continue;
        }
        for ti in 0..lines.len() {
            let Some(s) = source_header(page, rules, &lines, &h, ti) else {
                continue;
            };
            if s.ordered() {
                continue;
            }
            let region = lines[h.first].scope;
            let offset = page
                .regions
                .iter()
                .take(region)
                .map(|r| r.lines.len())
                .sum::<usize>();
            let first = s.chosen[0] - offset;
            let mut next = page.clone();
            let replacement = s
                .order
                .iter()
                .map(|&i| lines[s.chosen[i]].line.clone())
                .collect::<Vec<_>>();
            next.regions[region]
                .lines
                .splice(first..first + s.chosen.len(), replacement);
            let next_lines = source_lines(&next)?;
            let title_ids = lines[h.first..h.end]
                .iter()
                .flat_map(|l| l.ids.iter().copied())
                .collect::<Vec<_>>();
            let next_heads = heads(&next, rules, &next_lines);
            let Some(n) = next_heads.iter().find(|n| {
                next_lines[n.first..n.end]
                    .iter()
                    .flat_map(|l| l.ids.iter().copied())
                    .eq(title_ids.iter().copied())
            }) else {
                continue;
            };
            if !financial(&next, rules, &next_lines, n, fence(&next, &next_lines, n)) {
                continue;
            }
            let roots = starts(page, rules, &lines)
                .into_iter()
                .filter_map(|i| lines.get(i)?.ids.first().copied())
                .collect::<BTreeSet<_>>();
            let next_roots = starts(&next, rules, &next_lines)
                .into_iter()
                .filter_map(|i| next_lines.get(i)?.ids.first().copied())
                .collect::<BTreeSet<_>>();
            if !roots.is_subset(&next_roots) {
                continue;
            }
            drop(next_lines);
            let (text, doc, tables) = render(&next, table_start);
            if tables != page.tables {
                continue;
            }
            next.text = text;
            next.doc_text = doc;
            next.tables = tables;
            // Sorting a contiguous complete source span strictly decreases the
            // finite number of source-baseline/x inversions. Outside Lines keep
            // their relative position; no retry limit is used.
            return Some(next);
        }
    }
    None
}
pub(crate) fn restore_header_order(page: &mut PageTrace, rules: &PageRules, table_start: usize) {
    while let Some(next) = candidate(page, rules, table_start) {
        *page = next;
    }
}
