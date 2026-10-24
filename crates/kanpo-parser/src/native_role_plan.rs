// Certify a native open-top pair from source occurrences and continuous rulings.
pub(crate) fn source_plan(source: Source<'_>) -> Option<Plan> {
    use std::collections::{BTreeMap, HashSet};
    type Rect = [f32; 4];
    fn positive(b: Rect) -> bool {
        b.iter().all(|v| v.is_finite()) && b[0] < b[2] && b[1] < b[3]
    }
    fn intersects(a: Rect, b: Rect) -> bool {
        a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
    }
    fn contains(a: Rect, b: Rect) -> bool {
        a[0] <= b[0] && a[1] <= b[1] && b[2] <= a[2] && b[3] <= a[3]
    }
    fn cover(mut segments: Vec<(f32, f32)>, lo: f32, hi: f32) -> bool {
        if !lo.is_finite() || !hi.is_finite() || lo >= hi {
            return false;
        }
        segments.retain(|&(a, b)| a.is_finite() && b.is_finite() && a < b);
        segments.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut end = lo;
        for (a, b) in segments {
            if b <= end {
                continue;
            }
            if a > end {
                return false;
            }
            end = end.max(b);
            if end >= hi {
                return true;
            }
        }
        false
    }
    if !source.inherited_explicit_roles
        || source.axes.len() != source.words.len()
        || source.bounds.len() != source.words.len()
    {
        return None;
    }
    let mut current = BTreeMap::new();
    for c in source.cols {
        let w = source.words.get(c.id)?;
        let b = [c.x, c.y, c.x1, c.y1];
        if w.id != c.id
            || w.status != WordStatus::Used
            || w.text != c.text
            || w.text.trim().is_empty()
            || b != [w.x0, w.y0, w.x1, w.y1]
            || b != w.source_box
            || !positive(b)
            || current.insert(c.id, c).is_some()
        {
            return None;
        }
    }
    let mut bounds = BTreeMap::new();
    for (i, w) in source.words.iter().enumerate() {
        if w.id != i {
            return None;
        }
        if w.status != WordStatus::Used {
            continue;
        }
        if !current.contains_key(&i) {
            return None;
        }
        let NativeBounds::Glyphs(b) = source.bounds[i] else {
            return None;
        };
        if !positive(b) {
            return None;
        }
        bounds.insert(i, b);
    }
    let mut proposals = Vec::new();
    for (gi, g) in source.grids.iter().enumerate() {
        // Exactly one physical row with two complete role cells defines a pair.
        // These dimensions express the role relationship, not a record-count cap.
        if g.xs.len() != 3
            || g.ys.len() != 2
            || g.cells.len() != 2
            || g.xs.iter().chain(&g.ys).any(|x| !x.is_finite())
            || g.xs.windows(2).any(|w| w[0] >= w[1])
            || g.ys[0] >= g.ys[1]
            || !(0..2).all(|x| {
                g.cells
                    .iter()
                    .filter(|c| c.x0 == x && c.x1 == x + 1 && c.y0 == 0 && c.y1 == 1)
                    .count()
                    == 1
            })
        {
            continue;
        }
        let outer = [g.xs[0], g.ys[0], g.xs[2], g.ys[1]];
        // A second overlapping source interpretation invalidates this footprint.
        if source.grids.iter().enumerate().any(|(j, other)| {
            j != gi
                && !other.xs.is_empty()
                && !other.ys.is_empty()
                && intersects(
                    outer,
                    [other.left(), other.top(), other.right(), other.bottom()],
                )
        }) {
            continue;
        }
        if !g.xs.iter().all(|&x| {
            cover(
                source
                    .rules
                    .vertical
                    .iter()
                    .filter(|v| v.x == x)
                    .map(|v| (v.y0, v.y1))
                    .collect(),
                g.top(),
                g.bottom(),
            )
        }) {
            continue;
        }
        if !cover(
            source
                .rules
                .horizontal
                .iter()
                .filter(|h| h.y == g.bottom())
                .map(|h| (h.x0, h.x1))
                .collect(),
            g.left(),
            g.right(),
        ) {
            continue;
        }
        // No complete or partial incoming cap can certify an open continuation.
        if source.rules.horizontal.iter().any(|h| {
            h.y == g.top()
                && h.x0.is_finite()
                && h.x1.is_finite()
                && h.x0 < h.x1
                && h.x0 < g.right()
                && g.left() < h.x1
        }) {
            continue;
        }
        if bounds
            .values()
            .any(|b| b[0] < g.right() && g.left() < b[2] && b[1] < g.top())
        {
            continue;
        }
        let mut members = [HashSet::new(), HashSet::new()];
        let mut valid = true;
        for (&id, &b) in &bounds {
            if !intersects(outer, b) {
                continue;
            }
            let mut owners = Vec::new();
            for x in 0..2 {
                if contains([g.xs[x], g.top(), g.xs[x + 1], g.bottom()], b) {
                    owners.push(x);
                }
            }
            if owners.len() != 1 || source.axes[id] != NativeAxis::Right {
                valid = false;
                break;
            }
            members[owners[0]].insert(id);
        }
        if !valid || members.iter().any(|m| m.is_empty()) {
            continue;
        }
        let mut fields = Vec::new();
        for ids in &members {
            let cols: Vec<_> = source
                .cols
                .iter()
                .filter(|c| ids.contains(&c.id))
                .cloned()
                .collect();
            let lines = crate::horizontal::row_lines(cols.clone());
            let ordered: Vec<_> = lines.iter().flat_map(|l| l.words.iter().copied()).collect();
            if ordered.len() != ids.len() || ordered.iter().copied().collect::<HashSet<_>>() != *ids
            {
                valid = false;
                break;
            }
            fields.push(ordered);
        }
        if valid {
            proposals.push(Plan {
                after: fields.remove(0),
                before: fields.remove(0),
            });
        }
    }
    (proposals.len() == 1).then(|| proposals.remove(0))
}
