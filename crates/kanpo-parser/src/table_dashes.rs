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

/// 完全な物理格子と論理表が一対一で一致する場合の、数値のない行の横棒。
/// 横組み専用。既存の数値anchorによる復元の後に、元のfind_grids結果を渡す。
pub(crate) fn restore_ruled_table_dashes(
    units: &mut [Unit], words: &mut [Word], grids: &[crate::grid::Grid],
) {
    let restore = |lines: &mut [Line], words: &mut [Word]| {
        for line in lines {
            if let Some(table) = &mut line.table {
                let matches: Vec<&crate::grid::Grid> = grids.iter()
                    .filter(|g| ruled_geometry_matches(table, g, words)).collect();
                if matches.len() == 1 {
                    restore_ruled_table(table, words, matches[0]);
                    line.text = table.render();
                }
            }
        }
    };
    for unit in units.iter_mut() {
        match unit {
            Unit::Plain(lines) => restore(lines, words),
            Unit::Shinkyu { preamble, after, before, outside } => {
                restore(preamble, words);
                for lines in after.iter_mut().chain(before).chain(outside) { restore(lines, words); }
            }
        }
    }
    restore_partitioned_table_dashes(units, words, grids);
}

fn ruled_geometry_matches(table: &Table, g: &crate::grid::Grid, words: &[Word]) -> bool {
    let near = |a:f32,b:f32| (a-b).abs() <= 1.0;
    table.cols >= 3 && table.rows >= 3
        && g.xs.len() == table.cols+1 && g.ys.len() == table.rows+1
        && near(table.x0,g.left()) && near(table.x1,g.right())
        && near(table.y0,g.top()) && near(table.y1,g.bottom())
        && g.cells.len() == table.cols*table.rows
        && g.cells.iter().all(|c|c.x1-c.x0==1 && c.y1-c.y0==1)
        && table.cells.iter().all(|c| {
            if c.rowspan!=1 || c.colspan!=1 || c.row>=table.rows || c.col>=table.cols { return false; }
            let (x0,x1,y0,y1)=(g.xs[c.col],g.xs[c.col+1],g.ys[c.row],g.ys[c.row+1]);
            // 先行のanchor復元が足した横棒は語の高さをセル枠にしている。字形が同じ物理升目に
            // 入る場合だけ許し、それ以外の既存セルは物理罫線の升目との完全な対応が必要。
            if c.text=="－" && !c.words.is_empty()
                && c.words.iter().all(|id|words.get(*id).is_some_and(|w|w.raw=="\u{E61C}"
                    && w.x0>=x0 && w.x1<=x1 && w.y0>=y0 && w.y1<=y1)) { return true; }
            near(c.x0,x0) && near(c.x1,x1) && near(c.y0,y0) && near(c.y1,y1)
        })
}

fn restore_ruled_table(table: &mut Table, words: &mut [Word], g: &crate::grid::Grid) {
    let candidates: Vec<(usize,usize,usize)> = words.iter().enumerate().filter_map(|(id,w)| {
        if w.raw!="\u{E61C}" || !(w.status==WordStatus::Empty
            || (w.status==WordStatus::Used && table.cells.iter().any(|c|c.words.contains(&id)))) { return None; }
        let (width,height)=(w.x1-w.x0,w.y1-w.y0);
        if !(3.0..=20.0).contains(&height) || !(height*0.7..=height*1.3).contains(&width) { return None; }
        let cell=g.cells.get(g.cell_at((w.x0+w.x1)/2.0,(w.y0+w.y1)/2.0)?)?;
        if w.x0<g.xs[cell.x0] || w.x1>g.xs[cell.x1]
            || w.y0<g.ys[cell.y0] || w.y1>g.ys[cell.y1] { return None; }
        Some((id,cell.y0,cell.x0))
    }).collect();
    let numeric_columns: Vec<bool>=(0..table.cols).map(|col| {
        let rows: std::collections::HashSet<usize>=candidates.iter()
            .filter(|c|c.2==col).map(|c|c.1).collect();
        let Some(first)=rows.iter().min() else { return false; };
        // 実際に印字された既知の横棒が2行以上。見出しより下に科目・文章のある欄は除く。
        rows.len()>=2 && table.cells.iter().filter(|c|c.col==col && c.row>=*first)
            .all(|c|c.text.trim().is_empty() || c.text.trim()=="－" || amount(&c.text))
    }).collect();
    for &(id,row,col) in &candidates {
        if words[id].status!=WordStatus::Empty || !numeric_columns[col] { continue; }
        // 同じ升目に複数の横棒があるものは、罫線の欠落や多段値の可能性がある。
        if candidates.iter().filter(|c|c.1==row && c.2==col).count()!=1 { continue; }
        // 実際の字のない空欄を補完したり、既存の数値・文章のセルへ横棒を足したりしない。
        if table.cells.iter().any(|c|c.row==row && c.col==col
            && (!c.text.trim().is_empty() || !c.words.is_empty())) { continue; }
        if let Some(cell)=table.cells.iter_mut().find(|c|c.row==row && c.col==col) {
            cell.text="－".into();cell.words.push(id);
        } else {
            table.cells.push(TableCell { row,col,rowspan:1,colspan:1,
                x0:g.xs[col],x1:g.xs[col+1],y0:g.ys[row],y1:g.ys[row+1],
                text:"－".into(),words:vec![id] });
        }
        words[id].text="－".into();words[id].status=WordStatus::Used;
    }
}

