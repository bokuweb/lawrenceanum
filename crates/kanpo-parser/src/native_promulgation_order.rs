//! A displaced promulgation signature returns to its unique physical column.
//! No literal personal name, instrument-number value, or previous guessed
//! article role selects the move. Each complete original row must have
//! one downwards native glyph path and non-overlapping original glyph ranges.
use crate::{
    native_column_flow::AxisFlow,
    native_word_axes::NativeAxis,
    trace::{Line, NativeBounds, Unit, Word, WordStatus},
};
use std::collections::BTreeSet;
struct Row {
    path: f64,
    start: f64,
    end: f64,
    b: [f32; 4],
    em: f64,
    glyphs: Vec<(usize, usize)>,
}
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn row(
    l: &Line,
    words: &[Word],
    bounds: &[NativeBounds],
    axes: &[NativeAxis],
    flows: &[Option<AxisFlow>],
) -> Option<Row> {
    if l.table.is_some() || l.words.is_empty() {
        return None;
    }
    let mut ids = BTreeSet::new();
    let mut text = String::new();
    let mut out: Option<Row> = None;
    for &id in &l.words {
        let w = words.get(id)?;
        if w.id != id
            || w.status != WordStatus::Used
            || !ids.insert(id)
            || *axes.get(id)? != NativeAxis::Down
        {
            return None;
        }
        let NativeBounds::Glyphs(b) = *bounds.get(id)? else {
            return None;
        };
        let f = flows.get(id)?.as_ref()?;
        let [path, start, end] = f.vertical_path(b)?;
        let em = (b[2] - b[0]) as f64;
        if let Some(r) = &mut out {
            if (path - r.path).abs() > r.em * 0.05
                || (em - r.em).abs() > r.em * 0.05
                || start < r.end - r.em * 0.05
            {
                return None;
            }
            r.end = end;
            r.b[0] = r.b[0].min(b[0]);
            r.b[1] = r.b[1].min(b[1]);
            r.b[2] = r.b[2].max(b[2]);
            r.b[3] = r.b[3].max(b[3]);
            r.glyphs.push(f.source_range());
        } else {
            out = Some(Row {
                path,
                start,
                end,
                b,
                em,
                glyphs: vec![f.source_range()],
            });
        }
        text.push_str(&compact(&w.text));
    }
    if text != compact(&l.text) {
        return None;
    }
    out
}
fn signature(l: &Line, words: &[Word], bounds: &[NativeBounds]) -> bool {
    let text = compact(&l.text);
    let Some(name) = text.strip_prefix("内閣総理大臣") else {
        return false;
    };
    if !(2..=20).contains(&name.chars().count())
        || !name
            .chars()
            .all(|c| c.is_alphabetic() || matches!(c, '・' | '々' | 'ヶ'))
    {
        return false;
    }
    let mut prefix = String::new();
    for (i, &id) in l.words.iter().enumerate() {
        let Some(w) = words.get(id) else { return false };
        prefix.push_str(&compact(&w.text));
        if prefix == "内閣総理大臣" {
            let Some(&next) = l.words.get(i + 1) else {
                return false;
            };
            let (Some(NativeBounds::Glyphs(a)), Some(NativeBounds::Glyphs(b))) =
                (bounds.get(id), bounds.get(next))
            else {
                return false;
            };
            let em = a[2] - a[0];
            let gap = b[1] - a[3];
            return gap >= em * 0.5 && gap <= em * 4.5;
        }
        if !"内閣総理大臣".starts_with(&prefix) {
            return false;
        }
    }
    false
}
fn identifier(l: &Line) -> bool {
    let text = compact(&l.text);
    let Some((kind, n)) = text.split_once('第') else {
        return false;
    };
    let Some(n) = n.strip_suffix('号') else {
        return false;
    };
    matches!(kind, "法律" | "政令" | "条約" | "勅令")
        && !n.is_empty()
        && n.chars()
            .all(|c| "〇零一二三四五六七八九十百千万0123456789０１２３４５６７８９".contains(c))
}
fn triplet(d: &Row, s: &Row, n: &Row) -> bool {
    if (d.em - s.em).abs() > d.em * 0.05
        || (d.em - n.em).abs() > d.em * 0.05
        || s.b[2] > d.b[0]
        || n.b[2] > s.b[0]
        || d.path - n.path > d.em * 6.
        || d.start < n.start
        || d.start - n.start > d.em * 8.
        || s.start < d.end.max(n.end)
    {
        return false;
    }
    let mut glyphs = d
        .glyphs
        .iter()
        .chain(&s.glyphs)
        .chain(&n.glyphs)
        .copied()
        .collect::<Vec<_>>();
    glyphs.sort_unstable();
    !glyphs.windows(2).any(|g| g[0].1 > g[1].0)
}
pub(crate) fn restore(
    units: &mut Vec<Unit>,
    words: &[Word],
    bounds: &[NativeBounds],
    axes: &[NativeAxis],
    flows: &[Option<AxisFlow>],
) {
    let mut changed = false;
    loop {
        let mut flat = Vec::new();
        for (u, unit) in units.iter().enumerate() {
            if let Unit::Plain(lines) = unit {
                for (l, line) in lines.iter().enumerate() {
                    flat.push((u, l, line));
                }
            }
        }
        let mut plans = Vec::new();
        for (seal, (_, _, line)) in flat.iter().enumerate() {
            if compact(&line.text) != "御名御璽" {
                continue;
            }
            let prefix = flat[..seal]
                .iter()
                .rev()
                .take(4)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .map(|(_, _, l)| compact(&l.text))
                .collect::<String>();
            if !prefix.ends_with("をここに公布する。") {
                continue;
            }
            let Some((du, _, date)) = flat.get(seal + 1) else {
                continue;
            };
            if !crate::notice_boundaries::date(&compact(&date.text)) {
                continue;
            }
            let Some(d) = row(date, words, bounds, axes, flows) else {
                continue;
            };
            let end = flat
                .iter()
                .enumerate()
                .skip(seal + 2)
                .find(|(_, (_, _, l))| compact(&l.text) == "御名御璽")
                .map_or(flat.len(), |(i, _)| i);
            let mut own = Vec::new();
            for (ni, (nu, nl, number)) in flat.iter().enumerate().take(end).skip(seal + 2) {
                if !identifier(number) {
                    continue;
                }
                let Some(n) = row(number, words, bounds, axes, flows) else {
                    continue;
                };
                for &(su, sl, signer) in &flat[ni + 1..end] {
                    if !signature(signer, words, bounds) {
                        continue;
                    }
                    let Some(s) = row(signer, words, bounds, axes, flows) else {
                        continue;
                    };
                    let first = (*du).min(*nu).min(su);
                    let last = (*du).max(*nu).max(su);
                    if units[first..=last]
                        .iter()
                        .any(|u| !matches!(u, Unit::Plain(_)))
                    {
                        continue;
                    }
                    if triplet(&d, &s, &n) {
                        own.push((su, sl, *nu, *nl));
                    }
                }
            }
            if own.len() == 1 {
                plans.push(own[0]);
            }
        }
        plans.sort_unstable();
        plans.dedup();
        let Some((su, sl, nu, nl)) = plans.first().copied() else {
            break;
        };
        let Unit::Plain(from) = &mut units[su] else {
            unreachable!()
        };
        let signer = from.remove(sl);
        let Unit::Plain(to) = &mut units[nu] else {
            unreachable!()
        };
        to.insert(nl, signer);
        changed = true;
    }
    if changed {
        units.retain(|u| !matches!(u,Unit::Plain(lines) if lines.is_empty()));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_headers_keep_the_signature_before_the_instrument_number() {
        for pdf in [
            include_bytes!("../tests/data/workbook/case-13.pdf").as_slice(),
            include_bytes!("../tests/data/workbook/case-18.pdf").as_slice(),
        ] {
            let inputs = crate::page_inputs(pdf).unwrap();
            let a = crate::analyze_item(&inputs[..1]);
            let text = &a.articles[0].text;
            assert!(
                text.find("内閣総理大臣").unwrap() < text.find("政令第").unwrap(),
                "{text}"
            );
            let ids = a.pages[0]
                .regions
                .iter()
                .flat_map(|r| &r.lines)
                .flat_map(|l| {
                    l.words.iter().copied().chain(
                        l.table
                            .iter()
                            .flat_map(|t| t.cells.iter().flat_map(|c| c.words.iter().copied())),
                    )
                })
                .collect::<Vec<_>>();
            let unique = ids.iter().copied().collect::<BTreeSet<_>>();
            assert_eq!(
                ids.len(),
                unique.len(),
                "moving the complete row must not duplicate source Words"
            );
        }
    }
    #[test]
    fn crossing_columns_foreign_indent_or_shared_source_glyphs_do_not_prove_a_triplet() {
        let make = |x: f64, start: f64, end: f64, glyph: usize| Row {
            path: x,
            start,
            end,
            em: 8.,
            b: [(x - 4.) as f32, start as f32, (x + 4.) as f32, end as f32],
            glyphs: vec![(glyph, glyph + 2)],
        };
        let d = make(100., 40., 120., 0);
        let s = make(88., 240., 320., 4);
        let n = make(68., 8., 72., 8);
        assert!(triplet(&d, &s, &n));
        assert!(!triplet(&d, &make(102., 240., 320., 4), &n));
        assert!(!triplet(&d, &s, &make(68., 160., 224., 8)));
        assert!(!triplet(&d, &make(88., 240., 320., 1), &n));
    }
}
