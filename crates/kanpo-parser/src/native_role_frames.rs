//! Continue a printed comparison scope through its original ruled footprint.
//! Labels seed semantics; original material and stroke centers prove geometry.
use super::{printed_pair, RoleSource};
use crate::{
    lines::PageRules,
    native_word_axes::NativeAxis,
    trace::{Line, NativeBounds, PageTrace, Role, Unit, WordStatus},
    ComparisonLabels,
};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone)]
struct Material {
    bounds: [f64; 4],
    down: Option<f64>,
    right: Option<f64>,
}
impl Material {
    fn support(&self, axis: NativeAxis) -> [f64; 2] {
        let (cross, path) = if axis == NativeAxis::Down {
            (0, self.down)
        } else {
            (1, self.right)
        };
        path.map(|p| [p, p])
            .unwrap_or([self.bounds[cross], self.bounds[cross + 2]])
    }
    fn interval(&self, axis: NativeAxis) -> [f64; 2] {
        let k = usize::from(axis == NativeAxis::Down);
        [self.bounds[k], self.bounds[k + 2]]
    }
}
#[derive(Clone)]
struct Token {
    cuts: [f64; 3],
    extent: f64,
    labels: ComparisonLabels,
}
#[derive(Default)]
pub(crate) struct State {
    token: Option<Token>,
}
fn source_size(p: &PageTrace, node: scraper::ElementRef<'_>) -> Option<[f64; 2]> {
    let dims = [
        node.value().attr("width")?.parse::<f64>().ok()?,
        node.value().attr("height")?.parse::<f64>().ok()?,
    ];
    let display = [p.width, p.height];
    if !dims.iter().all(|v| v.is_finite() && *v > 0.)
        || !display.iter().all(|v| v.is_finite() && *v > 0.)
    {
        return None;
    }
    // The original bbox convention retains the paper dimensions on /Rotate
    // pages while glyph coordinates use device axes. Bind either orthogonal
    // basis to the current display; missing source dimensions never default.
    [dims, [dims[1], dims[0]]]
        .into_iter()
        .find(|v| v.map(|x| x as f32) == display)
}

