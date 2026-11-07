//! Recover a sparse continuation from a preceding open, compound-header grid.
//! Empty columns are source structure, not evidence that the text is prose.
//! No wording, publication identity, expected output or distance-closing rule
//! participates. Missing native inventory, a moved grid or incomplete stroke
//! support refuses the repair.
use crate::{
    grid::{Grid, GridCell},
    lines::PageRules,
    native_source_scene::Scene,
    native_word_axes::NativeAxis,
    trace::{Line, NativeBounds, PageTrace, Role, Table, TableCell, Unit, WordStatus},
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
struct Open {
    page: usize,
    role: Role,
    table: Table,
    xs: Vec<f32>,
    size: [f32; 2],
}
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn unique(mut v: Vec<f32>) -> Vec<f32> {
    v.sort_by(f32::total_cmp);
    v.dedup();
    v
}
fn columns(t: &Table) -> Option<Vec<f32>> {
    if t.rows < 1 || t.cols < 3 {
        return None;
    }
    let mut xs = vec![None; t.cols + 1];
    let mut ys = vec![None; t.rows + 1];
    let mut owners = BTreeSet::new();
    for c in &t.cells {
        if c.rowspan == 0
            || c.colspan == 0
            || c.row + c.rowspan > t.rows
            || c.col + c.colspan > t.cols
            || ![c.x0, c.x1, c.y0, c.y1].iter().all(|v| v.is_finite())
            || c.x0 >= c.x1
            || c.y0 >= c.y1
        {
            return None;
        }
        for (slots, lo, hi, a, b) in [
            (&mut xs, c.col, c.col + c.colspan, c.x0, c.x1),
            (&mut ys, c.row, c.row + c.rowspan, c.y0, c.y1),
        ] {
            for (i, v) in [(lo, a), (hi, b)] {
                if slots[i].is_some_and(|old| old != v) {
                    return None;
                }
                slots[i] = Some(v);
            }
        }
        for row in c.row..c.row + c.rowspan {
            for col in c.col..c.col + c.colspan {
                if !owners.insert((row, col)) {
                    return None;
                }
            }
        }
    }
    let xs = xs.into_iter().collect::<Option<Vec<_>>>()?;
    let ys = ys.into_iter().collect::<Option<Vec<_>>>()?;
    (owners.len() == t.rows * t.cols
        && xs.windows(2).all(|w| w[0] < w[1])
        && ys.windows(2).all(|w| w[0] < w[1])
        && xs[0] == t.x0
        && xs[t.cols] == t.x1
        && ys[0] == t.y0
        && ys[t.rows] == t.y1)
        .then_some(xs)
}
// Coverage joins only touching source center intervals. A clipped PageRules
// fragment never closes a gap; the independent complete native stroke can.
fn coverage(mut spans: Vec<[f32; 2]>, lo: f32, hi: f32) -> bool {
    spans.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let mut end = lo;
    for [a, b] in spans {
        if a <= end && end < b {
            end = b;
        }
    }
    hi <= end
}
fn strokes(scene: &Scene) -> Vec<[f32; 4]> {
    let mut out = scene.original_lines().collect::<Vec<_>>();
    out.sort_by(|a, b| a.partial_cmp(b).unwrap());
    out.dedup();
    out
}
fn vertical(lines: &[[f32; 4]], x: f32, lo: f32, hi: f32) -> bool {
    coverage(
        lines
            .iter()
            .filter(|l| l[0] == x && l[2] == x)
            .map(|l| [l[1], l[3]])
            .collect(),
        lo,
        hi,
    )
}
fn horizontal(lines: &[[f32; 4]], y: f32, lo: f32, hi: f32) -> bool {
    coverage(
        lines
            .iter()
            .filter(|l| l[1] == y && l[3] == y)
            .map(|l| [l[0], l[2]])
            .collect(),
        lo,
        hi,
    )
}
fn is_open(t: &Table, xs: &[f32], scene: &Scene) -> bool {
    let lines = strokes(scene);
    !horizontal(&lines, t.y1, t.x0, t.x1)
        && xs.iter().all(|&x| {
            vertical(
                &lines,
                x,
                t.y0.max(
                    t.cells
                        .iter()
                        .filter(|c| c.y1 == t.y1)
                        .map(|c| c.y0)
                        .fold(t.y0, f32::max),
                ),
                t.y1,
            )
        })
}
fn seed(t: &Table, role: Role, pi: usize, p: &PageTrace, scene: &Scene) -> Option<Open> {
    let xs = columns(t)?;
    if t.rows < 3
        || !t.cells.iter().any(|c| c.row == 0 && c.rowspan > 1)
        || !t.cells.iter().any(|c| c.row == 0 && c.colspan > 1)
        || !is_open(t, &xs, scene)
    {
        return None;
    }
    let ids = t
        .cells
        .iter()
        .flat_map(|c| &c.words)
        .copied()
        .collect::<BTreeSet<_>>();
    if ids.is_empty()
        || ids.iter().any(|&id| {
            p.native_axes.get(id) != Some(&NativeAxis::Right)
                || p.native_horizontal_baselines
                    .get(id)
                    .copied()
                    .flatten()
                    .is_none()
                || !matches!(p.native_bounds.get(id), Some(NativeBounds::Glyphs(_)))
        })
    {
        return None;
    }
    Some(Open {
        page: pi,
        role,
        table: t.clone(),
        xs,
        size: [p.width, p.height],
    })
}
fn grid(xs: &[f32], scene: &Scene) -> Option<Grid> {
    let lines = strokes(scene);
    let left = *xs.first()?;
    let right = *xs.last()?;
    let spans = |x| {
        lines
            .iter()
            .filter(|l| l[0] == x && l[2] == x)
            .map(|l| [l[1], l[3]])
            .collect::<Vec<_>>()
    };
    let lv = spans(left);
    let rv = spans(right);
    let top = lv.iter().map(|l| l[0]).min_by(f32::total_cmp)?;
    let bottom = lv.iter().map(|l| l[1]).max_by(f32::total_cmp)?;
    if top >= bottom
        || rv.iter().map(|l| l[0]).min_by(f32::total_cmp)? != top
        || rv.iter().map(|l| l[1]).max_by(f32::total_cmp)? != bottom
        || !vertical(&lines, left, top, bottom)
        || !vertical(&lines, right, top, bottom)
        || horizontal(&lines, top, left, right)
    {
        return None;
    }
    let ys = unique(
        std::iter::once(top)
            .chain(std::iter::once(bottom))
            .chain(
                lines
                    .iter()
                    .filter(|l| {
                        l[1] == l[3]
                            && top < l[1]
                            && l[1] < bottom
                            && xs.contains(&l[0])
                            && xs.contains(&l[2])
                    })
                    .map(|l| l[1]),
            )
            .collect(),
    );
    // The continuing first row must retain every original column boundary.
    if !xs.iter().all(|&x| vertical(&lines, x, ys[0], ys[1])) {
        return None;
    }
    let nx = xs.len() - 1;
    let ny = ys.len() - 1;
    let mut groups = (0..nx * ny).collect::<Vec<_>>();
    fn root(g: &[usize], mut i: usize) -> usize {
        while g[i] != i {
            i = g[i];
        }
        i
    }
    fn unite(g: &mut [usize], a: usize, b: usize) {
        let a = root(g, a);
        let b = root(g, b);
        g[a] = b;
    }
    for y in 0..ny {
        for x in 0..nx {
            if x + 1 < nx {
                let edge = xs[x + 1];
                let present = vertical(&lines, edge, ys[y], ys[y + 1]);
                let partial = lines
                    .iter()
                    .any(|l| l[0] == edge && l[2] == edge && l[1] < ys[y + 1] && ys[y] < l[3]);
                if !present {
                    if partial {
                        return None;
                    }
                    unite(&mut groups, y * nx + x, y * nx + x + 1);
                }
            }
            if y + 1 < ny {
                let edge = ys[y + 1];
                let present = horizontal(&lines, edge, xs[x], xs[x + 1]);
                let partial = lines
                    .iter()
                    .any(|l| l[1] == edge && l[3] == edge && l[0] < xs[x + 1] && xs[x] < l[2]);
                if !present {
                    if partial {
                        return None;
                    }
                    unite(&mut groups, y * nx + x, (y + 1) * nx + x);
                }
            }
        }
    }
    let mut components: BTreeMap<usize, Vec<(usize, usize)>> = BTreeMap::new();
    for y in 0..ny {
        for x in 0..nx {
            components
                .entry(root(&groups, y * nx + x))
                .or_default()
                .push((x, y));
        }
    }
    let mut cells = vec![];
    for points in components.values() {
        let x0 = points.iter().map(|p| p.0).min()?;
        let x1 = points.iter().map(|p| p.0).max()? + 1;
        let y0 = points.iter().map(|p| p.1).min()?;
        let y1 = points.iter().map(|p| p.1).max()? + 1;
        if points.len() != (x1 - x0) * (y1 - y0) {
            return None;
        }
        cells.push(GridCell { x0, x1, y0, y1 });
    }
    cells.sort_by_key(|c| (c.y0, c.x0));
    Some(Grid {
        xs: xs.to_vec(),
        ys,
        cells,
    })
}
fn inside(b: [f32; 4], a: [f32; 4]) -> bool {
    a[0] <= b[0] && b[2] <= a[2] && a[1] <= b[1] && b[3] <= a[3]
}
fn intersects(b: [f32; 4], a: [f32; 4]) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}
fn rect(g: &Grid, c: &GridCell) -> [f32; 4] {
    [g.xs[c.x0], g.ys[c.y0], g.xs[c.x1], g.ys[c.y1]]
}
fn line_ids(l: &Line) -> Vec<usize> {
    l.words
        .iter()
        .chain(
            l.table
                .iter()
                .flat_map(|t| t.cells.iter().flat_map(|c| &c.words)),
        )
        .copied()
        .collect()
}
fn source_text(ids: &[usize], p: &PageTrace) -> String {
    let mut out = String::new();
    let mut prior: Option<usize> = None;
    for &id in ids {
        let w = &p.words[id];
        if let Some(prev) = prior {
            let a = &p.words[prev];
            let em = (a.source_box[3] - a.source_box[1]).min(w.source_box[3] - w.source_box[1]);
            let gap = w.source_box[0] - a.source_box[2];
            if gap >= em * 0.5 {
                out.push_str(&"　".repeat((gap / em).round().max(1.) as usize));
            }
        }
        out.push_str(&w.text);
        prior = Some(id);
    }
    out
}
fn recover(lines: &[Line], p: &PageTrace, g: &Grid, scene: &Scene) -> Option<(Vec<Line>, Table)> {
    let area = [g.left(), g.top(), g.right(), g.bottom()];
    let original = strokes(scene);
    let open_top = !horizontal(&original, g.top(), g.left(), g.right());
    let open_bottom = !horizontal(&original, g.bottom(), g.left(), g.right());
    let mut belongs = BTreeMap::new();
    for w in p.words.iter().filter(|w| w.status == WordStatus::Used) {
        let NativeBounds::Glyphs(mut b) = *p.native_bounds.get(w.id)? else {
            return None;
        };
        if !intersects(b, area) {
            continue;
        }
        let baseline = p
            .native_horizontal_baselines
            .get(w.id)
            .copied()
            .flatten()
            .filter(|v| v.is_finite())? as f32;
        // At an open page seam an authored line's em box can extend beyond
        // the vertical stroke endpoint. Its own certified baseline still lies
        // in that cell. Never shrink an em box across an internal/closed rule.
        if open_top && b[1] < g.top() && g.top() <= baseline {
            b[1] = g.top();
        }
        if open_bottom && g.bottom() < b[3] && baseline <= g.bottom() {
            b[3] = g.bottom();
        }
        if !inside(b, area) || p.native_axes.get(w.id) != Some(&NativeAxis::Right) {
            return None;
        }
        let owners = g
            .cells
            .iter()
            .enumerate()
            .filter(|(_, c)| inside(b, rect(g, c)))
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        let [owner] = owners.as_slice() else {
            return None;
        };
        belongs.insert(w.id, *owner);
    }
    // All original paint that crosses a cell must be an existing grid stroke
    // or a parallel frame displaced by less than half an original glyph em.
    // These frames decorate the grid; they never supply extra rows/columns.
    let em = p
        .words
        .iter()
        .filter(|w| {
            p.native_axes.get(w.id) == Some(&NativeAxis::Right)
                && p.native_horizontal_baselines
                    .get(w.id)
                    .copied()
                    .flatten()
                    .is_some()
        })
        .map(|w| w.source_box[3] - w.source_box[1])
        .reduce(f32::min)?;
    let pad = em * 0.35;
    for l in scene.original_lines().filter(|&l| {
        let b = [
            l[0].min(l[2]),
            l[1].min(l[3]),
            l[0].max(l[2]),
            l[1].max(l[3]),
        ];
        b[0] < area[2] && area[0] < b[2] && b[1] <= area[3] && area[1] <= b[3]
            || b[1] < area[3] && area[1] < b[3] && b[0] <= area[2] && area[0] <= b[2]
    }) {
        if l[0] == l[2] {
            if !g.xs.iter().any(|x| (x - l[0]).abs() <= pad) {
                return None;
            }
        } else if !g.ys.iter().any(|y| (y - l[1]).abs() <= pad) {
            // A padded annotation can change its column span within an empty
            // source cell. Its shifted endpoints do not authorize a new row.
            if (g.xs.contains(&l[0]) && g.xs.contains(&l[2]))
                || ![l[0], l[2]]
                    .iter()
                    .all(|end| g.xs.iter().any(|x| (x - end).abs() <= pad))
            {
                return None;
            }
        }
    }
    let mut range: Option<(usize, usize)> = None;
    let mut seen = BTreeSet::new();
    let mut pieces = vec![vec![]; g.cells.len()];
    for (i, l) in lines.iter().enumerate() {
        let ids = line_ids(l);
        let affected = ids.iter().any(|id| belongs.contains_key(id))
            || l.table
                .as_ref()
                .is_some_and(|t| [t.x0, t.y0, t.x1, t.y1] == area);
        if !affected {
            continue;
        }
        if ids.iter().any(|id| !belongs.contains_key(id)) || ids.iter().any(|id| !seen.insert(*id))
        {
            return None;
        }
        let (start, end) = range.get_or_insert((i, i));
        *start = (*start).min(i);
        *end = i;
        if let Some(t) = &l.table {
            // Reentrant source-frame/underline passes reuse the same certified
            // table rather than rewriting its existing source text projection.
            if g.cells.len() != t.cells.len()
                || t.rows != g.ys.len() - 1
                || t.cols != g.xs.len() - 1
            {
                return None;
            }
            for (c, gc) in t.cells.iter().zip(&g.cells) {
                if (c.row, c.col, c.rowspan, c.colspan)
                    != (gc.y0, gc.x0, gc.y1 - gc.y0, gc.x1 - gc.x0)
                    || [c.x0, c.y0, c.x1, c.y1] != rect(g, gc)
                    || c.words
                        .iter()
                        .any(|id| belongs.get(id) != g.cells.iter().position(|g| g == gc).as_ref())
                    || compact(&c.text)
                        != compact(
                            &c.words
                                .iter()
                                .map(|id| p.words[*id].text.as_str())
                                .collect::<String>(),
                        )
                {
                    return None;
                }
            }
            if ids.len() != belongs.len() {
                return None;
            }
            return Some((lines.to_vec(), t.clone()));
        }
        if compact(&l.text)
            != compact(
                &ids.iter()
                    .map(|id| p.words[*id].text.as_str())
                    .collect::<String>(),
            )
        {
            return None;
        }
        let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for id in ids {
            groups.entry(belongs[&id]).or_default().push(id);
        }
        let sole = groups.len() == 1;
        for (owner, mut ids) in groups {
            ids.sort_by(|a, b| p.words[*a].source_box[0].total_cmp(&p.words[*b].source_box[0]));
            let baseline = ids
                .iter()
                .filter_map(|id| p.native_horizontal_baselines[*id])
                .min_by(f64::total_cmp)?;
            let text = if sole {
                l.text.clone()
            } else {
                source_text(&ids, p)
            };
            pieces[owner].push((baseline, i, text, ids));
        }
    }
    if seen.len() != belongs.len() {
        return None;
    }
    if let Some((start, end)) = range {
        if lines[start..=end]
            .iter()
            .any(|l| line_ids(l).iter().any(|id| !belongs.contains_key(id)))
        {
            return None;
        }
    }
    let mut cells = vec![];
    for (i, gc) in g.cells.iter().enumerate() {
        pieces[i].sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let b = rect(g, gc);
        cells.push(TableCell {
            row: gc.y0,
            col: gc.x0,
            rowspan: gc.y1 - gc.y0,
            colspan: gc.x1 - gc.x0,
            x0: b[0],
            y0: b[1],
            x1: b[2],
            y1: b[3],
            text: pieces[i]
                .iter()
                .map(|v| v.2.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
            words: pieces[i].iter().flat_map(|v| v.3.iter().copied()).collect(),
        });
    }
    let t = Table {
        x0: g.left(),
        y0: g.top(),
        x1: g.right(),
        y1: g.bottom(),
        rows: g.ys.len() - 1,
        cols: g.xs.len() - 1,
        cells,
    };
    let (start, next) = range.map(|(a, b)| (a, b + 1)).unwrap_or((0, 0));
    let mut replacement = lines[..start].to_vec();
    replacement.push(Line {
        text: t.render(),
        words: vec![],
        table: Some(t.clone()),
    });
    replacement.extend_from_slice(&lines[next..]);
    Some((replacement, t))
}

pub(crate) fn restore(
    pages: &mut [PageTrace],
    xhtml: &str,
    rules: &[Option<PageRules>],
) -> Vec<(usize, usize)> {
    let doc = scraper::Html::parse_document(xhtml);
    let selector = scraper::Selector::parse("page").unwrap();
    let nodes = doc.select(&selector).collect::<Vec<_>>();
    if nodes.len() != pages.len() {
        return vec![];
    }
    let Some(mut units) = pages
        .iter()
        .map(crate::native_role_continuation::frames::existing_units)
        .collect::<Option<Vec<_>>>()
    else {
        return vec![];
    };
    let mut count = 0;
    for (p, u) in pages.iter().zip(&units) {
        let (text, tables) = crate::vertical::doc_text_of(u, &mut count);
        if text != p.doc_text
            || tables != p.tables
            || u.iter()
                .map(Unit::text)
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
                != p.text
        {
            return vec![];
        }
    }
    let mut state: Vec<Open> = vec![];
    let mut links = vec![];
    let mut changed = false;
    for pi in 0..pages.len() {
        let p = &pages[pi];
        let Some(scene) = rules
            .get(pi)
            .and_then(Option::as_ref)
            .and_then(|r| crate::native_source_scene::for_page(nodes[pi], r))
            .filter(|s| {
                s.matches(&p.words) && s.original_lines().all(|l| l.iter().all(|v| v.is_finite()))
            })
        else {
            state.clear();
            continue;
        };
        let mut next = vec![];
        // Exactly one comparison scope; a body/footer may follow it.
        if units[pi]
            .iter()
            .filter(|u| matches!(u, Unit::Shinkyu { .. }))
            .count()
            != 1
        {
            state.clear();
            continue;
        }
        let Some(Unit::Shinkyu {
            preamble,
            after,
            before,
            outside,
            ..
        }) = units[pi]
            .iter_mut()
            .find(|u| matches!(u, Unit::Shinkyu { .. }))
        else {
            unreachable!()
        };
        let continuation = !crate::native_role_continuation::explicit_page(p)
            && preamble.is_empty()
            && outside.is_empty();
        for (role, parts) in [(Role::After, after), (Role::Before, before)] {
            if parts.len() > 1 {
                continue;
            }
            if continuation {
                if let Some(open) = state
                    .iter()
                    .find(|o| o.role == role && o.page + 1 == pi && o.size == [p.width, p.height])
                {
                    if let Some(g) = grid(&open.xs, &scene) {
                        if parts.is_empty() {
                            parts.push(vec![]);
                        }
                        // An entirely empty side still has an explicit source
                        // grid, but contributes no text and needs no model run.
                        if let Some((replacement, t)) = recover(&parts[0], p, &g, &scene) {
                            changed |= replacement != parts[0];
                            parts[0] = replacement;
                            links.push((open.page, open.table.clone(), pi, t.clone()));
                            if parts[0].last().and_then(|l| l.table.as_ref()) == Some(&t)
                                && is_open(&t, &open.xs, &scene)
                            {
                                next.push(Open {
                                    page: pi,
                                    role,
                                    table: t,
                                    xs: open.xs.clone(),
                                    size: open.size,
                                });
                            }
                            continue;
                        }
                    }
                }
            }
            if let Some(t) = parts
                .last()
                .and_then(|lines| lines.last())
                .and_then(|l| l.table.as_ref())
            {
                if let Some(open) = seed(t, role, pi, p, &scene) {
                    next.push(open);
                }
            }
        }
        state = next;
    }
    if changed {
        let mut count = 0;
        for (p, u) in pages.iter_mut().zip(units) {
            let (doc_text, tables) = crate::vertical::doc_text_of(&u, &mut count);
            p.native_comparison_labels = u
                .iter()
                .map(|u| match u {
                    Unit::Shinkyu { labels, .. } => *labels,
                    _ => None,
                })
                .collect();
            p.text = u
                .iter()
                .map(Unit::text)
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            p.regions = u
                .into_iter()
                .enumerate()
                .flat_map(|(i, u)| u.regions(i))
                .collect();
            p.doc_text = doc_text;
            p.tables = tables;
        }
    }
    let mut offsets = vec![];
    let mut count = 0;
    for p in pages.iter() {
        offsets.push(count);
        count += p.tables.len();
    }
    links
        .into_iter()
        .filter_map(|(pa, a, pb, b)| {
            let ids = |page: usize, t: &Table| {
                let ids = pages[page]
                    .tables
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| *s == t)
                    .map(|(i, _)| i)
                    .collect::<Vec<_>>();
                match ids.as_slice() {
                    [id] => Some(offsets[page] + id),
                    _ => None,
                }
            };
            Some((ids(pa, &a)?, ids(pb, &b)?))
        })
        .collect()
}

