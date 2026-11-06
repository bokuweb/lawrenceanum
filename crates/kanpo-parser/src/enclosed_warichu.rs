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

/// Attach a consumed legacy occurrence only when the exact complete rendered
/// text proves its membership. Do not expand unrelated legacy proposals merely
/// because a different inline/marker root was accepted on the same page.
pub(super) fn expand_literal_legacy(
    regions: &mut [crate::trace::Region],
    words: &[crate::trace::Word],
    legacy: &[(usize, usize)],
    bounds: &[crate::trace::NativeBounds],
    axes: &[crate::native_word_axes::NativeAxis],
    certified_root: fn(&str) -> bool,
) {
    use std::collections::HashSet;
    if legacy.is_empty() || bounds.len()!=words.len() || axes.len()!=words.len() { return; }
    let compact = |text: &str| text.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    let mut owned = HashSet::new();
    for r in regions.iter() {
        for l in &r.lines {
            for &id in l.words.iter().chain(l.table.iter().flat_map(|t| t.cells.iter().flat_map(|c| &c.words))) {
                if id >= words.len() || words[id].id != id || !owned.insert(id) { return; }
            }
        }
    }
    let mut attach = |text: &str, ids: &mut Vec<usize>| {
        let mut expanded = Vec::new();
        for &root in ids.iter() {
            let mut members = vec![root];
            let mut at = 0;
            while at < members.len() {
                let owner = members[at];
                for &(id, parent) in legacy {
                    if parent == owner && !owned.contains(&id) && !members.contains(&id) {
                        if id >= words.len() || words[id].id != id || words[id].status != crate::trace::WordStatus::Used { return; }
                        members.push(id);
                    }
                }
                at += 1;
            }
            expanded.extend(members);
        }
        if expanded.len() == ids.len() || expanded.iter().copied().collect::<HashSet<_>>().len() != expanded.len() { return; }
        let Some(expanded)=ordered_source_members(&expanded,bounds,axes) else {return;};
        if !expanded.iter().filter(|id|ids.contains(id)).eq(ids.iter()) {return;}
        let rendered = compact(text);
        let mut rest = rendered.as_str();
        for (index, &id) in expanded.iter().enumerate() {
            let part = compact(&words[id].text);
            if part.is_empty() { return; }
            let next = rest.strip_prefix(&part).or_else(|| {
                // Existing legacy composition inserts a middle dot only at a
                // source-word boundary between split inline names.
                (index > 0).then(|| rest.strip_prefix('・')?.strip_prefix(&part)).flatten()
            });
            let Some(next) = next else { return; };
            rest = next;
        }
        if rest.is_empty() {
            owned.extend(expanded.iter().copied());
            *ids = expanded;
        }
    };
    for r in regions {
        for l in &mut r.lines {
            if let Some(table) = &mut l.table {
                for c in &mut table.cells { if certified_root(&c.text) {attach(&c.text, &mut c.words);} }
            } else if certified_root(&l.text) { attach(&l.text, &mut l.words); }
        }
    }
}

