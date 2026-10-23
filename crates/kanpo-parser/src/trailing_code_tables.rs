//! Two fenced columns of unruled name/code records. Keep each trailing code
//! beside its own name before generic leading-key tables can consume it.
use super::*;

fn code(text: &str) -> bool {
    let chars: Vec<_> = text
        .chars()
        .map(|c| match c {
            'Ａ'..='Ｚ' => char::from_u32(c as u32 - 'Ａ' as u32 + 'A' as u32).unwrap(),
            '０'..='９' => char::from_u32(c as u32 - '０' as u32 + '0' as u32).unwrap(),
            '－' => '-',
            _ => c,
        })
        .collect();
    let letters = chars.iter().take_while(|c| c.is_ascii_uppercase()).count();
    if !(1..=3).contains(&letters) {
        return false;
    }
    let rest: String = chars[letters..].iter().collect();
    let mut parts = rest.split('-');
    let first = parts.next().unwrap_or("");
    if !(3..=6).contains(&first.len()) || !first.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    if let Some(part) = parts.next() {
        if !(1..=3).contains(&part.len()) || !part.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
    }
    parts.next().is_none()
}

fn table(words: &[Col], left: f32, right: f32) -> Option<trace::Table> {
    let mut keys: Vec<_> = words.iter().filter(|w| code(&w.text)).collect();
    if keys.len() < 6 {
        return None;
    }
    keys.sort_by(|a, b| a.y.total_cmp(&b.y));
    let em = keys.iter().map(|w| w.y1 - w.y).fold(0., f32::max);
    let key_x = keys[0].x;
    if em <= 0.
        || keys
            .iter()
            .any(|w| (w.x - key_x).abs() > em * 0.25 || w.x1 - w.x > em * 6.)
        || keys
            .windows(2)
            .any(|p| p[1].y - p[0].y < em * 0.8 || p[1].y - p[0].y > em * 4.)
    {
        return None;
    }
    let names: Vec<_> = words.iter().filter(|w| !code(&w.text)).collect();
    let name_x = names.iter().map(|w| w.x).reduce(f32::min)?;
    if key_x - name_x < em * 12.
        || names.iter().any(|w| {
            (w.x - name_x).abs() > em * 0.25
                || w.x1 > key_x - em * 0.25
                || w.text.contains('。')
                || w.text
                    .chars()
                    .filter(|c| {
                        ('\u{3040}'..='\u{30ff}').contains(c)
                            || ('\u{3400}'..='\u{9fff}').contains(c)
                    })
                    .count()
                    < 2
        })
    {
        return None;
    }
    let top = keys[0].y;
    let bottom = words.iter().map(|w| w.y1).reduce(f32::max)?;
    let mut cells = Vec::new();
    let mut consumed = std::collections::HashSet::new();
    for (row, key) in keys.iter().enumerate() {
        let next = keys.get(row + 1).map_or(bottom + em, |w| w.y);
        let ws: Vec<_> = names
            .iter()
            .filter(|w| y_center(w) >= y_center(key) - em * 0.25 && y_center(w) < next)
            .map(|w| (*w).clone())
            .collect();
        let lines = group_cell_lines(&ws);
        if lines.is_empty()
            || lines.len() > 3
            || (lines[0].center() - y_center(key)).abs() > em * 0.25
            || lines
                .windows(2)
                .any(|p| p[1].center() - p[0].center() > em * 1.5)
        {
            return None;
        }
        let y1 = keys.get(row + 1).map_or(bottom + 0.5, |w| w.y - 0.5);
        cells.push(trace::TableCell {
            row,
            col: 0,
            rowspan: 1,
            colspan: 1,
            x0: left,
            y0: key.y - 0.5,
            x1: key_x - 0.5,
            y1,
            text: lines.iter().map(|l| render_line(&l.words)).collect(),
            words: ws.iter().map(|w| w.id).collect(),
        });
        cells.push(trace::TableCell {
            row,
            col: 1,
            rowspan: 1,
            colspan: 1,
            x0: key_x - 0.5,
            y0: key.y - 0.5,
            x1: right,
            y1,
            text: key.text.clone(),
            words: vec![key.id],
        });
        for w in ws {
            if !consumed.insert(w.id) {
                return None;
            }
        }
        if !consumed.insert(key.id) {
            return None;
        }
    }
    if consumed.len() != words.len() {
        return None;
    }
    Some(trace::Table {
        x0: left,
        y0: top - 0.5,
        x1: right,
        y1: bottom + 0.5,
        rows: keys.len(),
        cols: 2,
        cells,
    })
}