fn ids(line: &Line) -> Vec<usize> {
    line.words
        .iter()
        .chain(
            line.table
                .iter()
                .flat_map(|t| t.cells.iter().flat_map(|c| &c.words)),
        )
        .copied()
        .collect()
}
fn characters(text: impl Iterator<Item = char>) -> BTreeMap<char, usize> {
    let mut out = BTreeMap::new();
    for c in text.filter(|c| !c.is_whitespace()) {
        *out.entry(c).or_default() += 1;
    }
    out
}
fn line_material(line: &Line, p: &PageTrace) -> bool {
    let check = |text: &str, ids: &[usize]| {
        let Some(parts) = ids
            .iter()
            .map(|id| p.words.get(*id))
            .collect::<Option<Vec<_>>>()
        else {
            return false;
        };
        characters(text.chars()) == characters(parts.iter().flat_map(|w| w.text.chars()))
    };
    if let Some(table) = &line.table {
        line.words.is_empty()
            && line.text == table.render()
            && table.cells.iter().all(|c| check(&c.text, &c.words))
    } else {
        check(&line.text, &line.words)
    }
}
fn materials(
    p: &PageTrace,
    node: scraper::ElementRef<'_>,
    wanted: &BTreeSet<usize>,
) -> Option<BTreeMap<usize, Material>> {
    let mut records = BTreeMap::new();
    for w in node.select(&scraper::Selector::parse("word").ok()?) {
        let a = w.value();
        let b = ["xmin", "ymin", "xmax", "ymax"]
            .map(|key| a.attr(key).and_then(|s| s.parse::<f32>().ok()));
        let [Some(x0), Some(y0), Some(x1), Some(y1)] = b else {
            return None;
        };
        records
            .entry((
                w.text().collect::<String>(),
                [x0, y0, x1, y1].map(f32::to_bits),
            ))
            .or_insert_with(Vec::new)
            .push(w);
    }
    let total = node.value().attr("data-native-glyph-count")?.parse().ok();
    let mut out = BTreeMap::new();
    for &id in wanted {
        let w = p.words.get(id)?;
        if w.id != id || w.status != WordStatus::Used {
            return None;
        }
        let [source] = records
            .get(&(w.raw.clone(), w.source_box.map(f32::to_bits)))?
            .as_slice()
        else {
            return None;
        };
        let NativeBounds::Glyphs(b) = *p.native_bounds.get(id)? else {
            return None;
        };
        let axis = *p.native_axes.get(id)?;
        if crate::native_word_bounds::from_word(*source) != NativeBounds::Glyphs(b)
            || crate::native_word_axes::from_word(*source) != axis
            || matches!(axis, NativeAxis::Unknown | NativeAxis::Compatibility)
        {
            return None;
        }
        let bounds = if let Some(s) = source.value().attr("data-native-source-occurrence") {
            let values = s
                .split_whitespace()
                .skip(3)
                .map(|s| s.parse().ok())
                .collect::<Option<Vec<f64>>>()?;
            <[f64; 4]>::try_from(values).ok()?
        } else {
            b.map(f64::from)
        };
        if !bounds.iter().all(|v| v.is_finite()) || bounds[0] >= bounds[2] || bounds[1] >= bounds[3]
        {
            return None;
        }
        let down = crate::native_column_flow::from_word(*source, total)
            .and_then(|f| f.vertical_path(b))
            .map(|p| p[0]);
        let right = crate::native_word_flow::from_word(*source)
            .filter(|f| f.mode == Some(crate::native_word_flow::NativeMode::Horizontal))
            .map(|f| f.baseline);
        out.insert(
            id,
            Material {
                bounds,
                down,
                right,
            },
        );
    }
    Some(out)
}
fn span<'a>(rows: impl Iterator<Item = &'a Material>, axis: NativeAxis) -> Option<[f64; 2]> {
    rows.map(|m| m.interval(axis))
        .reduce(|a, b| [a[0].min(b[0]), a[1].max(b[1])])
}
fn side(m: &Material, axis: NativeAxis, q: [f64; 3]) -> Option<usize> {
    let [lo, hi] = m.interval(axis);
    if q[0] <= lo && hi <= q[1] {
        Some(0)
    } else if q[1] <= lo && hi <= q[2] {
        Some(1)
    } else {
        None
    }
}
pub(crate) struct Observation {
    pub(crate) printed: bool,
    pub(crate) units: Option<Vec<Unit>>,
}
fn aligned_cuts(token: &Token, fences: &[f64], size: f64) -> Option<[f64; 3]> {
    let mut q = [0.; 3];
    for (i, root) in token.cuts.iter().enumerate() {
        let target = root * size;
        if !target.is_finite() {
            return None;
        }
        let matches = fences
            .iter()
            .copied()
            .filter(|v| {
                let value = v * token.extent;
                value.is_finite() && value == target
            })
            .collect::<Vec<_>>();
        let [only] = matches.as_slice() else {
            return None;
        };
        q[i] = *only;
    }
    Some(q)
}
impl State {
    pub(crate) fn clear(&mut self) {
        self.token = None;
    }
    pub(crate) fn observe(
        &mut self,
        p: &PageTrace,
        node: scraper::ElementRef<'_>,
        rules: &PageRules,
        has_next_page: bool,
    ) -> Observation {
        let printed = printed_pair(RoleSource {
            regions: &p.regions,
            words: &p.words,
            bounds: &p.native_bounds,
            axes: &p.native_axes,
        });
        let prefix = printed.and_then(|_| self.token.as_ref())
            .and_then(|token| prefix::restore(p, node, rules, token));
        // A printed root cannot relabel its own already classified page.
        // At input end its geometric certificate has no future consumer.
        // An active earlier scope still proves an unlabelled final page.
        if printed.is_some() && !has_next_page {
            self.clear();
            return Observation {
                printed: true,
                units: prefix,
            };
        }
        Observation {
            printed: printed.is_some(),
            units: self.prove(p, node, rules, printed).map(|unit| vec![unit]).or(prefix),
        }
    }
    #[cfg(test)]
    pub(crate) fn step(
        &mut self,
        p: &PageTrace,
        node: scraper::ElementRef<'_>,
        rules: &PageRules,
    ) -> Option<Unit> {
        self.observe(p, node, rules, true).units
            .and_then(|mut units| (units.len() == 1).then(|| units.remove(0)))
    }
    /// Failure consumes the open scope. Inferred roles cannot create a root.
    fn prove(
        &mut self,
        p: &PageTrace,
        node: scraper::ElementRef<'_>,
        rules: &PageRules,
        printed: Option<ComparisonLabels>,
    ) -> Option<Unit> {
        let previous = self.token.take();
        if printed.is_none() && previous.is_none() {
            return None;
        }
        let scene =
            crate::native_source_scene::for_page(node, rules).filter(|s| s.matches(&p.words))?;
        let dimensions = source_size(p, node)?;
        let unit = p.regions.last()?.unit;
        let selected = p
            .regions
            .iter()
            .filter(|r| {
                printed.is_none() || r.unit == unit && matches!(r.role, Role::After | Role::Before)
            })
            .collect::<Vec<_>>();
        if selected.is_empty()
            || selected.iter().any(|r| {
                r.unit != unit || !matches!(r.role, Role::Body | Role::After | Role::Before)
            })
        {
            return None;
        }
        if printed.is_none()
            && selected
                .iter()
                .flat_map(|r| &r.lines)
                .any(|l| crate::segment::is_heading_mark(l.text.trim()))
        {
            return None;
        }
        let mut wanted = BTreeSet::new();
        let mut rows = Vec::new();
        for r in &selected {
            for l in &r.lines {
                let ids = ids(l);
                if !line_material(l, p) {
                    return None;
                }
                if ids.is_empty() || ids.iter().any(|id| !wanted.insert(*id)) {
                    return None;
                }
                rows.push((r.role, l, ids));
            }
        }
        if printed.is_none()
            && p.words
                .iter()
                .any(|w| w.status == WordStatus::Used && !wanted.contains(&w.id))
        {
            return None;
        }
        let material = materials(p, node, &wanted)?;
        let mut candidates = Vec::new();
        for axis in [NativeAxis::Down, NativeAxis::Right] {
            let size = dimensions[usize::from(axis == NativeAxis::Down)];
            let supports = material
                .values()
                .map(|m| m.support(axis))
                .collect::<Vec<_>>();
            let fences = scene.parallel_fences(axis, &supports);
            let cuts = if let Some(labels) = printed {
                let field = |role| {
                    rows.iter()
                        .filter(move |r| r.0 == role)
                        .flat_map(|r| &r.2)
                        .map(|id| &material[id])
                };
                let (Some(a), Some(b)) = (
                    span(field(Role::After), axis),
                    span(field(Role::Before), axis),
                ) else {
                    continue;
                };
                if a[1] > b[0] {
                    continue;
                }
                let (Some(lo), Some(hi)) = (
                    fences
                        .iter()
                        .copied()
                        .filter(|q| *q <= a[0])
                        .max_by(f64::total_cmp),
                    fences
                        .iter()
                        .copied()
                        .filter(|q| b[1] <= *q)
                        .min_by(f64::total_cmp),
                ) else {
                    continue;
                };
                let middle = fences
                    .iter()
                    .copied()
                    .filter(|q| a[1] <= *q && *q <= b[0])
                    .collect::<Vec<_>>();
                let [mid] = middle.as_slice() else { continue };
                (
                    Token {
                        cuts: [lo, *mid, hi],
                        extent: size,
                        labels,
                    },
                    [lo, *mid, hi],
                )
            } else {
                let token = previous.as_ref()?;
                let Some(q) = aligned_cuts(token, &fences, size) else {
                    continue;
                };
                (token.clone(), q)
            };
            let (token, q) = cuts;
            if !(q[0] < q[1] && q[1] < q[2]) || scene.opaque_crosses_fence(axis, q[1], &supports) {
                continue;
            }
            let mut fields: [Vec<Line>; 2] = Default::default();
            let mut consistent = true;
            for (role, line, ids) in &rows {
                let Some(first) = side(&material[&ids[0]], axis, q) else {
                    consistent = false;
                    break;
                };
                if ids
                    .iter()
                    .any(|id| side(&material[id], axis, q) != Some(first))
                    || (*role != Role::Body && *role != [Role::After, Role::Before][first])
                {
                    consistent = false;
                    break;
                }
                fields[first].push((*line).clone());
            }
            if consistent && fields.iter().all(|f| !f.is_empty()) {
                candidates.push((token, fields));
            }
        }
        let [(token, fields)] = candidates.as_slice() else {
            return None;
        };
        self.token = Some(token.clone());
        if printed.is_some() || selected.iter().all(|r| r.role != Role::Body) {
            return None;
        }
        if selected.iter().any(|r| r.role != Role::Body) {
            self.token = None;
            return None;
        }
        Some(Unit::Shinkyu {
            labels: Some(token.labels),
            preamble: vec![],
            after: vec![fields[0].clone()],
            before: vec![fields[1].clone()],
            outside: vec![],
        })
    }
}

