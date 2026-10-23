//! Recover unlabelled source-ruled comparison fields while preserving existing classifications.
use super::{band_pair_unit, page_lines, Col};
use crate::grid::Grid;
use crate::lines::PageRules;
use crate::trace::{Line, Unit};

#[derive(Clone, Copy)]
struct Bounds {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}
impl Bounds {
    fn of(cols: &[Col]) -> Option<Self> {
        (!cols.is_empty()).then(|| Self {
            left: cols.iter().map(|c| c.x).fold(f32::INFINITY, f32::min),
            top: cols.iter().map(|c| c.y).fold(f32::INFINITY, f32::min),
            right: cols.iter().map(|c| c.x1).fold(f32::NEG_INFINITY, f32::max),
            bottom: cols.iter().map(|c| c.y1).fold(f32::NEG_INFINITY, f32::max),
        })
    }
    fn contains(self, c: &Col, epsilon: f32) -> bool {
        c.x >= self.left - epsilon
            && c.x1 <= self.right + epsilon
            && c.y >= self.top - epsilon
            && c.y1 <= self.bottom + epsilon
    }
    // Horizontal rule endpoints are not vertical walls. A known vertical source
    // occurrence may overhang an endpoint in its advance box while its source
    // centerline remains inside. Every original member must satisfy this, and no
    // recorded vertical ruling may be crossed. Other axes retain full containment.
    fn contains_column(
        self,
        c: &Col,
        epsilon: f32,
        vertical: &std::collections::HashMap<usize, Vec<Bounds>>,
        rules: &PageRules,
    ) -> bool {
        if c.y < self.top - epsilon || c.y1 > self.bottom + epsilon {
            return false;
        }
        if self.contains(c, epsilon) {
            return true;
        }
        let Some(parts) = vertical.get(&c.id) else {
            return false;
        };
        parts.iter().all(|b| {
            let axis = (b.left + b.right) / 2.;
            self.left < axis
                && axis < self.right
                && !rules.vertical.iter().any(|v| {
                    b.left < v.x - epsilon
                        && v.x + epsilon < b.right
                        && b.top.max(v.y0) < b.bottom.min(v.y1)
                })
        })
    }

    fn overlaps(self, b: Self) -> bool {
        self.left.max(b.left) < self.right.min(b.right)
            && self.top.max(b.top) < self.bottom.min(b.bottom)
    }
    fn before(self, b: Self) -> bool {
        self.bottom <= b.top || (self.top < b.bottom && b.top < self.bottom && self.left >= b.right)
    }
}

// Accept a whole marker, not a substring of prose. Numbered/ranged omissions are
// publication grammar; the number and its length are not fixed to an example.
fn marker(text: &str) -> Option<bool> {
    let s: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let mut rest = s.as_str();
    let number = |s: &str| {
        s.chars()
            .take_while(|c| {
                "〇零一二三四五六七八九十百千万0123456789０１２３４５６７８９".contains(*c)
            })
            .map(char::len_utf8)
            .sum::<usize>()
    };
    let n = number(rest);
    if n > 0 {
        rest = &rest[n..];
        while let Some(ch) = rest.chars().next().filter(|c| "・〜～".contains(*c)) {
            rest = &rest[ch.len_utf8()..];
            let n = number(rest);
            if n == 0 {
                return None;
            }
            rest = &rest[n..];
        }
    }
    let inner = [('（', '）'), ('〔', '〕'), ('［', '］'), ('(', ')')]
        .into_iter()
        .find_map(|(a, b)| rest.strip_prefix(a)?.strip_suffix(b))?;
    match inner {
        "新設" | "削る" => Some(true),
        "略" | "同上" | "同左" => Some(false),
        _ => None,
    }
}

// A common native x interval, without the legacy column grouper's fixed-pt floor.
fn axis_groups(cols: &[Col]) -> Vec<Vec<Col>> {
    let mut cs = cols.to_vec();
    cs.sort_by(|a, b| {
        ((b.x + b.x1) / 2.)
            .total_cmp(&((a.x + a.x1) / 2.))
            .then(a.y.total_cmp(&b.y))
    });
    let mut out: Vec<Vec<Col>> = Vec::new();
    let (mut left, mut right) = (f32::INFINITY, f32::NEG_INFINITY);
    for c in cs {
        if left.max(c.x) >= right.min(c.x1) {
            out.push(Vec::new());
            left = c.x;
            right = c.x1
        } else {
            left = left.max(c.x);
            right = right.min(c.x1)
        }
        out.last_mut().unwrap().push(c);
    }
    for g in &mut out {
        g.sort_by(|a, b| a.y.total_cmp(&b.y));
    }
    out
}

