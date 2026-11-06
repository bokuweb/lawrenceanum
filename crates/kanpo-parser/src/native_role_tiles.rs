//! A printed pair can repeat in further source-ruled tiers on the same page.
//! Reuse its physical field capacities, not the wording or number of tiers.
use super::*;

pub(super) struct Restored {
    pub units: Vec<Unit>,
    pub next: Option<Token>,
    pub printed: bool,
}

fn matching_frames(token: &Token, fences: &[f64], size: f64) -> Vec<[f64; 3]> {
    let capacity = [token.cuts[1] - token.cuts[0], token.cuts[2] - token.cuts[1]];
    if !size.is_finite()
        || size <= 0.
        || !token.extent.is_finite()
        || token.extent <= 0.
        || capacity.iter().any(|v| !v.is_finite() || *v <= 0.)
    {
        return vec![];
    }
    let equal_capacity = |a: f64, b: f64| {
        let left = a * token.extent;
        let right = b * size;
        left.is_finite() && right.is_finite() && left == right
    };
    let mut out = vec![];
    for (i, &lo) in fences.iter().enumerate() {
        for (j, &mid) in fences.iter().enumerate().skip(i + 1) {
            if !equal_capacity(mid - lo, capacity[0]) {
                continue;
            }
            for &hi in fences.iter().skip(j + 1) {
                if equal_capacity(hi - mid, capacity[1]) {
                    out.push([lo, mid, hi]);
                }
            }
        }
    }
    out
}

