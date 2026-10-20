//! Vertical before/after measurement tables with wrapped alphabetic section codes.
use crate::trace::{Line, Table, TableCell, Unit, Word};
use std::collections::{BTreeMap, BTreeSet};
fn cx(w: &Word) -> f32 {
    // A horizontal halfwidth dot is reboxed into the left half of a vertical em.
    // Its right edge is the center of the containing full-width glyph column.
    if w.text == "･"
        && w.raw == "･"
        && (2. * (w.x1 - w.x0) - (w.y1 - w.y0)).abs() < (w.y1 - w.y0) * 0.1
    {
        w.x1
    } else {
        (w.x0 + w.x1) / 2.
    }
}
fn cy(w: &Word) -> f32 {
    (w.y0 + w.y1) / 2.
}
fn numeric(t: &str) -> bool {
    !t.is_empty()
        && t.chars().all(|c| {
            c.is_ascii_digit()
                || ('０'..='９').contains(&c)
                || "〇零一二三四五六七八九・･．.〜～".contains(c)
        })
}
fn decimal(t: &str) -> bool {
    let p = t.split(['・', '･', '．', '.']).collect::<Vec<_>>();
    p.len() == 2
        && p.iter().all(|s| {
            !s.is_empty()
                && s.chars().all(|c| {
                    c.is_ascii_digit()
                        || ('０'..='９').contains(&c)
                        || "〇零一二三四五六七八九".contains(c)
                })
        })
}
fn range(t: &str) -> bool {
    let p = t.split(['〜', '～']).collect::<Vec<_>>();
    p.len() == 2 && p.iter().all(|s| decimal(s))
}
fn letter(c: char) -> bool {
    c.is_ascii_uppercase() || ('Ａ'..='Ｚ').contains(&c)
}
fn codes(t: &str) -> bool {
    !t.is_empty() && t.chars().all(|c| letter(c) || c == '・' || c == '･')
}
fn valid_codes(t: &str) -> bool {
    let p = t.split(['・', '･']).collect::<Vec<_>>();
    !p.is_empty()
        && p.iter()
            .all(|s| s.chars().count() == 1 && s.chars().all(letter))
}
fn text(ws: &[&Word]) -> String {
    let mut ws = ws.to_vec();
    ws.sort_by(|a, b| cx(b).total_cmp(&cx(a)).then(a.y0.total_cmp(&b.y0)));
    ws.iter().map(|w| w.text.as_str()).collect()
}
fn groups<'a>(ws: &[&'a Word], tol: f32) -> Vec<Vec<&'a Word>> {
    let mut sorted = ws.to_vec();
    sorted.sort_by(|a, b| cx(b).total_cmp(&cx(a)));
    let mut out: Vec<Vec<&Word>> = vec![];
    for w in sorted {
        if let Some(last) = out.last_mut().filter(|g| (cx(g[0]) - cx(w)).abs() <= tol) {
            last.push(w);
        } else {
            out.push(vec![w]);
        }
    }
    out
}
fn counts(s: &str) -> BTreeMap<char, usize> {
    let mut m = BTreeMap::new();
    for c in s.chars().filter(|c| !c.is_whitespace()) {
        *m.entry(c).or_insert(0) += 1;
    }
    m
}
// Every consecutive code-column group must have its union-box center at the corresponding measurement row.
fn partition_codes<'a>(
    cols: &[Vec<&'a Word>],
    anchors: &[f32],
    tol: f32,
) -> Option<Vec<Vec<&'a Word>>> {
    fn walk<'a>(
        cols: &[Vec<&'a Word>],
        anchors: &[f32],
        tol: f32,
        at: usize,
        row: usize,
        current: &mut Vec<Vec<&'a Word>>,
        solutions: &mut Vec<Vec<Vec<&'a Word>>>,
    ) {
        if solutions.len() > 1 {
            return;
        }
        if row == anchors.len() {
            if at == cols.len() {
                solutions.push(current.clone());
            }
            return;
        }
        for end in at + 1..=cols.len() {
            let ws = cols[at..end].iter().flatten().copied().collect::<Vec<_>>();
            let lo = ws.iter().map(|w| w.x0).fold(f32::INFINITY, f32::min);
            let hi = ws.iter().map(|w| w.x1).fold(f32::NEG_INFINITY, f32::max);
            if ((lo + hi) / 2. - anchors[row]).abs() <= tol && valid_codes(&text(&ws)) {
                current.push(ws);
                walk(cols, anchors, tol, end, row + 1, current, solutions);
                current.pop();
            }
        }
    }
    let mut out = vec![];
    walk(cols, anchors, tol, 0, 0, &mut vec![], &mut out);
    (out.len() == 1).then(|| out.remove(0))
}
fn infer(head: &Word, words: &[Word]) -> Option<Table> {
    let em = head.x1 - head.x0;
    if !(5.0..=14.0).contains(&em) {
        return None;
    }

    let tail = words.iter().find(|w| {
        w.text == "後別"
            && (w.y0 - head.y0).abs() < em * 0.2
            && (w.x0 - (head.x0 - em)).abs() < em * 0.2
    })?;
    let hx = (cx(head) + cx(tail)) / 2.;
    let mut hs = words
        .iter()
        .filter(|w| {
            (cx(w) - hx).abs() < em * 0.15
                && w.y0 >= head.y0 - em * 32.
                && w.y1 <= head.y0 + em * 30.
        })
        .collect::<Vec<_>>();
    hs.sort_by(|a, b| a.y0.total_cmp(&b.y0));
    let header = hs.iter().map(|w| w.text.as_str()).collect::<String>();
    let has_remarks = header == "区間敷地の幅員延長備考" && hs.len() == 11;
    if !has_remarks && !(header == "区間敷地の幅員延長" && hs.len() == 9) {
        return None;
    }
    let units = |s: &str| {
        words.iter().find(|w| {
            w.text == s
                && cx(w) < hx - em
                && cx(w) > hx - em * 3.
                && w.y0 >= hs[2].y0
                && w.y1 <= hs[8].y1 + em * 2.
        })
    };
    let meter = units("メートル")?;
    let km = units("キロメートル")?;
    if (cx(meter) - cx(km)).abs() > 1. {
        return None;
    }
    let b3 = (head.y1 + hs[2].y0) / 2.;
    let b4 = (hs[6].y1 + hs[7].y0) / 2.;
    let b5 = if has_remarks {
        (hs[8].y1 + hs[9].y0) / 2.
    } else {
        hs[8].y1 + em * 2.
    };
    // Keep original source words immutable. A proved letter/digit boundary may cross a native word.
    // Local suffix fragments have virtual IDs, which never leave this inference routine.
    let source_count = words.len();
    let mut fragments = Vec::new();
    for w in words {
        let mut chars = w.text.chars();
        let first = chars.next();
        let rest = chars.collect::<String>();
        let n = rest.chars().count();
        let h = w.y1 - w.y0;
        let crosses = w.x0 < hx && w.x0 > hx - em * 18. && w.y0 < b3 && w.y1 > b3;
        let digits = !rest.is_empty()
            && rest.chars().all(|c| {
                c.is_ascii_digit()
                    || ('０'..='９').contains(&c)
                    || "〇零一二三四五六七八九".contains(c)
            });
        if crosses
            && first.is_some_and(letter)
            && digits
            && n <= 4
            && w.raw == w.text
            && h >= em * (n + 1) as f32
            && h <= em * ((n + 1) as f32 + 0.5)
        {
            let mut code = w.clone();
            code.text = first.unwrap().to_string();
            code.raw = code.text.clone();
            code.y1 = code.y0 + em;
            let mut amount = w.clone();
            amount.id = source_count + w.id;
            amount.text = rest;
            amount.raw = amount.text.clone();
            amount.y0 = amount.y1 - em * n as f32;
            fragments.push(code);
            fragments.push(amount);
        } else {
            fragments.push(w.clone());
        }
    }
    let words = fragments.as_slice();
    let candidates = words
        .iter()
        .filter(|w| {
            cx(w) < cx(meter) - em * 0.5
                && cx(w) > cx(meter) - em * 16.
                && cy(w) > b3
                && cy(w) < b4
                && numeric(&w.text)
        })
        .collect::<Vec<_>>();
    let mut rows = groups(&candidates, em * 0.15)
        .into_iter()
        .filter(|g| range(&text(g)))
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| cx(b[0]).total_cmp(&cx(a[0])));
    if rows.len() < 2 || rows.len() > 20 || rows.len() % 2 != 0 {
        return None;
    }
    let anchors = rows.iter().map(|g| cx(g[0])).collect::<Vec<_>>();
    if anchors.windows(2).any(|p| p[0] - p[1] < em * 0.9) {
        return None;
    }

    let xlo = anchors.last().copied()? - em * 4.;
    let xhi = anchors[0] + em * 0.6;
    let lengths = anchors
        .iter()
        .map(|&x| {
            words
                .iter()
                .filter(|w| {
                    (cx(w) - x).abs() < em * 0.15 && cy(w) > b4 && cy(w) < b5 && numeric(&w.text)
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    if lengths.iter().any(|g| !decimal(&text(g))) {
        return None;
    }
    let code_words = words
        .iter()
        .filter(|w| cx(w) >= xlo && cx(w) <= xhi && w.y0 >= head.y0 && cy(w) < b3 && codes(&w.text))
        .collect::<Vec<_>>();

    let has_codes = !code_words.is_empty();
    if has_codes && anchors.len() < 4 {
        return None;
    }
    let code_rows = if has_codes {
        partition_codes(&groups(&code_words, em * 0.15), &anchors, em * 0.2)?
    } else {
        vec![]
    };

    let interval_candidates = words
        .iter()
        .filter(|w| {
            !w.text.is_empty()
                && cx(w) >= xlo
                && cx(w) <= xhi
                && w.y0 >= hs[0].y0 - em * 4.
                && w.y1 <= head.y0
        })
        .collect::<Vec<_>>();
    let ig = groups(&interval_candidates, em * 0.15);
    if ig
        .first()
        .is_none_or(|g| (cx(g[0]) - anchors[0]).abs() > em * 0.3)
    {
        return None;
    }
    // A shared interval wraps at the glyph pitch. The wider normal prose-column gap ends the cell.
    let mut intervals = vec![];
    let mut previous = anchors[0];
    for g in ig {
        if previous - cx(g[0]) > em * 1.2 {
            break;
        }
        previous = cx(g[0]);
        intervals.extend(g);
    }
    if intervals.is_empty() {
        return None;
    }
    let starts = groups(&intervals, em * 0.15)
        .iter()
        .map(|g| g.iter().map(|w| w.y0).fold(f32::INFINITY, f32::min))
        .collect::<Vec<_>>();
    if starts.iter().copied().fold(f32::NEG_INFINITY, f32::max)
        - starts.iter().copied().fold(f32::INFINITY, f32::min)
        > em
    {
        return None;
    }
    let x0 = code_words
        .iter()
        .chain(&intervals)
        .map(|w| w.x0)
        .fold(f32::INFINITY, f32::min)
        .min(
            rows.last()?
                .iter()
                .map(|w| w.x0)
                .fold(f32::INFINITY, f32::min),
        );
    let x1 = head.x1;
    let y0 = intervals.iter().map(|w| w.y0).fold(hs[0].y0, f32::min);
    let y1 = hs.last()?.y1 + em;
    let inside = words
        .iter()
        .filter(|w| {
            !w.text.is_empty()
                && w.x0 >= x0 - em * 0.1
                && w.x1 <= x1 + em * 0.1
                && w.y0 >= y0 - em * 0.1
                && w.y1 <= y1 + em * 0.1
        })
        .collect::<Vec<_>>();
    let states = inside
        .iter()
        .copied()
        .filter(|w| {
            (w.text == "前" || w.text == "後")
                && w.y0 >= head.y0 - em * 0.1
                && cy(w) < b3
                && cx(w) < cx(meter)
        })
        .collect::<Vec<_>>();
    if states.len() != 2 {
        return None;
    }
    let mut states = states;
    states.sort_by(|a, b| cx(b).total_cmp(&cx(a)));
    if states[0].text != "前" || states[1].text != "後" {
        return None;
    }
    let mid = (cx(states[0]) + cx(states[1])) / 2.;
    let first = anchors.iter().take_while(|&&x| x > mid).count();
    if first < (if has_codes { 2 } else { 1 })
        || anchors.len() - first < (if has_codes { 2 } else { 1 })
    {
        return None;
    }

    let width_col = if has_codes { 3 } else { 2 };
    let length_col = width_col + 1;
    let remarks_col = length_col + 1;
    let mut table = Table {
        x0,
        y0,
        x1,
        y1,
        rows: anchors.len() + 1,
        cols: remarks_col + usize::from(has_remarks),
        cells: vec![],
    };
    let mut assigned = BTreeSet::new();
    let mut add = |r, c, span, colspan, ws: Vec<&Word>| {
        if ws.is_empty() {
            return;
        }
        for w in &ws {
            assigned.insert(w.id);
        }
        let bounds = ws.iter().fold(
            (
                f32::INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
            ),
            |(a, b, c, d), w| (a.min(w.x0), b.min(w.y0), c.max(w.x1), d.max(w.y1)),
        );
        table.cells.push(TableCell {
            row: r,
            col: c,
            rowspan: span,
            colspan,
            x0: bounds.0,
            y0: bounds.1,
            x1: bounds.2,
            y1: bounds.3,
            text: text(&ws),
            words: ws
                .iter()
                .filter(|w| w.id < source_count)
                .map(|w| w.id)
                .collect(),
        });
    };
    add(0, 0, 1, 1, hs[..2].to_vec());
    add(0, 1, 1, if has_codes { 2 } else { 1 }, vec![head, tail]);
    let mut hw = hs[2..7].to_vec();
    hw.push(meter);
    add(0, width_col, 1, 1, hw);
    let mut hl = hs[7..9].to_vec();
    hl.push(km);
    add(0, length_col, 1, 1, hl);
    if has_remarks {
        add(0, remarks_col, 1, 1, hs[9..].to_vec());
    }
    add(1, 1, first, 1, vec![states[0]]);
    add(first + 1, 1, anchors.len() - first, 1, vec![states[1]]);
    for r in 0..anchors.len() {
        if has_codes {
            add(r + 1, 2, 1, 1, code_rows[r].clone());
        }
        add(r + 1, width_col, 1, 1, rows[r].clone());
        add(r + 1, length_col, 1, 1, lengths[r].clone());
    }
    drop(add);
    let remaining = inside
        .iter()
        .copied()
        .filter(|w| !assigned.contains(&w.id))
        .collect::<Vec<_>>();
    let interval = remaining
        .iter()
        .copied()
        .filter(|w| cy(w) < head.y0)
        .collect::<Vec<_>>();
    let remarks = remaining
        .iter()
        .copied()
        .filter(|w| cy(w) > b5)
        .collect::<Vec<_>>();

    for (col, ws) in [(0, interval), (remarks_col, remarks)] {
        if col == remarks_col && !has_remarks {
            if !ws.is_empty() {
                return None;
            }
            continue;
        }
        if ws.is_empty() {
            return None;
        }
        let bounds = ws.iter().fold(
            (
                f32::INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
            ),
            |(a, b, c, d), w| (a.min(w.x0), b.min(w.y0), c.max(w.x1), d.max(w.y1)),
        );
        for w in &ws {
            assigned.insert(w.id);
        }
        table.cells.push(TableCell {
            row: 1,
            col,
            rowspan: anchors.len(),
            colspan: 1,
            x0: bounds.0,
            y0: bounds.1,
            x1: bounds.2,
            y1: bounds.3,
            text: text(&ws),
            words: ws
                .iter()
                .filter(|w| w.id < source_count)
                .map(|w| w.id)
                .collect(),
        });
    }

    if inside.iter().any(|w| !assigned.contains(&w.id))
        || assigned.iter().filter(|&&id| id < source_count).count()
            != table.cells.iter().map(|c| c.words.len()).sum::<usize>()
    {
        return None;
    }
    Some(table)
}
pub(crate) fn restore(units: &mut [Unit], words: &[Word]) {
    for head in words.iter().filter(|w| w.text == "変更前") {
        let Some(table) = infer(head, words) else {
            continue;
        };
        let ids = table
            .cells
            .iter()
            .flat_map(|c| &c.words)
            .copied()
            .collect::<BTreeSet<_>>();
        for unit in &mut *units {
            let Unit::Plain(lines) = unit else {
                continue;
            };
            let touched = lines
                .iter()
                .enumerate()
                .filter(|(_, l)| l.words.iter().any(|id| ids.contains(id)))
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            let (Some(&first), Some(&last)) = (touched.first(), touched.last()) else {
                continue;
            };
            let old = &lines[first..=last];
            if old
                .iter()
                .any(|l| l.table.is_some() || l.words.iter().any(|id| !ids.contains(id)))
            {
                continue;
            }
            if counts(&old.iter().map(|l| l.text.as_str()).collect::<String>())
                != counts(&table.render())
            {
                continue;
            }
            let oldids = old
                .iter()
                .flat_map(|l| &l.words)
                .copied()
                .collect::<BTreeSet<_>>();
            if !oldids.is_subset(&ids) {
                continue;
            }
            if lines[..first]
                .iter()
                .chain(&lines[last + 1..])
                .any(|l| l.words.iter().any(|id| ids.contains(id)))
            {
                continue;
            }
            let line = Line {
                text: table.render(),
                words: vec![],
                table: Some(table.clone()),
            };
            lines.splice(first..=last, [line]);
            break;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn source() -> Vec<Word> {
        let pdf = include_bytes!("../tests/archive/pages/20260831h01779/20260831h017790008.pdf");
        crate::analyze_item(&crate::page_inputs(pdf).unwrap())
            .pages
            .remove(0)
            .words
    }
    #[test]
    fn table_requires_both_measurement_units_and_matching_value_rows() {
        let ws = source();
        let infer_first = |w: &[Word]| infer(&w[33], w);
        assert!(infer_first(&ws).is_some());
        let mut w = ws.clone();
        w[49].text = "センチメートル".into();
        assert!(infer_first(&w).is_none());
        let mut w = ws.clone();
        w[50].x0 -= 12.;
        w[50].x1 -= 12.;
        assert!(infer_first(&w).is_none());
        let mut w = ws.clone();
        for i in [83, 84, 85] {
            w[i].x0 -= 3.;
            w[i].x1 -= 3.;
        }
        assert!(infer_first(&w).is_none());
        let mut w = ws.clone();
        w[76].text = "当初".into();
        assert!(infer_first(&w).is_none());
        let mut w = ws.clone();
        for i in [66, 67, 68] {
            w[i].x0 += 5.;
            w[i].x1 += 5.;
        }
        assert!(infer_first(&w).is_none());
        let mut w = ws.clone();
        w[47].y0 += 4.;
        w[47].y1 += 4.;
        assert!(infer_first(&w).is_none());
    }
    #[test]
    fn wrapped_code_partition_does_not_assign_b_to_the_a_measurement() {
        let ws = source();
        let t = infer(&ws[33], &ws).unwrap();
        let cell = |r, c| {
            t.cells
                .iter()
                .find(|c2| c2.row == r && c2.col == c)
                .unwrap()
        };
        assert_eq!(cell(1, 2).text, "Ａ");
        assert_eq!(cell(2, 2).text, "Ｂ・Ｃ・Ｄ・Ｅ・Ｆ・Ｇ");
        assert_eq!(cell(2, 4).text, "八・五四八");
        let mut w = ws.clone();
        for i in [66, 67, 68, 73, 74, 75, 88, 89, 90, 93, 94] {
            w[i].x0 += 2.;
            w[i].x1 += 2.;
        }
        assert!(infer(&w[33], &w).is_none());
    }
    #[test]
    fn a_word_crossing_code_and_width_keeps_one_original_id_and_no_virtual_ids() {
        let ws = source();
        let t = infer(&ws[190], &ws).unwrap();
        assert_eq!(ws[226].text, "Ｃ一二");
        assert_eq!(
            t.cells
                .iter()
                .flat_map(|c| &c.words)
                .filter(|&&id| id == 226)
                .count(),
            1
        );
        assert!(t
            .cells
            .iter()
            .flat_map(|c| &c.words)
            .all(|&id| id < ws.len()));
        assert_eq!(
            t.cells
                .iter()
                .find(|c| c.row == 2 && c.col == 2)
                .unwrap()
                .text,
            "Ｂ・Ｃ"
        );
        assert_eq!(
            t.cells
                .iter()
                .find(|c| c.row == 2 && c.col == 3)
                .unwrap()
                .text,
            "一二・六四〜二四六・四三"
        );
        let mut w = ws.clone();
        w[226].raw = "unproven glyphs".into();
        assert!(infer(&w[190], &w).is_none());
    }
    #[test]
    fn a_following_section_and_notice_are_not_part_of_the_common_interval() {
        let pdf = include_bytes!("../tests/archive/pages/20261014h01807/20261014h018070006.pdf");
        let a = crate::analyze_item(&crate::page_inputs(pdf).unwrap());
        let ws = &a.pages[0].words;
        let t = infer(&ws[196], ws).unwrap();
        let common = &t
            .cells
            .iter()
            .find(|c| c.row == 1 && c.col == 0)
            .unwrap()
            .text;
        assert!(common.ends_with("まで"));
        assert!(!common.contains("（四）"));
        assert!(!common.contains("九州"));
        assert_eq!(t.cols, 4);
    }
}

#[cfg(test)]
mod halfwidth_tests {
    use super::*;
    fn source() -> Vec<Word> {
        let pdf = include_bytes!("../tests/archive/pages/20260731h01759/20260731h017590006.pdf");
        crate::analyze_item(&crate::page_inputs(pdf).unwrap())
            .pages
            .remove(0)
            .words
    }
    #[test]
    fn a_halfwidth_dot_requires_its_native_shape_and_raw_character() {
        let w = source();
        assert!((cx(&w[56]) - cx(&w[50])).abs() < 0.01);
        let mut dot = w[56].clone();
        dot.raw = "unknown".into();
        assert!((cx(&dot) - cx(&w[50])).abs() > 1.9);
        dot.raw = "･".into();
        dot.x1 = dot.x0 + 8.;
        assert!((cx(&dot) - ((dot.x0 + dot.x1) / 2.)).abs() < 0.01);
    }
    #[test]
    fn missing_decimal_or_a_state_above_the_header_does_not_make_a_table() {
        let w = source();
        assert!(infer(&w[4], &w).is_some());
        let mut changed = w.clone();
        changed[56].text.clear();
        assert!(infer(&changed[4], &changed).is_none());
        let mut changed = w.clone();
        changed[60].y0 -= 2.;
        changed[60].y1 -= 2.;
        assert!(infer(&changed[4], &changed).is_none());
    }
    #[test]
    fn halfwidth_decimal_and_code_separators_do_not_accept_missing_digits_or_punctuation() {
        assert!(range("九･七七〜四二･三六"));
        assert!(valid_codes("Ｂ･Ｃ･Ｄ"));
        for bad in [
            "九七七〜四二三六",
            "九･･七七〜四二･三六",
            "･七七〜四二･三六",
            "九･〜四二･三六",
        ] {
            assert!(!range(bad), "{bad}");
        }
        for bad in ["Ｂ･Ｃ･", "･Ｂ", "Ｂ･･Ｃ", "ＢＣ"] {
            assert!(!valid_codes(bad), "{bad}");
        }
    }
}
