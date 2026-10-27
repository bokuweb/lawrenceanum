//! Read only CFF1 CID collection/charset evidence, never infer a glyph from a
//! font name. Unsupported or malformed structures have no source authority.
use std::collections::BTreeSet;
fn index<'a>(data: &'a [u8], pos: &mut usize) -> Option<Vec<&'a [u8]>> {
    let count = u16::from_be_bytes(data.get(*pos..pos.checked_add(2)?)?.try_into().ok()?) as usize;
    *pos += 2;
    if count == 0 {
        return Some(Vec::new());
    }
    let size = *data.get(*pos)? as usize;
    *pos += 1;
    if !(1..=4).contains(&size) {
        return None;
    }
    let mut offsets = Vec::with_capacity(count + 1);
    for _ in 0..=count {
        let mut v = 0usize;
        for &b in data.get(*pos..pos.checked_add(size)?)? {
            v = v.checked_mul(256)?.checked_add(b as usize)?;
        }
        *pos += size;
        offsets.push(v)
    }
    if offsets[0] != 1 || offsets.windows(2).any(|w| w[0] > w[1]) {
        return None;
    }
    let start = *pos;
    let end = start.checked_add(offsets[count].checked_sub(1)?)?;
    data.get(start..end)?;
    let out = offsets
        .windows(2)
        .map(|w| data.get(start + w[0] - 1..start + w[1] - 1))
        .collect::<Option<Vec<_>>>()?;
    *pos = end;
    Some(out)
}
fn dict(data: &[u8]) -> Option<std::collections::BTreeMap<u16, Vec<Option<i64>>>> {
    read_dict(data).map(|(_, values)| values)
}
fn read_dict(
    data: &[u8],
) -> Option<(
    Option<u16>,
    std::collections::BTreeMap<u16, Vec<Option<i64>>>,
)> {
    let (mut i, mut args, mut out) = (0usize, Vec::new(), std::collections::BTreeMap::new());
    let mut first_operator = None;
    while i < data.len() {
        let b = data[i];
        i += 1;
        let n = match b {
            32..=246 => Some(b as i64 - 139),
            247..=250 => {
                let x = *data.get(i)?;
                i += 1;
                Some((b as i64 - 247) * 256 + x as i64 + 108)
            }
            251..=254 => {
                let x = *data.get(i)?;
                i += 1;
                Some(-(b as i64 - 251) * 256 - x as i64 - 108)
            }
            28 => {
                let n = i16::from_be_bytes(data.get(i..i + 2)?.try_into().ok()?);
                i += 2;
                Some(n as i64)
            }
            29 => {
                let n = i32::from_be_bytes(data.get(i..i + 4)?.try_into().ok()?);
                i += 4;
                Some(n as i64)
            }
            30 => {
                let mut done = false;
                let mut value = String::new();
                while !done {
                    let n = *data.get(i)?;
                    i += 1;
                    for x in [n >> 4, n & 15] {
                        match x {
                            0..=9 => value.push(char::from(b'0' + x)),
                            10 => value.push('.'),
                            11 => value.push('E'),
                            12 => value.push_str("E-"),
                            14 => value.push('-'),
                            15 => {
                                done = true;
                                break;
                            }
                            _ => return None,
                        }
                    }
                }
                if !value.parse::<f64>().ok()?.is_finite() {
                    return None;
                }
                args.push(None);
                if args.len() > 48 { return None; }
                continue;
            }
            0..=21 => {
                let op = if b == 12 {
                    let n = *data.get(i)?;
                    i += 1;
                    0x0c00 + n as u16
                } else {
                    b as u16
                };
                first_operator.get_or_insert(op);
                if out.insert(op, std::mem::take(&mut args)).is_some() {
                    return None;
                }
                continue;
            }
            _ => return None,
        };
        args.push(n);
        if args.len() > 48 {
            return None;
        }
    }
    if !args.is_empty() {
        return None;
    }
    Some((first_operator, out))
}
// Validate the supported CFF1 CIDFont routing before using a charset as
// Unicode evidence. FDSelect is indexed by GID (including .notdef), not CID.
fn private_dict(data: &[u8], fd: &[u8]) -> Option<()> {
    let d = dict(fd)?;
    let [Some(size), Some(offset)] = d.get(&18)?.as_slice() else {
        return None;
    };
    let size = usize::try_from(*size).ok()?;
    let offset = usize::try_from(*offset).ok()?;
    let end = offset.checked_add(size)?;
    let p = dict(data.get(offset..end)?)?;
    // These are CFF1 Type2 Private DICT operators. An unsupported dictionary
    // has no source authority; in particular, arbitrary header/INDEX bytes
    // cannot masquerade as an in-bounds Private DICT.
    if p.keys()
        .any(|op| !matches!(*op, 6..=11 | 19..=21 | 0x0c09..=0x0c0e | 0x0c11..=0x0c13))
    {
        return None;
    }
    if let Some(subrs) = p.get(&19) {
        let [Some(relative)] = subrs.as_slice() else {
            return None;
        };
        let relative = usize::try_from(*relative).ok()?;
        if relative < size {
            return None;
        }
        let mut start = offset.checked_add(relative)?;
        index(data, &mut start)?;
    }
    Some(())
}
fn cid_routing(
    data: &[u8],
    top: &std::collections::BTreeMap<u16, Vec<Option<i64>>>,
    count: usize,
) -> Option<()> {
    let offset = |op| -> Option<usize> {
        let [Some(x)] = top.get(&op)?.as_slice() else {
            return None;
        };
        usize::try_from(*x).ok()
    };
    let mut fda = offset(0x0c24)?;
    let fonts = index(data, &mut fda)?;
    if fonts.is_empty() || fonts.len() > 256 {
        return None;
    }
    for fd in &fonts {
        private_dict(data, fd)?;
    }
    let mut s = offset(0x0c25)?;
    let format = *data.get(s)?;
    s += 1;
    match format {
        0 => {
            let end = s.checked_add(count)?;
            if data
                .get(s..end)?
                .iter()
                .any(|&fd| fd as usize >= fonts.len())
            {
                return None;
            }
        }
        3 => {
            let n = u16::from_be_bytes(data.get(s..s.checked_add(2)?)?.try_into().ok()?) as usize;
            s += 2;
            if n == 0 || n > count {
                return None;
            }
            let mut previous = None;
            for _ in 0..n {
                let end = s.checked_add(3)?;
                let r = data.get(s..end)?;
                s = end;
                let first = u16::from_be_bytes([r[0], r[1]]) as usize;
                if r[2] as usize >= fonts.len()
                    || first >= count
                    || previous.map_or(first != 0, |p| first <= p)
                {
                    return None;
                }
                previous = Some(first);
            }
            let sentinel =
                u16::from_be_bytes(data.get(s..s.checked_add(2)?)?.try_into().ok()?) as usize;
            if sentinel != count {
                return None;
            }
        }
        _ => return None,
    }
    Some(())
}
pub(crate) fn standard_cids(data: &[u8]) -> Option<Vec<u16>> {
    routed_cids(data, |reg, order, supplement| reg == b"Adobe" && order == b"Japan1" && (0..=7).contains(&supplement))
}
/// Structural CID routing only; this supplies no Unicode or collection meaning.
pub(crate) fn program_cids(data: &[u8]) -> Option<Vec<u16>> {
    routed_cids(data, |reg, order, supplement| !reg.is_empty() && !order.is_empty() && supplement >= 0)
}
fn routed_cids(data: &[u8], authorize: impl FnOnce(&[u8], &[u8], i64) -> bool) -> Option<Vec<u16>> {
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
    if first != Some(0x0c1e) {
        return None;
    }
    let ros = top.get(&0x0c1e)?;
    let [Some(reg), Some(order), Some(supp)] = ros.as_slice() else {
        return None;
    };
    let sid = |n: i64| -> Option<&[u8]> {
        strings
            .get(usize::try_from(n.checked_sub(391)?).ok()?)
            .copied()
    };
    if !authorize(sid(*reg)?, sid(*order)?, *supp) {
        return None;
    }
    let offset = |op| -> Option<usize> {
        let [Some(x)] = top.get(&op)?.as_slice() else {
            return None;
        };
        usize::try_from(*x).ok()
    };
    let mut cs = offset(17)?;
    let charstrings = index(data, &mut cs)?;
    let count = charstrings.len();
    if count == 0 || count > 65536 || charstrings.iter().any(|s| s.is_empty()) {
        return None;
    }
    cid_routing(data, &top, count)?;
    let mut c = offset(15)?;
    if c <= 2 {
        return None;
    }
    let format = *data.get(c)?;
    c += 1;
    let mut result = Vec::with_capacity(count);
    result.push(0u16);
    while result.len() < count {
        let first = u16::from_be_bytes(data.get(c..c + 2)?.try_into().ok()?);
        c += 2;
        let left = match format {
            0 => 0usize,
            1 => {
                let n = *data.get(c)? as usize;
                c += 1;
                n
            }
            2 => {
                let n = u16::from_be_bytes(data.get(c..c + 2)?.try_into().ok()?) as usize;
                c += 2;
                n
            }
            _ => return None,
        };
        if left.checked_add(1)? > count - result.len() {
            return None;
        }
        for step in 0..=left {
            result.push(first.checked_add(step.try_into().ok()?)?)
        }
    }
    if result.iter().copied().collect::<BTreeSet<_>>().len() != result.len() {
        return None;
    }
    result.sort_unstable();
    Some(result)
}

#[cfg(test)]
#[path = "cff_aj1_tests.rs"]
mod tests;

#[path="cff_rect.rs"]pub(crate) mod rectangles;