/// 既存の表と原罫線を使い、空行除去・セル内の行分割後にも既知の横棒を戻す。
/// horizontal の restore_ruled_table_dashes の後に呼ぶ。
pub(crate) fn restore_partitioned_table_dashes(units: &mut [Unit], words: &mut [Word], grids: &[crate::grid::Grid]) {
    let restore = |lines: &mut [Line], words: &mut [Word]| {
        for line in lines {
            if let Some(table) = &mut line.table {
                let matches: Vec<_> = grids.iter().filter(|g| partitioned_geometry_matches(table, g, words)).collect();
                if matches.len() != 1 {
                    continue;
                }
                let g = matches[0];
                restore_ruled_body(table, words, g);
                restore_partitioned_columns(table, words, g);
                line.text = table.render();
            }
        }
    };
    for unit in units {
        match unit {
            Unit::Plain(lines) => restore(lines, words),
            Unit::Shinkyu { preamble, after, before, outside } => {
                restore(preamble, words);
                for lines in after.iter_mut().chain(before).chain(outside) {
                    restore(lines, words);
                }
            }
        }
    }
}

fn partitioned_geometry_matches(t: &Table, g: &crate::grid::Grid, words: &[Word]) -> bool {
    let near = |a: f32, b: f32| (a - b).abs() <= 1.;
    t.cols >= 2
        && t.rows >= 3
        && g.xs.len() == t.cols + 1
        && near(t.x0, g.left())
        && near(t.x1, g.right())
        && near(t.y0, g.top())
        && near(t.y1, g.bottom())
        && t.cells.iter().all(|c| {
            if c.rowspan != 1 || c.col + c.colspan > t.cols {
                return false;
            }
            // 先行anchorが作る字形高さのセル以外は原物理セルそのもの。
            g.cells.iter().any(|p| {
                p.x0 == c.col
                    && p.x1 == c.col + c.colspan
                    && near(c.x0, g.xs[p.x0])
                    && near(c.x1, g.xs[p.x1])
                    && ((near(c.y0, g.ys[p.y0]) && near(c.y1, g.ys[p.y1]))
                        || (c.text == "－"
                            && !c.words.is_empty()
                            && c.words.iter().all(|&id| {
                                words.get(id).is_some_and(|w| {
                                    w.raw == "\u{E61C}" && w.x0 >= g.xs[p.x0] && w.x1 <= g.xs[p.x1] && w.y0 >= g.ys[p.y0] && w.y1 <= g.ys[p.y1]
                                })
                            })))
            })
        })
}

