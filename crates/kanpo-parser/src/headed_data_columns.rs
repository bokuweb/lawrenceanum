//! Distinguish wrapped values inside a headed data column from a second key column.
use crate::vertical::Col;
use std::collections::HashSet;
fn center(w: &Col) -> f32 {
    (w.y + w.y1) * 0.5
}

pub(crate) fn embedded_key_ids(
    words: &[Col],
    keys: &[Col],
    vrules: &[(f32, f32, f32)],
) -> HashSet<usize> {
    let mut out = HashSet::new();
    if keys.len() < 3
        || words.iter().map(|w| w.id).collect::<HashSet<_>>().len() != words.len()
        || keys.iter().map(|w| w.id).collect::<HashSet<_>>().len() != keys.len()
        || keys.iter().any(|w| {
            ![w.x, w.x1, w.y, w.y1].iter().all(|x| x.is_finite()) || w.x1 <= w.x || w.y1 <= w.y
        })
    {
        return out;
    }
    let em = keys.iter().map(|k| k.y1 - k.y).fold(0.0, f32::max);
    if !em.is_finite()
        || !(3.0..=24.0).contains(&em)
        || keys.iter().any(|k| ((k.y1 - k.y) - em).abs() > em * 0.1)
        || keys.windows(2).any(|p| p[1].y - p[0].y < em * 1.5)
    {
        return out;
    }
    let first = keys[0].y;
    let last = keys.last().unwrap().y1;
    let key_right = keys.iter().map(|k| k.x1).fold(f32::NEG_INFINITY, f32::max);
    let Some(right) = vrules
        .iter()
        .filter(|&&(x, y0, y1)| x > key_right && y0 <= first && y1 >= last)
        .map(|&(x, _, _)| x)
        .reduce(f32::min)
    else {
        return out;
    };
    let headers: Vec<_> = words
        .iter()
        .filter(|w| {
            w.y1 <= first + 0.5
                && w.y >= first - em * 2.0
                && w.x >= keys[0].x - em
                && w.x1 <= right + 0.1
                && w.y1 - w.y >= em * 0.8
                && !w.text.chars().any(|c| {
                    c.is_ascii_digit() || ('０'..='９').contains(&c) || "。！？".contains(c)
                })
        })
        .collect();
    let mut hs: Vec<_> = headers.iter().map(|w| (w.x, w.x1)).collect();
    hs.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut end = f32::NEG_INFINITY;
    let mut hn = 0;
    for (x, x1) in hs {
        if x - end >= em {
            hn += 1;
        }
        end = end.max(x1);
    }
    if hn < 3
        || !headers
            .iter()
            .any(|h| h.x >= keys[0].x - em && h.x1 <= key_right + em)
    {
        return out;
    }
    let mut candidates: Vec<_> = words
        .iter()
        .filter(|w| {
            w.x > key_right + em
                && w.x1 < right
                && w.y >= first - em * 0.35
                && w.y1 <= last + em * 1.2
                && crate::horizontal::row_key(&w.text)
        })
        .collect();
    candidates.sort_by(|a, b| a.x.total_cmp(&b.x));
    let mut groups: Vec<Vec<&Col>> = Vec::new();
    for w in candidates {
        match groups.last_mut() {
            Some(g) if w.x - g[0].x <= em * 0.25 => g.push(w),
            _ => groups.push(vec![w]),
        }
    }
    for group in groups {
        if group.len() < 3 {
            continue;
        }
        let x = group[0].x;
        // Each original source row must begin a value at this same column.
        let mut values = Vec::new();
        let mut next_starts = Vec::new();
        let mut valid = true;
        for key in keys {
            let row: Vec<_> = words
                .iter()
                .filter(|w| {
                    (center(w) - center(key)).abs() <= em * 0.35
                        && w.x >= x - em * 0.25
                        && w.x1 < right
                })
                .collect();
            let own: Vec<_> = row
                .iter()
                .copied()
                .filter(|w| (w.x - x).abs() <= em * 0.25 && w.x1 - w.x >= em)
                .collect();
            if own.len() != 1 {
                valid = false;
                break;
            }
            let value = own[0];
            let next = row
                .iter()
                .filter(|w| w.x >= value.x1 + em * 0.2)
                .map(|w| w.x)
                .reduce(f32::min);
            let Some(next) = next else {
                valid = false;
                break;
            };
            values.push(value);
            next_starts.push(next);
        }
        if !valid {
            continue;
        }
        let col_right = next_starts.iter().copied().fold(f32::INFINITY, f32::min);
        if col_right - x < em * 2.0
            || col_right - x > em * 16.0
            || next_starts
                .iter()
                .any(|n| (*n - col_right).abs() > em * 0.5)
            || values.iter().any(|w| w.x1 > col_right - em * 0.2)
            || !headers.iter().any(|h| {
                h.x >= x - em * 0.5 && h.x1 <= col_right - em * 0.2 && h.text.chars().count() >= 2
            })
        {
            continue;
        }
        let mut wrapped = 0;
        let mut ids = HashSet::new();
        for w in &group {
            let Some(ri) = keys.iter().rposition(|k| k.y - em * 0.35 <= w.y) else {
                valid = false;
                break;
            };
            let value = values[ri];
            let bottom = keys.get(ri + 1).map_or(last + em * 1.2, |k| k.y);
            if w.x1 > col_right - em * 0.2 || w.y1 > bottom + em * 0.05 || !ids.insert(w.id) {
                valid = false;
                break;
            }
            if w.id == value.id {
                continue;
            }
            if w.y - value.y >= em * 0.65
                && w.y - value.y1 <= em * 0.4
                && w.y >= value.y1 - em * 0.1
                && value.x1 >= col_right - em * 2.0
            {
                wrapped += 1;
            } else {
                valid = false;
                break;
            }
        }
        // At least two keys, and a strict majority, are proved wrapped suffixes.
        // Ordinary rows of independent codes therefore keep their original fence.
        if valid && wrapped >= 2 && wrapped * 2 > group.len() {
            out.extend(ids);
        }
    }
    out
}
