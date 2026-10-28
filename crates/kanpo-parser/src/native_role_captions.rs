// Semantic captions are publication grammar;
// no body wording, page identity, expected data or spacing/count score is used.
pub(crate) fn explicit_pair(source: RoleSource<'_>) -> bool { printed_pair(source).is_some() }
pub(crate) fn printed_pair(source: RoleSource<'_>) -> Option<crate::ComparisonLabels> {
    use std::collections::{HashMap, HashSet};
    type Rect = [f32; 4];
    fn positive(b: Rect) -> bool {
        b.iter().all(|x| x.is_finite()) && b[0] < b[2] && b[1] < b[3]
    }
    fn overlap(a: (f32, f32), b: (f32, f32)) -> bool {
        a.0 < b.1 && b.0 < a.1
    }
    fn union(a: Rect, b: Rect) -> Rect {
        [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[2].max(b[2]),
            a[3].max(b[3]),
        ]
    }
    fn intersects(a: Rect, b: Rect) -> bool {
        positive(a)
            && positive(b)
            && overlap((a[0], a[2]), (b[0], b[2]))
            && overlap((a[1], a[3]), (b[1], b[3]))
    }
    fn line_ids(line: &Line) -> Vec<usize> {
        let mut ids = line.words.clone();
        if let Some(t) = &line.table {
            for c in &t.cells {
                ids.extend_from_slice(&c.words);
            }
        }
        ids
    }
    fn compact(s: &str) -> String {
        s.chars().filter(|c| !c.is_whitespace()).collect()
    }
    const GRAMMAR: [(&str, &str, Role); 4] = [
        ("改正後", "改正", Role::After),
        ("改正前", "改正", Role::Before),
        ("変更後", "変更", Role::After),
        ("変更前", "変更", Role::Before),
    ];
    struct Label {
        stem: &'static str,
        pair: crate::ComparisonLabels,
        role: Role,
        axis: NativeAxis,
        ids: Vec<usize>,
        bounds: Rect,
    }
    if source.words.len() != source.bounds.len() || source.words.len() != source.axes.len() {
        return None;
    }
    if source.regions.windows(2).any(|r| r[0].unit > r[1].unit) {
        return None;
    }
    let Some(last) = source.regions.last() else {
        return None;
    };
    let unit = last.unit;
    let mut owners = HashMap::new();
    let mut fields = [HashSet::new(), HashSet::new()];
    for r in source.regions {
        for l in &r.lines {
            for id in line_ids(l) {
                if id >= source.words.len() || owners.insert(id, (r.unit, r.role)).is_some() {
                    return None;
                }
                if r.unit == unit {
                    match r.role {
                        Role::After => {
                            fields[0].insert(id);
                        }
                        Role::Before => {
                            fields[1].insert(id);
                        }
                        Role::Body | Role::Outside => return None,
                        Role::Preamble => {}
                    }
                }
            }
        }
    }
    if fields.iter().any(|f| f.is_empty()) {
        return None;
    }
    let mut boxes = HashMap::new();
    for (i, w) in source.words.iter().enumerate() {
        if w.id != i {
            return None;
        }
        let omitted_noise = w.status == WordStatus::Noise && !owners.contains_key(&i);
        if !matches!(w.status, WordStatus::Used | WordStatus::HeaderLabel) && !omitted_noise {
            continue;
        }
        // Diagnostic Noise is not semantic evidence by itself. A retained
        // original occurrence can participate only through the complete native
        // caption grammar and exclusive leading field scope below. Other such
        // occurrences remain opaque obstacles.
        if omitted_noise && !matches!(source.bounds[i],NativeBounds::Glyphs(b)if positive(b)) {
            continue;
        }
        // Used constituents from earlier legacy merges may lack a direct root.
        // They remain opaque source obstacles, never semantic caption evidence.
        if w.status == WordStatus::HeaderLabel && owners.contains_key(&i) {
            return None;
        }
        let NativeBounds::Glyphs(b) = source.bounds[i] else {
            return None;
        };
        if !positive(b)
            || !omitted_noise
                && matches!(
                    source.axes[i],
                    NativeAxis::Unknown | NativeAxis::Compatibility
                )
        {
            return None;
        }
        boxes.insert(i, b);
    }
    // A role cannot borrow a current occurrence whose source status was dropped.
    if owners.keys().any(|id| !boxes.contains_key(id)) {
        return None;
    }
    let mut labels = Vec::new();
    for (&seed, &b) in &boxes {
        let axis = source.axes[seed];
        if !matches!(axis, NativeAxis::Down | NativeAxis::Right) {
            continue;
        }
        let mut text = compact(&source.words[seed].text);
        if text.is_empty() || !GRAMMAR.iter().any(|g| g.0.starts_with(&text)) {
            continue;
        }
        let ownership = owners.get(&seed).copied();
        if ownership.is_none()
            && !matches!(
                source.words[seed].status,
                WordStatus::HeaderLabel | WordStatus::Noise
            )
        {
            continue;
        }
        if ownership.is_some_and(|(u, r)| u != unit || !matches!(r, Role::After | Role::Before)) {
            continue;
        }
        let mut ids = vec![seed];
        let mut bb = b;
        let mut last = b;
        let mut band = if axis == NativeAxis::Down {
            (b[0], b[2])
        } else {
            (b[1], b[3])
        };
        let mut valid = true;
        loop {
            let id = *ids.last().unwrap();
            let w = &source.words[id];
            // Caption semantics must come from this original source occurrence.
            // A prior text rewrite cannot lend an unrelated word its geometry.
            if compact(&crate::normalize::word_text(&w.raw)) != compact(&w.text) {
                break;
            }
            // Display normalization can select this occurrence's original
            // glyph frame instead of its legacy projected source rectangle.
            // Neither frame authorizes an arbitrary reconstructed position.
            let display=[w.x0,w.y0,w.x1,w.y1];
            if (display!=w.source_box && display!=boxes[&id]) || !positive(w.source_box) {
                break;
            }
            if let Some(&(full, stem, role)) = GRAMMAR.iter().find(|g| g.0 == text) {
                let _ = full;
                if ownership.is_some_and(|(_, owner)| owner != role) {
                    valid = false;
                }
                if valid {
                    labels.push(Label {
                        stem,
                        pair: crate::ComparisonLabels::from_after(full).or_else(||crate::ComparisonLabels::from_before(full))?,
                        role,
                        axis,
                        ids: ids.clone(),
                        bounds: bb,
                    });
                }
                break;
            }
            // Choose the next actual source occurrence on the common native
            // axis band. Intervening body/other-axis words cannot be skipped.
            let mut next: Vec<_> = boxes
                .iter()
                .filter(|(i, b)| {
                    !ids.contains(i)
                        && if axis == NativeAxis::Down {
                            overlap(band, (b[0], b[2])) && last[3] <= b[1]
                        } else {
                            overlap(band, (b[1], b[3])) && last[2] <= b[0]
                        }
                })
                .map(|(&i, &b)| (i, b))
                .collect();
            next.sort_by(|a, b| {
                let k = if axis == NativeAxis::Down { 1 } else { 0 };
                a.1[k].total_cmp(&b.1[k])
            });
            let Some(&(id, b)) = next.first() else { break };
            let k = if axis == NativeAxis::Down { 1 } else { 0 };
            if next.get(1).is_some_and(|n| n.1[k] == b[k])
                || source.axes[id] != axis
                || owners.get(&id).copied() != ownership
                || (ownership.is_none()
                    && !matches!(
                        source.words[id].status,
                        WordStatus::HeaderLabel | WordStatus::Noise
                    ))
            {
                break;
            }
            let part = compact(&source.words[id].text);
            if part.is_empty() {
                break;
            }
            text.push_str(&part);
            if !GRAMMAR.iter().any(|g| g.0.starts_with(&text)) {
                break;
            }
            band = if axis == NativeAxis::Down {
                (band.0.max(b[0]), band.1.min(b[2]))
            } else {
                (band.0.max(b[1]), band.1.min(b[3]))
            };
            if band.0 >= band.1 {
                break;
            }
            ids.push(id);
            last = b;
            bb = union(bb, b);
        }
    }
    let body = |field: &HashSet<usize>, label: &Label| -> Option<Rect> {
        field
            .iter()
            .filter(|id| !label.ids.contains(id))
            .map(|id| boxes[id])
            .reduce(union)
    };
    let attached = |label: &Label, body: Rect, field: &HashSet<usize>| -> bool {
        let b = label.bounds;
        // Certify the complete relevant support. Foreign/opaque occurrences in
        // its white space cannot be silently assigned to a current role.
        let support = union(b, body);
        if boxes.iter().any(|(id, b)| {
            !field.contains(id) && !label.ids.contains(id) && intersects(support, *b)
        }) {
            return false;
        }
        let corridor = if label.axis == NativeAxis::Down {
            if b[0] < body[2] || !overlap((b[1], b[3]), (body[1], body[3])) {
                return false;
            }
            [body[2], b[1].max(body[1]), b[0], b[3].min(body[3])]
        } else {
            if b[3] > body[1] || !overlap((b[0], b[2]), (body[0], body[2])) {
                return false;
            }
            [b[0].max(body[0]), b[3], b[2].min(body[2]), body[1]]
        };
        !boxes.iter().any(|(id, b)| {
            !field.contains(id) && !label.ids.contains(id) && intersects(corridor, *b)
        })
    };
    let mut proofs = Vec::new();
    for a in labels.iter().filter(|l| l.role == Role::After) {
        for b in labels.iter().filter(|l| l.role == Role::Before) {
            if a.stem != b.stem || a.axis != b.axis {
                continue;
            }
            let (Some(ea), Some(eb)) = (body(&fields[0], a), body(&fields[1], b)) else {
                continue;
            };
            if !attached(a, ea, &fields[0]) || !attached(b, eb, &fields[1]) {
                continue;
            }
            let ordered = if a.axis == NativeAxis::Down {
                a.bounds[3] <= b.bounds[1]
                    && overlap((a.bounds[0], a.bounds[2]), (b.bounds[0], b.bounds[2]))
                    && ea[3] <= eb[1]
            } else {
                a.bounds[2] <= b.bounds[0]
                    && overlap((a.bounds[1], a.bounds[3]), (b.bounds[1], b.bounds[3]))
                    && ea[2] <= eb[0]
            };
            if ordered {
                proofs.push(a.pair);
            }
        }
    }
    let [only] = proofs.as_slice() else { return None; };
    Some(*only)
}