pub(super) fn restore(
    p: &PageTrace,
    node: scraper::ElementRef<'_>,
    rules: &PageRules,
    previous: Option<&Token>,
) -> Option<Restored> {
    // A page must actually have a plain suffix. Already classified units do
    // not become new semantic roots, nor can they be moved by this fallback.
    if p.regions.last()?.role != Role::Body {
        return None;
    }
    let mut units = prefix::original_units(p)?;
    let mut owners = BTreeSet::new();
    for id in p.regions.iter().flat_map(|r| &r.lines).flat_map(ids) {
        if !owners.insert(id)
            || p.words
                .get(id)
                .is_none_or(|w| w.id != id || w.status != WordStatus::Used)
        {
            return None;
        }
    }
    if p.words
        .iter()
        .any(|w| w.status == WordStatus::Used && !owners.contains(&w.id))
    {
        return None;
    }
    let root = units
        .iter()
        .rposition(|u| matches!(u, Unit::Shinkyu { .. }));
    if previous.is_none() && root.is_none() {
        return None;
    }
    let scene =
        crate::native_source_scene::for_page(node, rules).filter(|s| s.matches(&p.words))?;
    let dimensions = source_size(p, node)?;
    let (start, token, printed) = if let Some(root) = root {
        let mut projection = p.clone();
        projection.regions.retain(|r| r.unit == root);
        let labels = crate::native_role_continuation::printed_pair_in_scene(
            RoleSource {
                regions: &projection.regions,
                words: &p.words,
                bounds: &p.native_bounds,
                axes: &p.native_axes,
            },
            Some(&scene),
        )?;
        let mut state = State::default();
        state.prove(&projection, node, rules, Some(labels));
        (root + 1, state.token?, true)
    } else {
        (0, previous?.clone(), false)
    };
    if start >= units.len() || units[start..].iter().any(|u| !matches!(u, Unit::Plain(_))) {
        return None;
    }
    let mut wanted = BTreeSet::new();
    let mut rows = vec![];
    for (u, unit) in units.iter().enumerate().skip(start) {
        let Unit::Plain(lines) = unit else {
            return None;
        };
        for (l, line) in lines.iter().enumerate() {
            let ids = ids(line);
            if ids.is_empty() || !line_material(line, p) || ids.iter().any(|id| !wanted.insert(*id))
            {
                return None;
            }
            rows.push((u, l, line, ids));
        }
    }
    let material = materials(p, node, &wanted)?;
    let mut groups: Vec<(NativeAxis, [f64; 3], [Vec<Line>; 2], Vec<[f64; 4]>)> = vec![];
    let mut tail = None;
    for (u, l, line, ids) in &rows {
        let mut candidates = vec![];
        if !crate::segment::is_heading_mark(line.text.trim()) {
            for axis in [NativeAxis::Down, NativeAxis::Right] {
                let supports = ids
                    .iter()
                    .map(|id| material[id].support(axis))
                    .collect::<Vec<_>>();
                let fences = scene.parallel_fences(axis, &supports);
                for q in matching_frames(
                    &token,
                    &fences,
                    dimensions[usize::from(axis == NativeAxis::Down)],
                ) {
                    let Some(sides) = ids
                        .iter()
                        .map(|id| side(&material[id], axis, q))
                        .collect::<Option<Vec<_>>>()
                    else {
                        continue;
                    };
                    if scene.opaque_crosses_fence(axis, q[1], &supports) {
                        continue;
                    }
                    if sides.iter().all(|&s| s == sides[0]) {
                        let mut parts: [Option<Line>; 2] = Default::default();
                        parts[sides[0]] = Some((*line).clone());
                        candidates.push((axis, q, parts));
                    } else if single_flow_fragments(ids, &sides, &material, axis) {
                        if let Some(parts) = line_spans::partition(line, &p.words, &sides) {
                            candidates.push((axis, q, parts));
                        }
                    }
                }
            }
        }
        let [(axis, q, parts)] = candidates.as_slice() else {
            let (axis, q, _, retained) = groups.last()?;
            let next = ids.iter().map(|id| material[id].bounds).collect::<Vec<_>>();
            if !candidates.is_empty() || !scene.cross_cap_separates(*axis, *q, retained, &next) {
                return None;
            }
            tail = Some((*u, *l));
            break;
        };
        if groups.last().is_none_or(|g| g.0 != *axis || g.1 != *q) {
            // Returning to a previous tile would invent a new reading sequence.
            if groups.iter().any(|g| g.0 == *axis && g.1 == *q) {
                return None;
            }
            groups.push((*axis, *q, Default::default(), vec![]));
        }
        let group = groups.last_mut()?;
        for (field, part) in parts.iter().enumerate() {
            if let Some(part) = part {
                group.2[field].push(part.clone());
            }
        }
        group.3.extend(ids.iter().map(|id| material[id].bounds));
    }
    if groups.is_empty() {
        return None;
    }
    let mut out = units.drain(..start).collect::<Vec<_>>();
    for (_, _, fields, _) in groups {
        let [after, before] = fields;
        out.push(Unit::Shinkyu {
            labels: Some(token.labels),
            preamble: vec![],
            after: vec![after],
            before: vec![before],
            outside: vec![],
        });
    }
    if let Some((u, l)) = tail {
        // The physical cap ends this source scope. Retain the exact remainder.
        let Unit::Plain(lines) = &units[u - start] else {
            return None;
        };
        out.push(Unit::Plain(lines[l..].to_vec()));
        out.extend(units.into_iter().skip(u - start + 1));
    }
    Some(Restored {
        units: out,
        next: tail.is_none().then_some(token),
        printed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn original() -> (crate::PageInput, PageTrace) {
        let input = crate::page_inputs(include_bytes!("../tests/data/workbook/case-03.pdf"))
            .unwrap()
            .remove(0);
        let mut page = crate::analyze_item(std::slice::from_ref(&input))
            .pages
            .remove(0);
        // Recreate the independently observed legacy container partition. The
        // source Words, original lines, geometry and text remain untouched.
        for r in &mut page.regions {
            if r.unit > 0 {
                r.unit = if r.role == Role::Before { 2 } else { 1 };
                r.role = Role::Body;
                r.part = 0;
            }
        }
        page.native_comparison_labels = vec![None; 3];
        (input, page)
    }
    fn attempt(input: &crate::PageInput, page: &PageTrace) -> Option<Restored> {
        let dom = scraper::Html::parse_document(&input.xhtml);
        let node = dom
            .select(&scraper::Selector::parse("page").unwrap())
            .next()
            .unwrap();
        restore(page, node, input.rules.as_ref().unwrap(), None)
    }
    #[test]
    fn original_tier_proof_preserves_every_word_value_and_complete_line() {
        let (input, page) = original();
        let result = attempt(&input, &page).unwrap();
        let regions = result
            .units
            .into_iter()
            .enumerate()
            .flat_map(|(i, u)| u.regions(i))
            .collect::<Vec<_>>();
        let before = page
            .regions
            .iter()
            .flat_map(|r| &r.lines)
            .cloned()
            .collect::<Vec<_>>();
        let after = regions
            .iter()
            .flat_map(|r| &r.lines)
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(before, after);
        assert!(result.next.is_some());
    }
    #[test]
    fn original_tier_requires_source_scene_captions_axes_material_and_exclusive_ownership() {
        let (input, page) = original();
        let id = page
            .regions
            .iter()
            .find(|r| r.role == Role::Body)
            .unwrap()
            .lines[0]
            .words[0];
        for mode in 0..9 {
            let mut input = input.clone();
            let mut page = page.clone();
            match mode {
                0 => {
                    input.xhtml = input
                        .xhtml
                        .replace(" data-native-scene=", " data-missing-scene=")
                }
                1 => page.words[id].raw.push('x'),
                2 => page.words[id].source_box[0] += 1.,
                3 => page.native_axes[id] = NativeAxis::Unknown,
                4 => page.native_bounds[id] = NativeBounds::Unknown,
                5 => page.words[id].status = WordStatus::Noise,
                6 => page
                    .regions
                    .iter_mut()
                    .find(|r| r.role == Role::Body)
                    .unwrap()
                    .lines[0]
                    .text
                    .push('x'),
                7 => page
                    .regions
                    .iter_mut()
                    .find(|r| r.role == Role::Body)
                    .unwrap()
                    .lines[0]
                    .words
                    .push(id),
                _ => {
                    let caption = page
                        .words
                        .iter_mut()
                        .find(|w| w.status == WordStatus::HeaderLabel)
                        .unwrap();
                    caption.raw.push('x');
                }
            }
            assert!(attempt(&input, &page).is_none(), "mode {mode}");
        }
    }
    #[test]
    fn original_tier_cannot_invent_a_missing_original_divider() {
        let (mut input, page) = original();
        let dom = scraper::Html::parse_document(&input.xhtml);
        let node = dom
            .select(&scraper::Selector::parse("page").unwrap())
            .next()
            .unwrap();
        let mut scene: serde_json::Value =
            serde_json::from_str(node.value().attr("data-native-scene").unwrap()).unwrap();
        scene["lines"]
            .as_array_mut()
            .unwrap()
            .retain(|l| l[0][1].as_f64() != Some(609.) || l[0][3].as_f64() != Some(609.));
        let encoded = serde_json::to_string(&scene)
            .unwrap()
            .replace('&', "&amp;")
            .replace('"', "&quot;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        let start =
            input.xhtml.find(" data-native-scene=\"").unwrap() + " data-native-scene=\"".len();
        let end = start + input.xhtml[start..].find('"').unwrap();
        input.xhtml.replace_range(start..end, &encoded);
        assert!(attempt(&input, &page).is_none());
    }
    #[test]
    fn field_capacities_survive_source_translation_scale_and_arbitrary_tier_counts() {
        let token = Token {
            cuts: [2., 7., 13.],
            extent: 16.,
            labels: ComparisonLabels::AMENDMENT,
        };
        for count in [1, 2, 17] {
            for scale in [0.125, 1., 64.] {
                for offset in [-32., 0., 256.] {
                    let mut expected = vec![];
                    for i in 0..count {
                        expected.push(token.cuts.map(|v| (v + i as f64 * 32.) * scale + offset));
                    }
                    let fences = expected.iter().flatten().copied().collect::<Vec<_>>();
                    assert_eq!(
                        matching_frames(&token, &fences, token.extent * scale),
                        expected
                    );
                }
            }
        }
    }
    #[test]
    fn unequal_capacities_missing_fences_and_overflow_cannot_manufacture_a_tier() {
        let token = Token {
            cuts: [2., 7., 13.],
            extent: 16.,
            labels: ComparisonLabels::AMENDMENT,
        };
        assert!(matching_frames(&token, &[2., 7f64.next_up(), 13.], 16.).is_empty());
        assert!(matching_frames(&token, &[2., 13.], 16.).is_empty());
        assert!(matching_frames(&token, &[2., 7., 13.], f64::MAX).is_empty());
    }
}
