//! 横組みの算式で、上下に積まれた分子・分母を `(分子/分母)` に戻す。
//!
//! 数字だけの行／乗算を含む行／数字だけの行の 3 行に限り、短い分数線と語の位置を
//! 確かめる。通常の表や小さな上付き文字は扱わない。同じ算式に確かな分数線があれば、
//! 抽出器の最短線長に満たない隣の分数も、同じ上下位置・左右中央揃えから補える。

use std::collections::{HashMap, HashSet};

use crate::trace::{Line, Unit, Word};

pub(crate) fn restore_fraction_lines(
    units: &mut [Unit],
    words: &[Word],
    hrules: &[(f32, f32, f32)],
) {
    if hrules.is_empty() {
        return;
    }
    let by_id: HashMap<usize, &Word> = words.iter().map(|w| (w.id, w)).collect();
    for unit in units {
        let Unit::Plain(lines) = unit else { continue };
        restore_cell_fractions(lines, &by_id, hrules);
        let mut i = 0;
        while i + 2 < lines.len() {
            if let Some(line) = fraction_line(&lines[i..i + 3], &by_id, hrules) {
                lines.splice(i..i + 3, [line]);
            }
            i += 1;
        }
    }
}

/// セル全体の文章は組み直さない。語の位置から確かめられた算式の3行だけを、既存の本文中で
/// 一意に対応する範囲へ戻す。セルが既に持つ語IDと、算式の前後の文章・空白はそのまま保持する。
fn restore_cell_fractions(lines: &mut [Line], by_id: &HashMap<usize, &Word>, hrules: &[(f32, f32, f32)]) {
    for line in lines {
        let Some(table) = &mut line.table else { continue };
        let mut changed = false;
        for cell in &mut table.cells {
            if !cell.text.contains('×') {
                continue;
            }
            let Some(rows) = cell_word_lines(&cell.words, by_id) else { continue };
            for group in rows.windows(3) {
                if let Some(formula) = fraction_line(group, by_id, hrules) {
                    let original: String = group.iter().flat_map(|l| l.text.chars()).filter(|c| !c.is_whitespace()).collect();
                    changed |= replace_unique_compact(&mut cell.text, &original, &formula.text);
                }
            }
        }
        if changed {
            line.text = table.render();
        }
    }
}

fn cell_word_lines(ids: &[usize], by_id: &HashMap<usize, &Word>) -> Option<Vec<Line>> {
    let mut words: Vec<&Word> = ids.iter().map(|id| by_id.get(id).copied()).collect::<Option<_>>()?;
    words.retain(|w| !w.text.trim().is_empty());
    words.sort_by(|a, b| ((a.y0 + a.y1) / 2.0).total_cmp(&((b.y0 + b.y1) / 2.0)).then(a.x0.total_cmp(&b.x0)));
    let mut rows: Vec<(f32, f32, Vec<&Word>)> = Vec::new();
    for word in words {
        let cy = (word.y0 + word.y1) / 2.0;
        let em = word.y1 - word.y0;
        match rows.last_mut() {
            Some((y, height, row)) if (cy - *y).abs() <= (em.min(*height) * 0.2).max(0.5) => {
                *y = (*y * row.len() as f32 + cy) / (row.len() + 1) as f32;
                *height = height.min(em);
                row.push(word);
            }
            _ => rows.push((cy, em, vec![word])),
        }
    }
    Some(rows.into_iter().map(|(_, _, mut row)| {
        row.sort_by(|a, b| a.x0.total_cmp(&b.x0).then(a.id.cmp(&b.id)));
        Line::text(row.iter().map(|w| w.text.as_str()).collect(), row.iter().map(|w| w.id).collect())
    }).collect())
}

