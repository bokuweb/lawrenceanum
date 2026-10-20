//! Join a wrapped, explicitly framed after-cell whose opposite cell is empty.
//!
//! This runs after line/table layout. It moves original Line objects and their
//! IDs; it does not recreate words or infer a continuation from prose alone.
use crate::grid::{find_grids, Grid};
use crate::lines::PageRules;
use crate::trace::{Line, Unit, Word, WordStatus};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug)]
struct Rect {
    l: f32,
    r: f32,
    t: f32,
    b: f32,
}
impl Rect {
    fn contains(self, w: &Word) -> bool {
        w.x0 >= self.l - 0.1 && w.x1 <= self.r + 0.1 && w.y0 >= self.t - 0.1 && w.y1 <= self.b + 0.1
    }
    fn intersects(self, w: &Word) -> bool {
        w.x0 < self.r - 0.1 && w.x1 > self.l + 0.1 && w.y0 < self.b - 0.1 && w.y1 > self.t + 0.1
    }
    fn width(self) -> f32 {
        self.r - self.l
    }
    fn height(self) -> f32 {
        self.b - self.t
    }
}
fn cell(g: &Grid, x: usize, y: usize) -> Rect {
    Rect {
        l: g.xs[x],
        r: g.xs[x + 1],
        t: g.ys[y],
        b: g.ys[y + 1],
    }
}
fn exact_grid(g: &Grid, nx: usize) -> bool {
    g.xs.len() == nx + 1
        && g.ys.len() == 3
        && g.cells.len() == nx * 2
        && (0..nx).all(|x| {
            (0..2).all(|y| {
                g.cells
                    .iter()
                    .filter(|c| c.x0 == x && c.x1 == x + 1 && c.y0 == y && c.y1 == y + 1)
                    .count()
                    == 1
            })
        })
}
fn visit(unit: &Unit, mut f: impl FnMut(&Line)) {
    match unit {
        Unit::Plain(ls) => ls.iter().for_each(f),
        Unit::Shinkyu {
            preamble,
            after,
            before,
            outside,
        } => {
            for l in preamble {
                f(l)
            }
            for l in after.iter().chain(before).chain(outside).flatten() {
                f(l)
            }
        }
    }
}
fn source_lines<'a>(
    ls: &[Line],
    words: &'a [Word],
    owners: &HashMap<usize, usize>,
) -> Option<Vec<&'a Word>> {
    if ls.is_empty() {
        return None;
    }
    let mut used = HashSet::new();
    let mut result = Vec::new();
    for l in ls {
        if l.table.is_some() || l.words.is_empty() {
            return None;
        }
        let mut text = String::new();
        for id in &l.words {
            if !used.insert(*id) || owners.get(id) != Some(&1) {
                return None;
            }
            let w = words.iter().find(|w| w.id == *id)?;
            if w.status != WordStatus::Used
                || w.text.is_empty()
                || !w.x0.is_finite()
                || !w.x1.is_finite()
                || !w.y0.is_finite()
                || !w.y1.is_finite()
                || w.x1 <= w.x0
                || w.y1 - w.y0 < 2.0 * (w.x1 - w.x0)
            {
                return None;
            }
            text.extend(w.text.chars().filter(|c| !c.is_whitespace()));
            result.push(w);
        }
        if text
            != l.text
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>()
        {
            return None;
        }
    }
    Some(result)
}
fn bounds(ws: &[&Word]) -> Rect {
    Rect {
        l: ws.iter().map(|w| w.x0).fold(f32::INFINITY, f32::min),
        r: ws.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max),
        t: ws.iter().map(|w| w.y0).fold(f32::INFINITY, f32::min),
        b: ws.iter().map(|w| w.y1).fold(f32::NEG_INFINITY, f32::max),
    }
}
fn ordered_columns(
    ls: &[Line],
    words: &[Word],
    owners: &HashMap<usize, usize>,
) -> Option<(Vec<Rect>, f32)> {
    let ws = source_lines(ls, words, owners)?;
    let em = ws.iter().map(|w| w.x1 - w.x0).fold(f32::INFINITY, f32::min);
    if em < 4.0 || ws.iter().any(|w| ((w.x1 - w.x0) / em - 1.0).abs() > 0.05) {
        return None;
    }
    let columns: Vec<_> = ls
        .iter()
        .map(|l| {
            bounds(
                &ws.iter()
                    .copied()
                    .filter(|w| l.words.contains(&w.id))
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    if columns.iter().any(|c| c.width() > em * 1.1)
        || columns.windows(2).any(|w| {
            let d = w[0].l - w[1].l;
            d < em * 1.1 || d > em * 1.8
        })
    {
        return None;
    }
    Some((columns, em))
}
// A crossing parenthesis must open in the first physical cell and close in
// the second. A complete first cell, extra closer or mismatched bracket fails.
fn bracket_tail(first: &[Line], tail: &[Line]) -> bool {
    fn feed(ls: &[Line], stack: &mut Vec<char>) -> bool {
        for c in ls.iter().flat_map(|l| l.text.chars()) {
            match c {
                '（' | '「' | '『' | '【' | '〔' => stack.push(c),
                '）' | '」' | '』' | '】' | '〕' => {
                    let want = match c {
                        '）' => '（',
                        '」' => '「',
                        '』' => '『',
                        '】' => '【',
                        _ => '〔',
                    };
                    if stack.pop() != Some(want) {
                        return false;
                    }
                }
                _ => {}
            }
        }
        true
    }
    let mut s = Vec::new();
    feed(first, &mut s) && !s.is_empty() && feed(tail, &mut s) && s.is_empty()
}
fn short_parenthetical(ls: &[Line]) -> bool {
    ls.len() == 1
        && ls[0].table.is_none()
        && ls[0].text.starts_with('（')
        && ls[0].text.ends_with('）')
        && (3..=10).contains(&ls[0].text.chars().count())
        && !ls[0].text.contains(['。', '、'])
        && {
            let mut depth = 0i32;
            ls[0].text.chars().all(|c| {
                if c == '（' {
                    depth += 1
                }
                if c == '）' {
                    depth -= 1
                }
                depth >= 0
            }) && depth == 0
        }
}
// Header statuses are finalized after Unit layout. At this stage consumed
// header words can still be Used, but must be unowned and spell the exact
// full original label inside its real ruled cell.
fn header_cell(r: Rect, expected: &str, words: &[Word], owners: &HashMap<usize, usize>) -> bool {
    let mut ws: Vec<_> = words.iter().filter(|w| r.intersects(w)).collect();
    if ws.is_empty()
        || ws.iter().any(|w| {
            !r.contains(w)
                || !matches!(w.status, WordStatus::Used | WordStatus::HeaderLabel)
                || owners.contains_key(&w.id)
        })
    {
        return false;
    }
    ws.sort_by(|a, b| a.y0.total_cmp(&b.y0));
    let em = ws.iter().map(|w| w.x1 - w.x0).fold(f32::INFINITY, f32::min);
    em > 0.0
        && ws.iter().all(|w| (w.x0 - ws[0].x0).abs() < em * 0.25)
        && ws.iter().map(|w| w.text.as_str()).collect::<String>() == expected
}
fn covered(r: Rect, own: &[&Word], words: &[Word]) -> bool {
    own.iter().all(|w| r.contains(w))
        && words
            .iter()
            .filter(|w| r.intersects(w))
            .all(|w| own.iter().any(|o| o.id == w.id))
}
fn plan(
    first: &Unit,
    next: &Unit,
    words: &[Word],
    rules: &PageRules,
    grids: &[Grid],
    owners: &HashMap<usize, usize>,
) -> Option<usize> {
    let Unit::Shinkyu {
        after,
        before,
        outside,
        ..
    } = first
    else {
        return None;
    };
    let Unit::Plain(body) = next else { return None };
    if after.len() != 1
        || before.len() != 1
        || outside.iter().any(|v| !v.is_empty())
        || after[0].len() < 3
        || !short_parenthetical(&before[0])
        || body.len() < 3
    {
        return None;
    }
    let (ac, em) = ordered_columns(&after[0], words, owners)?;
    let aw = source_lines(&after[0], words, owners)?;
    let bw = source_lines(&before[0], words, owners)?;
    let sources: Vec<_> = grids
        .iter()
        .filter(|g| exact_grid(g, 2))
        .filter(|g| {
            let a = cell(g, 0, 0);
            let b = cell(g, 0, 1);
            let label = Rect {
                l: g.xs[1],
                r: g.xs[2],
                t: g.top(),
                b: g.bottom(),
            };
            (a.height() - b.height()).abs() < em * 0.1
                && a.height() > em * 10.0
                && label.width() >= em
                && label.width() <= em * 3.0
                && covered(a, &aw, words)
                && covered(b, &bw, words)
                && (ac.last().unwrap().l - a.l).abs() < em * 0.5
                && a.r - ac[0].r < em
                && header_cell(cell(g, 1, 0), "改正後", words, owners)
                && header_cell(cell(g, 1, 1), "改正前", words, owners)
        })
        .collect();
    if sources.len() != 1 {
        return None;
    }
    let s = sources[0];
    let source = cell(s, 0, 0);
    let mut matches = Vec::new();
    for g in grids.iter().filter(|g| exact_grid(g, 1)) {
        let upper = cell(g, 0, 0);
        let lower = cell(g, 0, 1);
        if upper.t <= s.bottom() + em
            || upper.l <= s.right()
            || (upper.width() - source.width()).abs() > em * 0.25
            || (upper.height() - source.height()).abs() > em * 0.1
            || (lower.height() - upper.height()).abs() > em * 0.1
            || words.iter().any(|w| lower.intersects(w))
        {
            continue;
        }
        // The first cell finishes at the left edge of the upper tier; the
        // continuation starts at the right edge beyond the same tier separator.
        if !rules.horizontal.iter().any(|h| {
            h.y > s.bottom()
                && h.y < g.top()
                && (h.x0 - s.left()).abs() < em * 0.5
                && (h.x1 - g.right()).abs() < em * 0.5
        }) {
            continue;
        }
        let n = body
            .iter()
            .take_while(|l| {
                !l.words.is_empty()
                    && l.words.iter().all(|id| {
                        words
                            .iter()
                            .find(|w| w.id == *id)
                            .is_some_and(|w| upper.contains(w))
                    })
            })
            .count();
        if n < 3 {
            continue;
        }
        let tail = &body[..n];
        let Some(tw) = source_lines(tail, words, owners) else {
            continue;
        };
        let Some((tc, te)) = ordered_columns(tail, words, owners) else {
            continue;
        };
        if (te / em - 1.0).abs() > 0.05
            || !covered(upper, &tw, words)
            || upper.r - tc[0].r < 0.0
            || upper.r - tc[0].r > em * 0.75
            || tc.last().unwrap().l - upper.l > em * 0.75
            || tc[0].t - upper.t < 0.0
            || tc[0].t - upper.t > em * 2.0
            || tw.iter().any(|w| w.y0 < upper.t + em * 0.5)
            || !bracket_tail(&after[0], tail)
        {
            continue;
        }
        // A heading in a purported continuation signals a different article.
        if tail.iter().any(|l| l.text.starts_with(['〇', '○'])) {
            continue;
        }
        matches.push(n);
    }
    (matches.len() == 1).then(|| matches[0])
}

/// Caller restricts this to upright display coordinates, before Unit text and
/// article segmentation. Other words/lines/tables and unit indices stay fixed.
pub(crate) fn restore(
    units: &mut [Unit],
    words: &[Word],
    rules: Option<&PageRules>,
    width: f32,
    height: f32,
) {
    // Almost all pages are ordinary text/tables. Do not rediscover native
    // frames unless an adjacent Shinkyu/Plain pair can have this shape.
    if !units.windows(2).any(|pair| {
        matches!((&pair[0], &pair[1]),
        (Unit::Shinkyu { after, before, outside, .. }, Unit::Plain(body))
        if after.len()==1 && before.len()==1 && after[0].len()>=3
            && short_parenthetical(&before[0]) && outside.iter().all(Vec::is_empty)
            && body.len()>=3)
    }) {
        return;
    }
    let Some(rules) = rules else { return };
    if words.iter().map(|w| w.id).collect::<HashSet<_>>().len() != words.len() {
        return;
    }
    let grids = find_grids(rules, width, height);
    if grids.len() < 2 {
        return;
    }
    let mut owners = HashMap::new();
    for u in units.iter() {
        visit(u, |l| {
            for id in l
                .words
                .iter()
                .chain(l.table.iter().flat_map(|t| &t.cells).flat_map(|c| &c.words))
            {
                *owners.entry(*id).or_insert(0) += 1;
            }
        });
    }
    for i in 0..units.len().saturating_sub(1) {
        let Some(n) = plan(&units[i], &units[i + 1], words, rules, &grids, &owners) else {
            continue;
        };
        let (head, tail) = units.split_at_mut(i + 1);
        let Unit::Plain(body) = &mut tail[0] else {
            unreachable!()
        };
        let moved: Vec<_> = body.drain(..n).collect();
        let Unit::Shinkyu { after, before, .. } = &mut head[i] else {
            unreachable!()
        };
        after.push(moved);
        before.push(Vec::new());
    }
}