fn marker_counterpart(a: &[Col], b: &[Col]) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    let ga = axis_groups(a);
    let gb = axis_groups(b);
    let text = |g: &Vec<Col>| g.iter().map(|c| c.text.as_str()).collect::<String>();
    let kinds: Option<Vec<_>> = gb.iter().map(|g| marker(&text(g))).collect();
    let Some(kinds) = kinds else { return false };
    if !kinds.into_iter().any(|k| k) || ga.iter().all(|g| marker(&text(g)).is_some()) {
        return false;
    }
    let mut used = std::collections::HashSet::new();
    for g in &gb {
        let bounds = Bounds::of(g).unwrap();
        let hits: Vec<_> = ga
            .iter()
            .enumerate()
            .filter(|(_, v)| {
                let r = Bounds::of(v).unwrap();
                r.left.max(bounds.left) < r.right.min(bounds.right)
            })
            .map(|(i, _)| i)
            .collect();
        if hits.len() != 1 || !used.insert(hits[0]) {
            return false;
        }
    }
    true
}

fn layout_with_source_axes(
    cols: &[Col],
    source_cols: &[Col],
    vertical: &std::collections::HashMap<usize, Vec<Bounds>>,
    rules: Option<&PageRules>,
    grids: &[Grid],
) -> Option<Vec<Unit>> {
    if cols.len() != source_cols.len() {
        return None;
    }
    let originals: std::collections::HashMap<_, _> = cols.iter().map(|c| (c.id, c)).collect();
    if originals.len() != cols.len() || source_cols.iter().any(|c| !originals.contains_key(&c.id)) {
        return None;
    }
    let scope = source_cols;
    let rules = rules?;
    if super::detect_shinkyu_header(cols).is_some() {
        return None;
    }
    let mut ids = std::collections::HashSet::new();
    if cols.is_empty()||cols.iter().any(|c|!ids.insert(c.id)||![c.x,c.x1,c.y,c.y1].iter().all(|v|v.is_finite())||c.x>=c.x1||c.y>=c.y1||c.text.chars().any(|ch|ch.is_control()||matches!(ch,'\u{e000}'..='\u{f8ff}'|'\u{f0000}'..='\u{ffffd}'|'\u{100000}'..='\u{10fffd}'))){return None}
    let em = scope
        .iter()
        .map(|c| c.x1 - c.x)
        .fold(f32::INFINITY, f32::min);
    let epsilon = em * 0.01;
    let mut raw: Vec<_> = rules
        .horizontal
        .iter()
        .filter(|h| [h.y, h.x0, h.x1].iter().all(|v| v.is_finite()) && h.x0 < h.x1)
        .copied()
        .collect();
    raw.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x0.total_cmp(&b.x0)));
    let mut hs: Vec<crate::lines::HRule> = Vec::new();
    for h in raw {
        if let Some(last) = hs.last_mut() {
            if (last.y - h.y).abs() <= epsilon && h.x0 <= last.x1 + epsilon {
                last.x1 = last.x1.max(h.x1);
                continue;
            }
        }
        hs.push(h)
    }
    let mut proposals = Vec::new();
    for (i, top) in hs.iter().enumerate() {
        let same: Vec<_> = hs[i..]
            .iter()
            .filter(|h| (h.x0 - top.x0).abs() <= epsilon && (h.x1 - top.x1).abs() <= epsilon)
            .collect();
        if same.len() < 3 {
            continue;
        }
        let (mid, bottom) = (same[1], same[2]);
        if ((mid.y - top.y) - (bottom.y - mid.y)).abs() > epsilon
            || mid.y <= top.y
            || bottom.y <= mid.y
        {
            continue;
        }
        let bounds = Bounds {
            left: top.x0.max(mid.x0).max(bottom.x0),
            right: top.x1.min(mid.x1).min(bottom.x1),
            top: top.y,
            bottom: bottom.y,
        };
        let touching: Vec<_> = scope
            .iter()
            .filter(|c| {
                c.x < bounds.right && c.x1 > bounds.left && c.y < bounds.bottom && c.y1 > bounds.top
            })
            .collect();
        if touching.iter().any(|c| {
            !bounds.contains_column(c, epsilon, vertical, rules)
                || (c.y < mid.y - epsilon && c.y1 > mid.y + epsilon)
        }) {
            continue;
        }
        if rules.vertical.iter().any(|v| {
            v.x > bounds.left + epsilon
                && v.x < bounds.right - epsilon
                && v.y0 <= bounds.top + epsilon
                && v.y1 >= bounds.bottom - epsilon
        }) {
            continue;
        }
        let (a, b): (Vec<_>, Vec<_>) = touching
            .into_iter()
            .cloned()
            .partition(|c| (c.y + c.y1) / 2. < mid.y);
        if !marker_counterpart(&a, &b) && !marker_counterpart(&b, &a) {
            continue;
        }
        proposals.push((bounds, a, b));
    }
    if proposals.is_empty() {
        return None;
    }
    let mut claimed = std::collections::HashSet::new();
    for (_, a, b) in &proposals {
        for c in a.iter().chain(b) {
            if !claimed.insert(c.id) {
                return None;
            }
        }
    }
    let rest: Vec<_> = cols
        .iter()
        .filter(|c| !claimed.contains(&c.id))
        .cloned()
        .collect();
    let mut atoms = Vec::new();
    for (bounds, a, b) in proposals {
        let a: Vec<_> = a.iter().map(|c| (*originals[&c.id]).clone()).collect();
        let b: Vec<_> = b.iter().map(|c| (*originals[&c.id]).clone()).collect();
        let expected: std::collections::HashMap<_, _> = a
            .iter()
            .map(|c| (c.id, true))
            .chain(b.iter().map(|c| (c.id, false)))
            .collect();
        let unit = band_pair_unit(a, b, grids, Some(rules));
        if pair_roots(&unit)? != expected {
            return None;
        }
        atoms.push((bounds, unit));
    }
    let source_rest: Vec<_> = scope
        .iter()
        .filter(|c| !claimed.contains(&c.id))
        .cloned()
        .collect();
    if let Some(bounds) = Bounds::of(&source_rest) {
        atoms.push((bounds, Unit::Plain(page_lines(rest))))
    }
    // Only a unique geometric order is accepted. No text, expected article count,
    // column-count bound or source-page identity participates in this ordering.
    let mut out = Vec::new();
    while !atoms.is_empty() {
        let eligible: Vec<_> = (0..atoms.len())
            .filter(|&i| {
                (0..atoms.len()).all(|j| {
                    i == j || (!atoms[i].0.overlaps(atoms[j].0) && atoms[i].0.before(atoms[j].0))
                })
            })
            .collect();
        if eligible.len() != 1 {
            return None;
        }
        out.push(atoms.remove(eligible[0]).1);
    }
    Some(out)
}

