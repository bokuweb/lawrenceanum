//! Source marker identities survive Document splicing. A recovered Table can be
//! a column fragment, so its existence alone does not prove an independent root.
use crate::{
    lines::PageRules,
    trace::{PageTrace, Table},
};
use std::collections::HashSet;
pub(crate) struct Scopes {
    tables: Vec<Table>,
    separate: HashSet<(usize, usize)>,
}
impl Scopes {
    pub(crate) fn for_pages(pages: &[PageTrace], rules: &[Option<PageRules>]) -> Self {
        let mut out = Self {
            tables: vec![],
            separate: HashSet::new(),
        };
        for (pi, p) in pages.iter().enumerate() {
            let offset = out.tables.len();
            out.tables.extend(p.tables.iter().cloned());
            let Some(r) = rules.get(pi).and_then(Option::as_ref) else {
                continue;
            };
            let Some(vs) = vertical_supports(r) else {
                continue;
            };
            for (a, ta) in p.tables.iter().enumerate() {
                for (b, tb) in p.tables.iter().enumerate().skip(a + 1) {
                    if independent_columns(ta, tb, p, r, &vs) {
                        out.separate.insert((offset + a, offset + b));
                        out.separate.insert((offset + b, offset + a));
                    }
                }
            }
        }
        out
    }
    pub(crate) fn tables(&self) -> &[Table] {
        &self.tables
    }
    pub(crate) fn may_join(&self, a: usize, b: usize) -> bool {
        a != b && a < self.tables.len() && b < self.tables.len() && !self.separate.contains(&(a, b))
    }
}
// Join only touching/overlapping pieces of the same original vertical support.
// Neither rounded table boxes nor a distance tolerance closes a physical gap.
fn vertical_supports(r: &PageRules) -> Option<Vec<crate::lines::VRule>> {
    if r.vertical
        .iter()
        .any(|v| ![v.x, v.y0, v.y1].iter().all(|x| x.is_finite()) || v.y0 >= v.y1)
        || r.horizontal
            .iter()
            .any(|h| ![h.y, h.x0, h.x1].iter().all(|x| x.is_finite()) || h.x0 >= h.x1)
    {
        return None;
    }
    let mut vs = r.vertical.clone();
    vs.sort_by(|a, b| {
        a.x.total_cmp(&b.x)
            .then(a.y0.total_cmp(&b.y0))
            .then(a.y1.total_cmp(&b.y1))
    });
    let mut out: Vec<crate::lines::VRule> = vec![];
    for v in vs {
        if let Some(last) = out.last_mut() {
            if last.x == v.x && v.y0 <= last.y1 {
                last.y1 = last.y1.max(v.y1);
                continue;
            }
        }
        out.push(v);
    }
    Some(out)
}
fn crossbar(r: &PageRules, y: f32, left: f32, right: f32) -> bool {
    let mut hs = r
        .horizontal
        .iter()
        .filter(|h| h.y == y)
        .copied()
        .collect::<Vec<_>>();
    hs.sort_by(|a, b| a.x0.total_cmp(&b.x0));
    let mut end = left;
    for h in hs {
        if h.x0 <= end && end < h.x1 {
            end = h.x1;
        }
    }
    right <= end
}
fn valid_table(t: &Table) -> bool {
    [t.x0, t.y0, t.x1, t.y1].iter().all(|x| x.is_finite())
        && t.x0 < t.x1
        && t.y0 < t.y1
        && t.rows > 0
        && t.cols > 0
}
fn independent_columns(
    a: &Table,
    b: &Table,
    p: &PageTrace,
    r: &PageRules,
    vs: &[crate::lines::VRule],
) -> bool {
    if !valid_table(a) || !valid_table(b) {
        return false;
    }
    let (a, b) = if a.x1 < b.x0 {
        (a, b)
    } else if b.x1 < a.x0 {
        (b, a)
    } else {
        return false;
    };
    let top = a.y0.min(b.y0);
    let bottom = a.y1.max(b.y1);
    let ids = a
        .cells
        .iter()
        .flat_map(|c| c.words.iter())
        .collect::<HashSet<_>>();
    if b.cells
        .iter()
        .flat_map(|c| c.words.iter())
        .any(|id| ids.contains(id))
    {
        return false;
    }
    for divider in vs
        .iter()
        .filter(|v| a.x1 < v.x && v.x < b.x0 && v.y0 <= top && bottom <= v.y1)
    {
        // A divider with two strict enclosing walls creates distinct parent columns.
        // A full-width original crossbar meeting continuing outer walls supplies
        // an internal outer-grid row. A bottom border at the common newspaper
        // frontier does not distinguish reading columns from table cells. With an
        // open page-clipped parent, two distinct inner walls must instead bound
        // disjoint sibling frames, and both child tables must occupy exactly
        // those complete original vertical intervals. A shared newspaper-column
        // gutter/end frontier alone supplies no independent-table authority.
        let closed = vs
            .iter()
            .filter(|l| l.x < a.x0 && l.y0 == divider.y0 && divider.y1 <= l.y1)
            .any(|l| {
                vs.iter()
                    .filter(|rr| b.x1 < rr.x && rr.y0 == divider.y0 && divider.y1 <= rr.y1)
                    .any(|rr| {
                        (divider.y1 < l.y1
                            && divider.y1 < rr.y1
                            && crossbar(r, divider.y1, l.x, rr.x))
                            || (l.y1 == divider.y1
                                && rr.y1 == divider.y1
                                && a.y0 == divider.y0
                                && b.y0 == divider.y0
                                && a.y1 == divider.y1
                                && b.y1 == divider.y1
                                && vs.iter().any(|other| {
                                    a.x1 < other.x
                                        && other.x < b.x0
                                        && other.x != divider.x
                                        && other.y0 == divider.y0
                                        && other.y1 == divider.y1
                                }))
                    })
            });
        if !closed {
            continue;
        }
        if p.words
            .iter()
            .filter(|w| w.status == crate::trace::WordStatus::Used)
            .any(|w| {
                ![w.x0, w.y0, w.x1, w.y1].iter().all(|x| x.is_finite())
                    || w.x0 >= w.x1
                    || w.y0 >= w.y1
                    || (w.y0 < divider.y1
                        && divider.y0 < w.y1
                        && w.x0 < divider.x
                        && divider.x < w.x1)
            })
        {
            continue;
        }
        return true;
    }
    false
}
#[cfg(test)]
#[path = "native_table_scopes_tests158.rs"]
mod tests;
#[cfg(test)]
pub(crate) fn test_page(
    tables: Vec<Table>,
    words: Vec<crate::trace::Word>,
    baselines: Vec<Option<f64>>,
) -> PageTrace {
    PageTrace {
        native_comparison_labels: vec![],
        native_horizontal_baselines: baselines,
        native_atomic_columns: vec![],
        native_styles: vec![],
        native_bounds: vec![],
        native_content_styles: vec![],
        native_axes: vec![],
        native_rulings: vec![],
        width: 900.,
        height: 900.,
        layout: "horizontal",
        words,
        regions: vec![],
        text: String::new(),
        doc_text: String::new(),
        tables,
    }
}
