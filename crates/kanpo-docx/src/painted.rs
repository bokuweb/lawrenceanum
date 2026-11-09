//! Read characters that the original paints as vector shapes instead of text
//! (号外・特号の「（二十八）」のような縦組みの括弧付き番号), using this PDF's own
//! native glyphs as templates.
//!
//! This is the Rust form of `tools/kanpo-validation/read_painted_compounds.py`.
//! It finds paint between two native glyph advances, requires two opposed
//! brackets around a unique equal-pitch column of cells, and compares each
//! cell with every isolated CJK glyph the same document renders from its own
//! embedded font program (component count, then symmetric chamfer distance).
//! The result is a *candidate*: the Word writer inserts it highlighted with a
//! comment saying the reading is unverified. Cells without a clear winner,
//! competing whole-glyph readings and unsupported geometry produce nothing.
//! Each cell is compared from its own connected components, so a stroke that
//! crosses the pitch boundary by a rasterisation pixel does not leak into the
//! next cell.
use anyhow::Result;
use kanpo_pdf::{Glyph, Page, PathItem};
use std::collections::BTreeMap;

type Rect = [f64; 4];

/// Device pixels per point for both the paint and the templates.
const SCALE: f64 = 16.;
/// Supersampling per device pixel; a pixel is ink at half coverage.
const SUB: usize = 4;
/// Side of the normalised comparison bitmap.
const N: usize = 64;

fn positive(b: Rect) -> bool {
    b.iter().all(|v| v.is_finite()) && b[0] < b[2] && b[1] < b[3]
}
fn overlaps(a: Rect, b: Rect) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}
fn contains(a: Rect, b: Rect) -> bool {
    a[0] <= b[0] && b[2] <= a[2] && a[1] <= b[1] && b[3] <= a[3]
}

/// Bounds of a straight, bounded paint path (same rule as the parser's
/// `painted_text::paint_bounds`).
fn paint_bounds(path: &PathItem) -> Option<Rect> {
    if !path.fill && !path.stroke {
        return None;
    }
    if path
        .subpaths
        .iter()
        .any(|s| s.curved || path.stroke && s.points.len() > 2 && !s.rect)
    {
        return None;
    }
    let width = if path.stroke { path.line_width } else { 0. };
    if !width.is_finite() || width < 0. {
        return None;
    }
    let mut b = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for p in path.subpaths.iter().flat_map(|s| &s.points) {
        if !p.iter().all(|v| v.is_finite()) {
            return None;
        }
        for k in 0..2 {
            b[k] = b[k].min(p[k] - width / 2.);
            b[k + 2] = b[k + 2].max(p[k] + width / 2.);
        }
    }
    positive(b).then_some(b)
}

/// The writing axis (0 = x, 1 = y) and direction of a glyph's advance.
fn axis(g: &Glyph) -> Option<(usize, i8)> {
    let d = [g.end_x - g.x, g.end_y - g.y];
    if !d.iter().all(|v| v.is_finite()) || g.invisible || !positive(g.bbox) {
        return None;
    }
    if d[0] == 0. && d[1] != 0. {
        Some((1, if d[1] > 0. { 1 } else { -1 }))
    } else if d[1] == 0. && d[0] != 0. {
        Some((0, if d[0] > 0. { 1 } else { -1 }))
    } else {
        None
    }
}

/// Paint between two consecutive native glyph advances.
#[derive(Debug, Clone)]
pub(crate) struct Gap {
    pub before_glyph: usize,
    pub after_glyph: usize,
    pub gap: Rect,
    pub path_ids: Vec<usize>,
}