// Scope/ordering use complete native glyph occurrences, rather than derived
// working column rectangles (which can be shifted for vertical composition).
// Unknown explicit native geometry is never replaced by a guessed rectangle.
pub(super) struct NativeLayout {
    pub(super) units: Vec<Unit>,
    pub(super) members: Vec<(usize, usize)>,
}

pub(super) fn native_layout_with_axes(
    cols: &[Col],
    words: &[crate::trace::Word],
    native: &[crate::trace::NativeBounds],
    axes: &[crate::native_word_axes::NativeAxis],
    legacy: &[(usize, usize)],
    inline: &[(usize, usize)],
    rules: Option<&PageRules>,
    grids: &[Grid],
) -> Option<NativeLayout> {
    use crate::trace::NativeBounds;
    use std::collections::{HashMap, HashSet};
    if rules?.horizontal.len() < 3 {
        return None;
    }
    if axes.len() != words.len()
        || native.len() != words.len()
        || words.iter().enumerate().any(|(i, w)| w.id != i)
    {
        return None;
    }
    let mut parents = HashMap::new();
    let mut children: HashMap<usize, HashSet<usize>> = HashMap::new();
    for &(id, owner) in legacy.iter().chain(inline) {
        if id >= words.len() || owner >= words.len() || id == owner {
            return None;
        }
        if parents.insert(id, owner).is_some_and(|p| p != owner) {
            return None;
        }
        children.entry(owner).or_default().insert(id);
    }
    let mut used = HashSet::new();
    let mut source = Vec::new();
    let mut flat_members = Vec::new();
    let mut vertical = HashMap::new();
    for c in cols {
        let mut all_down = true;
        let mut parts = Vec::new();
        let mut queue = vec![c.id];
        let mut members = HashSet::new();
        let mut bb = [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ];
        while let Some(id) = queue.pop() {
            if !members.insert(id) || !used.insert(id) {
                return None;
            }
            let w = words.get(id)?;
            if w.status!=crate::trace::WordStatus::Used||w.text.trim().is_empty()||w.text.chars().any(|ch|ch.is_control()||matches!(ch,'\u{e000}'..='\u{f8ff}'|'\u{f0000}'..='\u{ffffd}'|'\u{100000}'..='\u{10fffd}')){return None}
            let NativeBounds::Glyphs(b) = native.get(id)? else {
                return None;
            };
            if !b.iter().all(|v| v.is_finite()) || b[0] >= b[2] || b[1] >= b[3] {
                return None;
            }
            all_down &= axes[id] == crate::native_word_axes::NativeAxis::Down;
            parts.push(Bounds {
                left: b[0],
                top: b[1],
                right: b[2],
                bottom: b[3],
            });
            bb = [
                bb[0].min(b[0]),
                bb[1].min(b[1]),
                bb[2].max(b[2]),
                bb[3].max(b[3]),
            ];
            if let Some(cs) = children.get(&id) {
                queue.extend(cs)
            }
        }
        // Preserve recorded inline-merge order, then recorded legacy-merge order.
        // Sets authorize membership only; their iteration cannot order sourceWords.
        let mut ordered = vec![c.id];
        loop {
            let before = ordered.len();
            for &(id, owner) in inline.iter().chain(legacy) {
                if ordered.contains(&owner) && !ordered.contains(&id) {
                    ordered.push(id)
                }
            }
            if ordered.len() == before {
                break;
            }
        }
        if ordered.len() != members.len() || ordered.iter().any(|id| !members.contains(id)) {
            return None;
        }
        flat_members.extend(ordered.into_iter().skip(1).map(|id| (id, c.id)));
        if all_down {
            vertical.insert(c.id, parts);
        }
        let mut physical = c.clone();
        physical.x = bb[0];
        physical.y = bb[1];
        physical.x1 = bb[2];
        physical.y1 = bb[3];
        source.push(physical);
    }
    Some(NativeLayout {
        units: layout_with_source_axes(cols, &source, &vertical, rules, grids)?,
        members: flat_members,
    })
}