fn restore_partitioned_columns(t: &mut Table, words: &mut [Word], g: &crate::grid::Grid) {
    let mut proposals = Vec::new();
    for (id, w) in words.iter().enumerate() {
        if w.status != WordStatus::Empty || w.raw != "\u{E61C}" {
            continue;
        }
        let (width, height) = (w.x1 - w.x0, w.y1 - w.y0);
        if !(3.0..=20.0).contains(&height) || !(height * 0.7..=height * 1.3).contains(&width) {
            continue;
        }
        let (x, y) = ((w.x0 + w.x1) / 2., (w.y0 + w.y1) / 2.);
        let Some(p) = g.cell_at(x, y).map(|i| g.cells[i]) else {
            continue;
        };
        if p.x1 - p.x0 != 1 || w.x0 < g.xs[p.x0] || w.x1 > g.xs[p.x1] || w.y0 < g.ys[p.y0] || w.y1 > g.ys[p.y1] {
            continue;
        }
        // 縦長の物理セルを実際に数値欄として使っていることを、2行以上の金額で確認する。
        let column: Vec<_> = t
            .cells
            .iter()
            .filter(|c| {
                c.col == p.x0 && c.colspan == 1 && c.words.iter().any(|&id| words.get(id).is_some_and(|v| v.y0 >= g.ys[p.y0] && v.y1 <= g.ys[p.y1]))
            })
            .collect();
        let numeric_rows: std::collections::HashSet<_> = column.iter().filter(|c| amount(&c.text)).map(|c| c.row).collect();
        if numeric_rows.len() < 2 || column.iter().any(|c| !c.text.trim().is_empty() && c.text.trim() != "－" && !amount(&c.text)) {
            continue;
        }
        // 元の論理行の印字位置でのみ対応する。隣の折返し行とも一致する場合は採用しない。
        let mut rows: Vec<_> = t
            .cells
            .iter()
            .filter(|c| c.rowspan == 1)
            .filter(|c| {
                c.words.iter().any(|&id| {
                    words
                        .get(id)
                        .is_some_and(|v| !v.text.trim().is_empty() && ((v.y0 + v.y1) / 2. - y).abs() <= (height.min(v.y1 - v.y0) * 0.4).max(1.))
                })
            })
            .map(|c| c.row)
            .collect();
        rows.sort();
        rows.dedup();
        if rows.len() != 1 {
            continue;
        }
        let row = rows[0];
        let col = p.x0;
        if t.cells.iter().any(|c| {
            c.row <= row && row < c.row + c.rowspan && c.col <= col && col < c.col + c.colspan && (!c.text.trim().is_empty() || !c.words.is_empty())
        }) {
            continue;
        }
        proposals.push((id, row, col, p));
    }
    for &(id, row, col, p) in &proposals {
        if proposals.iter().filter(|v| v.1 == row && v.2 == col).count() != 1 {
            continue;
        }
        if let Some(c) = t.cells.iter_mut().find(|c| c.row == row && c.col == col && c.rowspan == 1 && c.colspan == 1) {
            c.text = "－".into();
            c.words.push(id);
        } else {
            t.cells.push(TableCell {
                row,
                col,
                rowspan: 1,
                colspan: 1,
                x0: g.xs[p.x0],
                x1: g.xs[p.x1],
                y0: g.ys[p.y0],
                y1: g.ys[p.y1],
                text: "－".into(),
                words: vec![id],
            });
        }
        words[id].text = "－".into();
        words[id].status = WordStatus::Used;
    }
}

