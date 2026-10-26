use crate::vertical::Col;
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct ComparisonLabels {
    pub after: &'static str,
    pub before: &'static str,
}
impl ComparisonLabels {
    pub(crate) const AMENDMENT: Self = Self {
        after: "改正後",
        before: "改正前",
    };
    pub(crate) const CHANGE: Self = Self {
        after: "変更後",
        before: "変更前",
    };
    pub(crate) fn from_before(s: &str) -> Option<Self> {
        [Self::AMENDMENT, Self::CHANGE]
            .into_iter()
            .find(|p| p.before == s)
    }
    pub(crate) fn from_after(s: &str) -> Option<Self> {
        [Self::AMENDMENT, Self::CHANGE]
            .into_iter()
            .find(|p| p.after == s)
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Header {
    pub(crate) x: f32,
    pub(crate) divider: f32,
    pub(crate) labels: ComparisonLabels,
    pub(crate) words: Vec<usize>,
}

fn valid(c: &Col) -> bool {
    [c.x, c.x1, c.y, c.y1].iter().all(|v| v.is_finite())
        && c.x < c.x1
        && c.y < c.y1
        && !c.text.is_empty()
}
#[derive(Clone)]
struct Lexeme {
    indices: Vec<usize>,
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
}
fn lexemes(cols: &[Col], surface: &str) -> Vec<Lexeme> {
    fn extend(cols: &[Col], remaining: &str, current: Option<Lexeme>, out: &mut Vec<Lexeme>) {
        if remaining.is_empty() {
            out.push(current.unwrap());
            return;
        }
        for (i, c) in cols.iter().enumerate() {
            if !valid(c) || !remaining.starts_with(&c.text) {
                continue;
            }
            let next = match &current {
                None => Lexeme {
                    indices: vec![i],
                    left: c.x,
                    right: c.x1,
                    top: c.y,
                    bottom: c.y1,
                },
                Some(p) => {
                    let left = p.left.max(c.x);
                    let right = p.right.min(c.x1);
                    if c.y < p.bottom || left >= right {
                        continue;
                    }
                    let mut indices = p.indices.clone();
                    indices.push(i);
                    Lexeme {
                        indices,
                        left,
                        right,
                        top: p.top,
                        bottom: c.y1,
                    }
                }
            };
            extend(cols, &remaining[c.text.len()..], Some(next), out);
        }
    }
    let mut out = vec![];
    extend(cols, surface, None, &mut out);
    out
}
pub(crate) fn headers(cols: &[Col]) -> Vec<Header> {
    let mut out = vec![];
    for labels in [ComparisonLabels::AMENDMENT, ComparisonLabels::CHANGE] {
        for after in lexemes(cols, labels.after) {
            for before in lexemes(cols, labels.before) {
                let left = after.left.max(before.left);
                let right = after.right.min(before.right);
                if left >= right || after.bottom > before.top {
                    continue;
                }
                let mut indices = after.indices.clone();
                indices.extend(&before.indices);
                let words: Vec<_> = indices.iter().map(|&i| cols[i].id).collect();
                let unique: std::collections::HashSet<_> = words.iter().copied().collect();
                if unique.len() != words.len() {
                    continue;
                }
                // A label column is a complete source witness: no intervening native
                // word may be skipped to manufacture a matching pair.
                if cols.iter().enumerate().any(|(i, c)| {
                    !indices.contains(&i)
                        && c.x < right
                        && c.x1 > left
                        && c.y < before.bottom
                        && c.y1 > after.top
                }) {
                    continue;
                }
                out.push(Header {
                    x: indices
                        .iter()
                        .map(|&i| cols[i].x)
                        .fold(f32::INFINITY, f32::min),
                    divider: ((after.top + after.bottom) * 0.5
                        + (before.top + before.bottom) * 0.5)
                        * 0.5,
                    labels,
                    words,
                });
            }
        }
    }
    out.sort_by(|a, b| b.x.total_cmp(&a.x).then(a.divider.total_cmp(&b.divider)));
    out.dedup_by(|a, b| a.words == b.words);
    out
}

#[cfg(test)]
#[path = "comparison_labels_tests147.rs"]
mod tests;

/// Closed native frames containing every word of a confirmed label pair.
/// If a field has no such frame, it cannot certify the independence of other
/// tables and this proof declines the whole collection.
pub(crate) fn field_frames<'a>(
    cols: &[Col],
    grids: &'a [crate::grid::Grid],
) -> Option<Vec<&'a crate::grid::Grid>> {
    let hs = headers(cols);
    if hs.is_empty() {
        return None;
    }
    hs.into_iter()
        .map(|h| {
            let own: Vec<_> = cols.iter().filter(|c| h.words.contains(&c.id)).collect();
            let mut candidates: Vec<_> = grids
                .iter()
                .filter(|g| {
                    let inside = |c: &&Col| g.contains((c.x + c.x1) * 0.5, (c.y + c.y1) * 0.5, 0.);
                    if !own.iter().all(inside) {
                        return false;
                    }
                    // A caption strip is not the comparison field. Its frame must also
                    // enclose native body columns on the printed side in both roles.
                    let body = |c: &&Col| !h.words.contains(&c.id) && c.x1 <= h.x && inside(c);
                    cols.iter().filter(body).any(|c| c.y1 <= h.divider)
                        && cols.iter().filter(body).any(|c| c.y >= h.divider)
                        && !cols
                            .iter()
                            .filter(inside)
                            .any(|c| crate::segment::is_heading_mark(&c.text))
                })
                .collect();
            candidates.sort_by(|a, b| {
                ((a.right() - a.left()) * (a.bottom() - a.top()))
                    .total_cmp(&((b.right() - b.left()) * (b.bottom() - b.top())))
            });
            let first = *candidates.first()?;
            // Crossing frames do not establish a unique owner.
            if candidates.iter().skip(1).any(|g| {
                !(g.left() <= first.left()
                    && first.right() <= g.right()
                    && g.top() <= first.top()
                    && first.bottom() <= g.bottom())
            }) {
                return None;
            }
            Some(first)
        })
        .collect()
}