fn line_ids(line: &Line) -> impl Iterator<Item = usize> + '_ {
    line.words.iter().copied().chain(
        line.table
            .iter()
            .flat_map(|t| t.cells.iter().flat_map(|c| c.words.iter().copied())),
    )
}
fn lines(unit: &Unit) -> Vec<&Line> {
    match unit {
        Unit::Plain(ls) => ls.iter().collect(),
        Unit::Shinkyu {
            preamble,
            after,
            before,
            outside,
        } => preamble
            .iter()
            .chain(after.iter().flatten())
            .chain(before.iter().flatten())
            .chain(outside.iter().flatten())
            .collect(),
    }
}
fn pair_roots(unit: &Unit) -> Option<std::collections::HashMap<usize, bool>> {
    let Unit::Shinkyu {
        preamble,
        after,
        before,
        outside,
    } = unit
    else {
        return None;
    };
    if preamble
        .iter()
        .chain(outside.iter().flatten())
        .any(|l| !l.text.trim().is_empty() || line_ids(l).next().is_some())
    {
        return None;
    }
    let mut roots = std::collections::HashMap::new();
    for (role, parts) in [(true, after), (false, before)] {
        let mut nonempty = false;
        for l in parts.iter().flatten() {
            if l.table.is_some() || (!l.text.trim().is_empty() && l.words.is_empty()) {
                return None;
            }
            for &id in &l.words {
                if roots.insert(id, role).is_some() {
                    return None;
                }
                nonempty = true;
            }
        }
        if !nonempty {
            return None;
        }
    }
    Some(roots)
}
// The legacy whole-page split classifies each working column by its center.
// Reject that inference only on an explicit role contradiction with a complete
// source-certified pair. Plain remainder and unsupported columns are not proof.
pub(super) fn contradicts_global_divider(plan: &NativeLayout, cols: &[Col], divider: f32) -> bool {
    plan.units.iter().filter_map(pair_roots).any(|roles| {
        cols.iter().any(|c| {
            roles
                .get(&c.id)
                .is_some_and(|&after| after != ((c.y + c.y1) / 2. < divider))
        })
    })
}

fn characters<'a>(texts: impl Iterator<Item = &'a str>) -> std::collections::BTreeMap<char, usize> {
    let mut out = std::collections::BTreeMap::new();
    for ch in texts.flat_map(str::chars).filter(|c| !c.is_whitespace()) {
        *out.entry(ch).or_insert(0) += 1
    }
    out
}