/// 空白を除いた文字列が一度だけ一致する場合に限り、その元の範囲を置き換える。
fn replace_unique_compact(text: &mut String, original: &str, replacement: &str) -> bool {
    if original.is_empty() {
        return false;
    }
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let Some(start) = compact.find(original) else { return false };
    if compact.rfind(original) != Some(start) {
        return false;
    }
    let end = start + original.len();
    let (mut offset, mut from, mut to) = (0, None, None);
    for (at, ch) in text.char_indices().filter(|(_, c)| !c.is_whitespace()) {
        if offset == start {
            from = Some(at);
        }
        offset += ch.len_utf8();
        if offset == end {
            to = Some(at + ch.len_utf8());
            break;
        }
    }
    match (from, to) {
        (Some(from), Some(to)) => {
            text.replace_range(from..to, replacement);
            true
        }
        _ => false,
    }
}

struct Number<'a> {
    words: Vec<&'a Word>,
    x0: f32,
    x1: f32,
    text: String,
}

fn numeric(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .any(|c| c.is_ascii_digit() || ('０'..='９').contains(&c))
        && text
            .chars()
            .all(|c| c.is_ascii_digit() || ('０'..='９').contains(&c) || ".．".contains(c))
}

fn number_groups<'a>(words: &[&'a Word], em: f32) -> Option<Vec<Number<'a>>> {
    let mut sorted = words.to_vec();
    sorted.sort_by(|a, b| a.x0.total_cmp(&b.x0).then(a.id.cmp(&b.id)));
    let mut groups: Vec<Number<'a>> = Vec::new();
    for w in sorted {
        let text = w.text.trim();
        if text.is_empty()
            || !text
                .chars()
                .all(|c| c.is_ascii_digit() || ('０'..='９').contains(&c) || ".．".contains(c))
        {
            return None;
        }
        match groups.last_mut() {
            Some(last) if w.x0 - last.x1 <= em * 0.3 => {
                last.x1 = last.x1.max(w.x1);
                last.text.push_str(text);
                last.words.push(w);
            }
            _ => groups.push(Number {
                words: vec![w],
                x0: w.x0,
                x1: w.x1,
                text: text.to_string(),
            }),
        }
    }
    groups
        .iter()
        .all(|g| numeric(&g.text) && g.text.chars().count() <= 16 && g.x1 - g.x0 <= em * 6.0)
        .then_some(groups)
}

