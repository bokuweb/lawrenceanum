//! Restore only CFF-verified circled 11 and the unique overlaid 2+1 pair after layout.
//! Both components keep their original IDs and geometry; the right half owns no duplicate text.
use crate::trace::{Line, Unit, Word, WordStatus};
use std::collections::{BTreeMap, BTreeSet};
fn private(c: char) -> bool {
    crate::normalize::is_private_use(c)
        || matches!(c as u32, 0xF0000..=0xFFFFD | 0x100000..=0x10FFFD)
}
fn replacement(c: char, paired: bool) -> Option<&'static str> {
    match c {
        '\u{E7BC}' => Some("⑪"),
        '\u{EAD6}' if paired => Some("㉑"),
        _ => None,
    }
}
fn mapped(w: &Word, paired: bool) -> Option<String> {
    if w.raw.chars().any(|c| {
        if !private(c) || replacement(c, paired).is_some() {
            return false;
        }
        if c as u32 >= 0xF0000 {
            return true;
        }
        // A known inline dash normalizes empty in isolation but is not an unknown glyph.
        let n = crate::normalize::word_text(&c.to_string());
        crate::normalize::text_char(c).is_none() && (n.is_empty() || n.chars().any(private))
    }) {
        return None;
    }
    let (mut old, mut new, mut part) = (String::new(), String::new(), String::new());
    for c in w.raw.chars() {
        if let Some(m) = replacement(c, paired) {
            let n = crate::normalize::word_text(&part);
            old.push_str(&n);
            new.push_str(&n);
            new.push_str(m);
            part.clear();
        } else {
            part.push(c);
        }
    }
    let n = crate::normalize::word_text(&part);
    old.push_str(&n);
    new.push_str(&n);
    (old == w.text && new != w.text).then_some(new)
}
fn same_tail(a: &Word, b: &Word) -> bool {
    (a.y0 - b.y0).abs() < 0.02
        && (a.y1 - b.y1).abs() < 0.02
        && (a.x1 - b.x1).abs() < 0.02
        && a.x0 <= b.x0 + 0.02
}
fn partner(w: &Word, words: &[Word], attached: &BTreeMap<usize, Vec<usize>>) -> Option<usize> {
    if !w.raw.ends_with('\u{EAD6}') || w.raw.chars().filter(|&c| c == '\u{EAD6}').count() != 1 {
        return None;
    }
    let candidates = words
        .iter()
        .filter(|a| a.raw == "\u{EADF}" && same_tail(w, a))
        .collect::<Vec<_>>();
    if candidates.len() != 1 {
        return None;
    }
    let right = candidates[0];
    let em = right.y1 - right.y0;
    let width = right.x1 - right.x0;
    if right.status != WordStatus::Empty
        || !right.text.is_empty()
        || attached.contains_key(&right.id)
        || !(5.0..=20.0).contains(&em)
        || width < em * 0.7
        || width > em * 1.3
    {
        return None;
    }
    // A suffix word may be wider than the single overlaid glyph. Its rightmost cell must
    // have exactly one left component; two words sharing that endpoint are ambiguous.
    if words
        .iter()
        .filter(|a| a.raw.ends_with('\u{EAD6}') && same_tail(a, right))
        .count()
        != 1
    {
        return None;
    }
    // A third glyph at the same endpoint is another composite, not the verified 2+1 pair.
    if words
        .iter()
        .filter(|a| same_tail(a, right))
        .any(|a| a.id != w.id && a.id != right.id)
    {
        return None;
    }
    // All verified source fonts advance these raw glyphs by one full character cell.
    // A collapsed mixed prefix could conceal a third overlaid glyph in the same Word;
    // require its complete raw sequence to occupy the expected distinct cells.
    let expected_width = width * w.raw.chars().count() as f32;
    if ((w.x1 - w.x0) - expected_width).abs() > width * 0.03 {
        return None;
    }
    Some(right.id)
}
fn all_lines(units: &[Unit]) -> Vec<&Line> {
    let mut out = vec![];
    for u in units {
        match u {
            Unit::Plain(p) => out.extend(p),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            ..} => {
                out.extend(preamble);
                for p in after.iter().chain(before).chain(outside) {
                    out.extend(p);
                }
            }
        }
    }
    out
}
fn all_lines_mut(units: &mut [Unit]) -> Vec<&mut Line> {
    let mut out = vec![];
    for u in units {
        match u {
            Unit::Plain(p) => out.extend(p),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            ..} => {
                out.extend(preamble);
                for p in after.iter_mut().chain(before).chain(outside) {
                    out.extend(p);
                }
            }
        }
    }
    out
}
#[derive(Clone)]
struct Owner {
    line: usize,
    cell: Option<usize>,
    text: String,
    ids: Vec<usize>,
    bounds: Option<[f32; 4]>,
    editable: bool,
}
struct Edit {
    owner: usize,
    start: usize,
    end: usize,
    text: String,
    id: usize,
    word_text: String,
    append_id: bool,
    companion: Option<usize>,
}
fn unique(text: &str, part: &str) -> Option<(usize, usize)> {
    if part.is_empty() {
        return None;
    }
    let mut p = text.match_indices(part);
    let (start, _) = p.next()?;
    p.next().is_none().then_some((start, start + part.len()))
}
fn contains(b: [f32; 4], w: &Word) -> bool {
    let t = (w.y1 - w.y0) * 0.03;
    w.x0 >= b[0] - t && w.y0 >= b[1] - t && w.x1 <= b[2] + t && w.y1 <= b[3] + t
}
fn insertion(o: &Owner, w: &Word, words: &[Word], m: &str) -> Option<(usize, String)> {
    let em = w.y1 - w.y0;
    if o.text.is_empty() {
        return o.bounds.map(|_| (0, m.into()));
    }
    let anchors = o
        .ids
        .iter()
        .filter_map(|&id| words.get(id))
        .filter(|a| {
            !a.text.is_empty()
                && a.status == WordStatus::Used
                && (a.y0 - w.y0).abs() < em * 0.2
                && ((a.y1 - a.y0) - em).abs() < em * 0.2
        })
        .collect::<Vec<_>>();
    if anchors
        .iter()
        .any(|a| a.x0 < w.x1 - em * 0.05 && a.x1 > w.x0 + em * 0.05)
    {
        return None;
    }
    let left = anchors
        .iter()
        .copied()
        .filter(|a| a.x1 <= w.x0 + em * 0.05)
        .max_by(|a, b| a.x1.total_cmp(&b.x1));
    let right = anchors
        .iter()
        .copied()
        .filter(|a| a.x0 >= w.x1 - em * 0.05)
        .min_by(|a, b| a.x0.total_cmp(&b.x0));
    if o.bounds.is_none()
        && left
            .map(|a| w.x0 - a.x1)
            .unwrap_or(f32::INFINITY)
            .min(right.map(|a| a.x0 - w.x1).unwrap_or(f32::INFINITY))
            > em * 1.5
    {
        return None;
    }
    let l = left.and_then(|a| unique(&o.text, &a.text).map(|p| (a, p)));
    let r = right.and_then(|a| unique(&o.text, &a.text).map(|p| (a, p)));
    if let (Some((_, lp)), Some((_, rp))) = (l, r) {
        if lp.1 > rp.0 {
            return None;
        }
    }
    if let Some((a, p)) = l {
        return Some((
            p.1,
            format!("{}{m}", if w.x0 - a.x1 > em * 0.6 { "　" } else { "" }),
        ));
    }
    r.map(|(a, p)| {
        (
            p.0,
            format!("{m}{}", if a.x0 - w.x1 > em * 0.6 { "　" } else { "" }),
        )
    })
}
pub(crate) fn restore(units: &mut [Unit], words: &mut [Word]) {
    let lines = all_lines(units);
    let mut owners = vec![];
    let mut attached: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (li, l) in lines.iter().enumerate() {
        if !l.words.is_empty() || l.table.is_none() {
            let oi = owners.len();
            for &id in &l.words {
                attached.entry(id).or_default().push(oi);
            }
            owners.push(Owner {
                line: li,
                cell: None,
                text: l.text.clone(),
                ids: l.words.clone(),
                bounds: None,
                editable: l.table.is_none(),
            });
        }
        if let Some(t) = &l.table {
            for (ci, c) in t.cells.iter().enumerate() {
                let oi = owners.len();
                for &id in &c.words {
                    attached.entry(id).or_default().push(oi);
                }
                owners.push(Owner {
                    line: li,
                    cell: Some(ci),
                    text: c.text.clone(),
                    ids: c.words.clone(),
                    bounds: Some([c.x0, c.y0, c.x1, c.y1]),
                    editable: true,
                });
            }
        }
    }
    let mut edits = vec![];
    for w in words
        .iter()
        .filter(|w| w.raw.contains('') || w.raw.contains(''))
    {
        if words.get(w.id).is_none_or(|a| a.id != w.id) {
            continue;
        }
        let companion = if w.raw.contains('') {
            let Some(id) = partner(w, words, &attached) else {
                continue;
            };
            Some(id)
        } else {
            None
        };
        let Some(new) = mapped(w, companion.is_some()) else {
            continue;
        };
        let existing = attached.get(&w.id).map(Vec::as_slice).unwrap_or(&[]);
        if !w.text.is_empty() {
            if w.status != WordStatus::Used || existing.len() != 1 {
                continue;
            }
            let oi = existing[0];
            let o = &owners[oi];
            if !o.editable {
                continue;
            }
            if let Some((start, end)) = unique(&o.text, &w.text) {
                edits.push(Edit {
                    owner: oi,
                    start,
                    end,
                    text: new.clone(),
                    id: w.id,
                    word_text: new,
                    append_id: false,
                    companion,
                });
            }
            continue;
        }
        if w.status != WordStatus::Empty || !existing.is_empty() || w.raw.chars().count() != 1 {
            continue;
        }
        let em = w.y1 - w.y0;
        let width = w.x1 - w.x0;
        if !(5.0..=20.0).contains(&em) || width < em * 0.7 || width > em * 1.3 {
            continue;
        }
        let candidates = owners
            .iter()
            .enumerate()
            .filter(|(_, o)| o.editable && o.bounds.is_none_or(|b| contains(b, w)))
            .filter_map(|(oi, o)| insertion(o, w, words, &new).map(|(pos, text)| (oi, pos, text)))
            .collect::<Vec<_>>();
        if candidates.len() != 1 {
            continue;
        }
        let (oi, pos, text) = candidates.into_iter().next().unwrap();
        edits.push(Edit {
            owner: oi,
            start: pos,
            end: pos,
            text,
            id: w.id,
            word_text: new,
            append_id: true,
            companion,
        });
    }
    let mut conflicts = BTreeSet::new();
    for (i, a) in edits.iter().enumerate() {
        for b in &edits[i + 1..] {
            if a.owner == b.owner && (a.start == b.start || a.start < b.end && b.start < a.end) {
                conflicts.insert(a.owner);
            }
            if a.id == b.id
                || a.companion
                    .is_some_and(|id| id == b.id || b.companion == Some(id))
                || b.companion == Some(a.id)
            {
                conflicts.insert(a.owner);
                conflicts.insert(b.owner);
            }
        }
    }
    drop(lines);
    let mut lines = all_lines_mut(units);
    let mut grouped: BTreeMap<usize, Vec<Edit>> = BTreeMap::new();
    for e in edits {
        if !conflicts.contains(&e.owner) {
            grouped.entry(e.owner).or_default().push(e);
        }
    }
    let mut tables = BTreeSet::new();
    for (oi, mut es) in grouped {
        let o = &owners[oi];
        es.sort_by_key(|e| std::cmp::Reverse(e.start));
        let mut text = o.text.clone();
        let mut added = vec![];
        for e in es {
            text.replace_range(e.start..e.end, &e.text);
            if e.append_id {
                added.push(e.id);
            }
            words[e.id].text = e.word_text;
            words[e.id].status = WordStatus::Used;
            if let Some(id) = e.companion {
                // The right component belongs to the same displayed character. Keep its raw
                // text/bbox and attach its ID exactly once, without emitting a second ㉑.
                words[id].status = WordStatus::Used;
                added.push(id);
            }
        }
        let l = &mut lines[o.line];
        if let Some(ci) = o.cell {
            let c = &mut l.table.as_mut().unwrap().cells[ci];
            c.text = text;
            c.words.extend(added);
            tables.insert(o.line);
        } else {
            l.text = text;
            l.words.extend(added);
        }
    }
    for li in tables {
        lines[li].text = lines[li].table.as_ref().unwrap().render();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::{Table, TableCell};
    fn w(id: usize, raw: &str, text: &str, x: f32) -> Word {
        Word {
            source_box: [x, 0., x + 8., 8.],
            id,
            raw: raw.into(),
            text: text.into(),
            x0: x,
            y0: 0.,
            x1: x + 8.,
            y1: 8.,
            status: if text.is_empty() {
                WordStatus::Empty
            } else {
                WordStatus::Used
            },
        }
    }
    fn plain(text: &str, ids: &[usize]) -> Vec<Unit> {
        vec![Unit::Plain(vec![Line::text(text.into(), ids.to_vec())])]
    }
    fn text(u: &[Unit]) -> String {
        all_lines(u)[0].text.clone()
    }
    fn snapshot(u: &[Unit], ws: &[Word]) -> String {
        format!("{:?}{ws:?}", all_lines(u))
    }
    fn unchanged(mut u: Vec<Unit>, mut ws: Vec<Word>) {
        let before = snapshot(&u, &ws);
        restore(&mut u, &mut ws);
        assert_eq!(snapshot(&u, &ws), before);
    }
    #[test]
    fn mixed_eleven_keeps_repaired_line_and_existing_ids() {
        let mut ws = vec![w(0, "⑨から\u{E7BC}まで", "⑨からまで", 0.)];
        let mut u = plain("“既修復”　⑨からまで　(1/2)　科目\n折返し　1㎡", &[0]);
        restore(&mut u, &mut ws);
        assert_eq!(text(&u), "“既修復”　⑨から⑪まで　(1/2)　科目\n折返し　1㎡");
        assert_eq!(all_lines(&u)[0].words, [0]);
        assert_eq!(ws[0].raw, "⑨から\u{E7BC}まで");
        let once = snapshot(&u, &ws);
        restore(&mut u, &mut ws);
        assert_eq!(snapshot(&u, &ws), once);
    }
    #[test]
    fn empty_eleven_uses_unique_nearby_owner() {
        let mut ws = vec![
            w(0, "⑨から", "⑨から", 0.),
            w(1, "\u{E7BC}", "", 8.),
            w(2, "まで", "まで", 16.),
        ];
        let mut u = plain("⑨からまで", &[0, 2]);
        restore(&mut u, &mut ws);
        assert_eq!(text(&u), "⑨から⑪まで");
        assert_eq!(all_lines(&u)[0].words, [0, 2, 1]);
        assert_eq!(ws[1].text, "⑪");
        assert_eq!(ws[1].status, WordStatus::Used);
    }
    #[test]
    fn used_pair_emits_one_twenty_one_and_owns_both_component_ids() {
        let mut ws = vec![w(0, "６\u{EAD6}", "６", 0.), w(1, "\u{EADF}", "", 8.)];
        ws[0].x1 = 16.;
        let mut u = plain("６に掲げる事項", &[0]);
        restore(&mut u, &mut ws);
        assert_eq!(text(&u), "６㉑に掲げる事項");
        assert_eq!(ws[0].text, "６㉑");
        assert_eq!(ws[1].text, "");
        assert_eq!(ws[1].status, WordStatus::Used);
        assert_eq!(all_lines(&u)[0].words, [0, 1]);
        let once = snapshot(&u, &ws);
        restore(&mut u, &mut ws);
        assert_eq!(snapshot(&u, &ws), once);
    }
    #[test]
    fn empty_pair_can_use_existing_empty_cell_without_new_table() {
        let cells = vec![
            TableCell {
                row: 0,
                col: 0,
                rowspan: 1,
                colspan: 1,
                x0: 0.,
                y0: 0.,
                x1: 20.,
                y1: 10.,
                text: "".into(),
                words: vec![],
            },
            TableCell {
                row: 0,
                col: 1,
                rowspan: 1,
                colspan: 1,
                x0: 20.,
                y0: 0.,
                x1: 40.,
                y1: 10.,
                text: "2,000".into(),
                words: vec![2],
            },
        ];
        let t = Table {
            x0: 0.,
            y0: 0.,
            x1: 40.,
            y1: 10.,
            rows: 1,
            cols: 2,
            cells,
        };
        let mut u = vec![Unit::Plain(vec![Line {
            text: t.render(),
            words: vec![],
            table: Some(t),
        }])];
        let mut ws = vec![
            w(0, "\u{EAD6}", "", 4.),
            w(1, "\u{EADF}", "", 4.),
            w(2, "2,000", "2,000", 24.),
        ];
        restore(&mut u, &mut ws);
        let table = all_lines(&u)[0].table.as_ref().unwrap();
        assert_eq!(table.cells[0].text, "㉑");
        assert_eq!(table.cells[0].words, [0, 1]);
        assert_eq!(table.cells[1].text, "2,000");
        assert_eq!(table.cells[1].words, [2]);
        assert_eq!(ws[0].text, "㉑");
        assert_eq!(ws[1].text, "");
    }
    #[test]
    fn isolated_half_or_other_overlaid_digit_is_not_guessed() {
        unchanged(plain("６", &[0]), vec![w(0, "６\u{EAD6}", "６", 0.)]);
        unchanged(
            plain("６", &[0]),
            vec![w(0, "６\u{EAD6}", "６", 0.), w(1, "\u{EAE0}", "", 0.)],
        );
        unchanged(plain("", &[]), vec![w(0, "\u{EADF}", "", 0.)]);
    }
    #[test]
    fn pair_requires_exact_geometry_unique_halves_and_unowned_empty_right() {
        let mut first = w(0, "６\u{EAD6}", "６", 0.);
        first.x1 = 16.;
        let mut off = w(1, "\u{EADF}", "", 8.);
        off.y0 = 1.;
        off.y1 = 9.;
        unchanged(plain("６", &[0]), vec![first.clone(), off]);
        unchanged(
            plain("６", &[0]),
            vec![
                first.clone(),
                w(1, "\u{EADF}", "", 8.),
                w(2, "\u{EADF}", "", 8.),
            ],
        );
        unchanged(
            plain("６", &[0, 1]),
            vec![first.clone(), w(1, "\u{EADF}", "", 8.)],
        );
        let mut second = first.clone();
        second.id = 2;
        unchanged(
            plain("６", &[0]),
            vec![first, w(1, "\u{EADF}", "", 8.), second],
        );
    }
    #[test]
    fn other_unknown_private_use_keeps_entire_word_unchanged() {
        for (raw, t) in [
            ("⑨\u{E7BC}\u{E7BD}", "⑨"),
            ("\u{E7BC}\u{E952}", ""),
            ("６\u{EAD6}\u{EADF}", "６"),
            ("\u{E7BC}\u{FE952}", ""),
            ("\u{E7BC}\u{F4E00}", "一"),
        ] {
            unchanged(plain(t, &[0]), vec![w(0, raw, t, 0.)]);
        }
    }
    #[test]
    fn known_inline_dash_is_not_an_unknown_pua() {
        let raw = "E\u{E61C}\u{E7BC}\u{E61C}48521";
        let old = crate::normalize::word_text(raw);
        let mut ws = vec![w(0, raw, &old, 0.)];
        let mut u = plain(&old, &[0]);
        restore(&mut u, &mut ws);
        assert_eq!(text(&u), "E－⑪－48521");
    }
    #[test]
    fn repaired_word_text_or_ambiguous_owner_and_substring_are_not_rebuilt() {
        unchanged(plain("1/2", &[0]), vec![w(0, "1\u{E7BC}2", "1/2", 0.)]);
        unchanged(
            plain("科目\n折返し", &[0]),
            vec![w(0, "科目\u{E7BC}", "科目\n折返し", 0.)],
        );
        unchanged(plain("1㎡", &[0]), vec![w(0, "1\u{E7BC}", "1㎡", 0.)]);
        unchanged(plain("６６", &[0]), vec![w(0, "６\u{E7BC}", "６", 0.)]);
        let mut u = plain("６", &[0]);
        u.extend(plain("６", &[0]));
        unchanged(u, vec![w(0, "６\u{E7BC}", "６", 0.)]);
    }
    #[test]
    fn empty_marker_does_not_cross_distant_line_or_ambiguous_cell() {
        unchanged(
            plain("まで", &[0]),
            vec![w(0, "まで", "まで", 100.), w(1, "\u{E7BC}", "", 0.)],
        );
        let mut u = plain("から", &[0]);
        u.extend(plain("から", &[0]));
        unchanged(u, vec![w(0, "から", "から", 0.), w(1, "\u{E7BC}", "", 8.)]);
    }
    #[test]
    fn shinkyu_parts_keep_separate_membership() {
        let mut u = vec![Unit::Shinkyu {labels:None,
            preamble: vec![],
            after: vec![vec![Line::text("６".into(), vec![0])]],
            before: vec![vec![Line::text("５".into(), vec![1])]],
            outside: vec![],
        }];
        let mut ws = vec![w(0, "６\u{E7BC}", "６", 0.), w(1, "５\u{E7BC}", "５", 0.)];
        restore(&mut u, &mut ws);
        let ls = all_lines(&u);
        assert_eq!(ls[0].text, "６⑪");
        assert_eq!(ls[1].text, "５⑪");
        assert_eq!(ls[0].words, [0]);
        assert_eq!(ls[1].words, [1]);
    }
    #[test]
    fn mixed_quote_word_is_preserved_when_normalization_already_changed() {
        unchanged(
            plain("“６”", &[0]),
            vec![w(0, "\u{E20A}６\u{E7BC}\u{E209}", "“６”", 0.)],
        );
    }

    #[test]
    fn third_overlaid_component_is_not_a_unique_twenty_one() {
        let mut first = w(0, "６\u{EAD6}", "６", 0.);
        first.x1 = 16.;
        unchanged(
            plain("６", &[0]),
            vec![first, w(1, "\u{EADF}", "", 8.), w(2, "1", "1", 8.)],
        );
    }

    #[test]
    fn collapsed_mixed_prefix_does_not_prove_a_two_component_pair() {
        unchanged(
            plain("６", &[0]),
            vec![w(0, "６\u{EAD6}", "６", 8.), w(1, "\u{EADF}", "", 8.)],
        );
    }
}