pub(super) fn paired(
    words: &[Col],
    vrules: &[(f32, f32, f32)],
    hrules: &[(f32, f32, f32)],
) -> Option<Vec<trace::Table>> {
    let top = words.iter().map(|w| w.y).reduce(f32::min)?;
    let bottom = words.iter().map(|w| w.y1).reduce(f32::max)?;
    let em = words.iter().map(|w| w.y1 - w.y).reduce(f32::min)?;
    let left = words.iter().map(|w| w.x).reduce(f32::min)?;
    let right = words.iter().map(|w| w.x1).reduce(f32::max)?;
    let covers = |r: &&(f32, f32, f32)| r.1 <= top + em * 0.25 && r.2 >= bottom - em * 0.25;
    let fences: Vec<_> = vrules.iter().filter(covers).collect();
    let mid: Vec<_> = fences
        .iter()
        .filter(|r| r.0 > left && r.0 < right)
        .collect();
    if mid.len() != 1 || hrules.iter().any(|&(y, _, _)| y > top && y < bottom) {
        return None;
    }
    let mid = mid[0].0;
    if words.iter().any(|w| w.x < mid && w.x1 > mid) {
        return None;
    }
    let a = fences
        .iter()
        .filter(|r| r.0 < left)
        .map(|r| r.0)
        .reduce(f32::max)?;
    let b = fences
        .iter()
        .filter(|r| r.0 > right)
        .map(|r| r.0)
        .reduce(f32::min)?;
    let l: Vec<_> = words.iter().filter(|w| w.x1 < mid).cloned().collect();
    let r: Vec<_> = words.iter().filter(|w| w.x > mid).cloned().collect();
    if l.len() + r.len() != words.len() {
        return None;
    }
    Some(vec![table(&l, a, mid)?, table(&r, mid, b)?])
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Vec<Col>, Vec<(f32, f32, f32)>) {
        let mut words = Vec::new();
        for side in 0..2 {
            for row in 0..6 {
                let x = 10. + side as f32 * 200.;
                let y = 40. + row as f32 * 12.;
                let id = words.len();
                words.push(Col {
                    id,
                    x,
                    x1: x + 88.,
                    y,
                    y1: y + 8.,
                    text: format!("材料の試験方法その{row}"),
                });
                let id = words.len();
                words.push(Col {
                    id,
                    x: x + 150.,
                    x1: x + 178.,
                    y,
                    y1: y + 8.,
                    text: format!("Ａ{}", 1000 + row),
                });
            }
        }
        (
            words,
            vec![(0., 40., 108.), (200., 40., 108.), (400., 40., 108.)],
        )
    }
    #[test]
    fn two_fenced_lists_keep_their_own_names_codes_and_source_ids() {
        let (words, rules) = fixture();
        let ts = paired(&words, &rules, &[]).unwrap();
        assert_eq!(ts.len(), 2);
        let ids: std::collections::HashSet<_> = ts
            .iter()
            .flat_map(|t| &t.cells)
            .flat_map(|c| &c.words)
            .copied()
            .collect();
        assert_eq!(ids.len(), words.len());
        for t in ts {
            assert_eq!(t.rows, 6);
            for row in 0..6 {
                assert_eq!(t.cells[row * 2].text, format!("材料の試験方法その{row}"));
                assert_eq!(t.cells[row * 2 + 1].text, format!("Ａ{}", 1000 + row));
            }
        }
    }
    #[test]
    fn no_or_short_separator_cannot_join_independent_names_and_codes() {
        let (words, mut rules) = fixture();
        rules.remove(1);
        assert!(paired(&words, &rules, &[]).is_none());
        let (_, mut rules) = fixture();
        rules[1].2 = 80.;
        assert!(paired(&words, &rules, &[]).is_none());
    }
    #[test]
    fn separate_ruled_tables_and_body_text_are_not_claimed() {
        let (words, rules) = fixture();
        assert!(paired(&words, &rules, &[(60., 0., 400.)]).is_none());
        let (mut words, rules) = fixture();
        words[0].text = "これは別の記事の本文です。".into();
        assert!(paired(&words, &rules, &[]).is_none());
    }
    #[test]
    fn codes_aligned_to_different_rows_or_columns_are_not_claimed() {
        for case in 0..2 {
            let (mut words, rules) = fixture();
            if case == 0 {
                words[1].y += 4.;
                words[1].y1 += 4.;
            } else {
                words[1].x += 12.;
                words[1].x1 += 12.;
            }
            assert!(paired(&words, &rules, &[]).is_none());
        }
    }
    #[test]
    fn an_extra_heading_or_an_unpaired_name_is_not_silently_dropped() {
        let (mut words, rules) = fixture();
        words.push(Col {
            id: 24,
            x: 10.,
            x1: 100.,
            y: 20.,
            y1: 28.,
            text: "規格一覧の別の見出し".into(),
        });
        assert!(paired(&words, &rules, &[]).is_none());
    }
    #[test]
    fn code_syntax_does_not_admit_units_dates_or_sentences() {
        for text in [
            "10ppm",
            "2026年８月",
            "Ｚ0103、次項",
            "000123",
            "Ａ１",
            "Ａ1234567",
        ] {
            assert!(!code(text), "{text}");
        }
        for text in ["Z0103", "Ａ1234－２", "B2000", "ABC123456-123"] {
            assert!(code(text), "{text}");
        }
    }
    #[test]
    fn name_ending_at_the_middle_fence_is_not_discarded() {
        let (mut words, rules) = fixture();
        words.push(Col {
            id: 24,
            x: 190.,
            x1: 200.,
            y: 60.,
            y1: 68.,
            text: "中央罫に接する名称".into(),
        });
        assert!(paired(&words, &rules, &[]).is_none());
    }
    #[test]
    fn heading_starting_at_the_middle_fence_is_not_discarded() {
        let (mut words, rules) = fixture();
        words.push(Col {
            id: 24,
            x: 200.,
            x1: 210.,
            y: 60.,
            y1: 68.,
            text: "中央罫に接する見出し".into(),
        });
        assert!(paired(&words, &rules, &[]).is_none());
    }
}
