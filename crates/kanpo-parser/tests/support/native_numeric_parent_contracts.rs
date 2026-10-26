//! Independent source-occurrence and full-parent checks for printed matrices.
//! The older closed-frame contract remains strict for all its existing callers.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

fn word_ids(t: &Value) -> Vec<usize> {
    array(&t["cells"])
        .iter()
        .flat_map(|c| array(&c["words"]))
        .map(n)
        .collect()
}
fn parent_frame(rules: &Value, page: &Value, t: &Value) -> Frame {
    let owned: BTreeSet<_> = word_ids(t).into_iter().collect();
    let mut answers = native_frames(rules)
        .into_iter()
        .filter(|r| {
            r.x0 == f(&t["x0"])
                && r.x1 == f(&t["x1"])
                && r.y0 < f(&t["y1"])
                && f(&t["y0"]) < r.y1
                && (!r.top || r.y0 <= f(&t["y0"]))
                && (!r.bottom || f(&t["y1"]) <= r.y1)
                && array(&page["words"])
                    .iter()
                    .filter(|w| {
                        s(&w["status"]) == "used"
                            && f(&w["x0"]) < r.x1
                            && r.x0 < f(&w["x1"])
                            && f(&w["y0"]) < r.y1
                            && r.y0 < f(&w["y1"])
                    })
                    .map(|w| n(&w["id"]))
                    .collect::<BTreeSet<_>>()
                    == owned
        })
        .collect::<Vec<_>>();
    assert_eq!(answers.len(), 1, "one complete original parent inventory");
    answers.remove(0)
}
fn source_baselines(xhtml: &str, page: &Value) -> BTreeMap<usize, Option<f64>> {
    let dom = scraper::Html::parse_document(xhtml);
    let sel = scraper::Selector::parse("word").unwrap();
    let mut by = BTreeMap::<(String, [u32; 4]), Vec<scraper::ElementRef<'_>>>::new();
    for w in dom.select(&sel) {
        let b = ["xmin", "ymin", "xmax", "ymax"]
            .map(|k| w.value().attr(k).unwrap().parse::<f32>().unwrap().to_bits());
        by.entry((w.text().collect(), b)).or_default().push(w);
    }
    assert_eq!(
        by.values().map(Vec::len).sum::<usize>(),
        array(&page["words"]).len()
    );
    let mut out = BTreeMap::new();
    for w in array(&page["words"]) {
        let b = std::array::from_fn(|i| (f(&w["source_box"][i]) as f32).to_bits());
        let native = by
            .get(&(s(&w["raw"]).to_owned(), b))
            .expect("same original source occurrence");
        let ys = native
            .iter()
            .map(|x| {
                let a = x.value();
                if a.attr("data-wmode") != Some("0") || a.attr("data-native-axis") != Some("right")
                {
                    return None;
                }
                let values =
                    ["start", "baseline", "end", "leading", "trailing", "prefix"].map(|k| {
                        a.attr(&format!("data-native-flow-{k}"))
                            .and_then(|v| v.parse::<f64>().ok())
                    });
                let [Some(start), Some(y), Some(end), Some(leading), Some(trailing), Some(prefix)] =
                    values
                else {
                    return None;
                };
                ([start, y, end, leading, trailing, prefix]
                    .iter()
                    .all(|v| v.is_finite())
                    && start < end
                    && 0. < leading
                    && leading <= end - start
                    && start as f32 == f(&w["x0"]) as f32
                    && end as f32 == f(&w["x1"]) as f32
                    && (f(&w["y0"]) as f32 as f64) <= y
                    && y <= f(&w["y1"]) as f32 as f64)
                    .then_some(y)
            })
            .collect::<Vec<_>>();
        assert!(
            ys.iter().all(|y| *y == ys[0]),
            "ambiguous source occurrences cannot borrow metadata"
        );
        out.insert(n(&w["id"]), ys[0]);
    }
    out
}
fn dividers(rules: &Value, a: &Frame, b: &Frame) -> Vec<f64> {
    let mut xs = array(&rules["vertical"])
        .iter()
        .map(|v| f(&v["x"]))
        .collect::<Vec<_>>();
    xs.sort_by(f64::total_cmp);
    xs.dedup();
    xs.into_iter()
        .filter(|x| a.x1 < *x && *x < b.x0)
        .filter(|x| {
            let mut ys = array(&rules["vertical"])
                .iter()
                .filter(|v| f(&v["x"]) == *x)
                .map(|v| (f(&v["y0"]), f(&v["y1"])))
                .collect::<Vec<_>>();
            ys.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
            let mut merged: Vec<(f64, f64)> = vec![];
            for (lo, hi) in ys {
                if let Some(last) = merged.last_mut().filter(|last| lo <= last.1) {
                    last.1 = last.1.max(hi);
                } else {
                    merged.push((lo, hi));
                }
            }
            merged.contains(&(b.y0, a.y1))
        })
        .collect()
}
fn columns(parts: &[(&Value, &Frame)], page: &Value, baselines: &BTreeMap<usize, Option<f64>>) {
    let cols = n(&parts[0].0["cols"]);
    let mut rails = vec![BTreeSet::new(); cols];
    let mut occurrences = vec![BTreeSet::new(); cols];
    let mut cooccur = vec![false; cols];
    for (part, (t, frame)) in parts.iter().enumerate() {
        assert_eq!(n(&t["cols"]), cols);
        for row in 0..n(&t["rows"]) {
            let cells = array(&t["cells"])
                .iter()
                .filter(|c| n(&c["row"]) == row)
                .collect::<Vec<_>>();
            let owned = cells
                .iter()
                .flat_map(|c| array(&c["words"]))
                .map(n)
                .collect::<Vec<_>>();
            let top = owned
                .iter()
                .map(|id| f(&page["words"][*id]["y0"]) as f32 as f64)
                .reduce(f64::max)
                .unwrap();
            let bottom = owned
                .iter()
                .map(|id| f(&page["words"][*id]["y1"]) as f32 as f64)
                .reduce(f64::min)
                .unwrap();
            assert!(top < bottom);
            let mut numeric = vec![];
            for c in cells {
                let ids = array(&c["words"]).iter().map(n).collect::<Vec<_>>();
                if ids.is_empty() {
                    continue;
                }
                let y = baselines[&ids[0]].expect("each original cell has native horizontal flow");
                assert!(top <= y && y <= bottom && ids.iter().all(|id| baselines[id] == Some(y)));
                let col = n(&c["col"]);
                if col > 0 {
                    let right = ids
                        .iter()
                        .map(|id| f(&page["words"][*id]["x1"]) as f32 as f64)
                        .reduce(f64::max)
                        .unwrap()
                        - frame.x0;
                    rails[col].insert(right.to_bits());
                    occurrences[col].insert((part, row));
                    numeric.push(col);
                }
            }
            if numeric.len() > 1 {
                for col in numeric {
                    cooccur[col] = true;
                }
            }
        }
    }
    for col in 1..cols {
        assert_eq!(
            rails[col].len(),
            1,
            "one exact physical right edge per logical amount column"
        );
        assert!(
            cooccur[col] || occurrences[col].len() > 1,
            "an isolated source amount cannot invent a total column"
        );
    }
}
pub(crate) fn reviewed_parent_projection(
    old: &Value,
    new: &Value,
    rules: &[Value],
    xhtml: &[String],
) -> usize {
    assert_eq!(xhtml.len(), rules.len());
    let baselines = array(&new["pages"])
        .iter()
        .zip(xhtml)
        .map(|(p, x)| source_baselines(x, p))
        .collect::<Vec<_>>();
    reviewed_projection_with_parent(old, new, rules, |pi, before, page, t| {
        let frame = parent_frame(&rules[pi], page, t);
        if frame.top && frame.bottom {
            columns(&[(t, &frame)], page, &baselines[pi]);
            return;
        }
        let old_tables = array(&before["regions"])
            .iter()
            .flat_map(|r| array(&r["lines"]))
            .filter_map(|l| l.get("table"))
            .collect::<Vec<_>>();
        let mut candidates = vec![];
        for region in array(&page["regions"]) {
            for pair in array(&region["lines"]).windows(2) {
                let (Some(a), Some(b)) = (pair[0].get("table"), pair[1].get("table")) else {
                    continue;
                };
                if (a != t && b != t) || old_tables.contains(&a) || old_tables.contains(&b) {
                    continue;
                }
                let fa = parent_frame(&rules[pi], page, a);
                let fb = parent_frame(&rules[pi], page, b);
                if !fa.top
                    || fa.bottom
                    || fb.top
                    || !fb.bottom
                    || fa.x1 >= fb.x0
                    || fa.y0 < fb.y0
                    || fb.y1 > fa.y1
                {
                    continue;
                }
                if fb.x0 - fa.x0 != fb.x1 - fa.x1 {
                    continue;
                }
                let ds = dividers(&rules[pi], &fa, &fb);
                if ds.len() != 1 {
                    continue;
                }
                let d = ds[0];
                let ia = word_ids(a);
                let ib = word_ids(b);
                let last = ia
                    .iter()
                    .map(|id| baselines[pi][id].unwrap())
                    .reduce(f64::max)
                    .unwrap();
                let first = ib
                    .iter()
                    .map(|id| baselines[pi][id].unwrap())
                    .reduce(f64::min)
                    .unwrap();
                for w in array(&page["words"])
                    .iter()
                    .filter(|w| s(&w["status"]) == "used")
                {
                    let id = n(&w["id"]);
                    let y = baselines[pi][&id].expect("complete printed column inventory");
                    assert!(fb.y0 <= y && y <= fa.y1 && !(f(&w["x0"]) < d && d < f(&w["x1"])));
                    if f(&w["x1"]) <= d {
                        assert!(y <= last && (y != last || ia.contains(&id)));
                    } else {
                        assert!(first <= y && (y != first || ib.contains(&id)));
                    }
                }
                columns(&[(a, &fa), (b, &fb)], page, &baselines[pi]);
                candidates.push(());
            }
        }
        assert_eq!(
            candidates.len(),
            1,
            "both original caps, one divider and the complete adjacent column flow"
        );
    })
}
