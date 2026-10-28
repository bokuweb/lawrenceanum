//! Two native fields with coherent enumerations and explicit editing operators.
//! Neither similar body wording nor a previous guessed role is semantic evidence.
use crate::{
    grid::Grid,
    lines::PageRules,
    native_column_flow::AxisFlow,
    native_word_axes::NativeAxis,
    trace::{ContentStyles, NativeBounds, Unit, Word, WordStatus},
    vertical::Col,
};
use std::collections::{BTreeMap, BTreeSet};
type Rect = [f32; 4];
#[derive(Clone, Copy)]
pub(crate) struct Source<'a> {
    pub words: &'a [Word],
    pub cols: &'a [Col],
    pub bounds: &'a [NativeBounds],
    pub axes: &'a [NativeAxis],
    pub flows: &'a [Option<AxisFlow>],
    pub styles: &'a [ContentStyles],
    pub rules: &'a PageRules,
}
pub(crate) struct Certificate {
    pub after: Vec<usize>,
    pub before: Vec<usize>,
    boxes: [Rect; 2],
    original: BTreeMap<usize, (Rect, String)>,
    native: BTreeMap<usize, Rect>,
}
impl Certificate {
    pub(crate) fn matches(&self, cols: &[Col]) -> bool {
        cols.len() == self.original.len()
            && cols.iter().map(|c| c.id).collect::<BTreeSet<_>>().len() == cols.len()
            && cols.iter().all(|c| {
                self.original
                    .get(&c.id)
                    .is_some_and(|(b, t)| *b == [c.x, c.y, c.x1, c.y1] && *t == c.text)
            })
    }
    pub(crate) fn into_unit(
        &self,
        cols: Vec<Col>,
        grids: &[Grid],
        rules: Option<&PageRules>,
    ) -> Unit {
        let a = self.after.iter().copied().collect::<BTreeSet<_>>();
        let b = self.before.iter().copied().collect::<BTreeSet<_>>();
        let (mut aa, mut bb) = (vec![], vec![]);
        for mut c in cols {
            let r = self.native[&c.id];
            (c.x, c.y, c.x1, c.y1) = (r[0], r[1], r[2], r[3]);
            if a.contains(&c.id) {
                aa.push(c)
            } else if b.contains(&c.id) {
                bb.push(c)
            }
        }
        crate::shinkyu::ShinkyuLayout::native_fields(
            aa,
            bb,
            self.boxes.map(|b| (b[0], b[2], b[1], b[3])),
        )
        .into_unit(grids, rules)
    }
}
fn valid(b: Rect) -> bool {
    b.iter().all(|v| v.is_finite()) && b[0] < b[2] && b[1] < b[3]
}
fn overlap(a: (f32, f32), b: (f32, f32)) -> bool {
    a.0 < b.1 && b.0 < a.1
}
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Item,
    Paragraph,
    Article,
}
#[derive(Debug, Clone, Copy)]
struct Span {
    kind: Kind,
    first: u64,
    last: u64,
}
#[derive(Debug, Clone, Copy)]
enum Head {
    Number(Span),
    Edit {
        kind: Option<Kind>,
        empty_before: bool,
    },
}
// Checked numeric grammar: a representational overflow is unknown, not a count cap.
fn number(s: &str) -> Option<u64> {
    if s.is_empty() {
        return None;
    }
    let decimal = |c: char| {
        c.to_digit(10)
            .or_else(|| ('０'..='９').contains(&c).then(|| c as u32 - '０' as u32))
    };
    if s.chars().all(|c| decimal(c).is_some()) {
        return s.chars().try_fold(0u64, |n, c| {
            n.checked_mul(10)?.checked_add(decimal(c)? as u64)
        });
    }
    let digit = |c: char| {
        "〇一二三四五六七八九"
            .chars()
            .position(|d| d == c)
            .map(|n| n as u64)
    };
    if s.chars().all(|c| digit(c).is_some()) {
        return s
            .chars()
            .try_fold(0u64, |n, c| n.checked_mul(10)?.checked_add(digit(c)?));
    }
    let (mut total, mut group, mut coefficient, mut low, mut high) =
        (0u64, 0u64, None, 10_000u64, u64::MAX);
    for c in s.chars() {
        let unit = match c {
            '十' => 10,
            '百' => 100,
            '千' => 1000,
            '万' => 10_000,
            '億' => 100_000_000,
            '兆' => 1_000_000_000_000,
            _ => 0,
        };
        if unit == 0 {
            if coefficient.is_some() {
                return None;
            }
            coefficient = Some(digit(c)?);
        } else if unit < 10_000 {
            if unit >= low {
                return None;
            }
            let n = coefficient.take().unwrap_or(1);
            if n == 0 {
                return None;
            }
            group = group.checked_add(n.checked_mul(unit)?)?;
            low = unit;
        } else {
            if unit >= high {
                return None;
            }
            group = group.checked_add(coefficient.take().unwrap_or(0))?;
            if group == 0 {
                return None;
            }
            total = total.checked_add(group.checked_mul(unit)?)?;
            group = 0;
            low = 10_000;
            high = unit;
        }
    }
    total
        .checked_add(group)?
        .checked_add(coefficient.unwrap_or(0))
}
fn ordinal(s: &str) -> Option<Span> {
    if let Some(n) = s
        .strip_prefix('第')
        .and_then(|s| s.strip_suffix('条'))
        .and_then(number)
    {
        return Some(Span {
            kind: Kind::Article,
            first: n,
            last: n,
        });
    }
    let n = number(s)?;
    let kind = if s
        .chars()
        .all(|c| c.is_ascii_digit() || ('０'..='９').contains(&c))
    {
        Kind::Paragraph
    } else {
        Kind::Item
    };
    Some(Span {
        kind,
        first: n,
        last: n,
    })
}
fn bracket(s: &str) -> Option<&str> {
    for (a, b) in [('〔', '〕'), ('（', '）'), ('［', '］'), ('[', ']')] {
        if let Some(t) = s.strip_prefix(a).and_then(|s| s.strip_suffix(b)) {
            return Some(t);
        }
    }
    None
}
fn standalone(text: &str) -> Option<Head> {
    let text = compact(text);
    let body = bracket(&text)?;
    for (unit, kind) in [
        ("条", Kind::Article),
        ("項", Kind::Paragraph),
        ("号", Kind::Item),
    ] {
        if body == format!("{unit}を加える。") {
            return Some(Head::Edit {
                kind: Some(kind),
                empty_before: true,
            });
        }
        if body == format!("{unit}を削る。") {
            return Some(Head::Edit {
                kind: Some(kind),
                empty_before: false,
            });
        }
    }
    if body == "新設" {
        return Some(Head::Edit {
            kind: None,
            empty_before: true,
        });
    }
    if body == "削る" || body == "削除" {
        return Some(Head::Edit {
            kind: None,
            empty_before: false,
        });
    }
    let span = ["略", "同上", "同左"]
        .into_iter()
        .find_map(|suffix| body.strip_suffix(suffix))?;
    let (a, b) = ['〜', '～', '－', '―', '-']
        .into_iter()
        .find_map(|c| span.split_once(c))?;
    let a = ordinal(a)?;
    let b = ordinal(b)?;
    if a.kind != b.kind || a.first > b.first {
        return None;
    }
    Some(Head::Number(Span {
        kind: a.kind,
        first: a.first,
        last: b.first,
    }))
}
// A numbered item and an explicitly empty item cell identify row level.
// Paragraphs inside an article and references such as 二十八号... do not.
pub(crate) fn item_row_front(text: &str) -> Option<bool> {
    let line = text.trim_start().lines().next()?;
    if matches!(
        standalone(line),
        Some(Head::Edit {
            kind: Some(Kind::Item),
            ..
        })
    ) {
        return Some(false);
    }
    let end = line.char_indices().find(|(_, c)| c.is_whitespace())?.0;
    let span = ordinal(&line[..end])?;
    (span.kind == Kind::Item && span.first == span.last).then_some(true)
}
struct Column {
    text: String,
    head: Option<Head>,
    band: (f32, f32),
}
struct Entry {
    head: Head,
    band: (f32, f32),
}
fn columns(s: Source<'_>, ids: &[usize]) -> Option<Vec<Column>> {
    let mut groups: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for &id in ids {
        let NativeBounds::Glyphs(b) = s.bounds[id] else {
            return None;
        };
        let path = s.flows[id]?.vertical_path(b)?;
        let x = path[0] as f32;
        if !x.is_finite() {
            return None;
        }
        groups
            .entry(if x == 0. { 0 } else { x.to_bits() })
            .or_default()
            .push(id);
    }
    let mut out = vec![];
    for (_, mut ids) in groups {
        ids.sort_by(|a, b| {
            let NativeBounds::Glyphs(a) = s.bounds[*a] else {
                unreachable!()
            };
            let NativeBounds::Glyphs(b) = s.bounds[*b] else {
                unreachable!()
            };
            a[1].total_cmp(&b[1])
        });
        let mut end = f32::NEG_INFINITY;
        let mut text = String::new();
        let mut band = (f32::NEG_INFINITY, f32::INFINITY);
        for &id in &ids {
            let NativeBounds::Glyphs(b) = s.bounds[id] else {
                return None;
            };
            if b[1] < end {
                return None;
            }
            end = b[3];
            band = (band.0.max(b[0]), band.1.min(b[2]));
            text.push_str(&s.words[id].text);
        }
        if band.0 >= band.1 {
            return None;
        }
        let first = ids[0];
        let raw = compact(&s.words[first].text);
        let head = if let Some(span) = ordinal(&raw) {
            let second = *ids.get(1)?;
            let (NativeBounds::Glyphs(a), NativeBounds::Glyphs(b)) =
                (&s.bounds[first], &s.bounds[second])
            else {
                return None;
            };
            if !s.flows[first]?.vertical_prefix_gap(*a, s.flows[second]?, *b) {
                return None;
            }
            Some(Head::Number(span))
        } else {
            standalone(&text)
        };
        if head.is_some() {
            for &id in &ids {
                let w = &s.words[id];
                if compact(&crate::normalize::word_text(&w.raw)) != compact(&w.text) {
                    return None;
                }
                let ContentStyles::Glyphs(styles) = &s.styles[id] else {
                    return None;
                };
                if styles.is_empty()
                    || styles.iter().any(|(_, b)| {
                        let n = f64::from_bits(*b);
                        !n.is_finite() || n <= 0.
                    })
                {
                    return None;
                }
            }
        }
        out.push(Column { text, head, band });
    }
    out.sort_by(|a, b| b.band.0.total_cmp(&a.band.0));
    Some(out)
}
fn entries(cols: &[Column]) -> Option<Vec<Entry>> {
    let mut out = vec![];
    let mut last: Option<Span> = None;
    let texts = cols
        .iter()
        .map(|c| (c.text.as_str(), false))
        .chain(std::iter::once(("", false)))
        .collect::<Vec<_>>();
    let quotes = crate::amendment_quotes::states(&texts);
    if !quotes.last()?.strict() {
        return None;
    }
    for (index, c) in cols.iter().enumerate() {
        if let Some(h) = c.head {
            if !quotes[index].strict() {
                return None;
            }
            if let Head::Number(n) = h {
                if last.is_some_and(|p| p.kind != n.kind || p.last >= n.first) {
                    return None;
                }
                last = Some(n);
            }
            out.push(Entry {
                head: h,
                band: c.band,
            });
        } else if out.is_empty()
            || out
                .last()
                .is_some_and(|e: &Entry| matches!(e.head, Head::Edit { .. }))
        {
            return None;
        }
    }
    Some(out)
}
fn role(a: &[Entry], b: &[Entry]) -> Option<bool> {
    if a.len() != b.len() || a.is_empty() {
        return None;
    }
    let mut orientation = None;
    let mut common = false;
    for (x, y) in a.iter().zip(b) {
        if !overlap(x.band, y.band) {
            return None;
        }
        match (x.head, y.head) {
            (Head::Number(x), Head::Number(y)) => {
                if x.kind != y.kind || x.last - x.first != y.last - y.first {
                    return None;
                }
                common = true;
            }
            (Head::Edit { kind, empty_before }, Head::Number(n))
            | (Head::Number(n), Head::Edit { kind, empty_before }) => {
                if n.first != n.last || kind.is_some_and(|k| k != n.kind) {
                    return None;
                }
                let first_after = if matches!(x.head, Head::Edit { .. }) {
                    !empty_before
                } else {
                    empty_before
                };
                if orientation.is_some_and(|v| v != first_after) {
                    return None;
                }
                orientation = Some(first_after);
            }
            _ => return None,
        }
    }
    common.then_some(orientation).flatten()
}
pub(crate) fn prove(s: Source<'_>) -> Option<Certificate> {
    let len = s.words.len();
    if [s.bounds.len(), s.axes.len(), s.flows.len(), s.styles.len()]
        .into_iter()
        .any(|n| n != len)
        || s.words.iter().enumerate().any(|(i, w)| w.id != i)
    {
        return None;
    }
    let mut original = BTreeMap::new();
    for c in s.cols {
        let w = s.words.get(c.id)?;
        if w.status != WordStatus::Used
            || w.text != c.text
            || !valid([c.x, c.y, c.x1, c.y1])
            || original
                .insert(c.id, ([c.x, c.y, c.x1, c.y1], c.text.clone()))
                .is_some()
        {
            return None;
        }
    }
    let mut native = BTreeMap::new();
    for (i, w) in s.words.iter().enumerate() {
        if w.status != WordStatus::Used {
            continue;
        }
        let NativeBounds::Glyphs(b) = s.bounds[i] else {
            return None;
        };
        if !valid(b)
            || s.axes[i] != NativeAxis::Down
            || s.flows[i]?.vertical_path(b).is_none()
            || !original.contains_key(&i)
        {
            return None;
        }
        native.insert(i, b);
    }
    if original.len() != native.len() {
        return None;
    }
    let rails = crate::native_caption_tiers::horizontal_rails(s.rules);
    let mut frames = vec![];
    for h in &rails {
        let mut same = rails
            .iter()
            .filter(|g| g[0] == h[0] && g[2] == h[2])
            .copied()
            .collect::<Vec<_>>();
        same.sort_by(|a, b| a[1].total_cmp(&b[1]));
        if same.len() == 3 && h == &same[0] {
            frames.push([
                [h[0], same[0][1], h[2], same[1][1]],
                [h[0], same[1][1], h[2], same[2][1]],
            ]);
        }
    }
    let mut proofs = vec![];
    for boxes in frames {
        if boxes.iter().any(|b| !valid(*b)) {
            continue;
        }
        let outer = [boxes[0][0], boxes[0][1], boxes[0][2], boxes[1][3]];
        if s.rules
            .vertical
            .iter()
            .any(|v| outer[0] < v.x && v.x < outer[2] && v.y0 < outer[3] && outer[1] < v.y1)
        {
            continue;
        }
        let mut fields = [vec![], vec![]];
        let mut valid_frame = true;
        let src = crate::native_caption_tiers::Source {
            words: s.words,
            bounds: s.bounds,
            axes: s.axes,
            styles: s.styles,
            flows: s.flows,
        };
        for &id in original.keys() {
            let b = native[&id];
            let options = (0..2)
                .filter(|&k| {
                    crate::native_caption_tiers::inside_body(src, id, b, boxes[k], s.rules)
                })
                .collect::<Vec<_>>();
            if options.len() != 1 {
                valid_frame = false;
                break;
            }
            fields[options[0]].push(id);
        }
        if !valid_frame || fields.iter().any(Vec::is_empty) {
            continue;
        }
        if s.words.iter().enumerate().any(|(id, w)| {
            w.status != WordStatus::Used
                && match s.bounds[id] {
                    NativeBounds::Glyphs(b) if valid(b) => {
                        b[0] < outer[2] && outer[0] < b[2] && b[1] < outer[3] && outer[1] < b[3]
                    }
                    _ => {
                        valid(w.source_box)
                            && w.source_box[0] < outer[2]
                            && outer[0] < w.source_box[2]
                            && w.source_box[1] < outer[3]
                            && outer[1] < w.source_box[3]
                    }
                }
        }) {
            continue;
        }

        let (Some(aa), Some(bb)) = (
            columns(s, &fields[0]).and_then(|c| entries(&c)),
            columns(s, &fields[1]).and_then(|c| entries(&c)),
        ) else {
            continue;
        };
        let Some(first_after) = role(&aa, &bb) else {
            continue;
        };
        let indices = if first_after { [0, 1] } else { [1, 0] };
        proofs.push(Certificate {
            after: s
                .cols
                .iter()
                .filter(|c| fields[indices[0]].contains(&c.id))
                .map(|c| c.id)
                .collect(),
            before: s
                .cols
                .iter()
                .filter(|c| fields[indices[1]].contains(&c.id))
                .map(|c| c.id)
                .collect(),
            boxes: [boxes[indices[0]], boxes[indices[1]]],
            original: original.clone(),
            native: native.clone(),
        });
    }
    (proofs.len() == 1).then(|| proofs.remove(0))
}
#[cfg(test)]
#[path = "native_parallel_edit_tests.rs"]
mod tests;