// 結合見出しより下だけが完全格子になり、数値のない実在行を空行除去が詰めた場合。
fn restore_ruled_body(t: &mut Table, words: &mut [Word], g: &crate::grid::Grid) {
    let Some(first) = t.cells.iter().filter(|c| amount(&c.text)).map(|c| c.row).min() else {
        return;
    };
    let body: Vec<_> = t.cells.iter().filter(|c| c.row >= first).collect();
    let physical = |c: &TableCell| -> Option<usize> {
        if c.rowspan != 1 || c.colspan != 1 {
            return None;
        }
        let w = words.get(*c.words.first()?)?;
        let p = g.cells[g.cell_at((w.x0 + w.x1) / 2., (w.y0 + w.y1) / 2.)?];
        if p.x1 - p.x0 != 1 || p.y1 - p.y0 != 1 || p.x0 != c.col {
            return None;
        }
        if c.text != "－" && ((c.y0 - g.ys[p.y0]).abs() > 1. || (c.y1 - g.ys[p.y1]).abs() > 1.) {
            return None;
        }
        Some(p.y0)
    };
    let Some(mut first_physical) = body.iter().filter_map(|c| physical(c)).min() else {
        return;
    };
    // 科目・見出しのない数値だけの継続表では、先頭の全横棒行もデータ行。
    // 既存セルが全て数値/横棒で、全物理升目が未結合の一意な格子の場合だけ先頭まで読む。
    let numeric_continuation = first == 0
        && t.cells.iter().all(|c| c.text.trim().is_empty() || c.text.trim() == "－" || amount(&c.text))
        && g.cells.len() == (g.xs.len() - 1) * (g.ys.len() - 1)
        && g.cells.iter().all(|p| p.x1 - p.x0 == 1 && p.y1 - p.y0 == 1);
    if numeric_continuation {
        first_physical = 0;
    }
    if g.cells.iter().filter(|p| p.y1 > first_physical).any(|p| p.y0 < first_physical || p.y1 - p.y0 != 1 || p.x1 - p.x0 != 1) {
        return;
    }
    if t.cells.iter().filter(|c| c.row < first).any(|c| c.y1 > g.ys[first_physical] + 1.) {
        return;
    }
    let mut mapping = Vec::new();
    for row in first..t.rows {
        let cells: Vec<_> = body.iter().filter(|c| c.row == row).collect();
        // 数値継続表の実在する完全空行には、対応させる語セルがない。
        if numeric_continuation && cells.is_empty() {
            mapping.push(None);
            continue;
        }
        let mut rows = Vec::new();
        for c in cells {
            let Some(r) = physical(c) else {
                return;
            };
            rows.push(r);
        }
        rows.sort();
        rows.dedup();
        if rows.len() != 1 {
            return;
        }
        mapping.push(Some(rows[0]));
    }
    let mapped: Vec<_> = mapping.iter().flatten().copied().collect();
    if !mapped.windows(2).all(|m| m[0] < m[1]) {
        return;
    }
    let candidates: Vec<_> = words
        .iter()
        .enumerate()
        .filter_map(|(id, w)| {
            if w.raw != "\u{E61C}"
                || !(w.status == WordStatus::Empty || (w.status == WordStatus::Used && t.cells.iter().any(|c| c.words.contains(&id))))
            {
                return None;
            }
            let (width, height) = (w.x1 - w.x0, w.y1 - w.y0);
            if !(3.0..=20.0).contains(&height) || !(height * 0.7..=height * 1.3).contains(&width) {
                return None;
            }
            let p = g.cells[g.cell_at((w.x0 + w.x1) / 2., (w.y0 + w.y1) / 2.)?];
            if p.y0 < first_physical
                || p.x1 - p.x0 != 1
                || p.y1 - p.y0 != 1
                || w.x0 < g.xs[p.x0]
                || w.x1 > g.xs[p.x1]
                || w.y0 < g.ys[p.y0]
                || w.y1 > g.ys[p.y1]
            {
                return None;
            }
            Some((id, p))
        })
        .collect();
    let eligible: Vec<_> = (0..t.cols)
        .map(|col| {
            let raw_rows: std::collections::HashSet<_> = candidates.iter().filter(|(_, p)| p.x0 == col).map(|(_, p)| p.y0).collect();
            let numbers = body.iter().filter(|c| c.col == col && amount(&c.text)).count();
            (raw_rows.len() >= 2 || numbers >= 2)
                && body.iter().filter(|c| c.col == col).all(|c| c.text.trim().is_empty() || c.text.trim() == "－" || amount(&c.text))
        })
        .collect();
    let additions: Vec<_> = candidates
        .iter()
        .filter(|&&(id, p)| {
            words[id].status == WordStatus::Empty
                && eligible[p.x0]
                && candidates.iter().filter(|(_, q)| q.x0 == p.x0 && q.y0 == p.y0).count() == 1
                && !body.iter().any(|c| c.col == p.x0 && physical(c) == Some(p.y0) && (!c.text.trim().is_empty() || !c.words.is_empty()))
        })
        .copied()
        .collect();
    if additions.is_empty() {
        return;
    }
    for c in &mut t.cells {
        if c.row >= first {
            c.row = first + mapping[c.row - first].expect("every occupied row was mapped") - first_physical;
        }
    }
    t.rows = first + g.ys.len() - 1 - first_physical;
    for (id, p) in additions {
        let (row, col) = (first + p.y0 - first_physical, p.x0);
        if let Some(c) = t.cells.iter_mut().find(|c| c.row == row && c.col == col && c.rowspan == 1 && c.colspan == 1) {
            c.text = "－".into();
            c.words.push(id);
        } else {
            t.cells.push(TableCell {
                row,
                col,
                rowspan: 1,
                colspan: 1,
                x0: g.xs[col],
                x1: g.xs[col + 1],
                y0: g.ys[p.y0],
                y1: g.ys[p.y1],
                text: "－".into(),
                words: vec![id],
            });
        }
        words[id].text = "－".into();
        words[id].status = WordStatus::Used;
    }
}
#[cfg(test)]
mod partitioned_dash_tests {
    // table_dashes.rs の末尾から #[cfg(test)] mod tests { include!(...); } で読む単独unit module。
    use super::*;
    use crate::grid::{Grid, GridCell};
    fn fixture() -> (Vec<Unit>, Vec<Word>, Vec<Grid>) {
        let mut words = Vec::new();
        let mut cell = |row: usize, col: usize, text: &str, y: f32| {
            let id = words.len();
            let x = if col == 0 { 8. } else { 68. };
            words.push(Word { id, x0: x, y0: y, x1: x + 8., y1: y + 8., raw: text.into(), text: text.into(), status: WordStatus::Used });
            TableCell {
                row,
                col,
                rowspan: 1,
                colspan: 1,
                x0: col as f32 * 40.,
                x1: (col + 1) as f32 * 40.,
                y0: 0.,
                y1: 60.,
                text: text.into(),
                words: vec![id],
            }
        };
        let cells = vec![cell(0, 0, "収入", 8.), cell(0, 1, "100", 8.), cell(1, 0, "費用", 28.), cell(2, 0, "合計", 48.), cell(2, 1, "200", 48.)];
        words.push(Word { id: 5, x0: 68., y0: 28., x1: 76., y1: 36., raw: "\u{E61C}".into(), text: String::new(), status: WordStatus::Empty });
        let table = Table { x0: 0., y0: 0., x1: 80., y1: 60., rows: 3, cols: 2, cells };
        let units = vec![Unit::Plain(vec![Line { text: table.render(), words: Vec::new(), table: Some(table) }])];
        let grids = vec![Grid {
            xs: vec![0., 40., 80.],
            ys: vec![0., 60.],
            cells: vec![GridCell { x0: 0, x1: 1, y0: 0, y1: 1 }, GridCell { x0: 1, x1: 2, y0: 0, y1: 1 }],
        }];
        (units, words, grids)
    }
    fn table(units: &mut [Unit]) -> &mut Table {
        let Unit::Plain(l) = &mut units[0] else { unreachable!() };
        l[0].table.as_mut().unwrap()
    }
    fn unchanged(mut units: Vec<Unit>, mut words: Vec<Word>, grids: Vec<Grid>) {
        let before = units[0].text();
        restore_partitioned_table_dashes(&mut units, &mut words, &grids);
        assert_eq!(units[0].text(), before);
        assert_eq!(words[5].status, WordStatus::Empty);
        assert!(words[5].text.is_empty());
    }
    #[test]
    fn a_two_column_strip_recovers_its_one_existing_bar_once() {
        let (mut u, mut w, g) = fixture();
        restore_partitioned_table_dashes(&mut u, &mut w, &g);
        assert_eq!(u[0].text(), "収入　100\n費用　－\n合計　200");
        assert_eq!(w[5].status, WordStatus::Used);
        let before = u[0].text();
        restore_partitioned_table_dashes(&mut u, &mut w, &g);
        assert_eq!(u[0].text(), before);
        assert_eq!(table(&mut u).cells.iter().flat_map(|c| &c.words).filter(|&&id| id == 5).count(), 1);
    }
    #[test]
    fn unknown_private_glyph_is_not_recovered() {
        let (u, mut w, g) = fixture();
        w[5].raw = "\u{EFFF}".into();
        unchanged(u, w, g);
    }
    #[test]
    fn glyph_must_fit_inside_the_physical_cell() {
        let (u, mut w, g) = fixture();
        w[5].x0 = 78.;
        w[5].x1 = 86.;
        unchanged(u, w, g);
    }
    #[test]
    fn absent_physical_column_rule_is_not_inferred() {
        let (u, w, mut g) = fixture();
        g[0].xs = vec![0., 80.];
        unchanged(u, w, g);
    }
    #[test]
    fn prose_in_numeric_body_prevents_recovery() {
        let (mut u, w, g) = fixture();
        table(&mut u).cells.iter_mut().find(|c| c.row == 0 && c.col == 1).unwrap().text = "注記".into();
        let t = table(&mut u).render();
        let Unit::Plain(l) = &mut u[0] else { unreachable!() };
        l[0].text = t;
        unchanged(u, w, g);
    }
    #[test]
    fn one_numeric_anchor_is_insufficient_for_a_long_cell() {
        let (mut u, w, g) = fixture();
        table(&mut u).cells.retain(|c| c.row != 2 || c.col != 1);
        let t = table(&mut u).render();
        let Unit::Plain(l) = &mut u[0] else { unreachable!() };
        l[0].text = t;
        unchanged(u, w, g);
    }
    #[test]
    fn two_logical_rows_at_same_height_are_ambiguous() {
        let (u, mut w, g) = fixture();
        w[3].y0 = 28.;
        w[3].y1 = 36.;
        unchanged(u, w, g);
    }
    #[test]
    fn multiple_bars_in_one_logical_slot_are_ambiguous() {
        let (u, mut w, g) = fixture();
        let mut duplicate = w[5].clone();
        duplicate.id = 6;
        w.push(duplicate);
        unchanged(u, w, g);
    }
    #[test]
    fn an_existing_value_is_not_overwritten() {
        let (mut u, mut w, g) = fixture();
        w.push(Word { id: 6, x0: 68., y0: 28., x1: 76., y1: 36., raw: "0".into(), text: "0".into(), status: WordStatus::Used });
        table(&mut u).cells.push(TableCell {
            row: 1,
            col: 1,
            rowspan: 1,
            colspan: 1,
            x0: 40.,
            x1: 80.,
            y0: 0.,
            y1: 60.,
            text: "0".into(),
            words: vec![6],
        });
        let t = table(&mut u).render();
        let Unit::Plain(l) = &mut u[0] else { unreachable!() };
        l[0].text = t;
        unchanged(u, w, g);
    }
    #[test]
    fn a_merged_physical_slot_does_not_supply_two_columns() {
        let (u, w, mut g) = fixture();
        g[0].cells = vec![GridCell { x0: 0, x1: 2, y0: 0, y1: 1 }];
        unchanged(u, w, g);
    }
}

