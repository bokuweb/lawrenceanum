// Reconcile logical row cuts with complete sentence scope inside native cells.
// No publication IDs, body vocabulary, size/count caps or guessed source axes.
pub(crate) fn source_plan(table: &Table, source: Source<'_>) -> Option<Table> {
    use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
    type R = [f32; 4];
    fn valid(b: R) -> bool {
        b.iter().all(|x| x.is_finite()) && b[0] < b[2] && b[1] < b[3]
    }
    fn inside(a: R, b: R) -> bool {
        a[0] <= b[0] && a[1] <= b[1] && b[2] <= a[2] && b[3] <= a[3]
    }
    fn overlap(a: R, b: R) -> bool {
        a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
    }
    fn union(a: R, b: R) -> R {
        [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[2].max(b[2]),
            a[3].max(b[3]),
        ]
    }
    fn outer(g: &Grid) -> Option<R> {
        if g.xs.len() < 2 || g.ys.len() < 2 {
            return None;
        }
        Some([g.left(), g.top(), g.right(), g.bottom()])
    }
    fn cr(g: &Grid, i: usize) -> R {
        let c = g.cells[i];
        [g.xs[c.x0], g.ys[c.y0], g.xs[c.x1], g.ys[c.y1]]
    }
    fn tr(t: &Table) -> R {
        [t.x0, t.y0, t.x1, t.y1]
    }
    fn ccr(c: &TableCell) -> R {
        [c.x0, c.y0, c.x1, c.y1]
    }
    fn material(s: &str) -> BTreeMap<char, usize> {
        let mut a = BTreeMap::new();
        for c in s.chars().filter(|c| !c.is_whitespace()) {
            *a.entry(c).or_default() += 1;
        }
        a
    }
    fn numeric(s: &str) -> bool {
        let chars: Vec<_> = s.chars().filter(|c| !c.is_whitespace()).collect();
        chars
            .iter()
            .any(|c| c.is_ascii_digit() || ('０'..='９').contains(c))
            && chars.iter().all(|c| {
                c.is_ascii_digit()
                    || ('０'..='９').contains(c)
                    || ".,，．＋+−－-△▲()（）%％/／:：".contains(*c)
            })
    }
    // Repeated balanced parenthetical prefixes label separate field values.
    // They cannot borrow a later field's terminal to form one prose sentence.
    fn field_prefix(s: &str) -> bool {
        let s = s.trim_start();
        let pairs = [('(', ')'), ('（', '）'), ('[', ']'), ('［', '］')];
        let Some(first) = s.chars().next() else {
            return false;
        };
        let Some(&(_, end)) = pairs.iter().find(|p| p.0 == first) else {
            return false;
        };
        let mut stack = vec![end];
        let mut content = false;
        for (offset, ch) in s.char_indices().skip(1) {
            if let Some(&(_, end)) = pairs.iter().find(|p| p.0 == ch) {
                stack.push(end);
            } else if pairs.iter().any(|p| p.1 == ch) {
                if stack.pop() != Some(ch) {
                    return false;
                }
                if stack.is_empty() {
                    return content && !s[offset + ch.len_utf8()..].trim().is_empty();
                }
            } else if !ch.is_whitespace() {
                content = true;
            }
        }
        false
    }
    // An ordinal labels a section only when it has a non-scalar body.
    fn numbered_prefix(s: &str) -> bool {
        let s = s.trim_start();
        if numeric(s) { return false; }
        let rest = if let Some(s) = s.strip_prefix('(') { Some((s, ')')) }
            else { s.strip_prefix('（').map(|s| (s, '）')) };
        let Some((s, close)) = rest else { return false };
        let Some((ordinal, body)) = s.split_once(close) else { return false };
        let ds: Vec<_> = ordinal.chars().filter(|c| !c.is_whitespace()).collect();
        !ds.is_empty() && ds.iter().all(|c| c.is_ascii_digit() || ('０'..='９').contains(c))
            && !body.trim().is_empty() && !numeric(body)
    }
    fn parenthetical_scalar(s: &str) -> bool {
        let s = s.trim();
        if !numeric(s) { return false; }
        let mut stack = Vec::new();
        for (offset, c) in s.char_indices() {
            match c {
                '(' => stack.push(')'), '（' => stack.push('）'),
                ')' | '）' => {
                    if stack.pop() != Some(c) { return false; }
                    if stack.is_empty() { return offset + c.len_utf8() == s.len(); }
                }
                _ if stack.is_empty() => return false,
                _ => {}
            }
        }
        false
    }
    fn band(ids: &[usize], b: &HashMap<usize, R>) -> Option<(f32, f32)> {
        let mut lo = f32::NEG_INFINITY;
        let mut hi = f32::INFINITY;
        if ids.is_empty() {
            return None;
        }
        for id in ids {
            let r = b.get(id)?;
            lo = lo.max(r[1]);
            hi = hi.min(r[3]);
        }
        (lo < hi).then_some((lo, hi))
    }
    fn envelope(ids: &[usize], b: &HashMap<usize, R>) -> Option<R> {
        ids.iter()
            .map(|id| b.get(id).copied())
            .collect::<Option<Vec<_>>>()?
            .into_iter()
            .reduce(union)
    }
    fn intersects(a: (f32, f32), b: (f32, f32)) -> bool {
        a.0 < b.1 && b.0 < a.1
    }
    fn root(parents: &[usize], mut i: usize) -> usize {
        while parents[i] != i {
            i = parents[i];
        }
        i
    }
    // Only complete top-level Japanese sentence scopes authorize a coalescence.
    // Periods in quotations/parentheses do not end the containing sentence.
    fn sentences(parts: &[usize], t: &Table, text: &HashMap<usize, &Col>) -> Option<Vec<usize>> {
        let pairs = [
            ('(', ')'),
            ('（', '）'),
            ('[', ']'),
            ('［', '］'),
            ('{', '}'),
            ('｛', '｝'),
            ('「', '」'),
            ('『', '』'),
            ('【', '】'),
            ('〈', '〉'),
            ('《', '》'),
        ];
        let mut parents: Vec<_> = (0..parts.len()).collect();
        let mut stack = Vec::new();
        let mut pending = BTreeSet::new();
        let mut complete = false;
        for (k, &ci) in parts.iter().enumerate() {
            for id in &t.cells[ci].words {
                for ch in text.get(id)?.text.chars() {
                    if ch.is_whitespace() {
                        continue;
                    }
                    pending.insert(k);
                    if let Some(&(_, end)) = pairs.iter().find(|p| p.0 == ch) {
                        stack.push(end);
                    } else if pairs.iter().any(|p| p.1 == ch) {
                        if stack.pop() != Some(ch) {
                            return None;
                        }
                    }
                    if ch == '。' && stack.is_empty() {
                        let first = *pending.first()?;
                        for &other in &pending {
                            let a = root(&parents, first);
                            let b = root(&parents, other);
                            parents[b] = a;
                        }
                        pending.clear();
                        complete = true;
                    }
                }
            }
        }
        if !complete || !stack.is_empty() || !pending.is_empty() {
            return None;
        }
        for i in 0..parents.len() {
            parents[i] = root(&parents, i);
        }
        Some(parents)
    }
    fn paragraph_grammar(lines: &[Vec<usize>], text: &HashMap<usize, &Col>) -> bool {
        let pairs = [
            ('(', ')'),
            ('（', '）'),
            ('[', ']'),
            ('［', '］'),
            ('{', '}'),
            ('｛', '｝'),
            ('「', '」'),
            ('『', '』'),
            ('【', '】'),
            ('〈', '〉'),
            ('《', '》'),
        ];
        let mut stack = Vec::new();
        for (k, line) in lines.iter().enumerate() {
            let s: String = line.iter().map(|id| text[id].text.as_str()).collect();
            let n = s
                .trim_start()
                .chars()
                .take_while(|c| {
                    c.is_ascii_digit()
                        || ('０'..='９').contains(c)
                        || "〇零一二三四五六七八九十百千万億兆".contains(*c)
                })
                .count();
            let item = n > 0
                && s.trim_start()
                    .chars()
                    .nth(n)
                    .is_some_and(|c| c.is_whitespace() || "、.．)）〜～".contains(c));
            if numeric(&s) || (k > 0 && stack.is_empty() && (field_prefix(&s) || item)) {
                return false;
            }
            let mut tail = None;
            for ch in s.chars().filter(|c| !c.is_whitespace()) {
                if let Some(&(_, end)) = pairs.iter().find(|p| p.0 == ch) {
                    stack.push(end);
                } else if pairs.iter().any(|p| p.1 == ch) {
                    if stack.pop() != Some(ch) {
                        return false;
                    }
                }
                tail = Some(ch);
            }
            if k + 1 < lines.len()
                && stack.is_empty()
                && tail.is_some_and(|c| "。.!?！？".contains(c))
            {
                return false;
            }
        }
        stack.is_empty()
    }
    fn glue(a: &str, b: &str, connected: bool) -> String {
        let mut s = a.to_owned();
        if !a.is_empty() && !b.is_empty() {
            let x = a.chars().last().unwrap();
            let y = b.chars().next().unwrap();
            if !connected || x.is_ascii_alphabetic() && y.is_ascii_alphabetic() {
                s.push('\n');
            }
        }
        s.push_str(b);
        s
    }
    if (!table.cells.iter().any(|c| c.text.contains('。')) && source.flows.is_none())
        || !table.cells.iter().enumerate().any(|(i, a)| {
            table.cells[i + 1..]
                .iter()
                .any(|b| ccr(a) == ccr(b) && a.col == b.col && a.colspan == b.colspan)
        })
    {
        return None;
    }
    if source
        .rules
        .horizontal
        .iter()
        .any(|h| ![h.y, h.x0, h.x1].iter().all(|x| x.is_finite()) || h.x0 > h.x1)
        || source
            .rules
            .vertical
            .iter()
            .any(|v| ![v.x, v.y0, v.y1].iter().all(|x| x.is_finite()) || v.y0 > v.y1)
    {
        return None;
    }
    if source.words.len() != source.bounds.len()
        || source.words.len() != source.axes.len()
        || source.words.len() != source.rulings.len()
        || table.rows == 0
        || table.cols == 0
        || !valid(tr(table))
    {
        return None;
    }
    let targets: Vec<_> = source
        .grids
        .iter()
        .enumerate()
        .filter(|(_, g)| outer(g) == Some(tr(table)))
        .collect();
    let [(gi, g)] = targets.as_slice() else {
        return None;
    };
    let gi = *gi;
    let g = *g;
    if !crate::ruled_cell_flow::partition(g) || table.cols != g.xs.len() - 1 {
        return None;
    }
    for (j, other) in source.grids.iter().enumerate() {
        if j == gi {
            continue;
        }
        let b = outer(other)?;
        if overlap(tr(table), b) && (inside(tr(table), b) || !inside(b, tr(table))) {
            return None;
        }
    }
    let mut originals = HashMap::new();
    for w in source.originals {
        if source.words.get(w.id).is_none_or(|s| s.id != w.id)
            || originals.insert(w.id, w).is_some()
        {
            return None;
        }
    }
    let mut physical = Vec::new();
    let mut groups = vec![vec![]; g.cells.len()];
    let mut owned = HashMap::new();
    let mut boxes = HashMap::new();
    let mut native_owners = HashMap::new();
    for (i, w) in source.words.iter().enumerate() {
        if w.id != i {
            return None;
        }
        if w.status != WordStatus::Used {
            continue;
        }
        let NativeBounds::Glyphs(b) = source.bounds[i] else {
            return None;
        };
        if !valid(b) {
            return None;
        }
        if !overlap(tr(table), b) {
            continue;
        }
        let owners: Vec<_> = (0..g.cells.len())
            .filter(|&j| inside(cr(g, j), b))
            .collect();
        if owners.len() != 1 || source.axes[i] != NativeAxis::Right {
            return None;
        }
        let c = originals.get(&i)?;
        if c.text != w.text
            || w.text.trim().is_empty()
            || [c.x, c.y, c.x1, c.y1] != w.source_box
            || [w.x0, w.y0, w.x1, w.y1] != w.source_box
            || !valid(w.source_box)
        {
            return None;
        }
        native_owners.insert(i, owners[0]);
        boxes.insert(i, b);
    }
    // Unemitted graphics are not lexical sentence tokens. A CFF-proved line
    // may be an overlay on sparse prose, but a value-row counterpart remains
    // opaque; its absence must not collapse a genuine record.
    for (i, w) in source.words.iter().enumerate() {
        if w.status == WordStatus::Used
            || w.raw.trim().is_empty()
                && w.text.trim().is_empty()
                && source.rulings[i].rect().is_none()
        {
            continue;
        }
        let NativeBounds::Glyphs(b) = source.bounds[i] else {
            if w.status != WordStatus::Margin {
                return None;
            } else {
                continue;
            }
        };
        if !overlap(tr(table), b) {
            continue;
        }
        let ps: Vec<_> = (0..g.cells.len())
            .filter(|&p| inside(cr(g, p), b))
            .collect();
        let [p] = ps.as_slice() else { return None };
        let p = *p;
        let ink = source.rulings[i].rect()?;
        if !source.rulings[i].matches_axis(source.axes[i]) {
            return None;
        }
        let cell = cr(g, p);
        if f64::from(cell[0]) > ink[0]
            || f64::from(cell[1]) > ink[1]
            || ink[2] > f64::from(cell[2])
            || ink[3] > f64::from(cell[3])
        {
            return None;
        }
        let on_text = boxes
            .iter()
            .any(|(id, other)| native_owners[id] == p && overlap(b, *other));
        let at_record = boxes.iter().any(|(id, other)| {
            native_owners[id] != p && ink[1] < f64::from(other[3]) && f64::from(other[1]) < ink[3]
        });
        if !on_text || at_record {
            return None;
        }
    }

    for (i, c) in table.cells.iter().enumerate() {
        if c.col >= table.cols
            || c.colspan == 0
            || c.col + c.colspan > table.cols
            || c.rowspan == 0
            || c.row.checked_add(c.rowspan)? > table.rows
            || !valid(ccr(c))
        {
            return None;
        }
        let ps: Vec<_> = (0..g.cells.len())
            .filter(|&p| {
                ccr(c) == cr(g, p) && c.col == g.cells[p].x0 && c.col + c.colspan == g.cells[p].x1
            })
            .collect();
        let [p] = ps.as_slice() else { return None };
        let p = *p;
        physical.push(p);
        groups[p].push(i);
        let mut text = String::new();
        for id in &c.words {
            if !boxes.contains_key(id)
                || !inside(cr(g, p), boxes[id])
                || owned.insert(*id, i).is_some()
            {
                return None;
            }
            text.push_str(&originals[id].text);
        }
        if !text
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .eq(c.text.chars().filter(|ch| !ch.is_whitespace()))
        {
            return None;
        }
    }
    if owned.len() != boxes.len() || boxes.keys().any(|id| !owned.contains_key(id)) {
        return None;
    }
    let mut envs = BTreeMap::new();
    for (&id, &b) in &boxes {
        let p = physical[owned[&id]];
        envs.entry(p).and_modify(|a| *a = union(*a, b)).or_insert(b);
    }
    let occupied: Vec<_> = envs.keys().copied().collect();
    for (i, &a) in occupied.iter().enumerate() {
        for &b in &occupied[i + 1..] {
            if !crate::ruled_cell_flow::fence(envs[&a], envs[&b], cr(g, a), cr(g, b), source.rules)
            {
                return None;
            }
        }
    }
    // A derived grid cannot hide a continuous original stroke separating
    // occupied parts of the same claimed physical cell. Near-border averaged
    // coordinates do not create a cut without source glyphs on both sides.
    for &p in &occupied {
        let c = cr(g, p);
        let bs: Vec<_> = boxes
            .iter()
            .filter(|(id, _)| native_owners[id] == p)
            .map(|(_, b)| *b)
            .collect();
        for h in &source.rules.horizontal {
            if c[1] < h.y
                && h.y < c[3]
                && bs.iter().any(|b| b[3] <= h.y)
                && bs.iter().any(|b| h.y <= b[1])
                && crate::ruled_cell_flow::cover(
                    source
                        .rules
                        .horizontal
                        .iter()
                        .filter(|q| q.y == h.y)
                        .map(|q| (q.x0, q.x1))
                        .collect(),
                    c[0],
                    c[2],
                )
            {
                return None;
            }
        }
        for v in &source.rules.vertical {
            if c[0] < v.x
                && v.x < c[2]
                && bs.iter().any(|b| b[2] <= v.x)
                && bs.iter().any(|b| v.x <= b[0])
                && crate::ruled_cell_flow::cover(
                    source
                        .rules
                        .vertical
                        .iter()
                        .filter(|q| q.x == v.x)
                        .map(|q| (q.y0, q.y1))
                        .collect(),
                    c[1],
                    c[3],
                )
            {
                return None;
            }
        }
    }
    // Cells that genuinely span physical rows connect those rows. Their real
    // source row boundaries stay protected while the sentence gains a rowspan.
    let mut native_rows: Vec<_> = (0..g.ys.len() - 1).collect();
    for c in &g.cells {
        for y in c.y0 + 1..c.y1 {
            let a = root(&native_rows, c.y0);
            let b = root(&native_rows, y);
            native_rows[b] = a;
        }
    }
    let native_keys: Vec<_> = g.cells.iter().map(|c| root(&native_rows, c.y0)).collect();
    let mut ranges = BTreeMap::new();
    for (p, parts) in groups.iter_mut().enumerate() {
        parts.sort_by_key(|&i| table.cells[i].row);
        if parts
            .windows(2)
            .any(|w| table.cells[w[0]].row + table.cells[w[0]].rowspan > table.cells[w[1]].row)
        {
            return None;
        }
        let ids: Vec<_> = parts
            .iter()
            .flat_map(|&i| table.cells[i].words.iter().copied())
            .collect();
        let set: HashSet<_> = ids.iter().copied().collect();
        let cols: Vec<_> = source
            .originals
            .iter()
            .filter(|c| set.contains(&c.id))
            .cloned()
            .collect();
        let canonical: Vec<_> = crate::horizontal::row_lines(cols)
            .iter()
            .flat_map(|l| l.words.iter().copied())
            .collect();
        if ids != canonical {
            return None;
        }
        for &i in parts.iter() {
            let c = &table.cells[i];
            ranges
                .entry(native_keys[p])
                .and_modify(|a: &mut (usize, usize)| {
                    a.0 = a.0.min(c.row);
                    a.1 = a.1.max(c.row + c.rowspan);
                })
                .or_insert((c.row, c.row + c.rowspan));
        }
    }
    let rs: Vec<_> = ranges.values().copied().collect();
    if rs.first()?.0 != 0 || rs.last()?.1 != table.rows || rs.windows(2).any(|w| w[0].1 != w[1].0) {
        return None;
    }
    // Native single-row correspondences or scalar value slots support retaining
    // real logical records. A long shared sentence may span those records.
    let mut records = BTreeMap::new();
    let mut data_columns = HashSet::new();
    let mut scalar_columns = HashSet::new();
    let mut section_columns = HashSet::new();
    for (&r, &(lo, hi)) in &ranges {
        if hi - lo < 2 {
            records.insert(r, false);
            continue;
        }
        let fields: Vec<_> = (0..g.cells.len())
            .filter(|&p| native_keys[p] == r)
            .collect();
        let at = |p: usize, row: usize| {
            groups[p]
                .iter()
                .copied()
                .find(|&i| table.cells[i].row == row && table.cells[i].rowspan == 1)
        };
        let mut paired = false;
        for (a, &p) in fields.iter().enumerate() {
            for &q in &fields[a + 1..] {
                let mut previous = f32::NEG_INFINITY;
                let mut valid = true;
                for row in lo..hi {
                    let Some((i, j)) = at(p, row).zip(at(q, row)) else {
                        valid = false;
                        break;
                    };
                    let Some((a, b)) = band(&table.cells[i].words, &boxes)
                        .zip(band(&table.cells[j].words, &boxes))
                    else {
                        valid = false;
                        break;
                    };
                    let v = (a.0.max(b.0), a.1.min(b.1));
                    if v.0 >= v.1 || v.0 < previous {
                        valid = false;
                        break;
                    }
                    previous = v.1;
                }
                paired |= valid;
            }
        }
        for &p in &fields {
            let mut previous = f32::NEG_INFINITY;
            let mut observed_value = false;
            for row in lo..hi {
                let Some(i) = at(p, row) else { continue };
                let c = &table.cells[i];
                let mut values = Vec::new();
                if numeric(&c.text) {
                    if let Some(b) = envelope(&c.words, &boxes) {
                        values.push((b[1], b[3]));
                    }
                } else {
                    let set: HashSet<_> = c.words.iter().copied().collect();
                    let cols = source
                        .originals
                        .iter()
                        .filter(|w| set.contains(&w.id))
                        .cloned()
                        .collect();
                    for line in crate::horizontal::row_lines(cols) {
                        if numeric(&line.text) {
                            if let Some(b) = band(&line.words, &boxes) {
                                values.push(b);
                            }
                        }
                    }
                }
                values.retain(|&v| {
                    v.0 >= previous
                        && fields.iter().any(|&q| {
                            q != p
                                && at(q, row).is_some_and(|j| {
                                    table.cells[j]
                                        .words
                                        .iter()
                                        .any(|id| intersects(v, (boxes[id][1], boxes[id][3])))
                                })
                        })
                });
                values.sort_by(|a, b| a.1.total_cmp(&b.1));
                if let Some(v) = values.first() {
                    observed_value = true;
                    previous = v.1;
                }
            }
            let sectioned = groups[p].len() > 1
                && groups[p]
                    .iter()
                    .all(|&i| field_prefix(&table.cells[i].text));
            if observed_value {
                scalar_columns.insert(p);
            }
            if sectioned {
                section_columns.insert(p);
            }
            if observed_value || sectioned {
                data_columns.insert(p);
            }
        }
        records.insert(
            r,
            native_rows
                .iter()
                .enumerate()
                .filter(|(i, _)| root(&native_rows, *i) == r)
                .count()
                > 1
                || paired
                || fields.iter().any(|p| data_columns.contains(p)),
        );
    }
    let mut components: Vec<Vec<usize>> = groups.iter().map(|ps| (0..ps.len()).collect()).collect();
    let mut collapse = BTreeSet::new();
    let mut changed = false;
    // A fully source-wrapped field can own several parallel explanatory
    // sections. Exact native baselines distinguish atomic value rows from
    // multi-line fragments whose font rectangles merely overlap.
    if let Some(flows) = source
        .flows
        .filter(|flows| flows.len() == source.words.len())
    {
        for (&r, &(lo, hi)) in &ranges {
            if hi - lo < 2
                || native_rows
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| root(&native_rows, *i) == r)
                    .count()
                    != 1
            {
                continue;
            }
            let fields: Vec<_> = (0..g.cells.len())
                .filter(|&p| native_keys[p] == r)
                .collect();
            if fields
                .iter()
                .flat_map(|&p| groups[p].iter())
                .flat_map(|&i| table.cells[i].words.iter())
                .any(|&id| {
                    flows
                        .get(id)
                        .copied()
                        .flatten()
                        .is_none_or(|f| !f.belongs(boxes[&id]))
                })
            {
                continue;
            }
            // A separately wrapped numeric parenthesis can remain subordinate
            // to explicit ordinal sections when every original glyph starts
            // strictly inside its section's source margin. Bare scalar records,
            // unnumbered labels and aligned value heads do not gain this proof.
            let mut ordinal_sections = HashSet::new();
            for &p in &fields {
                let mut margin = None;
                let mut heads = 0;
                let mut continuation = false;
                let mut valid = true;
                for &i in &groups[p] {
                    let c = &table.cells[i];
                    let Some(first) = c.words.first().and_then(|&id| flows[id]) else {
                        valid = false;
                        break;
                    };
                    let head: String = c.words.iter()
                        .take_while(|&&id| flows[id].unwrap().baseline == first.baseline)
                        .map(|id| originals[id].text.as_str()).collect();
                    if numbered_prefix(&head) {
                        margin = Some(first.start);
                        heads += 1;
                    } else if parenthetical_scalar(&c.text)
                        && margin.is_some_and(|left| c.words.iter().all(|&id| flows[id].unwrap().start > left)) {
                        continuation = true;
                    } else { valid = false; break; }
                }
                if valid && heads >= 2 && continuation { ordinal_sections.insert(p); }
            }
            // A numeric continuation inside a labelled section is not a
            // second value record. A numeric section head still is. Use the
            // exact first source baseline, without guessing a line from boxes.
            let independent_scalar = |p: usize| {
                scalar_columns.contains(&p) && !ordinal_sections.contains(&p) && (!section_columns.contains(&p)
                    || groups[p].iter().any(|&i| {
                        let ids = &table.cells[i].words;
                        let first = flows[ids[0]].unwrap().baseline;
                        let head: String = ids.iter()
                            .take_while(|&&id| flows[id].unwrap().baseline == first)
                            .map(|id| originals[id].text.as_str()).collect();
                        numeric(&head)
                    }))
            };
            let mut atomic = HashSet::new();
            for &p in &fields {
                let mut previous = f64::NEG_INFINITY;
                let mut valid = true;
                for row in lo..hi {
                    let Some(i) = groups[p]
                        .iter()
                        .copied()
                        .find(|&i| table.cells[i].row == row && table.cells[i].rowspan == 1)
                    else {
                        valid = false;
                        break;
                    };
                    let ids = &table.cells[i].words;
                    let Some(first) = ids.first().and_then(|&id| flows[id]) else {
                        valid = false;
                        break;
                    };
                    if first.baseline <= previous
                        || ids
                            .iter()
                            .any(|&id| flows[id].is_none_or(|f| f.baseline != first.baseline))
                    {
                        valid = false;
                        break;
                    }
                    previous = first.baseline;
                }
                if valid {
                    atomic.insert(p);
                }
            }
            let mut wrapped = HashSet::new();
            for &p in &fields {
                if groups[p].is_empty()
                    || independent_scalar(p)
                    || fields.iter().any(|q| *q != p && atomic.contains(q))
                {
                    continue;
                }
                let ids: Vec<_> = groups[p]
                    .iter()
                    .flat_map(|&i| table.cells[i].words.iter().copied())
                    .collect();
                let cols: Vec<_> = ids.iter().map(|id| (*originals[id]).clone()).collect();
                if let Some(lines) =
                    crate::native_paragraph_scope::scope(&cols, flows, source.bounds, cr(g, p))
                {
                    if paragraph_grammar(&lines, &originals) {
                        wrapped.insert(p);
                    }
                }
            }
            if wrapped.is_empty()
                || fields.iter().any(|p| {
                    groups[*p].len() > 1
                        && (independent_scalar(*p)
                            || (!wrapped.contains(p) && !section_columns.contains(p) && !ordinal_sections.contains(p)))
                })
            {
                continue;
            }
            collapse.insert(r);
            changed = true;
            for p in wrapped {
                components[p].fill(0);
            }
        }
    }

    for (p, parts) in groups.iter().enumerate() {
        if parts.len() < 2 || data_columns.contains(&p) {
            continue;
        }
        let Some(cs) = sentences(parts, table, &originals) else {
            continue;
        };
        let cross = cs.iter().enumerate().any(|(i, &r)| r != i);
        if !cross {
            continue;
        }
        changed = true;
        let all = cs.iter().all(|c| *c == cs[0]);
        components[p] = cs;
        if all && !records[&native_keys[p]] {
            collapse.insert(native_keys[p]);
        }
    }
    if !changed {
        return None;
    }
    let shifts: Vec<_> = collapse.iter().map(|r| ranges[r]).collect();
    let row_map = |row: usize| {
        let mut shift = 0;
        for &(lo, hi) in &shifts {
            if row < lo {
                break;
            }
            if row < hi {
                return lo - shift;
            }
            shift += hi - lo - 1;
        }
        row - shift
    };
    let mut out = table.clone();
    out.cells.clear();
    out.rows = table.rows - shifts.iter().map(|(a, b)| b - a - 1).sum::<usize>();
    for (p, parts) in groups.iter().enumerate() {
        let mut merged: BTreeMap<usize, Vec<(usize, usize)>> = BTreeMap::new();
        for (k, &i) in parts.iter().enumerate() {
            let key = if collapse.contains(&native_keys[p]) {
                0
            } else {
                components[p][k]
            };
            merged.entry(key).or_default().push((k, i));
        }
        for members in merged.into_values() {
            let mut c = table.cells[members[0].1].clone();
            let first = members.iter().map(|(_, i)| table.cells[*i].row).min()?;
            let end = members
                .iter()
                .map(|(_, i)| table.cells[*i].row + table.cells[*i].rowspan)
                .max()?;
            c.row = row_map(first);
            c.rowspan = row_map(end - 1) + 1 - c.row;
            c.words.clear();
            c.text.clear();
            let mut last = None;
            for (k, i) in members {
                let old = &table.cells[i];
                if let Some(previous) = last {
                    c.text = glue(
                        &c.text,
                        &old.text,
                        components[p][previous] == components[p][k],
                    );
                } else {
                    c.text = old.text.clone();
                }
                c.words.extend_from_slice(&old.words);
                last = Some(k);
            }
            out.cells.push(c);
        }
    }
    // Reorder only cell objects, never their internal native occurrence sequence.
    out.cells.sort_by_key(|c| {
        table
            .cells
            .iter()
            .position(|old| old.words.first().is_some() && old.words.first() == c.words.first())
            .unwrap_or(usize::MAX)
    });
    let mut before: Vec<_> = table
        .cells
        .iter()
        .flat_map(|c| c.words.iter().copied())
        .collect();
    let mut after: Vec<_> = out
        .cells
        .iter()
        .flat_map(|c| c.words.iter().copied())
        .collect();
    before.sort_unstable();
    after.sort_unstable();
    if before != after || after.windows(2).any(|w| w[0] == w[1]) {
        return None;
    }
    let old_text = table
        .cells
        .iter()
        .map(|c| c.text.as_str())
        .collect::<String>();
    let new_text = out
        .cells
        .iter()
        .map(|c| c.text.as_str())
        .collect::<String>();
    if material(&old_text) != material(&new_text)
        || out.cells.iter().any(|c| c.row + c.rowspan > out.rows)
    {
        return None;
    }
    (out != *table).then_some(out)
}
