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
    crate::nested_fractions::restore(units, words, hrules);
    let by_id: HashMap<usize, &Word> = words.iter().map(|w| (w.id, w)).collect();
    for unit in units {
        let Unit::Plain(lines) = unit else { continue };
        restore_cell_fractions(lines, &by_id, hrules);
        let mut i = 0;
        while i + 2 < lines.len() {
            if i + 3 < lines.len() {
                if let Some(line) = wrapped_fraction_line(&lines[i..i + 4], &by_id, hrules) {
                    lines.splice(i..i + 4, [line]);
                    i += 1;
                    continue;
                }
            }
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
            for group in rows.windows(4) {
                if let Some(formula) = wrapped_fraction_line(group, by_id, hrules) {
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

/// 前行の末尾の×に続く分数だけが次行の左端へ折り返した算式。
/// 幾何検査用の仮の×を行頭に置いて既存のfraction_lineを通し、検査後に仮語を除く。
/// 仮語のIDと文字は返さず、元の4行の全IDを1回ずつ保つ。
fn wrapped_fraction_line(
    lines: &[Line], by_id: &HashMap<usize, &Word>, hrules: &[(f32,f32,f32)],
) -> Option<Line> {
    if lines.len()!=4 || lines.iter().any(|l| l.table.is_some() || l.words.is_empty())
        || !lines[0].text.trim_end().ends_with('×') { return None; }
    let rows: Vec<Vec<&Word>>=lines.iter().map(|l| l.words.iter()
        .map(|id|by_id.get(id).copied()).collect::<Option<Vec<_>>>()).collect::<Option<_>>()?;
    let mut heights: Vec<f32>=rows[2].iter().map(|w|w.y1-w.y0).collect();
    heights.sort_by(f32::total_cmp);
    let em=heights[heights.len()/2];
    if !(3.0..=20.0).contains(&em) { return None; }
    let cy=|w:&&Word|(w.y0+w.y1)/2.0;
    let center=|r:&[&Word]|r.iter().map(cy).sum::<f32>()/r.len() as f32;
    let (prev, body)=(&rows[0],&rows[2]);
    if !(em*1.0..=em*2.2).contains(&(center(body)-center(prev)))
        || prev.iter().any(|w| (cy(w)-center(prev)).abs()>em*0.2
            || !(em*0.8..=em*1.2).contains(&(w.y1-w.y0))) { return None; }
    let nums=number_groups(&rows[1],em)?;
    let dens=number_groups(&rows[3],em)?;
    if nums.is_empty() || nums.len()!=dens.len() { return None; }
    let (num,den)=(&nums[0],&dens[0]);
    let (left,right)=(num.x0.min(den.x0),num.x1.max(den.x1));
    let prev_left=prev.iter().map(|w|w.x0).fold(f32::INFINITY,f32::min);
    let prev_right=prev.iter().map(|w|w.x1).fold(f32::NEG_INFINITY,f32::max);
    // 折返し元は十分長い1行で、分数は同じ書出し位置へ戻る。
    // 次行の同じ分数の前に×が既にあれば既存の3行処理へ任せる。
    if (left-prev_left).abs()>em*1.5 || prev_right-prev_left<em*12.0
        || body.iter().any(|w|w.x0<right-em*0.2) { return None; }
    let num_bottom=num.words.iter().map(|w|w.y1).fold(f32::NEG_INFINITY,f32::max);
    let den_top=den.words.iter().map(|w|w.y0).fold(f32::INFINITY,f32::min);
    // 折返した最初の分数自身に実短線が必要。別の兄弟分数の線だけでは復元しない。
    if !hrules.iter().any(|&(y,x0,x1)|y>=num_bottom-0.5 && y<=den_top+0.5
        && (y-center(body)).abs()<=em*0.3 && x0<=left+0.5 && x1>=right-0.5
        && x1-x0<=right-left+em*1.5) { return None; }
    let virtual_id=usize::MAX;
    if by_id.contains_key(&virtual_id) { return None; }
    let virtual_word=Word {source_box: [left-em, center(body)-em/2.0, left-em*0.25, center(body)+em/2.0],id:virtual_id,x0:left-em,x1:left-em*0.25,
        y0:center(body)-em/2.0,y1:center(body)+em/2.0,
        raw:"×".into(),text:"×".into(),status:crate::trace::WordStatus::Used};
    let mut virtual_words: HashMap<usize,&Word>=by_id.iter().map(|(id,w)|(*id,*w)).collect();
    virtual_words.insert(virtual_id,&virtual_word);
    let mut group=lines[1..].to_vec();
    group[1].text.insert(0,'×');
    group[1].words.push(virtual_id);
    let formula=fraction_line(&group,&virtual_words,hrules)?;
    let text=format!("{}{}",lines[0].text,formula.text.strip_prefix('×')?);
    let ids: Vec<usize>=lines[0].words.iter().copied().chain(formula.words.into_iter()
        .filter(|id|*id!=virtual_id)).collect();
    let original: HashSet<usize>=lines.iter().flat_map(|l|l.words.iter().copied()).collect();
    (original.len()==lines.iter().map(|l|l.words.len()).sum::<usize>()
        && ids.len()==original.len() && ids.iter().copied().collect::<HashSet<_>>()==original)
        .then(||Line::text(text,ids))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::WordStatus;

    fn word(id: usize, x0: f32, x1: f32, y: f32, text: &str) -> Word {
        Word {
            source_box: [x0, y, x1, y + 8.184],
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
    fn wrapped_sample() -> (Vec<Unit>,Vec<Word>) {
        let words=vec![
            word(0,165.0,420.994,81.86,"額面金額＋経過利子に相当する金額−（初期利子に相当する金額×"),
            word(1,167.0,189.0,89.86,"79.685"),
            word(2,305.0,327.0,89.864,"79.685"),
            word(3,191.0,303.0,95.36,"＋第２期利子に相当する金額×"),
            word(4,329.0,337.0,95.36,"）"),
            word(5,172.0,184.0,100.86,"100"),
            word(6,310.0,322.0,100.864,"100"),
        ];
        let lines=vec![Line::text(words[0].text.clone(),vec![0]),
            Line::text("79.685　79.685".into(),vec![1,2]),
            Line::text("＋第２期利子に相当する金額×）".into(),vec![3,4]),
            Line::text("100　100".into(),vec![5,6])];
        (vec![Unit::Plain(lines)],words)
    }

    #[test]
    fn restores_a_fraction_wrapped_after_a_long_line_ending_in_multiplication() {
        let (mut units,words)=wrapped_sample();
        restore_fraction_lines(&mut units,&words,&[(98.5,165.0,191.0),(98.5,303.0,329.0)]);
        let l=&plain(&units)[0];
        assert_eq!(plain(&units).len(),1);
        assert_eq!(l.text,"額面金額＋経過利子に相当する金額−（初期利子に相当する金額×(79.685/100)＋第２期利子に相当する金額×(79.685/100)）");
        let mut ids=l.words.clone();ids.sort_unstable();assert_eq!(ids,(0..7).collect::<Vec<_>>());
    }

    #[test]
    fn wrapped_fraction_requires_its_own_bar_and_a_matching_long_previous_line() {
        let (mut units,words)=wrapped_sample();
        restore_fraction_lines(&mut units,&words,&[(98.5,303.0,329.0)]);
        assert_eq!(plain(&units).len(),4);
        for mode in 0..3 {
            let (mut units,mut words)=wrapped_sample();
            let Unit::Plain(lines)=&mut units[0] else {unreachable!()};
            match mode {
                0=>{words[0].text="行末の説明。".into();lines[0].text=words[0].text.clone();}
                1=>words[0].x0=200.0,
                _=>words[0].x1=190.0,
            }
            restore_fraction_lines(&mut units,&words,&[(98.5,165.0,191.0),(98.5,303.0,329.0)]);
            assert_eq!(plain(&units).len(),4);
        }
    }

    #[test]
    fn wrapped_cell_fraction_replaces_only_a_unique_contiguous_span() {
        let (units,words)=wrapped_sample();
        let original=plain(&units).iter().map(|l|l.text.as_str()).collect::<Vec<_>>().join("　");
        let expected="額面金額＋経過利子に相当する金額−（初期利子に相当する金額×(79.685/100)＋第２期利子に相当する金額×(79.685/100)）";
        for duplicate in [false,true] {
            let text=if duplicate {format!("{original}。{original}")} else {format!("前の文章。\n{original}\n後の文章。")};
            let table=crate::trace::Table {x0:150.0,x1:430.0,y0:70.0,y1:120.0,rows:1,cols:1,
                cells:vec![crate::trace::TableCell {row:0,col:0,rowspan:1,colspan:1,
                    x0:150.0,x1:430.0,y0:70.0,y1:120.0,text:text.clone(),words:(0..7).collect()}]};
            let mut units=vec![Unit::Plain(vec![Line {text:table.render(),words:Vec::new(),table:Some(table)}])];
            restore_fraction_lines(&mut units,&words,&[(98.5,165.0,191.0),(98.5,303.0,329.0)]);
            let line=&plain(&units)[0];let table=line.table.as_ref().unwrap();
            assert_eq!(table.cells[0].words,(0..7).collect::<Vec<_>>());
            assert_eq!(line.text,table.render());
            if duplicate {assert_eq!(table.cells[0].text,text);}
            else {assert_eq!(table.cells[0].text,format!("前の文章。\n{expected}\n後の文章。"));}
        }
    }
}
