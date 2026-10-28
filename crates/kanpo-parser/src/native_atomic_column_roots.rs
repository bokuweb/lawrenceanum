//! Only a certified column reconstruction may introduce a source block.
//! Existing regions are not reclassified from their shape or label vocabulary.
use super::*;
pub(super) fn starts(page: &PageTrace, rules: &PageRules, lines: &[SourceLine<'_>]) -> Vec<usize> {
    if page.native_atomic_columns.is_empty() {
        return vec![];
    }
    if page
        .regions
        .iter()
        .any(|r| r.role != Role::Body || r.lines.iter().any(|l| l.table.is_some()))
    {
        return vec![];
    }
    let mut native = HashMap::new();
    for w in &page.words {
        if w.status != WordStatus::Used {
            continue;
        }
        let Some(crate::trace::NativeBounds::Glyphs(b)) = page.native_bounds.get(w.id) else {
            return vec![];
        };
        if !valid(*b)
            || page.native_axes.get(w.id) != Some(&crate::native_word_axes::NativeAxis::Right)
        {
            return vec![];
        }
        native.insert(w.id, *b);
    }
    let mut cuts = BTreeSet::new();
    for (at, row) in lines.iter().enumerate() {
        let Some(region) = page.regions.get(row.scope) else {
            continue;
        };
        if region.lines.len() != 1 || row.ids.is_empty() || !row.unquoted || row.tokens.is_none() {
            continue;
        }
        let Some(b) = row.bbox else { continue };
        let mut walls = rules
            .vertical
            .iter()
            .filter(|v| v.y0 <= b[1] && b[3] <= v.y1 && v.x.is_finite())
            .collect::<Vec<_>>();
        walls.sort_by(|a, b| a.x.total_cmp(&b.x));
        let mut proof = 0;
        for pair in walls.windows(2) {
            let frame = [
                pair[0].x,
                pair[0].y0.max(pair[1].y0),
                pair[1].x,
                pair[0].y1.min(pair[1].y1),
            ];
            if !valid(frame) || !contains(frame, b) {
                continue;
            }
            if !page.native_atomic_columns.iter().any(|proof| {
                proof.frame == frame && proof.words == row.ids.iter().copied().collect()
            }) {
                continue;
            }
            if rules.enclosures.iter().any(|e| intersects(*e, frame)) {
                continue;
            }
            // Closing or internal horizontal rules make this a possible cell,
            // rather than an independent open physical column.
            if rules
                .horizontal
                .iter()
                .any(|h| frame[1] <= h.y && h.y <= frame[3] && h.x0 < frame[2] && frame[0] < h.x1)
            {
                continue;
            }
            let inside = native
                .iter()
                .filter(|(_, b)| intersects(frame, **b))
                .collect::<Vec<_>>();
            if inside.iter().any(|(_, b)| !contains(frame, **b)) {
                continue;
            }
            if inside.iter().map(|(&id, _)| id).collect::<BTreeSet<_>>()
                != row.ids.iter().copied().collect()
            {
                continue;
            }
            // The complete physical lane occupies one original row envelope.
            if inside
                .iter()
                .map(|(_, b)| b[1])
                .reduce(f32::max)
                .zip(inside.iter().map(|(_, b)| b[3]).reduce(f32::min))
                .is_none_or(|(a, b)| a >= b)
            {
                continue;
            }
            proof += 1;
        }
        if proof == 1 {
            cuts.insert(at);
            cuts.insert(at + 1);
        }
    }
    cuts.into_iter().collect()
}