/// Same search as `audit_paint_gaps.candidates`.
fn candidates(page: &Page) -> Vec<Gap> {
    let glyphs = &page.glyphs;
    let mut groups: BTreeMap<(usize, u64, usize, i8), Vec<usize>> = BTreeMap::new();
    for (i, g) in glyphs.iter().enumerate() {
        if let Some((a, s)) = axis(g) {
            if g.size.is_finite() && g.size > 0. {
                groups
                    .entry((g.font, g.size.to_bits(), a, s))
                    .or_default()
                    .push(i);
            }
        }
    }
    let paint = page
        .paths
        .iter()
        .enumerate()
        .filter_map(|(i, p)| Some((i, paint_bounds(p)?)))
        .collect::<Vec<_>>();
    let mut out = Vec::new();
    for ((_, _, primary, sign), mut ids) in groups {
        let cross = 1 - primary;
        let sign = f64::from(sign);
        let at = |g: &Glyph| if primary == 0 { g.x } else { g.y };
        let end_of = |g: &Glyph| if primary == 0 { g.end_x } else { g.end_y };
        ids.sort_by(|&a, &b| (at(&glyphs[a]) * sign).total_cmp(&(at(&glyphs[b]) * sign)));
        let starts = ids
            .iter()
            .map(|&i| at(&glyphs[i]) * sign)
            .collect::<Vec<_>>();
        for &before in &ids {
            let a = &glyphs[before];
            let end = end_of(a) * sign;
            let first = starts.partition_point(|&s| s <= end);
            let mut after: Option<(f64, usize, f64, f64)> = None;
            for pos in first..ids.len() {
                let j = ids[pos];
                let b = &glyphs[j];
                let lo = a.bbox[cross].max(b.bbox[cross]);
                let hi = a.bbox[cross + 2].min(b.bbox[cross + 2]);
                if lo < hi {
                    if let Some(prev) = after {
                        if starts[pos] == prev.0 {
                            after = None;
                        }
                        break;
                    }
                    after = Some((starts[pos], j, lo, hi));
                }
            }
            let Some((start, successor, lo, hi)) = after else {
                continue;
            };
            let mut gap = [0.; 4];
            gap[cross] = lo;
            gap[cross + 2] = hi;
            let (p0, p1) = if end * sign <= start * sign {
                (end * sign, start * sign)
            } else {
                (start * sign, end * sign)
            };
            gap[primary] = p0;
            gap[primary + 2] = p1;
            if !positive(gap) {
                continue;
            }
            let path_ids = paint
                .iter()
                .filter(|(_, b)| contains(gap, *b))
                .map(|(i, _)| *i)
                .collect::<Vec<_>>();
            if path_ids.is_empty()
                || glyphs
                    .iter()
                    .any(|g| !g.invisible && positive(g.bbox) && overlaps(gap, g.bbox))
            {
                continue;
            }
            out.push(Gap {
                before_glyph: before,
                after_glyph: successor,
                gap,
                path_ids,
            });
        }
    }
    out.sort_by_key(|g| (g.before_glyph, g.after_glyph));
    out
}

/// A binary bitmap, row-major, `true` = ink.
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct Mask {
    pub w: usize,
    pub h: usize,
    pub px: Vec<bool>,
}

impl Mask {
    fn get(&self, x: usize, y: usize) -> bool {
        self.px[y * self.w + x]
    }
    fn crop(&self, [x0, y0, x1, y1]: [usize; 4]) -> Mask {
        let mut px = Vec::with_capacity((x1 - x0) * (y1 - y0));
        for y in y0..y1 {
            for x in x0..x1 {
                px.push(self.get(x, y));
            }
        }
        Mask {
            w: x1 - x0,
            h: y1 - y0,
            px,
        }
    }
}

/// Fill polygons (nonzero winding) inside `box_` at `SCALE`, supersampled.
fn rasterize(polygons: &[Vec<[f64; 2]>], box_: Rect) -> Mask {
    let w = ((box_[2] - box_[0]) * SCALE).round().max(1.) as usize;
    let h = ((box_[3] - box_[1]) * SCALE).round().max(1.) as usize;
    let (sw, sh) = (w * SUB, h * SUB);
    let k = SCALE * SUB as f64;
    let mut cover = vec![0u16; w * h];
    let edges = polygons
        .iter()
        .flat_map(|poly| {
            poly.iter()
                .zip(poly.iter().cycle().skip(1))
                .take(poly.len())
                .map(|(p, q)| {
                    (
                        [(p[0] - box_[0]) * k, (p[1] - box_[1]) * k],
                        [(q[0] - box_[0]) * k, (q[1] - box_[1]) * k],
                    )
                })
        })
        .filter(|(p, q)| p[1] != q[1])
        .collect::<Vec<_>>();
    let mut xs: Vec<(f64, i32)> = Vec::new();
    for sy in 0..sh {
        let y = sy as f64 + 0.5;
        xs.clear();
        for (p, q) in &edges {
            let (lo, hi, dir) = if p[1] < q[1] { (p, q, 1) } else { (q, p, -1) };
            if lo[1] <= y && y < hi[1] {
                let t = (y - lo[1]) / (hi[1] - lo[1]);
                xs.push((lo[0] + t * (hi[0] - lo[0]), dir));
            }
        }
        xs.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut wind = 0;
        for i in 0..xs.len() {
            wind += xs[i].1;
            if wind == 0 || i + 1 == xs.len() {
                continue;
            }
            let (a, b) = (xs[i].0, xs[i + 1].0);
            let first = (a - 0.5).ceil().max(0.) as usize;
            let last = ((b - 0.5).floor() as isize).min(sw as isize - 1);
            for sx in first as isize..=last {
                cover[(sy / SUB) * w + sx as usize / SUB] += 1;
            }
        }
    }
    let half = (SUB * SUB) as u16 / 2;
    Mask {
        w,
        h,
        px: cover.into_iter().map(|c| c >= half).collect(),
    }
}

