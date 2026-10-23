//! Restore verified degree/minute/second glyphs inside final coordinate cells.
use crate::trace::{Line, TableCell, Unit, Word, WordStatus};
use std::collections::HashSet;
pub(crate) fn restore_coordinate_symbols(units: &mut [Unit], words: &mut [Word]) {
    if !words.iter().any(|w| {
        w.status == WordStatus::Empty
            && matches!(w.raw.as_str(), "\u{EA6F}" | "\u{EA6D}" | "\u{EA6E}")
    }) {
        return;
    }
    let mut attached = HashSet::new();
    let mut owner_counts = vec![0usize; words.len()];
    let mut collect = |lines: &[Line]| {
        for line in lines {
            for &id in &line.words {
                if let Some(n) = owner_counts.get_mut(id) {
                    *n += 1;
                }
                attached.insert(id);
            }
            if let Some(t) = &line.table {
                for c in &t.cells {
                    for &id in &c.words {
                        if let Some(n) = owner_counts.get_mut(id) {
                            *n += 1;
                        }
                        attached.insert(id);
                    }
                }
            }
        }
    };
    for u in units.iter() {
        match u {
            Unit::Plain(p) => collect(p),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            } => {
                collect(preamble);
                for p in after.iter().chain(before).chain(outside) {
                    collect(p);
                }
            }
        }
    }
    fn lines(
        lines: &mut [Line],
        words: &mut [Word],
        attached: &mut HashSet<usize>,
        owner_counts: &[usize],
    ) {
        for line in lines {
            let Some(table) = line.table.as_mut() else {
                continue;
            };
            if table.cols != 3 || table.rows < 2 {
                continue;
            }
            let header = |col| {
                table
                    .cells
                    .iter()
                    .filter(|c| c.row == 0 && c.col == col && c.rowspan == 1 && c.colspan == 1)
                    .collect::<Vec<_>>()
            };
            let hs = [header(0), header(1), header(2)];
            if hs.iter().any(|h| h.len() != 1)
                || hs[0][0].text.trim() != "点"
                || hs[1][0].text.trim() != "北緯"
                || hs[2][0].text.trim() != "東経"
            {
                continue;
            }
            let header_bottom = hs.iter().map(|h| h[0].y1).fold(f32::NEG_INFINITY, f32::max);
            let mut changed = false;
            for row in 1..table.rows {
                let indices: Vec<_> = table
                    .cells
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| c.row == row)
                    .map(|(i, _)| i)
                    .collect();
                if indices.len() != 3 {
                    continue;
                }
                let at = |col| {
                    indices.iter().copied().find(|&i| {
                        table.cells[i].col == col
                            && table.cells[i].rowspan == 1
                            && table.cells[i].colspan == 1
                    })
                };
                let (Some(point), Some(lat), Some(lon)) = (at(0), at(1), at(2)) else {
                    continue;
                };
                let point_text = table.cells[point].text.trim();
                if point_text.is_empty()
                    || point_text.chars().count() > 4
                    || !point_text
                        .chars()
                        .all(|c| c.is_ascii_digit() || ('０'..='９').contains(&c))
                    || table.cells[lat].y0 < header_bottom
                    || table.cells[lon].y0 < header_bottom
                    || (table.cells[lat].y0 - table.cells[lon].y0).abs() > 0.1
                    || (table.cells[lat].y1 - table.cells[lon].y1).abs() > 0.1
                {
                    continue;
                }
                // Both halves of one named coordinate must be proven before either is edited.
                let (Some(a), Some(b)) = (
                    restore_cell(&table.cells[lat], words, 90, owner_counts),
                    restore_cell(&table.cells[lon], words, 180, owner_counts),
                ) else {
                    continue;
                };
                let ids: Vec<_> = a
                    .symbols
                    .iter()
                    .chain(&b.symbols)
                    .map(|(id, _)| *id)
                    .collect();
                if ids.iter().any(|id| attached.contains(id))
                    || ids
                        .iter()
                        .enumerate()
                        .any(|(k, id)| ids[k + 1..].contains(id))
                {
                    continue;
                }
                for (index, edit) in [(lat, a), (lon, b)] {
                    table.cells[index].text = edit.text;
                    for (id, ch) in edit.symbols {
                        table.cells[index].words.push(id);
                        words[id].text = ch.to_string();
                        words[id].status = WordStatus::Used;
                        attached.insert(id);
                    }
                }
                changed = true;
            }
            if changed {
                line.text = table.render();
            }
        }
    }
    for unit in units {
        match unit {
            Unit::Plain(p) => lines(p, words, &mut attached, &owner_counts),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            } => {
                lines(preamble, words, &mut attached, &owner_counts);
                for p in after.iter_mut().chain(before).chain(outside) {
                    lines(p, words, &mut attached, &owner_counts);
                }
            }
        }
    }
}
pub(crate) struct Edit {
    pub(crate) text: String,
    pub(crate) symbols: Vec<(usize, char)>,
}
pub(crate) fn restore_cell(
    cell: &TableCell,
    words: &[Word],
    degree_limit: u32,
    owner_counts: &[usize],
) -> Option<Edit> {
    // Use original numeric Word boundaries only when the final cell omitted spacing.
    // The five proven source fields remain the layout anchors and keep their IDs.
    if cell.words.len() != 5 {
        return None;
    }
    let mut source: Vec<_> = cell
        .words
        .iter()
        .map(|&id| words.get(id).filter(|w| w.id == id))
        .collect::<Option<_>>()?;
    if cell
        .words
        .iter()
        .enumerate()
        .any(|(i, id)| cell.words[i + 1..].contains(id))
    {
        return None;
    }
    source.sort_by(|a, b| a.x0.total_cmp(&b.x0));
    let original_fields: Vec<_> = cell.text.split_whitespace().collect();
    let joined = original_fields.len() == 1;
    let joined_second = format!("{}{}{}", source[2].text, source[3].text, source[4].text);
    let fields: Vec<&str> = if joined {
        if cell
            .words
            .iter()
            .any(|&id| owner_counts.get(id) != Some(&1))
            || cell.words.iter().zip(&source).any(|(&id, w)| id != w.id)
            || cell.text.trim() != source.iter().map(|w| w.text.as_str()).collect::<String>()
        {
            return None;
        }
        vec![
            source[0].text.as_str(),
            source[1].text.as_str(),
            &joined_second,
        ]
    } else {
        original_fields
    };
    if fields.len() != 3
        || !fields[0].chars().all(|c| c.is_ascii_digit())
        || !fields[1].chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    let degree: u32 = fields[0].parse().ok()?;
    let minute: u32 = fields[1].parse().ok()?;
    let (whole, fraction) = fields[2].split_once('.')?;
    if degree > degree_limit
        || minute >= 60
        || fields[0].len() > 3
        || fields[1].len() != 2
        || whole.len() != 2
        || fraction.is_empty()
        || fraction.len() > 8
        || !whole
            .chars()
            .chain(fraction.chars())
            .all(|c| c.is_ascii_digit())
        || whole.parse::<u32>().ok()? >= 60
    {
        return None;
    }
    let expected = [fields[0], fields[1], whole, ".", fraction];
    let em = source[0].y1 - source[0].y0;
    if !em.is_finite() || em <= 0. {
        return None;
    }
    if source.iter().zip(expected).any(|(w, text)| {
        w.text != text
            || w.status != WordStatus::Used
            || w.text.is_empty()
            || (w.y0 - source[0].y0).abs() > em * 0.05
            || ((w.y1 - w.y0) - em).abs() > em * 0.05
            || w.x0 < cell.x0
            || w.x1 > cell.x1 + em * 0.05
            || w.y0 < cell.y0
            || w.y1 > cell.y1 + em * 0.05
    }) {
        return None;
    }
    // Horizontal advance is required even when the surrounding article is vertical.
    if source.iter().any(|w| {
        !((w.text.len() as f32 * em * 0.35)..=(w.text.len().max(1) as f32 * em * 1.05))
            .contains(&(w.x1 - w.x0))
    }) {
        return None;
    }
    let glyphs: Vec<_> = words
        .iter()
        .filter(|w| {
            w.raw
                .chars()
                .any(|c| matches!(c, '\u{EA6F}' | '\u{EA6D}' | '\u{EA6E}'))
                && (cell.x0..cell.x1).contains(&((w.x0 + w.x1) * 0.5))
                && (cell.y0..cell.y1).contains(&((w.y0 + w.y1) * 0.5))
        })
        .collect();
    if glyphs.len() != 3 {
        return None;
    }
    let mut symbols = Vec::new();
    for (ch, symbol, anchor, next) in [
        ('\u{EA6F}', '°', source[0], Some(source[1])),
        ('\u{EA6D}', '′', source[1], Some(source[2])),
        ('\u{EA6E}', '″', source[4], None),
    ] {
        let matching: Vec<_> = glyphs
            .iter()
            .copied()
            .filter(|w| w.raw == ch.to_string())
            .collect();
        if matching.len() != 1 {
            return None;
        }
        let w = matching[0];
        if words
            .get(w.id)
            .is_none_or(|original| !std::ptr::eq(w, original))
            || w.status != WordStatus::Empty
            || !w.text.is_empty()
            || cell.words.contains(&w.id)
            || (w.y0 - anchor.y0).abs() > em * 0.05
            || ((w.y1 - w.y0) - em).abs() > em * 0.05
            || !(-em * 0.35..=em * 0.10).contains(&(w.x0 - anchor.x1))
            || !(em * 0.8..=em * 1.2).contains(&(w.x1 - w.x0))
            || w.x0 < cell.x0
            || w.x1 > cell.x1 + em * 0.05
            || w.y0 < cell.y0
            || w.y1 > cell.y1 + em * 0.05
            || next.is_some_and(|n| !(em * 0.5..=em * 1.2).contains(&(n.x0 - w.x0)))
        {
            return None;
        }
        symbols.push((w.id, symbol));
    }
    // Insert only into the existing text. Preserve every original byte, including whitespace.
    let ends = if joined {
        let offset = cell.text.len() - cell.text.trim_start().len();
        vec![
            offset + source[0].text.len(),
            offset + source[0].text.len() + source[1].text.len(),
            cell.text.trim_end().len(),
        ]
    } else {
        let mut ends = Vec::new();
        let mut inside = false;
        for (i, ch) in cell.text.char_indices() {
            if ch.is_whitespace() {
                if inside {
                    ends.push(i);
                    inside = false;
                }
            } else {
                inside = true;
            }
        }
        if inside {
            ends.push(cell.text.len());
        }
        ends
    };
    if ends.len() != 3 {
        return None;
    }
    let mut text = cell.text.clone();
    for (&at, ch) in ends.iter().zip(['°', '′', '″']).rev() {
        text.insert(at, ch);
    }
    Some(Edit { text, symbols })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::Table;
    fn word(id: usize, text: &str, raw: &str, x: f32, y: f32, width: f32) -> Word {
        Word {
            source_box: [x, y, x + width, y + 8.],
            id,
            x0: x,
            y0: y,
            x1: x + width,
            y1: y + 8.,
            text: text.into(),
            raw: raw.into(),
            status: if text.is_empty() {
                WordStatus::Empty
            } else {
                WordStatus::Used
            },
        }
    }
    fn fixture() -> (Vec<Unit>, Vec<Word>) {
        let mut words = Vec::new();
        let mut cells = Vec::new();
        for (col, head) in ["点", "北緯", "東経"].iter().enumerate() {
            cells.push(TableCell {
                row: 0,
                col,
                rowspan: 1,
                colspan: 1,
                x0: col as f32 * 100.,
                y0: 0.,
                x1: (col + 1) as f32 * 100.,
                y1: 15.,
                text: (*head).into(),
                words: vec![],
            });
        }
        cells.push(TableCell {
            row: 1,
            col: 0,
            rowspan: 1,
            colspan: 1,
            x0: 0.,
            y0: 15.,
            x1: 100.,
            y1: 30.,
            text: "１".into(),
            words: vec![],
        });
        for (col, deg) in [(1, "34"), (2, "137")] {
            let x = col as f32 * 100. + 4.;
            let width = deg.len() as f32 * 4.;
            let mut ids = Vec::new();
            for (text, raw, px, pw) in [
                (deg, deg, x, width),
                ("", "\u{EA6F}", x + width - 2., 8.),
                ("53", "53", x + width + 4., 8.),
                ("", "\u{EA6D}", x + width + 10., 8.),
                ("19", "19", x + width + 16., 8.),
                (".", "\u{EA74}", x + width + 21., 8.),
                ("8544", "8544", x + width + 26., 16.),
                ("", "\u{EA6E}", x + width + 40., 8.),
            ] {
                let id = words.len();
                if !text.is_empty() {
                    ids.push(id);
                }
                words.push(word(id, text, raw, px, 18., pw));
            }
            cells.push(TableCell {
                row: 1,
                col,
                rowspan: 1,
                colspan: 1,
                x0: col as f32 * 100.,
                y0: 15.,
                x1: (col + 1) as f32 * 100.,
                y1: 30.,
                text: format!("{deg} 53 19.8544"),
                words: ids,
            });
        }
        let t = Table {
            x0: 0.,
            y0: 0.,
            x1: 300.,
            y1: 30.,
            rows: 2,
            cols: 3,
            cells,
        };
        (
            vec![Unit::Plain(vec![Line {
                text: t.render(),
                words: vec![],
                table: Some(t),
            }])],
            words,
        )
    }
    fn table(u: &mut [Unit]) -> &mut Table {
        let Unit::Plain(ls) = &mut u[0] else { panic!() };
        ls[0].table.as_mut().unwrap()
    }
    fn snapshot(u: &[Unit], w: &[Word]) -> String {
        format!(
            "{}{}",
            u.iter().map(Unit::text).collect::<Vec<_>>().join("\n"),
            serde_json::to_string(w).unwrap()
        )
    }
    #[test]
    fn restores_only_verified_paired_coordinate_cells() {
        let (mut u, mut w) = fixture();
        let old = w.clone();
        restore_coordinate_symbols(&mut u, &mut w);
        assert_eq!(table(&mut u).cells[4].text, "34° 53′ 19.8544″");
        assert_eq!(table(&mut u).cells[5].text, "137° 53′ 19.8544″");
        for (a, b) in w.iter().zip(&old) {
            if b.text.is_empty() {
                assert_eq!(a.status, WordStatus::Used);
                assert!("°′″".contains(&a.text));
            } else {
                assert_eq!(
                    serde_json::to_string(a).unwrap(),
                    serde_json::to_string(b).unwrap()
                );
            }
            assert_eq!(
                (&a.raw, a.id, a.x0, a.y0, a.x1, a.y1),
                (&b.raw, b.id, b.x0, b.y0, b.x1, b.y1)
            );
        }
        let once = snapshot(&u, &w);
        restore_coordinate_symbols(&mut u, &mut w);
        assert_eq!(snapshot(&u, &w), once);
    }
    #[test]
    fn rejects_non_coordinate_headers_and_plain_prose() {
        for heading in ["金額", "東経度"] {
            let (mut v, mut w) = fixture();
            table(&mut v).cells[2].text = heading.into();
            let before = snapshot(&v, &w);
            restore_coordinate_symbols(&mut v, &mut w);
            assert_eq!(snapshot(&v, &w), before);
        }
        let (mut v, mut w) = fixture();
        let Unit::Plain(ls) = &mut v[0] else { panic!() };
        ls[0].table = None;
        let before = snapshot(&v, &w);
        restore_coordinate_symbols(&mut v, &mut w);
        assert_eq!(snapshot(&v, &w), before);
    }
    #[test]
    fn rejects_missing_duplicate_or_nonempty_pua() {
        for mode in 0..3 {
            let (mut u, mut w) = fixture();
            match mode {
                0 => {
                    w[7].raw = "".into();
                }
                1 => {
                    let mut a = w[1].clone();
                    a.id = w.len();
                    w.push(a);
                }
                _ => {
                    w[1].text = "°".into();
                    w[1].status = WordStatus::Used;
                }
            }
            let before = snapshot(&u, &w);
            restore_coordinate_symbols(&mut u, &mut w);
            assert_eq!(snapshot(&u, &w), before);
        }
    }
    #[test]
    fn rejects_shifted_suffix_baseline_and_vertical_geometry() {
        for mode in 0..3 {
            let (mut u, mut w) = fixture();
            match mode {
                0 => {
                    w[1].x0 += 4.;
                    w[1].x1 += 4.;
                }
                1 => {
                    w[1].y0 += 4.;
                    w[1].y1 += 4.;
                }
                _ => {
                    for a in &mut w {
                        a.x1 = a.x0 + 8.;
                        a.y1 = a.y0 + 20.;
                    }
                }
            }
            let before = snapshot(&u, &w);
            restore_coordinate_symbols(&mut u, &mut w);
            assert_eq!(snapshot(&u, &w), before);
        }
    }
    #[test]
    fn rejects_numeric_decoys_existing_symbols_and_incomplete_pairs() {
        for mode in 0..5 {
            let (mut u, mut w) = fixture();
            match mode {
                0 => {
                    table(&mut u).cells[4].text = "91 53 19.8544".into();
                    w[0].text = "91".into();
                }
                1 => {
                    table(&mut u).cells[4].text = "34° 53′ 19.8544″".into();
                }
                2 => {
                    table(&mut u).cells[5].col = 3;
                }
                3 => {
                    let c = &mut table(&mut u).cells[4];
                    c.words[1] = c.words[0];
                }
                _ => {
                    table(&mut u).cells[4].text = "34 53 19.8544 0".into();
                }
            }
            let before = snapshot(&u, &w);
            restore_coordinate_symbols(&mut u, &mut w);
            assert_eq!(snapshot(&u, &w), before);
        }
    }
    #[test]
    fn preserves_existing_whitespace_ids_geometry_and_structure() {
        let (mut u, mut w) = fixture();
        table(&mut u).cells[4].text = " 34\t53  19.8544 ".into();
        let old = table(&mut u).clone();
        restore_coordinate_symbols(&mut u, &mut w);
        let t = table(&mut u);
        assert_eq!(t.cells[4].text, " 34°\t53′  19.8544″ ");
        let strip = |s: &str| {
            s.chars()
                .filter(|c| !"°′″".contains(*c))
                .collect::<String>()
        };
        for (a, b) in t.cells.iter().zip(&old.cells) {
            assert_eq!(strip(&a.text), b.text);
            assert_eq!(&a.words[..b.words.len()], b.words);
            for id in &a.words[b.words.len()..] {
                assert!("°′″".contains(&w[*id].text));
            }
            assert_eq!(
                (a.row, a.col, a.rowspan, a.colspan, a.x0, a.y0, a.x1, a.y1),
                (b.row, b.col, b.rowspan, b.colspan, b.x0, b.y0, b.x1, b.y1)
            );
        }
    }
    #[test]
    fn rejects_pua_with_an_existing_owner_or_wrong_source_id() {
        for mode in 0..2 {
            let (mut u, mut w) = fixture();
            if mode == 0 {
                table(&mut u).cells[0].words.push(1);
            } else {
                w[1].id = 777;
            }
            let before = snapshot(&u, &w);
            restore_coordinate_symbols(&mut u, &mut w);
            assert_eq!(snapshot(&u, &w), before);
        }
    }
    #[test]
    fn rejects_cell_boundary_and_other_row_symbols() {
        for mode in 0..2 {
            let (mut u, mut w) = fixture();
            if mode == 0 {
                table(&mut u).cells[4].x1 = w[7].x0 + 1.;
            } else {
                w[7].y0 += 16.;
                w[7].y1 += 16.;
            }
            let before = snapshot(&u, &w);
            restore_coordinate_symbols(&mut u, &mut w);
            assert_eq!(snapshot(&u, &w), before);
        }
    }
    #[test]
    fn rejects_non_used_numeric_anchors() {
        for status in [
            WordStatus::Margin,
            WordStatus::Empty,
            WordStatus::Noise,
            WordStatus::Ruby,
            WordStatus::HeaderLabel,
        ] {
            for anchor in [0, 2, 4, 5, 6] {
                let (mut u, mut w) = fixture();
                w[anchor].status = status;
                let before = snapshot(&u, &w);
                restore_coordinate_symbols(&mut u, &mut w);
                assert_eq!(snapshot(&u, &w), before);
            }
        }
    }
    #[test]
    fn restores_joined_numeric_components_at_original_word_boundaries() {
        let (mut u, mut w) = fixture();
        table(&mut u).cells[4].text = "345319.8544".into();
        table(&mut u).cells[5].text = "1375319.8544".into();
        let old = table(&mut u).clone();
        let old_words = w.clone();
        restore_coordinate_symbols(&mut u, &mut w);
        let t = table(&mut u);
        assert_eq!(t.cells[4].text, "34°53′19.8544″");
        assert_eq!(t.cells[5].text, "137°53′19.8544″");
        for (a, b) in t.cells.iter().zip(&old.cells) {
            assert_eq!(a.text.replace(['°', '′', '″'], ""), b.text);
            assert_eq!(&a.words[..b.words.len()], b.words);
        }
        for (a, b) in w.iter().zip(old_words) {
            if !b.text.is_empty() {
                assert_eq!(
                    serde_json::to_string(a).unwrap(),
                    serde_json::to_string(&b).unwrap()
                );
            }
        }
        let once = snapshot(&u, &w);
        restore_coordinate_symbols(&mut u, &mut w);
        assert_eq!(snapshot(&u, &w), once);
    }
    #[test]
    fn joined_cells_preserve_outer_whitespace_and_equal_numeric_parts() {
        let (mut u, mut w) = fixture();
        table(&mut u).cells[4].text = " \t345353.8544 ".into();
        w[4].text = "53".into();
        w[4].raw = "53".into();
        table(&mut u).cells[5].text = "1375319.8544".into();
        restore_coordinate_symbols(&mut u, &mut w);
        assert_eq!(table(&mut u).cells[4].text, " \t34°53′53.8544″ ");
    }
    #[test]
    fn joined_cells_reject_mismatched_text_reordered_ids_and_other_pua() {
        for mode in 0..6 {
            let (mut u, mut w) = fixture();
            table(&mut u).cells[4].text = "345319.8544".into();
            table(&mut u).cells[5].text = "1375319.8544".into();
            match mode {
                0 => table(&mut u).cells[4].text = "345319.85440".into(),
                1 => table(&mut u).cells[4].words.swap(0, 1),
                2 => w[1].raw = "\u{EA70}".into(),
                3 => table(&mut u).cells[4].text = "345319.8544円".into(),
                4 => table(&mut u).cells[4].text = "34°5319.8544".into(),
                _ => table(&mut u).cells[0].words.push(0),
            }
            let old = snapshot(&u, &w);
            restore_coordinate_symbols(&mut u, &mut w);
            assert_eq!(snapshot(&u, &w), old);
        }
    }
}
