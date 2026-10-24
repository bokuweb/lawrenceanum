//! Attach a source-typed section masthead to its own child field publication.
//! This does not change lexical segmentation or numbered-root lookup. Original
//! Runs, paragraph boundaries, tables, styles and notice references are copied.
use crate::{
    lines::PageRules,
    model::Block,
    native_word_axes::NativeAxis,
    trace::{ContentStyles, NativeBounds, NativeStyles, PageTrace, Role, WordStatus},
    ArticleAnalysis,
};
use std::collections::{BTreeSet, HashMap};
type Rect = [f32; 4];
type Source = (usize, usize);
type Location = (usize, usize, usize);
/// Word IDs alone cannot own a blank table. Retain the original internal table
/// marker identities before rendering ArticleAnalysis.text or copying Runs.
pub(crate) struct OwnedSources {
    marked_text: String,
    words: Vec<Source>,
    tables: Vec<usize>,
}
impl OwnedSources {
    pub(crate) fn new(marked_text: &str, words: Vec<Source>) -> Self {
        Self {
            marked_text: marked_text.into(),
            words,
            tables: marked_text
                .lines()
                .filter_map(crate::trace::parse_table_marker)
                .collect(),
        }
    }
}
struct TableSource<'a> {
    page: usize,
    table: &'a crate::trace::Table,
}
fn table_sources(pages: &[PageTrace]) -> Vec<Option<TableSource<'_>>> {
    let mut out = vec![];
    for (pi, p) in pages.iter().enumerate() {
        let actual = p
            .regions
            .iter()
            .flat_map(|r| &r.lines)
            .filter_map(|l| l.table.as_ref())
            .collect::<Vec<_>>();
        let bound =
            actual.len() == p.tables.len() && actual.iter().zip(&p.tables).all(|(a, b)| *a == b);
        for i in 0..p.tables.len() {
            out.push(bound.then(|| TableSource {
                page: pi,
                table: actual[i],
            }));
        }
    }
    out
}
fn table_words(
    scope: &OwnedSources,
    owner: &BTreeSet<Source>,
    tables: &[Option<TableSource<'_>>],
    pages: &[PageTrace],
    map: &HashMap<Source, Vec<Location>>,
) -> Option<BTreeSet<Source>> {
    if scope.tables.iter().collect::<BTreeSet<_>>().len() != scope.tables.len() {
        return None;
    }
    let mut opaque = BTreeSet::new();
    for &index in &scope.tables {
        let s = tables.get(index)?.as_ref()?;
        let t = s.table;
        let b = [t.x0, t.y0, t.x1, t.y1];
        if !valid(b) || t.rows == 0 || t.cols == 0 {
            return None;
        }
        for c in &t.cells {
            let cb = [c.x0, c.y0, c.x1, c.y1];
            if !valid(cb)
                || !inside(b, cb)
                || c.rowspan == 0
                || c.colspan == 0
                || c.row.checked_add(c.rowspan)? > t.rows
                || c.col.checked_add(c.colspan)? > t.cols
            {
                return None;
            }
            for &id in &c.words {
                let key = (s.page, id);
                let word = pages.get(s.page)?.words.get(id)?;
                if word.id != id
                    || word.status != WordStatus::Used
                    || !owner.contains(&key)
                    || !opaque.insert(key)
                {
                    return None;
                }
            }
        }
    }
    // A word from a Table must carry that same original marker, never a
    // value-matched substitute or a fragment cut out of an opaque Table.
    for key in owner {
        let locs = map.get(key)?;
        if locs.len() != 1 {
            return None;
        }
        let (pi, ri, li) = locs[0];
        if pages[pi].regions[ri].lines[li].table.is_some() && !opaque.contains(key) {
            return None;
        }
    }
    Some(opaque)
}
fn frame_owns_child(
    frame: Rect,
    pi: usize,
    scope: &OwnedSources,
    tables: &[Option<TableSource<'_>>],
    pages: &[PageTrace],
    opaque: &BTreeSet<Source>,
) -> Option<()> {
    for &index in &scope.tables {
        let s = tables.get(index)?.as_ref()?;
        let t = s.table;
        if s.page != pi || !inside(frame, [t.x0, t.y0, t.x1, t.y1]) {
            return None;
        }
    }
    for &(page, id) in &scope.words {
        if page != pi {
            return None;
        }
        let p = pages.get(page)?;
        let word = p.words.get(id)?;
        let b = match p.native_bounds.get(id)? {
            NativeBounds::Glyphs(b) => *b,
            // An existing complete opaque table supplies its own source
            // extent; a legacy box can only corroborate inclusion in it.
            NativeBounds::Compatibility if opaque.contains(&(page, id)) => {
                let b = word.source_box;
                let containing = scope.tables.iter().any(|&index| {
                    tables[index].as_ref().is_some_and(|s| {
                        s.page == page
                            && s.table.cells.iter().any(|c| {
                                c.words.contains(&id) && inside([c.x0, c.y0, c.x1, c.y1], b)
                            })
                    })
                });
                if !containing {
                    return None;
                }
                b
            }
            _ => return None,
        };
        if !valid(b) || !inside(frame, b) {
            return None;
        }
    }
    Some(())
}

fn compact(t: &str) -> String {
    t.chars().filter(|c| !c.is_whitespace()).collect()
}
fn valid(b: Rect) -> bool {
    b.iter().all(|v| v.is_finite()) && b[0] < b[2] && b[1] < b[3]
}
fn union(a: Rect, b: Rect) -> Rect {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}
fn inside(a: Rect, b: Rect) -> bool {
    a[0] <= b[0] && a[1] <= b[1] && b[2] <= a[2] && b[3] <= a[3]
}
fn touches(a: Rect, b: Rect) -> bool {
    a[0] <= b[2] && b[0] <= a[2] && a[1] <= b[3] && b[1] <= a[3]
}
fn project(b: Rect, axis: NativeAxis) -> Option<Rect> {
    Some(match axis {
        NativeAxis::Right => b,
        NativeAxis::Down => [b[1], -b[2], b[3], -b[0]],
        NativeAxis::Left => [-b[2], -b[3], -b[0], -b[1]],
        NativeAxis::Up => [-b[3], b[0], -b[1], b[2]],
        _ => return None,
    })
}
fn point(x: f32, y: f32, axis: NativeAxis) -> Option<(f32, f32)> {
    let b = project([x, y, x, y], axis)?;
    Some((b[0], b[1]))
}
fn content(page: &PageTrace, ids: &[usize]) -> Option<NativeStyles> {
    let mut out = NativeStyles::new();
    for &id in ids {
        let word = page.words.get(id)?;
        if word.id != id
            || word.status != WordStatus::Used
            || word.text.is_empty()
            || word.raw.is_empty()
        {
            return None;
        }
        let all = page.native_styles.get(id)?.as_ref()?;
        let ContentStyles::Glyphs(s) = page.native_content_styles.get(id)? else {
            return None;
        };
        if all.is_empty()
            || s.is_empty()
            || !s.is_subset(all)
            || all.iter().any(|&(_, v)| {
                let x = f64::from_bits(v);
                !x.is_finite() || x <= 0.
            })
        {
            return None;
        }
        out.extend(s.iter().copied());
    }
    (!out.is_empty()).then_some(out)
}
struct Heading {
    rect: Rect,
    axis: NativeAxis,
    fonts: BTreeSet<usize>,
    size: f64,
}
fn heading(page: &PageTrace, ids: &[usize]) -> Option<Heading> {
    let styles = content(page, ids)?;
    let sizes = styles.iter().map(|s| s.1).collect::<BTreeSet<_>>();
    if sizes.len() != 1 {
        return None;
    }
    let axis = *page.native_axes.get(*ids.first()?)?;
    project([0., 0., 1., 1.], axis)?;
    let mut rect = None;
    for &id in ids {
        if page.native_axes.get(id) != Some(&axis) {
            return None;
        }
        let NativeBounds::Glyphs(b) = page.native_bounds.get(id)? else {
            return None;
        };
        if !valid(*b) {
            return None;
        }
        rect = Some(rect.map_or(*b, |r| union(r, *b)));
    }
    Some(Heading {
        rect: rect?,
        axis,
        fonts: styles.iter().map(|s| s.0).collect(),
        size: f64::from_bits(*sizes.first()?),
    })
}
fn locations(pages: &[PageTrace]) -> HashMap<Source, Vec<Location>> {
    let mut out: HashMap<Source, Vec<Location>> = HashMap::new();
    for (pi, p) in pages.iter().enumerate() {
        for (ri, r) in p.regions.iter().enumerate() {
            for (li, l) in r.lines.iter().enumerate() {
                for &id in l.words.iter().chain(
                    l.table
                        .iter()
                        .flat_map(|t| t.cells.iter().flat_map(|c| &c.words)),
                ) {
                    out.entry((pi, id)).or_default().push((pi, ri, li));
                }
            }
        }
    }
    out
}
fn owned_rows(
    sources: &[Source],
    map: &HashMap<Source, Vec<Location>>,
) -> Option<BTreeSet<Location>> {
    let unique = sources.iter().copied().collect::<BTreeSet<_>>();
    if unique.is_empty() || unique.len() != sources.len() {
        return None;
    }
    unique
        .into_iter()
        .map(|id| {
            let ls = map.get(&id)?;
            (ls.len() == 1).then_some(ls[0])
        })
        .collect()
}
fn whole_line(pages: &[PageTrace], loc: Location, owner: &BTreeSet<Source>) -> Option<Vec<usize>> {
    let (pi, ri, li) = loc;
    let p = pages.get(pi)?;
    let r = p.regions.get(ri)?;
    let l = r.lines.get(li)?;
    if r.role != Role::Body
        || l.table.is_some()
        || l.words.is_empty()
        || l.words.iter().any(|&id| !owner.contains(&(pi, id)))
    {
        return None;
    }
    let text = l
        .words
        .iter()
        .map(|&id| p.words.get(id).map(|w| w.text.as_str()))
        .collect::<Option<Vec<_>>>()?
        .join("");
    (compact(&text) == compact(&l.text)).then(|| l.words.clone())
}
fn quotes_before(pages: &[PageTrace], loc: Location) -> Option<()> {
    let mut stack = vec![];
    let (pi, ri, li) = loc;
    for (pj, p) in pages.iter().enumerate().take(pi + 1) {
        for (rj, r) in p.regions.iter().enumerate() {
            for (lj, l) in r.lines.iter().enumerate() {
                if pj == pi && (rj, lj) == (ri, li) {
                    return stack.is_empty().then_some(());
                }
                for c in l.text.chars() {
                    match c {
                        '「' => stack.push('」'),
                        '『' => stack.push('』'),
                        '」' | '』' => {
                            if stack.pop() != Some(c) {
                                return None;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    None
}
#[path = "section_masthead.rs"]
mod masthead;
type RuleSegment = ((f32, f32), (f32, f32));
fn rule_segments(rule: &PageRules) -> Option<Vec<RuleSegment>> {
    let mut segments = vec![];
    for r in &rule.horizontal {
        if !r.x0.is_finite() || !r.x1.is_finite() || !r.y.is_finite() || r.x0 >= r.x1 {
            return None;
        }
        segments.push(((r.x0, r.y), (r.x1, r.y)));
    }
    for r in &rule.vertical {
        if !r.y0.is_finite() || !r.y1.is_finite() || !r.x.is_finite() || r.y0 >= r.y1 {
            return None;
        }
        segments.push(((r.x, r.y0), (r.x, r.y1)));
    }
    Some(segments)
}
fn fenced(
    rule: &PageRules,
    parent: &Heading,
    child: &Heading,
    page: &PageTrace,
    pi: usize,
    owner: &BTreeSet<Source>,
) -> bool {
    let envelope = union(parent.rect, child.rect);
    for &frame in &rule.enclosures {
        if !valid(frame) {
            return true;
        }
        if !(inside(frame, parent.rect) && inside(frame, child.rect)) && touches(frame, envelope) {
            return true;
        }
    }
    let a = project(parent.rect, parent.axis).unwrap();
    let b = project(child.rect, child.axis).unwrap();
    let low = a[0].min(b[0]);
    let high = a[2].max(b[2]);
    let Some(segments) = rule_segments(rule) else {
        return true;
    };
    let own = masthead::rulings(rule, &segments, parent, child, page, pi, owner);
    segments
        .into_iter()
        .enumerate()
        .any(|(index, ((x0, y0), (x1, y1)))| {
            if own.contains(&index) {
                return false;
            }
            let (u, v) = point(x0, y0, parent.axis).unwrap();
            let (s, t) = point(x1, y1, parent.axis).unwrap();
            (v == t && a[3] <= v && v <= b[1] && u.min(s) <= low && high <= u.max(s))
                || (u == s
                    && ((a[2] <= u && u <= b[0]) || (b[2] <= u && u <= a[0]))
                    && v.min(t) <= a[1].min(b[1])
                    && a[3].max(b[3]) <= v.max(t))
        })
}
fn unaccounted(
    page: &PageTrace,
    pi: usize,
    owner: &BTreeSet<Source>,
    envelope: Rect,
    axis: NativeAxis,
) -> bool {
    page.words.iter().enumerate().any(|(id,w)|{
  if owner.contains(&(pi,id)){return false}if id!=w.id{return true}
  let native=matches!(page.native_bounds.get(id),Some(NativeBounds::Glyphs(_)));
  let b=match page.native_bounds.get(id){Some(NativeBounds::Glyphs(b))=>*b,_=>[w.x0,w.y0,w.x1,w.y1]};
  if !valid(b){return true}if !touches(b,envelope){return false}
  if w.raw.chars().all(char::is_whitespace)&&w.text.chars().all(char::is_whitespace)&&matches!(page.native_content_styles.get(id),Some(ContentStyles::Glyphs(s))if s.is_empty()){return false}
  let decoration=page.native_rulings.get(id).is_some_and(|r|r.matches_axis(axis)&&r.rect().is_some());
  !(native&&decoration&&w.status==WordStatus::Empty&&w.text.is_empty()&&!w.raw.is_empty())
 })
}
fn certified(
    parent: &ArticleAnalysis,
    child: &ArticleAnalysis,
    parent_sources: &OwnedSources,
    child_sources: &OwnedSources,
    pages: &[PageTrace],
    rules: &[Option<PageRules>],
    map: &HashMap<Source, Vec<Location>>,
    tables: &[Option<TableSource<'_>>],
) -> Option<()> {
    if !crate::segment::is_section_label(&parent.text)
        || crate::segment::is_field_label(&parent.text)
        || !parent.notices.is_empty()
        || !parent_sources.tables.is_empty()
    {
        return None;
    }
    let mut rendered = String::new();
    for b in &parent.document.blocks {
        let Block::Paragraph { runs } = b else {
            return None;
        };
        for run in runs {
            if run.table.is_some() {
                return None;
            }
            rendered.push_str(&run.text);
        }
    }
    if compact(&rendered) != compact(&parent.text) {
        return None;
    }
    let pr = owned_rows(&parent_sources.words, map)?;
    let cr = owned_rows(&child_sources.words, map)?;
    let po = parent_sources
        .words
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let co = child_sources.words.iter().copied().collect::<BTreeSet<_>>();
    if !po.is_disjoint(&co) {
        return None;
    }
    let &(pi, ri, first) = pr.first()?;
    let &(pj, rj, mut after) = pr.last()?;
    if pi != pj || ri != rj {
        return None;
    }
    let opaque = table_words(child_sources, &co, tables, pages, map)?;
    let page = pages.get(pi)?;
    let region = page.regions.get(ri)?;
    let rule = rules.get(pi)?.as_ref()?;
    let mut pids = vec![];
    let mut ptext = String::new();
    for li in first..=after {
        let l = region.lines.get(li)?;
        if l.text.trim().is_empty() {
            continue;
        }
        pids.extend(whole_line(pages, (pi, ri, li), &po)?);
        ptext.push_str(&l.text);
    }
    if compact(&ptext) != compact(&parent.text) {
        return None;
    }
    let p = heading(page, &pids)?;
    if cr.first()?.0 != pi || cr.first()?.1 != ri {
        return None;
    }
    after += 1;
    while region
        .lines
        .get(after)
        .is_some_and(|l| l.text.trim().is_empty())
    {
        after += 1
    }
    if cr.first() != Some(&(pi, ri, after)) {
        return None;
    }
    quotes_before(pages, (pi, ri, first))?;
    let mut fids = vec![];
    let mut ftext = String::new();
    let field;
    loop {
        let line = region.lines.get(after)?;
        fids.extend(whole_line(pages, (pi, ri, after), &co)?);
        ftext.push_str(&line.text);
        let h = heading(page, &fids)?;
        if h.axis != p.axis || h.fonts != p.fonts || p.size <= h.size {
            return None;
        }
        after += 1;
        if crate::segment::is_field_label(&ftext) {
            field = h;
            break;
        }
    }
    if !compact(&child.text).starts_with(&compact(&ftext)) {
        return None;
    }
    while region
        .lines
        .get(after)
        .is_some_and(|l| l.text.trim().is_empty())
    {
        after += 1
    }
    let body = region.lines.get(after)?;
    if crate::segment::is_section_label(&body.text) {
        return None;
    }
    let bids = whole_line(pages, (pi, ri, after), &co)?;
    content(page, &bids)?;
    if bids.iter().any(|&id| {
        page.native_axes
            .get(id)
            .and_then(|&axis| project([0., 0., 1., 1.], axis))
            .is_none()
    }) {
        return None;
    }
    let a = project(p.rect, p.axis)?;
    let b = project(field.rect, field.axis)?;
    if a[3] > b[1] || fenced(rule, &p, &field, page, pi, &po) {
        return None;
    }
    for &frame in &rule.enclosures {
        if inside(frame, p.rect) && inside(frame, field.rect) {
            frame_owns_child(frame, pi, child_sources, tables, pages, &opaque)?;
        }
    }
    // A flat child Article may already own unrelated intervening material.
    // Only the proven heading Words may establish an empty heading domain.
    let owner = po
        .iter()
        .copied()
        .chain(fids.iter().map(|&id| (pi, id)))
        .collect();
    let envelope = union(p.rect, field.rect);
    if page
        .tables
        .iter()
        .chain(
            page.regions
                .iter()
                .flat_map(|r| &r.lines)
                .filter_map(|l| l.table.as_ref()),
        )
        .any(|table| {
            let rect = [table.x0, table.y0, table.x1, table.y1];
            !valid(rect) || touches(rect, envelope)
        })
    {
        return None;
    }
    if unaccounted(page, pi, &owner, union(p.rect, field.rect), p.axis) {
        return None;
    }
    Some(())
}
pub(crate) fn repair(
    articles: &mut Vec<ArticleAnalysis>,
    sources: &mut Vec<OwnedSources>,
    pages: &[PageTrace],
    rules: &[Option<PageRules>],
) {
    if articles.len() != sources.len() {
        return;
    }
    if !articles.windows(2).any(|pair| {
        crate::segment::is_section_label(&pair[0].text)
            && !crate::segment::is_field_label(&pair[0].text)
            && pair[0].notices.is_empty()
    }) {
        return;
    }
    if articles
        .iter()
        .zip(sources.iter())
        .any(|(article, source)| {
            crate::segment::is_section_label(&article.text) && source.words.is_empty()
        })
    {
        stream::restore(sources, pages);
    }
    let map = locations(pages);
    let tables = table_sources(pages);
    let mut i = 0;
    while i + 1 < articles.len() {
        if certified(
            &articles[i],
            &articles[i + 1],
            &sources[i],
            &sources[i + 1],
            pages,
            rules,
            &map,
            &tables,
        )
        .is_some()
        {
            let mut parent = articles.remove(i);
            let child = &mut articles[i];
            parent.text.push('\n');
            parent.text.push_str(&child.text);
            child.text = parent.text;
            parent.document.blocks.append(&mut child.document.blocks);
            child.document.blocks = parent.document.blocks;
            let mut owned = sources.remove(i);
            owned.words.append(&mut sources[i].words);
            owned.tables.append(&mut sources[i].tables);
            sources[i] = owned;
        } else {
            i += 1
        }
    }
}
#[path = "section_source_stream.rs"]
mod stream;
#[cfg(test)]
#[path = "section_context_tests.rs"]
mod tests;

/// Certify the printed heading/domain geometry for a source-only section root.
/// The caller supplies complete original Line word lists and their first source
/// location. The category grammar is independent of this native source proof.
/// A section title may naturally use a different font from its masthead.
pub(crate) fn printed_root(
    page: &PageTrace,
    rules: &PageRules,
    region: usize,
    line: usize,
    header: &[usize],
    body: &[usize],
) -> bool {
    (|| {
        let own = header.iter().map(|&id| (0, id)).collect::<BTreeSet<_>>();
        let title = body.iter().map(|&id| (0, id)).collect::<BTreeSet<_>>();
        if own.len() != header.len() || title.len() != body.len() || !own.is_disjoint(&title) {
            return None;
        }
        quotes_before(std::slice::from_ref(page), (0, region, line))?;
        let parent = heading(page, header)?;
        let child = heading(page, body)?;
        if parent.axis != child.axis || parent.size <= child.size {
            return None;
        }
        let a = project(parent.rect, parent.axis)?;
        let b = project(child.rect, parent.axis)?;
        if a[3] > b[1] {
            return None;
        }
        let segments = rule_segments(rules)?;
        if masthead::rulings(rules, &segments, &parent, &child, page, 0, &own).is_empty()
            || fenced(rules, &parent, &child, page, 0, &own)
        {
            return None;
        }
        let owner = own.union(&title).copied().collect();
        let envelope = union(parent.rect, child.rect);
        if unaccounted(page, 0, &owner, envelope, parent.axis)
            || page
                .tables
                .iter()
                .chain(
                    page.regions
                        .iter()
                        .flat_map(|r| &r.lines)
                        .filter_map(|l| l.table.as_ref()),
                )
                .any(|table| {
                    let rect = [table.x0, table.y0, table.x1, table.y1];
                    !valid(rect) || touches(rect, envelope)
                })
        {
            return None;
        }
        Some(())
    })()
    .is_some()
}
