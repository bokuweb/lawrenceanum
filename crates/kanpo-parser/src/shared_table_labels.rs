//! 複数の数値行の中央に置かれた共通見出しを、原本の組み括弧で確かめて先に読む。
use crate::trace::{Table, TableCell, Word, WordStatus};

fn center(w: &Word) -> f32 {
    (w.y0 + w.y1) / 2.0
}
fn bounds(c: &TableCell, words: &[Word]) -> Option<(f32, f32, f32, f32)> {
    let ws: Option<Vec<_>> = c.words.iter().map(|&id| words.get(id)).collect();
    let ws = ws.filter(|w| !w.is_empty())?;
    Some(
        ws.iter()
            .fold((f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY), |b, w| {
                (b.0.min(w.x0), b.1.min(w.y0), b.2.max(w.x1), b.3.max(w.y1))
            }),
    )
}
fn numbered(c: &TableCell, letter: char) -> bool {
    let mut chars = c.text.trim().chars();
    if chars.next() != Some(letter) {
        return false;
    }
    let rest: String = chars.collect();
    let values: Vec<_> = rest.split_whitespace().collect();
    values.len() == 2
        && values
            .iter()
            .all(|s| s.chars().any(|c| c.is_ascii_digit()) && s.chars().all(|c| c.is_ascii_digit() || ".,-−－+△".contains(c)))
}
fn raise_braced_row_label(t: &mut Table, words: &[Word]) -> usize {
    let mut count = 0;
    // 原本で確認した組み括弧の上・中央・下。文字として補わず、対応する a/b 行を示す根拠にする。
    for top in words.iter().filter(|w| w.raw == "\u{E7A4}" && w.status == WordStatus::Empty) {
        let em = top.y1 - top.y0;
        if !(4.0..=16.0).contains(&em) || top.x0 < t.x0 || top.x1 > t.x1 || top.y0 < t.y0 || top.y1 > t.y1 {
            continue;
        }
        let same = |w: &&Word| (w.x0 - top.x0).abs() < 0.5 && ((w.y1 - w.y0) - em).abs() < em * 0.2 && w.status == WordStatus::Empty;
        let middle: Vec<_> = words
            .iter()
            .filter(same)
            .filter(|w| w.raw == "\u{EB2E}" && center(w) > center(top) && center(w) - center(top) < em)
            .collect();
        if middle.len() != 1 {
            continue;
        }
        let middle = middle[0];
        let bottom: Vec<_> = words
            .iter()
            .filter(same)
            .filter(|w| w.raw == "\u{E7A5}" && center(w) > center(middle) && center(w) - center(middle) < em)
            .collect();
        if bottom.len() != 1 {
            continue;
        }
        let bottom = bottom[0];
        if ((center(top) + center(bottom)) / 2.0 - center(middle)).abs() > em * 0.15 {
            continue;
        }
        let candidate = t.cells.iter().find_map(|c| {
            if c.row == 0 || c.row + 1 >= t.rows || c.rowspan != 1 || c.colspan != 1 || t.cells.iter().filter(|o| o.row == c.row).count() != 1 {
                return None;
            }
            let text: String = c.text.chars().filter(|c| !c.is_whitespace()).collect();
            if !(2..=16).contains(&text.chars().count()) || text.contains(['。', '、', '：', ':']) {
                return None;
            }
            let b = bounds(c, words)?;
            if ((b.1 + b.3) / 2.0 - center(middle)).abs() > em * 0.25 || b.2 > top.x0 + 0.5 || top.x0 - b.2 > em {
                return None;
            }
            let row = |r| {
                let cs: Vec<_> = t.cells.iter().filter(|o| o.row == r).collect();
                (cs.len() == 1)
                    .then(|| cs[0])
                    .filter(|o| o.col == c.col && o.rowspan == 1 && o.colspan == 1 && (o.y0 - c.y0).abs() < 0.5 && (o.y1 - c.y1).abs() < 0.5)
            };
            let (a, b) = (row(c.row - 1)?, row(c.row + 1)?);
            if !((numbered(a, 'ａ') && numbered(b, 'ｂ')) || (numbered(a, 'a') && numbered(b, 'b'))) {
                return None;
            }
            for (cell, bar) in [(a, top), (b, bottom)] {
                let bb = bounds(cell, words)?;
                if bb.0 < top.x1 - 0.5 || cell.words.iter().any(|&id| (center(&words[id]) - center(bar)).abs() > em * 0.3) {
                    return None;
                }
            }
            Some(c.row)
        });
        if let Some(row) = candidate {
            for c in &mut t.cells {
                if c.row == row {
                    c.row -= 1;
                } else if c.row == row - 1 {
                    c.row += 1;
                }
            }
            count += 1;
        }
    }
    count
}

