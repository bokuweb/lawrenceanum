//! Opt-in CFF outline underline recovery. Source text must match the complete
//! document field before any Run is changed; native Analysis stays available.
use crate::{
    model::{Block, NestedTable, Run},
    trace::{PageTrace, Role, WordStatus},
    Analysis,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

type Rect = [f64; 4];
type Location = (usize, usize, Option<usize>);

#[derive(Clone, Deserialize)]
struct Scene {
    glyphs: usize,
    rectangle_ids: Vec<usize>,
    rectangles: Vec<Rect>,
    rulings: Vec<Rect>,
    unknown: Vec<usize>,
    images: Vec<Rect>,
    lines: Vec<(Rect, Rect)>,
    fills: Vec<Rect>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(crate) struct Basis {
    side: [i8; 2],
    flow: [i8; 2],
}
impl Basis {
    pub(crate) fn glyph(g: &kanpo_pdf::Glyph, axes: kanpo_pdf::SourceTextAxes) -> Option<Self> {
        let (x, y) = (g.end_x - g.x, g.end_y - g.y);
        if ![x, y].iter().all(|v| v.is_finite()) {
            return None;
        }
        let flow = match (x, y) {
            (x, 0.) if x > 0. => [1, 0],
            (x, 0.) if x < 0. => [-1, 0],
            (0., y) if y > 0. => [0, 1],
            (0., y) if y < 0. => [0, -1],
            _ => return None,
        };
        let side = if g.vertical {
            axes.x
        } else {
            axes.y.map(|n| -n)
        };
        (side[0] * flow[0] + side[1] * flow[1] == 0).then_some(Self { side, flow })
    }
    pub(crate) fn point(self, p: [f64; 2]) -> [f64; 2] {
        [
            self.side[0] as f64 * p[0] + self.side[1] as f64 * p[1],
            self.flow[0] as f64 * p[0] + self.flow[1] as f64 * p[1],
        ]
    }
    pub(crate) fn rect(self, b: Rect) -> Rect {
        let p = [
            self.point([b[0], b[1]]),
            self.point([b[0], b[3]]),
            self.point([b[2], b[1]]),
            self.point([b[2], b[3]]),
        ];
        [
            p.iter().map(|p| p[0]).reduce(f64::min).unwrap(),
            p.iter().map(|p| p[1]).reduce(f64::min).unwrap(),
            p.iter().map(|p| p[0]).reduce(f64::max).unwrap(),
            p.iter().map(|p| p[1]).reduce(f64::max).unwrap(),
        ]
    }
    fn scene(self, s: &Scene) -> Scene {
        Scene {
            glyphs: s.glyphs,
            rectangle_ids: s.rectangle_ids.clone(),
            rectangles: s.rectangles.iter().map(|&b| self.rect(b)).collect(),
            rulings: s.rulings.iter().map(|&b| self.rect(b)).collect(),
            unknown: s.unknown.clone(),
            images: s.images.iter().map(|&b| self.rect(b)).collect(),
            fills: s.fills.iter().map(|&b| self.rect(b)).collect(),
            lines: s
                .lines
                .iter()
                .map(|&(c, b)| (self.rect(c), self.rect(b)))
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    Paragraph,
    After,
    Before,
}
impl Field {
    fn index(self) -> usize {
        match self {
            Self::Paragraph => 0,
            Self::After => 1,
            Self::Before => 2,
        }
    }
    fn role(role: Role) -> Self {
        match role {
            Role::After => Self::After,
            Role::Before => Self::Before,
            _ => Self::Paragraph,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct GlyphBinding {
    pub page: usize,
    pub glyph: usize,
    pub word: usize,
    pub decoration_glyphs: Vec<usize>,
    pub decoration_components: Vec<DecorationComponent>,
    /// Distinct parallel painted lanes, not the number of glyphs or segments.
    /// None retains the evidence without choosing between writing directions
    /// or certifying short/partial parallel marks.
    pub parallel_strokes: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DecorationComponent {
    pub glyph: usize,
    pub component: usize,
    pub bounds: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SourceWord {
    pub page: usize,
    pub word: usize,
}

/// Offsets count non-whitespace Unicode scalars in field_text(), across all
/// articles. A source Word is retained even if its raw glyphs form one token.
#[derive(Debug, Clone, Serialize)]
pub struct ParallelUnderline {
    pub field: Field,
    pub offset: usize,
    pub length: usize,
    pub parallel_strokes: usize,
    pub source_word: SourceWord,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnresolvedField {
    pub field: Field,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceIssue {
    pub page: usize,
    pub word: Option<usize>,
    pub glyphs: Vec<usize>,
    pub reason: String,
}

/// Recovery covers cardinal CFF rectangle chains using original text axes.
/// Path-drawn decoration and ambiguous source fields remain pending.
#[derive(Debug, Clone, Serialize)]
pub struct UnderlinedAnalysis {
    pub scope: &'static str,
    pub coverage_verified: bool,
    pub native: Analysis,
    pub styled: Analysis,
    pub glyph_bindings: Vec<GlyphBinding>,
    pub applied_fields: Vec<Field>,
    pub unresolved_fields: Vec<UnresolvedField>,
    pub source_issues: Vec<SourceIssue>,
    pub parallel_underlines: Vec<ParallelUnderline>,
    pub unresolved_parallel_words: Vec<SourceWord>,
    #[serde(skip_serializing_if="Vec::is_empty")]
    pub line_projections:Vec<crate::source_line_projection::Record>,
}

fn positive(b: Rect) -> bool {
    b.iter().all(|n| n.is_finite()) && b[0] < b[2] && b[1] < b[3]
}
fn contains(a: Rect, b: Rect) -> bool {
    a[0] <= b[0] && b[2] <= a[2] && a[1] <= b[1] && b[3] <= a[3]
}
fn overlaps(a: Rect, b: Rect) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

#[derive(Clone)]
struct Placed {
    id: usize,
    word: usize,
    location: Location,
    baseline: f64,
    center: f64,
    bounds: Rect,
}

#[derive(Clone)]
struct PaintHit {
    index: usize,
    basis: Basis,
    full_primary_support: bool,
}

fn same_source_row(own: &[&Placed], placed: &[Placed]) -> bool {
    let Some(first) = own.first() else { return false; };
    if own.iter().any(|p| p.location != first.location) {
        return false;
    }
    // Different fonts in one original bbox Word may have different text
    // origins. A shared source baseline must still be witnessed for every
    // candidate, either by that glyph or by an overlapping glyph of its own
    // original Word. Equal cell ownership alone does not join separate rows.
    placed.iter().filter(|q| q.word == first.word && q.location == first.location)
        .any(|anchor| own.iter().all(|p| {
            p.baseline == anchor.baseline || placed.iter().any(|q| {
                q.word == p.word && q.location == p.location
                    && q.baseline == anchor.baseline
                    && p.bounds[0] < q.bounds[2] && q.bounds[0] < p.bounds[2]
            })
        }))
}

fn decoration_paints(
    placed: &[Placed],
    native: &kanpo_pdf::Page,
    scene: &Scene,
) -> BTreeMap<usize, Vec<(usize, bool)>> {
    let mut marked = BTreeMap::<usize, Vec<(usize, bool)>>::new();
    if !scene.unknown.is_empty() {
        return marked;
    }
    for (index, (&id, &rect)) in scene
        .rectangle_ids
        .iter()
        .zip(&scene.rectangles)
        .enumerate()
    {
        let Some(g) = native.glyphs.get(id) else {
            continue;
        };
        if g.invisible || !positive(rect) || rect[2] - rect[0] >= rect[3] - rect[1] {
            continue;
        }
        let support = scene
            .rulings
            .iter()
            .copied()
            .find(|&b| contains(b, rect))
            .unwrap_or(rect);
        if placed.iter().any(|p| overlaps(support, p.bounds)) {
            continue;
        }
        // A rule joined to an orthogonal rule is a frame boundary, not a local
        // annotation. Check the actual stroke paint, not a fuzzy line distance.
        if scene
            .lines
            .iter()
            .any(|(center, ink)| center[1] == center[3] && overlaps(support, *ink))
        {
            continue;
        }
        if scene.rectangles.iter().enumerate().any(|(other, b)| {
            other != index
                && b[2] - b[0] > b[3] - b[1]
                && b[0] <= support[2]
                && support[0] <= b[2]
                && b[1] <= support[3]
                && support[1] <= b[3]
        }) {
            continue;
        }
        let candidates = placed
            .iter()
            .filter(|p| {
                p.bounds[2] <= rect[0]
                    && rect[0] - p.bounds[2] < p.bounds[2] - p.bounds[0]
                    && rect[1] <= p.center
                    && p.center < rect[3]
            })
            .collect::<Vec<_>>();
        // A long rule can cover narrow body glyphs and a wider inline unit.
        // A nearer glyph only occludes the primary span it actually occupies.
        let own = candidates
            .iter()
            .copied()
            .filter(|p| {
                !placed.iter().any(|q| {
                    q.bounds[2] > p.bounds[2]
                        && q.bounds[2] <= rect[0]
                        && q.bounds[1] <= p.center
                        && p.center < q.bounds[3]
                })
            })
            .collect::<Vec<_>>();
        // Kerning can put two punctuation glyphs at one origin. Both already
        // belong to the same original line/cell; an overprint in another line
        // does not acquire that line's annotation.
        if !same_source_row(&own, placed) {
            continue;
        }
        for p in own {
            let corridor = [
                p.bounds[0],
                support[1].max(p.bounds[1]),
                support[2],
                support[3].min(p.bounds[3]),
            ];
            if !positive(corridor)
                || scene
                    .images
                    .iter()
                    .chain(&scene.fills)
                    .any(|&b| overlaps(corridor, b))
                || scene.rectangles.iter().enumerate().any(|(other, &b)| {
                    other != index && b[2] - b[0] >= b[3] - b[1] && overlaps(corridor, b)
                })
            {
                continue;
            }
            marked
                .entry(p.id)
                .or_default()
                .push((index, support[3] - support[1] >= p.bounds[3] - p.bounds[1]));
        }
    }
    marked
}

#[cfg(test)]
fn decorations(
    placed: &[Placed],
    native: &kanpo_pdf::Page,
    scene: &Scene,
) -> BTreeMap<usize, Vec<usize>> {
    decoration_paints(placed, native, scene)
        .into_iter()
        .map(|(id, hits)| {
            (
                id,
                hits.into_iter()
                    .map(|(i, _)| scene.rectangle_ids[i])
                    .collect(),
            )
        })
        .collect()
}

fn parallel_strokes(hits: &[PaintHit], scene: &Scene) -> Option<usize> {
    let first = hits.first()?;
    if hits.iter().any(|h| h.basis != first.basis) {
        return None;
    }
    let mut spans = hits
        .iter()
        .map(|h| {
            let b = h.basis.rect(scene.rectangles[h.index]);
            (b[0], b[2], h.full_primary_support)
        })
        .collect::<Vec<_>>();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let mut lanes: Vec<(f64, f64, bool)> = Vec::new();
    for (a, b, full) in spans {
        if let Some(last) = lanes.last_mut().filter(|last| a <= last.1) {
            last.1 = last.1.max(b);
            last.2 |= full;
        } else {
            lanes.push((a, b, full));
        }
    }
    (lanes.len() == 1 || lanes.iter().all(|lane| lane.2)).then_some(lanes.len())
}

fn canonical_styles<T: Clone + PartialEq>(
    word: &crate::trace::Word,
    values: &[T],
) -> Option<Vec<T>> {
    let text = compact(&word.text);
    if let Some(first) = values
        .first()
        .filter(|first| values.iter().all(|v| v == *first))
    {
        return Some(vec![first.clone(); text.chars().count()]);
    }
    let raw = word.raw.chars().collect::<Vec<_>>();
    if raw.len() != values.len() {
        return None;
    }
    let normalized = raw
        .iter()
        .map(|c| crate::normalize::word_text(&c.to_string()))
        .collect::<Vec<_>>();
    if compact(&normalized.concat()) != text {
        return None;
    }
    Some(
        normalized
            .iter()
            .zip(values)
            .flat_map(|(text, value)| {
                text.chars()
                    .filter(|c| !c.is_whitespace())
                    .map(move |_| value.clone())
            })
            .collect(),
    )
}

fn locations(page: &PageTrace) -> BTreeMap<usize, Vec<Location>> {
    let mut out = BTreeMap::<usize, Vec<Location>>::new();
    for (ri, region) in page.regions.iter().enumerate() {
        for (li, line) in region.lines.iter().enumerate() {
            for &id in &line.words {
                out.entry(id).or_default().push((ri, li, None));
            }
            if let Some(table) = &line.table {
                for (ci, cell) in table.cells.iter().enumerate() {
                    for &id in &cell.words {
                        out.entry(id).or_default().push((ri, li, Some(ci)));
                    }
                }
            }
        }
    }
    out
}

// Preserve observations in a horizontal Word's own font basis, and additionally
// consider its unique enclosing vertical column. This is a placement proof:
// no character, font name, unit vocabulary, or neighbouring style is consulted.
fn inline_placements(
    groups: &BTreeMap<Basis, Vec<Placed>>,
    mapping: &BTreeMap<usize, Vec<usize>>,
    native: &kanpo_pdf::Page,
) -> Vec<(Basis, Vec<Placed>)> {
    let by_id = groups
        .iter()
        .flat_map(|(&basis, placed)| placed.iter().map(move |p| (p.id, (basis, p))))
        .collect::<BTreeMap<_, _>>();
    let mut out = Vec::new();
    for (&word, ids) in mapping {
        let Some((&first_id, rest)) = ids.split_first() else {
            continue;
        };
        let Some(&(own_basis, first)) = by_id.get(&first_id) else {
            continue;
        };
        if native.glyphs[first_id].vertical
            || rest.iter().any(|id| {
                native.glyphs[*id].vertical
                    || by_id.get(id).is_none_or(|(basis, p)| {
                        *basis != own_basis || p.location != first.location || p.word != word
                    })
            })
        {
            continue;
        }
        let bounds = ids
            .iter()
            .map(|&id| native.glyphs[id].bbox)
            .reduce(|a, b| {
                [
                    a[0].min(b[0]),
                    a[1].min(b[1]),
                    a[2].max(b[2]),
                    a[3].max(b[3]),
                ]
            })
            .unwrap();
        let mut parents = Vec::new();
        for (&basis, placed) in groups {
            if basis.flow[0] * own_basis.flow[0] + basis.flow[1] * own_basis.flow[1] != 0 {
                continue;
            }
            let box_in_parent = basis.rect(bounds);
            let center = (box_in_parent[1] + box_in_parent[3]) / 2.;
            let height = box_in_parent[3] - box_in_parent[1];
            // A horizontal inline object spans the cross axis of a vertical
            // column. A long vertical Word must not acquire a horizontal row.
            if box_in_parent[2] - box_in_parent[0] <= height {
                continue;
            }
            let body = placed
                .iter()
                .filter(|p| {
                    native.glyphs[p.id].vertical
                        && p.location == first.location
                        && box_in_parent[0] <= p.baseline
                        && p.baseline <= box_in_parent[2]
                })
                .collect::<Vec<_>>();
            let mut baselines = Vec::new();
            for p in &body {
                if !baselines.contains(&p.baseline) {
                    baselines.push(p.baseline);
                }
            }
            for baseline in baselines {
                let spans = body
                    .iter()
                    .filter(|p| p.baseline == baseline)
                    .map(|p| {
                        let g = &native.glyphs[p.id];
                        (
                            basis.point([g.x, g.y])[1],
                            basis.point([g.end_x, g.end_y])[1],
                        )
                    })
                    .collect::<Vec<_>>();
                if spans.iter().any(|&(a, b)| a <= center && center < b) {
                    continue;
                }
                let before = spans
                    .iter()
                    .map(|&(_, b)| b)
                    .filter(|&b| b <= center)
                    .max_by(f64::total_cmp);
                let after = spans
                    .iter()
                    .map(|&(a, _)| a)
                    .filter(|&a| a > center)
                    .min_by(f64::total_cmp);
                if let (Some(before), Some(after)) = (before, after) {
                    if center - before <= height && after - center <= height {
                        parents.push((basis, baseline, box_in_parent, center));
                    }
                }
            }
        }
        if let [(basis, baseline, bounds, center)] = parents.as_slice() {
            out.push((
                *basis,
                ids.iter()
                    .map(|&id| Placed {
                        id,
                        word,
                        location: first.location,
                        baseline: *baseline,
                        center: *center,
                        bounds: *bounds,
                    })
                    .collect(),
            ));
        }
    }
    out
}

fn word_styles(
    page: &PageTrace,
    native: &kanpo_pdf::Page,
    source: &kanpo_pdf::Aj1PageSources,
    scene: &Scene,
    pi: usize,
    nontext: &BTreeSet<(usize, usize)>,
    auxiliary:&BTreeMap<usize,Location>,
    bindings: &mut Vec<GlyphBinding>,
    issues: &mut Vec<SourceIssue>,
    parallel_styles: &mut BTreeMap<usize, Option<Vec<Option<usize>>>>,
) -> BTreeMap<usize, Option<Vec<bool>>> {
    let mut locations = locations(page);
    for (&id,&location) in auxiliary {
        if !locations.contains_key(&id){locations.insert(id,vec![location]);}
    }
    let mut mapping = BTreeMap::new();
    let mut owner = BTreeMap::new();
    let mut duplicate_glyphs = BTreeSet::new();
    let mut text_glyphs = BTreeSet::new();
    for source in kanpo_pdf::bbox::words(native) {
        let bounds = [source.x_min, source.y_min, source.x_max, source.y_max]
            .map(|n| format!("{n:.6}").parse::<f32>().unwrap());
        let own = page
            .words
            .iter()
            .filter(|w| w.raw == source.text && w.source_box == bounds)
            .collect::<Vec<_>>();
        if own.iter().any(|word| word.status == WordStatus::Used || auxiliary.contains_key(&word.id)) {
            text_glyphs.extend(source.glyphs.iter().copied());
        }
        if own.len() != 1 || (own[0].status != WordStatus::Used && !auxiliary.contains_key(&own[0].id)) {
            continue;
        }
        let id = own[0].id;
        if locations.get(&id).is_none_or(|v| v.len() != 1) {
            continue;
        }
        for &glyph in &source.glyphs {
            if owner.insert(glyph, id).is_some() {
                duplicate_glyphs.insert(glyph);
            }
        }
        mapping.insert(id, source.glyphs);
    }
    let mut groups = BTreeMap::<Basis, Vec<Placed>>::new();
    let mut known = BTreeSet::new();
    for (&id, &word) in &owner {
        if nontext.contains(&(pi, id)) {
            known.insert(id);
            continue;
        }
        let Some(g) = native.glyphs.get(id) else {
            continue;
        };
        if duplicate_glyphs.contains(&id)
            || g.invisible
            || !positive(g.bbox)
            || ![g.x, g.y, g.end_x, g.end_y].iter().all(|n| n.is_finite())
        {
            continue;
        }
        let Some(basis) = source
            .glyph_text_axes(id)
            .and_then(|axes| Basis::glyph(g, axes))
        else {
            continue;
        };
        known.insert(id);
        let origin = basis.point([g.x, g.y]);
        let end = basis.point([g.end_x, g.end_y]);
        groups.entry(basis).or_default().push(Placed {
            id,
            word,
            location: locations[&word][0],
            baseline: origin[0],
            center: (origin[1] + end[1]) / 2.,
            bounds: basis.rect(g.bbox),
        });
    }
    let inline = inline_placements(&groups, &mapping, native);
    for (basis, placed) in inline {
        groups.entry(basis).or_default().extend(placed);
    }
    // A smaller run in a proved empty slot shares the surrounding row's
    // annotation space. Keep its real glyph placement too. Each bordering
    // source origin is retained; independently positioned text runs need not
    // have bit-identical baselines. No neighbouring style selects the row.
    for run in crate::source_baseline_shifts::for_page(page, source) {
        let Some(placed) = groups.get_mut(&run.basis) else { continue; };
        let own = run.glyphs.iter().map(|&id| {
            let found = placed.iter().filter(|p| p.id == id && p.word == run.word).collect::<Vec<_>>();
            (found.len() == 1).then(|| found[0].clone())
        }).collect::<Option<Vec<_>>>();
        let Some(own) = own else { continue; };
        for (baseline, bounds) in run.parent_baselines.into_iter().zip(run.parent_bounds) {
            placed.extend(own.iter().map(|p| Placed {
                baseline,
                bounds: [p.bounds[0].min(bounds[0]), p.bounds[1], p.bounds[2].max(bounds[2]), p.bounds[3]],
                ..p.clone()
            }));
        }
    }
    // The ordinary scene omits glyphs already grouped into bbox Words. A run
    // of CFF rulings can itself be such a Word, although no used text owns it.
    // Recover its original paint without changing legacy scene/Word metadata.
    let mut scene = scene.clone();
    for id in 0..native.glyphs.len() {
        if text_glyphs.contains(&id) || scene.rectangle_ids.contains(&id) {
            continue;
        }
        if let Some(rect) = source.glyph_painted_rectangle(id) {
            scene.rectangle_ids.push(id);
            scene.rectangles.push(rect);
        } else if let Some(rects) = source.glyph_separate_painted_rectangles(id) {
            // All contours were proved, with positive separation and complete
            // original paint context. Keep their identities separately.
            for &rect in rects {
                scene.rectangle_ids.push(id);
                scene.rectangles.push(rect);
            }
        }
    }
    let mut marked = BTreeMap::<usize, Vec<PaintHit>>::new();
    for (basis, placed) in &groups {
        let mut oriented = basis.scene(&scene);
        // Text in a different writing basis is still an obstacle. Grouping
        // subjects by their axes must not make a rotated overprint disappear.
        let subjects = placed.iter().map(|p| p.id).collect::<BTreeSet<_>>();
        oriented.fills.extend(
            native
                .glyphs
                .iter()
                .enumerate()
                .filter(|(id, g)| {
                    !subjects.contains(id)
                        && !scene.rectangle_ids.contains(id)
                        && !g.invisible
                        && positive(g.bbox)
                })
                .flat_map(|(id, g)| {
                    source
                        .glyph_rectangle_frame(id)
                        .map(|f| f.painted_rectangles().to_vec())
                        .unwrap_or_else(|| vec![g.bbox])
                })
                .map(|b| basis.rect(b)),
        );
        for (id, hits) in decoration_paints(placed, native, &oriented) {
            marked.entry(id).or_default().extend(hits.into_iter().map(
                |(index, full_primary_support)| PaintHit {
                    index,
                    basis: *basis,
                    full_primary_support,
                },
            ));
        }
    }
    let mut next_component = BTreeMap::<usize, usize>::new();
    let components = scene
        .rectangle_ids
        .iter()
        .zip(&scene.rectangles)
        .map(|(&glyph, &bounds)| {
            let component = next_component.entry(glyph).or_default();
            let result = DecorationComponent {
                glyph,
                component: *component,
                bounds,
            };
            *component += 1;
            result
        })
        .collect::<Vec<_>>();
    for (&id, &word) in &owner {
        if let Some(hits) = marked.get(&id) {
            let mut indices = hits.iter().map(|h| h.index).collect::<Vec<_>>();
            indices.sort_unstable();
            indices.dedup();
            let mut ids = indices
                .iter()
                .map(|&i| scene.rectangle_ids[i])
                .collect::<Vec<_>>();
            ids.sort_unstable();
            ids.dedup();
            bindings.push(GlyphBinding {
                page: pi,
                glyph: id,
                word,
                decoration_glyphs: ids,
                decoration_components: indices.into_iter().map(|i| components[i].clone()).collect(),
                parallel_strokes: parallel_strokes(hits, &scene),
            });
        }
    }
    mapping
        .into_iter()
        .map(|(word, ids)| {
            let active=ids.iter().copied().filter(|id|!nontext.contains(&(pi,*id))).collect::<Vec<_>>();
            let mut canonical=page.words[word].clone();
            let removal_proof=if active.len()==ids.len(){true}else{
                let raw=ids.iter().map(|&id|source.glyph_bbox_text(id)).collect::<Option<Vec<_>>>();
                if raw.as_ref().is_some_and(|raw|raw.concat()==canonical.raw) {
                    canonical.raw=ids.iter().zip(raw.unwrap()).filter(|(id,_)|!nontext.contains(&(pi,**id))).map(|(_,text)|text).collect();true
                }else{false}
            };
            let flags = active
                .iter()
                .map(|i| marked.contains_key(i))
                .collect::<Vec<_>>();
            let known = removal_proof&&scene.unknown.is_empty() && ids.iter().all(|id| known.contains(id));
            let result = known.then(||canonical_styles(&canonical,&flags)).flatten();
            let counts=active.iter().map(|id|marked.get(id).map_or(Some(0),|hits|parallel_strokes(hits,&scene))).collect::<Vec<_>>();
            parallel_styles.insert(word,known.then(||canonical_styles(&canonical,&counts)).flatten());
            if result.is_none() {
                issues.push(SourceIssue {
                    page: pi,
                    word: Some(word),
                    glyphs: ids,
                    reason: if !known {
                        "source glyphs have unknown/non-cardinal text transforms or incomplete scene"
                    } else {
                        "raw scalar normalization differs from canonical Word text"
                    }
                    .into(),
                });
            }
            (word, result)
        })
        .collect()
}

#[derive(Default)]
struct Stream {
    text: String,
    flags: Vec<bool>,
    complete: bool,
    words: Vec<(SourceWord, usize, usize)>,
    projections:Vec<crate::source_line_projection::Record>,
}
impl Stream {
    fn new() -> Self {
        Self {
            complete: true,
            ..Self::default()
        }
    }
    fn projected_part(&mut self,text:&str,ids:&[usize],page:&PageTrace,styles:&BTreeMap<usize,Option<Vec<bool>>>,pi:usize,
        projection:Option<crate::source_line_projection::Record>) {
        let text = compact(text);
        let offset = self.text.chars().count();
        self.text.push_str(&text);
        let direct=(|| {
        let mut seen = BTreeSet::new();
        let mut source = String::new();
        let mut flags: Vec<bool> = Vec::new();
        let mut words=Vec::new();
        for &id in ids {
            let Some(word) = page
                .words
                .get(id)
                .filter(|w| w.id == id && w.status == WordStatus::Used && seen.insert(id))
            else {
                return None;
            };
            source.push_str(&compact(&word.text));
            let Some(Some(style)) = styles.get(&id) else {
                return None;
            };
            words.push((
                SourceWord { page: pi, word: id },
                offset + flags.len(),
                style.len(),
            ));
            flags.extend(style.iter().copied());
        }
        if source != text || text.chars().count() != flags.len() {
            return None;
        }
        Some((flags,words))
        })();
        if let Some((flags,words))=direct {self.flags.extend(flags);self.words.extend(words);return;}
        let recovered=projection.and_then(|mut record|{
            if crate::source_line_projection::value(&record.tokens)!=text{return None;}
            let mut flags=Vec::new();let mut words:Vec<(SourceWord,usize,usize)>=Vec::new();
            for token in &record.tokens {
                match &token.origin {
                    crate::source_line_projection::Origin::Word{word,scalar} | crate::source_line_projection::Origin::RelocatedWord{word,scalar,..}=>{
                        let style=*styles.get(word)?.as_ref()?.get(*scalar)?;
                        if let Some(last)=words.last_mut().filter(|last|last.0.word==*word && last.1+last.2==offset+flags.len()) {last.2+=1;}
                        else {words.push((SourceWord{page:pi,word:*word},offset+flags.len(),1));}
                        flags.push(style);
                    },
                    crate::source_line_projection::Origin::ParallelJoin{..}=>flags.push(false),
                    _=>return None,
                }
            }
            if flags.len()!=text.chars().count(){return None;}
            record.offset=offset;
            Some((flags,words,record))
        });
        if let Some((flags,words,record))=recovered {
            self.flags.extend(flags);self.words.extend(words);self.projections.push(record);
        } else {self.complete=false;}
    }
}

fn annotations(
    field: Field,
    stream: &Stream,
    counts: &BTreeMap<(usize, usize), Option<Vec<Option<usize>>>>,
) -> Vec<ParallelUnderline> {
    let mut out = Vec::new();
    if !stream.complete || stream.flags.len() != stream.text.chars().count() {
        return out;
    }
    for &(word, offset, length) in &stream.words {
        let Some(Some(values)) = counts.get(&(word.page, word.word)) else {
            continue;
        };
        if values.len() != length
            || offset
                .checked_add(length)
                .is_none_or(|end| end > stream.flags.len())
        {
            continue;
        }
        let mut start = 0;
        while start < values.len() {
            let mut end = start + 1;
            while end < values.len() && values[end] == values[start] {
                end += 1;
            }
            if let Some(parallel_strokes) = values[start].filter(|&count| count >= 2) {
                out.push(ParallelUnderline {
                    field,
                    offset: offset + start,
                    length: end - start,
                    parallel_strokes,
                    source_word: word,
                });
            }
            start = end;
        }
    }
    out
}

fn table_text(table: &NestedTable, out: &mut String) {
    for row in &table.rows {
        for cell in row {
            runs_text(cell, out);
        }
    }
}
fn runs_text(runs: &[Run], out: &mut String) {
    for run in runs {
        out.push_str(&compact(&run.text));
        if let Some(table) = &run.table {
            table_text(table, out);
        }
    }
}
pub fn field_text(analysis: &Analysis, field: Field) -> String {
    let mut out = String::new();
    for article in &analysis.articles {
        for block in &article.document.blocks {
            match block {
                Block::Paragraph { runs } if field == Field::Paragraph => runs_text(runs, &mut out),
                Block::Shinkyu { rows, .. } if field != Field::Paragraph => {
                    for row in rows {
                        let (runs, table) = if field == Field::After {
                            (&row.after, &row.after_table)
                        } else {
                            (&row.before, &row.before_table)
                        };
                        runs_text(runs, &mut out);
                        if let Some(table) = table {
                            table_text(table, &mut out);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    out
}

fn style_table(table: &mut NestedTable, flags: &[bool], cursor: &mut usize) {
    for row in &mut table.rows {
        for cell in row {
            style_runs(cell, flags, cursor);
        }
    }
}
fn style_runs(runs: &mut Vec<Run>, flags: &[bool], cursor: &mut usize) {
    let mut out: Vec<Run> = Vec::new();
    for mut run in std::mem::take(runs) {
        let original_underline = run.underline;
        for c in run.text.chars() {
            let underline = if c.is_whitespace() {
                original_underline
            } else {
                let result = original_underline || flags[*cursor];
                *cursor += 1;
                result
            };
            if let Some(last) = out
                .last_mut()
                .filter(|r| r.table.is_none() && r.underline == underline)
            {
                last.text.push(c);
            } else {
                let mut next = Run::plain(c.to_string());
                next.underline = underline;
                out.push(next);
            }
        }
        if let Some(mut table) = run.table.take() {
            style_table(&mut table, flags, cursor);
            out.push(Run {
                text: String::new(),
                underline: original_underline,
                table: Some(table),
            });
        }
    }
    *runs = out;
}
fn apply(analysis: &mut Analysis, field: Field, flags: &[bool]) {
    let mut cursor = 0;
    for article in &mut analysis.articles {
        for block in &mut article.document.blocks {
            match block {
                Block::Paragraph { runs } if field == Field::Paragraph => {
                    style_runs(runs, flags, &mut cursor)
                }
                Block::Shinkyu { rows, .. } if field != Field::Paragraph => {
                    for row in rows {
                        let (runs, table) = if field == Field::After {
                            (&mut row.after, &mut row.after_table)
                        } else {
                            (&mut row.before, &mut row.before_table)
                        };
                        style_runs(runs, flags, &mut cursor);
                        if let Some(table) = table {
                            style_table(table, flags, &mut cursor);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    debug_assert_eq!(cursor, flags.len());
}

/// Transfer existing underline annotations only across a complete, unchanged
/// field stream. Recognition may add material to another field independently.
/// A mismatch returns false without modifying the target.
pub fn transfer_field(source: &Analysis, target: &mut Analysis, field: Field) -> bool {
    if field_text(source, field) != field_text(target, field) {
        return false;
    }
    fn table(table: &NestedTable, out: &mut Vec<bool>) {
        for row in &table.rows {
            for cell in row {
                runs(cell, out);
            }
        }
    }
    fn runs(items: &[Run], out: &mut Vec<bool>) {
        for item in items {
            out.extend(
                item.text
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .map(|_| item.underline),
            );
            if let Some(t) = &item.table {
                table(t, out);
            }
        }
    }
    let mut flags = Vec::new();
    for article in &source.articles {
        for block in &article.document.blocks {
            match block {
                Block::Paragraph { runs: r } if field == Field::Paragraph => runs(r, &mut flags),
                Block::Shinkyu { rows, .. } if field != Field::Paragraph => {
                    for row in rows {
                        let (r, t) = if field == Field::After {
                            (&row.after, &row.after_table)
                        } else {
                            (&row.before, &row.before_table)
                        };
                        runs(r, &mut flags);
                        if let Some(t) = t {
                            table(t, &mut flags);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    apply(target, field, &flags);
    true
}

/// Recover source CFF rectangle underlines explicitly, retaining native output.
/// `applied_fields` have complete matching source/document character streams;
/// other fields are retained unchanged with a reason. This does not certify
/// path-drawn underlines, arbitrary orientations, images or whitespace styling.
pub fn analyze_pdf(pdf: &[u8]) -> Result<UnderlinedAnalysis> {
    let sources = kanpo_pdf::extract_pages_with_aj1_sources(pdf)?;
    let inputs = crate::page_inputs_from_sources(&sources);
    let native = crate::analyze_item(&inputs);
    recover(&sources, &inputs, native, &BTreeSet::new())
}

pub(crate) fn analyze_observed(
    sources: &[kanpo_pdf::Aj1PageSources],
    native: Analysis,
    nontext: &BTreeSet<(usize, usize)>,
) -> Result<UnderlinedAnalysis> {
    let inputs = crate::page_inputs_from_sources(sources);
    recover(sources, &inputs, native, nontext)
}
fn recover(
    sources: &[kanpo_pdf::Aj1PageSources],
    inputs: &[crate::PageInput],
    native: Analysis,
    nontext: &BTreeSet<(usize, usize)>,
) -> Result<UnderlinedAnalysis> {
    ensure!(
        inputs.len() == sources.len() && sources.len() == native.pages.len(),
        "native page inventories differ"
    );
    let mut streams = [Stream::new(), Stream::new(), Stream::new()];
    let mut glyph_bindings = Vec::new();
    let mut source_issues = Vec::new();
    let mut counts = BTreeMap::new();
    for (pi, ((input, source), page)) in inputs.iter().zip(sources).zip(&native.pages).enumerate() {
        let html = scraper::Html::parse_document(&input.xhtml);
        let selector = scraper::Selector::parse("page").unwrap();
        let node = html.select(&selector).next();
        let scene: Option<Scene> = node
            .and_then(|n| n.value().attr("data-native-scene"))
            .and_then(|v| serde_json::from_str(v).ok());
        let valid = node
            .zip(input.rules.as_ref())
            .and_then(|(n, r)| crate::native_source_scene::for_page(n, r))
            .is_some_and(|s| s.matches(&page.words));
        let mut page_counts = BTreeMap::new();
        let needs_projection=page.regions.iter().flat_map(|r|&r.lines).any(|line|{
            if let Some(table)=&line.table {table.cells.iter().any(|c|{
                crate::source_line_projection::ordered_cell(page,c).is_none_or(|ids|ids.iter().filter_map(|&id|page.words.get(id)).map(|w|compact(&w.text)).collect::<String>()!=compact(&c.text))
            })}
            else {line.words.iter().filter_map(|&id|page.words.get(id)).map(|w|compact(&w.text)).collect::<String>()!=compact(&line.text)}
        });
        let projection=if needs_projection {node.map(|node|crate::source_line_projection::Page::new(node,source.page().width as f32,source.page().height as f32,input,source))}else{None};
        let auxiliary=projection.as_ref().map(|p|p.auxiliary_locations(page)).unwrap_or_default();
        let styles = if let Some(scene) = scene.filter(|s| {
            valid
                && s.glyphs == source.page().glyphs.len()
                && s.rectangle_ids.len() == s.rectangles.len()
        }) {
            word_styles(
                page,
                source.page(),
                source,
                &scene,
                pi,
                nontext,
                &auxiliary,
                &mut glyph_bindings,
                &mut source_issues,
                &mut page_counts,
            )
        } else {
            source_issues.push(SourceIssue {
                page: pi,
                word: None,
                glyphs: vec![],
                reason: "complete original source scene is unavailable".into(),
            });
            BTreeMap::new()
        };
        counts.extend(
            page_counts
                .into_iter()
                .map(|(word, values)| ((pi, word), values)),
        );
        for word in &page.words {
            if word.status == WordStatus::Used && !styles.contains_key(&word.id) {
                source_issues.push(SourceIssue {
                    page: pi,
                    word: Some(word.id),
                    glyphs: vec![],
                    reason:
                        "native Word occurrence or unique original line/cell ownership unavailable"
                            .into(),
                });
            }
        }
        for (ri,region) in page.regions.iter().enumerate() {
            let stream = &mut streams[Field::role(region.role).index()];
            for (li,line) in region.lines.iter().enumerate() {
                if let Some(table) = &line.table {
                    let mut cells = table.cells.iter().enumerate().collect::<Vec<_>>();
                    cells.sort_by_key(|(_,c)| (c.row, c.col));
                    if compact(&line.text)
                        != compact(&cells.iter().map(|(_,c)| c.text.as_str()).collect::<String>())
                    {
                        stream.complete = false;
                    }
                    for (ci,cell) in cells {
                        let ordered = crate::source_line_projection::ordered_cell(page,cell);
                        // Geometry may fail to render an irregular cell, but
                        // cannot silently discard or duplicate its source IDs.
                        if let Some(ordered)=ordered {
                            let record=projection.as_ref().and_then(|p|p.cell(cell,page)).map(|tokens|crate::source_line_projection::Record{
                                page:pi,region:ri,line:li,cell:Some(ci),field:Field::role(region.role),offset:0,text:compact(&cell.text),tokens,
                            });
                            stream.projected_part(&cell.text,&ordered,page,&styles,pi,record);
                        } else {stream.complete=false;}
                    }
                } else {
                    let record=projection.as_ref().and_then(|p|p.line(line,page)).map(|tokens|crate::source_line_projection::Record{
                        page:pi,region:ri,line:li,cell:None,field:Field::role(region.role),offset:0,text:compact(&line.text),tokens,
                    });
                    stream.projected_part(&line.text, &line.words, page, &styles, pi,record);
                }
            }
        }
    }
    let mut styled = native.clone();
    let mut applied_fields = Vec::new();
    let mut unresolved_fields = Vec::new();
    let mut parallel_underlines = Vec::new();
    let mut line_projections=Vec::new();
    for field in [Field::Paragraph, Field::After, Field::Before] {
        let source = &streams[field.index()];
        let text = field_text(&native, field);
        if !source.complete || source.flags.len() != source.text.chars().count() {
            unresolved_fields.push(UnresolvedField {
                field,
                reason: "incomplete original character/style ownership".into(),
            });
        } else if source.text != text {
            unresolved_fields.push(UnresolvedField {
                field,
                reason: "complete original and document field streams differ".into(),
            });
        } else {
            line_projections.extend(source.projections.iter().cloned());
            if source.flags.iter().any(|&f| f) {
            apply(&mut styled, field, &source.flags);
            applied_fields.push(field);
            parallel_underlines.extend(annotations(field, source, &counts));
            }
        }
    }
    Ok(UnderlinedAnalysis {
        scope: "cardinal_cff_rectangle_decorations",
        coverage_verified: false,
        native,
        styled,
        glyph_bindings,
        applied_fields,
        unresolved_fields,
        source_issues,
        parallel_underlines,
        line_projections,
        unresolved_parallel_words: counts
            .into_iter()
            .filter_map(|((page, word), values)| {
                values
                    .is_none_or(|values| values.iter().any(Option::is_none))
                    .then_some(SourceWord { page, word })
            })
            .collect(),
    })
}

#[cfg(test)]
#[path = "source_underlines_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "source_underlines_parallel_tests.rs"]
mod parallel_tests;
