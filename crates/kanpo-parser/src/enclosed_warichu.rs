//! A bracketed citation can temporarily occupy several adjacent native columns.
use super::{Col, PageRules};

fn brackets(text: &str, stack: &mut Vec<char>) -> bool {
    for ch in text.chars() {
        match ch {
            '（' => stack.push('）'),
            '(' => stack.push(')'),
            '[' => stack.push(']'),
            '［' => stack.push('］'),
            '「' => stack.push('」'),
            '『' => stack.push('』'),
            '【' => stack.push('】'),
            '）' | ')' | ']' | '］' | '」' | '』' | '】' => {
                if stack.pop() != Some(ch) {
                    return false;
                }
            }
            _ => {}
        }
    }
    true
}

pub(super) fn merge(cols: &mut Vec<Col>, rules: &PageRules) -> Vec<(usize, usize)> {
    let mut seen = std::collections::HashSet::new();
    if cols.iter().any(|c| {
        !seen.insert(c.id)
            || ![c.x, c.x1, c.y, c.y1].iter().all(|v| v.is_finite())
            || c.x1 <= c.x
            || c.y1 <= c.y
            || !(c.x1 - c.x).is_finite()
            || !(c.y1 - c.y).is_finite()
    }) || rules
        .vertical
        .iter()
        .any(|r| ![r.x, r.y0, r.y1].iter().all(|v| v.is_finite()) || r.y1 < r.y0)
        || rules
            .horizontal
            .iter()
            .any(|r| ![r.y, r.x0, r.x1].iter().all(|v| v.is_finite()) || r.x1 < r.x0)
        || rules
            .enclosures
            .iter()
            .any(|r| !r.iter().all(|v| v.is_finite()) || r[2] <= r[0] || r[3] <= r[1])
    {
        return vec![];
    }
    let mut proposals = vec![];
    for (a, above) in cols.iter().enumerate() {
        let w = above.x1 - above.x;
        // Relative tolerance absorbs rounding in the native coordinate stream.
        let eps = w * 0.01;
        let near = |x: f32, y: f32| (x - y).abs() <= eps;
        let mut opened = vec![];
        if !brackets(&above.text, &mut opened) || opened.is_empty() {
            continue;
        }
        for (b, below) in cols.iter().enumerate() {
            if a == b || !near(above.x, below.x) || !near(above.x1, below.x1) || below.y <= above.y1
            {
                continue;
            }
            let (top, bottom) = (above.y1, below.y);
            let mut closed = opened.clone();
            if !brackets(&below.text, &mut closed) || !closed.is_empty() {
                continue;
            }
            let mut members = vec![];
            let (mut left, mut right) = (above.x, above.x1);
            loop {
                let before = members.len();
                for (i, c) in cols.iter().enumerate() {
                    if i == a
                        || i == b
                        || members.contains(&i)
                        || c.y >= bottom - eps
                        || c.y1 <= top + eps
                    {
                        continue;
                    }
                    if c.x <= right + eps && c.x1 >= left - eps {
                        members.push(i);
                        left = left.min(c.x);
                        right = right.max(c.x1);
                    }
                }
                if members.len() == before {
                    break;
                }
            }
            let crosses_v = |x: f32, y0: f32, y1: f32| {
                x > left + eps && x < right - eps && y0 < bottom - eps && y1 > top + eps
            };
            let crosses_h = |y: f32, x0: f32, x1: f32| {
                y > top + eps && y < bottom - eps && x0 < right - eps && x1 > left + eps
            };
            if rules.vertical.iter().any(|r| crosses_v(r.x, r.y0, r.y1))
                || rules.horizontal.iter().any(|r| crosses_h(r.y, r.x0, r.x1))
                || rules.enclosures.iter().any(|&[x, y, x1, y1]| {
                    crosses_v(x, y, y1)
                        || crosses_v(x1, y, y1)
                        || crosses_h(y, x, x1)
                        || crosses_h(y1, x, x1)
                })
            {
                continue;
            }
            if members.is_empty() || !near((left + right) / 2., (above.x + above.x1) / 2.) {
                continue;
            }
            if members.iter().any(|&i| {
                let c = &cols[i];
                !near(c.x1 - c.x, w)
                    || c.y < top - eps
                    || c.y1 > bottom + eps
                    || c.text.is_empty()
                    || c.text.chars().any(|ch| {
                        ch == '\u{fffd}'
                            || ch.is_control()
                            || ('\u{e000}'..='\u{f8ff}').contains(&ch)
                            || ('\u{f0000}'..='\u{ffffd}').contains(&ch)
                            || ('\u{100000}'..='\u{10fffd}').contains(&ch)
                    })
            }) {
                continue;
            }
            members.sort_by(|&i, &j| {
                cols[j]
                    .x
                    .total_cmp(&cols[i].x)
                    .then(cols[i].y.total_cmp(&cols[j].y))
            });
            let mut lanes: Vec<Vec<usize>> = vec![];
            for &i in &members {
                if let Some(lane) = lanes.last_mut().filter(|l| near(cols[l[0]].x, cols[i].x)) {
                    lane.push(i);
                } else {
                    lanes.push(vec![i]);
                }
            }
            if lanes.len() < 2
                || lanes.iter().any(|l| {
                    !near(cols[l[0]].y, top)
                        || l.windows(2).any(|p| cols[p[1]].y < cols[p[0]].y1 - eps)
                })
                || lanes
                    .windows(2)
                    .any(|p| !near(cols[p[0][0]].x, cols[p[1][0]].x1))
                || !near(
                    members
                        .iter()
                        .map(|&i| cols[i].y1)
                        .fold(f32::NEG_INFINITY, f32::max),
                    bottom,
                )
            {
                continue;
            }
            // A whitespace boundary records the physical lanes without inventing punctuation.
            let text = lanes
                .iter()
                .map(|lane| {
                    lane.iter()
                        .map(|&i| cols[i].text.as_str())
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\u{3000}");
            let mut inner = opened.clone();
            if !brackets(&text, &mut inner) || inner != opened {
                continue;
            }
            proposals.push((a, b, members, text));
        }
    }
    let mut merged = vec![];
    let mut gone = std::collections::HashSet::new();
    for (i, (a, b, ids, text)) in proposals.iter().enumerate() {
        // Hold all competing interpretations, including overlaps with a parent.
        if proposals.iter().enumerate().any(|(j, q)| {
            i != j
                && ids
                    .iter()
                    .chain([a, b])
                    .any(|id| q.2.contains(id) || q.0 == *id || q.1 == *id)
        }) {
            continue;
        }
        // The enclosing body and its inline block are one logical reading span.
        // Keeping the parents separate lets later tier detection insert unrelated text.
        let root = *a;
        let root_id = cols[root].id;
        let text = format!("{}{}{}", cols[*a].text, text, cols[*b].text);
        for &id in ids.iter().chain([b]) {
            merged.push((cols[id].id, root_id));
            gone.insert(id);
        }
        cols[root] = Col {
            id: root_id,
            x: cols[*a].x,
            x1: cols[*a].x1,
            y: cols[*a].y,
            y1: cols[*b].y1,
            text,
        };
    }
    let mut i = 0;
    cols.retain(|_| {
        let keep = !gone.contains(&i);
        i += 1;
        keep
    });
    merged
}

pub(super) fn expand(
    regions: &mut [crate::trace::Region],
    merged: &[(usize, usize)],
    legacy: &[(usize, usize)],
) {
    let roots: std::collections::HashSet<_> = merged.iter().map(|p| p.1).collect();
    let mut groups = std::collections::HashMap::new();
    for root in roots {
        let mut ids = vec![root];
        ids.extend(merged.iter().filter(|p| p.1 == root).map(|p| p.0));
        loop {
            let before = ids.len();
            for &(id, owner) in legacy {
                if ids.contains(&owner) && !ids.contains(&id) {
                    ids.push(id);
                }
            }
            if ids.len() == before {
                break;
            }
        }
        groups.insert(root, ids);
    }
    for region in regions {
        for line in &mut region.lines {
            line.words = line
                .words
                .iter()
                .flat_map(|id| groups.get(id).cloned().unwrap_or_else(|| vec![*id]))
                .collect();
            if let Some(table) = &mut line.table {
                for cell in &mut table.cells {
                    cell.words = cell
                        .words
                        .iter()
                        .flat_map(|id| groups.get(id).cloned().unwrap_or_else(|| vec![*id]))
                        .collect();
                }
            }
        }
    }
}