/// Paint paths as fill polygons: filled subpaths as they are, stroked
/// segments as rectangles of the line width (at least one device pixel).
fn paint_polygons(paths: &[&PathItem]) -> Vec<Vec<[f64; 2]>> {
    let mut out = Vec::new();
    for path in paths {
        if path.fill {
            for s in &path.subpaths {
                if s.points.len() > 2 {
                    out.push(s.points.clone());
                }
            }
        }
        if path.stroke {
            let half = path.line_width / 2.;
            for s in &path.subpaths {
                let n = s.points.len();
                let segments = if s.closed { n } else { n.saturating_sub(1) };
                for i in 0..segments {
                    let (p, q) = (s.points[i], s.points[(i + 1) % n]);
                    let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
                    let len = dx.hypot(dy);
                    if len == 0. {
                        continue;
                    }
                    let (nx, ny) = (-dy / len * half, dx / len * half);
                    out.push(vec![
                        [p[0] + nx, p[1] + ny],
                        [q[0] + nx, q[1] + ny],
                        [q[0] - nx, q[1] - ny],
                        [p[0] - nx, p[1] - ny],
                    ]);
                }
            }
        }
    }
    out
}

/// Eight-connected components with half-open bounds `[x0, y0, x1, y1]`.
struct Component {
    bounds: [usize; 4],
    points: Vec<(usize, usize)>,
}

