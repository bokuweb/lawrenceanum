//! Reassign a native horizontal triangle inside an already assembled table.
use crate::trace::{Line, Table, TableCell, Unit, Word, WordStatus};
use scraper::{ElementRef, Selector};

fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn native_signs(page: ElementRef<'_>, words: &[Word]) -> Vec<(usize, [f32; 4])> {
    let selector = Selector::parse("word").unwrap();
    let elements: Vec<_> = page.select(&selector).collect();
    let mut out = Vec::new();
    for e in &elements {
        let a = e.value();
        let num = |name| a.attr(name).and_then(|v| v.parse::<f32>().ok());
        if a.attr("data-wmode") != Some("1") {
            continue;
        }
        let Some((ax, ay)) = num("data-advance-x").zip(num("data-advance-y")) else {
            continue;
        };
        if !ax.is_finite() || !ay.is_finite() || ax.abs() <= ay.abs() * 1.5 || ax.abs() == 0.0 {
            continue;
        }
        let mut box4 = [0.; 4];
        let mut native = [0.; 4];
        let mut valid = true;
        for (slot, key) in box4.iter_mut().zip(["xmin", "ymin", "xmax", "ymax"]) {
            match num(key).filter(|n| n.is_finite()) {
                Some(n) => *slot = n,
                None => valid = false,
            }
        }
        for (slot, key) in native.iter_mut().zip([
            "data-native-xmin",
            "data-native-ymin",
            "data-native-xmax",
            "data-native-ymax",
        ]) {
            match num(key).filter(|n| n.is_finite()) {
                Some(n) => *slot = n,
                None => valid = false,
            }
        }
        if !valid || native[2] <= native[0] || native[3] <= native[1] {
            continue;
        }
        let raw: String = e.text().collect();
        let same_source = |o: &&ElementRef<'_>| {
            o.text().collect::<String>() == raw
                && ["xmin", "ymin", "xmax", "ymax"]
                    .iter()
                    .zip(box4)
                    .all(|(key, n)| {
                        o.value().attr(key).and_then(|v| v.parse::<f32>().ok()) == Some(n)
                    })
        };
        if elements.iter().filter(same_source).count() != 1 {
            continue;
        }
        let matching: Vec<_> = words
            .iter()
            .filter(|w| {
                w.raw == raw
                    && w.text == "△"
                    && w.status == WordStatus::Used
                    && [w.x0, w.y0, w.x1, w.y1] == box4
            })
            .collect();
        if let [w] = matching.as_slice() {
            out.push((w.id, native));
        }
    }
    out
}

fn owner_count(units: &[Unit], id: usize) -> usize {
    let mut n = 0;
    let mut line = |l: &Line| {
        n += match &l.table {
            Some(t) => t
                .cells
                .iter()
                .map(|c| c.words.iter().filter(|&&i| i == id).count())
                .sum(),
            None => l.words.iter().filter(|&&i| i == id).count(),
        };
    };
    for u in units {
        match u {
            Unit::Plain(ls) => ls.iter().for_each(&mut line),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            ..} => {
                preamble.iter().for_each(&mut line);
                for p in after.iter().chain(before).chain(outside) {
                    p.iter().for_each(&mut line)
                }
            }
        }
    }
    n
}

