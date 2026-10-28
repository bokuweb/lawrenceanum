//! Restore CFF-verified parenthesized A/B/C references after layout, without rebuilding text.
use crate::trace::{Line, TableCell, Unit, Word, WordStatus};
use std::collections::{BTreeMap, BTreeSet};
fn marker(c: char) -> Option<&'static str> {
    match c {
        '\u{E94F}' => Some("(A)"),
        '\u{E950}' => Some("(B)"),
        '\u{E951}' => Some("(C)"),
        _ => None,
    }
}
fn mapped(w: &Word) -> Option<String> {
    // Do not turn a formula with other unknown reference letters into a partly restored formula.
    if w.raw
        .chars()
        .any(|c| ('\u{E952}'..='\u{E968}').contains(&c))
    {
        return None;
    }
    let (mut original, mut result, mut part) = (String::new(), String::new(), String::new());
    for c in w.raw.chars() {
        if let Some(m) = marker(c) {
            let n = crate::normalize::word_text(&part);
            original.push_str(&n);
            result.push_str(&n);
            result.push_str(m);
            part.clear();
        } else {
            part.push(c);
        }
    }
    let n = crate::normalize::word_text(&part);
    original.push_str(&n);
    result.push_str(&n);
    (original == w.text && result != w.text).then_some(result)
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
}
struct Slot {
    line: usize,
    cell: TableCell,
    id: usize,
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
fn insertion(o: &Owner, w: &Word, words: &[Word]) -> Option<(usize, String)> {
    let m = marker(w.raw.chars().next()?)?;
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
fn empty_symbol_slots(lines: &[&Line], w: &Word) -> Vec<Slot> {
    let mut out = vec![];
    for (li, line) in lines.iter().enumerate() {
        let Some(t) = &line.table else {
            continue;
        };
        let hs = t
            .cells
            .iter()
            .filter(|c| {
                c.row == 0 && c.col == 0 && c.rowspan == 1 && c.colspan == 1 && c.text == "記号"
            })
            .collect::<Vec<_>>();
        if hs.len() != 1 {
            continue;
        }
        let h = hs[0];
        let em = w.y1 - w.y0;
        if h.x1 - h.x0 > em * 4. || w.x0 < h.x0 || w.x1 > h.x1 || w.y0 < h.y1 || w.y1 > t.y1 {
            continue;
        }
        let mut bands: Vec<(f32, f32)> = vec![];
        for c in &t.cells {
            if c.col > 0
                && w.y0 >= c.y0
                && w.y1 <= c.y1
                && !bands
                    .iter()
                    .any(|&(a, b)| (a - c.y0).abs() < 0.05 && (b - c.y1).abs() < 0.05)
            {
                bands.push((c.y0, c.y1));
            }
        }
        if bands.len() != 1 {
            continue;
        }
        let (y0, y1) = bands[0];
        let mut rows = BTreeSet::new();
        for c in &t.cells {
            if (c.y0 - y0).abs() < 0.05 && (c.y1 - y1).abs() < 0.05 {
                rows.extend(c.row..c.row + c.rowspan);
            }
        }
        let (Some(&first), Some(&last)) = (rows.first(), rows.last()) else {
            continue;
        };
        if rows.len() != last - first + 1
            || t.cells
                .iter()
                .any(|c| c.col == 0 && (first..=last).any(|r| c.row <= r && r < c.row + c.rowspan))
        {
            continue;
        }
        out.push(Slot {
            line: li,
            cell: TableCell {
                row: first,
                col: 0,
                rowspan: last - first + 1,
                colspan: 1,
                x0: h.x0,
                y0,
                x1: h.x1,
                y1,
                text: marker(w.raw.chars().next().unwrap()).unwrap().into(),
                words: vec![w.id],
            },
            id: w.id,
        });
    }
    out
}
pub(crate) fn restore(units: &mut [Unit], words: &mut [Word]) {
    if !words
        .iter()
        .any(|w| w.raw.chars().any(|c| marker(c).is_some()))
    {
        return;
    }
    let lines = all_lines(units);
    let mut owners = vec![];
    let mut attached: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (li, line) in lines.iter().enumerate() {
        if !line.words.is_empty() || line.table.is_none() {
            let oi = owners.len();
            for &id in &line.words {
                attached.entry(id).or_default().push(oi);
            }
            owners.push(Owner {
                line: li,
                cell: None,
                text: line.text.clone(),
                ids: line.words.clone(),
                bounds: None,
                editable: line.table.is_none(),
            });
        }
        if let Some(t) = &line.table {
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
    let (mut edits, mut slots) = (vec![], vec![]);
    for w in words
        .iter()
        .filter(|w| w.raw.chars().any(|c| marker(c).is_some()))
    {
        let Some(new) = mapped(w) else {
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
            .filter_map(|(oi, o)| insertion(o, w, words).map(|(pos, text)| (oi, pos, text)))
            .collect::<Vec<_>>();
        let vacancy = empty_symbol_slots(&lines, w);
        if candidates.len() + vacancy.len() != 1 {
            continue;
        }
        if let Some((oi, pos, text)) = candidates.into_iter().next() {
            edits.push(Edit {
                owner: oi,
                start: pos,
                end: pos,
                text,
                id: w.id,
                word_text: new,
                append_id: true,
            });
        } else {
            slots.extend(vacancy);
        }
    }
    // Shared offsets or overlapping word replacements are ambiguous. Leave that whole owner alone.
    let mut conflicts = BTreeSet::new();
    for (i, a) in edits.iter().enumerate() {
        for b in &edits[i + 1..] {
            if a.owner == b.owner && (a.start == b.start || a.start < b.end && b.start < a.end) {
                conflicts.insert(a.owner);
            }
        }
    }
    let mut slot_conflicts = BTreeSet::new();
    for (i, a) in slots.iter().enumerate() {
        for b in &slots[i + 1..] {
            if a.line == b.line && a.cell.row == b.cell.row && a.cell.col == b.cell.col {
                slot_conflicts.insert((a.line, a.cell.row, a.cell.col));
            }
        }
    }
    drop(lines);
    let mut lines = all_lines_mut(units);
    let mut by_owner: BTreeMap<usize, Vec<Edit>> = BTreeMap::new();
    for e in edits {
        if !conflicts.contains(&e.owner) {
            by_owner.entry(e.owner).or_default().push(e);
        }
    }
    let mut changed = BTreeSet::new();
    for (oi, mut es) in by_owner {
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
        }
        let l = &mut lines[o.line];
        if let Some(ci) = o.cell {
            let c = &mut l.table.as_mut().unwrap().cells[ci];
            c.text = text;
            c.words.extend(added);
            changed.insert(o.line);
        } else {
            l.text = text;
            l.words.extend(added);
        }
    }
    for s in slots {
        if slot_conflicts.contains(&(s.line, s.cell.row, s.cell.col)) {
            continue;
        }
        words[s.id].text = s.cell.text.clone();
        words[s.id].status = WordStatus::Used;
        lines[s.line].table.as_mut().unwrap().cells.push(s.cell);
        changed.insert(s.line);
    }
    for li in changed {
        lines[li].text = lines[li].table.as_ref().unwrap().render();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::Table;
    fn word(id: usize, raw: &str, text: &str, x: f32, y: f32) -> Word {
        Word {
            source_box: [x, y, x + 8., y + 8.],
            id,
            raw: raw.into(),
            text: text.into(),
            x0: x,
            y0: y,
            x1: x + 8.,
            y1: y + 8.,
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
    fn rendered(units: &[Unit]) -> Vec<String> {
        all_lines(units).iter().map(|l| l.text.clone()).collect()
    }
    fn unchanged(mut units: Vec<Unit>, mut words: Vec<Word>) {
        let before = serde_json::to_string(&words).unwrap();
        let texts = rendered(&units);
        restore(&mut units, &mut words);
        assert_eq!(rendered(&units), texts);
        assert_eq!(serde_json::to_string(&words).unwrap(), before);
    }
    fn table_line() -> Line {
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
                text: "記号".into(),
                words: vec![0],
            },
            TableCell {
                row: 0,
                col: 1,
                rowspan: 1,
                colspan: 1,
                x0: 20.,
                y0: 0.,
                x1: 100.,
                y1: 10.,
                text: "内容".into(),
                words: vec![1],
            },
            TableCell {
                row: 1,
                col: 1,
                rowspan: 2,
                colspan: 1,
                x0: 20.,
                y0: 10.,
                x1: 100.,
                y1: 40.,
                text: "宿舎\n継続".into(),
                words: vec![2],
            },
        ];
        let t = Table {
            x0: 0.,
            y0: 0.,
            x1: 100.,
            y1: 40.,
            rows: 3,
            cols: 2,
            cells,
        };
        Line {
            text: t.render(),
            words: vec![],
            table: Some(t),
        }
    }
    fn table_words() -> Vec<Word> {
        vec![
            word(0, "記号", "記号", 0., 0.),
            word(1, "内容", "内容", 20., 0.),
            word(2, "宿舎継続", "宿舎\n継続", 25., 12.),
            word(3, "\u{E94F}", "", 5., 18.),
        ]
    }
    #[test]
    fn inline_reference_is_local_and_keeps_fraction_unit_and_account_wrap() {
        let mut words = vec![
            word(0, "退職\u{E94F}", "退職", 0., 0.),
            word(1, "1", "1", 40., 0.),
        ];
        let mut u = plain("退職　(1/2)　1㎡　科目\n折返し", &[0, 1]);
        restore(&mut u, &mut words);
        assert_eq!(rendered(&u), ["退職(A)　(1/2)　1㎡　科目\n折返し"]);
        assert_eq!(words[0].text, "退職(A)");
        assert_eq!(all_lines(&u)[0].words, [0, 1]);
        let once = serde_json::to_string(&words).unwrap();
        restore(&mut u, &mut words);
        assert_eq!(once, serde_json::to_string(&words).unwrap());
    }
    #[test]
    fn raw_text_mismatch_keeps_previously_repaired_fractions_and_units() {
        for (raw, text) in [
            ("1\u{E94F}2", "(1/2)"),
            ("科目\u{E94F}", "科目\n折返し"),
            ("1\u{E94F}", "1㎡"),
        ] {
            unchanged(plain(text, &[0]), vec![word(0, raw, text, 0., 0.)]);
        }
    }
    #[test]
    fn unknown_later_reference_keeps_formula_atomic() {
        unchanged(
            plain("−−", &[0]),
            vec![word(0, "\u{E952}=\u{E94F}−\u{E950}−\u{E951}", "−−", 0., 0.)],
        );
    }
    #[test]
    fn repeated_text_or_shared_owner_is_ambiguous() {
        unchanged(
            plain("退職退職", &[0]),
            vec![word(0, "退職\u{E94F}", "退職", 0., 0.)],
        );
        let mut units = plain("退職", &[0]);
        units.extend(plain("退職", &[0]));
        unchanged(units, vec![word(0, "退職\u{E94F}", "退職", 0., 0.)]);
    }
    #[test]
    fn no_owner_and_non_used_mixed_word_are_not_reassigned() {
        unchanged(
            plain("別項", &[]),
            vec![word(0, "退職\u{E94F}", "退職", 0., 0.)],
        );
        for status in [
            WordStatus::Margin,
            WordStatus::HeaderLabel,
            WordStatus::Noise,
            WordStatus::Ruby,
        ] {
            let mut w = word(0, "退職\u{E94F}", "退職", 0., 0.);
            w.status = status;
            unchanged(plain("退職", &[0]), vec![w]);
        }
    }
    #[test]
    fn standalone_marker_uses_one_nearby_baseline_and_preserves_other_text() {
        let mut w = vec![
            word(0, "注記", "注記", 8., 10.),
            word(1, "\u{E950}", "", 0., 10.),
        ];
        let mut u = plain("注記　1㎡　(1/2)", &[0]);
        restore(&mut u, &mut w);
        assert_eq!(rendered(&u), ["(B)注記　1㎡　(1/2)"]);
        assert_eq!(all_lines(&u)[0].words, [0, 1]);
    }
    #[test]
    fn duplicate_candidate_lines_or_distant_baselines_are_not_guessed() {
        let w = vec![
            word(0, "注記", "注記", 8., 10.),
            word(1, "\u{E950}", "", 0., 10.),
        ];
        let mut u = plain("注記", &[0]);
        u.extend(plain("注記", &[0]));
        unchanged(u, w);
        unchanged(
            plain("注記", &[0]),
            vec![
                word(0, "注記", "注記", 80., 10.),
                word(1, "\u{E950}", "", 0., 10.),
            ],
        );
        unchanged(
            plain("注記", &[0]),
            vec![
                word(0, "注記", "注記", 8., 30.),
                word(1, "\u{E950}", "", 0., 10.),
            ],
        );
    }
    #[test]
    fn a_previously_reordered_line_is_not_rebuilt() {
        unchanged(
            plain("右左", &[0, 1]),
            vec![
                word(0, "左", "左", 0., 0.),
                word(1, "右", "右", 16., 0.),
                word(2, "\u{E94F}", "", 8., 0.),
            ],
        );
    }
    #[test]
    fn overlapping_or_non_square_standalone_glyphs_are_rejected() {
        unchanged(
            plain("注記", &[0]),
            vec![
                word(0, "注記", "注記", 0., 0.),
                word(1, "\u{E94F}", "", 2., 0.),
            ],
        );
        let mut w = vec![
            word(0, "注記", "注記", 8., 0.),
            word(1, "\u{E94F}", "", 0., 0.),
        ];
        w[1].x1 = 2.;
        unchanged(plain("注記", &[0]), w);
    }
    #[test]
    fn competing_insertions_at_the_same_offset_leave_owner_unchanged() {
        unchanged(
            plain("注記", &[0]),
            vec![
                word(0, "注記", "注記", 16., 0.),
                word(1, "\u{E94F}", "", 0., 0.),
                word(2, "\u{E950}", "", 8., 0.),
            ],
        );
    }
    #[test]
    fn a_vacant_symbol_cell_uses_the_existing_physical_rowspan() {
        let mut u = vec![Unit::Plain(vec![table_line()])];
        let mut w = table_words();
        restore(&mut u, &mut w);
        let t = all_lines(&u)[0].table.as_ref().unwrap();
        let c = t.cells.iter().find(|c| c.col == 0 && c.row == 1).unwrap();
        assert_eq!((c.rowspan, c.colspan), (2, 1));
        assert_eq!(c.text, "(A)");
        assert_eq!(c.words, [3]);
        assert_eq!((c.y0, c.y1), (10., 40.));
        assert!(t
            .cells
            .iter()
            .any(|c| c.text == "宿舎\n継続" && c.words == [2]));
    }
    #[test]
    fn missing_symbol_column_requires_the_header_and_exact_nonconflicting_band() {
        let mut line = table_line();
        line.table.as_mut().unwrap().cells[0].text = "分類".into();
        unchanged(vec![Unit::Plain(vec![line])], table_words());
        let mut line = table_line();
        let mut overlapping = line.table.as_ref().unwrap().cells[2].clone();
        overlapping.y0 = 15.;
        overlapping.y1 = 38.;
        line.table.as_mut().unwrap().cells.push(overlapping);
        unchanged(vec![Unit::Plain(vec![line])], table_words());
        let mut line = table_line();
        let mut occupied = line.table.as_ref().unwrap().cells[0].clone();
        occupied.row = 1;
        occupied.y0 = 10.;
        occupied.y1 = 40.;
        occupied.text = "既存".into();
        occupied.words = vec![];
        line.table.as_mut().unwrap().cells.push(occupied);
        unchanged(vec![Unit::Plain(vec![line])], table_words());
    }
    #[test]
    fn a_marker_outside_the_physical_cell_does_not_create_a_row() {
        let mut w = table_words();
        w[3].y0 = 42.;
        w[3].y1 = 50.;
        unchanged(vec![Unit::Plain(vec![table_line()])], w);
    }
    #[test]
    fn an_existing_empty_glyph_owner_is_not_duplicated() {
        let mut l = table_line();
        l.table.as_mut().unwrap().cells[2].words.push(3);
        unchanged(vec![Unit::Plain(vec![l])], table_words());
    }
}
