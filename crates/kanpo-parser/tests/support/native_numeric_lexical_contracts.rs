//! Source authority for a terminal amount in a complete original frame inventory.
//! The frame can contain interleaved nonnumeric section headings; none is dropped.
use super::*;
use std::collections::BTreeSet;

fn touches(w: &Value, r: &Frame) -> bool {
    f(&w["x0"]) < r.x1 && r.x0 < f(&w["x1"]) && f(&w["y0"]) < r.y1 && r.y0 < f(&w["y1"])
}
fn certify(
    r: &Frame,
    page: &Value,
    table: &Value,
    ys: &std::collections::BTreeMap<usize, Option<f64>>,
) {
    assert_eq!(n(&table["cols"]), 2, "one complete terminal amount rail");
    let source = array(&page["words"]);
    let expected = source
        .iter()
        .filter(|w| s(&w["status"]) == "used" && touches(w, r))
        .map(|w| n(&w["id"]))
        .collect::<BTreeSet<_>>();
    let mut claimed = BTreeSet::new();
    let mut rail = None;
    let mut label_end = f64::NEG_INFINITY;
    let mut amount_start = f64::INFINITY;
    let mut numeric_rows = 0;
    let mut scope = None;
    let mut previous = None;
    let mut original_numeric = vec![];
    for (ri, region) in array(&page["regions"]).iter().enumerate() {
        for (li, line) in array(&region["lines"]).iter().enumerate() {
            let owned = ids(line);
            if !owned.iter().any(|id| expected.contains(id)) {
                continue;
            }
            assert!(
                line.get("table").is_none(),
                "existing source grids are authoritative"
            );
            assert_eq!(region["role"], "body");
            assert!(
                scope.is_none_or(|old| old == ri),
                "one original region/unit"
            );
            scope = Some(ri);
            assert!(
                previous.is_none_or(|old| old + 1 == li),
                "contiguous full source scope"
            );
            previous = Some(li);
            for id in &owned {
                let w = &source[*id];
                let y = ys[id].expect("own original horizontal flow, never a neighbor's metadata");
                assert!(
                    expected.contains(id) && claimed.insert(*id),
                    "one complete original owner"
                );
                assert!(r.x0 <= f(&w["x0"]) && f(&w["x1"]) <= r.x1);
                assert!(r.y0 <= y && y <= r.y1);
                assert!(!r.top || r.y0 <= f(&w["y0"]));
                assert!(!r.bottom || f(&w["y1"]) <= r.y1);
            }
            let text = s(&line["text"]);
            let amount_like = |x: &str| numeric(x) || matches!(x, "-" | "−" | "－");
            if !text.split('\u{3000}').any(amount_like)
                && !owned.iter().any(|id| amount_like(s(&source[*id]["text"])))
            {
                continue;
            }
            let (label, value) = text
                .rsplit_once('\u{3000}')
                .expect("complete label and amount fields");
            assert!(!label.trim().is_empty() && numeric(value));
            assert!(
                label
                    .split_whitespace()
                    .all(|field| !numeric(field) && !field.chars().all(char::is_numeric)),
                "a bare separate numeric field cannot hide another amount lane"
            );
            let split = (1..owned.len())
                .find(|&k| {
                    compact(
                        &owned[..k]
                            .iter()
                            .map(|id| s(&source[*id]["text"]))
                            .collect::<String>(),
                    ) == compact(label)
                })
                .expect("full source-label occurrence stream");
            assert_eq!(
                owned[split..]
                    .iter()
                    .map(|id| s(&source[*id]["text"]))
                    .collect::<String>(),
                value
            );
            let y = ys[&owned[0]].unwrap() as f32;
            assert!(owned.iter().all(|id| ys[id].unwrap() as f32 == y));
            let right = owned[split..]
                .iter()
                .map(|id| f(&source[*id]["x1"]))
                .reduce(f64::max)
                .unwrap();
            assert!(
                rail.is_none_or(|old| old == right),
                "whole original frame has one exact terminal rail"
            );
            rail = Some(right);
            label_end = label_end.max(
                owned[..split]
                    .iter()
                    .map(|id| f(&source[*id]["x1"]))
                    .reduce(f64::max)
                    .unwrap(),
            );
            amount_start = amount_start.min(
                owned[split..]
                    .iter()
                    .map(|id| f(&source[*id]["x0"]))
                    .reduce(f64::min)
                    .unwrap(),
            );
            numeric_rows += 1;
            original_numeric.push((owned[..split].to_vec(), owned[split..].to_vec()));
        }
    }
    assert_eq!(
        claimed, expected,
        "all source rows, including every section heading"
    );
    assert!(
        numeric_rows >= 2 && label_end < amount_start,
        "whole-frame empty source gutter"
    );
    for row in 0..n(&table["rows"]) {
        let cells = array(&table["cells"])
            .iter()
            .filter(|c| n(&c["row"]) == row)
            .collect::<Vec<_>>();
        assert_eq!(cells.len(), 2);
        let pair = (
            array(&cells[0]["words"]).iter().map(n).collect::<Vec<_>>(),
            array(&cells[1]["words"]).iter().map(n).collect::<Vec<_>>(),
        );
        assert!(
            original_numeric.contains(&pair),
            "exact source row and its own amount partition"
        );
    }
}

pub(crate) fn reviewed_lexical_projection(
    old: &Value,
    new: &Value,
    rules: &[Value],
    xhtml: &[String],
) -> usize {
    assert_eq!(array(&old["pages"]).len(), xhtml.len());
    reviewed_projection_with_min_columns(old, new, rules, 2, |pi, before, after, table| {
        let ys = super::parent_contracts::source_baselines(&xhtml[pi], after);
        let mut frames = native_frames(&rules[pi])
            .into_iter()
            .filter(|r| {
                r.x0 == f(&table["x0"])
                    && r.x1 == f(&table["x1"])
                    && r.y0 < f(&table["y1"])
                    && f(&table["y0"]) < r.y1
                    && (!r.top || r.y0 <= f(&table["y0"]))
                    && (!r.bottom || f(&table["y1"]) <= r.y1)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            frames.len(),
            1,
            "one original capped source frame, no invented edge"
        );
        certify(&frames.remove(0), before, table, &ys);
    })
}
