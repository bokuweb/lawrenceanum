//! Source/material contracts for reviewed complete physical amount columns.
//! Only certified blank column slots can add separators; old anchors stay fixed.
use serde_json::{json, Map, Value};
fn array(v: &Value) -> &Vec<Value> {
    v.as_array().unwrap()
}
fn n(v: &Value) -> usize {
    v.as_u64().unwrap() as usize
}
fn f(v: &Value) -> f64 {
    v.as_f64().unwrap()
}
fn s(v: &Value) -> &str {
    v.as_str().unwrap()
}
fn compact(v: &str) -> String {
    v.chars().filter(|c| !c.is_whitespace()).collect()
}
fn numeric(v: &str) -> bool {
    let v = v.strip_prefix(['△', '▲', '−', '－', '+', '-']).unwrap_or(v);
    let mut parts = v.split('.');
    let whole = parts.next().unwrap();
    if parts
        .next()
        .is_some_and(|v| v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()))
        || parts.next().is_some()
    {
        return false;
    }
    let groups = whole.split(',').collect::<Vec<_>>();
    groups
        .iter()
        .all(|g| !g.is_empty() && g.bytes().all(|b| b.is_ascii_digit()))
        && (groups.len() == 1 || (groups[0].len() <= 3 && groups[1..].iter().all(|g| g.len() == 3)))
}
fn ids(l: &Value) -> Vec<usize> {
    array(&l["words"])
        .iter()
        .chain(
            l.get("table")
                .into_iter()
                .flat_map(|t| array(&t["cells"]))
                .flat_map(|c| array(&c["words"])),
        )
        .map(n)
        .collect()
}
fn owners(p: &Value) -> Vec<usize> {
    array(&p["regions"])
        .iter()
        .flat_map(|r| array(&r["lines"]))
        .flat_map(ids)
        .collect()
}
fn except(v: &Value, keys: &[&str]) -> Map<String, Value> {
    v.as_object()
        .unwrap()
        .iter()
        .filter(|(k, _)| !keys.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}
#[derive(Debug, PartialEq)]
enum Token {
    Char(char, Map<String, Value>),
    Row(Value),
}
fn row_signature(row: &Value, merges: Value, style: Value) -> Value {
    json!({"cols":array(row).len(),"cells":row,"merges":merges,"outer_style":style})
}
fn tokens(doc: &Value) -> Vec<Token> {
    fn runs(rs: &Value, out: &mut Vec<Token>) {
        for run in array(rs) {
            let style = except(run, &["text", "table"]);
            out.extend(
                run.get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .chars()
                    .filter(|c| *c != '\n')
                    .map(|c| Token::Char(c, style.clone())),
            );
            if let Some(t) = run.get("table") {
                for (i, row) in array(&t["rows"]).iter().enumerate() {
                    let merges = t
                        .get("merges")
                        .into_iter()
                        .flat_map(array)
                        .filter(|m| n(&m["row"]) == i)
                        .map(|m| Value::Object(except(m, &["row"])))
                        .collect::<Vec<_>>();
                    out.push(Token::Row(row_signature(row, json!(merges), json!(style))));
                }
            }
        }
    }
    let mut out = vec![];
    for b in array(&doc["blocks"]) {
        match s(&b["kind"]) {
            "paragraph" => runs(&b["runs"], &mut out),
            "shinkyu" => {
                for row in array(&b["rows"]) {
                    runs(&row["after"], &mut out);
                    runs(&row["before"], &mut out);
                }
            }
            other => panic!("unknown block {other}"),
        }
    }
    out
}
fn dense(sig: &Value) -> Vec<Token> {
    let mut out = vec![];
    for (i, cell) in array(&sig["cells"])
        .iter()
        .filter(|c| !array(c).is_empty())
        .enumerate()
    {
        if i > 0 {
            out.push(Token::Char('\u{3000}', Map::new()));
        }
        for r in array(cell) {
            assert_eq!(
                r.as_object().unwrap().len(),
                1,
                "exact plain matrix source payload"
            );
            out.extend(
                s(&r["text"])
                    .chars()
                    .filter(|c| *c != '\n')
                    .map(|c| Token::Char(c, Map::new())),
            );
        }
    }
    out
}
fn row_text(sig: &Value, slots: bool) -> String {
    let cells = array(&sig["cells"])
        .iter()
        .filter(|c| slots || !array(c).is_empty())
        .map(|c| array(c).iter().map(|r| s(&r["text"])).collect::<String>())
        .collect::<Vec<_>>();
    cells.join("\u{3000}").trim_end().to_owned()
}
fn notice(v: &Value, pages: &Value) -> Value {
    match v {
        Value::Array(xs) => Value::Array(xs.iter().map(|x| notice(x, pages)).collect()),
        Value::Object(m) => {
            let mut out = Map::new();
            for (k, v) in m {
                if k == "source"
                    && v.as_array().is_some_and(|xs| {
                        xs.iter().all(|x| {
                            x.as_object().is_some_and(|m| {
                                m.len() == 4
                                    && ["page", "region", "start_line", "end_line"]
                                        .iter()
                                        .all(|k| m.contains_key(*k))
                            })
                        })
                    })
                {
                    let mut stream = vec![];
                    for span in array(v) {
                        let (pi, ri, lo, hi) = (
                            n(&span["page"]),
                            n(&span["region"]),
                            n(&span["start_line"]),
                            n(&span["end_line"]),
                        );
                        let ls = array(&pages[pi]["regions"][ri]["lines"]);
                        assert!(lo <= hi && hi <= ls.len());
                        for l in &ls[lo..hi] {
                            let owned = ids(l);
                            if owned.is_empty() {
                                stream.push(json!(["opaque", pi, ri, l]));
                            } else {
                                stream.extend(owned.iter().map(|id| json!(["word", pi, id])));
                            }
                        }
                    }
                    out.insert(k.clone(), json!(stream));
                } else {
                    out.insert(k.clone(), notice(v, pages));
                }
            }
            Value::Object(out)
        }
        _ => v.clone(),
    }
}
#[derive(Clone)]
struct Frame {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    top: bool,
    bottom: bool,
}
fn native_frames(rules: &Value) -> Vec<Frame> {
    // Independent sorted interval sweep over original PDF strokes. A shorter
    // overlapping source path cannot become an alternative parent frame.
    let mut columns: Vec<(f64, Vec<(f64, f64)>)> = vec![];
    for v in array(&rules["vertical"]) {
        let (x, lo, hi) = (f(&v["x"]), f(&v["y0"]), f(&v["y1"]));
        assert!([x, lo, hi].iter().all(|v| v.is_finite()) && lo < hi);
        if let Some((_, ys)) = columns.iter_mut().find(|(xx, _)| *xx == x) {
            ys.push((lo, hi));
        } else {
            columns.push((x, vec![(lo, hi)]));
        }
    }
    let mut strokes = vec![];
    for (x, mut ys) in columns {
        ys.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        let mut merged: Vec<(f64, f64)> = vec![];
        for (lo, hi) in ys {
            if let Some(last) = merged.last_mut().filter(|last| lo <= last.1) {
                last.1 = last.1.max(hi);
            } else {
                merged.push((lo, hi));
            }
        }
        strokes.extend(merged.into_iter().map(|(lo, hi)| (x, lo, hi)));
    }
    let mut out = vec![];
    for &(left, lo, hi) in &strokes {
        for &(right, rlo, rhi) in &strokes {
            if left >= right || (lo, hi) != (rlo, rhi) {
                continue;
            }
            let wall = |y| {
                array(&rules["horizontal"])
                    .iter()
                    .any(|h| f(&h["y"]) == y && f(&h["x0"]) == left && f(&h["x1"]) == right)
            };
            let (top, bottom) = (wall(lo), wall(hi));
            if !(top || bottom)
                || strokes
                    .iter()
                    .any(|&(x, a, b)| left < x && x < right && a < hi && lo < b)
            {
                continue;
            }
            out.push(Frame {
                x0: left,
                y0: lo,
                x1: right,
                y1: hi,
                top,
                bottom,
            });
        }
    }
    out
}
fn native_parent(rules: &Value, page: &Value, t: &Value) {
    let mut owned = array(&t["cells"])
        .iter()
        .flat_map(|c| array(&c["words"]))
        .map(n)
        .collect::<Vec<_>>();
    owned.sort_unstable();
    let matches = native_frames(rules)
        .into_iter()
        .filter(|r| {
            r.top
                && r.bottom
                && r.x0 == f(&t["x0"])
                && r.x1 == f(&t["x1"])
                && r.y0 < f(&t["y1"])
                && f(&t["y0"]) < r.y1
                && (!r.top || f(&t["y0"]) >= r.y0)
                && (!r.bottom || f(&t["y1"]) <= r.y1)
        })
        .filter(|r| {
            let mut expected = array(&page["words"])
                .iter()
                .filter(|w| {
                    s(&w["status"]) == "used"
                        && f(&w["x0"]) < r.x1
                        && r.x0 < f(&w["x1"])
                        && f(&w["y0"]) < r.y1
                        && r.y0 < f(&w["y1"])
                })
                .map(|w| n(&w["id"]))
                .collect::<Vec<_>>();
            expected.sort_unstable();
            expected == owned
        })
        .count();
    assert_eq!(
        matches, 1,
        "one maximal original parent and every used word; not an easy partial fragment"
    );
}
fn project(text: &str, changes: &[(String, String)]) -> String {
    let mut out = text.to_owned();
    for (new, old) in changes {
        if new != old {
            assert!(out.contains(new), "complete certified source block");
            out = out.replacen(new, old, 1);
        }
    }
    out
}

pub(crate) fn reviewed_projection(old: &Value, new: &Value, rules: &[Value]) -> usize {
    reviewed_projection_with_parent(old, new, rules, |pi, _, page, table| {
        native_parent(&rules[pi], page, table)
    })
}
fn reviewed_projection_with_parent(
    old: &Value,
    new: &Value,
    rules: &[Value],
    parent: impl FnMut(usize, &Value, &Value, &Value),
) -> usize {
    reviewed_projection_with_min_columns(old, new, rules, 3, parent)
}
fn reviewed_projection_with_min_columns(
    old: &Value,
    new: &Value,
    rules: &[Value],
    min_columns: usize,
    mut parent: impl FnMut(usize, &Value, &Value, &Value),
) -> usize {
    assert!(min_columns >= 2);
    assert_eq!(old["format"], new["format"]);
    assert_eq!(array(&old["pages"]).len(), array(&new["pages"]).len());
    assert_eq!(array(&old["pages"]).len(), rules.len());
    assert_eq!(array(&old["articles"]).len(), array(&new["articles"]).len());
    let mut approved: Vec<(Value, usize)> = vec![];
    let mut all_changes = vec![];
    let mut promoted = 0;
    for (pi, (a, b)) in array(&old["pages"])
        .iter()
        .zip(array(&new["pages"]))
        .enumerate()
    {
        assert_eq!(
            except(a, &["regions", "text"]),
            except(b, &["regions", "text"]),
            "all original words/raw/ID/status/coordinates and page metadata"
        );
        assert_eq!(
            owners(a),
            owners(b),
            "all source owners and their order/multiplicity"
        );
        assert_eq!(array(&a["regions"]).len(), array(&b["regions"]).len());
        let mut changes = vec![];
        for (ar, br) in array(&a["regions"]).iter().zip(array(&b["regions"])) {
            assert_eq!(except(ar, &["lines"]), except(br, &["lines"]));
            let before = array(&ar["lines"]);
            let mut at = 0;
            for l in array(&br["lines"]) {
                if before.get(at) == Some(l) {
                    at += 1;
                    continue;
                }
                let t = l
                    .get("table")
                    .expect("only complete primitive rows can change");
                let (rows, cols) = (n(&t["rows"]), n(&t["cols"]));
                assert!(rows > 0 && cols >= min_columns && at + rows <= before.len());
                assert!(array(&l["words"]).is_empty());
                assert_eq!(array(&t["cells"]).len(), rows * cols);
                let original = &before[at..at + rows];
                assert!(original.iter().all(|l| l.get("table").is_none()));
                let mut rendered = vec![];
                let mut dense_text = vec![];
                let mut edges: Option<Vec<f64>> = None;
                let mut last_bottom = None;
                for (row, ol) in original.iter().enumerate() {
                    let cs = array(&t["cells"])
                        .iter()
                        .filter(|c| n(&c["row"]) == row)
                        .collect::<Vec<_>>();
                    assert_eq!(cs.len(), cols);
                    assert_eq!(
                        cs.iter().map(|c| n(&c["col"])).collect::<Vec<_>>(),
                        (0..cols).collect::<Vec<_>>()
                    );
                    assert!(!s(&cs[0]["text"]).is_empty());
                    let payload = cs
                        .iter()
                        .filter(|c| !s(&c["text"]).is_empty())
                        .map(|c| s(&c["text"]))
                        .collect::<Vec<_>>()
                        .join("\u{3000}");
                    assert_eq!(
                        payload,
                        s(&ol["text"]),
                        "exact cell payload and every original ASCII/fullwidth gap"
                    );
                    assert_eq!(
                        cs.iter()
                            .flat_map(|c| array(&c["words"]))
                            .collect::<Vec<_>>(),
                        array(&ol["words"]).iter().collect::<Vec<_>>()
                    );
                    for c in &cs {
                        assert_eq!(n(&c["rowspan"]), 1);
                        assert_eq!(n(&c["colspan"]), 1);
                        assert_eq!(s(&c["text"]).is_empty(), array(&c["words"]).is_empty());
                        if n(&c["col"]) > 0 && !s(&c["text"]).is_empty() {
                            assert!(
                                numeric(s(&c["text"])),
                                "complete amount notation in its physical slot"
                            );
                        }
                        assert!(["x0", "x1", "y0", "y1"]
                            .iter()
                            .all(|k| f(&c[*k]).is_finite()));
                        assert!(f(&c["x0"]) < f(&c["x1"]) && f(&c["y0"]) < f(&c["y1"]));
                        assert!(
                            f(&t["x0"]) <= f(&c["x0"])
                                && f(&c["x1"]) <= f(&t["x1"])
                                && f(&t["y0"]) <= f(&c["y0"])
                                && f(&c["y1"]) <= f(&t["y1"])
                        );
                        let text = array(&c["words"])
                            .iter()
                            .map(|id| s(&b["words"][n(id)]["text"]))
                            .collect::<String>();
                        assert_eq!(compact(&text), compact(s(&c["text"])));
                        for id in array(&c["words"]) {
                            let w = &b["words"][n(id)];
                            assert!(
                                f(&c["x0"]) <= f(&w["x0"])
                                    && f(&w["x1"]) <= f(&c["x1"])
                                    && f(&c["y0"]) <= f(&w["y0"])
                                    && f(&w["y1"]) <= f(&c["y1"]),
                                "own source word rectangle in its cell"
                            );
                        }
                    }
                    assert!(cs.windows(2).all(|p| p[0]["x1"] == p[1]["x0"]));
                    let e = std::iter::once(f(&cs[0]["x0"]))
                        .chain(cs.iter().map(|c| f(&c["x1"])))
                        .collect::<Vec<_>>();
                    if let Some(old) = &edges {
                        assert_eq!(*old, e);
                    } else {
                        edges = Some(e);
                    }
                    assert!(cs
                        .iter()
                        .all(|c| c["y0"] == cs[0]["y0"] && c["y1"] == cs[0]["y1"]));
                    if let Some(y) = last_bottom {
                        assert_eq!(y, f(&cs[0]["y0"]));
                    }
                    last_bottom = Some(f(&cs[0]["y1"]));
                    let nested = Value::Array(
                        cs.iter()
                            .map(|c| {
                                if s(&c["text"]).is_empty() {
                                    json!([])
                                } else {
                                    json!([{"text":c["text"]}])
                                }
                            })
                            .collect(),
                    );
                    let sig = row_signature(&nested, json!([]), json!({}));
                    if let Some((_, count)) = approved.iter_mut().find(|(v, _)| *v == sig) {
                        *count += 1;
                    } else {
                        approved.push((sig, 1));
                    }
                    rendered.push(
                        cs.iter()
                            .map(|c| s(&c["text"]))
                            .collect::<Vec<_>>()
                            .join("\u{3000}")
                            .trim_end()
                            .to_owned(),
                    );
                    dense_text.push(payload);
                }
                assert_eq!(s(&l["text"]), rendered.join("\n"));
                parent(pi, a, b, t);
                changes.push((s(&l["text"]).to_owned(), dense_text.join("\n")));
                at += rows;
                promoted += 1;
            }
            assert_eq!(at, before.len());
        }
        assert_eq!(project(s(&b["text"]), &changes), s(&a["text"]));
        all_changes.extend(changes);
    }
    assert_eq!(project(s(&new["text"]), &all_changes), s(&old["text"]));
    for (a, b) in array(&old["articles"]).iter().zip(array(&new["articles"])) {
        assert_eq!(
            except(a, &["text", "document", "notices"]),
            except(b, &["text", "document", "notices"])
        );
        assert_eq!(
            except(&a["document"], &["blocks"]),
            except(&b["document"], &["blocks"])
        );
        assert_eq!(
            array(&a["document"]["blocks"])
                .iter()
                .filter(|b| b["kind"] == "shinkyu")
                .collect::<Vec<_>>(),
            array(&b["document"]["blocks"])
                .iter()
                .filter(|b| b["kind"] == "shinkyu")
                .collect::<Vec<_>>()
        );
        let before = tokens(&a["document"]);
        let after = tokens(&b["document"]);
        let mut at = 0;
        let mut changes = vec![];
        for tok in after {
            if before.get(at) == Some(&tok) {
                at += 1;
                continue;
            }
            let Token::Row(sig) = tok else {
                panic!("unexplained document character/style change")
            };
            let (_, remaining) = approved
                .iter_mut()
                .find(|(v, count)| *v == sig && *count > 0)
                .expect("one original certified matrix row");
            let row = dense(&sig);
            assert_eq!(
                before.get(at..at + row.len()),
                Some(row.as_slice()),
                "all document characters/styles/ASCII/U3000; LF excluded"
            );
            at += row.len();
            *remaining -= 1;
            changes.push((row_text(&sig, true), row_text(&sig, false)));
        }
        assert_eq!(
            at,
            before.len(),
            "all old opaque table rows/cells/merges and styled text retained in order"
        );
        assert_eq!(project(s(&b["text"]), &changes), s(&a["text"]));
        let empty = json!([]);
        assert_eq!(
            notice(a.get("notices").unwrap_or(&empty), &old["pages"]),
            notice(b.get("notices").unwrap_or(&empty), &new["pages"])
        );
    }
    assert!(
        approved.iter().all(|(_, count)| *count == 0),
        "all source rows appear in their own structured documents"
    );
    promoted
}

#[path = "native_numeric_parent_contracts.rs"]
mod parent_contracts;
#[allow(unused_imports)]
pub(crate) use parent_contracts::reviewed_parent_projection;

// This negative control deliberately verifies material alone before asserting
// that the complete original parent rejects the coordinated shape corruption.
#[allow(dead_code)]
pub(crate) fn material_projection_for_counterexample(
    old: &Value,
    new: &Value,
    rules: &[Value],
) -> usize {
    reviewed_projection_with_parent(old, new, rules, |_, _, _, _| {})
}

#[path = "native_numeric_lexical_contracts.rs"]
mod lexical_contracts;
#[allow(unused_imports)]
pub(crate) use lexical_contracts::reviewed_lexical_projection;

// Deliberately omits authority only for the coordinated-corruption control.
#[allow(dead_code)]
pub(crate) fn lexical_material_projection_for_counterexample(
    old: &Value,
    new: &Value,
    rules: &[Value],
) -> usize {
    reviewed_projection_with_min_columns(old, new, rules, 2, |_, _, _, _| {})
}

#[path = "native_numeric_terminal_contracts.rs"]
mod terminal_contracts;
pub(crate) use terminal_contracts::reviewed_terminal_projection;