// Primary-axis successors surround a split inline name. Where their primary
// extents overlap, its parallel lanes read right-to-left (Down) or top-to-bottom
// (Right). Require one unique acyclic source order, independent of text and of
// the legacy merge's storage order.
fn ordered_source_members(ids:&[usize],bounds:&[crate::trace::NativeBounds],axes:&[crate::native_word_axes::NativeAxis])->Option<Vec<usize>> {
    use crate::{native_word_axes::NativeAxis,trace::NativeBounds};
    let axis=*axes.get(*ids.first()?)?;
    if !matches!(axis,NativeAxis::Down|NativeAxis::Right) {return None;}
    let primary=usize::from(axis==NativeAxis::Down);let cross=1-primary;
    let rects=ids.iter().map(|&id| {
        let NativeBounds::Glyphs(b)=*bounds.get(id)? else {return None;};
        (axes.get(id)==Some(&axis)&&b.iter().all(|v|v.is_finite())&&b[0]<b[2]&&b[1]<b[3]).then_some(b)
    }).collect::<Option<Vec<_>>>()?;
    let mut incoming=vec![0;ids.len()];let mut outgoing=vec![vec![];ids.len()];
    for i in 0..ids.len() {for j in i+1..ids.len() {
        let (a,b)=(rects[i],rects[j]);
        let before=if a[primary+2]<=b[primary] {true}
            else if b[primary+2]<=a[primary] {false}
            else if a[cross+2]<=b[cross] {axis==NativeAxis::Right}
            else if b[cross+2]<=a[cross] {axis==NativeAxis::Down}
            else {return None;};
        let (first,last)=if before {(i,j)}else{(j,i)};
        outgoing[first].push(last);incoming[last]+=1;
    }}
    let mut used=vec![false;ids.len()];let mut out=vec![];
    while out.len()<ids.len() {
        let ready=(0..ids.len()).filter(|&i|!used[i]&&incoming[i]==0).collect::<Vec<_>>();
        let [only]=ready.as_slice() else {return None;};
        used[*only]=true;out.push(ids[*only]);
        for &next in &outgoing[*only] {incoming[next]-=1;}
    }
    Some(out)
}

