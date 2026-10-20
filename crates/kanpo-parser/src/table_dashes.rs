//! 列の切り分けでは空きとして扱った金額の「－」を、確定した横組みの表のセルへ戻す。

use crate::trace::{Line, Table, TableCell, Unit, Word, WordStatus};

/// 横組みページで表を組み終わった後、本文を書き下す前に呼ぶ。
pub(crate) fn restore_table_dashes(units: &mut [Unit], words: &mut [Word]) {
    for unit in units {
        match unit {
            Unit::Plain(lines) => restore_lines(lines, words),
            Unit::Shinkyu { preamble, after, before, outside } => {
                restore_lines(preamble, words);
                for lines in after.iter_mut().chain(before).chain(outside) {
                    restore_lines(lines, words);
                }
            }
        }
    }
}

fn restore_lines(lines: &mut [Line], words: &mut [Word]) {
    for line in lines {
        if let Some(table) = &mut line.table {
            restore_table(table, words);
            line.text = table.render();
        }
    }
}

fn amount(text: &str) -> bool {
    text.chars().any(|c| c.is_ascii_digit())
        && text.chars().all(|c| c.is_ascii_digit() || c.is_whitespace() || ",.△▲－−-()（）円".contains(c))
}

fn restore_table(table: &mut Table, words: &mut [Word]) {
    if table.cols < 3 {
        return;
    }
    let mut columns = vec![None; table.cols];
    for cell in &table.cells {
        if cell.colspan == 1 {
            columns[cell.col] = Some((cell.x0, cell.x1));
        }
    }
    // 前ページから続く表には科目欄が無く、先頭の欄も金額のことがある。最初の数値行より上の
    // 見出しは除き、その下の先頭欄の値が2行以上あり、全て数値なら先頭欄も対象にする。
    // 本文に名前・科目のある先頭欄や、番号だけの見出し1行を数値の欄にはしない。
    let first_data_row = table.cells.iter().filter(|c| c.colspan == 1 && amount(&c.text)).map(|c| c.row).min();
    let numeric_first_column = first_data_row.is_some_and(|first| {
        let body: Vec<&TableCell> = table.cells.iter().filter(|c| c.row >= first && c.col == 0 && !c.text.trim().is_empty()).collect();
        body.iter().filter(|c| amount(&c.text)).count() >= 2
            && body.iter().all(|c| c.colspan == 1 && c.rowspan == 1 && (amount(&c.text) || c.text.trim() == "－"))
    });
    let first_amount_col = if numeric_first_column { 0 } else { 1 };
    // 同じ行に数値の欄が2つ以上ある行だけ。セルの中に何行もの数が入ったものは、横組みの
    // 1行と確かめられないので除く。桁の切れ目で語が割れた数は、同じ高さなら1行として扱う。
    let mut rows = Vec::new();
    for row in 0..table.rows {
        let cells: Vec<&TableCell> = table.cells.iter().filter(|c| c.row == row && c.col >= first_amount_col && c.colspan == 1 && amount(&c.text)).collect();
        let cols: std::collections::HashSet<usize> = cells.iter().map(|c| c.col).collect();
        if cols.len() < 2 {
            continue;
        }
        let row_words: Vec<&Word> = cells.iter().flat_map(|c| &c.words).filter_map(|&id| words.get(id)).collect();
        let mut centers: Vec<f32> = row_words.iter().map(|w| (w.y0 + w.y1) / 2.0).collect();
        let mut heights: Vec<f32> = row_words.iter().map(|w| w.y1 - w.y0).collect();
        if centers.is_empty() {
            continue;
        }
        centers.sort_by(f32::total_cmp);
        heights.sort_by(f32::total_cmp);
        let em = heights[heights.len() / 2].max(1.0);
        if centers[centers.len() - 1] - centers[0] <= em * 0.75 {
            rows.push((row, centers[centers.len() / 2], em));
        }
    }
    for id in 0..words.len() {
        let word = &words[id];
        // 意味が分かる既知の字だけ。傍線など他の私用グリフや、本当に空のセルは補わない。
        if word.status != WordStatus::Empty || word.raw != "\u{E61C}" {
            continue;
        }
        let (x, y) = ((word.x0 + word.x1) / 2.0, (word.y0 + word.y1) / 2.0);
        if x <= table.x0 || x >= table.x1 || y <= table.y0 || y >= table.y1 {
            continue;
        }
        let Some((col, &(x0, x1))) = columns.iter().enumerate().skip(first_amount_col).filter_map(|(col, bounds)| bounds.as_ref().map(|b| (col, b))).find(|(_, (x0, x1))| *x0 < x && x < *x1) else {
            continue;
        };
        let Some(&(row, _, _)) = rows.iter().filter(|&&(_, cy, em)| (cy - y).abs() <= (em * 0.4).max(1.0)).min_by(|a, b| (a.1 - y).abs().total_cmp(&(b.1 - y).abs())) else {
            continue;
        };
        // 既存の値や結合セルのある位置へは足さない。
        if table.cells.iter().any(|c| c.row <= row && row < c.row + c.rowspan && c.col <= col && col < c.col + c.colspan && !c.text.is_empty()) {
            continue;
        }
        if let Some(cell) = table.cells.iter_mut().find(|c| c.row == row && c.col == col && c.rowspan == 1 && c.colspan == 1) {
            cell.text = "－".to_string();
            cell.words.push(id);
        } else {
            table.cells.push(TableCell { row, col, rowspan: 1, colspan: 1, x0, x1, y0: word.y0, y1: word.y1, text: "－".to_string(), words: vec![id] });
        }
        words[id].text = "－".to_string();
        words[id].status = WordStatus::Used;
    }
}
