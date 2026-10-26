//! Restore verified two-line brackets only after table layout is fixed.
use crate::trace::{Line, Unit, Word};
pub(crate) fn restore_large_parentheses(units: &mut [Unit], words: &[Word]) {
    fn lines(lines: &mut [Line], words: &[Word]) {
        for line in lines {
            let Some(table) = line.table.as_mut() else {
                continue;
            };
            let mut changed = false;
            for cell in &mut table.cells {
                if cell.words.len() != 2 || cell.text.starts_with(['（', '(']) || cell.text.ends_with(['）', ')']) {
                    continue;
                }
                let source: Vec<_> = cell.words.iter().filter_map(|&id| words.get(id)).collect();
                if source.len() != 2 {
                    continue;
                }
                let Some(top) = source
                    .iter()
                    .copied()
                    .find(|w| w.raw.starts_with('\u{EAED}') && w.raw.ends_with('\u{EAEF}'))
                else {
                    continue;
                };
                let Some(bottom) = source.iter().copied().find(|w| w.raw.starts_with('\u{EAEE}')) else {
                    continue;
                };
                let em = top.y1 - top.y0;
                if em <= 0.
                    || !(em * 0.8..=em * 1.3).contains(&(bottom.y0 - top.y0))
                    || (top.x0 - bottom.x0).abs() > em * 0.05
                    || ((bottom.y1 - bottom.y0) - em).abs() > em * 0.1
                {
                    continue;
                }
                let japanese = |s: &str| {
                    s.chars()
                        .all(|c| ('\u{3040}'..='\u{30ff}').contains(&c) || ('\u{3400}'..='\u{9fff}').contains(&c))
                };
                if top.text.chars().count() < 3
                    || bottom.text.is_empty()
                    || bottom.text.chars().count() > 3
                    || !japanese(&top.text)
                    || !japanese(&bottom.text)
                {
                    continue;
                }
                let ending: Vec<_> = words
                    .iter()
                    .filter(|w| {
                        w.raw == "\u{EAF0}"
                            && (w.x1 - top.x1).abs() <= em * 0.05
                            && (w.y0 - bottom.y0).abs() <= em * 0.05
                            && (w.y1 - bottom.y1).abs() <= em * 0.05
                            && w.x0 >= cell.x0
                            && w.x1 <= cell.x1 + em * 0.05
                            && w.y0 >= cell.y0
                            && w.y1 <= cell.y1 + em * 0.05
                    })
                    .collect();
                if ending.len() != 1 {
                    continue;
                }
                let compact: String = cell.text.chars().filter(|c| !c.is_whitespace()).collect();
                if compact != format!("{}{}", top.text, bottom.text) {
                    continue;
                }
                // Preserve the entire existing cell text, word IDs and geometry.
                // Do not rebuild its content from the raw source words.
                cell.text = format!("（{}）", cell.text);
                changed = true;
            }
            if changed {
                line.text = table.render();
            }
        }
    }
    for unit in units {
        match unit {
            Unit::Plain(p) => {
                restore_plain_period_parentheses(p, words);
                lines(p, words);
            }
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            ..} => {
                lines(preamble, words);
                for p in after.iter_mut().chain(before).chain(outside) {
                    lines(p, words);
                }
            }
        }
    }
}
/// The existing Plain lines and their word IDs are final. Only enclose a
/// two-line period when its four raw bracket corners occupy one rectangle.
fn restore_plain_period_parentheses(lines: &mut [Line], words: &[Word]) {
    fn compact(s: &str) -> String {
        s.chars().filter(|c| !c.is_whitespace()).collect()
    }
    fn digits(s: &str) -> Option<u32> {
        if s.is_empty() {
            return None;
        }
        s.chars().try_fold(0u32, |n, c| {
            let d = if c.is_ascii_digit() {
                c as u32 - '0' as u32
            } else if ('０'..='９').contains(&c) {
                c as u32 - '０' as u32
            } else {
                return None;
            };
            n.checked_mul(10)?.checked_add(d)
        })
    }
    fn date(s: &str) -> bool {
        let Some((y, rest)) = s.split_once('年') else {
            return false;
        };
        let Some((m, rest)) = rest.split_once('月') else {
            return false;
        };
        let Some(d) = rest.strip_suffix('日') else {
            return false;
        };
        let year = ["令和", "平成", "昭和", "大正", "明治"]
            .iter()
            .find_map(|era| y.strip_prefix(era))
            .unwrap_or(y);
        (year == "元" || digits(year).is_some_and(|n| n > 0 && n <= 9999))
            && digits(m).is_some_and(|n| (1..=12).contains(&n))
            && digits(d).is_some_and(|n| (1..=31).contains(&n))
    }
    fn corner(w: &Word, ch: char, start: bool) -> bool {
        let found = if start { w.raw.starts_with(ch) } else { w.raw.ends_with(ch) };
        found && w.raw.chars().filter(|c| ('\u{EAED}'..='\u{EAF0}').contains(c)).count() == 1 && w.status == crate::trace::WordStatus::Used
    }
    for i in 0..lines.len().saturating_sub(1) {
        let (top_lines, bottom_lines) = lines.split_at_mut(i + 1);
        let top_line = &mut top_lines[i];
        let bottom_line = &mut bottom_lines[0];
        if top_line.table.is_some() || bottom_line.table.is_some() || top_line.words.len() != 2 || !(2..=3).contains(&bottom_line.words.len()) {
            continue;
        }
        let ids = [top_line.words[0], top_line.words[1], bottom_line.words[0], bottom_line.words[1]];
        if ids.iter().any(|&id| words.get(id).is_none_or(|w| w.id != id)) || ids.iter().enumerate().any(|(k, id)| ids[k + 1..].contains(id)) {
            continue;
        }
        let [a, b, c, d] = ids.map(|id| &words[id]);
        if !corner(a, '\u{EAED}', true)
            || !corner(b, '\u{EAEF}', false)
            || !corner(c, '\u{EAEE}', true)
            || !corner(d, '\u{EAF0}', false)
            || a.text != "自"
            || c.text != "至"
            || !date(&b.text)
            || !date(&d.text)
        {
            continue;
        }
        let em = a.y1 - a.y0;
        if em <= 0.
            || (a.x0 - c.x0).abs() > em * 0.05
            || (b.x1 - d.x1).abs() > em * 0.05
            || !(em * 0.8..=em * 1.3).contains(&(c.y0 - a.y0))
            || (a.y0 - b.y0).abs() > em * 0.05
            || (c.y0 - d.y0).abs() > em * 0.05
            || [b, c, d].iter().any(|w| ((w.y1 - w.y0) - em).abs() > em * 0.1)
            || b.x0 < a.x1
            || d.x0 < c.x1
        {
            continue;
        }
        let suffix = if bottom_line.words.len() == 3 {
            let Some(w) = words.get(bottom_line.words[2]) else {
                continue;
            };
            if w.x0 < d.x1
                || (w.y0 - d.y0).abs() > em * 0.05
                || !(w.text.starts_with("(単位：") || w.text.starts_with("（単位："))
                || !(w.text.ends_with(')') || w.text.ends_with('）'))
            {
                continue;
            }
            compact(&w.text)
        } else {
            String::new()
        };
        if compact(&top_line.text) != format!("自{}", b.text) || compact(&bottom_line.text) != format!("至{}{}", d.text, suffix) {
            continue;
        }
        let matches: Vec<_> = bottom_line.text.match_indices(&d.text).collect();
        if matches.len() != 1 {
            continue;
        }
        let close = matches[0].0 + d.text.len();
        let Some(open) = top_line.text.find(|ch: char| !ch.is_whitespace()) else {
            continue;
        };
        // No words, word IDs, line boundaries or article/table roles are changed.
        // Keep the original strings, including whitespace and the unit suffix.
        top_line.text.insert(open, '（');
        bottom_line.text.insert(close, '）');
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::{Table, TableCell, WordStatus};
    fn fixture() -> (Vec<Unit>, Vec<Word>) {
        let w = |id, x0, y0, x1, y1, raw: &str, text: &str| Word {
            source_box: [x0, y0, x1, y1],
            id,
            x0,
            y0,
            x1,
            y1,
            raw: raw.into(),
            text: text.into(),
            status: WordStatus::Used,
        };
        let words = vec![
            w(0, 20., 100., 80., 106., "\u{EAED}社員退職給付引当\u{EAEF}", "社員退職給付引当"),
            w(1, 20., 106., 32., 112., "\u{EAEE}金", "金"),
            w(2, 74., 106., 80., 112., "\u{EAF0}", ""),
            w(3, 90., 106., 120., 112., "(119)", "(119)"),
        ];
        let cell = |col, x0, x1, t: &str, ids: Vec<usize>| TableCell {
            row: 0,
            col,
            rowspan: 1,
            colspan: 1,
            x0,
            y0: 90.,
            x1,
            y1: 120.,
            text: t.into(),
            words: ids,
        };
        let table = Table {
            x0: 10.,
            y0: 90.,
            x1: 130.,
            y1: 120.,
            rows: 1,
            cols: 2,
            cells: vec![cell(0, 10., 85., "社員退職給付引当金", vec![0, 1]), cell(1, 85., 130., "(119)", vec![3])],
        };
        let line = Line {
            text: table.render(),
            words: vec![],
            table: Some(table),
        };
        (vec![Unit::Plain(vec![line])], words)
    }
    fn line(u: &[Unit]) -> &Line {
        let Unit::Plain(l) = &u[0] else { panic!() };
        &l[0]
    }
    #[test]
    fn changes_only_the_existing_cell_text_and_rendered_line() {
        let (mut u, w) = fixture();
        let before = line(&u).clone();
        restore_large_parentheses(&mut u, &w);
        assert_eq!(line(&u).table.as_ref().unwrap().cells[0].text, "（社員退職給付引当金）");
        let mut normalized = line(&u).clone();
        normalized.text = before.text.clone();
        normalized.table.as_mut().unwrap().cells[0].text = before.table.as_ref().unwrap().cells[0].text.clone();
        assert_eq!(normalized, before);
    }
    #[test]
    fn missing_or_displaced_bottom_right_is_not_restored() {
        for case in 0..3 {
            let (mut u, mut w) = fixture();
            match case {
                0 => w[2].raw = "\u{EAF1}".into(),
                1 => w[2].x1 += 12.,
                _ => w[2].y0 += 8.,
            };
            let before = line(&u).clone();
            restore_large_parentheses(&mut u, &w);
            assert_eq!(line(&u), &before);
        }
    }
    #[test]
    fn bracket_halves_in_different_cells_are_not_combined() {
        let (mut u, w) = fixture();
        let Unit::Plain(l) = &mut u[0] else { panic!() };
        l[0].table.as_mut().unwrap().cells[0].words = vec![0];
        let before = line(&u).clone();
        restore_large_parentheses(&mut u, &w);
        assert_eq!(line(&u), &before);
    }
    #[test]
    fn extra_cell_content_is_not_enclosed() {
        let (mut u, w) = fixture();
        let Unit::Plain(l) = &mut u[0] else { panic!() };
        l[0].table.as_mut().unwrap().cells[0].text.push_str("他の項目");
        let before = line(&u).clone();
        restore_large_parentheses(&mut u, &w);
        assert_eq!(line(&u), &before);
    }
    #[test]
    fn plain_dates_and_unmatched_glyphs_do_not_change_layout() {
        let (mut u, w) = fixture();
        let Unit::Plain(l) = &mut u[0] else { panic!() };
        l[0].table = None;
        l[0].text = "自2025年４月１日\n至2026年３月31日".into();
        let before = line(&u).clone();
        restore_large_parentheses(&mut u, &w);
        assert_eq!(line(&u), &before);
    }
    #[test]
    fn already_restored_parentheses_are_not_doubled() {
        let (mut u, w) = fixture();
        restore_large_parentheses(&mut u, &w);
        let before = line(&u).clone();
        restore_large_parentheses(&mut u, &w);
        assert_eq!(line(&u), &before);
    }
}

#[cfg(test)]
mod period_tests {
    use super::*;
    use crate::trace::WordStatus;
    fn fixture() -> (Vec<Line>, Vec<Word>) {
        let w = |id, x0, y0, x1, raw: &str, text: &str| Word {
            source_box: [x0, y0, x1, y0 + 8.],
            id,
            x0,
            y0,
            x1,
            y1: y0 + 8.,
            raw: raw.into(),
            text: text.into(),
            status: WordStatus::Used,
        };
        let words = vec![
            w(0, 20., 100., 36., "\u{EAED}自", "自"),
            w(1, 44., 100., 116., "令和７年５月１日\u{EAEF}", "令和７年５月１日"),
            w(2, 20., 108., 36., "\u{EAEE}至", "至"),
            w(3, 44., 108., 116., "令和８年４月30日\u{EAF0}", "令和８年４月30日"),
            w(4, 120., 108., 180., "(単位：百万円)", "(単位：百万円)"),
        ];
        (
            vec![
                Line::text("自　令和７年５月１日".into(), vec![0, 1]),
                Line::text("至　令和８年４月30日(単位：百万円)".into(), vec![2, 3, 4]),
            ],
            words,
        )
    }
    #[test]
    fn proven_period_changes_only_local_text() {
        let (mut lines, words) = fixture();
        let before = lines.clone();
        restore_plain_period_parentheses(&mut lines, &words);
        assert_eq!(lines[0].text, "（自　令和７年５月１日");
        assert_eq!(lines[1].text, "至　令和８年４月30日）(単位：百万円)");
        for (line, old) in lines.iter_mut().zip(&before) {
            line.text = old.text.clone();
        }
        assert_eq!(lines, before);
    }
    #[test]
    fn all_four_corners_are_required_and_geometrically_aligned() {
        for case in 0..9 {
            let (mut lines, mut words) = fixture();
            match case {
                0..=3 => words[case].raw = words[case].text.clone(),
                4 => words[2].x0 += 4.,
                5 => words[3].x1 += 4.,
                6 => words[2].y0 += 8.,
                7 => words[1].y0 += 4.,
                _ => words[0].status = WordStatus::Margin,
            };
            let before = lines.clone();
            restore_plain_period_parentheses(&mut lines, &words);
            assert_eq!(lines, before, "case {case}");
        }
    }
    #[test]
    fn adjacent_prose_signatures_and_other_dates_are_not_periods() {
        for case in 0..6 {
            let (mut lines, mut words) = fixture();
            match case {
                0 => {
                    words[0].text = "代表者".into();
                    lines[0].text = "代表者　令和７年５月１日".into();
                }
                1 => words[1].text = "令和７年13月１日".into(),
                2 => lines[0].text.push_str("別記事"),
                3 => lines[1].text.push_str("別記事"),
                4 => words[4].text = "他社代表者".into(),
                _ => lines[0].words = vec![0, 0],
            };
            let before = lines.clone();
            restore_plain_period_parentheses(&mut lines, &words);
            assert_eq!(lines, before, "case {case}");
        }
    }
    #[test]
    fn separate_units_cannot_be_combined() {
        let (lines, words) = fixture();
        let mut units = vec![Unit::Plain(vec![lines[0].clone()]), Unit::Plain(vec![lines[1].clone()])];
        restore_large_parentheses(&mut units, &words);
        for (u, expected) in units.iter().zip(&lines) {
            let Unit::Plain(l) = u else { panic!() };
            assert_eq!(l[0], *expected);
        }
    }
    #[test]
    fn shinkyu_lines_are_not_changed() {
        let (lines, words) = fixture();
        let mut units = vec![Unit::Shinkyu {labels:None,
            preamble: lines.clone(),
            after: vec![],
            before: vec![],
            outside: vec![],
        }];
        restore_large_parentheses(&mut units, &words);
        let Unit::Shinkyu { preamble, .. } = &units[0] else { panic!() };
        assert_eq!(*preamble, lines);
    }
    #[test]
    fn restoration_is_idempotent_and_optional_units_are_not_enclosed() {
        let (mut lines, words) = fixture();
        lines[1].words.pop();
        lines[1].text = "至　令和８年４月30日".into();
        restore_plain_period_parentheses(&mut lines, &words);
        assert_eq!(lines[1].text, "至　令和８年４月30日）");
        let once = lines.clone();
        restore_plain_period_parentheses(&mut lines, &words);
        assert_eq!(lines, once);
    }
}