#[cfg(test)]
mod geometry_tests {
    use super::*;
    #[test]
    fn role_material_has_no_tolerance_across_the_original_divider() {
        for axis in [NativeAxis::Down, NativeAxis::Right] {
            let mut m = Material {
                bounds: [3., 1., 7., 5.],
                down: Some(4.),
                right: Some(4.),
            };
            if axis == NativeAxis::Down {
                m.bounds = [1., 3., 5., 7.];
            }
            assert_eq!(side(&m, axis, [2., 7., 13.]), Some(0));
            let primary = usize::from(axis == NativeAxis::Down);
            m.bounds[primary + 2] = f64::from_bits(7f64.to_bits() + 1);
            assert_eq!(side(&m, axis, [2., 7., 13.]), None);
            m.bounds[primary] = 7.;
            m.bounds[primary + 2] = 9.;
            assert_eq!(side(&m, axis, [2., 7., 13.]), Some(1));
            m.bounds[primary + 2] = f64::from_bits(13f64.to_bits() + 1);
            assert_eq!(side(&m, axis, [2., 7., 13.]), None);
        }
    }
    #[test]
    fn frame_identity_is_exact_under_page_scaling_and_rejects_overflow_or_missing_cuts() {
        let token = Token {
            cuts: [2., 7., 13.],
            extent: 16.,
            labels: ComparisonLabels::AMENDMENT,
        };
        for scale in [0.125, 0.5, 1., 4., 64.] {
            let q = token.cuts.map(|x| x * scale);
            assert_eq!(aligned_cuts(&token, &q, token.extent * scale), Some(q));
            let mut changed = q;
            changed[1] = f64::from_bits(changed[1].to_bits() + 1);
            assert_eq!(aligned_cuts(&token, &changed, token.extent * scale), None);
        }
        assert_eq!(aligned_cuts(&token, &[2., 13.], 16.), None);
        let huge = Token {
            cuts: [f64::MAX / 4., f64::MAX / 2., f64::MAX],
            extent: 16.,
            labels: token.labels,
        };
        assert_eq!(aligned_cuts(&huge, &huge.cuts, 16.), None);
    }
}

#[path="native_role_prefix.rs"]
mod prefix;