fn components(m: &Mask) -> Vec<Component> {
    let mut seen = vec![false; m.px.len()];
    let mut out = Vec::new();
    // Row-major scan finds the same first point as Python's min((y, x)).
    for start in 0..m.px.len() {
        if !m.px[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        let mut stack = vec![start];
        let mut points = Vec::new();
        while let Some(i) = stack.pop() {
            let (x, y) = (i % m.w, i / m.w);
            points.push((x, y));
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    if nx < 0 || ny < 0 || nx >= m.w as i32 || ny >= m.h as i32 {
                        continue;
                    }
                    let j = ny as usize * m.w + nx as usize;
                    if m.px[j] && !seen[j] {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        let x0 = points.iter().map(|p| p.0).min().unwrap();
        let y0 = points.iter().map(|p| p.1).min().unwrap();
        let x1 = points.iter().map(|p| p.0).max().unwrap() + 1;
        let y1 = points.iter().map(|p| p.1).max().unwrap() + 1;
        out.push(Component {
            bounds: [x0, y0, x1, y1],
            points,
        });
    }
    out
}

/// Tight crop resized to N×N by nearest sampling (PIL `NEAREST`).
fn normalize(m: &Mask) -> Option<Mask> {
    let ink =
        m.px.iter()
            .enumerate()
            .filter(|(_, v)| **v)
            .map(|(i, _)| (i % m.w, i / m.w))
            .collect::<Vec<_>>();
    let x0 = ink.iter().map(|p| p.0).min()?;
    let y0 = ink.iter().map(|p| p.1).min()?;
    let x1 = ink.iter().map(|p| p.0).max()? + 1;
    let y1 = ink.iter().map(|p| p.1).max()? + 1;
    let tight = m.crop([x0, y0, x1, y1]);
    let mut px = Vec::with_capacity(N * N);
    for y in 0..N {
        let sy = (((y as f64 + 0.5) * tight.h as f64 / N as f64) as usize).min(tight.h - 1);
        for x in 0..N {
            let sx = (((x as f64 + 0.5) * tight.w as f64 / N as f64) as usize).min(tight.w - 1);
            px.push(tight.get(sx, sy));
        }
    }
    Some(Mask { w: N, h: N, px })
}

/// Chebyshev distance of every pixel to the nearest ink.
fn distance(m: &Mask) -> Vec<u8> {
    let mut reached = m.px.clone();
    let mut out = vec![0u8; reached.len()];
    let mut step = 0u8;
    while reached.iter().any(|r| !r) {
        let mut next = reached.clone();
        for y in 0..m.h {
            for x in 0..m.w {
                if reached[y * m.w + x] {
                    continue;
                }
                let near = (y.saturating_sub(1)..(y + 2).min(m.h)).any(|ny| {
                    (x.saturating_sub(1)..(x + 2).min(m.w)).any(|nx| reached[ny * m.w + nx])
                });
                if near {
                    next[y * m.w + x] = true;
                }
            }
        }
        step = step.saturating_add(1);
        for i in 0..out.len() {
            if next[i] && !reached[i] {
                out[i] = step;
            }
        }
        reached = next;
    }
    out
}

struct Feature {
    mask: Mask,
    distance: Vec<u8>,
    components: usize,
}

fn feature(m: &Mask) -> Option<Feature> {
    let mask = normalize(m)?;
    Some(Feature {
        distance: distance(&mask),
        components: components(&mask).len(),
        mask,
    })
}

/// Templates ranked by symmetric mean chamfer distance (same component count).
fn rank(m: &Mask, templates: &BTreeMap<String, Feature>) -> Vec<(f64, String)> {
    let Some(own) = feature(m) else {
        return Vec::new();
    };
    let mean = |d: &[u8], mask: &Mask| {
        let (sum, n) = mask
            .px
            .iter()
            .zip(d)
            .filter(|(v, _)| **v)
            .fold((0u64, 0u64), |(s, n), (_, d)| (s + u64::from(*d), n + 1));
        sum as f64 / n.max(1) as f64
    };
    let mut scores = templates
        .iter()
        .filter(|(_, t)| t.components == own.components)
        .map(|(label, t)| {
            (
                (mean(&t.distance, &own.mask) + mean(&own.distance, &t.mask)) / 2.,
                label.clone(),
            )
        })
        .collect::<Vec<_>>();
    scores.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    scores
}

/// Two opposed brackets around a unique equal-pitch column of cells.
/// Returns the cell boxes and the whole inner box.
fn enclosed_cells(m: &Mask) -> Option<(Vec<Mask>, [usize; 4])> {
    let parts = components(m);
    if parts.len() < 4 {
        return None;
    }
    let li = (0..parts.len()).min_by_key(|&i| parts[i].bounds[0])?;
    let ri = (0..parts.len()).rev().max_by_key(|&i| parts[i].bounds[2])?;
    let (left, right) = (&parts[li], &parts[ri]);
    if li == ri || left.bounds[2] >= right.bounds[0] {
        return None;
    }
    let inner = parts
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != li && *i != ri)
        .map(|(_, c)| c)
        .collect::<Vec<_>>();
    let supported = inner.iter().all(|c| {
        let mid2 = c.bounds[1] + c.bounds[3];
        left.bounds[2] <= c.bounds[0]
            && c.bounds[0] < c.bounds[2]
            && c.bounds[2] <= right.bounds[0]
            && [left, right]
                .iter()
                .all(|a| 2 * a.bounds[1] < mid2 && mid2 < 2 * a.bounds[3])
    });
    if !supported {
        return None;
    }
    for (arc, sign) in [(left, -1.), (right, 1.)] {
        let (top, bottom) = (arc.bounds[1], arc.bounds[3]);
        let mut rows: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for &(x, y) in &arc.points {
            rows.entry(y).or_default().push(x);
        }
        if rows.len() != bottom - top {
            return None;
        }
        let mean = |y: usize| rows[&y].iter().sum::<usize>() as f64 / rows[&y].len() as f64;
        let middle = (top + bottom - 1) / 2;
        if ![top, bottom - 1]
            .iter()
            .all(|&y| sign * mean(middle) > sign * mean(y))
        {
            return None;
        }
    }
    let top = left.bounds[1].min(right.bounds[1]);
    let bottom = left.bounds[3].max(right.bounds[3]);
    let height = bottom - top;
    let tallest = inner.iter().map(|c| c.bounds[3] - c.bounds[1]).max()?;
    let mut partitions = Vec::new();
    for count in 2..=height / tallest {
        let pitch = height / count;
        let mut cells: Vec<Vec<usize>> = vec![Vec::new(); count];
        let fits = inner.iter().enumerate().all(|(k, part)| {
            let (a, z) = (part.bounds[1], part.bounds[3]);
            let slot = (a - top) / pitch;
            // One device pixel (1/16 pt) of rasterisation phase is allowed
            // at the cell edge; the partition must still be unique.
            if slot >= count || z - top > (slot + 1) * pitch + 1 {
                return false;
            }
            cells[slot].push(k);
            true
        });
        if fits && cells.iter().all(|c| !c.is_empty()) {
            partitions.push(cells);
        }
    }
    let [cells] = &partitions[..] else {
        return None;
    };
    // Each cell is drawn from its own components only, so a stroke that
    // crosses the pitch boundary by a pixel does not leak into the next cell.
    let cells = cells
        .iter()
        .map(|ids| {
            let mut px = vec![false; m.px.len()];
            for &k in ids {
                for &(x, y) in &inner[k].points {
                    px[y * m.w + x] = true;
                }
            }
            Mask { w: m.w, h: m.h, px }
        })
        .collect();
    Some((cells, [left.bounds[2], top, right.bounds[0], bottom]))
}

/// The bracketed reading of one paint mask, or `None` when it is not certain
/// enough to offer (ambiguous cell, competing whole glyph, other geometry).
fn recognize(m: &Mask, templates: &BTreeMap<String, Feature>) -> Option<String> {
    let (cells, inner) = enclosed_cells(m)?;
    let ranked = cells.iter().map(|c| rank(c, templates)).collect::<Vec<_>>();
    // Each cell needs a clear winner: no tie, and the runner-up at least a
    // third farther. A close second is left unread rather than guessed.
    if ranked
        .iter()
        .any(|s| s.is_empty() || s.len() > 1 && s[0].0 > 0.75 * s[1].0)
    {
        return None;
    }
    let mean = ranked.iter().map(|s| s[0].0).sum::<f64>() / ranked.len() as f64;
    if rank(&m.crop(inner), templates)
        .first()
        .is_some_and(|w| w.0 <= mean)
    {
        return None;
    }
    Some(format!(
        "（{}）",
        ranked.iter().map(|s| s[0].1.as_str()).collect::<String>()
    ))
}

fn trusted(page: &Page, g: &Glyph) -> bool {
    let font = &page.fonts[g.font];
    let mut chars = g.text.chars();
    let single = matches!((chars.next(), chars.next()), (Some(c), None) if ('\u{4e00}'..='\u{9fff}').contains(&c));
    !g.invisible
        && g.vertical
        && single
        && (font.ordering.as_deref() == Some("Japan1") || font.has_to_unicode)
        && positive(g.bbox)
}

/// A recognised reading at a paint gap.
#[derive(Debug, Clone)]
pub(crate) struct Reading {
    pub page: usize,
    pub gap: Gap,
    pub text: String,
}

/// Find and read painted compounds on pages without raster images.
pub(crate) fn readings(pdf: &[u8]) -> Result<Vec<Reading>> {
    // Most documents have no paint between glyph advances; outlines and the
    // image inventory are read only when a bracketed candidate exists.
    let pages = kanpo_pdf::extract_pages(pdf)?;
    let mut pending = Vec::new();
    for (pi, page) in pages.iter().enumerate() {
        for gap in candidates(page) {
            let (a, b) = (
                &page.glyphs[gap.before_glyph],
                &page.glyphs[gap.after_glyph],
            );
            // Native vertical writing that advances down the page only.
            if a.end_x != a.x || a.end_y <= a.y || b.end_x != b.x || b.end_y <= b.y {
                continue;
            }
            let paths = gap
                .path_ids
                .iter()
                .map(|&i| &page.paths[i])
                .collect::<Vec<_>>();
            let mask = rasterize(&paint_polygons(&paths), gap.gap);
            if enclosed_cells(&mask).is_some() {
                pending.push((pi, gap, mask));
            }
        }
    }
    if pending.is_empty() {
        return Ok(Vec::new());
    }
    // Paint on a page with raster images could be hidden or mixed with them.
    let sources = kanpo_pdf::extract_pages_with_xobject_sources(pdf)?;
    let plain = |i: usize| {
        sources
            .get(i)
            .is_some_and(|s| s.images().is_empty() && s.capture_gaps().is_empty())
    };
    pending.retain(|(pi, _, _)| plain(*pi));
    if pending.is_empty() {
        return Ok(Vec::new());
    }
    let mut templates = BTreeMap::new();
    for (pi, (page, outlines)) in kanpo_pdf::extract_pages_with_outlines(pdf)?
        .iter()
        .enumerate()
    {
        if !plain(pi) {
            continue;
        }
        for (g, outline) in page.glyphs.iter().zip(outlines) {
            if templates.contains_key(&g.text) || !trusted(page, g) {
                continue;
            }
            let Some(outline) = outline else { continue };
            if let Some(f) = feature(&rasterize(outline, g.bbox)) {
                templates.insert(g.text.clone(), f);
            }
        }
    }
    Ok(pending
        .into_iter()
        .filter_map(|(page, gap, mask)| {
            Some(Reading {
                page,
                text: recognize(&mask, &templates)?,
                gap,
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f64, y: f64, s: f64) -> Vec<[f64; 2]> {
        vec![[x, y], [x + s, y], [x + s, y + s], [x, y + s]]
    }

    #[test]
    fn rasterizing_fills_half_covered_pixels_with_nonzero_winding() {
        // A 1 pt square at 16 px/pt covers 16×16 pixels.
        let m = rasterize(&[square(0., 0., 1.)], [0., 0., 2., 1.]);
        assert_eq!((m.w, m.h), (32, 16));
        assert_eq!(m.px.iter().filter(|v| **v).count(), 256);
        // A hole drawn in the same direction stays filled (nonzero).
        let m = rasterize(
            &[square(0., 0., 1.), square(0.25, 0.25, 0.5)],
            [0., 0., 1., 1.],
        );
        assert!(m.px.iter().all(|v| *v));
    }

    #[test]
    fn chamfer_ranking_prefers_the_same_shape_and_component_count() {
        let bar = |rows: &[usize]| {
            let mut px = vec![false; 32 * 32];
            for &r in rows {
                for x in 4..28 {
                    for y in r..r + 3 {
                        px[y * 32 + x] = true;
                    }
                }
            }
            Mask { w: 32, h: 32, px }
        };
        let mut t = BTreeMap::new();
        t.insert("二".to_string(), feature(&bar(&[6, 24])).unwrap());
        t.insert("三".to_string(), feature(&bar(&[4, 14, 25])).unwrap());
        let r = rank(&bar(&[5, 23]), &t);
        assert_eq!(r.len(), 1, "component count filters 三: {r:?}");
        assert_eq!(r[0].1, "二");
    }

    #[test]
    fn workbook_no6_painted_numbers_are_read_from_its_own_glyphs() {
        // 財務省令第十七号（添付一覧 No.6）: eight bracketed numbers are paint.
        let pdf = include_bytes!("../../kanpo-parser/tests/data/workbook/case-06.pdf");
        let review: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../kanpo-parser/tests/data/workbook/painted-compound-review.json"
        ))
        .unwrap();
        let read = readings(pdf).unwrap();
        let got = read
            .iter()
            .map(|r| {
                (
                    r.page,
                    r.gap.before_glyph,
                    r.gap.after_glyph,
                    r.text.clone(),
                )
            })
            .collect::<Vec<_>>();
        let want = review["observations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| {
                (
                    o["page"].as_u64().unwrap() as usize,
                    o["before_glyph"].as_u64().unwrap() as usize,
                    o["after_glyph"].as_u64().unwrap() as usize,
                    o["text"].as_str().unwrap().to_string(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(got, want);
    }
}