#[cfg(test)]
mod literal_legacy_tests {
    use super::{expand_literal_legacy,ordered_source_members};
    use crate::trace::{Line, Region, Role, Word, WordStatus,NativeBounds};
    use crate::native_word_axes::NativeAxis;
    fn words(parts: &[&str]) -> Vec<Word> {
        parts.iter().enumerate().map(|(id, text)| Word {
            id, x0: 0., y0: id as f32*2., x1: 1., y1: id as f32*2.+1., source_box: [0., id as f32*2., 1., id as f32*2.+1.],
            raw: (*text).into(), text: (*text).into(), status: WordStatus::Used,
        }).collect()
    }
    fn region(text: &str, ids: Vec<usize>) -> Vec<Region> {
        vec![Region { role: Role::Body, unit: 0, part: 0, lines: vec![Line::text(text.into(), ids)] }]
    }
    fn expand(r:&mut [Region],w:&[Word],edges:&[(usize,usize)]) {
        let b=w.iter().map(|w|NativeBounds::Glyphs(w.source_box)).collect::<Vec<_>>();
        expand_literal_legacy(r,w,edges,&b,&vec![NativeAxis::Down;w.len()],|_|true);
    }
    #[test]
    fn joint_heading_order_comes_from_source_lanes_independently_of_storage_and_name_length() {
        for count in [1,2,17,63] {
            let end=2.*count as f32+1.;
            let mut rects=vec![[10.,0.,11.,1.],[11.,2.,12.,end]];
            for i in 0..count {let y=2.+i as f32*2.;rects.push([9.,y,10.,y+1.]);}
            rects.push([10.,end+1.,11.,end+2.]);
            let expected=(0..rects.len()).collect::<Vec<_>>();
            for axis in [NativeAxis::Down,NativeAxis::Right] {
                for scale in [0.125,1.,64.] {
                    for offset in [-128.,0.,256.] {
                        let bounds=rects.iter().map(|&b| {
                            let b=if axis==NativeAxis::Right {[b[1],-b[2],b[3],-b[0]]}else{b};
                            NativeBounds::Glyphs(b.map(|v|v*scale+offset))
                        }).collect::<Vec<_>>();
                        let axes=vec![axis;rects.len()];
                        let mut ids=expected.clone();ids.reverse();
                        assert_eq!(ordered_source_members(&ids,&bounds,&axes),Some(expected.clone()));
                        ids.rotate_left(1);
                        assert_eq!(ordered_source_members(&ids,&bounds,&axes),Some(expected.clone()));
                    }
                }
            }
        }
    }
    #[test]
    fn overlapping_cyclic_and_unknown_source_lanes_do_not_choose_a_plausible_text_order() {
        let b=|v|NativeBounds::Glyphs(v);
        assert!(ordered_source_members(&[0,1],&[b([0.,0.,1.,1.]),b([0.,0.,1.,1.])],&[NativeAxis::Down;2]).is_none());
        assert!(ordered_source_members(&[0,1,2],&[b([20.,0.,21.,10.]),b([10.,0.,11.,1.]),b([30.,2.,31.,3.])],&[NativeAxis::Down;3]).is_none());
        assert!(ordered_source_members(&[0],&[NativeBounds::Unknown],&[NativeAxis::Down]).is_none());
        assert!(ordered_source_members(&[0],&[b([0.,0.,1.,1.])],&[NativeAxis::Unknown]).is_none());
    }
    #[test]
    fn original_split_joint_title_has_one_physical_source_order() {
        let input=crate::page_inputs(include_bytes!("../tests/data/native-role-prefix/20261015g002260018.pdf")).unwrap();
        let p=crate::analyze_item(&input).pages.remove(0);
        let ids=[57,56,59,60,61,58];
        assert_eq!(ordered_source_members(&ids,&p.native_bounds,&p.native_axes),Some(ids.to_vec()),"{:?}",ids.iter().map(|&i|(i,&p.words[i].text,&p.native_bounds[i],p.native_axes[i])).collect::<Vec<_>>());
        let mut regions=p.regions.clone();
        expand_literal_legacy(&mut regions,&p.words,&[(59,56),(60,56),(61,56)],&p.native_bounds,&p.native_axes,crate::shinkyu::complete_numbered_publication_heading);
        let l=regions.iter().flat_map(|r|&r.lines).find(|l|l.text=="〇デジタル庁・総務省告示第三十八号").unwrap();
        assert_eq!(l.words,ids);
    }
    #[test]
    fn complete_source_text_proves_consumed_members_without_an_inline_root() {
        for parts in [vec!["A", "β", "終"], vec!["alpha", "123", "x"], vec!["同", "同", "同"]] {
            let w = words(&parts);
            for joiner in ["", "・", "　・　"] {
                let text = parts.join(joiner);
                let mut r = region(&text, vec![0]);
                expand(&mut r, &w, &[(1,0),(2,1)]);
                assert_eq!(r[0].lines[0].words, [0,1,2]);
                assert_eq!(r[0].lines[0].text, text);
            }
        }
    }
    #[test]
    fn an_uncertified_legacy_root_cannot_attach_even_literal_matching_material() {
        let w=words(&["甲","乙","丙"]);
        let mut r=region("甲乙丙",vec![0]);
        let b=w.iter().map(|w|NativeBounds::Glyphs(w.source_box)).collect::<Vec<_>>();
        expand_literal_legacy(&mut r,&w,&[(1,0),(2,0)],&b,&[NativeAxis::Down;3],|_|false);
        assert_eq!(r[0].lines[0].words,[0]);
    }
    #[test]
    fn missing_reordered_or_foreign_material_cannot_borrow_consumed_members() {
        let w = words(&["甲", "乙", "丙"]);
        for text in ["甲", "甲乙", "乙甲丙", "甲丁丙", "甲乙丙丁", "甲・・乙丙"] {
            let mut r = region(text, vec![0]);
            expand(&mut r, &w, &[(1,0),(2,1)]);
            assert_eq!(r[0].lines[0].words, [0]);
        }
        let mut r = region("甲乙丙", vec![0]);
        r[0].lines.push(Line::text("乙".into(), vec![1]));
        expand(&mut r, &w, &[(1,0),(2,1)]);
        assert_eq!(r[0].lines[0].words, [0]);
        assert_eq!(r[0].lines[1].words, [1]);
    }
    #[test]
    fn invalid_ids_status_and_ownership_leave_the_original_lines_intact() {
        for mode in 0..4 {
            let mut w = words(&["甲", "乙"]);
            let mut r = region("甲乙", vec![0]);
            let mut edges = vec![(1,0)];
            match mode { 0=>w[1].id=3, 1=>w[1].status=WordStatus::Noise, 2=>edges[0].0=3, _=>r[0].lines.push(Line::text("甲".into(), vec![0])) }
            let original = r[0].lines.clone();
            expand(&mut r, &w, &edges);
            assert_eq!(r[0].lines, original);
        }
    }
}
