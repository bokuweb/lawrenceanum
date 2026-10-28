//! Original glyph provenance for an interrupted physical column. The projection
//! is deliberately separate from the historical rightward NativeFlow contract.
use crate::native_word_axes::NativeAxis;
use crate::native_word_flow::{NativeFlow, NativeMode};
use crate::trace::{Line, NativeBounds, NativeStyles, Unit, Word};
#[derive(Debug, Clone, Copy)]
pub(crate) struct AxisFlow {
    axis: NativeAxis,
    flow: NativeFlow,
    first: usize,
    after: usize,
    total: usize,
}
fn project(axis: NativeAxis, b: [f32; 4]) -> Option<[f32; 4]> {
    use NativeAxis::*;
    Some(match axis {
        Right => b,
        Down => [b[1], -b[2], b[3], -b[0]],
        Left => [-b[2], -b[3], -b[0], -b[1]],
        Up => [-b[3], b[0], -b[1], b[2]],
        _ => return None,
    })
}
pub(crate) fn from_word(word: scraper::ElementRef<'_>, total: Option<usize>) -> Option<AxisFlow> {
    let axis = crate::native_word_axes::from_word(word);
    let NativeBounds::Glyphs(b) = crate::native_word_bounds::from_word(word) else {
        return None;
    };
    let b = project(axis, b)?;
    let a = word.value();
    let num = |key| a.attr(key)?.parse::<f64>().ok();
    let mode = match a.attr("data-wmode") {
        Some("0") => NativeMode::Horizontal,
        Some("1") => NativeMode::Vertical,
        _ => return None,
    };
    let flow = NativeFlow {
        mode: Some(mode),
        start: num("data-native-flow-start")?,
        baseline: num("data-native-flow-baseline")?,
        end: num("data-native-flow-end")?,
        leading: num("data-native-flow-leading")?,
        trailing: num("data-native-flow-trailing")?,
        prefix: num("data-native-flow-prefix")?,
    };
    let first = a.attr("data-native-flow-first-glyph")?.parse().ok()?;
    let after = a.attr("data-native-flow-after-glyph")?.parse().ok()?;
    let out = AxisFlow {
        axis,
        flow,
        first,
        after,
        total: total?,
    };
    out.belongs(b).then_some(out)
}
impl AxisFlow {
    pub(crate) fn source_range(self) -> (usize, usize) {
        (self.first, self.after)
    }
    pub(crate) fn vertical_steps(self, bounds: [f32; 4]) -> Option<[f64; 3]> {
        self.vertical_path(bounds)?;
        Some([self.flow.leading, self.flow.trailing, self.flow.prefix])
    }
    /// Bound every vertical glyph center from the complete source column and
    /// original em sizes. Mixed sizes are valid when their conservative center
    /// envelope stays in one cell; no average size or point tolerance is used.
    pub(crate) fn vertical_center_envelope(self,b:[f32;4],styles:&NativeStyles)->Option<[f64;3]>{
        let [x,start,end]=self.vertical_path(b)?;
        let sizes=styles.iter().map(|(_,bits)|f64::from_bits(*bits)).collect::<Vec<_>>();
        if sizes.is_empty()||sizes.iter().any(|s|!s.is_finite()||*s<=0.){return None;}
        let min=sizes.iter().copied().fold(f64::INFINITY,f64::min);
        let max=sizes.iter().copied().fold(f64::NEG_INFINITY,f64::max);
        // This is the native XHTML coordinate codec, not a geometric margin.
        let encoded=|v:f64|format!("{v:.6}").parse::<f32>().ok().filter(|v|v.is_finite());
        if encoded(x-max*0.5)!=Some(b[0])||encoded(x+max*0.5)!=Some(b[2]){return None;}
        let out=[x,start+min*0.5,end-min*0.5];
        (out.iter().all(|v|v.is_finite())&&out[1]<=out[2]).then_some(out)
    }

    pub(crate) fn vertical_prefix_gap(self,b:[f32;4],next:Self,nb:[f32;4])->bool{
        match(self.vertical_path(b),next.vertical_path(nb)){
            (Some(a),Some(z))=>a[0] as f32==z[0] as f32&&(a[2]+self.flow.leading)as f32<=z[1]as f32,
            _=>false,
        }
    }
    /// The actual vertical source path, with exact glyph-envelope binding.
    pub(crate) fn vertical_path(self,b:[f32;4])->Option<[f64;3]>{
        let f=self.flow;
        (self.axis==NativeAxis::Down && f.mode==Some(NativeMode::Vertical)
            && self.belongs(project(self.axis,b)?) && f.start as f32==b[1] && f.end as f32==b[3])
            .then_some([-f.baseline,f.start,f.end])
    }
    #[cfg(test)]
    pub(crate) fn range(self) -> (usize, usize) {
        (self.first, self.after)
    }
    fn belongs(self, b: [f32; 4]) -> bool {
        let f = self.flow;
        [
            f.start,
            f.baseline,
            f.end,
            f.leading,
            f.trailing,
            f.prefix,
            f.end + f.trailing,
            f.leading + f.prefix,
        ]
        .iter()
        .all(|v| v.is_finite())
            && f.start < f.end
            && f.leading > 0.
            && f.leading <= f.end - f.start
            && b.iter().all(|v| v.is_finite())
            && b[0] < b[2]
            && b[1] < b[3]
            && b[1] <= f.baseline as f32
            && f.baseline as f32 <= b[3]
            && (f.start as f32) < b[2]
            && b[0] < f.end as f32
            && self.first < self.after
            && self.after <= self.total
    }
}