/// 格子のない財務諸表でも、既存の横組み行と数値の右揃え位置が一意なら原字の横棒を戻す。
/// 表は推定せず、確定した行の後ろ（単位だけ残った場合はその直前）へ戻す。
pub(crate) fn restore_plain_numeric_dashes(units: &mut [Unit], words: &mut [Word], vrules: &[(f32, f32, f32)]) {
    if !words.iter().any(|w| w.status == WordStatus::Empty && w.raw == "\u{E61C}") {
        return;
    }
    let proposals = {
        let lines: Vec<_> = units
            .iter()
            .enumerate()
            .flat_map(|(u, unit)| match unit {
                Unit::Plain(ls) => ls
                    .iter()
                    .enumerate()
                    .filter(|(_, l)| l.table.is_none() && !l.words.is_empty())
                    .map(move |(i, l)| (u, i, l))
                    .collect::<Vec<_>>(),
                _ => Vec::new(),
            })
            .collect();
        let mut proposals = Vec::new();
        for (id, dash) in words.iter().enumerate() {
            let em = dash.y1 - dash.y0;
            if dash.status != WordStatus::Empty
                || dash.raw != "\u{E61C}"
                || !(3.0..=20.0).contains(&em)
                || !(em * 0.7..=em * 1.3).contains(&(dash.x1 - dash.x0))
            {
                continue;
            }
            let (x, y) = ((dash.x0 + dash.x1) / 2., (dash.y0 + dash.y1) / 2.);
            let rails: Vec<_> = vrules.iter().filter(|r| r.1 <= dash.y0 && r.2 >= dash.y1).collect();
            let Some(left) = rails.iter().filter(|r| r.0 < x).max_by(|a, b| a.0.total_cmp(&b.0)) else {
                continue;
            };
            let Some(right) = rails.iter().filter(|r| r.0 > x).min_by(|a, b| a.0.total_cmp(&b.0)) else {
                continue;
            };
            if !(150.0..=450.0).contains(&(right.0 - left.0)) {
                continue;
            }
            let (top, bottom) = (left.1.max(right.1), left.2.min(right.2));
            let row_words = |l: &Line| -> Option<Vec<&Word>> {
                let ws: Option<Vec<_>> = l.words.iter().map(|&i| words.get(i)).collect();
                let ws: Vec<_> = ws?.into_iter().filter(|w| !w.text.trim().is_empty()).collect();
                (!ws.is_empty()
                    && ws
                        .iter()
                        .all(|w| w.x0 >= left.0 - 1. && w.x1 <= right.0 + 1. && w.y0 >= top - 1. && w.y1 <= bottom + 1.))
                .then_some(ws)
            };
            let named = |w: &&Word, cy: f32| {
                w.x1 - w.x0 >= (w.y1 - w.y0) * 1.5
                    && w.text
                        .chars()
                        .filter(|c| ('\u{3040}'..='\u{30FF}').contains(c) || ('\u{4E00}'..='\u{9FFF}').contains(c))
                        .count()
                        >= 2
                    && ((w.y0 + w.y1) / 2. - cy).abs() <= (em.min(w.y1 - w.y0) * 0.4).max(1.)
            };
            let mut targets = Vec::new();
            for &(u, i, line) in &lines {
                if line.text.contains(['。', '、', '：', ':']) || line.text.chars().filter(|c| !c.is_whitespace()).count() > 60 {
                    continue;
                }
                let Some(ws) = row_words(line) else { continue };
                if !ws.iter().any(|w| w.x1 < dash.x0 && named(w, y)) {
                    continue;
                }
                let last = ws.iter().max_by(|a, b| a.x1.total_cmp(&b.x1)).unwrap();
                let unit = matches!(last.text.as_str(), "円" | "千円" | "百万円").then_some(last.text.as_str());
                let clear = ws
                    .iter()
                    .all(|w| w.x1 <= dash.x0 + 0.5 || (unit.is_some() && w.id == last.id && (w.x0 - dash.x1).abs() <= 1.));
                if !clear || unit.is_none() && last.x1 > dash.x0 {
                    continue;
                }
                let unit_end = last.x1;
                let mut anchors = Vec::new();
                for &(_, _, other) in &lines {
                    if other.text.contains(['。', '、', '：', ':']) {
                        continue;
                    }
                    let Some(vs) = row_words(other) else { continue };
                    let last = vs.iter().max_by(|a, b| a.x1.total_cmp(&b.x1)).unwrap();
                    let cy = (last.y0 + last.y1) / 2.;
                    if (cy - y).abs() > em * 10.
                        || last.text.chars().filter(|c| c.is_ascii_digit()).count() < 2
                        || !vs.iter().any(|w| w.x1 < last.x0 - em && named(w, cy))
                    {
                        continue;
                    }
                    let numeric = match unit {
                        Some(unit) => (last.x1 - unit_end).abs() <= 1. && last.text.strip_suffix(unit).is_some_and(amount),
                        None => (last.x1 - dash.x1).abs() <= 1. && amount(&last.text),
                    };
                    if numeric {
                        anchors.push(cy);
                    }
                }
                anchors.sort_by(f32::total_cmp);
                anchors.dedup_by(|a, b| (*a - *b).abs() < em);
                if anchors.len() >= 2 {
                    targets.push((u, i, unit.map(|s| (s.to_string(), last.id))));
                }
            }
            if targets.len() == 1 {
                let (u, i, unit) = targets.pop().unwrap();
                proposals.push((id, u, i, unit));
            }
        }
        proposals
    };
    for (id, u, i, unit) in &proposals {
        // 同じ行・右揃え位置に複数の原字があれば一意ではない。
        if proposals
            .iter()
            .filter(|(j, v, k, _)| v == u && k == i && (words[*j].x1 - words[*id].x1).abs() < 1.)
            .count()
            != 1
        {
            continue;
        }
        let Unit::Plain(lines) = &mut units[*u] else { continue };
        let line = &mut lines[*i];
        if let Some((unit, unit_id)) = unit {
            let Some(at) = line.text.strip_suffix(unit).map(str::len) else {
                continue;
            };
            let Some(pos) = line.words.iter().position(|&w| w == *unit_id) else {
                continue;
            };
            line.text.insert(at, '－');
            line.words.insert(pos, *id);
        } else {
            line.text.push('　');
            line.text.push('－');
            line.words.push(*id);
        }
        words[*id].text = "－".into();
        words[*id].status = WordStatus::Used;
    }
}

