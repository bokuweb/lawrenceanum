//! An earlier printed frame can own an initial part of a later preamble.
//! The new printed comparison remains an independent source root.
use super::*;
#[path = "native_line_field_spans.rs"]
mod line_spans;

// Reconstruct only canonical existing units. This is a lossless container
// conversion: every original line, part, role and label must round-trip.
fn original_units(p: &PageTrace) -> Option<Vec<Unit>> {
    let mut out = Vec::new();
    for (i, labels) in p.native_comparison_labels.iter().enumerate() {
        let regions = p.regions.iter().filter(|r| r.unit == i).collect::<Vec<_>>();
        if regions.is_empty() {
            return None;
        }
        let unit = if regions.len() == 1 && regions[0].role == Role::Body {
            if labels.is_some() {
                return None;
            }
            Unit::Plain(regions[0].lines.clone())
        } else {
            if regions.iter().any(|r| r.role == Role::Body) {
                return None;
            }
            let parts = |role| {
                regions
                    .iter()
                    .filter(|r| r.role == role)
                    .map(|r| r.lines.clone())
                    .collect::<Vec<_>>()
            };
            let mut preamble = parts(Role::Preamble);
            if preamble.len() > 1 {
                return None;
            }
            Unit::Shinkyu {
                labels: *labels,
                preamble: preamble.pop().unwrap_or_default(),
                after: parts(Role::After),
                before: parts(Role::Before),
                outside: parts(Role::Outside),
            }
        };
        out.push(unit);
    }
    let roundtrip = out
        .iter()
        .enumerate()
        .flat_map(|(i, u)| {
            let mut parts = Vec::new();
            let mut push = |role, part, lines: &Vec<Line>| {
                if !lines.is_empty() {
                    parts.push((i, role, part, lines.clone()));
                }
            };
            match u {
                Unit::Plain(lines) => push(Role::Body, 0, lines),
                Unit::Shinkyu {
                    preamble,
                    after,
                    before,
                    outside,
                    ..
                } => {
                    push(Role::Preamble, 0, preamble);
                    for (role, groups) in [
                        (Role::After, after),
                        (Role::Before, before),
                        (Role::Outside, outside),
                    ] {
                        for (part, lines) in groups.iter().enumerate() {
                            push(role, part, lines);
                        }
                    }
                }
            }
            parts
        })
        .collect::<Vec<_>>();
    if roundtrip.len() != p.regions.len()
        || roundtrip
            .iter()
            .zip(&p.regions)
            .any(|((unit, role, part, lines), b)| {
                *unit != b.unit || *part != b.part || *role != b.role || *lines != b.lines
            })
    {
        return None;
    }
    Some(out)
}

pub(super) fn restore(
    p: &PageTrace,
    node: scraper::ElementRef<'_>,
    rules: &PageRules,
    token: &Token,
) -> Option<Vec<Unit>> {
    let first = p.regions.first()?;
    if first.unit != 0 || first.role != Role::Preamble {
        return None;
    }
    let mut units = original_units(p)?;
    let scene =
        crate::native_source_scene::for_page(node, rules).filter(|s| s.matches(&p.words))?;
    let dimensions = source_size(p, node)?;
    // The untouched suffix is still original material. Reject corrupt ownership
    // anywhere on the page instead of certifying a plausible-looking prefix.
    let mut owned = BTreeSet::new();
    for line in p.regions.iter().flat_map(|r| &r.lines) {
        let ids = ids(line);
        if ids.is_empty()
            || !line_material(line, p)
            || ids.iter().any(|&id| {
                !owned.insert(id)
                    || p.words
                        .get(id)
                        .is_none_or(|w| w.id != id || w.status != WordStatus::Used)
            })
        {
            return None;
        }
    }
    if p.words
        .iter()
        .any(|w| w.status == WordStatus::Used && !owned.contains(&w.id))
    {
        return None;
    }
    let wanted = first.lines.iter().flat_map(ids).collect::<BTreeSet<_>>();
    let material = materials(p, node, &wanted)?;
    let mut candidates = Vec::new();
    for axis in [NativeAxis::Down, NativeAxis::Right] {
        let size = dimensions[usize::from(axis == NativeAxis::Down)];
        let mut fields: [Vec<Line>; 2] = Default::default();
        let mut supports = Vec::new();
        let mut count = 0;
        let mut cuts = None;
        let mut retained = Vec::new();
        for line in &first.lines {
            if crate::segment::is_heading_mark(line.text.trim()) {
                break;
            }
            let ids = ids(line);
            let own = ids
                .iter()
                .map(|id| material[id].support(axis))
                .collect::<Vec<_>>();
            let fences = scene.parallel_fences(axis, &own);
            let Some(q) = aligned_cuts(token, &fences, size) else {
                break;
            };
            let Some(sides) = ids
                .iter()
                .map(|id| side(&material[id], axis, q))
                .collect::<Option<Vec<_>>>()
            else {
                break;
            };
            if sides.iter().all(|&s| s == sides[0]) {
                fields[sides[0]].push(line.clone());
            } else {
                let Some(parts) = line_spans::partition(line, &p.words, &sides) else {
                    break;
                };
                for (field, part) in parts.into_iter().enumerate() {
                    fields[field].push(part?);
                }
            }
            supports.extend(own);
            cuts = Some(q);
            retained.extend(ids.iter().map(|id| &material[id]));
            count += 1;
        }
        let Some(next) = first.lines.get(count) else {
            continue;
        };
        let next = ids(next).iter().map(|id| &material[id]).collect::<Vec<_>>();
        if count == 0
            || fields.iter().any(Vec::is_empty)
            || scene.opaque_crosses_fence(axis, cuts?[1], &supports)
            || !scene.cross_cap_separates(
                axis,
                cuts?,
                &retained.iter().map(|m| m.bounds).collect::<Vec<_>>(),
                &next.iter().map(|m| m.bounds).collect::<Vec<_>>(),
            )
        {
            continue;
        }
        candidates.push((count, fields));
    }
    let [(count, fields)] = candidates.as_slice() else {
        return None;
    };
    let Unit::Shinkyu { preamble, .. } = units.first_mut()? else {
        return None;
    };
    preamble.drain(..*count);
    units.insert(
        0,
        Unit::Shinkyu {
            labels: Some(token.labels),
            preamble: vec![],
            after: vec![fields[0].clone()],
            before: vec![fields[1].clone()],
            outside: vec![],
        },
    );
    Some(units)
}
