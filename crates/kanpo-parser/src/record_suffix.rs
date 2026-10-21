//! A typed, aligned last band of a continued record table may contain three
//! stacked fields even when wrapped addresses in the preceding band occupy many
//! physical columns. This validates only that last band; the caller retains all
//! existing tier height, half-page, alignment and total-height guards.
use crate::trace::Unit;
use crate::vertical::Col;

pub(crate) fn has_prior_record_header(units: &[Unit]) -> bool {
    units.iter().any(|u| {
        let Unit::Plain(ls) = u else {
            return false;
        };
        (0..ls.len()).any(|i| {
            let fields: Vec<&str> = ls[i..(i + 2).min(ls.len())]
                .iter()
                .flat_map(|l| l.text.split_whitespace())
                .collect();
            let has = |s: &str| fields.iter().any(|t| t.ends_with(s));
            has("年月日")
                && (has("名称") || has("氏名"))
                && [
                    "名称",
                    "氏名",
                    "所在地",
                    "住所",
                    "番号",
                    "種類",
                    "型式",
                    "摘要",
                    "関係",
                    "年月日",
                ]
                .iter()
                .filter(|s| has(s))
                .count()
                    >= 3
        })
    })
}
/// Numeric date components only: positional digits, or one usual kanji ten.
/// No era/calendar-year interpretation is available in a bare three-part date.
fn date_number(s: &str) -> Option<u32> {
    if s.is_empty() {
        return None;
    }
    let digit = |c: char| -> Option<u32> {
        match c {
            '0'..='9' => Some(c as u32 - '0' as u32),
            '０'..='９' => Some(c as u32 - '０' as u32),
            '〇' | '零' => Some(0),
            '一' => Some(1),
            '二' => Some(2),
            '三' => Some(3),
            '四' => Some(4),
            '五' => Some(5),
            '六' => Some(6),
            '七' => Some(7),
            '八' => Some(8),
            '九' => Some(9),
            _ => None,
        }
    };
    if let Some((before, after)) = s.split_once('十') {
        let one_kanji = |part: &str, omitted: u32| -> Option<u32> {
            if part.is_empty() {
                return Some(omitted);
            }
            let mut chars = part.chars();
            let c = chars.next()?;
            if chars.next().is_some() || !"一二三四五六七八九".contains(c) {
                return None;
            }
            digit(c)
        };
        return one_kanji(before, 1)?
            .checked_mul(10)?
            .checked_add(one_kanji(after, 0)?);
    }
    s.chars().try_fold(0u32, |value, c| {
        value.checked_mul(10)?.checked_add(digit(c)?)
    })
}
fn date(s: &str) -> bool {
    let mut numbers = s.split('、').map(date_number);
    let (Some(Some(year)), Some(Some(month)), Some(Some(day)), None) = (
        numbers.next(),
        numbers.next(),
        numbers.next(),
        numbers.next(),
    ) else {
        return false;
    };
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        // An era is not specified, so February29 cannot be ruled out by year.
        2 => 29,
        _ => return false,
    };
    year > 0 && (1..=max_day).contains(&day)
}
pub(crate) fn infer_stacked_suffix(
    bands: &[Vec<Col>],
    t: usize,
    hrules: &[(f32, f32, f32)],
) -> Option<Vec<f32>> {
    if t < 3 || t + 1 != bands.len() {
        return None;
    }
    // Every participating source rectangle must define finite positive area.
    // Non-finite comparisons cannot be used as fail-open alignment evidence.
    if !bands.iter().flatten().all(|c| {
        [c.x, c.x1, c.y, c.y1].iter().all(|v| v.is_finite())
            && (c.x1 - c.x).is_finite()
            && c.x < c.x1
            && (c.y1 - c.y).is_finite()
            && c.y < c.y1
    }) {
        return None;
    }
    let tail = &bands[t];
    let mut seen = std::collections::HashSet::new();
    if !bands.iter().flatten().all(|c| seen.insert(c.id)) {
        return None;
    }
    let center = |c: &Col| (c.x + c.x1) / 2.;
    if !bands.iter().flatten().all(|c| center(c).is_finite()) {
        return None;
    }
    let mut ws: Vec<_> = tail.iter().collect();
    ws.sort_by(|a, b| {
        a.x.total_cmp(&b.x)
            .then(a.x1.total_cmp(&b.x1))
            .then(a.y.total_cmp(&b.y))
            .then(a.id.cmp(&b.id))
    });
    // Connected components of CLOSED source intervals. A contact connects the
    // component but cannot by itself supply the required positive common core.
    let mut columns: Vec<Vec<&Col>> = Vec::new();
    let mut union_right = f32::NEG_INFINITY;
    for c in ws {
        if !columns.is_empty() && c.x <= union_right {
            columns.last_mut().unwrap().push(c);
            union_right = union_right.max(c.x1);
        } else {
            union_right = c.x1;
            columns.push(vec![c]);
        }
    }
    let mut columns: Vec<(f32, Vec<&Col>)> = columns
        .into_iter()
        .map(|cs| {
            let lo = cs.iter().map(|c| c.x).fold(f32::NEG_INFINITY, f32::max);
            let hi = cs.iter().map(|c| c.x1).fold(f32::INFINITY, f32::min);
            // A transitive overlap chain or a source spanning multiple columns has
            // no one shared positive axis interval and is not partitioned by guess.
            (lo < hi).then(|| (lo + (hi - lo) / 2., cs))
        })
        .collect::<Option<_>>()?;
    columns.sort_by(|a, b| b.0.total_cmp(&a.0));
    if columns.len() < 3 {
        return None;
    }
    let anchors: Vec<f32> = columns.iter().map(|(axis, _)| *axis).collect();
    let columns: Vec<Vec<&Col>> = columns.into_iter().map(|(_, cs)| cs).collect();
    let em = tail
        .iter()
        .map(|c| c.x1 - c.x)
        .fold(f32::INFINITY, f32::min)
        .max(1.);
    let pitch = anchors
        .windows(2)
        .map(|w| w[0] - w[1])
        .fold(f32::INFINITY, f32::min);
    if pitch < em * 2. {
        return None;
    }
    let mut starts = Vec::new();
    let mut field_heads = [Vec::new(), Vec::new(), Vec::new()];
    let mut marks = Vec::new();
    let mut dates = Vec::new();
    let mut office_bottom = f32::NEG_INFINITY;
    let mut mark_bottom = f32::NEG_INFINITY;
    for mut cs in columns {
        cs.sort_by(|a, b| a.y.total_cmp(&b.y));
        let ms: Vec<_> = cs
            .iter()
            .enumerate()
            .filter(|(_, c)| matches!(c.text.trim(), "〇" | "○"))
            .map(|(i, _)| i)
            .collect();
        if ms.len() != 1 {
            return None;
        }
        let m = ms[0];
        if m == 0 || m + 1 == cs.len() {
            return None;
        }
        let office: String = cs[..m].iter().map(|c| c.text.as_str()).collect();
        if office.trim().is_empty() || office.chars().any(|c| "。、〇○".contains(c)) {
            return None;
        }
        let dt: String = cs[m + 1..].iter().map(|c| c.text.trim()).collect();
        if !date(&dt) {
            return None;
        }
        let bottom = cs[..m]
            .iter()
            .map(|c| c.y1)
            .fold(f32::NEG_INFINITY, f32::max);
        if cs[m].y - bottom < em * 0.5 || cs[m + 1].y - cs[m].y1 < em * 0.5 {
            return None;
        }
        field_heads[0].push((cs[0].y, cs[0].y1));
        field_heads[1].push((cs[m].y, cs[m].y1));
        field_heads[2].push((cs[m + 1].y, cs[m + 1].y1));
        starts.push(cs[0].y);
        marks.push(cs[m].y);
        dates.push(cs[m + 1].y);
        office_bottom = office_bottom.max(bottom);
        mark_bottom = mark_bottom.max(cs[m].y1);
    }
    // The same field's original first fragments must share positive vertical
    // extent across all records, not merely pairwise overlap or close starts.
    for row in &field_heads {
        let lo = row.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max);
        let hi = row.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
        if lo >= hi {
            return None;
        }
    }
    // Both inferred cuts must fit real whitespace between the whole fields.
    if office_bottom >= marks.iter().copied().fold(f32::INFINITY, f32::min)
        || mark_bottom >= dates.iter().copied().fold(f32::INFINITY, f32::min)
    {
        return None;
    }
    let top = starts.iter().copied().fold(f32::INFINITY, f32::min);
    let before = bands[t - 1]
        .iter()
        .map(|c| c.y1)
        .fold(f32::NEG_INFINITY, f32::max);
    if top < before - 1. || top - before > em * 2. {
        return None;
    }
    if hrules.iter().any(|&(y, x0, x1)| {
        y > before - 1.
            && y < top + 1.
            && x0 <= anchors[anchors.len() - 1] + 3.
            && x1 >= anchors[0] - 3.
    }) {
        return None;
    }
    let mut supporting = 0;
    for b in &bands[..t] {
        if b.iter().any(|c| {
            c.text.contains('。')
                || center(c) > anchors[0] + 3.
                || center(c) < anchors[anchors.len() - 1] - pitch + 3.
        }) {
            return None;
        }
        if anchors
            .iter()
            .all(|&a| b.iter().any(|c| (center(c) - a).abs() < 3.))
        {
            supporting += 1;
        }
    }
    if supporting < 2 {
        return None;
    }
    // Preserve the existing midpoint arithmetic for ordinary cut bits, but
    // never return an overflowed value or a rounded endpoint as a gap cut.
    let mark_top = marks.iter().copied().fold(f32::INFINITY, f32::min);
    let date_top = dates.iter().copied().fold(f32::INFINITY, f32::min);
    let tops = vec![
        (office_bottom + mark_top) / 2.,
        (mark_bottom + date_top) / 2.,
    ];
    if !tops[0].is_finite()
        || !(office_bottom < tops[0] && tops[0] < mark_top)
        || !tops[1].is_finite()
        || !(mark_bottom < tops[1] && tops[1] < date_top)
    {
        return None;
    }
    Some(tops)
}

#[cfg(test)]
#[path = "record_suffix_tests.rs"]
mod tests;