/// Only an already-certified initial comparison continuation can inherit a
/// family; a source label, preamble or independent body is a scope boundary.
pub(crate) fn inherit(
    units: &mut [crate::trace::Unit],
    previous: Option<ComparisonLabels>,
    explicit: bool,
) {
    if explicit || previous.is_none() {
        return;
    }
    if let Some(crate::trace::Unit::Shinkyu {
        preamble, labels, ..
    }) = units.first_mut()
    {
        if preamble.is_empty() && labels.is_none() {
            *labels = previous;
        }
    }
}

#[cfg(test)]
#[path = "comparison_label_scope_tests147.rs"]
mod scope_tests147;

/// A separate table needs positive source-frame disjointness. The complement of
/// containment would also admit parents and crossing frames, which can cut a
/// real comparison continuation into an unrelated body.
pub(crate) fn independent_table(
    (x0, x1, y0, y1): (f32, f32, f32, f32),
    frames: &[&crate::grid::Grid],
) -> bool {
    let valid = |a: f32, b: f32, c: f32, d: f32| {
        [a, b, c, d].iter().all(|v| v.is_finite()) && a < b && c < d
    };
    valid(x0, x1, y0, y1)
        && !frames.is_empty()
        && frames.iter().all(|g| {
            valid(g.left(), g.right(), g.top(), g.bottom())
                && (x1 <= g.left() || g.right() <= x0 || y1 <= g.top() || g.bottom() <= y0)
        })
}
#[cfg(test)]
#[path = "independent_comparison_table_tests147.rs"]
mod independent_table_tests147;

/// The marker-stream grammar is After-first within one explicit table scope.
/// Ordinary Before/After prose is not sufficient evidence of a paired field.
pub(crate) fn has_after_first_pair(text: &str) -> bool {
    let mut after = None;
    let mut leading_before = false;
    for line in text.lines().map(str::trim) {
        if line == crate::trace::SHINKYU_END {
            after = None;
            leading_before = false;
            continue;
        }
        if let Some(before) = ComparisonLabels::from_before(line) {
            if !leading_before && after == Some(before) {
                return true;
            }
            leading_before = true;
            after = None;
        } else if let Some(labels) = ComparisonLabels::from_after(line) {
            if !leading_before {
                after = Some(labels);
            }
        }
    }
    false
}
