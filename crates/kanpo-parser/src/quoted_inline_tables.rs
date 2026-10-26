//! Restore a quoted vertical cell that continues across an open column break.
//!
//! The evidence is quotation syntax, two complementary open ruled strips and
//! unique original Word ownership. Labels, their scripts/lengths, native Word
//! segmentation and unrelated surrounding lines do not select this repair.
use crate::lines::PageRules;
use crate::trace::{Line, Table, Unit, Word, WordStatus};
use std::collections::BTreeMap;

fn near(a: f32, b: f32, tolerance: f32) -> bool {
    (a - b).abs() <= tolerance
}
fn unresolved(c: char) -> bool {
    c.is_control()
        || c == '\u{fffd}'
        || matches!(c, '\u{e000}'..='\u{f8ff}' | '\u{f0000}'..='\u{ffffd}' | '\u{100000}'..='\u{10fffd}')
}
fn word(words: &[Word], id: usize) -> Option<&Word> {
    words.get(id).filter(|w| {
        w.id == id
            && w.status == WordStatus::Used
            && !w.text.is_empty()
            && !w.text.chars().any(unresolved)
    })
}
struct Span<'a> {
    ids: &'a [usize],
    text: String,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}
fn span<'a>(ids: &'a [usize], words: &[Word]) -> Option<Span<'a>> {
    let first = word(words, *ids.first()?)?;
    let mut s = Span {
        ids,
        text: String::new(),
        x0: first.x0,
        y0: first.y0,
        x1: first.x1,
        y1: first.y1,
    };
    for &id in ids {
        let w = word(words, id)?;
        if ![w.x0, w.y0, w.x1, w.y1].iter().all(|v| v.is_finite()) || w.x0 >= w.x1 || w.y0 >= w.y1 {
            return None;
        }
        s.text.push_str(&w.text);
        s.x0 = s.x0.min(w.x0);
        s.y0 = s.y0.min(w.y0);
        s.x1 = s.x1.max(w.x1);
        s.y1 = s.y1.max(w.y1);
    }
    Some(s)
}
fn source_line<'a>(line: &'a Line, words: &[Word]) -> Option<Span<'a>> {
    if line.table.is_some() {
        return None;
    }
    let s = span(&line.words, words)?;
    (s.text == line.text).then_some(s)
}
fn quoted_marker(line: &Line, marker: &str, words: &[Word]) -> bool {
    source_line(line, words).is_some_and(|s| s.text == marker)
}
fn ids(line: &Line) -> Vec<usize> {
    line.words
        .iter()
        .copied()
        .chain(
            line.table
                .iter()
                .flat_map(|t| t.cells.iter().flat_map(|c| c.words.iter().copied())),
        )
        .collect()
}
fn owners(units: &[Unit]) -> BTreeMap<usize, usize> {
    let mut counts = BTreeMap::new();
    let mut add = |lines: &[Line]| {
        for line in lines {
            for id in ids(line) {
                *counts.entry(id).or_insert(0) += 1;
            }
        }
    };
    for unit in units {
        match unit {
            Unit::Plain(lines) => add(lines),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            ..} => {
                add(preamble);
                for lines in after.iter().chain(before).chain(outside) {
                    add(lines)
                }
            }
        }
    }
    counts
}
fn cap(r: &PageRules, y: f32, x0: f32, x1: f32, t: f32) -> bool {
    r.horizontal
        .iter()
        .any(|h| near(h.y, y, t) && near(h.x0, x0, t) && near(h.x1, x1, t))
}
fn side(r: &PageRules, x: f32, y0: f32, y1: f32, t: f32) -> bool {
    r.vertical
        .iter()
        .any(|v| near(v.x, x, t) && near(v.y0, y0, t) && near(v.y1, y1, t))
}
// A vertical stack can contain any number of cells. Every boundary must be
// supported by the actual rules; a general multi-column grid is ambiguous here.
fn stack(table: &Table, words: &[Word], rules: &PageRules, tolerance: f32, open_top: bool) -> bool {
    if table.rows != 1
        || table.cols == 0
        || table.cells.len() != table.cols
        || table.x0 >= table.x1
        || table.y0 >= table.y1
    {
        return false;
    }
    if !side(rules, table.x0, table.y0, table.y1, tolerance)
        || !side(rules, table.x1, table.y0, table.y1, tolerance)
        || cap(rules, table.y0, table.x0, table.x1, tolerance) == open_top
    {
        return false;
    }
    let mut y = table.y0;
    for (i, c) in table.cells.iter().enumerate() {
        if c.row != 0
            || c.col != i
            || c.rowspan != 1
            || c.colspan != 1
            || !near(c.x0, table.x0, tolerance)
            || !near(c.x1, table.x1, tolerance)
            || !near(c.y0, y, tolerance)
            || c.y0 >= c.y1
        {
            return false;
        }
        if !span(&c.words, words).is_some_and(|s| {
            s.text == c.text
                && s.x0 >= c.x0 - tolerance
                && s.x1 <= c.x1 + tolerance
                && s.y0 >= c.y0 - tolerance
                && s.y1 <= c.y1 + tolerance
        }) || !cap(rules, c.y1, table.x0, table.x1, tolerance)
        {
            return false;
        }
        y = c.y1;
    }
    near(y, table.y1, tolerance)
}
fn bag(lines: &[Vec<Line>]) -> BTreeMap<char, usize> {
    let mut b = BTreeMap::new();
    for line in lines.iter().flatten() {
        for c in line.text.chars().filter(|c| !c.is_whitespace()) {
            *b.entry(c).or_insert(0) += 1;
        }
    }
    b
}
// A matching number of delimiters is insufficient: each close must have a
// preceding open, including nested quotations within ordinary prose.
fn balanced_quotes(text: &str) -> bool {
    let mut depth = 0usize;
    for c in text.chars() {
        match c {
            '「' => depth += 1,
            '」' => {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    depth == 0
}
// Closed intersection includes source advances touching a rule, Empty Words,
// inverted boxes and zero-width boxes. A center-only test can miss precisely
// the unresolved source that makes an otherwise plausible repair ambiguous.
fn source_coverage(rect: (f32, f32, f32, f32), expected: &[usize], words: &[Word]) -> bool {
    let (x0, y0, x1, y1) = rect;
    let mut inside = Vec::new();
    for w in words {
        if ![w.x0, w.y0, w.x1, w.y1].iter().all(|v| v.is_finite()) {
            return false;
        }
        if w.x0.min(w.x1) <= x1
            && w.x0.max(w.x1) >= x0
            && w.y0.min(w.y1) <= y1
            && w.y0.max(w.y1) >= y0
        {
            inside.push(w.id);
        }
    }
    inside.sort_unstable();
    let mut expected = expected.to_vec();
    expected.sort_unstable();
    inside == expected
}
// This fragment occupies one physical vertical strip. Its original Word order
// must advance down that strip; do not impose this on multi-column table cells.
fn vertical_fragment_order(ids: &[usize], words: &[Word]) -> bool {
    ids.windows(2).all(|pair| {
        let a = &words[pair[0]];
        let b = &words[pair[1]];
        a.y0 <= b.y0 && a.y1 <= b.y1 && a.y0 + a.y1 < b.y0 + b.y1
    })
}
fn plan_at(
    a: &[Line],
    b: &[Line],
    c: &[Line],
    ai: usize,
    bi: usize,
    ci: usize,
    w: &[Word],
    r: &PageRules,
    own: &BTreeMap<usize, usize>,
) -> Option<[Vec<Line>; 3]> {
    if !quoted_marker(&a[ai], "「", w) || !quoted_marker(&b[bi + 2], "」", w) {
        return None;
    }
    let open = source_line(&a[ai], w)?;
    let ordinary = source_line(&a[ai + 1], w)?;
    if !balanced_quotes(&ordinary.text) {
        return None;
    }
    let old = b[bi].table.as_ref()?;
    let new = c[ci].table.as_ref()?;
    if !b[bi].words.is_empty() || !c[ci].words.is_empty() {
        return None;
    }
    let replacement = source_line(&b[bi + 1], w)?;
    // Split only at original Word boundaries. A producer Word spanning both
    // connective and cell content cannot be divided by this restoration rule.
    let prefix_end = (1..replacement.ids.len())
        .find(|&i| span(&replacement.ids[..i], w).is_some_and(|s| s.text == "を「"))?;
    let prefix = span(&replacement.ids[..prefix_end], w)?;
    let fragment = span(&replacement.ids[prefix_end..], w)?;
    let closing_line = source_line(&c[ci + 1], w)?;
    if !closing_line.text.starts_with('」') {
        return None;
    }
    let close = word(w, *closing_line.ids.first()?)?;
    let first = word(w, *new.cells.first()?.words.first()?)?;
    let oldclose = source_line(&b[bi + 2], w)?;
    let em = ordinary.x1 - ordinary.x0;
    let tolerance = em * 0.13;
    if !em.is_finite()
        || em <= 0.
        || !near(ordinary.x0, fragment.x0, tolerance)
        || !near(ordinary.x1, fragment.x1, tolerance)
        || !near(ordinary.y0, new.y0, em * 0.3)
        || !near(ordinary.y1, old.y0, em * 0.3)
        || new.x1 >= old.x0
        || old.x0 - new.x1 > em
        || !near(first.y0, new.y0, em * 0.4)
        || !near(open.y1, old.y0, em * 0.4)
        || open.x0 < old.x0 - em * 0.2
        || open.x1 > old.x1 + em * 0.2
        || oldclose.x0 < old.x0 - em * 0.2
        || oldclose.x1 > old.x1 + em * 0.2
        || close.x0 < new.x0 - em * 0.2
        || close.x1 > new.x1 + em * 0.2
        || !near(oldclose.y0, old.y1, em * 0.4)
        || !near(close.y0, new.y1, em * 0.4)
        || fragment.y0 <= old.y1 + em
        || !near(prefix.y1, fragment.y0, em)
        || prefix.x0 > fragment.x1
        || prefix.x1 < fragment.x0
    {
        return None;
    }
    if !stack(old, w, r, tolerance, false) || !stack(new, w, r, tolerance, true) {
        return None;
    }
    let mut strips = Vec::new();
    for left in &r.vertical {
        for right in &r.vertical {
            if right.x > left.x
                && near(right.y0, left.y0, tolerance)
                && near(right.y1, left.y1, tolerance)
                && near(right.x - left.x, new.x1 - new.x0, tolerance)
                && left.x <= fragment.x0
                && fragment.x1 <= right.x
                && left.y0 <= fragment.y0
                && fragment.y1 <= left.y1 + tolerance
                && near(left.y0, fragment.y0, em * 0.5)
                && near(left.y1, fragment.y1, em * 0.5)
                && cap(r, left.y0, left.x, right.x, tolerance)
                && !cap(r, left.y1, left.x, right.x, tolerance)
                && r.horizontal.iter().any(|h| {
                    h.x0 < new.x0 && h.x1 > old.x1 && h.y >= left.y1 && h.y - left.y1 <= em
                })
            {
                strips.push((left.x, right.x, left.y0, left.y1));
            }
        }
    }
    if strips.len() != 1 {
        return None;
    }
    let (x0, x1, y0, y1) = strips[0];
    if prefix.x0 < x0 - tolerance || prefix.x1 > x1 + tolerance {
        return None;
    }
    let primary = new.cells.first()?;
    if !source_coverage((x0, y0, x1, y1), fragment.ids, w)
        || !source_coverage(
            (primary.x0, primary.y0, primary.x1, primary.y1),
            &primary.words,
            w,
        )
        || !vertical_fragment_order(fragment.ids, w)
    {
        return None;
    }
    if a[ai..=ai + 1]
        .iter()
        .chain(&b[bi..=bi + 2])
        .chain(&c[ci..=ci + 1])
        .flat_map(ids)
        .any(|id| own.get(&id) != Some(&1))
    {
        return None;
    }
    let mut aa = a.to_vec();
    aa.swap(ai, ai + 1);
    let mut bb = b.to_vec();
    bb[bi + 1] = b[bi + 2].clone();
    bb[bi + 2] = Line::text(prefix.text, prefix.ids.to_vec());
    let mut cc = c.to_vec();
    let table = cc[ci].table.as_mut()?;
    let cell = &mut table.cells[0];
    cell.text = fragment.text + &cell.text;
    cell.words.splice(0..0, fragment.ids.iter().copied());
    // Public trace uses the outer union. The physical cell consists of two
    // disjoint strips; do not infer that the intervening rectangle is cell ink.
    cell.x0 = cell.x0.min(x0);
    cell.y0 = cell.y0.min(y0);
    cell.x1 = cell.x1.max(x1);
    cell.y1 = cell.y1.max(y1);
    table.x0 = table.x0.min(x0);
    table.y0 = table.y0.min(y0);
    table.x1 = table.x1.max(x1);
    table.y1 = table.y1.max(y1);
    cc[ci].text = table.render();
    let out = [aa, bb, cc];
    (bag(&[a.to_vec(), b.to_vec(), c.to_vec()]) == bag(&out)).then_some(out)
}
struct Plan {
    edits: Vec<(usize, Line)>,
    sources: std::collections::BTreeSet<usize>,
    range: std::ops::Range<usize>,
}

fn plans(
    lines: &[Line],
    words: &[Word],
    rules: &PageRules,
    own: &BTreeMap<usize, usize>,
) -> Vec<Plan> {
    let opens: Vec<_> = lines
        .windows(2)
        .enumerate()
        .filter(|(_, ls)| quoted_marker(&ls[0], "「", words) && ls[1].table.is_none())
        .map(|(i, _)| i)
        .collect();
    if opens.is_empty() {
        return Vec::new();
    }
    let olds: Vec<_> = lines
        .windows(3)
        .enumerate()
        .filter(|(_, ls)| ls[0].table.is_some() && quoted_marker(&ls[2], "」", words))
        .map(|(i, _)| i)
        .collect();
    let news: Vec<_> = lines
        .windows(2)
        .enumerate()
        .filter(|(_, ls)| {
            ls[0].table.is_some()
                && source_line(&ls[1], words).is_some_and(|s| s.text.starts_with('」'))
        })
        .map(|(i, _)| i)
        .collect();
    let mut found = Vec::new();
    for ai in opens {
        for &bi in &olds {
            for &ci in &news {
                if ai + 1 >= bi || bi + 2 >= ci {
                    continue;
                }
                if let Some([a, b, c]) = plan_at(lines, lines, lines, ai, bi, ci, words, rules, own)
                {
                    let sources = lines[ai..=ai + 1]
                        .iter()
                        .chain(&lines[bi..=bi + 2])
                        .chain(&lines[ci..=ci + 1])
                        .flat_map(ids)
                        .collect();
                    found.push(Plan {
                        edits: vec![
                            (ai, a[ai].clone()),
                            (ai + 1, a[ai + 1].clone()),
                            (bi + 1, b[bi + 1].clone()),
                            (bi + 2, b[bi + 2].clone()),
                            (ci, c[ci].clone()),
                        ],
                        sources,
                        range: ai..ci + 2,
                    });
                }
            }
        }
    }
    // Independent continuations can coexist. Shared evidence or intersecting
    // quotation ranges make both plans ambiguous, even with distinct Word IDs.
    let ambiguous: Vec<_> = (0..found.len())
        .map(|i| {
            (0..found.len()).any(|j| {
                i != j
                    && (!found[i].sources.is_disjoint(&found[j].sources)
                        || (found[i].range.start < found[j].range.end
                            && found[j].range.start < found[i].range.end))
            })
        })
        .collect();
    found
        .into_iter()
        .zip(ambiguous)
        .filter_map(|(p, a)| (!a).then_some(p))
        .collect()
}

pub(crate) fn restore(units: &mut [Unit], words: &[Word], rules: Option<&PageRules>) {
    let Some(rules) = rules else {
        return;
    };
    let own = owners(units);
    let mut start = 0;
    while start < units.len() {
        if !matches!(units[start], Unit::Plain(_)) {
            start += 1;
            continue;
        }
        let mut end = start;
        let mut positions = Vec::new();
        let mut lines = Vec::new();
        // Unit grouping is an upstream implementation detail. Quotation and
        // physical source evidence remain the same when Plain units are split
        // or coalesced. A new/old role boundary is never traversed.
        while end < units.len() {
            let Unit::Plain(ls) = &units[end] else {
                break;
            };
            for (li, line) in ls.iter().enumerate() {
                positions.push((end, li));
                lines.push(line.clone());
            }
            end += 1;
        }
        for plan in plans(&lines, words, rules, &own) {
            for (at, line) in plan.edits {
                let (ui, li) = positions[at];
                let Unit::Plain(ls) = &mut units[ui] else {
                    unreachable!()
                };
                ls[li] = line;
            }
        }
        start = end;
    }
}

#[cfg(test)]
#[path="quoted_inline_table_tests.rs"]
mod tests;