/// 同一の物理セルから別の論理行へ割れた、左揃えの密な２行の折り返し見出しを戻す。
/// 隣の２行の数値はより広い行間で別々のセルにあり、見出しの２行目はどちらにも揃わないこと。
fn join_wrapped_physical_label(t: &mut Table, words: &[Word]) -> usize {
    let mut count = 0;
    let mut i = 0;
    while i < t.cells.len() {
        let a = &t.cells[i];
        if a.col != 0 || a.colspan != 1 || a.rowspan != 1 || a.text.chars().count() < 4 {
            i += 1;
            continue;
        }
        let same_box =
            |c: &TableCell| (c.x0 - a.x0).abs() < 0.1 && (c.x1 - a.x1).abs() < 0.1 && (c.y0 - a.y0).abs() < 0.1 && (c.y1 - a.y1).abs() < 0.1;
        let group: Vec<usize> = t
            .cells
            .iter()
            .enumerate()
            .filter(|(_, c)| c.col == a.col && c.colspan == 1 && c.rowspan == 1 && same_box(c))
            .map(|(j, _)| j)
            .collect();
        let other = (group.len() == 2).then(|| group.iter().copied().find(|&j| j != i)).flatten();
        let Some(j) = other else {
            i += 1;
            continue;
        };
        let b = &t.cells[j];
        if b.row != a.row + 1
            || b.text.chars().count() < 2
            || t.cells.iter().any(|c| c.col == 0 && (c.row == a.row || c.row == b.row) && !same_box(c))
        {
            i += 1;
            continue;
        }
        let label = |c: &TableCell| !c.text.is_empty() && !c.text.chars().any(|ch| ch.is_numeric() || "。．.,，、:：;；()（）".contains(ch));
        let (Some(aa), Some(bb)) = (bounds(a, words), bounds(b, words)) else {
            i += 1;
            continue;
        };
        let em = (aa.3 - aa.1).max(bb.3 - bb.1);
        let ac = (aa.1 + aa.3) / 2.0;
        let bc = (bb.1 + bb.3) / 2.0;
        if !label(a)
            || !label(b)
            || !(4.0..=16.0).contains(&em)
            || (aa.3 - aa.1).min(bb.3 - bb.1) < em * 0.8
            || (aa.0 - bb.0).abs() > em * 0.4
            || aa.0 < a.x0 - 0.5
            || aa.0 - a.x0 > em
            || aa.2 > a.x1 + 0.5
            || a.x1 - aa.2 > em
            || aa.1 < a.y0 - 0.5
            || bb.3 > a.y1 + 0.5
            || bc - ac < em * 0.8
            || bc - ac > em * 1.3
        {
            i += 1;
            continue;
        }
        // 物理行の異なる数値セルが２列以上で上下に対応していること。
        let anchor = |r: usize| -> Option<(f32, std::collections::BTreeSet<usize>)> {
            let cells: Vec<_> = t
                .cells
                .iter()
                .filter(|c| {
                    c.row == r
                        && c.col > 0
                        && c.rowspan == 1
                        && c.y0 >= a.y0 - 0.5
                        && c.y1 <= a.y1 + 0.5
                        && c.y1 - c.y0 < (a.y1 - a.y0) * 0.75
                        && c.text.chars().any(|ch| ch.is_numeric())
                })
                .collect();
            let cols = cells.iter().map(|c| c.col).collect::<std::collections::BTreeSet<_>>();
            if cols.len() < 2 {
                return None;
            }
            let mut ys: Vec<_> = cells
                .iter()
                .flat_map(|c| c.words.iter())
                .map(|&id| words.get(id).map(center))
                .collect::<Option<Vec<_>>>()?;
            if ys.is_empty() {
                return None;
            }
            ys.sort_by(f32::total_cmp);
            let y = ys[ys.len() / 2];
            (ys.iter().all(|v| (v - y).abs() <= em * 0.4)).then_some((y, cols))
        };
        let (Some((ay, acol)), Some((by, bcol))) = (anchor(a.row), anchor(b.row)) else {
            i += 1;
            continue;
        };
        if acol.intersection(&bcol).count() < 2 || by - ay < em * 1.5 || (ac - ay).abs() > em * 0.4 || bc - ay < em * 0.5 || by - bc < em * 0.5 {
            i += 1;
            continue;
        }
        // 同じ見出しが３行以上の記録にも掛かる場合、字の２行ではなく物理セル全体の範囲を戻す。
        let covered: std::collections::BTreeSet<usize> = t
            .cells
            .iter()
            .filter(|c| c.col > 0 && c.y0 >= a.y0 - 0.5 && c.y1 <= a.y1 + 0.5)
            .flat_map(|c| c.row..c.row + c.rowspan)
            .collect();
        let end = covered.last().copied().map_or(a.row, |r| r + 1);
        if end > t.rows
            || end < a.row + 2
            || (a.row..end).any(|r| !covered.contains(&r))
            || covered.first().copied() != Some(a.row)
            || t.cells
                .iter()
                .enumerate()
                .any(|(k, c)| k != i && k != j && c.col == 0 && c.row < end && c.row + c.rowspan > a.row)
        {
            i += 1;
            continue;
        }
        let span = end - a.row;
        let mut ids = a.words.clone();
        ids.extend(&b.words);
        let mut unique = std::collections::HashSet::new();
        if !ids.iter().all(|id| unique.insert(*id)) {
            i += 1;
            continue;
        }
        let text = format!("{}{}", a.text.trim_end(), b.text.trim_start());
        t.cells[i].text = text;
        t.cells[i].words = ids;
        t.cells[i].rowspan = span;
        t.cells.remove(j);
        count += 1;
        i = if j < i { i - 1 } else { i + 1 };
    }
    count
}

