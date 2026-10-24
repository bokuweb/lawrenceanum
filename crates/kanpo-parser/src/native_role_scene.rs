// The pair is source certified; ordinary
// components use complete source-grid membership or one disjoint opaque support.
pub(crate) fn source_scene(source: Source<'_>, pair: Plan) -> Option<Vec<SceneAtom>> {
    use std::collections::{BTreeMap, HashSet};
    type Rect = [f32; 4];
    fn positive(b: Rect) -> bool {
        b.iter().all(|x| x.is_finite()) && b[0] < b[2] && b[1] < b[3]
    }
    fn contains(a: Rect, b: Rect) -> bool {
        a[0] <= b[0] && a[1] <= b[1] && b[2] <= a[2] && b[3] <= a[3]
    }
    fn intersects(a: Rect, b: Rect) -> bool {
        a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
    }
    fn union(a: Rect, b: Rect) -> Rect {
        [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[2].max(b[2]),
            a[3].max(b[3]),
        ]
    }
    fn rect(g: &Grid) -> Option<Rect> {
        if g.xs.len() < 2
            || g.ys.len() < 2
            || g.xs.iter().chain(&g.ys).any(|x| !x.is_finite())
            || g.xs.windows(2).any(|w| w[0] >= w[1])
            || g.ys.windows(2).any(|w| w[0] >= w[1])
        {
            return None;
        }
        if g.cells
            .iter()
            .any(|c| c.x0 >= c.x1 || c.y0 >= c.y1 || c.x1 >= g.xs.len() || c.y1 >= g.ys.len())
        {
            return None;
        }
        Some([g.left(), g.top(), g.right(), g.bottom()])
    }
    fn caption_row(s: &str) -> bool {
        const LABELS: [&str; 4] = ["改正後", "改正前", "変更後", "変更前"];
        let compact: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        let mut rest = compact.as_str();
        let mut found = false;
        while !rest.is_empty() {
            let Some(next) = LABELS.iter().find_map(|s| rest.strip_prefix(*s)) else {
                return false;
            };
            rest = next;
            found = true;
        }
        found
    }
    struct Atom {
        value: SceneAtom,
        outer: Rect,
        envelope: Rect,
    }
    if !source.inherited_explicit_roles {
        return None;
    }
    let mut boxes = BTreeMap::new();
    for w in source.words {
        if w.status != WordStatus::Used {
            continue;
        }
        let NativeBounds::Glyphs(b) = *source.bounds.get(w.id)? else {
            return None;
        };
        if !positive(b) || source.axes.get(w.id) != Some(&NativeAxis::Right) {
            return None;
        }
        boxes.insert(w.id, b);
    }
    let pair_ids: HashSet<_> = pair.after.iter().chain(&pair.before).copied().collect();
    if pair.after.is_empty()
        || pair.before.is_empty()
        || pair_ids.len() != pair.after.len() + pair.before.len()
    {
        return None;
    }
    let rectangles: Vec<_> = source.grids.iter().map(rect).collect::<Option<_>>()?;
    let targets: Vec<_> = rectangles
        .iter()
        .enumerate()
        .filter(|(_, r)| {
            boxes
                .iter()
                .filter_map(|(&id, &b)| contains(**r, b).then_some(id))
                .collect::<HashSet<_>>()
                == pair_ids
        })
        .map(|(i, _)| i)
        .collect();
    let [target] = targets.as_slice() else {
        return None;
    };
    let target = *target;
    let envelope = |ids: &[usize]| -> Option<Rect> {
        ids.iter()
            .map(|i| boxes.get(i).copied())
            .collect::<Option<Vec<_>>>()?
            .into_iter()
            .reduce(union)
    };
    let ordered_ids = |ids: &HashSet<usize>| -> Vec<usize> {
        source
            .cols
            .iter()
            .filter(|c| ids.contains(&c.id))
            .map(|c| c.id)
            .collect()
    };
    let has_caption = |ids: &HashSet<usize>| -> bool {
        let native_cols: Vec<_> = source
            .cols
            .iter()
            .filter(|c| ids.contains(&c.id))
            .map(|c| {
                let mut c = c.clone();
                let b = boxes[&c.id];
                c.x = b[0];
                c.y = b[1];
                c.x1 = b[2];
                c.y1 = b[3];
                c
            })
            .collect();
        crate::horizontal::row_lines(native_cols)
            .iter()
            .any(|l| caption_row(&l.text))
    };
    if has_caption(&pair_ids) {
        return None;
    }
    // Keep each ordinary parent as one opaque scope; its existing renderer may
    // still use all nested grids internally. Equal or crossing alternatives are
    // not an unambiguous hierarchy.
    for i in 0..rectangles.len() {
        for j in i + 1..rectangles.len() {
            if i == target || j == target {
                if intersects(rectangles[i], rectangles[j]) {
                    return None;
                }
                continue;
            }
            if rectangles[i] == rectangles[j]
                || intersects(rectangles[i], rectangles[j])
                    && !(contains(rectangles[i], rectangles[j])
                        || contains(rectangles[j], rectangles[i]))
            {
                return None;
            }
        }
    }
    let mut atoms = vec![Atom {
        outer: rectangles[target],
        envelope: envelope(&pair_ids.iter().copied().collect::<Vec<_>>())?,
        value: SceneAtom::Pair(pair),
    }];
    let mut claimed = pair_ids;
    for (i, &r) in rectangles.iter().enumerate() {
        if i == target {
            continue;
        }
        // Even child grids must not hide a current printed role-caption row.
        // This fallback is for inherited, unheaded fields; an unclassified
        // caption-bearing component must use the regular explicit-role path.
        let mut members = HashSet::new();
        for (&id, &b) in &boxes {
            if intersects(r, b) {
                if !contains(r, b) {
                    return None;
                }
                members.insert(id);
            }
        }
        if !members.is_empty() {
            let native_cols: Vec<_> = source
                .cols
                .iter()
                .filter(|c| members.contains(&c.id))
                .map(|c| {
                    let mut c = c.clone();
                    let b = boxes[&c.id];
                    c.x = b[0];
                    c.y = b[1];
                    c.x1 = b[2];
                    c.y1 = b[3];
                    c
                })
                .collect();
            if crate::horizontal::row_lines(native_cols)
                .iter()
                .any(|l| caption_row(&l.text))
            {
                return None;
            }
        }
        if rectangles
            .iter()
            .enumerate()
            .any(|(j, &other)| j != i && j != target && contains(other, r))
        {
            continue;
        }
        if members.is_empty() {
            continue;
        }
        if members.iter().any(|id| !claimed.insert(*id)) {
            return None;
        }
        let ids = ordered_ids(&members);
        atoms.push(Atom {
            outer: r,
            envelope: envelope(&ids)?,
            value: SceneAtom::Body(ids),
        });
    }
    let foreign: HashSet<_> = boxes
        .keys()
        .filter(|id| !claimed.contains(id))
        .copied()
        .collect();
    if !foreign.is_empty() {
        let ids = ordered_ids(&foreign);
        let bb = envelope(&ids)?;
        if rectangles.iter().any(|&r| intersects(r, bb)) {
            return None;
        }
        let native_cols: Vec<_> = source
            .cols
            .iter()
            .filter(|c| foreign.contains(&c.id))
            .map(|c| {
                let mut c = c.clone();
                let b = boxes[&c.id];
                c.x = b[0];
                c.y = b[1];
                c.x1 = b[2];
                c.y1 = b[3];
                c
            })
            .collect();
        if crate::horizontal::row_lines(native_cols)
            .iter()
            .any(|l| caption_row(&l.text))
        {
            return None;
        }
        claimed.extend(foreign);
        atoms.push(Atom {
            outer: bb,
            envelope: bb,
            value: SceneAtom::Body(ids),
        });
    }
    if claimed.len() != boxes.len() {
        return None;
    }
    let n = atoms.len();
    let mut edges = vec![vec![false; n]; n];
    let mut incoming = vec![0usize; n];
    for i in 0..n {
        for j in 0..n {
            if i != j
                && crate::ruled_cell_flow::certified_before(
                    atoms[i].outer,
                    atoms[i].envelope,
                    atoms[j].outer,
                    atoms[j].envelope,
                    source.rules,
                )
            {
                edges[i][j] = true;
                incoming[j] += 1;
            }
        }
    }
    let mut selected = vec![false; n];
    let mut order = Vec::new();
    while order.len() < n {
        let eligible: Vec<_> = (0..n)
            .filter(|&i| !selected[i] && incoming[i] == 0)
            .collect();
        let [next] = eligible.as_slice() else {
            return None;
        };
        let next = *next;
        selected[next] = true;
        order.push(next);
        for j in 0..n {
            if edges[next][j] {
                incoming[j] -= 1;
            }
        }
    }
    if !matches!(atoms[order[0]].value, SceneAtom::Pair(_)) {
        return None;
    }
    let mut values: Vec<_> = atoms.into_iter().map(|a| Some(a.value)).collect();
    Some(
        order
            .into_iter()
            .map(|i| values[i].take().unwrap())
            .collect(),
    )
}
