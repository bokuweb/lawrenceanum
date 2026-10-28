//! Read-only geometry of supported embedded CFF1 outlines; no Unicode inference.
//! CFF #5176 and Type2 #5177. Unsupported geometry stays unknown.
use super::*;
use std::collections::BTreeMap;
/// Every side must be traversed once, allowing arbitrary collinear subdivision.
pub(crate) fn filled_rectangle(points: &[[f64; 2]]) -> Option<[f64; 4]> {
    if points.is_empty() || points.iter().flatten().any(|n| !n.is_finite()) {
        return None;
    }
    let mut b = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for &[x, y] in points {
        b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
    }
    if b[0] >= b[2] || b[1] >= b[3] {
        return None;
    }
    let mut sides: [Vec<(f64, f64)>; 4] = Default::default();
    for (p, q) in points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
    {
        if p == q {
            continue;
        }
        let (side, a, z) = if p[0] == q[0] {
            let side = if p[0] == b[0] {
                0
            } else if p[0] == b[2] {
                1
            } else {
                return None;
            };
            (side, p[1].min(q[1]), p[1].max(q[1]))
        } else if p[1] == q[1] {
            let side = if p[1] == b[1] {
                2
            } else if p[1] == b[3] {
                3
            } else {
                return None;
            };
            (side, p[0].min(q[0]), p[0].max(q[0]))
        } else {
            return None;
        };
        sides[side].push((a, z));
    }
    for (i, side) in sides.iter_mut().enumerate() {
        side.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        let (low, high) = if i < 2 { (b[1], b[3]) } else { (b[0], b[2]) };
        if side.first()?.0 != low
            || side.last()?.1 != high
            || side.windows(2).any(|p| p[0].1 != p[1].0)
        {
            return None;
        }
    }
    Some(b)
}
fn contours(data: &[u8]) -> Option<Vec<Vec<[f64; 2]>>> {
    // Standard Type2 implementation limits; not corpus-dependent retry caps.
    if data.is_empty() || data.len() > 65535 {
        return None;
    }
    let (mut i, mut args, mut points) = (0, Vec::<f64>::new(), Vec::<[f64; 2]>::new());
    let mut p = [0., 0.];
    let mut paths = Vec::new();
    while i < data.len() {
        let op = data[i];
        i += 1;
        let n = match op {
            32..=246 => Some(op as f64 - 139.),
            247..=250 => {
                let b = *data.get(i)?;
                i += 1;
                Some((op as f64 - 247.) * 256. + b as f64 + 108.)
            }
            251..=254 => {
                let b = *data.get(i)?;
                i += 1;
                Some(-(op as f64 - 251.) * 256. - b as f64 - 108.)
            }
            28 => {
                let n = i16::from_be_bytes(data.get(i..i + 2)?.try_into().ok()?);
                i += 2;
                Some(n as f64)
            }
            255 => {
                let n = i32::from_be_bytes(data.get(i..i + 4)?.try_into().ok()?);
                i += 4;
                Some(n as f64 / 65536.)
            }
            _ => None,
        };
        if let Some(n) = n {
            args.push(n);
            if args.len() > 48 {
                return None;
            }
            continue;
        }
        match op {
            4 | 21 | 22 => {
                let initial = paths.is_empty() && points.is_empty();
                if !points.is_empty() {
                    paths.push(std::mem::take(&mut points));
                }
                let count = if op == 21 { 2 } else { 1 };
                // The optional initial width is not a path operand.
                if initial && args.len() == count + 1 {
                    args.remove(0);
                }
                if args.len() != count {
                    return None;
                }
                p = match op {
                    4 => [p[0], p[1] + args[0]],
                    22 => [p[0] + args[0], p[1]],
                    _ => [p[0] + args[0], p[1] + args[1]],
                };
                points.push(p);
                args.clear();
            }
            5 | 6 | 7 => {
                if points.is_empty() || args.is_empty() || op == 5 && args.len() % 2 != 0 {
                    return None;
                }
                if op == 5 {
                    for q in args.chunks_exact(2) {
                        p = [p[0] + q[0], p[1] + q[1]];
                        points.push(p);
                    }
                } else {
                    let mut axis = usize::from(op == 7);
                    for &v in &args {
                        p[axis] += v;
                        points.push(p);
                        axis = 1 - axis;
                    }
                }
                args.clear();
            }
            14 => {
                if i != data.len() || !args.is_empty() {
                    return None;
                }
                paths.push(points);
                return Some(paths);
            }
            // Curves, subroutines, arithmetic and hints are unknown.
            _ => return None,
        }
        if p.iter().any(|n| !n.is_finite()) {
            return None;
        }
    }
    None
}
fn outline(data:&[u8])->Option<[f64;4]>{
 let paths=contours(data)?;
 let [points]=paths.as_slice() else{return None;};
 filled_rectangle(points)
}
fn compound_outline(data:&[u8])->Option<[f64;4]>{
 let paths=contours(data)?;
 if paths.len()<2{return None;}
 let mut envelope:Option<[f64;4]>=None;
 for path in paths{
  let b=filled_rectangle(&path)?;
  envelope=Some(match envelope{None=>b,Some(a)=>[a[0].min(b[0]),a[1].min(b[1]),a[2].max(b[2]),a[3].max(b[3])]});
 }
 envelope
}
fn default_geometry(d: &BTreeMap<u16, Vec<Option<i64>>>) -> bool {
    // Explicit matrices require a separate proof; never assume the default.
    !d.contains_key(&0x0c07)
        && !d.contains_key(&0x0c14)
        && !d.contains_key(&0x0c17)
        && [(0x0c05, 0), (0x0c06, 2), (0x0c21, 0)]
            .iter()
            .all(|(op, want)| d.get(op).is_none_or(|v| v.as_slice() == [Some(*want)]))
}
pub(crate) fn rectangles(data:&[u8])->Option<BTreeMap<u16,[f64;4]>>{map_outlines(data,outline)}
pub(crate) fn compound_envelopes(data:&[u8])->Option<BTreeMap<u16,[f64;4]>>{map_outlines(data,compound_outline)}
fn map_outlines(data: &[u8],parse:fn(&[u8])->Option<[f64;4]>) -> Option<BTreeMap<u16, [f64; 4]>> {
    if data.get(..2)? != [1, 0] {
        return None;
    }
    let mut p = *data.get(2)? as usize;
    if p < 4 || p > data.len() || !(1..=4).contains(data.get(3)?) {
        return None;
    }
    let names = index(data, &mut p)?;
    if names.len() != 1 || names[0].is_empty() {
        return None;
    }
    let tops = index(data, &mut p)?;
    if tops.len() != 1 {
        return None;
    }
    let strings = index(data, &mut p)?;
    let _global = index(data, &mut p)?;
    let (first, top) = read_dict(tops[0])?;
    if first != Some(0x0c1e) || !default_geometry(&top) {
        return None;
    }
    let [Some(reg), Some(order), Some(supp)] = top.get(&0x0c1e)?.as_slice() else {
        return None;
    };
    let sid =
        |n: i64| n >= 0 && (n < 391 || usize::try_from(n - 391).is_ok_and(|i| i < strings.len()));
    if !sid(*reg) || !sid(*order) || *supp < 0 {
        return None;
    }
    let offset = |op| {
        let [Some(n)] = top.get(&op)?.as_slice() else {
            return None;
        };
        usize::try_from(*n).ok()
    };
    let mut cs = offset(17)?;
    let programs = index(data, &mut cs)?;
    if programs.is_empty() || programs.len() > 65536 || programs.iter().any(|s| s.is_empty()) {
        return None;
    }
    cid_routing(data, &top, programs.len())?;
    let mut fd = offset(0x0c24)?;
    for font in index(data, &mut fd)? {
        if !default_geometry(&dict(font)?) {
            return None;
        }
    }
    let mut p = offset(15)?;
    if p <= 2 {
        return None;
    }
    let format = *data.get(p)?;
    p += 1;
    let mut cids = vec![0u16];
    while cids.len() < programs.len() {
        let first = u16::from_be_bytes(data.get(p..p + 2)?.try_into().ok()?);
        p += 2;
        let left = match format {
            0 => 0usize,
            1 => {
                let n = *data.get(p)? as usize;
                p += 1;
                n
            }
            2 => {
                let n = u16::from_be_bytes(data.get(p..p + 2)?.try_into().ok()?) as usize;
                p += 2;
                n
            }
            _ => return None,
        };
        if left.checked_add(1)? > programs.len() - cids.len() {
            return None;
        }
        for step in 0..=left {
            cids.push(first.checked_add(step.try_into().ok()?)?);
        }
    }
    if cids.iter().copied().collect::<BTreeSet<_>>().len() != cids.len() {
        return None;
    }
    Some(
        cids.into_iter()
            .zip(programs)
            .filter_map(|(cid, p)| parse(p).map(|b| (cid, b)))
            .collect(),
    )
}
#[cfg(test)]
#[path = "cff_rect_tests.rs"]
pub(crate) mod tests;


#[cfg(test)]#[path="cff_compound_tests.rs"]mod compound_tests;