fn unit_lines(u: &Unit) -> Vec<&Line> {
    match u {
        Unit::Plain(l) => l.iter().collect(),
        Unit::Shinkyu {
            preamble,
            after,
            before,
            outside,
        ..} => preamble
            .iter()
            .chain(after.iter().chain(before).chain(outside).flatten())
            .collect(),
    }
}
fn same_key(a: AxisFlow, b: AxisFlow, sa: &NativeStyles, sb: &NativeStyles) -> bool {
    a.axis == b.axis
        && a.flow.mode == b.flow.mode
        && a.flow.baseline == b.flow.baseline
        && a.flow.leading == b.flow.leading
        && a.flow.trailing == b.flow.trailing
        && a.flow.prefix == b.flow.prefix
        && sa == sb
}
fn gap(a: AxisFlow, b: AxisFlow) -> f64 {
    b.flow.start - a.flow.end
}
fn adjacent(a: AxisFlow, b: AxisFlow) -> bool {
    a.axis == b.axis
        && a.total == b.total
        && a.after == b.first
        && a.flow.start < b.flow.start
        && a.flow.end < b.flow.end
}
fn envelope(ids: &[usize], bounds: &[NativeBounds]) -> Option<[f32; 4]> {
    let mut out = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    for &id in ids {
        let NativeBounds::Glyphs(b) = bounds.get(id)? else {
            return None;
        };
        if !b.iter().all(|v| v.is_finite()) || b[0] >= b[2] || b[1] >= b[3] {
            return None;
        }
        out = [
            out[0].min(b[0]),
            out[1].min(b[1]),
            out[2].max(b[2]),
            out[3].max(b[3]),
        ];
    }
    Some(out)
}
fn overlap(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}
fn fenced(b: [f32; 4], rules: Option<&crate::lines::PageRules>) -> bool {
    let Some(r) = rules else { return false };
    r.horizontal
        .iter()
        .chain(&r.short_fraction_bars)
        .any(|h| b[1] < h.y && h.y < b[3] && h.x0 < b[2] && b[0] < h.x1)
        || r.vertical
            .iter()
            .any(|v| b[0] < v.x && v.x < b[2] && v.y0 < b[3] && b[1] < v.y1)
        || r.enclosures.iter().any(|r| {
            overlap(b, *r) && !(r[0] <= b[0] && r[1] <= b[1] && b[2] <= r[2] && b[3] <= r[3])
        })
}
pub(crate) fn restore(
    units: &mut Vec<Unit>,
    words: &[Word],
    bounds: &[NativeBounds],
    styles: &[Option<NativeStyles>],
    flows: &[Option<AxisFlow>],
    rules: Option<&crate::lines::PageRules>,
) {
    if words.len() != bounds.len() || words.len() != styles.len() || words.len() != flows.len() {
        return;
    }
    // Ranges identify original primitives, not character counts or output order.
    let mut source = flows.iter().flatten().collect::<Vec<_>>();
    source.sort_by_key(|f| (f.first, f.after));
    if source
        .windows(2)
        .any(|p| p[0].total != p[1].total || p[0].after > p[1].first)
    {
        return;
    }
    let mut owners = vec![0usize; words.len()];
    for line in units.iter().flat_map(unit_lines) {
        for &id in &line.words {
            let Some(n) = owners.get_mut(id) else { return };
            *n += 1;
        }
    }
    let valid = |l: &Line| -> bool {
        if l.table.is_some() || l.words.is_empty() {
            return false;
        }
        let mut text = String::new();
        let mut cross = (f32::NEG_INFINITY, f32::INFINITY);
        let mut axis = None;
        for &id in &l.words {
            let Some(w) = words.get(id) else { return false };
            let Some(f) = flows[id] else { return false };
            let NativeBounds::Glyphs(b) = bounds[id] else {
                return false;
            };
            let Some(b) = project(f.axis, b) else {
                return false;
            };
            let Some(st) = &styles[id] else { return false };
            if w.id != id
                || owners[id] != 1
                || w.raw.is_empty()
                || w.status != crate::trace::WordStatus::Used
                || st.is_empty()
                || st.iter().any(|&(_, bits)| {
                    !f64::from_bits(bits).is_finite() || f64::from_bits(bits) <= 0.
                })
                || !f.belongs(b)
                || axis.is_some_and(|a| a != f.axis)
            {
                return false;
            }
            axis = Some(f.axis);
            cross = (cross.0.max(b[1]), cross.1.min(b[3]));
            text.push_str(&w.text);
        }
        text == l.text
            && cross.0 < cross.1
            && l.words
                .windows(2)
                .all(|p| adjacent(flows[p[0]].unwrap(), flows[p[1]].unwrap()))
    };
    let locations = units
        .iter()
        .enumerate()
        .flat_map(|(u, unit)| match unit {
            Unit::Plain(l) => l
                .iter()
                .enumerate()
                .map(move |(i, _)| (u, i))
                .collect::<Vec<_>>(),
            _ => vec![],
        })
        .collect::<Vec<_>>();
    let get = |pos: (usize, usize)| match &units[pos.0] {
        Unit::Plain(l) => &l[pos.1],
        _ => unreachable!(),
    };
    let mut candidates = vec![];
    for &target in &locations {
        let a = get(target);
        if !valid(a) {
            continue;
        }
        let witnesses = a
            .words
            .windows(2)
            .filter(|p| flows[p[0]].unwrap().flow.mode != flows[p[1]].unwrap().flow.mode)
            .collect::<Vec<_>>();
        if witnesses.is_empty() {
            continue;
        }
        for &fragment in &locations {
            if fragment == target {
                continue;
            }
            let z = get(fragment);
            if !valid(z) {
                continue;
            }
            let lo = target.0.min(fragment.0);
            let hi = target.0.max(fragment.0);
            if units[lo..=hi].iter().any(|u| match u {
                Unit::Plain(l) => l.iter().any(|l| l.table.is_some()),
                _ => true,
            }) {
                continue;
            }
            let tail = *a.words.last().unwrap();
            let head = z.words[0];
            let fa = flows[tail].unwrap();
            let fz = flows[head].unwrap();
            if !adjacent(fa, fz) {
                continue;
            }
            if !witnesses.iter().any(|p| {
                same_key(
                    fa,
                    flows[p[0]].unwrap(),
                    styles[tail].as_ref().unwrap(),
                    styles[p[0]].as_ref().unwrap(),
                ) && same_key(
                    fz,
                    flows[p[1]].unwrap(),
                    styles[head].as_ref().unwrap(),
                    styles[p[1]].as_ref().unwrap(),
                ) && gap(fa, fz) == gap(flows[p[0]].unwrap(), flows[p[1]].unwrap())
            }) {
                continue;
            }
            if !z.words.iter().all(|&id| {
                a.words.iter().any(|&j| {
                    same_key(
                        flows[id].unwrap(),
                        flows[j].unwrap(),
                        styles[id].as_ref().unwrap(),
                        styles[j].as_ref().unwrap(),
                    )
                })
            }) {
                continue;
            }
            let ids = a.words.iter().chain(&z.words).copied().collect::<Vec<_>>();
            let combined = Line::text(format!("{}{}", a.text, z.text), ids.clone());
            if !valid(&combined) {
                continue;
            }
            let Some(b) = envelope(&ids, bounds) else {
                continue;
            };
            if fenced(b, rules)
                || words.iter().enumerate().any(|(id, w)| {
                    !ids.contains(&id)
                        && overlap(
                            b,
                            match bounds[id] {
                                NativeBounds::Glyphs(b) => b,
                                _ => w.source_box,
                            },
                        )
                })
            {
                continue;
            }
            candidates.push((target, fragment));
        }
    }
    // Conflicting ownership never chooses the first candidate.
    let unique = candidates
        .iter()
        .copied()
        .filter(|(a, b)| {
            candidates
                .iter()
                .filter(|(x, y)| x == a || y == a || x == b || y == b)
                .count()
                == 1
        })
        .collect::<Vec<_>>();
    let changes = unique
        .into_iter()
        .map(|(a, b)| (a, b, get(b).clone()))
        .collect::<Vec<_>>();
    let mut removed = std::collections::BTreeSet::new();
    for (a, b, line) in changes {
        if let Unit::Plain(l) = &mut units[a.0] {
            l[a.1].text.push_str(&line.text);
            l[a.1].words.extend(line.words);
        }
        removed.insert(b);
    }
    for (u, unit) in units.iter_mut().enumerate() {
        if let Unit::Plain(l) = unit {
            let mut i = 0;
            l.retain(|_| {
                let keep = !removed.contains(&(u, i));
                i += 1;
                keep
            });
        }
    }
}

#[cfg(test)]
#[path = "native_column_flow_tests.rs"]
mod tests;
