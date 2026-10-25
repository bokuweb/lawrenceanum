//! Recover paired fields from actual native caption cells and source rails.
//! No tier count, body wording/similarity or point-distance score is evidence.
use crate::{
    grid::Grid,
    lines::PageRules,
    native_word_axes::NativeAxis,
    shinkyu::ShinkyuLayout,
    trace::{ContentStyles, NativeBounds, Unit, Word},
    vertical::Col,
};
use std::collections::{BTreeMap, BTreeSet, HashMap};
type Rect = [f32; 4];
#[derive(Clone, Copy)]
pub(crate) struct Source<'a> {
    pub words: &'a [Word],
    pub bounds: &'a [NativeBounds],
    pub axes: &'a [NativeAxis],
    pub styles: &'a [ContentStyles],
    pub flows: &'a [Option<crate::native_column_flow::AxisFlow>],
}
pub(crate) struct Restored {
    pub merged: usize,
    pub certificates: Vec<Certificate>,
}
pub(crate) struct Certificate {
    all: BTreeSet<usize>,
    after: BTreeSet<usize>,
    before: BTreeSet<usize>,
    boxes: [Rect; 2],
    columns: BTreeMap<usize, (Rect, String)>,
}
impl Certificate {
    pub(crate) fn matches(&self, cols: &[Col]) -> bool {
        cols.len() == self.all.len()
            && cols.iter().map(|c| c.id).collect::<BTreeSet<_>>() == self.all
            && cols.iter().all(|c| {
                self.columns
                    .get(&c.id)
                    .is_some_and(|(b, text)| *b == [c.x, c.y, c.x1, c.y1] && *text == c.text)
            })
    }
    pub(crate) fn into_unit(
        &self,
        cols: Vec<Col>,
        grids: &[Grid],
        rules: Option<&PageRules>,
    ) -> Unit {
        let mut a = vec![];
        let mut b = vec![];
        for c in cols {
            if self.after.contains(&c.id) {
                a.push(c)
            } else if self.before.contains(&c.id) {
                b.push(c)
            }
        }
        let boxes = self.boxes.map(|r| (r[0], r[2], r[1], r[3]));
        ShinkyuLayout::native_fields(a, b, boxes).into_unit(grids, rules)
    }
}
fn valid(b: Rect) -> bool {
    b.iter().all(|v| v.is_finite()) && b[0] < b[2] && b[1] < b[3]
}
fn contains(a: Rect, b: Rect) -> bool {
    a[0] <= b[0] && a[1] <= b[1] && b[2] <= a[2] && b[3] <= a[3]
}
fn intersects(a: Rect, b: Rect) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}
#[derive(Clone, Copy, PartialEq)]
struct Rail {
    y: f32,
    left: f32,
    right: f32,
}
fn rails(r: &PageRules) -> Vec<Rail> {
    let mut hs = r
        .horizontal
        .iter()
        .filter(|h| h.y.is_finite() && h.x0.is_finite() && h.x1.is_finite() && h.x0 < h.x1)
        .map(|h| Rail {
            y: h.y,
            left: h.x0,
            right: h.x1,
        })
        .collect::<Vec<_>>();
    hs.sort_by(|a, b| {
        a.y.total_cmp(&b.y)
            .then(a.left.total_cmp(&b.left))
            .then(a.right.total_cmp(&b.right))
    });
    let mut out: Vec<Rail> = vec![];
    for h in hs {
        if let Some(last) = out.last_mut() {
            if last.y == h.y && h.left <= last.right {
                last.right = last.right.max(h.right);
                continue;
            }
        }
        out.push(h)
    }
    out
}
#[derive(Clone, Copy, PartialEq)]
struct Frame {
    left: f32,
    cap: f32,
    right: f32,
    top: f32,
    mid: f32,
    bottom: f32,
}
impl Frame {
    fn outer(self) -> Rect {
        [self.left, self.top, self.right, self.bottom]
    }
    fn bodies(self) -> [Rect; 2] {
        [
            [self.left, self.top, self.cap, self.mid],
            [self.left, self.mid, self.cap, self.bottom],
        ]
    }
    fn labels(self) -> [Rect; 2] {
        [
            [self.cap, self.top, self.right, self.mid],
            [self.cap, self.mid, self.right, self.bottom],
        ]
    }
}
fn frames(r: &PageRules) -> Vec<Frame> {
    let hs = rails(r);
    let mut out = vec![];
    for a in &r.vertical {
        for b in &r.vertical {
            if ![a.x, b.x, a.y0, a.y1, b.y0, b.y1]
                .iter()
                .all(|v| v.is_finite())
                || a.x >= b.x
                || a.y0 >= a.y1
                || a.y0 != b.y0
                || a.y1 != b.y1
            {
                continue;
            }
            if r.vertical
                .iter()
                .any(|v| a.x < v.x && v.x < b.x && v.y0 <= a.y0 && a.y1 <= v.y1)
            {
                continue;
            }
            for top in hs
                .iter()
                .filter(|h| h.y == a.y0 && h.left < a.x && h.right == b.x)
            {
                if !hs
                    .iter()
                    .any(|h| h.y == a.y1 && h.left == top.left && h.right == top.right)
                {
                    continue;
                }
                let mid = hs
                    .iter()
                    .filter(|h| {
                        a.y0 < h.y && h.y < a.y1 && h.left == top.left && h.right == top.right
                    })
                    .collect::<Vec<_>>();
                // Two caption cells have one shared native boundary.
                if mid.len() != 1 {
                    continue;
                }
                let f = Frame {
                    left: top.left,
                    cap: a.x,
                    right: b.x,
                    top: a.y0,
                    mid: mid[0].y,
                    bottom: a.y1,
                };
                if !out.contains(&f) {
                    out.push(f)
                }
            }
        }
    }
    out
}
// A glyph bearing can protrude past an open horizontal rail end. Only the
// original bound vertical path can prove its field; an actual left wall blocks it.
fn inside_body(source: Source<'_>, id: usize, b: Rect, r: Rect, rules: &PageRules) -> bool {
    if contains(r, b) {
        return true;
    }
    if !(b[0] < r[0] && r[1] <= b[1] && b[2] <= r[2] && b[3] <= r[3]) {
        return false;
    }
    if rules
        .vertical
        .iter()
        .any(|v| v.x == r[0] && v.y0 < b[3] && b[1] < v.y1)
    {
        return false;
    }
    let protrusion = [b[0], b[1], r[0], b[3]];
    if source.bounds.iter().enumerate().any(|(other, bb)| {
        other != id
            && matches!(bb,NativeBounds::Glyphs(bb) if valid(*bb)&&intersects(protrusion,*bb))
    }) {
        return false;
    }
    let Some(path) = source
        .flows
        .get(id)
        .copied()
        .flatten()
        .and_then(|f| f.vertical_path(b))
    else {
        return false;
    };
    r[0] as f64 <= path[0]
        && path[0] < (r[2] as f64)
        && r[1] as f64 <= path[1]
        && path[2] <= r[3] as f64
}
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn certify(
    source: Source<'_>,
    f: Frame,
    cols: &HashMap<usize, &Col>,
    rules: &PageRules,
) -> Option<Certificate> {
    let mut labels: [Vec<(usize, Rect)>; 2] = [vec![], vec![]];
    let mut fields: [BTreeSet<usize>; 2] = [BTreeSet::new(), BTreeSet::new()];
    let mut all = BTreeSet::new();
    for (id, w) in source.words.iter().enumerate() {
        let bbox = match source.bounds.get(id)? {
            NativeBounds::Glyphs(b) => *b,
            _ => {
                if intersects(f.outer(), w.source_box) {
                    return None;
                } else {
                    continue;
                }
            }
        };
        if !valid(bbox) {
            if intersects(f.outer(), w.source_box) {
                return None;
            } else {
                continue;
            }
        }
        if !intersects(f.outer(), bbox) {
            continue;
        }
        let c = *cols.get(&id)?;
        // Tier positions are legacy projections (vertical bbox normalization).
        // Roles and reconstruction use the original glyph occurrence instead.
        if w.status != crate::trace::WordStatus::Used || c.text != w.text {
            return None;
        }
        if matches!(
            source.axes[id],
            NativeAxis::Compatibility | NativeAxis::Unknown
        ) {
            return None;
        }
        let label = f.labels().iter().position(|r| contains(*r, bbox));
        if let Some(k) = label {
            if source.axes[id] != NativeAxis::Down
                || compact(&crate::normalize::word_text(&w.raw)) != compact(&w.text)
            {
                return None;
            }
            let ContentStyles::Glyphs(styles) = &source.styles[id] else {
                return None;
            };
            if styles.is_empty()
                || styles.iter().any(|(_, v)| {
                    let n = f64::from_bits(*v);
                    !n.is_finite() || n <= 0.
                })
            {
                return None;
            }
            labels[k].push((id, bbox));
        } else if let Some(k) = f
            .bodies()
            .iter()
            .position(|r| inside_body(source, id, bbox, *r, rules))
        {
            fields[k].insert(id);
        } else {
            return None;
        }
        all.insert(id);
    }
    for (k, expected) in ["改正後", "改正前"].iter().enumerate() {
        labels[k].sort_by(|a, b| a.1[1].total_cmp(&b.1[1]));
        let mut band: Option<(f32, f32)> = None;
        let mut bottom = f32::NEG_INFINITY;
        let mut text = String::new();
        for (id, b) in &labels[k] {
            if b[1] < bottom {
                return None;
            }
            bottom = b[3];
            let common = band.map_or((b[0], b[2]), |(l, r)| (l.max(b[0]), r.min(b[2])));
            if common.0 >= common.1 {
                return None;
            }
            band = Some(common);
            text.push_str(&compact(&source.words[*id].text));
        }
        if text != *expected || fields[k].is_empty() {
            return None;
        }
    }
    Some(Certificate {
        all: all.clone(),
        after: fields[0].clone(),
        before: fields[1].clone(),
        boxes: f.bodies(),
        columns: all
            .iter()
            .map(|&id| {
                let NativeBounds::Glyphs(b) = source.bounds[id] else {
                    unreachable!()
                };
                (id, (b, source.words[id].text.clone()))
            })
            .collect(),
    })
}
pub(crate) fn merge(
    tiers: &mut Vec<Vec<Col>>,
    source: Source<'_>,
    rules: Option<&PageRules>,
) -> Restored {
    let empty = || Restored {
        merged: 0,
        certificates: vec![],
    };
    let Some(rules) = rules else { return empty() };
    if source.words.len() != source.bounds.len()
        || source.words.len() != source.axes.len()
        || source.words.len() != source.styles.len()
        || source.words.iter().enumerate().any(|(i, w)| i != w.id)
    {
        return empty();
    }
    if source.flows.len() != source.words.len()
        || source.words.iter().any(|w| {
            w.status == crate::trace::WordStatus::Used
                && (!matches!(source.bounds[w.id],NativeBounds::Glyphs(b) if valid(b))
                    || matches!(
                        source.axes[w.id],
                        NativeAxis::Unknown | NativeAxis::Compatibility
                    ))
        })
    {
        return empty();
    }
    let mut cols = HashMap::new();
    for c in tiers.iter().flatten() {
        if c.id >= source.words.len() || cols.insert(c.id, c).is_some() {
            return empty();
        }
    }
    let mut proofs = vec![];
    for f in frames(rules) {
        let Some(cert) = certify(source, f, &cols, rules) else {
            continue;
        };
        let at = tiers
            .iter()
            .enumerate()
            .filter(|(_, t)| t.iter().any(|c| cert.all.contains(&c.id)))
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        let (Some(&start), Some(&end)) = (at.first(), at.last()) else {
            continue;
        };
        if tiers[start..=end]
            .iter()
            .flatten()
            .any(|c| !cert.all.contains(&c.id))
            || tiers[start..=end].iter().map(Vec::len).sum::<usize>() != cert.all.len()
        {
            continue;
        }
        proofs.push((start, end, cert));
    }
    let safe = (0..proofs.len())
        .filter(|&i| {
            !(0..proofs.len())
                .any(|j| i != j && proofs[i].0 <= proofs[j].1 && proofs[j].0 <= proofs[i].1)
        })
        .collect::<BTreeSet<_>>();
    let mut proofs = proofs
        .into_iter()
        .enumerate()
        .filter(|(i, _)| safe.contains(i))
        .map(|(_, p)| p)
        .collect::<Vec<_>>();
    proofs.sort_by_key(|p| p.0);
    // Rebuild only proven occurrences on the native geometry basis. Word
    // records remain untouched; later consumers must match this exact binding.
    for (_, _, certificate) in &proofs {
        for col in tiers
            .iter_mut()
            .flatten()
            .filter(|c| certificate.all.contains(&c.id))
        {
            let (b, _) = &certificate.columns[&col.id];
            col.x = b[0];
            col.y = b[1];
            col.x1 = b[2];
            col.y1 = b[3];
        }
    }
    let mut merged = 0;
    for (start, end, _) in proofs.iter().rev() {
        if start == end {
            continue;
        }
        let joined = tiers.drain(*start..=*end).flatten().collect();
        tiers.insert(*start, joined);
        merged += 1
    }
    Restored {
        merged,
        certificates: proofs.into_iter().map(|p| p.2).collect(),
    }
}
#[cfg(test)]
#[path = "native_caption_tier_tests.rs"]
mod tests;