fn move_in_table(t: &mut Table, words: &[Word], id: usize, b: [f32; 4]) -> bool {
    let sources: Vec<_> = t
        .cells
        .iter()
        .enumerate()
        .filter(|(_, c)| c.words.contains(&id))
        .map(|(i, _)| i)
        .collect();
    let [source] = sources.as_slice() else {
        return false;
    };
    let source = *source;
    let c = &t.cells[source];
    if c.colspan != 1 || c.rowspan != 1 || c.text.matches('△').count() != 1 {
        return false;
    }
    let Some(source_texts) = c
        .words
        .iter()
        .map(|&i| words.get(i).map(|w| w.text.as_str()))
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    if compact(&source_texts.concat()) != compact(&c.text) {
        return false;
    }
    let em = (b[2] - b[0]).max(b[3] - b[1]);
    let cx = (b[0] + b[2]) / 2.;
    let cy = (b[1] + b[3]) / 2.;
    if em < 1. || cx < t.x0 || cx > t.x1 || cy < t.y0 || cy > t.y1 || t.cols < 2 || t.rows < 3 {
        return false;
    }
    let headers: Vec<_> = t
        .cells
        .iter()
        .filter(|h| {
            h.rowspan == 1
                && h.colspan == 1
                && h.col > 0
                && compact(&h.text) == "金額"
                && b[0] >= h.x0 + em * 0.15
                && b[2] <= h.x1 + 0.5
        })
        .collect();
    let [header] = headers.as_slice() else {
        return false;
    };
    let amount_col = header.col;
    if c.col != amount_col && c.col + 1 != amount_col {
        return false;
    }
    let mut targets = Vec::new();
    for label in t.cells.iter().filter(|c| {
        c.col + 1 == amount_col
            && c.row > header.row
            && c.rowspan == 1
            && c.colspan == 1
            && (c.x1 - header.x0).abs() < 0.5
            && cy >= c.y0
            && cy <= c.y1
    }) {
        let label_words: Vec<_> = label
            .words
            .iter()
            .filter_map(|&i| words.get(i))
            .filter(|w| w.text != "△")
            .collect();
        // A group containing several printed rows is deliberately not split.
        let [w] = label_words.as_slice() else {
            continue;
        };
        let h = w.y1 - w.y0;
        let overlap = (b[3].min(w.y1) - b[1].max(w.y0)) / h.min(b[3] - b[1]).max(0.1);
        let japanese = w
            .text
            .chars()
            .filter(|c| ('\u{3040}'..='\u{9fff}').contains(c))
            .count();
        if w.status != WordStatus::Used
            || japanese < 2
            || w.x1 - w.x0 < h * 1.5
            || h < em * 0.7
            || h > em * 1.4
            || overlap < 0.7
            || (((w.y0 + w.y1) / 2.) - cy).abs() > em * 0.2
        {
            continue;
        }
        if compact(&label.text.replace('△', "")) != compact(&w.text) {
            continue;
        }
        targets.push((label.row, label.y0, label.y1));
    }
    let [(row, y0, y1)] = targets.as_slice() else {
        return false;
    };
    let (row, y0, y1) = (*row, *y0, *y1);
    if c.row == row && c.col == amount_col {
        return false;
    }
    let slots: Vec<_> = t
        .cells
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            c.row <= row
                && row < c.row + c.rowspan
                && c.col <= amount_col
                && amount_col < c.col + c.colspan
        })
        .map(|(i, _)| i)
        .collect();
    let target = match slots.as_slice() {
        [] => None,
        [i] => {
            let c = &t.cells[*i];
            if c.row != row
                || c.col != amount_col
                || c.rowspan != 1
                || c.colspan != 1
                || !c.words.is_empty()
                || !c.text.trim().is_empty()
            {
                return false;
            }
            Some(*i)
        }
        _ => return false,
    };
    let (x0, x1) = (header.x0, header.x1);
    t.cells[source].words.retain(|&i| i != id);
    t.cells[source].text = t.cells[source].text.replacen('△', "", 1);
    match target {
        Some(i) => {
            t.cells[i].text = "△".into();
            t.cells[i].words = vec![id];
        }
        None => t.cells.push(TableCell {
            row,
            col: amount_col,
            rowspan: 1,
            colspan: 1,
            x0,
            y0,
            x1,
            y1,
            text: "△".into(),
            words: vec![id],
        }),
    }
    true
}

pub(crate) fn restore(units: &mut [Unit], words: &[Word], page: ElementRef<'_>) {
    let signs = native_signs(page, words);
    for (id, b) in signs {
        if owner_count(units, id) != 1 {
            continue;
        }
        let mut fix = |l: &mut Line| {
            if let Some(t) = l.table.as_mut() {
                if move_in_table(t, words, id, b) {
                    l.text = t.render();
                }
            }
        };
        for u in units.iter_mut() {
            match u {
                Unit::Plain(ls) => ls.iter_mut().for_each(&mut fix),
                Unit::Shinkyu {
                    preamble,
                    after,
                    before,
                    outside,
                ..} => {
                    preamble.iter_mut().for_each(&mut fix);
                    for p in after.iter_mut().chain(before).chain(outside) {
                        p.iter_mut().for_each(&mut fix)
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "native_triangle_cell_tests.rs"]
mod tests;
