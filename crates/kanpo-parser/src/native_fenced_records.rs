//! Native fenced two-field records with trailing identifiers. The identifier
//! grammar has no token-length, hyphen-count, row-count or wrap-count ceiling.
//! Complete original glyph cells, source anchor rows and empty column gaps
//! supply ownership. No label/issuer/page vocabulary is consulted.
use super::*;
use crate::{
    native_word_axes::NativeAxis,
    native_word_flow::{NativeFlow, NativeMode},
    trace::NativeBounds,
};
use std::collections::HashSet;
fn identifier(t: &str) -> bool {
    let mut chars = t.trim().chars().peekable();
    let letter = |c: char| c.is_ascii_uppercase() || ('Ａ'..='Ｚ').contains(&c);
    let digit = |c: char| c.is_ascii_digit() || ('０'..='９').contains(&c);
    if !chars.peek().copied().is_some_and(letter) {
        return false;
    }
    while chars.peek().copied().is_some_and(letter) {
        chars.next();
    }
    if !chars.peek().copied().is_some_and(digit) {
        return false;
    }
    while chars.peek().copied().is_some_and(digit) {
        chars.next();
    }
    while let Some(c) = chars.next() {
        if !matches!(c, '-' | '－') || !chars.peek().copied().is_some_and(digit) {
            return false;
        }
        while chars.peek().copied().is_some_and(digit) {
            chars.next();
        }
    }
    true
}
fn flow(w: &Col, s: SourceGeometry<'_>) -> Option<NativeFlow> {
    let f = (*s.flows.get(w.id)?)?;
    (f.mode == Some(NativeMode::Horizontal) && f.belongs([w.x, w.y, w.x1, w.y1])).then_some(f)
}
fn panel(words: &[Col], s: SourceGeometry<'_>, left: f32, right: f32) -> Option<trace::Table> {
    let mut spans = words.iter().map(|w| (w.x, w.x1)).collect::<Vec<_>>();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut union: Vec<(f32, f32)> = vec![];
    for (a, b) in spans {
        match union.last_mut() {
            Some(p) if a <= p.1 => p.1 = p.1.max(b),
            _ => union.push((a, b)),
        }
    }
    // More independent projected lanes do not prove a two-cell record grammar.
    if union.len() != 2 {
        return None;
    }
    let cut = ((f64::from(union[0].1) + f64::from(union[1].0)) / 2.) as f32;
    if !(union[0].1 < cut && cut < union[1].0) {
        return None;
    }
    let names = words.iter().filter(|w| w.x1 < cut).collect::<Vec<_>>();
    let mut keys = words
        .iter()
        .filter(|w| w.x > cut)
        .map(|w| Some((w, flow(w, s)?)))
        .collect::<Option<Vec<_>>>()?;
    if names.len() + keys.len() != words.len()
        || keys.len() < 2
        || keys.iter().any(|(w, _)| !identifier(&w.text))
    {
        return None;
    }
    keys.sort_by(|a, b| a.1.baseline.total_cmp(&b.1.baseline));
    if keys
        .windows(2)
        .any(|p| p[0].1.baseline >= p[1].1.baseline || p[0].0.y1 >= p[1].0.y)
    {
        return None;
    }
    let mut grouped = vec![Vec::<Col>::new(); keys.len()];
    let mut first = vec![false; keys.len()];
    for w in names {
        // A mixed-font original Word can lack a complete advance-flow witness. Its
        // complete native glyph cell can still contain exactly one explicit anchor baseline.
        let direct = keys
            .iter()
            .enumerate()
            .filter(|(_, (k, kf))| match flow(w, s) {
                Some(wf) => {
                    f64::from(w.y) <= kf.baseline
                        && kf.baseline <= f64::from(w.y1)
                        && f64::from(k.y) <= wf.baseline
                        && wf.baseline <= f64::from(k.y1)
                }
                None => f64::from(w.y) <= kf.baseline && kf.baseline <= f64::from(w.y1),
            })
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        let row = if direct.len() == 1 {
            first[direct[0]] = true;
            direct[0]
        } else {
            if direct.len() > 1 {
                return None;
            }
            if let Some(f) = s.flows.get(w.id).copied().flatten() {
                if f.mode != Some(NativeMode::Horizontal) || !f.belongs([w.x, w.y, w.x1, w.y1]) {
                    return None;
                }
                let r = keys.iter().rposition(|(_, k)| k.baseline <= f.baseline)?;
                if f.baseline == keys[r].1.baseline {
                    first[r] = true;
                }
                r
            } else {
                keys.iter().rposition(|(k, _)| k.y <= w.y)?
            }
        };
        if let Some((next, _)) = keys.get(row + 1) {
            if w.y1 >= next.y {
                return None;
            }
        }
        grouped[row].push(w.clone());
    }
    if first.iter().any(|v| !*v) {
        return None;
    }
    let mut cells = vec![];
    let mut all = HashSet::new();
    for (row, ((key, _), ws)) in keys.iter().zip(grouped).enumerate() {
        let mut lines = crate::horizontal::row_lines(ws);
        let ids = lines
            .iter()
            .flat_map(|l| l.words.iter().copied())
            .collect::<Vec<_>>();
        for id in ids.iter().copied().chain([key.id]) {
            if !all.insert(id) {
                return None;
            }
        }
        let top = words
            .iter()
            .filter(|w| ids.contains(&w.id))
            .map(|w| w.y)
            .fold(key.y, f32::min);
        let bottom = words
            .iter()
            .filter(|w| ids.contains(&w.id))
            .map(|w| w.y1)
            .fold(key.y1, f32::max);
        let text = lines
            .drain(..)
            .map(|l| l.text)
            .collect::<Vec<_>>()
            // Each group is one name field owned by one trailing identifier.
            // Physical wrapping must not insert a new field separator into it.
            .join("");
        cells.push(trace::TableCell {
            row,
            col: 0,
            rowspan: 1,
            colspan: 1,
            x0: left,
            y0: top,
            x1: cut,
            y1: bottom,
            text,
            words: ids,
        });
        cells.push(trace::TableCell {
            row,
            col: 1,
            rowspan: 1,
            colspan: 1,
            x0: cut,
            y0: top,
            x1: right,
            y1: bottom,
            text: key.text.clone(),
            words: vec![key.id],
        });
    }
    if all.len() != words.len() {
        return None;
    }
    Some(trace::Table {
        x0: left,
        y0: words.iter().map(|w| w.y).reduce(f32::min)?,
        x1: right,
        y1: words.iter().map(|w| w.y1).reduce(f32::max)?,
        rows: keys.len(),
        cols: 2,
        cells,
    })
}
fn invalid_flow(w: &Col, s: SourceGeometry<'_>) -> bool {
    s.flows.get(w.id).copied().flatten().is_some() && flow(w, s).is_none()
}
pub(super) fn tables(
    words: &[Col],
    source: SourceGeometry<'_>,
    v: &[(f32, f32, f32)],
    h: &[(f32, f32, f32)],
) -> Option<Vec<trace::Table>> {
    if words.is_empty() || !source.sources_unchanged(words) {
        return None;
    }
    let mut seen = HashSet::new();
    for w in words {
        if !seen.insert(w.id) || source.axes.get(w.id) != Some(&NativeAxis::Right) {
            return None;
        }
        let NativeBounds::Glyphs(b) = source.bounds.get(w.id)? else {
            return None;
        };
        if !b.iter().all(|n| n.is_finite())
            || b[0] >= b[2]
            || b[1] >= b[3]
            || *b != [w.x, w.y, w.x1, w.y1]
        {
            return None;
        }
    }
    if v.iter()
        .chain(h)
        .any(|&(a, b, c)| !a.is_finite() || !b.is_finite() || !c.is_finite() || b >= c)
    {
        return None;
    }
    // A source rule can terminate on the last text baseline. Glyph ink may
    // extend below that baseline; source flow supplies the actual row coordinate.
    // Mixed-font Words without one common baseline still require full glyph coverage.
    for w in words {
        if invalid_flow(w, source) {
            return None;
        }
    }
    let top = words.iter().map(|w| w.y).reduce(f32::min)?;
    let bottom = words.iter().map(|w| w.y1).reduce(f32::max)?;
    if h.iter().any(|&(y, _, _)| top < y && y < bottom) {
        return None;
    }
    let mut fences = v
        .iter()
        .filter(|&&(_, a, b)| {
            words.iter().all(|w| match flow(w, source) {
                Some(f) => f64::from(a) <= f.baseline && f.baseline <= f64::from(b),
                None => a <= w.y && b >= w.y1,
            })
        })
        .map(|r| r.0)
        .collect::<Vec<_>>();
    fences.sort_by(f32::total_cmp);
    fences.dedup();
    let mut out = vec![];
    let mut owned = HashSet::new();
    for pair in fences.windows(2) {
        let ws = words
            .iter()
            .filter(|w| w.x >= pair[0] && w.x1 <= pair[1])
            .cloned()
            .collect::<Vec<_>>();
        if ws.is_empty() {
            continue;
        }
        let t = panel(&ws, source, pair[0], pair[1])?;
        for w in &ws {
            if !owned.insert(w.id) {
                return None;
            }
        }
        out.push(t);
    }
    (owned.len() == words.len() && !out.is_empty()).then_some(out)
}
#[cfg(test)]
#[path = "native_fenced_record_tests.rs"]
mod tests;
