//! Bind legal-outline rows to their own original printed counterpart columns.
//! Every result copies original Run slices and complete opaque tables.
use super::*;
use crate::model::{Document, NestedTable, Run, ShinkyuRow};
use crate::trace::Line;
use std::collections::HashSet;
#[derive(Clone, Debug, PartialEq, Eq)]
enum Atom {
    Char(char),
    Table(NestedTable),
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Material {
    Char(char, bool),
    Table(NestedTable, bool),
}
fn text_atoms(s: &str) -> impl Iterator<Item = Atom> + '_ {
    s.chars().filter(|c| !c.is_whitespace()).map(Atom::Char)
}
fn run_atoms(rs: &[Run], out: &mut Vec<Atom>) -> Option<()> {
    for r in rs {
        if let Some(t) = &r.table {
            if !r.text.is_empty() {
                return None;
            }
            out.push(Atom::Table(t.clone()));
        } else {
            out.extend(text_atoms(&r.text));
        }
    }
    Some(())
}
fn runs(r: &ShinkyuRow, a: bool) -> &[Run] {
    if a {
        &r.after
    } else {
        &r.before
    }
}
fn runs_mut(r: &mut ShinkyuRow, a: bool) -> &mut Vec<Run> {
    if a {
        &mut r.after
    } else {
        &mut r.before
    }
}
fn table(r: &ShinkyuRow, a: bool) -> Option<&NestedTable> {
    if a {
        r.after_table.as_ref()
    } else {
        r.before_table.as_ref()
    }
}
struct Cell {
    block: usize,
    row: usize,
    start: usize,
    end: usize,
}
struct Typed {
    atoms: Vec<Atom>,
    cells: Vec<Cell>,
}
fn typed(d: &Document, a: bool) -> Option<Typed> {
    let mut out = Typed {
        atoms: vec![],
        cells: vec![],
    };
    for (bi, b) in d.blocks.iter().enumerate() {
        if let Block::Shinkyu { rows } = b {
            for (ri, r) in rows.iter().enumerate() {
                let start = out.atoms.len();
                run_atoms(runs(r, a), &mut out.atoms)?;
                if let Some(t) = table(r, a) {
                    out.atoms.push(Atom::Table(t.clone()));
                }
                out.cells.push(Cell {
                    block: bi,
                    row: ri,
                    start,
                    end: out.atoms.len(),
                });
            }
        }
    }
    Some(out)
}
fn material(d: &Document, a: bool) -> Option<Vec<Material>> {
    let mut out = vec![];
    for b in &d.blocks {
        if let Block::Shinkyu { rows } = b {
            for r in rows {
                for run in runs(r, a) {
                    if let Some(t) = &run.table {
                        if !run.text.is_empty() {
                            return None;
                        }
                        out.push(Material::Table(t.clone(), run.underline));
                    } else {
                        out.extend(
                            run.text
                                .chars()
                                .filter(|c| *c != '\n')
                                .map(|c| Material::Char(c, run.underline)),
                        );
                    }
                }
                if let Some(t) = table(r, a) {
                    out.push(Material::Table(t.clone(), false));
                }
            }
        }
    }
    Some(out)
}
#[derive(Clone, Copy)]
struct Column {
    axis: NativeAxis,
    lo: f32,
    hi: f32,
}
fn column(p: &PageTrace, l: &Line) -> Option<Column> {
    let ids = if let Some(t) = &l.table {
        t.cells
            .iter()
            .flat_map(|c| c.words.iter().copied())
            .collect::<Vec<_>>()
    } else {
        l.words.clone()
    };
    let mut axis = None;
    let mut lo = f32::NEG_INFINITY;
    let mut hi = f32::INFINITY;
    // The original legacy Word projection and actual native glyph rectangle are
    // distinct source contracts. Both must be finite; their coordinates need not
    // equal. Extraction keeps the native witness zipped to its own Word ID.
    for id in ids {
        let w = p.words.get(id)?;
        if w.id != id || w.status != WordStatus::Used || !valid(w.source_box) {
            return None;
        }
        let NativeBounds::Glyphs(b) = *p.native_bounds.get(id)? else {
            return None;
        };
        if !valid(b) {
            return None;
        }
        let a = *p.native_axes.get(id)?;
        let b = project(b, a)?;
        if axis.is_some_and(|v| v != a) {
            return None;
        }
        axis = Some(a);
        lo = lo.max(b[1]);
        hi = hi.min(b[3]);
    }
    let axis = axis?;
    if let Some(t) = &l.table {
        let b = [t.x0, t.y0, t.x1, t.y1];
        if !valid(b) {
            return None;
        }
        for id in t.cells.iter().flat_map(|c| &c.words) {
            let NativeBounds::Glyphs(w) = p.native_bounds[*id] else {
                return None;
            };
            if !inside(b, w) {
                return None;
            }
        }
        let b = project(b, axis)?;
        lo = b[1];
        hi = b[3];
    }
    (lo.is_finite() && hi.is_finite() && lo < hi).then_some(Column { axis, lo, hi })
}
struct Printed<'a> {
    page: usize,
    unit: usize,
    part: usize,
    line: &'a Line,
    start: usize,
    end: usize,
    column: Option<Column>,
}
struct SourceSide<'a> {
    atoms: Vec<Atom>,
    lines: Vec<Printed<'a>>,
}
fn source_side<'a>(
    pages: &'a [PageTrace],
    owner: &HashSet<Source>,
    tables: &HashSet<usize>,
    after: bool,
) -> Option<SourceSide<'a>> {
    let mut out = SourceSide {
        atoms: vec![],
        lines: vec![],
    };
    let mut ti = 0;
    for (pi, p) in pages.iter().enumerate() {
        for r in &p.regions {
            for l in &r.lines {
                let table_id = if l.table.is_some() {
                    let id = ti;
                    ti += 1;
                    Some(id)
                } else {
                    None
                };
                if r.role != if after { Role::After } else { Role::Before } {
                    continue;
                }
                let ids = if let Some(t) = &l.table {
                    t.cells
                        .iter()
                        .flat_map(|c| c.words.iter().copied())
                        .collect::<Vec<_>>()
                } else {
                    l.words.clone()
                };
                let n = ids.iter().filter(|&&id| owner.contains(&(pi, id))).count();
                let own_table = table_id.is_some_and(|i| tables.contains(&i));
                if n == 0 && !own_table {
                    continue;
                }
                if n != ids.len() || l.table.is_some() != own_table {
                    return None;
                }
                let start = out.atoms.len();
                if let Some(t) = &l.table {
                    out.atoms.push(Atom::Table(t.to_nested()));
                } else {
                    out.atoms.extend(text_atoms(&l.text));
                }
                if start != out.atoms.len() {
                    out.lines.push(Printed {
                        page: pi,
                        unit: r.unit,
                        part: r.part,
                        line: l,
                        start,
                        end: out.atoms.len(),
                        column: column(p, l),
                    });
                }
            }
        }
    }
    Some(out)
}
fn group(a: &Printed<'_>, b: &Printed<'_>) -> bool {
    (a.page, a.unit, a.part) == (b.page, b.unit, b.part)
}
fn counterpart<'a, 'b>(a: &Printed<'_>, other: &'b SourceSide<'a>) -> Option<&'b Printed<'a>> {
    let ac = a.column?;
    let mut found = None;
    for b in other.lines.iter().filter(|b| group(a, b)) {
        let bc = b.column?;
        if ac.axis != bc.axis {
            return None;
        }
        if ac.lo.max(bc.lo) < ac.hi.min(bc.hi) {
            if found.is_some() {
                return None;
            }
            found = Some(b);
        }
    }
    found
}
// Standard legal change symbols, never document names or arbitrary fragments.
fn marker(s: &str) -> bool {
    matches!(
        compact(s).as_str(),
        "（新設）" | "〔新設〕" | "［新設］" | "（削る）" | "〔削る〕" | "［削る］" | "（削る。）"
    )
}
fn structural(s: &str) -> bool {
    crate::model::outline_heading(s) || crate::model::article_number(s).is_some()
}
fn equivalent(a: &str, b: &str) -> bool {
    if marker(b) {
        return true;
    }
    if let Some(n) = crate::model::article_number(a) {
        return crate::model::article_number(b) == Some(n);
    }
    crate::model::outline_heading(a)
        && crate::model::outline_heading(b)
        && a.split_whitespace().next() == b.split_whitespace().next()
}
fn caption(s: &str) -> bool {
    let mut stack = vec![];
    let mut first = true;
    let mut finished = false;
    for c in s.chars().filter(|c| !c.is_whitespace()) {
        if finished || (first && !matches!(c, '（' | '(')) {
            return false;
        }
        first = false;
        match c {
            '（' => stack.push('）'),
            '(' => stack.push(')'),
            '）' | ')' => {
                if stack.pop() != Some(c) {
                    return false;
                }
                finished = stack.is_empty();
            }
            _ => {
                if stack.is_empty() {
                    return false;
                }
            }
        }
    }
    !first && finished
}
struct Plan {
    after: bool,
    block: usize,
    row: usize,
    previous: usize,
    head: usize,
    mark: usize,
    mark_end: usize,
}
fn containing(s: &Typed, start: usize, end: usize) -> Option<&Cell> {
    let mut found = s.cells.iter().filter(|c| c.start <= start && end <= c.end);
    let c = found.next()?;
    found.next().is_none().then_some(c)
}
fn offset(haystack: &[Atom], needle: &[Atom]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    let mut at = haystack
        .windows(needle.len())
        .enumerate()
        .filter(|(_, w)| *w == needle)
        .map(|(i, _)| i);
    let first = at.next()?;
    at.next().is_none().then_some(first)
}
fn plan(
    d: &Document,
    source: &SourceSide<'_>,
    other: &SourceSide<'_>,
    after: bool,
) -> Option<Plan> {
    let own = typed(d, after)?;
    let opp = typed(d, !after)?;
    let shift = offset(&own.atoms, &source.atoms)?;
    let opposite = offset(&opp.atoms, &other.atoms)?;
    for (i, h) in source.lines.iter().enumerate() {
        if !crate::model::outline_heading(&h.line.text) || h.line.table.is_some() {
            continue;
        }
        let tail = source.lines[i + 1..]
            .iter()
            .take_while(|l| group(h, l))
            .collect::<Vec<_>>();
        let Some(j) = tail.iter().position(|l| structural(&l.line.text)) else {
            continue;
        };
        let next = tail[j];
        let Some(first) = h.column else {
            continue;
        };
        let prefix = std::iter::once(h)
            .chain(tail[..=j].iter().copied())
            .collect::<Vec<_>>();
        if prefix
            .iter()
            .any(|l| l.line.table.is_some() || l.column.is_none_or(|c| c.axis != first.axis))
            || prefix
                .windows(2)
                .any(|p| p[0].column.unwrap().hi > p[1].column.unwrap().lo)
        {
            continue;
        }
        let mut end = next.start;
        if crate::model::article_number(&next.line.text).is_some() {
            if let Some(k) = tail[..j]
                .iter()
                .position(|l| l.line.text.trim_start().starts_with(['（', '(']))
            {
                let text = tail[k..j]
                    .iter()
                    .map(|l| l.line.text.as_str())
                    .collect::<String>();
                if !caption(&text) || marker(&text) {
                    continue;
                }
                end = tail[k].start;
            }
        }
        if source.atoms[h.start..end]
            .iter()
            .any(|a| matches!(a, Atom::Char('。')))
        {
            continue;
        }
        let Some(mark) =
            counterpart(h, other).filter(|m| m.line.table.is_none() && marker(&m.line.text))
        else {
            continue;
        };
        let Some(next_other) =
            counterpart(next, other).filter(|m| equivalent(&next.line.text, &m.line.text))
        else {
            continue;
        };
        if mark.start >= next_other.start {
            continue;
        }
        let Some(cell) = containing(&own, h.start + shift, next.end + shift) else {
            continue;
        };
        if cell.start != h.start + shift || end + shift >= cell.end {
            continue;
        }
        let Some(previous) = containing(&opp, mark.start + opposite, mark.end + opposite) else {
            continue;
        };
        let Some(target) = opp
            .cells
            .iter()
            .find(|c| c.block == cell.block && c.row == cell.row)
        else {
            continue;
        };
        if previous.block != cell.block
            || previous.row > cell.row
            || mark.end > next_other.start
            || next_other.end + opposite > target.end
        {
            continue;
        }
        if previous.row == cell.row {
            if mark.start + opposite != target.start {
                continue;
            }
        } else {
            if previous.end != target.start {
                continue;
            }
            // Several new outline levels can have their symbols grouped in one old
            // cell. Each remaining complete symbol must independently own a printed
            // outline/article column inside this same destination source scope.
            let Some(previous_end) = previous
                .end
                .checked_sub(opposite)
                .filter(|&n| n <= other.atoms.len())
            else {
                continue;
            };
            let mut at = mark.end;
            let mut bound = true;
            for l in other
                .lines
                .iter()
                .filter(|l| mark.end < l.end && l.start < previous_end)
            {
                if l.start != at || l.end > previous_end || !marker(&l.line.text) || !group(h, l) {
                    bound = false;
                    break;
                }
                let Some(a) = counterpart(l, source).filter(|a| {
                    structural(&a.line.text) && end <= a.start && a.end + shift <= cell.end
                }) else {
                    bound = false;
                    break;
                };
                if a.start < h.start {
                    bound = false;
                    break;
                }
                at = l.end;
            }
            if !bound || at != previous_end {
                continue;
            }
        }
        return Some(Plan {
            after,
            block: cell.block,
            row: cell.row,
            previous: previous.row,
            head: end + shift - cell.start,
            mark: mark.start + opposite - previous.start,
            mark_end: mark.end + opposite - previous.start,
        });
    }
    None
}
fn size(rs: &[Run]) -> Option<usize> {
    let mut a = vec![];
    run_atoms(rs, &mut a)?;
    Some(a.len())
}
fn slice(rs: &[Run], start: usize, end: usize) -> Option<Vec<Run>> {
    let total = size(rs)?;
    if start > end || end > total {
        return None;
    }
    let mut at = 0;
    let mut out = vec![];
    for r in rs {
        if r.table.is_some() {
            if start <= at && at < end {
                out.push(r.clone());
            }
            at += 1;
            continue;
        }
        let mut s = String::new();
        for c in r.text.chars() {
            if start <= at && (at < end || end == total) {
                s.push(c);
            }
            if !c.is_whitespace() {
                at += 1;
            }
        }
        if !s.is_empty() {
            let mut piece = r.clone();
            piece.text = s;
            out.push(piece);
        }
    }
    // Only legacy LF at newly explicit cell edges is removed. Other whitespace,
    // character styles and complete inline tables retain their original slices.
    while let Some(r) = out.first_mut() {
        if r.table.is_some() {
            break;
        }
        r.text = r.text.trim_start_matches('\n').into();
        if r.text.is_empty() {
            out.remove(0);
        } else {
            break;
        }
    }
    while let Some(r) = out.last_mut() {
        if r.table.is_some() {
            break;
        }
        r.text = r.text.trim_end_matches('\n').into();
        if r.text.is_empty() {
            out.pop();
        } else {
            break;
        }
    }
    Some(out)
}
fn apply(d: &Document, p: Plan) -> Option<Document> {
    let mut next = d.clone();
    let Block::Shinkyu { rows } = next.blocks.get_mut(p.block)? else {
        return None;
    };
    let old = runs(rows.get(p.row)?, p.after);
    let head = slice(old, 0, p.head)?;
    let rest = slice(old, p.head, size(old)?)?;
    let old = runs(rows.get(p.previous)?, !p.after);
    let keep = slice(old, 0, p.mark)?;
    let mark = slice(old, p.mark, p.mark_end)?;
    let mut carry = slice(old, p.mark_end, size(old)?)?;
    if head.is_empty() || rest.is_empty() || mark.is_empty() {
        return None;
    }
    *runs_mut(&mut rows[p.row], p.after) = rest;
    if p.previous == p.row {
        *runs_mut(&mut rows[p.row], !p.after) = carry;
    } else {
        *runs_mut(&mut rows[p.previous], !p.after) = keep;
        carry.extend(runs(&rows[p.row], !p.after).iter().cloned());
        *runs_mut(&mut rows[p.row], !p.after) = carry;
    }
    let mut row = ShinkyuRow::default();
    *runs_mut(&mut row, p.after) = head;
    *runs_mut(&mut row, !p.after) = mark;
    rows.insert(p.row, row);
    for a in [true, false] {
        if material(d, a)? != material(&next, a)? {
            return None;
        }
    }
    Some(next)
}
fn outline_prefix(rs: &[Run]) -> bool {
    let mut text = String::new();
    for r in rs {
        if r.table.is_some() {
            return false;
        }
        for c in r.text.chars() {
            if text.is_empty() && c.is_whitespace() {
                continue;
            }
            if c == '\n' {
                return crate::model::outline_heading(&text);
            }
            text.push(c);
        }
    }
    crate::model::outline_heading(&text)
}
fn relevant(d: &Document) -> bool {
    d.blocks.iter().any(|b| {
        if let Block::Shinkyu { rows } = b {
            rows.iter()
                .any(|r| outline_prefix(&r.after) || outline_prefix(&r.before))
        } else {
            false
        }
    })
}
fn valid_scope(s: &OwnedSources) -> bool {
    s.words.iter().copied().collect::<HashSet<_>>().len() == s.words.len()
        && s.tables.iter().copied().collect::<HashSet<_>>().len() == s.tables.len()
}
fn valid_claims(
    s: &OwnedSources,
    pages: &[PageTrace],
    locations: &HashMap<Source, Vec<Location>>,
) -> bool {
    s.words.iter().all(|&(pi, id)| {
        pages
            .get(pi)
            .and_then(|p| p.words.get(id))
            .is_some_and(|w| w.id == id && w.status == WordStatus::Used)
            && locations.get(&(pi, id)).is_some_and(|rows| rows.len() == 1)
    })
}
fn repair_scope(
    article: &mut ArticleAnalysis,
    scope: &OwnedSources,
    words: &[Source],
    pages: &[PageTrace],
    source_tables: &[Option<TableSource<'_>>],
    locations: &HashMap<Source, Vec<Location>>,
) {
    if !relevant(&article.document) || !valid_scope(scope) || !valid_claims(scope, pages, locations)
    {
        return;
    }
    let owner = words.iter().copied().collect::<HashSet<_>>();
    let tables = scope.tables.iter().copied().collect::<HashSet<_>>();
    if owner.len() != words.len() {
        return;
    }
    if super::table_words(
        scope,
        &words.iter().copied().collect(),
        source_tables,
        pages,
        locations,
    )
    .is_none()
    {
        return;
    }
    let (Some(after), Some(before)) = (
        source_side(pages, &owner, &tables, true),
        source_side(pages, &owner, &tables, false),
    ) else {
        return;
    };
    loop {
        let p = plan(&article.document, &after, &before, true)
            .or_else(|| plan(&article.document, &before, &after, false));
        let Some(next) = p.and_then(|p| apply(&article.document, p)) else {
            break;
        };
        article.document = next;
    }
}
fn unique_at(text: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    let at = text.find(needle)?;
    let after = at + text[at..].chars().next()?.len_utf8();
    (!text[after..].contains(needle)).then_some(at)
}
fn complete_interval(text: &str, needle: &str) -> bool {
    unique_at(text, needle).is_some_and(|at| {
        (at == 0 || text[..at].ends_with('\n'))
            && (at + needle.len() == text.len() || text[at + needle.len()..].starts_with('\n'))
    })
}
pub(super) fn repair(
    articles: &mut [ArticleAnalysis],
    scopes: &[OwnedSources],
    pages: &[PageTrace],
) {
    if articles.len() != scopes.len() || !articles.iter().any(|a| relevant(&a.document)) {
        return;
    }
    let source_tables = super::table_sources(pages);
    let locations = super::locations(pages);
    if let Some(bound) = super::stream::owners(scopes, pages) {
        for ((article, scope), words) in articles.iter_mut().zip(scopes).zip(bound) {
            repair_scope(article, scope, &words, pages, &source_tables, &locations);
        }
    }
    // A complete, unique native unit can be certified independently when an
    // unrelated physical page prevents a global reconstruction. No partial unit,
    // conflicting explicit owner or repeated source spelling supplies authority.
    let mut first_table = 0;
    for (pi, page) in pages.iter().enumerate() {
        let first = first_table;
        first_table += page.tables.len();
        if !page
            .regions
            .iter()
            .filter(|r| matches!(r.role, Role::After | Role::Before))
            .flat_map(|r| &r.lines)
            .any(|l| crate::model::outline_heading(&l.text))
        {
            continue;
        }
        let Some(units) = super::stream::page_units(page, pi, first) else {
            continue;
        };
        for unit in units.into_iter().filter(|u| u.has_outline) {
            let native = pages
                .iter()
                .filter(|p| p.doc_text.contains(&unit.text))
                .collect::<Vec<_>>();
            if native.len() != 1 || unique_at(&native[0].doc_text, &unit.text).is_none() {
                continue;
            }
            let candidates = scopes
                .iter()
                .enumerate()
                .filter(|(_, s)| s.marked_text.contains(&unit.text))
                .collect::<Vec<_>>();
            if candidates.len() != 1 {
                continue;
            }
            let (i, scope) = candidates[0];
            if !valid_scope(scope)
                || !valid_claims(scope, pages, &locations)
                || !complete_interval(&scope.marked_text, &unit.text)
                || !relevant(&articles[i].document)
            {
                continue;
            }
            if !scope.partial_claims
                && !scope.words.is_empty()
                && unit.words.iter().any(|w| !scope.words.contains(w))
            {
                continue;
            }
            if unit.tables.iter().any(|t| !scope.tables.contains(t)) {
                continue;
            }
            if scopes.iter().enumerate().any(|(j, s)| {
                j != i
                    && (s.words.iter().any(|w| unit.words.contains(w))
                        || s.tables.iter().any(|t| unit.tables.contains(t)))
            }) {
                continue;
            }
            let local = OwnedSources::new(&unit.text, unit.words.clone());
            if local.tables != unit.tables {
                continue;
            }
            repair_scope(
                &mut articles[i],
                &local,
                &unit.words,
                pages,
                &source_tables,
                &locations,
            );
        }
    }
}
#[cfg(test)]
#[path = "section_inserted_outline_tests.rs"]
mod tests;
