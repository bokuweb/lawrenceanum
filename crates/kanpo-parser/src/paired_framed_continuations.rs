//! Preserve both cells of an explicitly ruled new/old table across a tier wrap.
//! Runs after layout; moves complete original Lines without changing Words.
use crate::grid::{find_grids, Grid};
use crate::lines::PageRules;
use crate::trace::{Line, Unit, Word, WordStatus};
use std::collections::{HashMap, HashSet};
#[derive(Clone, Copy)]
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
fn exact_grid(g: &Grid, n: usize) -> bool {
    g.xs.len() == n + 1
        && g.ys.len() == 3
        && g.cells.len() == n * 2
        && (0..n).all(|x| {
            (0..2).all(|y| {
                g.cells
                    .iter()
                    .filter(|c| c.x0 == x && c.x1 == x + 1 && c.y0 == y && c.y1 == y + 1)
                    .count()
                    == 1
            })
        })
}
fn caps(g: &Grid, rules: &PageRules) -> bool {
    g.ys.iter().all(|y| {
        let mut hs: Vec<_> = rules
            .horizontal
            .iter()
            .filter(|h| (h.y - y).abs() < 0.1)
            .map(|h| (h.x0, h.x1))
            .collect();
        hs.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut merged: Vec<(f32, f32)> = Vec::new();
        for (l, r) in hs {
            if let Some(last) = merged.last_mut() {
                if l <= last.1 + 0.1 {
                    last.1 = last.1.max(r);
                    continue;
                }
            }
            merged.push((l, r));
        }
        merged
            .iter()
            .any(|(l, r)| (*l - g.left()).abs() < 0.1 && (*r - g.right()).abs() < 0.1)
    })
}
fn visit(u: &Unit, mut f: impl FnMut(&Line)) {
    match u {
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
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn bounds(ws: &[&Word]) -> Rect {
    Rect {
        l: ws.iter().map(|w| w.x0).fold(f32::INFINITY, f32::min),
        r: ws.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max),
        t: ws.iter().map(|w| w.y0).fold(f32::INFINITY, f32::min),
        b: ws.iter().map(|w| w.y1).fold(f32::NEG_INFINITY, f32::max),
    }
}
struct Columns<'a> {
    words: Vec<&'a Word>,
    boxes: Vec<Rect>,
    em: f32,
}
fn columns<'a>(
    ls: &[Line],
    words: &'a [Word],
    owners: &HashMap<usize, usize>,
) -> Option<Columns<'a>> {
    if ls.is_empty() {
        return None;
    }
    let mut seen = HashSet::new();
    let mut source = Vec::new();
    let mut boxes = Vec::new();
    for l in ls {
        if l.table.is_some() || l.words.is_empty() || l.text.starts_with(['〇', '○']) {
            return None;
        }
        let mut ws = Vec::new();
        for id in &l.words {
            let w = words.iter().find(|w| w.id == *id)?;
            if !seen.insert(*id)
                || owners.get(id) != Some(&1)
                || w.status != WordStatus::Used
                || w.text.is_empty()
                || ![w.x0, w.x1, w.y0, w.y1].iter().all(|v| v.is_finite())
                || w.x1 <= w.x0
                || w.y1 <= w.y0
            {
                return None;
            }
            ws.push(w);
        }
        if compact(&ws.iter().map(|w| w.text.as_str()).collect::<String>()) != compact(&l.text) {
            return None;
        }
        boxes.push(bounds(&ws));
        source.extend(ws);
    }
    let em = source
        .iter()
        .map(|w| w.x1 - w.x0)
        .fold(f32::INFINITY, f32::min);
    if em < 3.
        || em > 24.
        || source
            .iter()
            .any(|w| ((w.x1 - w.x0) / em - 1.).abs() > 0.05)
        || boxes
            .iter()
            .any(|b| b.width() > em * 1.1 || b.height() < em * 2.)
        || boxes.windows(2).any(|p| p[0].l - p[1].l < em * 0.5)
    {
        return None;
    }
    Some(Columns {
        words: source,
        boxes,
        em,
    })
}
fn covered(r: Rect, own: &[&Word], words: &[Word]) -> bool {
    own.iter().all(|w| r.contains(w))
        && words
            .iter()
            .filter(|w| r.intersects(w))
            .all(|w| own.iter().any(|x| x.id == w.id))
}
fn header(r: Rect, text: &str, words: &[Word], owners: &HashMap<usize, usize>) -> bool {
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
    let em = ws[0].x1 - ws[0].x0;
    em > 0.
        && ws.iter().all(|w| (w.x0 - ws[0].x0).abs() < em * 0.25)
        && ws.iter().map(|w| w.text.as_str()).collect::<String>() == text
}
// Both physical cells share a column lattice. Blank columns may occur on one
// side, but cannot hide a gap in both sides or an unrelated offset column.
fn lattice(a: &Columns<'_>, b: &Columns<'_>, r: Rect) -> Option<f32> {
    let em = a.em;
    if (b.em / em - 1.).abs() > 0.05 {
        return None;
    }
    let mut xs: Vec<_> = a.boxes.iter().chain(&b.boxes).map(|b| b.l).collect();
    xs.sort_by(f32::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() < em * 0.25);
    if xs.len() < 3 || xs[0] - r.l > em * 0.75 || r.r - (xs.last()? + em) > em * 0.75 {
        return None;
    }
    let ds: Vec<_> = xs.windows(2).map(|w| w[1] - w[0]).collect();
    if ds.iter().any(|d| *d < em * 1.1 || *d > em * 1.8) {
        return None;
    }
    Some(ds.iter().sum::<f32>() / ds.len() as f32)
}
fn prefix(ls: &[Line], r: Rect, words: &[Word]) -> usize {
    ls.iter()
        .take_while(|l| {
            !l.words.is_empty()
                && l.words.iter().all(|id| {
                    words
                        .iter()
                        .find(|w| w.id == *id)
                        .is_some_and(|w| r.contains(w))
                })
        })
        .count()
}
// At least one side must physically end mid-sentence at the left/bottom edge,
// then resume at the top/right edge of the matching lower cell.
fn cut(first: &[Line], cols: &Columns<'_>, r: Rect, tail: &Columns<'_>, next: Rect) -> bool {
    let Some(last) = cols.boxes.last() else {
        return false;
    };
    let Some(start) = tail.boxes.first() else {
        return false;
    };
    let incomplete = first
        .last()
        .and_then(|l| l.text.chars().rev().find(|c| !c.is_whitespace()))
        .is_some_and(|c| !matches!(c, '。' | '！' | '？' | '）' | '」' | '』' | '〕' | '】'));
    incomplete
        && (last.l - r.l).abs() < cols.em * 0.75
        && r.b - last.b >= 0.
        && r.b - last.b < cols.em * 0.9
        && next.r - start.r >= 0.
        && next.r - start.r < cols.em * 0.75
        && start.t - next.t >= cols.em * 0.5
        && start.t - next.t < cols.em * 2.
}
fn plan(
    head: &Unit,
    after_body: &Unit,
    before_body: &Unit,
    words: &[Word],
    owners: &HashMap<usize, usize>,
    rules: &PageRules,
    grids: &[Grid],
) -> Option<(usize, usize)> {
    let Unit::Shinkyu {
        after,
        before,
        outside,
        ..
    } = head
    else {
        return None;
    };
    let (Unit::Plain(al), Unit::Plain(bl)) = (after_body, before_body) else {
        return None;
    };
    if after.len() != 1
        || before.len() != 1
        || after[0].len() < 3
        || before[0].len() < 3
        || outside.iter().any(|ls| !ls.is_empty())
    {
        return None;
    }
    let ac = columns(&after[0], words, owners)?;
    let bc = columns(&before[0], words, owners)?;
    let em = ac.em;
    let mut matches = Vec::new();
    for s in grids.iter().filter(|g| exact_grid(g, 2) && caps(g, rules)) {
        let sa = cell(s, 0, 0);
        let sb = cell(s, 0, 1);
        let label = cell(s, 1, 0);
        if (sa.height() - sb.height()).abs() > em * 0.1
            || sa.height() < em * 8.
            || label.width() < em
            || label.width() > em * 4.
            || !covered(sa, &ac.words, words)
            || !covered(sb, &bc.words, words)
            || !header(cell(s, 1, 0), "改正後", words, owners)
            || !header(cell(s, 1, 1), "改正前", words, owners)
        {
            continue;
        }
        let Some(sp) = lattice(&ac, &bc, sa) else {
            continue;
        };
        for g in grids.iter().filter(|g| exact_grid(g, 1) && caps(g, rules)) {
            let da = cell(g, 0, 0);
            let db = cell(g, 0, 1);
            if da.t - s.bottom() < em
                || da.t - s.bottom() > em * 6.
                || g.right() <= s.right() + em
                || (da.height() - sa.height()).abs() > em * 0.1
                || (db.height() - da.height()).abs() > em * 0.1
                || da.width() < em * 6.
            {
                continue;
            }
            if !rules.vertical.iter().any(|v| {
                (v.x - g.left()).abs() < 0.1 && v.y0 <= g.top() + 0.1 && v.y1 >= g.bottom() - 0.1
            }) || !rules.horizontal.iter().any(|h| {
                h.y > s.bottom() + em * 0.5
                    && h.y < g.top() - em * 0.5
                    && (h.x0 - s.left()).abs() < em * 0.5
                    && (h.x1 - g.right()).abs() < em * 0.5
            }) {
                continue;
            }
            let na = prefix(al, da, words);
            let nb = prefix(bl, db, words);
            if na < 3 || nb < 3 {
                continue;
            }
            let Some(at) = columns(&al[..na], words, owners) else {
                continue;
            };
            let Some(bt) = columns(&bl[..nb], words, owners) else {
                continue;
            };
            if (at.em / em - 1.).abs() > 0.05
                || !covered(da, &at.words, words)
                || !covered(db, &bt.words, words)
            {
                continue;
            }
            let Some(dp) = lattice(&at, &bt, da) else {
                continue;
            };
            if (dp / sp - 1.).abs() > 0.1 {
                continue;
            }
            if !cut(&after[0], &ac, sa, &at, da) && !cut(&before[0], &bc, sb, &bt, db) {
                continue;
            }
            matches.push((na, nb));
        }
    }
    if matches.len() == 1 {
        matches.pop()
    } else {
        None
    }
}
/// Upright display coordinates only, before text rendering/article splitting.
pub(crate) fn restore(
    units: &mut [Unit],
    words: &[Word],
    rules: Option<&PageRules>,
    width: f32,
    height: f32,
) {
    if units.len()<3||!units.windows(3).any(|u|matches!((&u[0],&u[1],&u[2]),(Unit::Shinkyu{after,before,..},Unit::Plain(_),Unit::Plain(_))if after.len()==1&&before.len()==1)){return}
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
    for i in 0..units.len() - 2 {
        let Some((na, nb)) = plan(
            &units[i],
            &units[i + 1],
            &units[i + 2],
            words,
            &owners,
            rules,
            &grids,
        ) else {
            continue;
        };
        let (head, rest) = units.split_at_mut(i + 1);
        let (a, b) = rest.split_at_mut(1);
        let Unit::Plain(al) = &mut a[0] else {
            unreachable!()
        };
        let Unit::Plain(bl) = &mut b[0] else {
            unreachable!()
        };
        let at = al.drain(..na).collect();
        let bt = bl.drain(..nb).collect();
        let Unit::Shinkyu { after, before, .. } = &mut head[i] else {
            unreachable!()
        };
        after.push(at);
        before.push(bt);
    }
}