fn fraction_line(
    lines: &[Line],
    by_id: &HashMap<usize, &Word>,
    hrules: &[(f32, f32, f32)],
) -> Option<Line> {
    if lines.len() != 3
        || lines
            .iter()
            .any(|l| l.table.is_some() || l.words.is_empty())
        || !lines[1].text.contains('×')
    {
        return None;
    }
    let mut seen = HashSet::new();
    let rows: Vec<Vec<&Word>> = lines
        .iter()
        .map(|l| {
            l.words
                .iter()
                .map(|id| seen.insert(*id).then(|| by_id.get(id).copied()).flatten())
                .collect::<Option<Vec<_>>>()
        })
        .collect::<Option<_>>()?;
    let (upper, body, lower) = (&rows[0], &rows[1], &rows[2]);
    let mut heights: Vec<f32> = body.iter().map(|w| w.y1 - w.y0).collect();
    heights.sort_by(f32::total_cmp);
    let em = heights[heights.len() / 2];
    if !(3.0..=20.0).contains(&em)
        || rows
            .iter()
            .flatten()
            .any(|w| !(em * 0.8..=em * 1.2).contains(&(w.y1 - w.y0)))
    {
        return None;
    }
    let center =
        |row: &[&Word]| row.iter().map(|w| (w.y0 + w.y1) * 0.5).sum::<f32>() / row.len() as f32;
    let (uy, by, ly) = (center(upper), center(body), center(lower));
    // 分子と分母は本体からそれぞれ半行ほど離れ、同じ高さの字で中央を挟む。
    // 小字の上付き・下付きや、行間の空いた通常の数字の行は対象外。
    if !(em * 0.35..=em * 0.9).contains(&(by - uy))
        || !(em * 0.35..=em * 0.9).contains(&(ly - by))
        || ((by - uy) - (ly - by)).abs() > em * 0.25
        || rows.iter().any(|r| {
            r.iter()
                .any(|w| ((w.y0 + w.y1) * 0.5 - center(r)).abs() > em * 0.2)
        })
    {
        return None;
    }
    let nums = number_groups(upper, em)?;
    let dens = number_groups(lower, em)?;
    if nums.is_empty() || nums.len() != dens.len() || nums.len() > 8 {
        return None;
    }
    let mut fractions = Vec::new();
    let mut ruled = false;
    for (num, den) in nums.iter().zip(&dens) {
        let (left, right) = (num.x0.min(den.x0), num.x1.max(den.x1));
        if ((num.x0 + num.x1) - (den.x0 + den.x1)).abs() > em * 0.5
            || body.iter().any(|w| w.x1.min(right) - w.x0.max(left) > em * 0.2)
            // 各分数の直前に乗算記号がある。普通の表の上下に並ぶ値や注記を拾わない。
            || !body.iter().any(|w| w.text.trim_end().ends_with('×') && w.x1 <= left + em * 0.2 && left - w.x1 <= em * 1.5)
        {
            return None;
        }
        let num_bottom = num
            .words
            .iter()
            .map(|w| w.y1)
            .fold(f32::NEG_INFINITY, f32::max);
        let den_top = den.words.iter().map(|w| w.y0).fold(f32::INFINITY, f32::min);
        let has_rule = hrules.iter().any(|&(y, x0, x1)| {
            y >= num_bottom - 0.5
                && y <= den_top + 0.5
                && (y - by).abs() <= em * 0.3
                && x0 <= left + 0.5
                && x1 >= right - 0.5
                && x1 - x0 <= right - left + em * 1.5
        });
        ruled |= has_rule;
        let ids = num
            .words
            .iter()
            .chain(&den.words)
            .map(|w| w.id)
            .collect::<Vec<_>>();
        fractions.push((left, right, format!("({}/{})", num.text, den.text), ids));
    }
    // 罫線無しの推定だけでは変更しない。兄弟分数の欠けた短線だけを補う。
    if !ruled || fractions.windows(2).any(|p| p[0].1 >= p[1].0) {
        return None;
    }
    let mut parts: Vec<(f32, String, Vec<usize>)> = body
        .iter()
        .map(|w| (w.x0, w.text.trim().to_string(), vec![w.id]))
        .collect();
    parts.extend(
        fractions
            .into_iter()
            .map(|(x0, _, text, ids)| (x0, text, ids)),
    );
    parts.sort_by(|a, b| a.0.total_cmp(&b.0));
    let text = parts.iter().map(|p| p.1.as_str()).collect();
    let ids: Vec<usize> = parts.into_iter().flat_map(|p| p.2).collect();
    // 元の3行の全word idを1回ずつ保持する。
    (ids.len() == seen.len() && ids.iter().copied().collect::<HashSet<_>>() == seen)
        .then(|| Line::text(text, ids))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::WordStatus;

    fn word(id: usize, x0: f32, x1: f32, y: f32, text: &str) -> Word {
        Word {
            id,
            x0,
            x1,
            y0: y,
            y1: y + 8.184,
            raw: text.into(),
            text: text.into(),
            status: WordStatus::Used,
        }
    }

    fn sample() -> (Vec<Unit>, Vec<Word>) {
        let words = vec![
            word(0, 592.0, 596.0, 160.18, "2"),
            word(1, 593.0, 601.0, 160.18, "."),
            word(2, 598.0, 602.0, 160.18, "2"),
            word(3, 617.0, 625.0, 160.18, "60"),
            word(4, 525.0, 589.0, 165.68, "額面金額の総額×"),
            word(5, 605.0, 613.0, 165.68, "×"),
            word(6, 591.0, 603.0, 171.18, "100"),
            word(7, 615.0, 627.0, 171.18, "365"),
        ];
        let lines = vec![
            Line::text("2.2　60".into(), vec![0, 1, 2, 3]),
            Line::text("額面金額の総額×　×".into(), vec![4, 5]),
            Line::text("100　365".into(), vec![6, 7]),
        ];
        (vec![Unit::Plain(lines)], words)
    }

    fn plain(units: &[Unit]) -> &[Line] {
        let Unit::Plain(lines) = &units[0] else {
            panic!("plain")
        };
        lines
    }

    fn cell_sample(text: &str) -> (Vec<Unit>, Vec<Word>) {
        let (_, words) = sample();
        let table = crate::trace::Table {
            x0: 500.0, x1: 650.0, y0: 100.0, y1: 200.0, rows: 1, cols: 1,
            cells: vec![crate::trace::TableCell {
                row: 0, col: 0, rowspan: 1, colspan: 1,
                x0: 500.0, x1: 650.0, y0: 100.0, y1: 200.0,
                text: text.into(), words: (0..words.len()).collect(),
            }],
        };
        (vec![Unit::Plain(vec![Line { text: table.render(), words: Vec::new(), table: Some(table) }])], words)
    }

    #[test]
    fn restores_only_the_unique_formula_span_inside_a_cell() {
        let (mut units, words) = cell_sample("前の文章。\n2.2　60　額面金額の総額×　×　100　365\n後の文章。");
        restore_fraction_lines(&mut units, &words, &[(168.821, 589.0, 605.0)]);
        let line = &plain(&units)[0];
        let table = line.table.as_ref().unwrap();
        assert_eq!(table.cells[0].text, "前の文章。\n額面金額の総額×(2.2/100)×(60/365)\n後の文章。");
        assert_eq!(table.cells[0].words, (0..words.len()).collect::<Vec<_>>());
        assert_eq!(line.text, table.render());
    }

    #[test]
    fn does_not_replace_repeated_or_non_contiguous_formula_text() {
        for text in [
            "2.2 60 額面金額の総額× × 100 365。2.2 60 額面金額の総額× × 100 365",
            "2.2 60 説明 額面金額の総額× × 100 365",
        ] {
            let (mut units, words) = cell_sample(text);
            restore_fraction_lines(&mut units, &words, &[(168.821, 589.0, 605.0)]);
            assert_eq!(plain(&units)[0].table.as_ref().unwrap().cells[0].text, text);
        }
    }

    #[test]
    fn restores_fraction_products_and_keeps_every_word_once() {
        let (mut units, words) = sample();
        restore_fraction_lines(
            &mut units,
            &words,
            &[(168.821, 589.0, 605.0), (168.821, 613.0, 629.0)],
        );
        let lines = plain(&units);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "額面金額の総額×(2.2/100)×(60/365)");
        let mut ids = lines[0].words.clone();
        ids.sort_unstable();
        assert_eq!(ids, (0..words.len()).collect::<Vec<_>>());
    }

    #[test]
    fn allows_a_missing_short_bar_only_beside_a_ruled_fraction() {
        let (mut units, mut words) = sample();
        words[3].text = "１".into();
        words[7].text = "２".into();
        restore_fraction_lines(&mut units, &words, &[(168.821, 589.0, 605.0)]);
        assert_eq!(plain(&units)[0].text, "額面金額の総額×(2.2/100)×(１/２)");
        let (mut units, words) = sample();
        restore_fraction_lines(&mut units, &words, &[(100.0, 589.0, 605.0)]);
        assert_eq!(plain(&units).len(), 3);
    }

    #[test]
    fn rejects_labels_superscripts_and_plain_table_numbers() {
        let rule = [(168.821, 589.0, 605.0)];
        let (mut units, mut words) = sample();
        words[3].text = "税額".into();
        restore_fraction_lines(&mut units, &words, &rule);
        assert_eq!(plain(&units).len(), 3);
        let (mut units, mut words) = sample();
        words[0].y1 = words[0].y0 + 4.0;
        restore_fraction_lines(&mut units, &words, &rule);
        assert_eq!(plain(&units).len(), 3);
        let (mut units, mut words) = sample();
        words[4].text = "経常利益".into();
        words[5].text = "税引前".into();
        restore_fraction_lines(&mut units, &words, &rule);
        assert_eq!(plain(&units).len(), 3);
    }

    #[test]
    fn rejects_duplicate_word_ids() {
        let (mut units, words) = sample();
        let Unit::Plain(lines) = &mut units[0] else {
            unreachable!()
        };
        lines[2].words.push(0);
        restore_fraction_lines(&mut units, &words, &[(168.821, 589.0, 605.0)]);
        assert_eq!(plain(&units).len(), 3);
    }
}
