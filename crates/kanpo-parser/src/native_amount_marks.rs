//! Return original horizontal rectangle marks to source-certified numeric rows.
use crate::{
    lines::PageRules,
    native_terminal_glyph::{self, Terminals},
    trace::{ContentStyles, Line, NativeStyles, Unit, Word, WordStatus},
};
use std::collections::BTreeSet;
type Rect = [f32; 4];
fn compact(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}
fn piece(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_digit() || ",.△▲−－+-".contains(c))
}
fn overlaps(a: Rect, b: Rect) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}
struct Source<'a> {
    words: &'a [Word],
    styles: &'a [ContentStyles],
    terminals: Terminals,
    marks: Vec<bool>,
    owners: Vec<usize>,
    mark_ids: Vec<usize>,
}
impl Source<'_> {
    fn pos(&self, id: usize) -> Option<[f64; 3]> {
        let w = self.words.get(id)?;
        self.terminals.get(w)?.origins_for(w)
    }
    fn styles(&self, ids: &[usize]) -> Option<NativeStyles> {
        let mut out = NativeStyles::new();
        for &id in ids {
            let ContentStyles::Glyphs(s) = self.styles.get(id)? else {
                return None;
            };
            if s.is_empty() {
                return None;
            }
            out.extend(s);
        }
        Some(out)
    }
}
struct Row {
    ids: Vec<usize>,
    marks: Vec<usize>,
    left: f64,
    label_right: f64,
    labels: NativeStyles,
    numbers: NativeStyles,
    starts: Vec<f64>,
    ends: Vec<f64>,
    bounds: Rect,
    included: BTreeSet<usize>,
    edits: Vec<(usize, &'static str)>,
}
fn row(line: &Line, s: &Source<'_>) -> Option<Row> {
    if line.table.is_some()
        || line.words.is_empty()
        || line.words.iter().any(|&id| s.owners.get(id) != Some(&1))
    {
        return None;
    }
    let first = line
        .words
        .iter()
        .rposition(|&id| !piece(&s.words[id].text))?
        + 1;
    let label = &line.words[..first];
    let old_values = &line.words[first..];
    if old_values.is_empty() {
        return None;
    }
    let label_text = label
        .iter()
        .map(|&id| s.words[id].text.as_str())
        .collect::<String>();
    if !label_text.chars().any(char::is_alphabetic)
        || label_text.contains(['。', '、', '「', '」', '『', '』', '!', '?', '！', '？'])
    {
        return None;
    }
    let labels = s.styles(label)?;
    let positions = old_values
        .iter()
        .map(|&id| s.pos(id))
        .collect::<Option<Vec<_>>>()?;
    let low = positions.iter().map(|p| p[2]).min_by(f64::total_cmp)?;
    let high = positions.iter().map(|p| p[2]).max_by(f64::total_cmp)?;
    let top = old_values
        .iter()
        .map(|&id| s.words[id].y0 as f64)
        .max_by(f64::total_cmp)?;
    let bottom = old_values
        .iter()
        .map(|&id| s.words[id].y1 as f64)
        .min_by(f64::total_cmp)?;
    if low < top || high > bottom {
        return None;
    }
    let left = label
        .iter()
        .map(|&id| s.pos(id).map(|p| p[0]))
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .min_by(f64::total_cmp)?;
    let label_right = label
        .iter()
        .map(|&id| s.pos(id).map(|p| p[1]))
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .max_by(f64::total_cmp)?;
    if !label
        .iter()
        .any(|&id| s.words[id].y0 as f64 <= low && high <= s.words[id].y1 as f64)
    {
        return None;
    }
    let mut values = old_values.to_vec();
    values.extend(s.mark_ids.iter().copied().filter(|&id| {
        s.owners[id] == 0
            && s.pos(id).is_some_and(|p| {
                p[0] >= label_right
                    && top <= p[2]
                    && p[2] <= bottom
                    && s.words[id].y0 as f64 <= low
                    && high <= s.words[id].y1 as f64
            })
    }));
    values.sort_by(|&a, &b| s.pos(a).unwrap()[0].total_cmp(&s.pos(b).unwrap()[0]));
    let joined = |a: usize, b: usize| {
        let ranges = s
            .terminals
            .get(&s.words[a])
            .zip(s.terminals.get(&s.words[b]))
            .and_then(|(at, bt)| at.range_for(&s.words[a]).zip(bt.range_for(&s.words[b])));
        let (a_pos, b_pos) = (s.pos(a).unwrap(), s.pos(b).unwrap());
        ranges.is_some_and(|(a, b)| a.end == b.start)
            && a_pos[2] == b_pos[2]
            && a_pos[0] < b_pos[0]
            && a_pos[1] < b_pos[1]
            && b_pos[0] <= a_pos[1]
    };
    if values
        .iter()
        .copied()
        .filter(|&id| !s.marks[id])
        .collect::<Vec<_>>()
        != old_values
        || values
            .windows(2)
            .any(|p| s.pos(p[0]).unwrap()[1] > s.pos(p[1]).unwrap()[0] && !joined(p[0], p[1]))
        || values
            .first()
            .is_none_or(|&id| s.pos(id).unwrap()[0] < label_right)
    {
        return None;
    }
    let mut fields: Vec<Vec<usize>> = vec![];
    for &id in &values {
        if let Some(last) = fields.last_mut().filter(|last| {
            !s.marks[id] && !s.marks[*last.last().unwrap()] && joined(*last.last().unwrap(), id)
        }) {
            last.push(id);
        } else {
            fields.push(vec![id]);
        }
    }
    let numeric = fields.iter().filter(|f| !s.marks[f[0]]).collect::<Vec<_>>();
    if numeric.is_empty()
        || numeric.iter().any(|f| {
            !super::native_numeric_frames::amount(
                &f.iter()
                    .map(|&id| s.words[id].text.as_str())
                    .collect::<String>(),
            )
        })
    {
        return None;
    }
    // Preserved normalized punctuation need not pretend to be resolved font
    // content. The digit-bearing originals certify the numerical source style.
    let numbers = s.styles(
        &numeric
            .iter()
            .flat_map(|f| f.iter().copied())
            .filter(|&id| s.words[id].text.chars().any(|c| c.is_ascii_digit()))
            .collect::<Vec<_>>(),
    )?;
    let chars = line
        .text
        .char_indices()
        .filter(|(_, c)| !c.is_whitespace())
        .collect::<Vec<_>>();
    let flat = chars.iter().map(|c| c.1).collect::<String>();
    let label_text = compact(&label_text);
    let mut rest = flat.strip_prefix(&label_text)?;
    let mut consumed = label_text.chars().count();
    let mut offsets = vec![];
    let mut preserved = BTreeSet::new();
    for (field_index, field) in fields.iter().enumerate() {
        if s.marks[field[0]] {
            offsets.push(None);
            continue;
        }
        offsets.push(Some(chars.get(consumed)?.0));
        let text = field
            .iter()
            .map(|&id| compact(&s.words[id].text))
            .collect::<String>();
        if !rest.starts_with(&text) {
            let first = rest.chars().next()?;
            if !"△▲−－+-".contains(first) {
                return None;
            }
            let from = if field_index == 0 {
                label_right
            } else {
                s.pos(*fields[field_index - 1].last().unwrap())?[1]
            };
            let start = s.pos(field[0])?[0];
            let witnesses = s
                .words
                .iter()
                .filter(|w| {
                    matches!(w.status, WordStatus::Used | WordStatus::Noise)
                        && s.owners[w.id] == 0
                        && w.text == first.to_string()
                        && s.pos(w.id).is_some_and(|p| {
                            from <= p[0]
                                && p[1] <= start
                                && top <= p[2]
                                && p[2] <= bottom
                                && w.y0 as f64 <= low
                                && high <= w.y1 as f64
                        })
                })
                .collect::<Vec<_>>();
            if witnesses.len() > 1 {
                return None;
            }
            if let Some(w) = witnesses.first() {
                preserved.insert(w.id);
            }
            rest = &rest[first.len_utf8()..];
            consumed += 1;
        }
        rest = rest.strip_prefix(&text)?;
        consumed += text.chars().count();
    }
    if !rest.is_empty() {
        return None;
    }
    let mut edits = vec![];
    for (i, field) in fields.iter().enumerate() {
        if s.marks[field[0]] {
            let at = offsets[i + 1..]
                .iter()
                .copied()
                .flatten()
                .next()
                .unwrap_or(line.text.len());
            edits.push((
                at,
                if at == line.text.len() {
                    "　－"
                } else {
                    "－　"
                },
            ));
        }
    }
    let ids = label.iter().chain(&values).copied().collect::<Vec<_>>();
    let marks = values.iter().copied().filter(|&id| s.marks[id]).collect();
    let starts = fields.iter().map(|f| s.pos(f[0]).unwrap()[0]).collect();
    let ends = fields
        .iter()
        .map(|f| s.pos(*f.last().unwrap()).unwrap()[1])
        .collect();
    let bounds = ids
        .iter()
        .map(|&id| {
            let w = &s.words[id];
            [w.x0, w.y0, w.x1, w.y1]
        })
        .reduce(|a, b| {
            [
                a[0].min(b[0]),
                a[1].min(b[1]),
                a[2].max(b[2]),
                a[3].max(b[3]),
            ]
        })?;
    let included = ids
        .iter()
        .copied()
        .chain(preserved)
        .collect::<BTreeSet<_>>();
    Some(Row {
        ids,
        marks,
        left,
        label_right,
        labels,
        numbers,
        starts,
        ends,
        bounds,
        included,
        edits,
    })
}
fn supports(a: &Row, b: &Row) -> bool {
    b.marks.is_empty()
        && a.left < b.label_right
        && b.left < a.label_right
        && a.labels == b.labels
        && a.numbers == b.numbers
        && a.ends == b.ends
        && a.ends
            .windows(2)
            .enumerate()
            .all(|(i, w)| w[0] < a.starts[i + 1].min(b.starts[i + 1]))
}
fn insert(line: &Line, r: &Row) -> String {
    let mut out = line.text.clone();
    for &(at, text) in r.edits.iter().rev() {
        out.insert_str(at, text);
    }
    out
}
pub(crate) fn restore(
    units: &mut [Unit],
    words: &mut [Word],
    styles: &[ContentStyles],
    page: scraper::ElementRef<'_>,
    rules: &PageRules,
) {
    if !words
        .iter()
        .any(|w| w.status == WordStatus::Empty && w.text.is_empty())
    {
        return;
    }
    if !page
        .select(&scraper::Selector::parse("word[data-native-outline-rectangle]").unwrap())
        .any(|_| true)
    {
        return;
    }
    let terminals = native_terminal_glyph::for_words(page, words);
    let outlines = super::native_word_outlines::for_words(page, words, &terminals);
    let marks = words
        .iter()
        .enumerate()
        .map(|(id, w)| {
            w.status == WordStatus::Empty
                && w.text.is_empty()
                && outlines[id].is_some_and(|b| b[2] - b[0] > b[3] - b[1])
        })
        .collect::<Vec<_>>();
    if !marks.iter().any(|&b| b) {
        return;
    }
    let mut owners = vec![0; words.len()];
    let mut valid = true;
    let mut count = |line: &Line| {
        for &id in line.words.iter().chain(
            line.table
                .iter()
                .flat_map(|t| t.cells.iter().flat_map(|c| &c.words)),
        ) {
            if let Some(n) = owners.get_mut(id) {
                *n += 1;
            } else {
                valid = false;
            }
        }
    };
    for unit in units.iter() {
        match unit {
            Unit::Plain(ls) => ls.iter().for_each(&mut count),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
                ..
            } => preamble
                .iter()
                .chain(after.iter().flatten())
                .chain(before.iter().flatten())
                .chain(outside.iter().flatten())
                .for_each(&mut count),
        }
    }
    if !valid {
        return;
    }
    let mark_ids = marks
        .iter()
        .enumerate()
        .filter_map(|(i, &v)| v.then_some(i))
        .collect();
    let s = Source {
        words,
        styles,
        terminals,
        marks,
        owners,
        mark_ids,
    };
    let mut plans = vec![];
    for (ui, unit) in units.iter().enumerate() {
        let Unit::Plain(lines) = unit else { continue };
        let rows = lines.iter().map(|l| row(l, &s)).collect::<Vec<_>>();
        for (i, a) in rows.iter().enumerate() {
            let Some(a) = a.as_ref().filter(|r| !r.marks.is_empty()) else {
                continue;
            };
            let mut neighbors = vec![];
            for j in (0..i).rev() {
                let Some(b) = &rows[j] else { break };
                neighbors.push(b);
            }
            for item in &rows[i + 1..] {
                let Some(b) = item else { break };
                neighbors.push(b);
            }
            let clear_words = |r: &Row| {
                !s.words
                    .iter()
                    .any(|w| !r.included.contains(&w.id) && overlaps(r.bounds, w.source_box))
            };
            let witnesses = neighbors
                .into_iter()
                .filter(|b| supports(a, b) && clear_words(b))
                .map(|b| b.bounds)
                .collect::<Vec<_>>();
            if !witnesses.is_empty() && clear_words(a) {
                plans.push((
                    ui,
                    i,
                    a.ids.clone(),
                    a.marks.clone(),
                    insert(&lines[i], a),
                    a.bounds,
                    witnesses,
                ));
            }
        }
    }
    if plans.is_empty() {
        return;
    }
    let Some(scene) =
        super::native_source_scene::for_page(page, rules).filter(|s| s.matches(words))
    else {
        return;
    };
    let plans = plans
        .into_iter()
        .filter(|(_, _, _, _, _, bounds, witnesses)| {
            scene.unobstructed(*bounds) && witnesses.iter().any(|b| scene.unobstructed(*b))
        })
        .collect::<Vec<_>>();
    let mut uses = vec![0; words.len()];
    for (_, _, _, marks, _, _, _) in &plans {
        for &id in marks {
            uses[id] += 1;
        }
    }
    let changes = plans
        .into_iter()
        .filter(|(_, _, _, marks, _, _, _)| marks.iter().all(|&id| uses[id] == 1))
        .collect::<Vec<_>>();
    for (u, i, ids, marks, text, _, _) in changes {
        let Unit::Plain(lines) = &mut units[u] else {
            continue;
        };
        lines[i].words = ids;
        lines[i].text = text;
        for id in marks {
            words[id].text = "－".into();
            words[id].status = WordStatus::Used;
        }
    }
}
#[cfg(test)]
#[path = "native_amount_mark_tests.rs"]
mod tests;
