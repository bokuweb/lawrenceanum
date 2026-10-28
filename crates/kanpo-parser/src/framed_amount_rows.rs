//! Rejoin a detached amount-only tier to its unique native row in a ruled frame.
//! Existing lines/cells remain authoritative; no glyph or text is synthesized.
use crate::{
    lines::PageRules,
    trace::{Line, Unit, Word},
};
use std::collections::HashMap;
type Rect = [f32; 4];
fn compact(t: &str) -> String {
    t.chars().filter(|c| !c.is_whitespace()).collect()
}
fn amount(t: &str) -> bool {
    let t = compact(t);
    t.chars().any(|c| c.is_ascii_digit())
        && t.chars()
            .all(|c| c.is_ascii_digit() || ",.△▲－−-()（）".contains(c))
}
fn lines(units: &[Unit]) -> Vec<(usize, usize, &Line)> {
    units
        .iter()
        .enumerate()
        .flat_map(|(u, t)| match t {
            Unit::Plain(ls) => ls.iter().enumerate().map(move |(l, v)| (u, l, v)).collect(),
            _ => Vec::new(),
        })
        .collect()
}
fn bounds(l: &Line, ws: &HashMap<usize, &Word>) -> Option<Rect> {
    if l.table.is_some() || l.words.is_empty() {
        return None;
    }
    let w = l
        .words
        .iter()
        .map(|id| ws.get(id).copied())
        .collect::<Option<Vec<_>>>()?;
    if w.iter()
        .any(|w| w.text.chars().count() >= 2 && w.y1 - w.y0 > (w.x1 - w.x0) * 1.5)
    {
        return None;
    }
    Some(w.into_iter().fold(
        [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ],
        |b, w| {
            [
                b[0].min(w.x0),
                b[1].min(w.y0),
                b[2].max(w.x1),
                b[3].max(w.y1),
            ]
        },
    ))
}
fn contains(f: Rect, b: Rect) -> bool {
    b[0] >= f[0] - 1. && b[2] <= f[2] + 1. && b[1] >= f[1] - 1. && b[3] <= f[3] + 1.
}
fn line_mut(u: &mut [Unit], unit: usize, line: usize) -> &mut Line {
    let Unit::Plain(ls) = &mut u[unit] else {
        unreachable!()
    };
    &mut ls[line]
}
pub(crate) fn restore(units: &mut [Unit], words: &[Word], rules: &PageRules) {
    let ws: HashMap<_, _> = words.iter().map(|w| (w.id, w)).collect();
    let mut vs: Vec<(f32, f32, f32)> = Vec::new();
    for v in &rules.vertical {
        let mut a = (v.x, v.y0, v.y1);
        loop {
            let Some(i) = vs
                .iter()
                .position(|b| (a.0 - b.0).abs() < 0.5 && a.1 <= b.2 + 1. && b.1 <= a.2 + 1.)
            else {
                break;
            };
            let b = vs.remove(i);
            a.1 = a.1.min(b.1);
            a.2 = a.2.max(b.2);
        }
        vs.push(a);
    }
    let frames: Vec<Rect> = vs
        .iter()
        .flat_map(|a| {
            vs.iter().filter_map(move |b| {
                (b.0 - a.0 >= 100.
                    && b.0 - a.0 <= 450.
                    && (a.1 - b.1).abs() < 1.
                    && (a.2 - b.2).abs() < 1.
                    && a.2 - a.1 >= 50.)
                    .then_some([a.0, a.1, b.0, a.2])
            })
        })
        .filter(|f| {
            rules.horizontal.iter().any(|h| {
                (h.y - f[3]).abs() < 1. && (h.x0 - f[0]).abs() < 1. && (h.x1 - f[2]).abs() < 1.
            }) && !vs.iter().any(|v| {
                v.0 > f[0] + 1.
                    && v.0 < f[2] - 1.
                    && v.2.min(f[3]) - v.1.max(f[1]) > (f[3] - f[1]) * 0.3
            })
        })
        .collect();
    let mut repaired = false;
    for frame in frames {
        let ls = lines(units);
        let bs: Vec<_> = ls.iter().map(|(_, _, l)| bounds(l, &ws)).collect();
        let mut owners: HashMap<usize, usize> = HashMap::new();
        let mut all = Vec::new();
        for u in units.iter() {
            match u {
                Unit::Plain(p) => all.extend(p),
                Unit::Shinkyu {
                    preamble,
                    after,
                    before,
                    outside,
                ..} => {
                    all.extend(preamble);
                    for p in after.iter().chain(before).chain(outside) {
                        all.extend(p);
                    }
                }
            }
        }
        for l in all {
            for id in l
                .words
                .iter()
                .chain(l.table.iter().flat_map(|t| &t.cells).flat_map(|c| &c.words))
            {
                *owners.entry(*id).or_default() += 1;
            }
        }
        let inside: Vec<usize> = (0..ls.len())
            .filter(|&i| bs[i].is_some_and(|b| contains(frame, b)))
            .collect();
        let nums: Vec<_> = inside
            .iter()
            .copied()
            .filter(|&i| {
                let b = bs[i].unwrap();
                let em = b[3] - b[1];
                amount(&ls[i].2.text)
                    && b[0] > (frame[0] + frame[2]) / 2.
                    && frame[2] - b[2] < em * 2.
                    && b[2] - b[0] <= em * 8.
            })
            .collect();
        if nums.len() < 2
            || nums
                .iter()
                .any(|&i| (bs[i].unwrap()[2] - bs[nums[0]].unwrap()[2]).abs() > 1.)
        {
            continue;
        }
        let mut pairs = Vec::new();
        let mut valid = true;
        for n in nums {
            let nb = bs[n].unwrap();
            let em = nb[3] - nb[1];
            let targets: Vec<_> = inside
                .iter()
                .copied()
                .filter(|&i| {
                    let lb = bs[i].unwrap();
                    i < n
                        && ls[i].0 != ls[n].0
                        && lb[2] < nb[0] - em
                        && ((lb[1] + lb[3] - nb[1] - nb[3]) / 2.).abs() < em * 0.15
                        && ls[i]
                            .2
                            .text
                            .chars()
                            .filter(|c| ('\u{4E00}'..='\u{9FFF}').contains(c))
                            .count()
                            >= 2
                        && !ls[i].2.text.contains(['。', '、', '：', ':'])
                })
                .collect();
            let [i] = targets.as_slice() else {
                valid = false;
                break;
            };
            if ls[n]
                .2
                .words
                .iter()
                .chain(&ls[*i].2.words)
                .any(|id| owners.get(id) != Some(&1))
                || pairs.iter().any(|(j, _)| j == i)
            {
                valid = false;
                break;
            }
            pairs.push((*i, n));
        }
        if !valid || pairs.len() < 2 {
            continue;
        }
        // A native row must be a short account, never a coincidentally aligned prose column.
        if pairs.iter().any(|(i, _)| {
            let t = compact(&ls[*i].2.text);
            t.chars().count() > 48
                || ![
                    "合計", "損失", "利益", "額", "金", "資産", "負債", "費用", "収益",
                ]
                .iter()
                .any(|s| t.contains(s))
        }) {
            continue;
        }
        let changes: Vec<_> = pairs
            .iter()
            .map(|&(i, n)| {
                (
                    ls[i].0,
                    ls[i].1,
                    ls[n].0,
                    ls[n].1,
                    ls[n].2.text.clone(),
                    ls[n].2.words.clone(),
                )
            })
            .collect();
        drop(ls);
        repaired = true;
        for (u, l, nu, nl, text, ids) in changes {
            let target = line_mut(units, u, l);
            target.text.push('\u{3000}');
            target.text.push_str(text.trim());
            target.words.extend(ids);
            let removed = line_mut(units, nu, nl);
            removed.words.clear();
            removed.text.clear();
        }
    }
    if !repaired {
        return;
    }
    for u in units {
        if let Unit::Plain(ls) = u {
            ls.retain(|l| !l.text.is_empty() || !l.words.is_empty() || l.table.is_some());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn seed() -> (Vec<Unit>, Vec<Word>, PageRules) {
        let mut w = vec![];
        for (id, x, y, t) in [
            (0, 8., 8., "経常収益合計"),
            (1, 8., 28., "当期総損失"),
            (2, 160., 8., "12,345"),
            (3, 160., 28., "23,456"),
        ] {
            w.push(Word {
                source_box: [x, y, if id < 2 { x + 48. } else { x + 32. }, y + 8.],
                id,
                x0: x,
                y0: y,
                x1: if id < 2 { x + 48. } else { x + 32. },
                y1: y + 8.,
                raw: t.into(),
                text: t.into(),
                status: crate::trace::WordStatus::Used,
            });
        }
        let u = vec![
            Unit::Plain(vec![
                Line::text("経常収益合計".into(), vec![0]),
                Line::text("当期総損失".into(), vec![1]),
            ]),
            Unit::Plain(vec![
                Line::text("12,345".into(), vec![2]),
                Line::text("23,456".into(), vec![3]),
            ]),
        ];
        let r = PageRules {
            vertical: vec![
                crate::lines::VRule {
                    x: 0.,
                    y0: 0.,
                    y1: 30.,
                },
                crate::lines::VRule {
                    x: 0.,
                    y0: 30.,
                    y1: 60.,
                },
                crate::lines::VRule {
                    x: 200.,
                    y0: 0.,
                    y1: 60.,
                },
            ],
            horizontal: vec![crate::lines::HRule {
                y: 60.,
                x0: 0.,
                x1: 200.,
            }],
            enclosures: vec![], short_fraction_bars: vec![],
        };
        (u, w, r)
    }
    fn text(u: &[Unit]) -> String {
        u.iter().map(Unit::text).collect::<Vec<_>>().join("\n")
    }
    #[test]
    fn uniquely_aligned_native_amounts_rejoin_without_rebuilding_accounts() {
        let (mut u, w, r) = seed();
        restore(&mut u, &w, &r);
        assert!(text(&u).contains("経常収益合計　12,345\n当期総損失　23,456"));
        let a = text(&u);
        restore(&mut u, &w, &r);
        assert_eq!(a, text(&u));
        assert_eq!(line_mut(&mut u, 0, 0).words, vec![0, 2]);
    }
    #[test]
    fn requires_explicit_shared_bottom_and_no_internal_column_rule() {
        for case in 0..3 {
            let (mut u, w, mut r) = seed();
            match case {
                0 => r.horizontal.clear(),
                1 => r.vertical.push(crate::lines::VRule {
                    x: 100.,
                    y0: 0.,
                    y1: 60.,
                }),
                _ => r.vertical[2].y0 = 10.,
            };
            let before = text(&u);
            restore(&mut u, &w, &r);
            assert_eq!(before, text(&u));
        }
    }
    #[test]
    fn rejects_offset_or_incomplete_or_non_aligned_amount_columns() {
        for case in 0..3 {
            let (mut u, mut w, r) = seed();
            match case {
                0 => w[3].y0 += 4.,
                1 => w[3].x1 -= 5.,
                _ => line_mut(&mut u, 1, 1).text = "金額不明".into(),
            };
            let before = text(&u);
            restore(&mut u, &w, &r);
            assert_eq!(before, text(&u));
        }
    }
    #[test]
    fn rejects_duplicate_owners_and_ambiguous_rows() {
        for case in 0..2 {
            let (mut u, w, r) = seed();
            let extra = if case == 0 {
                Line::text("重複".into(), vec![2])
            } else {
                Line::text("経常収益合計".into(), vec![0])
            };
            u.push(Unit::Plain(vec![extra]));
            let before = text(&u);
            restore(&mut u, &w, &r);
            assert_eq!(before, text(&u));
        }
    }
    #[test]
    fn does_not_merge_prose_or_rows_already_in_the_same_tier() {
        for case in 0..2 {
            let (mut u, mut w, r) = seed();
            if case == 0 {
                line_mut(&mut u, 0, 0).text = "利益について、述べます。".into();
                w[0].text = "利益について、述べます。".into();
            } else {
                let Unit::Plain(ls) = u.remove(1) else {
                    unreachable!()
                };
                let Unit::Plain(p) = &mut u[0] else {
                    unreachable!()
                };
                p.extend(ls);
            }
            let before = text(&u);
            restore(&mut u, &w, &r);
            assert_eq!(before, text(&u));
        }
    }
    #[test]
    fn local_append_preserves_a_previously_repaired_label_fraction_and_unit() {
        let (mut u, w, r) = seed();
        line_mut(&mut u, 0, 0).text = "経常収益合計(1/2)㎡（注）".into();
        line_mut(&mut u, 1, 1).text = "△23,456".into();
        restore(&mut u, &w, &r);
        assert_eq!(
            line_mut(&mut u, 0, 0).text,
            "経常収益合計(1/2)㎡（注）　12,345"
        );
        assert_eq!(line_mut(&mut u, 0, 1).text, "当期総損失　△23,456");
    }
}
