//! Source-typed publication order. Length and presentation-line counts do not
//! identify a caption or a publisher; original flow, typography and signed
//! dated header fields do. Existing complete lines/tables move as units.
use super::*;
use crate::native_word_axes::NativeAxis;
use crate::trace::{ContentStyles, NativeBounds, NativeStyles, WordStatus};
use std::collections::{BTreeSet, HashSet};
type Rect = [f32; 4];
fn valid(b: Rect) -> bool {
    b.iter().all(|n| n.is_finite()) && b[0] < b[2] && b[1] < b[3]
}
fn inside(a: Rect, b: Rect) -> bool {
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
fn ids(l: &Line) -> Vec<usize> {
    l.table.as_ref().map_or_else(
        || l.words.clone(),
        |t| {
            t.cells
                .iter()
                .flat_map(|c| c.words.iter().copied())
                .collect()
        },
    )
}
fn nominal(t: &str) -> bool {
    !t.is_empty() && !t.contains(['。', '、', '！', '？', '!', '?', '「', '」', '『', '』'])
}
fn office(t: &str) -> bool {
    let text = compact(t);
    if crate::notice_boundaries::signature_office(&text) {
        return true;
    }
    let Some(open) = text.chars().next() else {
        return false;
    };
    let close = match open {
        '(' => ')',
        '（' => '）',
        _ => return false,
    };
    let mut stack = vec![close];
    for (i, c) in text.char_indices().skip(1) {
        match c {
            '(' => stack.push(')'),
            '（' => stack.push('）'),
            ')' | '）' => {
                if stack.pop() != Some(c) {
                    return false;
                }
                if stack.is_empty() {
                    let tail = &text[i + c.len_utf8()..];
                    return !tail.is_empty()
                        && crate::notice_boundaries::signature_office(&format!(
                            "{}{}",
                            &text[open.len_utf8()..i],
                            tail
                        ));
                }
            }
            _ => {}
        }
    }
    false
}
pub(super) struct Source<'a> {
    pub words: &'a [Word],
    pub styles: &'a [Option<NativeStyles>],
    pub content: &'a [ContentStyles],
    pub bounds: &'a [NativeBounds],
    pub axes: &'a [NativeAxis],
    pub rulings: &'a [crate::native_word_rulings::NativeRuling],
    pub previous: &'a [crate::trace::PageTrace],
}
impl Source<'_> {
    fn rect(&self, ids: &[usize]) -> Option<Rect> {
        ids.iter()
            .map(|&id| {
                let w = self.words.get(id)?;
                if w.id != id || w.status != WordStatus::Used {
                    return None;
                }
                let NativeBounds::Glyphs(b) = self.bounds.get(id)? else {
                    return None;
                };
                valid(*b).then_some(*b)
            })
            .collect::<Option<Vec<_>>>()?
            .into_iter()
            .reduce(union)
    }
    fn content(&self, ids: &[usize]) -> Option<NativeStyles> {
        let mut out = NativeStyles::new();
        for &id in ids {
            let all = self.styles.get(id)?.as_ref()?;
            if all.is_empty()
                || all.iter().any(|&(_, bits)| {
                    let s = f64::from_bits(bits);
                    !s.is_finite() || s <= 0.
                })
            {
                return None;
            }
            let ContentStyles::Glyphs(s) = self.content.get(id)? else {
                return None;
            };
            if !s.is_subset(all)
                || self.words[id].text.chars().any(char::is_alphanumeric) == s.is_empty()
            {
                return None;
            }
            out.extend(s.iter().copied());
        }
        Some(out)
    }
    fn known_styles(&self, ids: &[usize]) -> bool {
        ids.iter().all(|&id| {
            self.styles
                .get(id)
                .and_then(Option::as_ref)
                .is_some_and(|s| {
                    !s.is_empty()
                        && s.iter().all(|&(_, b)| {
                            let f = f64::from_bits(b);
                            f.is_finite() && f > 0.
                        })
                })
        })
    }
    fn text(&self, ids: &[usize]) -> String {
        ids.iter().map(|&i| compact(&self.words[i].text)).collect()
    }
    fn flow(&self, ids: &[usize], axis: NativeAxis) -> bool {
        !ids.is_empty() && ids.iter().all(|&id| self.axes.get(id) == Some(&axis))
    }
    fn extent(&self, line: &Line) -> Option<Rect> {
        let mut b = self.rect(&ids(line))?;
        if let Some(t) = &line.table {
            let r = [t.x0, t.y0, t.x1, t.y1];
            if !valid(r) || t.rows == 0 || t.cols == 0 {
                return None;
            }
            b = union(b, r);
        }
        Some(b)
    }
    fn shape(&self, line: &Line) -> Option<Rect> {
        let b = self.extent(line)?;
        if line.table.is_none() && compact(&line.text) != self.text(&line.words) {
            return None;
        }
        Some(b)
    }
    fn unaccounted(&self, frame: Rect, owned: &HashSet<usize>, opaque: &[Rect]) -> bool {
        let inside64 = |a: Rect, b: [f64; 4]| {
            let a = a.map(f64::from);
            a[0] <= b[0] && a[1] <= b[1] && b[2] <= a[2] && b[3] <= a[3]
        };
        self.words.iter().enumerate().any(|(i, w)| {
            if owned.contains(&i) {
                return false;
            }
            if w.id != i {
                return true;
            }
            let native = matches!(self.bounds.get(i), Some(NativeBounds::Glyphs(_)));
            let b = match self.bounds.get(i) {
                Some(NativeBounds::Glyphs(b)) => *b,
                _ => [w.x0, w.y0, w.x1, w.y1],
            };
            if !valid(b) {
                return true;
            }
            let ink = self
                .rulings
                .get(i)
                .copied()
                .filter(|r| self.axes.get(i).is_some_and(|a| r.matches_axis(*a)))
                .and_then(|r| r.rect());
            let extent = ink.map_or(b.map(f64::from), |r| {
                [
                    f64::from(b[0]).min(r[0]),
                    f64::from(b[1]).min(r[1]),
                    f64::from(b[2]).max(r[2]),
                    f64::from(b[3]).max(r[3]),
                ]
            });
            let f = frame.map(f64::from);
            if !(f[0] < extent[2] && extent[0] < f[2] && f[1] < extent[3] && extent[1] < f[3]) {
                return false;
            }
            // This is positive native outline evidence for a source decoration,
            // not permission to erase visible text or ignore other unknown fields.
            if ink.is_some()
                && native
                && self.known_styles(&[i])
                && w.status == WordStatus::Empty
                && w.text.is_empty()
                && !w.raw.is_empty()
                && inside64(frame, extent)
            {
                return false;
            }
            // Existing complete tables remain opaque children of this ordering
            // operation. Their unresolved fields are not reconstructed or blanked.
            if native && opaque.iter().any(|&t| inside64(t, extent)) {
                return false;
            }
            !(!w.raw.is_empty()
                && w.raw.chars().all(char::is_whitespace)
                && matches!(self.content.get(i),Some(ContentStyles::Glyphs(s))if s.is_empty()))
        })
    }
    // Group known source flow into common-axis rows/columns. Connected
    // projections need a common intersection; overprinted source is ambiguous.
    fn bands(&self, source: &[usize], vertical: bool) -> Option<Vec<Vec<usize>>> {
        if source.is_empty() {
            return Some(vec![]);
        }
        let cross = if vertical { 0 } else { 1 };
        let along = 1 - cross;
        let mid = |a: f32, b: f32| (a as f64 + b as f64) / 2.;
        let mut bands = source.iter().map(|&id| vec![id]).collect::<Vec<_>>();
        loop {
            let mut pair = None;
            'search: for i in 0..bands.len() {
                for j in i + 1..bands.len() {
                    if bands[i].iter().any(|&a| {
                        bands[j].iter().any(|&b| {
                            let (a, b) = (self.rect(&[a]).unwrap(), self.rect(&[b]).unwrap());
                            let nested = (a[cross] <= b[cross] && b[cross + 2] <= a[cross + 2])
                                || (b[cross] <= a[cross] && a[cross + 2] <= b[cross + 2]);
                            let common_centres = (b[cross] as f64) < mid(a[cross], a[cross + 2])
                                && mid(a[cross], a[cross + 2]) < (b[cross + 2] as f64)
                                && (a[cross] as f64) < mid(b[cross], b[cross + 2])
                                && mid(b[cross], b[cross + 2]) < (a[cross + 2] as f64);
                            nested || common_centres
                        })
                    }) {
                        pair = Some((i, j));
                        break 'search;
                    }
                }
            }
            let Some((i, j)) = pair else {
                break;
            };
            let other = bands.remove(j);
            bands[i].extend(other);
        }
        for band in &mut bands {
            let low = band
                .iter()
                .map(|&i| self.rect(&[i]).unwrap()[cross])
                .reduce(f32::max)?;
            let high = band
                .iter()
                .map(|&i| self.rect(&[i]).unwrap()[cross + 2])
                .reduce(f32::min)?;
            if low >= high {
                return None;
            }
            band.sort_by(|a, b| {
                self.rect(&[*a]).unwrap()[along].total_cmp(&self.rect(&[*b]).unwrap()[along])
            });
            for pair in band.windows(2) {
                let (a, b) = (self.rect(&[pair[0]])?, self.rect(&[pair[1]])?);
                if a[along + 2] > b[along]
                    && !(a[along] < b[along]
                        && a[along + 2] < b[along + 2]
                        && mid(a[along], a[along + 2]) < (b[along] as f64)
                        && (a[along + 2] as f64) < mid(b[along], b[along + 2]))
                {
                    return None;
                }
            }
        }
        bands.sort_by(|a, b| self.rect(a).unwrap()[cross].total_cmp(&self.rect(b).unwrap()[cross]));
        if vertical {
            bands.reverse();
        }
        Some(bands)
    }
}
fn vertical_order(lines: &[Line], selected: &[usize], src: &Source<'_>) -> Option<Vec<usize>> {
    let source = selected
        .iter()
        .copied()
        .filter(|&i| lines[i].table.is_none() && src.flow(&lines[i].words, NativeAxis::Down))
        .collect::<Vec<_>>();
    let mut components = source.iter().map(|&i| vec![i]).collect::<Vec<_>>();
    loop {
        let mut pair = None;
        'search: for i in 0..components.len() {
            for j in i + 1..components.len() {
                if components[i].iter().any(|&a| {
                    components[j].iter().any(|&b| {
                        let (a, b) = (
                            src.rect(&lines[a].words).unwrap(),
                            src.rect(&lines[b].words).unwrap(),
                        );
                        a[1] < b[3] && b[1] < a[3]
                    })
                }) {
                    pair = Some((i, j));
                    break 'search;
                }
            }
        }
        let Some((i, j)) = pair else {
            break;
        };
        let other = components.remove(j);
        components[i].extend(other);
    }
    components.sort_by(|a, b| {
        let top = |component: &[usize]| {
            component
                .iter()
                .map(|&i| src.rect(&lines[i].words).unwrap()[1])
                .reduce(f32::min)
                .unwrap()
        };
        top(a).total_cmp(&top(b))
    });
    let mut ordered = vec![];
    for component in components {
        let ids = component
            .iter()
            .flat_map(|&i| lines[i].words.iter().copied())
            .collect::<Vec<_>>();
        let columns = src.bands(&ids, true)?;
        let mut emitted = HashSet::new();
        let mut current = None;
        let mut actual = vec![];
        for id in columns.into_iter().flatten() {
            let line = *component.iter().find(|&&i| lines[i].words.contains(&id))?;
            if current != Some(line) {
                if let Some(previous) = current {
                    if actual != lines[previous].words {
                        return None;
                    }
                }
                if !emitted.insert(line) {
                    return None;
                }
                ordered.push(line);
                current = Some(line);
                actual.clear();
            }
            actual.push(id);
        }
        if let Some(previous) = current {
            if actual != lines[previous].words {
                return None;
            }
        }
    }
    Some(ordered)
}
#[derive(Clone)]
struct Caption {
    lines: Vec<usize>,
    words: Vec<usize>,
    styles: NativeStyles,
}
fn caption(
    lines: &[Line],
    selected: &[usize],
    at: usize,
    src: &Source<'_>,
    axis: NativeAxis,
    financial: bool,
) -> Option<Caption> {
    let first = lines.get(*selected.get(at)?)?;
    if first.table.is_some() || !src.flow(&first.words, axis) {
        return None;
    }
    let &first_id = first.words.first()?;
    let style = src.content(&[first_id])?;
    if style.is_empty() {
        return None;
    }
    let mut cap = Caption {
        lines: vec![],
        words: vec![],
        styles: style.clone(),
    };
    let mut cross: Option<(f32, f32)> = None;
    let d = if axis == NativeAxis::Down { 0 } else { 1 };
    'outer: for &index in &selected[at..] {
        let l = &lines[index];
        if l.table.is_some() || !src.flow(&l.words, axis) {
            break;
        }
        let mut touched = false;
        for &id in &l.words {
            let s = src.content(&[id])?;
            if !s.is_empty() && s != style {
                break 'outer;
            }
            let b = src.rect(&[id])?;
            let next = cross.map_or((b[d], b[d + 2]), |(low, high)| {
                (low.max(b[d]), high.min(b[d + 2]))
            });
            if next.0 >= next.1 {
                break 'outer;
            }
            cross = Some(next);
            cap.words.push(id);
            touched = true;
        }
        if touched {
            cap.lines.push(index);
        }
    }
    if cap.words.is_empty() {
        return None;
    }
    cap.lines = selected[at..]
        .iter()
        .copied()
        .filter(|&i| lines[i].words.iter().any(|id| cap.words.contains(id)))
        .collect();
    // A body suffix in the same original Line was deliberately not caption
    // evidence, but that complete source Line remains in its publication.
    if cap.lines.is_empty() {
        cap.lines.push(selected[at]);
    }
    let text = src.text(&cap.words);
    if !nominal(&text) || !text.ends_with("公告") || financial != text.ends_with("決算公告") {
        return None;
    }
    Some(cap)
}
// A signed notice can annex a named counterparty's statement. The binding
// must be explicit in one unquoted declaration, with distinct role labels;
// an arbitrary occurrence of the publisher name is not this relationship.
fn parenthesized_role(text: &str) -> Option<&str> {
    let (rest, close) = if let Some(s) = text.strip_prefix('（') {
        (s, '）')
    } else {
        (text.strip_prefix('(')?, ')')
    };
    let mut stack = vec![close];
    let mut first_field = None;
    for (i, c) in rest.char_indices() {
        match c {
            '(' => stack.push(')'),
            '（' => stack.push('）'),
            ')' | '）' => {
                if stack.pop() != Some(c) {
                    return None;
                }
                if stack.is_empty() {
                    let label = &rest[..first_field.map_or(i, |(end, _)| end)];
                    let annotated =
                        first_field.map_or(true, |(_, start)| !rest[start..i].is_empty());
                    return (nominal(label)
                        && !label.contains(['(', ')', '（', '）'])
                        && annotated)
                        .then_some(label);
                }
            }
            ',' | '、' if stack.len() == 1 && first_field.is_none() => {
                first_field = Some((i, i + c.len_utf8()))
            }
            _ => {}
        }
    }
    None
}
fn explicitly_named_counterparty(body: &str, name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut fragments = vec![];
    let mut text = String::new();
    let mut quotes = vec![];
    for c in body.chars() {
        match c {
            '「' | '『' => {
                if quotes.is_empty() {
                    fragments.push(std::mem::take(&mut text));
                }
                quotes.push(if c == '「' { '」' } else { '』' });
            }
            '」' | '』' => {
                if quotes.pop() != Some(c) {
                    return false;
                }
            }
            _ => {
                if quotes.is_empty() {
                    text.push(c);
                }
            }
        }
    }
    if !quotes.is_empty() {
        return false;
    }
    fragments.push(text);
    for fragment in &fragments {
        for sentence in fragment.split(['。', '！', '？', '!', '?']) {
            let own = sentence
                .match_indices("当社")
                .filter_map(|(i, _)| parenthesized_role(&sentence[i + "当社".len()..]))
                .collect::<BTreeSet<_>>();
            let other = sentence
                .match_indices(name)
                .filter_map(|(i, _)| parenthesized_role(&sentence[i + name.len()..]))
                .collect::<BTreeSet<_>>();
            if own.len() == 1 && other.len() == 1 && own.is_disjoint(&other) {
                return true;
            }
        }
    }
    false
}
struct NoticeProof {
    lines: Vec<usize>,
    text: String,
    attribution: Vec<BTreeSet<String>>,
    action: String,
    declaration: String,
    rect: Rect,
}
fn notice_witness(
    lines: &[Line],
    members: &[usize],
    head: &Caption,
    src: &Source<'_>,
) -> Option<Vec<BTreeSet<String>>> {
    let body = members
        .iter()
        .flat_map(|&i| lines[i].words.iter().copied())
        .filter(|i| !head.words.contains(i))
        .collect::<Vec<_>>();
    if !src.flow(&body, NativeAxis::Down) {
        return None;
    }
    let columns = src.bands(&body, true)?;
    let mut dated = None;
    for (i, col) in columns.iter().enumerate() {
        let mut text = String::new();
        for n in 0..col.len() {
            text.push_str(&compact(&src.words[col[n]].text));
            if crate::notice_boundaries::date(&text) {
                let styles = src.content(&col[..=n])?;
                if styles.is_empty() || !styles.is_disjoint(&head.styles) {
                    return None;
                }
                dated = Some((i, n + 1, styles));
                break;
            }
        }
    }
    let (column, after, date_style) = dated?;
    // Publisher identity belongs to the notice's own dated attribution.
    // Earlier prose and quoted references cannot supply this field.
    let attributed = columns[column][after..]
        .iter()
        .copied()
        .chain(columns[column + 1..].iter().flatten().copied())
        .collect::<Vec<_>>();
    let attribution = src.text(&attributed);
    if !nominal(&attribution) {
        return None;
    }
    let last = columns.last()?;
    // A typeset publisher may wrap above the signed office in the same
    // column. Recognize the complete office suffix only at a native Word
    // boundary; the preceding field must itself remain nominal source text.
    let starts = (0..last.len())
        .filter(|&i| office(&src.text(&last[i..])) && (i == 0 || nominal(&src.text(&last[..i]))))
        .collect::<Vec<_>>();
    if starts.len() != 1 {
        return None;
    }
    let signature = src.content(&last[starts[0]..])?;
    if signature.is_disjoint(&date_style) || !signature.is_disjoint(&head.styles) {
        return None;
    }
    // Identity is a complete original Word field immediately preceding an
    // independently styled signature, never a substring of a byline/address.
    // A field can wrap through any number of columns and any number of native
    // Words; presentation Line boundaries and character counts have no role.
    let mut attributed_columns = vec![columns[column][after..].to_vec()];
    attributed_columns.extend_from_slice(&columns[column + 1..]);
    let mut records = vec![];
    let mut after_signature = 0;
    for (end, col) in attributed_columns.iter().enumerate() {
        for at in 0..col.len() {
            if !office(&src.text(&col[at..])) {
                continue;
            }
            let style = src.content(&col[at..])?;
            if style.is_disjoint(&date_style) || !style.is_disjoint(&head.styles) {
                continue;
            }
            let prefix = attributed_columns
                .iter()
                .take(end)
                .flatten()
                .copied()
                .chain(col[..at].iter().copied())
                .collect::<Vec<_>>();
            let mut fields = BTreeSet::new();
            for start in after_signature..prefix.len() {
                let text = src.text(&prefix[start..]);
                if !nominal(&text) {
                    continue;
                }
                fields.insert(text.clone());
                // A balanced leading party role is a field label. Its value is
                // not a company dictionary or an empirically bounded prefix.
                if let Some(label) = parenthesized_role(&text) {
                    let prefix = if text.starts_with('（') {
                        format!("（{label}）")
                    } else {
                        format!("({label})")
                    };
                    if let Some(tail) = text.strip_prefix(&prefix) {
                        if nominal(tail) {
                            fields.insert(tail.to_owned());
                        }
                    }
                }
            }
            if !fields.is_empty() {
                records.push(fields);
            }
            // One signature terminates one dated actor record. A later actor
            // cannot take its name from an earlier signed record.
            after_signature = attributed_columns.iter().take(end + 1).map(Vec::len).sum();
        }
    }
    (!records.is_empty()).then_some(records)
}
fn publisher(
    lines: &[Line],
    header: &[usize],
    head: &Caption,
    table: &Line,
    src: &Source<'_>,
) -> Option<String> {
    let source = header
        .iter()
        .flat_map(|&i| lines[i].words.iter().copied())
        .filter(|id| !head.words.contains(id))
        .collect::<Vec<_>>();
    if !src.flow(&source, NativeAxis::Right) {
        return None;
    }
    let rows = src.bands(&source, false).or_else(|| {
        {};
        None
    })?;
    {};
    let fonts = head.styles.iter().map(|&(f, _)| f).collect::<BTreeSet<_>>();
    let mut date_row = None;
    for (i, row) in rows.iter().enumerate() {
        let mut text = String::new();
        for n in 0..row.len() {
            text.push_str(&compact(&src.words[row[n]].text));
            if crate::notice_boundaries::date(&text) {
                let s = src.content(&row[..=n])?;
                if s.is_empty() || !s.is_disjoint(&head.styles) {
                    return None;
                }
                date_row = Some((i, s));
                break;
            }
        }
        if date_row.is_some() {
            break;
        }
    }
    let (date_index, date_style) = date_row?;
    let mut names = vec![];
    for (i, row) in rows.iter().enumerate().skip(date_index + 1) {
        if !office(&src.text(row)) {
            continue;
        }
        let signature_style = src.content(row)?;
        if signature_style.is_disjoint(&date_style) || !signature_style.is_disjoint(&head.styles) {
            continue;
        }
        let mut first = i;
        while first > date_index + 1 {
            let s = src.content(&rows[first - 1])?;
            let fs = s.iter().map(|&(f, _)| f).collect::<BTreeSet<_>>();
            if s.is_empty() || fs != fonts {
                break;
            }
            first -= 1;
        }
        if first == i {
            continue;
        }
        let name = rows[first..i]
            .iter()
            .map(|r| src.text(r))
            .collect::<String>();
        if !nominal(&name) {
            continue;
        }
        let b = src.rect(row)?;
        let t = table.table.as_ref()?;
        if b[3] >= t.y0 {
            continue;
        }
        names.push(name);
    }
    {
        {};
        (names.len() == 1).then(|| names.remove(0))
    }
}
pub(super) struct Plan {
    pub selected: Vec<usize>,
    pub desired: Vec<usize>,
    pub words: HashSet<usize>,
}
pub(super) fn plan(lines: &[Line], src: &Source<'_>, frame: Rect) -> Option<Plan> {
    if !valid(frame) {
        return None;
    }
    let shapes = lines.iter().map(|l| src.extent(l)).collect::<Vec<_>>();
    let mut selected = vec![];
    let mut owned = HashSet::new();
    for (i, l) in lines.iter().enumerate() {
        let Some(b) = shapes[i] else {
            if !l.text.trim().is_empty() {
                return None;
            }
            continue;
        };
        if intersects(frame, b) && !inside(frame, b) {
            {};
            return None;
        }
        if !inside(frame, b) {
            continue;
        }
        src.shape(l)?;
        let ws = ids(l);
        if ws.is_empty() || !src.known_styles(&ws) || ws.iter().any(|id| !owned.insert(*id)) {
            return None;
        }
        selected.push(i);
    }
    let opaque = selected
        .iter()
        .filter_map(|&i| lines[i].table.as_ref().map(|t| [t.x0, t.y0, t.x1, t.y1]))
        .collect::<Vec<_>>();
    if selected.is_empty() || src.unaccounted(frame, &owned, &opaque) {
        {};
        return None;
    }
    // A quotation opened outside the publication owns its contents.
    // Balanced quotations opened and closed by its own body remain ordinary
    // source text. Captions themselves must begin outside any quotation.
    let mut quotes: Vec<(char, bool)> = vec![];
    let mut unquoted = vec![false; lines.len()];
    for text in src.previous.iter().map(|p| p.text.as_str()) {
        for c in text.chars() {
            match c {
                '「' => quotes.push(('」', false)),
                '『' => quotes.push(('』', false)),
                '」' | '』' => {
                    if quotes.pop().map(|q| q.0) != Some(c) {
                        return None;
                    }
                }
                _ => {}
            }
        }
    }
    for (i, line) in lines.iter().enumerate().take(selected.last()? + 1) {
        let owns = selected.contains(&i);
        unquoted[i] = quotes.is_empty();
        if owns && quotes.iter().any(|q| !q.1) {
            return None;
        }
        for c in line.text.chars() {
            match c {
                '「' => quotes.push(('」', owns)),
                '『' => quotes.push(('』', owns)),
                '」' | '』' => {
                    let old = quotes.pop()?;
                    if old.0 != c || owns != old.1 {
                        return None;
                    }
                }
                _ => {}
            }
        }
    }
    if quotes.iter().any(|q| q.1) {
        return None;
    }
    let vertical = vertical_order(lines, &selected, src)?;
    let mut notices = vec![];
    let mut k = 0;
    while k < vertical.len() {
        let Some(head) = caption(lines, &vertical, k, src, NativeAxis::Down, false)
            .filter(|_| unquoted[vertical[k]])
        else {
            k += 1;
            continue;
        };
        let start = k;
        let consumed = head.lines.len().max(1);
        k += consumed;
        while k < vertical.len()
            && caption(lines, &vertical, k, src, NativeAxis::Down, false)
                .filter(|_| unquoted[vertical[k]])
                .is_none()
        {
            k += 1;
        }
        let members = vertical[start..k].to_vec();
        let text = members
            .iter()
            .map(|&i| compact(&lines[i].text))
            .collect::<String>();
        if !text.contains("貸借対照表") {
            return None;
        }
        let attribution = notice_witness(lines, &members, &head, src)?;
        let rect = members.iter().map(|&i| shapes[i].unwrap()).reduce(union)?;
        let action = src
            .text(&head.words)
            .strip_suffix("公告")
            .unwrap_or("")
            .to_string();
        let body = members
            .iter()
            .flat_map(|&i| lines[i].words.iter().copied())
            .filter(|id| !head.words.contains(id))
            .collect::<Vec<_>>();
        let declaration = src.text(&body);
        notices.push(NoticeProof {
            lines: members,
            text,
            attribution,
            action,
            declaration,
            rect,
        });
    }
    if notices.is_empty() {
        {};
        return None;
    }
    {};
    let notice_lines = notices
        .iter()
        .flat_map(|n| n.lines.iter().copied())
        .collect::<HashSet<_>>();
    let rest = selected
        .iter()
        .copied()
        .filter(|i| !notice_lines.contains(i))
        .collect::<Vec<_>>();
    if rest.is_empty() {
        return None;
    }
    let mut headers = vec![];
    for i in 0..rest.len() {
        let Some(head) =
            caption(lines, &rest, i, src, NativeAxis::Right, true).filter(|_| unquoted[rest[i]])
        else {
            continue;
        };
        let table = (i..rest.len()).find(|&j| lines[rest[j]].table.is_some())?;
        if let Some(name) = publisher(lines, &rest[i..table], &head, &lines[rest[table]], src) {
            headers.push((i, table, name));
        }
    }
    if headers.first().is_none_or(|h| h.0 != 0) {
        {};
        return None;
    }
    let names = headers.iter().map(|h| h.2.clone()).collect::<BTreeSet<_>>();
    let named_operands = notices
        .iter()
        .map(|n| declaration::rights_transfer_parties(&n.declaration, &n.action, &names))
        .collect::<Vec<_>>();
    let mut matched = vec![false; notices.len()];
    for (i, (start, first_table, name)) in headers.iter().enumerate() {
        let end = headers.get(i + 1).map_or(rest.len(), |h| h.0);
        if *first_table >= end
            || !rest[*start..end]
                .iter()
                .any(|&j| lines[j].text.contains("貸借対照表"))
        {
            return None;
        }
        let table = lines[rest[*first_table]].table.as_ref()?;
        if rest[*start..end].iter().any(|&j| {
            let b = shapes[j].unwrap();
            b[2] <= table.x0 || table.x1 <= b[0]
        }) {
            return None;
        }
        let mut found = false;
        for (n, notice) in notices.iter().enumerate() {
            if notice
                .attribution
                .iter()
                .any(|fields| fields.contains(name) && fields.intersection(&names).count() == 1)
                || explicitly_named_counterparty(&notice.text, name)
                || named_operands[n]
                    .as_ref()
                    .is_some_and(|set| set.contains(name))
            {
                matched[n] = true;
                found = true;
            }
        }
        if !found {
            {};
            return None;
        }
    }
    if matched.iter().any(|b| !*b) {
        {};
        return None;
    }
    // The geometric precedence graph must have a unique topological reading:
    // disjoint bands precede lower bands; overlapping bands read right to left.
    let mut edges = vec![vec![]; notices.len()];
    let mut indegree = vec![0; notices.len()];
    for i in 0..notices.len() {
        for j in i + 1..notices.len() {
            let (a, b) = (notices[i].rect, notices[j].rect);
            let order = if a[3] <= b[1] {
                Some((i, j))
            } else if b[3] <= a[1] {
                Some((j, i))
            } else if b[2] <= a[0] {
                Some((i, j))
            } else if a[2] <= b[0] {
                Some((j, i))
            } else {
                None
            };
            let (from, to) = order?;
            edges[from].push(to);
            indegree[to] += 1;
        }
    }
    let mut order = vec![];
    while order.len() < notices.len() {
        let ready = (0..notices.len())
            .filter(|i| indegree[*i] == 0 && !order.contains(i))
            .collect::<Vec<_>>();
        if ready.len() != 1 {
            return None;
        }
        let i = ready[0];
        order.push(i);
        for &j in &edges[i] {
            indegree[j] -= 1;
        }
    }
    let desired = order
        .into_iter()
        .flat_map(|i| notices[i].lines.iter().copied())
        .chain(rest)
        .collect();
    {};
    Some(Plan {
        selected,
        desired,
        words: owned,
    })
}
pub(super) fn plans(lines: &[Line], src: &Source<'_>, frames: &[Rect]) -> Vec<Plan> {
    if frames.iter().any(|&f| !valid(f)) {
        return vec![];
    }
    let mut plans: Vec<Plan> = vec![];
    for &frame in frames {
        if let Some(p) = plan(lines, src, frame) {
            if !plans.iter().any(|q| q.words == p.words) {
                plans.push(p);
            }
        }
    }
    let conflicts = (0..plans.len())
        .filter(|&i| {
            (0..plans.len()).any(|j| i != j && !plans[i].words.is_disjoint(&plans[j].words))
        })
        .collect::<HashSet<_>>();
    plans
        .into_iter()
        .enumerate()
        .filter_map(|(i, p)| (!conflicts.contains(&i)).then_some(p))
        .collect()
}
pub(super) fn restore(units: &mut [Unit], src: &Source<'_>, frames: &[Rect]) {
    for unit in units {
        let Unit::Plain(lines) = unit else {
            continue;
        };
        let plans = plans(lines, src, frames);
        if plans.is_empty() {
            continue;
        }
        let selected = plans
            .iter()
            .flat_map(|p| p.selected.iter().copied())
            .collect::<HashSet<_>>();
        let mut old = std::mem::take(lines)
            .into_iter()
            .map(Some)
            .collect::<Vec<_>>();
        for i in 0..old.len() {
            if let Some(p) = plans.iter().find(|p| p.selected[0] == i) {
                for &j in &p.desired {
                    lines.push(old[j].take().unwrap());
                }
            }
            if !selected.contains(&i) {
                if let Some(line) = old[i].take() {
                    lines.push(line);
                }
            }
        }
    }
}

#[path = "publication_party_lists.rs"]
mod declaration;