pub(crate) fn order_shared_table_labels(units: &mut [crate::trace::Unit], words: &[Word]) {
    let visit = |lines: &mut [crate::trace::Line]| {
        for line in lines {
            if let Some(table) = &mut line.table {
                if raise_braced_row_label(table, words) + join_wrapped_physical_label(table, words) > 0 {
                    line.text = table.render();
                }
            }
        }
    };
    for unit in units {
        match unit {
            crate::trace::Unit::Plain(lines) => visit(lines),
            crate::trace::Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            } => {
                visit(preamble);
                for lines in after.iter_mut().chain(before).chain(outside) {
                    visit(lines);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (Table, Vec<Word>) {
        let mut words = Vec::new();
        let mut word = |x, y, width, text: &str, empty| {
            let id = words.len();
            words.push(Word {
                id,
                x0: x,
                x1: x + width,
                y0: y,
                y1: y + 8.0,
                raw: text.into(),
                text: if empty { String::new() } else { text.into() },
                status: if empty { WordStatus::Empty } else { WordStatus::Used },
            });
            id
        };
        let a = vec![word(64.0, 8.0, 40.0, "ａ0.005100", false), word(112.0, 8.0, 40.0, "−4.1780", false)];
        let label = vec![word(16.0, 14.0, 40.0, "10種地", false)];
        let b = vec![word(64.0, 20.0, 40.0, "ｂ0.005200", false), word(112.0, 20.0, 40.0, "−4.2800", false)];
        word(56.0, 8.0, 8.0, "\u{E7A4}", true);
        word(56.0, 14.0, 8.0, "\u{EB2E}", true);
        word(56.0, 20.0, 8.0, "\u{E7A5}", true);
        let cell = |row, text: &str, words| TableCell {
            row,
            col: 0,
            rowspan: 1,
            colspan: 1,
            x0: 0.0,
            x1: 160.0,
            y0: 0.0,
            y1: 40.0,
            text: text.into(),
            words,
        };
        (
            Table {
                x0: 0.0,
                x1: 160.0,
                y0: 0.0,
                y1: 40.0,
                rows: 3,
                cols: 1,
                cells: vec![
                    cell(0, "ａ0.005100　−4.1780", a),
                    cell(1, "10種地", label),
                    cell(2, "ｂ0.005200　−4.2800", b),
                ],
            },
            words,
        )
    }

    #[test]
    fn the_complete_brace_promotes_only_the_common_label_and_is_idempotent() {
        let (mut table, words) = fixture();
        let ids: Vec<_> = table.cells.iter().flat_map(|c| c.words.iter().copied()).collect();
        assert_eq!(raise_braced_row_label(&mut table, &words), 1);
        assert_eq!(table.render(), "10種地\nａ0.005100　−4.1780\nｂ0.005200　−4.2800");
        assert_eq!(raise_braced_row_label(&mut table, &words), 0);
        assert_eq!(table.cells.iter().flat_map(|c| c.words.iter().copied()).collect::<Vec<_>>(), ids);
    }

    #[test]
    fn missing_glyphs_ambiguous_rows_unpaired_values_and_other_cells_are_not_promoted() {
        for case in 0..11 {
            let (mut table, mut words) = fixture();
            match case {
                0 => words[6].raw = "\u{EFFF}".into(),
                1 => words[6].x0 += 3.0,
                2 => {
                    words[7].y0 += 5.0;
                    words[7].y1 += 5.0;
                }
                3 => table.cells[2].text = "ｃ0.005200　−4.2800".into(),
                4 => table.cells[2].text = "ｂ0.005200　次の文".into(),
                5 => {
                    words[2].x0 += 60.0;
                    words[2].x1 += 60.0;
                }
                6 => {
                    words[2].y0 += 6.0;
                    words[2].y1 += 6.0;
                }
                7 => table.cells[2].y1 += 3.0,
                8 => {
                    let mut c = table.cells[0].clone();
                    c.col = 1;
                    table.cells.push(c);
                    table.cols = 2;
                }
                9 => words[5].status = WordStatus::Used,
                10 => {
                    let mut c = table.cells[1].clone();
                    c.col = 1;
                    table.cells.push(c);
                    table.cols = 2;
                }
                _ => unreachable!(),
            }
            let before = table.render();
            assert_eq!(raise_braced_row_label(&mut table, &words), 0, "negative {case}");
            assert_eq!(table.render(), before);
        }
    }
}

#[cfg(test)]
#[path = "shared_physical_label_tests.rs"]
mod shared_physical_label_tests;
