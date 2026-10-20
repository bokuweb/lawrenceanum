//! Recover an unlabelled change table printed as two ruled after/before pairs.
//! Completed Line objects are moved intact; no word, cell, or source text is rebuilt.
use crate::{
    lines::{HRule, PageRules},
    trace::{Line, Unit, Word, WordStatus},
};
use std::collections::{HashMap, HashSet};
type Rect = [f32; 4];
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn bounds(line: &Line, words: &HashMap<usize, &Word>) -> Option<Rect> {
    if line.table.is_some() || line.words.is_empty() {
        return None;
    }
    line.words.iter().try_fold(
        [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ],
        |b, id| {
            let w = words.get(id)?;
            (w.status == WordStatus::Used && !w.text.is_empty() && w.x1 > w.x0 && w.y1 > w.y0)
                .then_some([
                    b[0].min(w.x0),
                    b[1].min(w.y0),
                    b[2].max(w.x1),
                    b[3].max(w.y1),
                ])
        },
    )
}
fn union(bs: &[Rect]) -> Rect {
    bs.iter().fold(
        [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ],
        |a, b| {
            [
                a[0].min(b[0]),
                a[1].min(b[1]),
                a[2].max(b[2]),
                a[3].max(b[3]),
            ]
        },
    )
}
fn merged_horizontal(rules: &PageRules, width: f32) -> Vec<HRule> {
    let mut hs = rules.horizontal.clone();
    hs.retain(|h| h.y.is_finite() && h.x0.is_finite() && h.x1.is_finite() && h.x1 > h.x0);
    hs.sort_by(|a, b| a.y.total_cmp(&b.y).then_with(|| a.x0.total_cmp(&b.x0)));
    let mut out: Vec<HRule> = Vec::new();
    for h in hs {
        if let Some(prev) = out.last_mut() {
            if (prev.y - h.y).abs() < 0.1 && h.x0 <= prev.x1 + 0.25 {
                prev.x1 = prev.x1.max(h.x1);
                continue;
            }
        }
        out.push(h);
    }
    out.retain(|h| h.x1 - h.x0 >= width * 0.75);
    out
}
fn similarity(a: &str, b: &str) -> bool {
    let (a, b) = (compact(a), compact(b));
    let (aa, bb): (Vec<_>, Vec<_>) = (a.chars().collect(), b.chars().collect());
    // Bound both the cost and the supported shape rather than truncate a comparison.
    if aa.len() < 100 || bb.len() < 100 || aa.len() > 2000 || bb.len() > 2000 {
        return false;
    }
    let sa: HashSet<_> = aa.windows(2).map(|x| (x[0], x[1])).collect();
    let sb: HashSet<_> = bb.windows(2).map(|x| (x[0], x[1])).collect();
    if sa.intersection(&sb).count() as f32 / (sa.union(&sb).count() as f32) < 0.6 {
        return false;
    }
    let mut prev = vec![0usize; bb.len() + 1];
    let mut cur = prev.clone();
    for &x in &aa {
        for (j, &y) in bb.iter().enumerate() {
            cur[j + 1] = if x == y {
                prev[j] + 1
            } else {
                prev[j + 1].max(cur[j])
            };
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    2. * prev[bb.len()] as f32 / (aa.len() + bb.len()) as f32 >= 0.6
}
fn plan(units: &[Unit], words: &[Word], rules: &PageRules, width: f32) -> bool {
    // Whole-page continuation only: unrelated body/preamble/table units are not candidates.
    if units.len() != 4 || !width.is_finite() || width <= 0. {
        return false;
    }
    let ls: Vec<&[Line]> = units
        .iter()
        .filter_map(|u| {
            if let Unit::Plain(ls) = u {
                Some(ls.as_slice())
            } else {
                None
            }
        })
        .collect();
    if ls.len() != 4 || ls.iter().any(|ls| ls.len() < 6) {
        return false;
    }
    let ws: HashMap<_, _> = words.iter().map(|w| (w.id, w)).collect();
    if ws.len() != words.len() {
        return false;
    }
    let mut ids = HashSet::new();
    for l in ls.iter().flat_map(|ls| ls.iter()) {
        if l.words.iter().any(|id| !ids.insert(*id)) {
            return false;
        }
        let t = compact(&l.text);
        // A printed notice heading cannot be part of an unlabelled continuation.
        if t.starts_with('〇') || t.starts_with('○') {
            return false;
        }
    }
    // Test the whole column stream, too: amending instructions may wrap at a column edge.
    for part in &ls {
        let text: String = part.iter().map(|l| compact(&l.text)).collect();
        if [
            "の一部を次のように改正する",
            "を次のように改める",
            "の次に次の",
            "」に改め",
            "」を削り",
        ]
        .iter()
        .any(|s| text.contains(s))
        {
            return false;
        }
    }
    if ids.is_empty() { return false; }
    let mut widths: Vec<_> = ids
        .iter()
        .filter_map(|id| ws.get(id).map(|w| w.x1 - w.x0))
        .collect();
    if widths.len() != ids.len() {
        return false;
    }
    widths.sort_by(f32::total_cmp);
    let em = widths[widths.len() / 2];
    if !em.is_finite()
        || em <= 0.
        || widths
            .iter()
            .any(|x| !x.is_finite() || *x < em * 0.9 || *x > em * 1.1)
    {
        return false;
    }
    let Some(bs): Option<Vec<Vec<Rect>>> = ls
        .iter()
        .map(|ls| ls.iter().map(|l| bounds(l, &ws)).collect())
        .collect()
    else {
        return false;
    };
    // Each completed line must still be a single vertical column, in right-to-left order.
    if bs.iter().any(|b| {
        b.iter().any(|b| b[2] - b[0] > em * 1.25)
            || b.windows(2).any(|p| p[1][0] > p[0][0] + em * 0.2)
    }) {
        return false;
    }
    let ext: Vec<_> = bs.iter().map(|b| union(b)).collect();
    if ext.windows(2).any(|p| p[0][3] >= p[1][1]) {
        return false;
    }
    if ids
        .iter()
        .filter(|id| ws[id].y1 - ws[id].y0 >= em * 1.5)
        .count()
        * 4
        < ids.len() * 3
    {
        return false;
    }
    let hs = merged_horizontal(rules, width);
    let above = |y: f32| {
        hs.iter()
            .filter(|h| h.y <= y)
            .max_by(|a, b| a.y.total_cmp(&b.y))
    };
    let below = |y: f32| {
        hs.iter()
            .filter(|h| h.y >= y)
            .min_by(|a, b| a.y.total_cmp(&b.y))
    };
    let chosen = [
        above(ext[0][1]),
        above(ext[1][1]),
        below(ext[1][3]),
        above(ext[2][1]),
        above(ext[3][1]),
        below(ext[3][3]),
    ];
    let Some(edges): Option<Vec<_>> = chosen.into_iter().collect() else {
        return false;
    };
    let (left, right) = (edges[0].x0, edges[0].x1);
    if edges
        .iter()
        .any(|h| (h.x0 - left).abs() > em * 0.1 || (h.x1 - right).abs() > em * 0.1)
    {
        return false;
    }
    let ys: Vec<_> = edges.iter().map(|h| h.y).collect();
    let heights = [ys[1] - ys[0], ys[2] - ys[1], ys[4] - ys[3], ys[5] - ys[4]];
    let height = heights[0];
    if height < em * 10.
        || heights.iter().any(|h| (*h - height).abs() > em * 0.15)
        || ys[3] - ys[2] < em
        || ys[3] - ys[2] > em * 4.
    {
        return false;
    }
    let bands = [
        [left, ys[0], right, ys[1]],
        [left, ys[1], right, ys[2]],
        [left, ys[3], right, ys[4]],
        [left, ys[4], right, ys[5]],
    ];
    let contains = |r: Rect, b: Rect| {
        b[0] >= r[0] - em * 0.05 && b[2] <= r[2] + em * 0.05 && b[1] >= r[1] && b[3] <= r[3]
    };
    if bs
        .iter()
        .enumerate()
        .any(|(i, bs)| bs.iter().any(|&b| !contains(bands[i], b)))
    {
        return false;
    }
    // No extra printed word inside a band may be silently left outside its candidate owner.
    if words.iter().filter(|w| !w.text.is_empty()).any(|w| {
        bands.iter().any(|&b| contains(b, [w.x0, w.y0, w.x1, w.y1])) && !ids.contains(&w.id)
    }) {
        return false;
    }
    // Do not traverse vertical cell boundaries.
    if rules.vertical.iter().any(|v| {
        v.x >= left && v.x <= right && bands.iter().any(|b| v.y1.min(b[3]) - v.y0.max(b[1]) > em)
    }) {
        return false;
    }
    let mark = |t: &str| {
        matches!(
            compact(t).as_str(),
            "（削る）" | "（新設）" | "〔削る〕" | "〔新設〕" | "［削る］" | "［新設］"
        )
    };
    if !ls[..2]
        .iter()
        .flat_map(|ls| ls.iter())
        .any(|l| mark(&l.text))
    {
        return false;
    }
    if [2, 3].iter().any(|&i| {
        ls[i]
            .iter()
            .filter(|l| compact(&l.text).contains("略）"))
            .count()
            < 2
    }) {
        return false;
    }
    // The same final clauses must end both lower cells at identical printed columns.
    // A lexical match alone is not sufficient (ordinary repeated notices share boilerplate).
    let mut suffix = 0;
    let mut suffix_chars = 0;
    for ((a, ab), (b, bb)) in ls[2]
        .iter()
        .zip(&bs[2])
        .rev()
        .zip(ls[3].iter().zip(&bs[3]).rev())
    {
        if compact(&a.text) != compact(&b.text)
            || (ab[0] - bb[0]).abs() > em * 0.2
            || (ab[2] - bb[2]).abs() > em * 0.2
            || (ab[1] + height - bb[1]).abs() > em * 0.2
            || (ab[3] + height - bb[3]).abs() > em * 0.2
        {
            break;
        }
        suffix += 1;
        suffix_chars += compact(&a.text).chars().count();
    }
    if suffix < 3 || suffix_chars < 40 {
        return false;
    }
    let text = |indices: [usize; 2]| {
        indices
            .iter()
            .flat_map(|&i| ls[i].iter())
            .map(|l| l.text.as_str())
            .collect::<String>()
    };
    similarity(&text([0, 2]), &text([1, 3]))
}
pub(crate) fn restore(
    units: &mut Vec<Unit>,
    words: &[Word],
    rules: Option<&PageRules>,
    width: f32,
) {
    let Some(rules) = rules else {
        return;
    };
    if !plan(units, words, rules, width) {
        return;
    }
    let mut original = std::mem::take(units).into_iter();
    let mut take = || {
        if let Unit::Plain(ls) = original.next().unwrap() {
            ls
        } else {
            unreachable!()
        }
    };
    let (a, b, c, d) = (take(), take(), take(), take());
    units.push(Unit::Shinkyu {
        preamble: Vec::new(),
        after: vec![a, c],
        before: vec![b, d],
        outside: Vec::new(),
    });
}
