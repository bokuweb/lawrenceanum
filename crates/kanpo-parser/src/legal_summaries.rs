//! Independently numbered legal summaries. Printed roots and their own source
//! typography establish ownership; law names and instrument values are opaque.
use super::*;
use crate::{
    native_word_axes::NativeAxis,
    trace::{ContentStyles, NativeBounds},
};

fn nominal(t: &str) -> bool {
    !t.is_empty() && !t.contains(['。', '！', '？', '!', '?', '「', '」', '『', '』'])
}
fn evidence(
    page: &PageTrace,
    axis: NativeAxis,
    l: &SourceLine<'_>,
) -> Option<(Rect, NativeStyles)> {
    if l.role != Role::Body || l.line.table.is_some() || l.ids.is_empty() {
        return None;
    }
    for &id in &l.ids {
        if page.native_axes.get(id) != Some(&axis)
            || !matches!(page.native_bounds.get(id),Some(NativeBounds::Glyphs(b))if valid(*b))
            || !matches!(
                page.native_content_styles.get(id),
                Some(ContentStyles::Glyphs(_))
            )
        {
            return None;
        }
    }
    let s = heading_styles(page, l, l.tokens.as_ref()?.len())?;
    (!s.is_empty()).then_some((flow(l.bbox?, axis), s))
}
fn identifier(s: &str) -> bool {
    ["法律第", "政令第", "条約第"].iter().any(|kind| {
        s.strip_prefix(kind)
            .and_then(|n| n.strip_suffix('号'))
            .is_some_and(super::court_records::positive_integer)
    })
}
// A full nominal title carries one top-level instrument identifier. Other
// balanced parenthetical qualifiers may occur in its name or after the number.
fn title(t: &str) -> bool {
    let Some(s) = t.strip_prefix('◇') else {
        return false;
    };
    if !nominal(s) {
        return false;
    }
    let mut stack = vec![];
    let mut start = 0;
    let mut identifiers = vec![];
    for (i, c) in s.char_indices() {
        match c {
            '(' | '（' => {
                if stack.is_empty() {
                    start = i;
                }
                stack.push(if c == '(' { ')' } else { '）' });
            }
            ')' | '）' => {
                if stack.pop() != Some(c) {
                    return false;
                }
                if stack.is_empty() {
                    let open = s[start..].chars().next().unwrap();
                    let contents = &s[start + open.len_utf8()..i];
                    if contents.is_empty() {
                        return false;
                    }
                    if identifier(contents) {
                        identifiers.push((start, i + c.len_utf8()));
                    }
                }
            }
            _ => {}
        }
    }
    if !stack.is_empty() || identifiers.len() != 1 {
        return false;
    }
    let (start, end) = identifiers[0];
    start > 0 && qualifier_bytes(&s[end..]).is_some_and(|n| end + n == s.len())
}
// A short first line is not the physical right edge of a column. Native
// vertical rules are barriers; overlapping source bands and repeated left
// margins establish continuation without a guessed overhang tolerance.
fn column(first: Rect, b: Rect) -> bool {
    first[0] <= b[0] && b[0] < first[2]
}
fn crosses(rules: &PageRules, b: Rect) -> bool {
    rules
        .vertical
        .iter()
        .any(|r| b[0] < r.x && r.x < b[2] && r.y0 < b[3] && b[1] < r.y1)
}
pub(super) fn larger_prefix(
    page: &PageTrace,
    axis: NativeAxis,
    rules: &PageRules,
    lines: &[SourceLine<'_>],
    first: usize,
    band: Rect,
    header: &NativeStyles,
) -> Option<usize> {
    let max = header
        .iter()
        .map(|&(_, b)| f64::from_bits(b))
        .fold(0., f64::max);
    let mut start = first;
    let mut top = band[1];
    while start > 0 {
        let l = &lines[start - 1];
        if l.scope != lines[first].scope {
            break;
        }
        let b = flow(l.bbox?, axis);
        if !column(band, b) || b[1] >= top {
            break;
        }
        if !l.unquoted || !nominal(&l.tokens.as_ref()?.concat()) {
            break;
        }
        let (b, s) = evidence(page, axis, l)?;
        if crosses(rules, b) {
            return None;
        }
        if !s.iter().all(|&(_, bits)| f64::from_bits(bits) > max) || b[3] > top {
            break;
        }
        top = b[1];
        start -= 1;
    }
    Some(start)
}
// Compare every explicit source axis in its own reading coordinates. No
// source Word or output geometry is rotated or rewritten.
fn flow(b: Rect, axis: NativeAxis) -> Rect {
    match axis {
        NativeAxis::Right => b,
        NativeAxis::Down => [b[1], -b[2], b[3], -b[0]],
        NativeAxis::Left => [-b[2], -b[3], -b[0], -b[1]],
        NativeAxis::Up => [-b[3], b[0], -b[1], b[2]],
        _ => unreachable!(),
    }
}
fn flow_rules(r: &PageRules, axis: NativeAxis) -> PageRules {
    let mut out = PageRules::default();
    match axis {
        NativeAxis::Right => out.vertical = r.vertical.clone(),
        NativeAxis::Left => {
            out.vertical = r
                .vertical
                .iter()
                .map(|v| crate::lines::VRule {
                    x: -v.x,
                    y0: -v.y1,
                    y1: -v.y0,
                })
                .collect()
        }
        NativeAxis::Down => {
            out.vertical = r
                .horizontal
                .iter()
                .map(|h| crate::lines::VRule {
                    x: h.y,
                    y0: -h.x1,
                    y1: -h.x0,
                })
                .collect()
        }
        NativeAxis::Up => {
            out.vertical = r
                .horizontal
                .iter()
                .map(|h| crate::lines::VRule {
                    x: -h.y,
                    y0: h.x0,
                    y1: h.x1,
                })
                .collect()
        }
        _ => {}
    }
    out
}
fn next_column(r: &PageRules, headers: &[Rect], body: Rect) -> bool {
    let end = headers
        .iter()
        .map(|b| b[2])
        .fold(f32::NEG_INFINITY, f32::max);
    let Some(edge) = r
        .vertical
        .iter()
        .filter(|v| {
            v.x.is_finite()
                && v.y0.is_finite()
                && v.y1.is_finite()
                && v.y0 < v.y1
                && v.x >= end
                && headers.iter().all(|b| v.y0 <= b[1] && b[1] < v.y1)
        })
        .min_by(|a, b| a.x.total_cmp(&b.x))
    else {
        return false;
    };
    if body[0] < edge.x || body[1] < edge.y0 || body[1] >= edge.y1 {
        return false;
    }
    !r.vertical
        .iter()
        .any(|v| edge.x < v.x && v.x <= body[0] && v.y0 <= body[1] && body[1] < v.y1)
}
fn root(
    page: &PageTrace,
    rules: &PageRules,
    lines: &[SourceLine<'_>],
    first: usize,
) -> Option<usize> {
    let l = &lines[first];
    let mut text = l.tokens.as_ref()?.concat();
    if !text.starts_with('◇') || !l.unquoted {
        return None;
    }
    let axis = *page.native_axes.get(*l.ids.first()?)?;
    if !matches!(
        axis,
        NativeAxis::Right | NativeAxis::Down | NativeAxis::Left | NativeAxis::Up
    ) {
        return None;
    }
    let projected = flow_rules(rules, axis);
    let rules = &projected;
    let (b, mut styles) = evidence(page, axis, l)?;
    if crosses(rules, b) {
        return None;
    }
    let mut headers = vec![b];
    let mut end = first + 1;
    let mut top = b[1];
    let mut hanging = None;
    loop {
        if !nominal(&text) {
            return None;
        }
        let next = lines.get(end)?;
        if next.scope != l.scope {
            return None;
        }
        let (nb, ns) = evidence(page, axis, next)?;
        if crosses(rules, nb) {
            return None;
        }
        let nt = next.tokens.as_ref()?.concat();
        if nt.starts_with('◇') {
            return None;
        }
        let same = column(b, nb) && nb[1] > top;
        // An already complete title may continue with its own first body in the
        // immediately adjacent source column, certified by the original separator.
        if title(&text) && !parenthetical_line(&nt) {
            if !same && !next_column(rules, &headers, nb) {
                return None;
            }
            if !ns.is_subset(&styles) || styles.is_subset(&ns) {
                return None;
            }
            return larger_prefix(page, axis, rules, lines, first, b, &styles);
        }
        if !same || !next.unquoted || nt.contains('◇') {
            return None;
        }
        match hanging {
            None => hanging = Some(nb[0]),
            Some(x) if x != nb[0] => return None,
            _ => {}
        }
        text.push_str(&nt);
        styles.extend(ns);
        headers.push(nb);
        top = nb[1];
        end += 1;
    }
}
pub(super) fn starts(page: &PageTrace, rules: &PageRules, lines: &[SourceLine<'_>]) -> Vec<usize> {
    (0..lines.len())
        .filter_map(|i| root(page, rules, lines, i))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

// A whole parenthetical line can continue a qualifier; text after its closed
// parenthesis is an outline/body line. An open group can wrap in source order.
fn parenthetical_line(t: &str) -> bool {
    let mut stack = vec![];
    let mut any = false;
    for c in t.chars() {
        match c {
            '(' | '（' => {
                stack.push(if c == '(' { ')' } else { '）' });
                any = true;
            }
            ')' | '）' => {
                if stack.pop() != Some(c) {
                    return false;
                }
            }
            _ => {
                if stack.is_empty() {
                    return false;
                }
            }
        }
    }
    any && nominal(t)
}
