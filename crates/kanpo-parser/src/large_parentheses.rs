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
            Unit::Plain(p) => lines(p, words),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            } => {
                lines(preamble, words);
                for p in after.iter_mut().chain(before).chain(outside) {
                    lines(p, words);
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::{Table, TableCell, WordStatus};
    fn fixture() -> (Vec<Unit>, Vec<Word>) {
        let w = |id, x0, y0, x1, y1, raw: &str, text: &str| Word {
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