/// Upgrade complete, contiguous Plain blocks after the existing semantic passes.
/// Existing comparison units and Tables are never downgraded or reconstructed.
/// Current corrected text must be conserved before any computed field labels.
pub(super) fn apply(
    units: &mut Vec<Unit>,
    plan: NativeLayout,
    inline: &[(usize, usize)],
) -> Option<Vec<(usize, usize)>> {
    use std::collections::HashSet;
    let NativeLayout {
        units: proposals,
        members,
    } = plan;
    let mut accepted = HashSet::new();
    for candidate in proposals {
        let Some(roots) = pair_roots(&candidate) else {
            continue;
        };
        let mut frame: HashSet<_> = roots.keys().copied().collect();
        frame.extend(
            members
                .iter()
                .filter(|(_, owner)| roots.contains_key(owner))
                .map(|(id, _)| *id),
        );
        let mut removed = HashSet::new();
        let mut positions = Vec::new();
        let mut seen = HashSet::new();
        let mut old_text = Vec::new();
        let (mut token, mut conflict) = (0, false);
        for (ui, u) in units.iter().enumerate() {
            match u {
                Unit::Plain(ls) => {
                    for (li, l) in ls.iter().enumerate() {
                        let ids: Vec<_> = line_ids(l).collect();
                        if ids.iter().any(|id| frame.contains(id)) {
                            if l.table.is_some() || ids.iter().any(|id| !frame.contains(id)) {
                                conflict = true;
                                break;
                            }
                            for id in ids {
                                if !seen.insert(id) {
                                    conflict = true
                                }
                            }
                            removed.insert((ui, li));
                            positions.push(token);
                            old_text.push(l.text.as_str());
                        }
                        token += 1;
                    }
                }
                Unit::Shinkyu { .. } => {
                    if lines(u)
                        .iter()
                        .flat_map(|l| line_ids(l))
                        .any(|id| frame.contains(&id))
                    {
                        conflict = true
                    }
                    token += 1;
                }
            }
            if conflict {
                break;
            }
        }
        if conflict
            || positions.is_empty()
            || !positions.windows(2).all(|p| p[1] == p[0] + 1)
            || roots.keys().any(|id| !seen.contains(id))
        {
            continue;
        }
        if characters(old_text.into_iter())
            != characters(lines(&candidate).iter().map(|l| l.text.as_str()))
        {
            continue;
        }
        let affected: HashSet<_> = removed.iter().map(|p| p.0).collect();
        let mut insertion = Some(candidate);
        let old = std::mem::take(units);
        for (ui, u) in old.into_iter().enumerate() {
            match u {
                Unit::Plain(ls) if affected.contains(&ui) => {
                    let mut retained = Vec::new();
                    for (li, l) in ls.into_iter().enumerate() {
                        if removed.contains(&(ui, li)) {
                            if !retained.is_empty() {
                                units.push(Unit::Plain(std::mem::take(&mut retained)))
                            }
                            if let Some(pair) = insertion.take() {
                                units.push(pair)
                            }
                        } else {
                            retained.push(l)
                        }
                    }
                    if !retained.is_empty() {
                        units.push(Unit::Plain(retained))
                    }
                }
                other => units.push(other),
            }
        }
        accepted.extend(roots.keys().copied());
    }
    if accepted.is_empty() {
        return None;
    }
    let mut out: Vec<_> = inline
        .iter()
        .filter(|(_, owner)| !accepted.contains(owner))
        .copied()
        .collect();
    out.extend(
        members
            .into_iter()
            .filter(|(_, owner)| accepted.contains(owner)),
    );
    Some(out)
}