/// Consumers receive complete table equality plus positional cell provenance;
/// a matching substring in some unrelated table can never borrow a border.
pub(crate) fn groups(
    pages: &[PageTrace],
    pairs: &[(usize, usize)],
) -> Vec<crate::media::TableContinuation> {
    let tables = pages.iter().flat_map(|p| &p.tables).collect::<Vec<_>>();
    let mut out = vec![];
    for &(start, _) in pairs
        .iter()
        .filter(|(a, _)| !pairs.iter().any(|(_, b)| b == a))
    {
        let Some(source) = tables.get(start) else {
            continue;
        };
        let mut table = source.to_nested();
        let mut cells = source
            .cells
            .iter()
            .map(|c| (start, c.row, c.col, c.row, c.col))
            .collect::<Vec<_>>();
        let mut current = start;
        let mut seen = BTreeSet::from([start]);
        let mut valid = true;
        loop {
            let next = pairs
                .iter()
                .filter(|(a, _)| *a == current)
                .map(|(_, b)| *b)
                .collect::<Vec<_>>();
            let [next] = next.as_slice() else {
                if !next.is_empty() {
                    valid = false;
                }
                break;
            };
            if !seen.insert(*next) {
                valid = false;
                break;
            }
            let Some(source) = tables.get(*next) else {
                valid = false;
                break;
            };
            let n = table.rows.len();
            let mut additions = vec![];
            for c in &source.cells {
                let (row, col) = if c.row == 0 {
                    let owner = table
                        .merges
                        .iter()
                        .find(|m| m.row <= n - 1 && n - 1 < m.row + m.rowspan && m.col == c.col);
                    owner.map(|m| (m.row, m.col)).unwrap_or((n - 1, c.col))
                } else {
                    (n + c.row - 1, c.col)
                };
                additions.push((*next, c.row, c.col, row, col));
            }
            if !crate::model::join_open_table_row(&mut table, &source.to_nested()) {
                valid = false;
                break;
            }
            cells.extend(additions);
            current = *next;
        }
        if valid && seen.len() > 1 {
            out.push(crate::media::TableContinuation { table, cells });
        }
    }
    out
}

#[cfg(test)]
#[path = "native_sparse_table_continuations_tests.rs"]
mod tests;