#[cfg(test)]
mod plain_numeric_dash_tests {
    use super::*;
    fn fixture(unit: bool) -> (Vec<Unit>, Vec<Word>, Vec<(f32, f32, f32)>) {
        let mut words = Vec::new();
        for (x0, x1, y0, text) in [
            (10., 74., 8., "収入"),
            (270., 302., 8., "100"),
            (10., 74., 28., "費用"),
            (10., 74., 48., "合計"),
            (270., 302., 48., "200"),
        ] {
            let id = words.len();
            words.push(Word {
                id,
                x0,
                x1,
                y0,
                y1: y0 + 8.,
                raw: text.into(),
                text: text.into(),
                status: WordStatus::Used,
            });
        }
        words.push(Word {
            id: 5,
            x0: 294.,
            x1: 302.,
            y0: 28.,
            y1: 36.,
            raw: "\u{E61C}".into(),
            text: String::new(),
            status: WordStatus::Empty,
        });
        let mut lines = vec![
            Line::text("収入　100".into(), vec![0, 1]),
            Line::text("費用".into(), vec![2]),
            Line::text("合計　200".into(), vec![3, 4]),
        ];
        if unit {
            for id in [1, 4] {
                words[id].text.push_str("千円");
                words[id].x0 = 290.;
                words[id].x1 = 318.;
            }
            words.push(Word {
                id: 6,
                x0: 302.,
                x1: 318.,
                y0: 28.,
                y1: 36.,
                raw: "千円".into(),
                text: "千円".into(),
                status: WordStatus::Used,
            });
            lines[0].text.push_str("千円");
            lines[1].text.push_str("　千円");
            lines[1].words.push(6);
            lines[2].text.push_str("千円");
        }
        (vec![Unit::Plain(lines)], words, vec![(0., 0., 64.), (320., 0., 64.)])
    }
    #[test]
    fn plain_rows_recover_an_existing_bar_at_the_numeric_edge_once() {
        for unit in [false, true] {
            let (mut u, mut w, r) = fixture(unit);
            restore_plain_numeric_dashes(&mut u, &mut w, &r);
            assert!(u[0].text().contains(if unit { "費用　－千円" } else { "費用　－" }));
            assert_eq!(w[5].status, WordStatus::Used);
            let before = u[0].text();
            restore_plain_numeric_dashes(&mut u, &mut w, &r);
            assert_eq!(u[0].text(), before);
            let Unit::Plain(lines) = &u[0] else { unreachable!() };
            assert_eq!(lines.iter().flat_map(|l| &l.words).filter(|&&id| id == 5).count(), 1);
            if unit {
                assert_eq!(lines[1].words, vec![2, 5, 6]);
            }
        }
    }
    #[test]
    fn reconstructed_formula_label_and_unit_text_are_only_locally_extended() {
        for (unit, text, expected) in [
            (false, "費用　(1)/(2)", "費用　(1)/(2)　－"),
            (false, "１　保険契約準備金への繰入額", "１　保険契約準備金への繰入額　－"),
            (true, "費用（換算後）　千円", "費用（換算後）　－千円"),
        ] {
            let (mut u, mut w, r) = fixture(unit);
            if let Unit::Plain(ls) = &mut u[0] {
                ls[1].text = text.into();
            }
            let before = u[0].text();
            restore_plain_numeric_dashes(&mut u, &mut w, &r);
            let Unit::Plain(ls) = &u[0] else { unreachable!() };
            assert_eq!(ls[1].text, expected);
            let strip = |s: &str| s.chars().filter(|c| !c.is_whitespace() && *c != '－').collect::<String>();
            assert_eq!(strip(&before), strip(&u[0].text()));
        }
    }
    #[test]
    fn uncertain_columns_rows_glyphs_and_existing_values_are_not_changed() {
        for case in 0..12 {
            let (mut u, mut w, mut r) = fixture(false);
            match case {
                0 => w[5].raw = "\u{E999}".into(),
                1 => r.clear(),
                2 => w[4].text = "不明".into(),
                3 => w[4].x1 += 8.,
                4 => {
                    if let Unit::Plain(ls) = &mut u[0] {
                        ls.insert(2, ls[1].clone());
                    }
                }
                5 => {
                    let mut d = w[5].clone();
                    d.id = 6;
                    w.push(d);
                }
                6 => {
                    w.push(Word {
                        id: 6,
                        x0: 294.,
                        x1: 302.,
                        y0: 28.,
                        y1: 36.,
                        raw: "0".into(),
                        text: "0".into(),
                        status: WordStatus::Used,
                    });
                    if let Unit::Plain(ls) = &mut u[0] {
                        ls[1].words.push(6);
                        ls[1].text.push('0');
                    }
                }
                7 => {
                    if let Unit::Plain(ls) = &mut u[0] {
                        ls[1].text = "費用を説明する。".into();
                    }
                }
                8 => r[0].2 = 24.,
                9 => {
                    w[2].x1 = w[2].x0 + 8.;
                    w[2].y1 = w[2].y0 + 16.;
                }
                10 => {
                    w[1].text = "1".into();
                    w[4].text = "2".into();
                }
                11 => w[5].x0 -= 24.,
                _ => unreachable!(),
            }
            let before = u[0].text();
            restore_plain_numeric_dashes(&mut u, &mut w, &r);
            assert_eq!(u[0].text(), before, "negative {case}");
            assert_eq!(w[5].status, WordStatus::Empty);
        }
        let (mut u, mut w, r) = fixture(true);
        w[6].text = "円".into();
        if let Unit::Plain(ls) = &mut u[0] {
            ls[1].text = "費用　円".into();
        }
        let before = u[0].text();
        restore_plain_numeric_dashes(&mut u, &mut w, &r);
        assert_eq!(u[0].text(), before, "different units cannot anchor the missing amount");
    }
}

#[cfg(test)]
#[path = "leading_dash_tests.rs"]
mod leading_dash_tests;
