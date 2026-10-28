//! Direct original PDF Glyph authority for the optional terminal-column profile.
//! Older exact-rail/parent contracts keep all their original callers and guards.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug)]
struct Proof {
    first: usize,
    after: usize,
    start: f64,
    end: f64,
    baseline: f64,
    last: f64,
}
fn native_f(v: &Value) -> f64 {
    v.as_str().unwrap().parse().unwrap()
}
fn original_proofs(page: &Value, original: &Value, xhtml: &str) -> BTreeMap<usize, Proof> {
    let dom = scraper::Html::parse_document(xhtml);
    let mut xml = BTreeMap::<(String, [u32; 4]), Vec<scraper::ElementRef<'_>>>::new();
    for w in dom.select(&scraper::Selector::parse("word").unwrap()) {
        let b = ["xmin", "ymin", "xmax", "ymax"]
            .map(|k| w.value().attr(k).unwrap().parse::<f32>().unwrap().to_bits());
        xml.entry((w.text().collect(), b)).or_default().push(w);
    }
    let mut native = BTreeMap::<(String, [u32; 4]), Vec<&Value>>::new();
    for w in array(&original["words"]) {
        let b = std::array::from_fn(|i| {
            format!("{:.6}", f(&w["box"][i]))
                .parse::<f32>()
                .unwrap()
                .to_bits()
        });
        native
            .entry((s(&w["raw"]).to_owned(), b))
            .or_default()
            .push(w);
    }
    assert_eq!(
        xml.values().map(Vec::len).sum::<usize>(),
        array(&page["words"]).len()
    );
    assert_eq!(
        native.values().map(Vec::len).sum::<usize>(),
        array(&page["words"]).len()
    );
    let mut out = BTreeMap::new();
    for w in array(&page["words"]) {
        let b = std::array::from_fn(|i| (f(&w["source_box"][i]) as f32).to_bits());
        let key = (s(&w["raw"]).to_owned(), b);
        let [source] = native[&key].as_slice() else {
            panic!("unique actual original Word")
        };
        let [element] = xml[&key].as_slice() else {
            panic!("unique own XML occurrence")
        };
        let attrs = element.value();
        let Some(raw) = attrs.attr("data-native-terminal-glyph") else {
            continue;
        };
        let v = raw.split_whitespace().collect::<Vec<_>>();
        assert_eq!(v.len(), 6);
        assert_eq!(v[0], "1");
        let first = v[1].parse::<usize>().unwrap();
        let after = v[2].parse::<usize>().unwrap();
        let gs = array(&source["glyphs"]).iter().map(n).collect::<Vec<_>>();
        assert_eq!(
            gs,
            (first..after).collect::<Vec<_>>(),
            "own complete original contiguous glyph range"
        );
        assert!(first < after && after <= array(&original["glyphs"]).len());
        let glyphs = gs
            .iter()
            .map(|i| &original["glyphs"][*i])
            .collect::<Vec<_>>();
        let head = glyphs[0];
        let last = glyphs[glyphs.len() - 1];
        assert!(glyphs
            .iter()
            .all(|g| g["vertical"] == false && g["y"] == head["y"]));
        let p = Proof {
            first,
            after,
            start: native_f(&head["x"]),
            end: native_f(&last["end_x"]),
            baseline: native_f(&last["y"]),
            last: native_f(&last["x"]),
        };
        assert_eq!(
            [
                v[3].parse::<f64>().unwrap(),
                v[4].parse().unwrap(),
                v[5].parse().unwrap()
            ],
            [p.last, p.end, p.baseline],
            "actual LAST original Glyph, no Unicode count or leading-width substitution"
        );
        assert_eq!(attrs.attr("data-native-axis"), Some("right"));
        assert_eq!(
            ["start", "end", "baseline"].map(|k| attrs
                .attr(&format!("data-native-flow-{k}"))
                .unwrap()
                .parse::<f64>()
                .unwrap()),
            [p.start, p.end, p.baseline]
        );
        assert_eq!(
            [p.start as f32, p.end as f32],
            [f(&w["x0"]) as f32, f(&w["x1"]) as f32]
        );
        assert_eq!(
            attrs
                .attr("data-native-flow-first-glyph")
                .unwrap()
                .parse::<usize>()
                .unwrap(),
            first
        );
        assert_eq!(
            attrs
                .attr("data-native-flow-after-glyph")
                .unwrap()
                .parse::<usize>()
                .unwrap(),
            after
        );
        out.insert(n(&w["id"]), p);
    }
    let mut claims = out.values().map(|p| (p.first, p.after)).collect::<Vec<_>>();
    claims.sort_unstable();
    assert!(
        claims.windows(2).all(|p| p[0].1 <= p[1].0),
        "original glyph ownership never overlaps"
    );
    out
}
fn covered(mut intervals: Vec<(f64, f64)>, lo: f64, hi: f64) -> bool {
    intervals.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let mut end = lo;
    for (a, b) in intervals {
        if a > end {
            break;
        }
        end = end.max(b);
        if hi <= end {
            return true;
        }
    }
    false
}
fn certify(before: &Value, table: &Value, rules: &Value, proof: &BTreeMap<usize, Proof>) {
    assert_eq!(n(&table["cols"]), 2);
    let (x0, x1, y0, y1) = (
        f(&table["x0"]),
        f(&table["x1"]),
        f(&table["y0"]),
        f(&table["y1"]),
    );
    let mut caps = array(&rules["horizontal"])
        .iter()
        .map(|h| f(&h["y"]))
        .filter(|y| {
            covered(
                array(&rules["horizontal"])
                    .iter()
                    .filter(|h| f(&h["y"]) == *y)
                    .map(|h| (f(&h["x0"]), f(&h["x1"])))
                    .collect(),
                x0,
                x1,
            )
        })
        .collect::<Vec<_>>();
    caps.sort_by(f64::total_cmp);
    caps.dedup();
    let mut parents = vec![];
    for &top in &caps {
        for &bottom in &caps {
            if top <= y0
                && y1 <= bottom
                && top < bottom
                && [x0, x1].iter().all(|x| {
                    covered(
                        array(&rules["vertical"])
                            .iter()
                            .filter(|v| f(&v["x"]) == *x)
                            .map(|v| (f(&v["y0"]), f(&v["y1"])))
                            .collect(),
                        top,
                        bottom,
                    )
                })
            {
                parents.push((top, bottom));
            }
        }
    }
    let parents = parents
        .iter()
        .copied()
        .filter(|v| !parents.iter().any(|z| z != v && v.0 <= z.0 && z.1 <= v.1))
        .collect::<Vec<_>>();
    let [(top, bottom)] = parents.as_slice() else {
        panic!("one smallest literal closed parent")
    };
    assert!(
        !caps.iter().any(|y| top < y && y < bottom),
        "no internal full source crosscut"
    );
    let ws = array(&before["words"]);
    let inventory = ws
        .iter()
        .filter(|w| {
            s(&w["status"]) == "used"
                && f(&w["x0"]) < x1
                && x0 < f(&w["x1"])
                && f(&w["y0"]) < *bottom
                && *top < f(&w["y1"])
        })
        .map(|w| n(&w["id"]))
        .collect::<BTreeSet<_>>();
    let owned = array(&table["cells"])
        .iter()
        .flat_map(|c| array(&c["words"]))
        .map(n)
        .collect::<Vec<_>>();
    assert_eq!(owned.len(), inventory.len());
    assert_eq!(
        owned.iter().copied().collect::<BTreeSet<_>>(),
        inventory,
        "all original source rows/headings exactly once"
    );
    assert!(owned.iter().all(|i| *top <= f(&ws[*i]["y0"])
        && f(&ws[*i]["y1"]) <= *bottom
        && x0 <= f(&ws[*i]["x0"])
        && f(&ws[*i]["x1"]) <= x1));
    // The unchanged material contract has already proved row order, owner streams,
    // exact cell text, source rectangles, all Doc styles/merges and notice streams.
    let mut lo = f64::NEG_INFINITY;
    let mut hi = f64::INFINITY;
    let mut label_end = f64::NEG_INFINITY;
    let mut amount_start = f64::INFINITY;
    let mut numeric_rows = 0;
    for row in 0..n(&table["rows"]) {
        let cells = array(&table["cells"])
            .iter()
            .filter(|c| n(&c["row"]) == row)
            .collect::<Vec<_>>();
        assert_eq!(cells.len(), 2);
        for (col, c) in cells.iter().enumerate() {
            let ids = array(&c["words"]).iter().map(n).collect::<Vec<_>>();
            let ps = ids
                .iter()
                .map(|i| proof.get(i).expect("own actual original Glyph proof"))
                .collect::<Vec<_>>();
            assert!(
                ps.windows(2).all(|p| p[0].baseline == p[1].baseline),
                "exact baseline inside original cell"
            );
            for p in ps.windows(2) {
                if p[0].start == p[1].start {
                    assert_eq!(col, 0);
                    assert_eq!(
                        [p[0].start, p[0].end, p[0].baseline],
                        [p[1].start, p[1].end, p[1].baseline]
                    );
                    assert!(
                        p[0].after <= p[1].first,
                        "ordered disjoint coincident label glyphs"
                    );
                } else {
                    assert!(p[0].start < p[1].start);
                }
            }
        }
        label_end = label_end.max(
            array(&cells[0]["words"])
                .iter()
                .map(|i| f(&ws[n(i)]["x1"]))
                .reduce(f64::max)
                .unwrap(),
        );
        if let Some(last) = array(&cells[1]["words"]).last() {
            let p = &proof[&n(last)];
            lo = lo.max(p.last);
            hi = hi.min(p.end);
            numeric_rows += 1;
            amount_start = amount_start.min(
                array(&cells[1]["words"])
                    .iter()
                    .map(|i| f(&ws[n(i)]["x0"]))
                    .reduce(f64::min)
                    .unwrap(),
            );
        }
    }
    assert!(
        numeric_rows >= 2 && lo < hi,
        "positive common actual LAST-Glyph advance band"
    );
    assert!(
        label_end < f(&table["cells"][0]["x1"]) && f(&table["cells"][0]["x1"]) < amount_start,
        "common empty source gutter"
    );
}
pub(crate) fn reviewed_terminal_projection(
    old: &Value,
    new: &Value,
    rules: &[Value],
    xhtml: &[String],
    originals: &[Value],
) -> usize {
    assert_eq!(array(&old["pages"]).len(), originals.len());
    assert_eq!(xhtml.len(), originals.len());
    let proofs = array(&new["pages"])
        .iter()
        .zip(originals)
        .zip(xhtml)
        .map(|((p, o), x)| original_proofs(p, o, x))
        .collect::<Vec<_>>();
    reviewed_projection_with_min_columns(old, new, rules, 2, |pi, before, _after, table| {
        certify(before, table, &rules[pi], &proofs[pi])
    })
}
