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
fn date(s: &str) -> bool {
    let parts: Vec<_> = s.split('、').collect();
    parts.len() == 3
        && parts.iter().all(|p| {
            (1..=3).contains(&p.chars().count())
                && p.chars().all(|c| {
                    "〇零一二三四五六七八九十".contains(c)
                        || c.is_ascii_digit()
                        || ('０'..='９').contains(&c)
                })
        })
}
pub(crate) fn infer_stacked_suffix(
    bands: &[Vec<Col>],
    t: usize,
    hrules: &[(f32, f32, f32)],
) -> Option<Vec<f32>> {
    if t < 3 || t + 1 != bands.len() {
        return None;
    }
    let tail = &bands[t];
    let mut seen = std::collections::HashSet::new();
    if !bands.iter().flatten().all(|c| seen.insert(c.id)) {
        return None;
    }
    let center = |c: &Col| (c.x + c.x1) / 2.;
    let mut ws: Vec<_> = tail.iter().collect();
    ws.sort_by(|a, b| center(b).total_cmp(&center(a)).then(a.y.total_cmp(&b.y)));
    let mut columns: Vec<Vec<&Col>> = Vec::new();
    for c in ws {
        if let Some(cs) = columns
            .last_mut()
            .filter(|cs| (center(cs[0]) - center(c)).abs() < 3.)
        {
            cs.push(c);
        } else {
            columns.push(vec![c]);
        }
    }
    if columns.len() < 3 {
        return None;
    }
    let anchors: Vec<f32> = columns.iter().map(|cs| center(cs[0])).collect();
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
        if !(2..=24).contains(&office.chars().count())
            || office
                .chars()
                .any(|c| c.is_ascii_digit() || ('０'..='９').contains(&c) || "。、〇○".contains(c))
        {
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
        starts.push(cs[0].y);
        marks.push(cs[m].y);
        dates.push(cs[m + 1].y);
        office_bottom = office_bottom.max(bottom);
        mark_bottom = mark_bottom.max(cs[m].y1);
    }
    let span = |v: &[f32]| {
        v.iter().copied().fold(f32::NEG_INFINITY, f32::max)
            - v.iter().copied().fold(f32::INFINITY, f32::min)
    };
    if span(&starts) > 3. || span(&marks) > 3. || span(&dates) > 3. {
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
    Some(vec![
        (office_bottom + marks.iter().copied().fold(f32::INFINITY, f32::min)) / 2.,
        (mark_bottom + dates.iter().copied().fold(f32::INFINITY, f32::min)) / 2.,
    ])
}

#[cfg(test)]
#[path = "record_suffix_tests.rs"]
mod tests;