#[cfg(test)]
fn native_layout(
    cols: &[Col],
    words: &[crate::trace::Word],
    native: &[crate::trace::NativeBounds],
    legacy: &[(usize, usize)],
    inline: &[(usize, usize)],
    rules: Option<&PageRules>,
    grids: &[Grid],
) -> Option<NativeLayout> {
    native_layout_with_axes(
        cols,
        words,
        native,
        &vec![crate::native_word_axes::NativeAxis::Down; words.len()],
        legacy,
        inline,
        rules,
        grids,
    )
}
#[cfg(test)]
fn layout_with_source(
    cols: &[Col],
    source: &[Col],
    rules: Option<&PageRules>,
    grids: &[Grid],
) -> Option<Vec<Unit>> {
    layout_with_source_axes(
        cols,
        source,
        &std::collections::HashMap::new(),
        rules,
        grids,
    )
}
#[cfg(test)]
fn layout(cols: &[Col], rules: Option<&PageRules>, grids: &[Grid]) -> Option<Vec<Unit>> {
    layout_with_source(cols, cols, rules, grids)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(n: usize) -> (Vec<Col>, PageRules) {
        let mut cs = Vec::new();
        for i in 0..n {
            let x = 20. + i as f32 * 12.;
            cs.push(Col {
                id: i * 2,
                x,
                x1: x + 8.,
                y: 12.,
                y1: 44.,
                text: format!("第{}号の本文", i + 1),
            });
            cs.push(Col {
                id: i * 2 + 1,
                x,
                x1: x + 8.,
                y: 64.,
                y1: 80.,
                text: if i % 2 == 0 {
                    "（新設）".into()
                } else {
                    format!("{}（略）", i + 1)
                },
            });
        }
        let right = 20. + n as f32 * 12.;
        let rules = PageRules {
            horizontal: [0., 60., 120.]
                .into_iter()
                .map(|y| crate::lines::HRule {
                    y,
                    x0: 10.,
                    x1: right,
                })
                .collect(),
            ..PageRules::default()
        };
        (cs, rules)
    }
    #[test]
    fn complete_marker_grammar_has_no_character_count_limit() {
        for s in [
            "（新設）",
            "〔削る〕",
            "［略］",
            "１２３４５６７８９（略）",
            "百二十三・百二十四（略）",
            "二百〜三百〔略〕",
        ] {
            assert!(marker(s).is_some(), "{s}")
        }
        for s in [
            "新設道路",
            "（新設）の説明",
            "前文（削る）",
            "（新設〕",
            "一・（略）",
            "（略）後段",
            "（設立）",
        ] {
            assert!(marker(s).is_none(), "{s}")
        }
    }
    #[test]
    fn variable_columns_scaling_translation_and_input_order() {
        for n in [1, 2, 3, 7, 16] {
            for scale in [0.125, 1., 8.] {
                let (mut cs, mut r) = fixture(n);
                for c in &mut cs {
                    c.x = c.x * scale + 13.;
                    c.x1 = c.x1 * scale + 13.;
                    c.y = c.y * scale + 17.;
                    c.y1 = c.y1 * scale + 17.;
                }
                for h in &mut r.horizontal {
                    h.x0 = h.x0 * scale + 13.;
                    h.x1 = h.x1 * scale + 13.;
                    h.y = h.y * scale + 17.;
                }
                cs.reverse();
                assert!(layout(&cs, Some(&r), &[]).is_some(), "{n}/{scale}");
            }
        }
    }
    #[test]
    fn incomplete_source_rules_hold_the_existing_interpretation() {
        let (cs, mut r) = fixture(3);
        assert!(layout(&cs, None, &[]).is_none());
        r.horizontal.remove(1);
        assert!(layout(&cs, Some(&r), &[]).is_none());
    }
    #[test]
    fn a_partial_marker_or_an_unmatched_column_is_not_evidence() {
        let (mut cs, r) = fixture(3);
        cs[1].text = "（新設）による説明".into();
        assert!(layout(&cs, Some(&r), &[]).is_none());
        cs[1].text = "（新設）".into();
        cs[1].x += 6.;
        cs[1].x1 += 6.;
        assert!(layout(&cs, Some(&r), &[]).is_none());
    }
    #[test]
    fn crossing_words_duplicate_ids_and_nonfinite_geometry_are_held() {
        let (cs, r) = fixture(3);
        for mode in 0..3 {
            let mut c = cs.clone();
            match mode {
                0 => c[0].y1 = 65.,
                1 => c[1].id = c[0].id,
                _ => c[0].x = f32::NAN,
            };
            assert!(layout(&c, Some(&r), &[]).is_none());
        }
    }
    #[test]
    fn full_height_internal_frame_boundary_is_counterevidence() {
        let (cs, mut r) = fixture(3);
        r.vertical.push(crate::lines::VRule {
            x: 32.,
            y0: 0.,
            y1: 120.,
        });
        assert!(layout(&cs, Some(&r), &[]).is_none());
    }
    #[test]
    fn a_shifted_working_box_cannot_reassign_a_source_outside_column() {
        let (mut cs, r) = fixture(3);
        cs.push(Col {
            id: 6,
            x: 6.,
            x1: 14.,
            y: 20.,
            y1: 100.,
            text: "outside".into(),
        });
        let mut source = cs.clone();
        source.last_mut().unwrap().x = -2.;
        source.last_mut().unwrap().x1 = 6.;
        assert!(layout_with_source(&cs, &source, Some(&r), &[]).is_some());
        assert!(layout(&cs, Some(&r), &[]).is_none());
    }
    #[test]
    fn ordinary_parallel_prose_is_not_a_marker_band() {
        let (mut cs, r) = fixture(3);
        for c in cs.iter_mut().skip(1).step_by(2) {
            c.text = "新設道路の計画".into()
        }
        assert!(layout(&cs, Some(&r), &[]).is_none());
    }
    fn source_fixture() -> (
        Vec<Col>,
        Vec<crate::trace::Word>,
        Vec<crate::trace::NativeBounds>,
        PageRules,
    ) {
        let (cs, r) = fixture(3);
        let ws = cs
            .iter()
            .map(|c| crate::trace::Word {
                id: c.id,
                x0: c.x,
                y0: c.y,
                x1: c.x1,
                y1: c.y1,
                source_box: [c.x, c.y, c.x1, c.y1],
                raw: c.text.clone(),
                text: c.text.clone(),
                status: crate::trace::WordStatus::Used,
            })
            .collect();
        let native = cs
            .iter()
            .map(|c| crate::trace::NativeBounds::Glyphs([c.x, c.y, c.x1, c.y1]))
            .collect();
        (cs, ws, native, r)
    }
    fn add_member(
        ws: &mut Vec<crate::trace::Word>,
        native: &mut Vec<crate::trace::NativeBounds>,
        bb: [f32; 4],
    ) {
        let mut w = ws[0].clone();
        w.id = ws.len();
        w.x0 = bb[0];
        w.y0 = bb[1];
        w.x1 = bb[2];
        w.y1 = bb[3];
        w.source_box = bb;
        ws.push(w);
        native.push(crate::trace::NativeBounds::Glyphs(bb));
    }
    #[test]
    fn transitive_source_members_require_every_native_occurrence() {
        let (cs, mut ws, mut native, r) = source_fixture();
        add_member(&mut ws, &mut native, [20., 44., 28., 48.]);
        add_member(&mut ws, &mut native, [20., 48., 28., 52.]);
        let edges = [(6, 0), (7, 6)];
        assert!(native_layout(&cs, &ws, &native, &edges, &[], Some(&r), &[]).is_some());
        for unknown in [
            crate::trace::NativeBounds::Unknown,
            crate::trace::NativeBounds::Compatibility,
        ] {
            native[7] = unknown;
            assert!(native_layout(&cs, &ws, &native, &edges, &[], Some(&r), &[]).is_none());
        }
    }
    #[test]
    fn a_hidden_member_crossing_the_native_divider_is_held() {
        let (cs, mut ws, mut native, r) = source_fixture();
        add_member(&mut ws, &mut native, [20., 55., 28., 65.]);
        assert!(native_layout(&cs, &ws, &native, &[(6, 0)], &[], Some(&r), &[]).is_none());
    }
    #[test]
    fn missing_cyclic_shared_and_still_active_members_are_not_ownership_proof() {
        let (cs, mut ws, mut native, r) = source_fixture();
        add_member(&mut ws, &mut native, [20., 44., 28., 48.]);
        for edges in [
            vec![(77, 0)],
            vec![(6, 0), (0, 6)],
            vec![(6, 0), (6, 2)],
            vec![(5, 0)],
        ] {
            assert!(
                native_layout(&cs, &ws, &native, &edges, &[], Some(&r), &[]).is_none(),
                "{edges:?}"
            );
        }
    }
    #[test]
    fn native_scope_does_not_depend_on_working_column_translation() {
        let (mut cs, ws, native, r) = source_fixture();
        for c in &mut cs {
            c.x += 4096.;
            c.x1 += 4096.;
            c.y += 2048.;
            c.y1 += 2048.;
        }
        assert!(native_layout(&cs, &ws, &native, &[], &[], Some(&r), &[]).is_some());
    }
    #[test]
    fn every_merged_native_member_has_one_final_field_role() {
        let (cs, mut ws, mut native, r) = source_fixture();
        add_member(&mut ws, &mut native, [20., 44., 28., 48.]);
        add_member(&mut ws, &mut native, [20., 48., 28., 52.]);
        let edges = [(6, 0), (7, 6)];
        let proposal = native_layout(&cs, &ws, &native, &edges, &[], Some(&r), &[]).unwrap();
        let mut regions: Vec<_> = proposal
            .units
            .into_iter()
            .enumerate()
            .flat_map(|(i, u)| u.regions(i))
            .collect();
        super::super::enclosed_warichu::expand(&mut regions, &proposal.members, &edges);
        let mut roles = std::collections::BTreeMap::new();
        for r in regions {
            for l in r.lines {
                for id in l.words.iter().chain(
                    l.table
                        .iter()
                        .flat_map(|t| t.cells.iter().flat_map(|c| &c.words)),
                ) {
                    assert!(roles.insert(*id, r.role).is_none());
                }
            }
        }
        assert_eq!(
            roles.keys().copied().collect::<Vec<_>>(),
            (0..8).collect::<Vec<_>>()
        );
        for id in [0, 6, 7] {
            assert_eq!(roles[&id], crate::trace::Role::After);
        }
    }

    #[test]
    fn repeated_member_expansion_preserves_recorded_inline_then_legacy_order() {
        let (cs, mut ws, mut native, r) = source_fixture();
        for bb in [
            [20., 44., 28., 48.],
            [20., 48., 28., 52.],
            [20., 52., 28., 56.],
        ] {
            add_member(&mut ws, &mut native, bb)
        }
        for _ in 0..64 {
            let p = native_layout(
                &cs,
                &ws,
                &native,
                &[(6, 0), (7, 0)],
                &[(8, 0)],
                Some(&r),
                &[],
            )
            .unwrap();
            assert_eq!(p.members, vec![(8, 0), (6, 0), (7, 0)]);
        }
    }

    #[test]
    fn horizontal_endpoints_allow_only_known_vertical_source_centerlines() {
        use crate::native_word_axes::NativeAxis as Axis;
        use crate::trace::NativeBounds;
        for right in [false, true] {
            for fraction in [0.025, 0.125, 0.25] {
                for scale in [0.125, 1., 8.] {
                    let (mut cs, ws, mut native, mut r) = source_fixture();
                    let root = if right { 4 } else { 0 };
                    let left = if right {
                        56. + fraction * 8. - 8.
                    } else {
                        10. - fraction * 8.
                    };
                    for id in [root, root + 1] {
                        let NativeBounds::Glyphs(bb) = &mut native[id] else {
                            panic!()
                        };
                        bb[0] = left;
                        bb[2] = left + 8.;
                    }
                    for c in &mut cs {
                        c.x = c.x * scale + 13.;
                        c.x1 = c.x1 * scale + 13.;
                        c.y = c.y * scale + 17.;
                        c.y1 = c.y1 * scale + 17.;
                    }
                    for b in &mut native {
                        let NativeBounds::Glyphs(v) = b else { panic!() };
                        v[0] = v[0] * scale + 13.;
                        v[2] = v[2] * scale + 13.;
                        v[1] = v[1] * scale + 17.;
                        v[3] = v[3] * scale + 17.;
                    }
                    for h in &mut r.horizontal {
                        h.x0 = h.x0 * scale + 13.;
                        h.x1 = h.x1 * scale + 13.;
                        h.y = h.y * scale + 17.;
                    }
                    let axes = vec![Axis::Down; ws.len()];
                    assert!(
                        native_layout_with_axes(&cs, &ws, &native, &axes, &[], &[], Some(&r), &[])
                            .is_some(),
                        "side{right}/fraction{fraction}/scale{scale}"
                    );
                    for axis in [
                        Axis::Right,
                        Axis::Left,
                        Axis::Up,
                        Axis::Unknown,
                        Axis::Compatibility,
                    ] {
                        let mut axes = axes.clone();
                        axes[root] = axis;
                        axes[root + 1] = axis;
                        assert!(
                            native_layout_with_axes(
                                &cs,
                                &ws,
                                &native,
                                &axes,
                                &[],
                                &[],
                                Some(&r),
                                &[]
                            )
                            .is_none(),
                            "{axis:?}"
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn a_real_vertical_ruling_keeps_its_boundary_for_vertical_source_boxes() {
        use crate::trace::NativeBounds;
        for right in [false, true] {
            let (cs, ws, mut native, mut r) = source_fixture();
            let root = if right { 4 } else { 0 };
            let (left, edge) = if right { (49., 56.) } else { (9., 10.) };
            for id in [root, root + 1] {
                let NativeBounds::Glyphs(b) = &mut native[id] else {
                    panic!()
                };
                b[0] = left;
                b[2] = left + 8.;
            }
            assert!(native_layout(&cs, &ws, &native, &[], &[], Some(&r), &[]).is_some());
            r.vertical.push(crate::lines::VRule {
                x: edge,
                y0: 0.,
                y1: 120.,
            });
            assert!(native_layout(&cs, &ws, &native, &[], &[], Some(&r), &[]).is_none());
        }
    }
    #[test]
    fn every_original_member_must_have_a_proved_vertical_side_assignment() {
        use crate::native_word_axes::NativeAxis as Axis;
        use crate::trace::NativeBounds;
        for (bb, axis) in [
            ([5., 44., 9., 48.], Axis::Down),
            ([12., 44., 16., 48.], Axis::Right),
            ([12., 44., 16., 48.], Axis::Unknown),
        ] {
            let (cs, mut ws, mut native, r) = source_fixture();
            for id in [0, 1] {
                let NativeBounds::Glyphs(b) = &mut native[id] else {
                    panic!()
                };
                b[0] = 9.;
                b[2] = 17.;
            }
            add_member(&mut ws, &mut native, bb);
            let mut axes = vec![Axis::Down; ws.len()];
            axes[6] = axis;
            assert!(native_layout_with_axes(
                &cs,
                &ws,
                &native,
                &axes,
                &[(6, 0)],
                &[],
                Some(&r),
                &[]
            )
            .is_none());
        }
    }
}

#[cfg(test)]
#[path = "ruled_marker_overlay_tests.rs"]
mod overlay_tests;
