//! Keep a proven closed financial publication atomic in the reading stream.
//! A transaction moves whole source lines only; existing publication witnesses
//! authorize its scope. No publisher value or fixed distance/count is inferred.
use super::*;
use crate::trace::{NativeBounds, Table, Unit};
use std::collections::HashSet;

#[derive(Clone)]
struct Scope {
    rect: Rect,
    members: Vec<usize>,
}
fn shape(line: &SourceLine<'_>) -> Option<Rect> {
    let mut b = line.bbox?;
    if let Some(table) = &line.line.table {
        let t = [table.x0, table.y0, table.x1, table.y1];
        if !valid(t) {
            return None;
        }
        b = [
            b[0].min(t[0]),
            b[1].min(t[1]),
            b[2].max(t[2]),
            b[3].max(t[3]),
        ];
    }
    valid(b).then_some(b)
}
fn scopes(page: &PageTrace, rules: &PageRules, lines: &[SourceLine<'_>]) -> Option<Vec<Scope>> {
    if rules.enclosures.iter().any(|&r| !valid(r)) {
        return None;
    }
    let boxes = lines.iter().map(shape).collect::<Vec<_>>();
    if lines
        .iter()
        .zip(&boxes)
        .any(|(l, b)| !l.line.text.trim().is_empty() && b.is_none())
    {
        return None;
    }
    let mut out = Vec::new();
    for &rect in &rules.enclosures {
        let mut members = Vec::new();
        let mut invalid = false;
        for (i, (line, b)) in lines.iter().zip(&boxes).enumerate() {
            let Some(b) = b else { continue };
            if intersects(rect, *b) && !contains(rect, *b) {
                invalid = true;
                break;
            }
            if !contains(rect, *b) {
                continue;
            }
            if line.role!=Role::Body || line.ids.is_empty() || line.ids.iter().any(|&id| !matches!(page.native_bounds.get(id),Some(NativeBounds::Glyphs(b))if valid(*b))) {invalid=true;break;}
            members.push(i);
        }
        if !invalid && !members.is_empty() {
            out.push(Scope { rect, members });
        }
    }
    out.sort_by(|a, b| {
        a.members[0]
            .cmp(&b.members[0])
            .then(b.members.len().cmp(&a.members.len()))
            .then_with(|| {
                for (x, y) in a.rect.iter().zip(&b.rect) {
                    let order = x.total_cmp(y);
                    if !order.is_eq() {
                        return order;
                    }
                }
                std::cmp::Ordering::Equal
            })
    });
    Some(out)
}
fn fragmentation(scopes: &[Scope]) -> usize {
    scopes
        .iter()
        .map(|s| s.members.windows(2).filter(|p| p[1] != p[0] + 1).count())
        .sum()
}
fn parent_fence(scope: &Scope, all: &[Scope], rules: &PageRules) -> bool {
    // Equal membership is a double border, not a new semantic parent. A larger
    // frame whose first line belongs to no smaller closed child has its own
    // source preamble. A child cannot leave that common scope on this evidence.
    for &outer in &rules.enclosures {
        if outer == scope.rect || !contains(outer, scope.rect) {
            continue;
        }
        let Some(parent) = all.iter().find(|s| s.rect == outer) else {
            return true;
        };
        if parent.members == scope.members {
            continue;
        }
        let first = parent.members[0];
        let first_in_child = all.iter().any(|child| {
            child.members.len() < parent.members.len()
                && contains(outer, child.rect)
                && child.members.contains(&first)
        });
        if !first_in_child {
            return true;
        }
    }
    all.iter().any(|other| {
        intersects(scope.rect, other.rect)
            && !contains(scope.rect, other.rect)
            && !contains(other.rect, scope.rect)
    })
}
fn quote_states(lines: &[SourceLine<'_>]) -> Option<Vec<(Vec<char>, Vec<char>)>> {
    let mut state = Vec::new();
    let mut out = Vec::new();
    for line in lines {
        let before = state.clone();
        advance_quotes(&line.line.text, &mut state)?;
        out.push((before, state.clone()));
    }
    state.is_empty().then_some(out)
}
fn advance_quotes(text: &str, state: &mut Vec<char>) -> Option<()> {
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
fn render(page: &PageTrace, table_start: usize) -> (String, String, Vec<Table>) {
    let lines = page
        .regions
        .iter()
        .flat_map(|r| r.lines.iter().cloned())
        .collect();
    let unit = Unit::Plain(lines);
    let mut tables = Vec::new();
    let text = unit.text();
    let doc = unit.doc_text(&mut tables, table_start);
    (text, doc, tables)
}
fn source_roundtrip(page: &PageTrace, table_start: usize) -> bool {
    let (text, doc, tables) = render(page, table_start);
    text == page.text && doc == page.doc_text && tables == page.tables
}
fn restore_page(page: &mut PageTrace, rules: &PageRules, table_start: usize) {
    if rules.enclosures.is_empty() {
        return;
    }
    if !source_roundtrip(page, table_start) {
        return;
    }
    loop {
        let Some(lines) = source_lines(page) else {
            return;
        };
        let Some(all) = scopes(page, rules, &lines) else {
            return;
        };
        let Some(quotes) = quote_states(&lines) else {
            return;
        };
        let before_energy = fragmentation(&all);
        if before_energy == 0 {
            return;
        }
        let locations = page
            .regions
            .iter()
            .enumerate()
            .flat_map(|(r, region)| (0..region.lines.len()).map(move |l| (r, l)))
            .collect::<Vec<_>>();
        let mut accepted = None;
        for scope in &all {
            if !scope
                .members
                .windows(2)
                .any(|p| (p[0] + 1..p[1]).any(|i| !lines[i].line.text.trim().is_empty()))
            {
                continue;
            }
            if lines[scope.members[0]].line.table.is_some() || parent_fence(scope, &all, rules) {
                continue;
            }
            let (region, first) = locations[scope.members[0]];
            if scope.members.iter().any(|&i| locations[i].0 != region) {
                continue;
            }
            let chosen = scope
                .members
                .iter()
                .map(|&i| locations[i].1)
                .collect::<Vec<_>>();
            let chosen_set = chosen.iter().copied().collect::<HashSet<_>>();
            let mut local = (0..first).collect::<Vec<_>>();
            local.extend(chosen.iter().copied());
            local.extend(
                (first..page.regions[region].lines.len()).filter(|i| !chosen_set.contains(i)),
            );
            let mut candidate = page.clone();
            candidate.regions[region].lines = local
                .iter()
                .map(|&i| page.regions[region].lines[i].clone())
                .collect();
            let Some(next) = source_lines(&candidate) else {
                continue;
            };
            let Some(next_scopes) = scopes(&candidate, rules, &next) else {
                continue;
            };
            if next_scopes.len() != all.len() || fragmentation(&next_scopes) >= before_energy {
                continue;
            }
            let Some(next_quotes) = quote_states(&next) else {
                continue;
            };
            let offset = locations
                .iter()
                .position(|&(r, l)| r == region && l == 0)
                .unwrap();
            if (0..locations.len()).any(|i| {
                let old = if i >= offset && i < offset + local.len() {
                    offset + local[i - offset]
                } else {
                    i
                };
                quotes[old] != next_quotes[i]
            }) {
                continue;
            }
            let first_global = offset + first;
            let owned = owned_frames(&candidate, rules, &next);
            if !owned.iter().any(|f| {
                f.rect == scope.rect
                    && f.first == first_global
                    && f.end == first_global + chosen.len()
            }) {
                continue;
            }
            if !headings(&candidate, &next, &owned)
                .iter()
                .any(|h| h.line == first_global && h.financial && h.authorized)
            {
                continue;
            }
            if !decisions(&candidate, rules, &next).contains(&first_global) {
                continue;
            }
            // The decreasing geometric fragmentation measure, not a retry cap,
            // bounds this loop. Every region line remains the same whole value.
            drop(next);
            let (text, doc, tables) = render(&candidate, table_start);
            if tables.len() != page.tables.len() {
                continue;
            }
            candidate.text = text;
            candidate.doc_text = doc;
            candidate.tables = tables;
            accepted = Some(candidate);
            break;
        }
        let Some(candidate) = accepted else { return };
        *page = candidate;
    }
}
pub(crate) fn restore_framed_reading_order(pages: &mut [PageTrace], rules: &[Option<PageRules>]) {
    let mut quotation = Some(Vec::new());
    let mut table_start = 0;
    for (i, page) in pages.iter_mut().enumerate() {
        let outside = quotation.as_ref().is_some_and(Vec::is_empty);
        if let Some(state) = &mut quotation {
            if advance_quotes(&page.text, state).is_none() {
                quotation = None;
            }
        }
        if outside && quotation.as_ref().is_some_and(Vec::is_empty) {
            if let Some(rules) = rules.get(i).and_then(Option::as_ref) {
                restore_page(page, rules, table_start);
            }
        }
        table_start += page.tables.len();
    }
}
#[cfg(test)]
#[path = "framed_publications_tests.rs"]
mod tests;
