//! Restore verified square-metre glyphs after layout, preserving owner text and IDs.
use crate::trace::{Line, Unit, Word, WordStatus};
pub(crate) fn restore_area_units(units: &mut [Unit], words: &mut [Word]) {
    if !words.iter().any(|w| w.raw.contains('\u{E68D}')) {
        return;
    }
    let mut owners = vec![0usize; words.len()];
    let mut count = |ls: &[Line]| {
        for l in ls {
            for &id in l
                .words
                .iter()
                .chain(l.table.iter().flat_map(|t| &t.cells).flat_map(|c| &c.words))
            {
                if let Some(n) = owners.get_mut(id) {
                    *n += 1;
                }
            }
        }
    };
    for u in units.iter() {
        match u {
            Unit::Plain(p) => count(p),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            } => {
                count(preamble);
                for p in after.iter().chain(before).chain(outside) {
                    count(p);
                }
            }
        }
    }
    fn lines(ls: &mut [Line], words: &mut [Word], owners: &[usize]) {
        for line in ls {
            if let Some(t) = &mut line.table {
                let mut changed = false;
                for c in &mut t.cells {
                    changed |= restore_owner(&mut c.text, &c.words, words, owners);
                }
                if changed {
                    line.text = t.render();
                }
            } else {
                restore_owner(&mut line.text, &line.words, words, owners);
            }
        }
    }
    for u in units {
        match u {
            Unit::Plain(p) => lines(p, words, &owners),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            } => {
                lines(preamble, words, &owners);
                for p in after.iter_mut().chain(before).chain(outside) {
                    lines(p, words, &owners);
                }
            }
        }
    }
}
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn restore_owner(text: &mut String, ids: &[usize], words: &mut [Word], owners: &[usize]) -> bool {
    if ids.is_empty()
        || ids.iter().any(|&id| {
            words
                .get(id)
                .is_none_or(|w| w.id != id || w.status != WordStatus::Used || w.text.is_empty())
                || owners.get(id) != Some(&1)
        })
    {
        return false;
    }
    // Existing layout and ID order must already explain the entire cell/line.
    // This is proof of insertion positions, never a reconstruction of body text.
    let joined = ids
        .iter()
        .map(|&id| compact(&words[id].text))
        .collect::<String>();
    if compact(text) != joined {
        return false;
    }
    let boundaries: Vec<_> = text
        .char_indices()
        .filter(|(_, ch)| !ch.is_whitespace())
        .map(|(i, ch)| i + ch.len_utf8())
        .collect();
    let mut offset = 0usize;
    let mut edits = Vec::new();
    for &id in ids {
        let w = &words[id];
        if w.raw.chars().filter(|&c| c == '\u{E68D}').count() == 1
            && crate::normalize::word_text(&w.raw) == w.text
        {
            let (before, after) = w.raw.split_once('\u{E68D}').unwrap();
            let prefix = crate::normalize::word_text(before);
            let suffix = crate::normalize::word_text(after);
            let numeric = prefix
                .chars()
                .last()
                .is_some_and(|c| c.is_ascii_digit() || ('０'..='９').contains(&c));
            let area_label = prefix.ends_with(['（', '(']) && prefix.contains("面積");
            if (numeric || area_label) && format!("{prefix}{suffix}") == w.text {
                let pos = offset + compact(&prefix).chars().count();
                if let Some(&at) = pos.checked_sub(1).and_then(|i| boundaries.get(i)) {
                    edits.push((at, id, prefix.len()));
                }
            }
        }
        offset += compact(&w.text).chars().count();
    }
    if edits.is_empty() {
        return false;
    }
    edits.sort_by_key(|e| e.0);
    if edits.windows(2).any(|w| w[0].0 == w[1].0) {
        return false;
    }
    for &(at, id, word_at) in edits.iter().rev() {
        text.insert(at, '㎡');
        words[id].text.insert(word_at, '㎡');
    }
    true
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::{Table, TableCell};
    fn w(id: usize, raw: &str, text: &str) -> Word {
        Word {
            id,
            x0: id as f32 * 12.,
            y0: 10.,
            x1: id as f32 * 12. + 16.,
            y1: 18.,
            raw: raw.into(),
            text: text.into(),
            status: WordStatus::Used,
        }
    }
    fn table_fixture() -> (Vec<Unit>, Vec<Word>) {
        let words = vec![
            w(0, "21", "21"),
            w(1, "\u{EA75}", ","),
            w(2, "140", "140"),
            w(3, "\u{EA74}", "."),
            w(4, "60\u{E68D}", "60"),
        ];
        let table = Table {
            x0: 0.,
            y0: 0.,
            x1: 100.,
            y1: 30.,
            rows: 1,
            cols: 1,
            cells: vec![TableCell {
                row: 0,
                col: 0,
                rowspan: 1,
                colspan: 1,
                x0: 0.,
                y0: 0.,
                x1: 100.,
                y1: 30.,
                text: "21,140.60".into(),
                words: vec![0, 1, 2, 3, 4],
            }],
        };
        (
            vec![Unit::Plain(vec![Line {
                text: table.render(),
                words: vec![],
                table: Some(table),
            }])],
            words,
        )
    }
    fn text(u: &[Unit]) -> String {
        u.iter().map(Unit::text).collect::<Vec<_>>().join("\n")
    }
    fn snap(u: &[Unit], w: &[Word]) -> String {
        format!("{}{}", text(u), serde_json::to_string(w).unwrap())
    }
    #[test]
    fn restores_a_numeric_cell_without_changing_its_source_owner() {
        let (mut u, mut w) = table_fixture();
        let old = serde_json::to_value(&w).unwrap();
        restore_area_units(&mut u, &mut w);
        assert_eq!(text(&u), "21,140.60㎡");
        assert_eq!(w[4].text, "60㎡");
        let mut now = serde_json::to_value(&w).unwrap();
        now[4]["text"] = "60".into();
        assert_eq!(old, now);
        let Unit::Plain(ls) = &u[0] else { panic!() };
        assert_eq!(
            ls[0].table.as_ref().unwrap().cells[0].words,
            vec![0, 1, 2, 3, 4]
        );
    }
    #[test]
    fn inserts_inside_prose_and_area_labels_at_the_raw_glyph_position() {
        for (raw, before, after) in [
            (
                "50\u{E68D}であります。",
                "50であります。",
                "50㎡であります。",
            ),
            (
                "Ｘ：マンションの総専有床面積（\u{E68D}）",
                "Ｘ：マンションの総専有床面積（）",
                "Ｘ：マンションの総専有床面積（㎡）",
            ),
        ] {
            let mut w = vec![w(0, raw, before)];
            let mut u = vec![Unit::Plain(vec![Line::text(before.into(), vec![0])])];
            restore_area_units(&mut u, &mut w);
            assert_eq!(text(&u), after);
            assert_eq!(w[0].text, after);
        }
    }
    #[test]
    fn preserves_whitespace_existing_ids_status_and_geometry() {
        let (mut u, mut w) = table_fixture();
        let Unit::Plain(ls) = &mut u[0] else { panic!() };
        let c = &mut ls[0].table.as_mut().unwrap().cells[0];
        c.text = " 21,140.60 \t".into();
        let old = c.clone();
        restore_area_units(&mut u, &mut w);
        let Unit::Plain(ls) = &u[0] else { panic!() };
        let c = &ls[0].table.as_ref().unwrap().cells[0];
        assert_eq!(c.text, " 21,140.60㎡ \t");
        assert_eq!(c.words, old.words);
        assert_eq!(
            (c.row, c.col, c.rowspan, c.colspan, c.x0, c.y0, c.x1, c.y1),
            (
                old.row,
                old.col,
                old.rowspan,
                old.colspan,
                old.x0,
                old.y0,
                old.x1,
                old.y1
            )
        );
        let once = snap(&u, &w);
        restore_area_units(&mut u, &mut w);
        assert_eq!(snap(&u, &w), once);
    }
    #[test]
    fn rejects_neighboring_pua_and_already_unicode_units() {
        for raw in ["60\u{E68C}", "60\u{E68E}", "60㎡", "60\u{E68B}"] {
            let (mut u, mut w) = table_fixture();
            w[4].raw = raw.into();
            let before = snap(&u, &w);
            restore_area_units(&mut u, &mut w);
            assert_eq!(snap(&u, &w), before);
        }
    }
    #[test]
    fn rejects_duplicate_or_other_owners() {
        for mode in 0..2 {
            let (mut u, mut w) = table_fixture();
            if mode == 0 {
                let Unit::Plain(ls) = &mut u[0] else { panic!() };
                ls[0].table.as_mut().unwrap().cells[0].words.push(4);
            } else {
                u.push(Unit::Plain(vec![Line::text("60".into(), vec![4])]));
            }
            let before = snap(&u, &w);
            restore_area_units(&mut u, &mut w);
            assert_eq!(snap(&u, &w), before);
        }
    }
    #[test]
    fn rejects_mismatched_owner_word_text_and_raw_offsets() {
        for mode in 0..3 {
            let (mut u, mut w) = table_fixture();
            if mode == 0 {
                let Unit::Plain(ls) = &mut u[0] else { panic!() };
                ls[0].table.as_mut().unwrap().cells[0].text = "60 21,140.60".into();
            } else if mode == 1 {
                w[4].text = "61".into();
            } else {
                w[4].raw = "60\u{E68D}\u{E68D}".into();
            }
            let before = snap(&u, &w);
            restore_area_units(&mut u, &mut w);
            assert_eq!(snap(&u, &w), before);
        }
    }
    #[test]
    fn rejects_empty_non_used_words_and_non_area_decoys() {
        for mode in 0..3 {
            let (mut u, mut w) = table_fixture();
            if mode == 0 {
                w[4].text = "".into();
                w[4].status = WordStatus::Empty;
            } else if mode == 1 {
                w[4].status = WordStatus::Margin;
            } else {
                w[4].raw = "単位外\u{E68D}".into();
                w[4].text = "単位外".into();
            }
            let before = snap(&u, &w);
            restore_area_units(&mut u, &mut w);
            assert_eq!(snap(&u, &w), before);
        }
    }
    #[test]
    fn equal_numeric_tokens_follow_their_own_source_id_offsets() {
        for at in [0, 2] {
            let mut words = vec![w(0, "60", "60"), w(1, "/", "/"), w(2, "60", "60")];
            words[at].raw = "60\u{E68D}".into();
            let mut u = vec![Unit::Plain(vec![Line::text(
                "60 / 60".into(),
                vec![0, 1, 2],
            )])];
            restore_area_units(&mut u, &mut words);
            assert_eq!(text(&u), if at == 0 { "60㎡ / 60" } else { "60 / 60㎡" });
        }
    }
    #[test]
    fn non_area_parentheses_and_mismatched_source_ids_are_rejected() {
        let mut words = vec![w(0, "符号（\u{E68D}）", "符号（）")];
        let mut u = vec![Unit::Plain(vec![Line::text("符号（）".into(), vec![0])])];
        let old = snap(&u, &words);
        restore_area_units(&mut u, &mut words);
        assert_eq!(snap(&u, &words), old);
        let (mut u, mut words) = table_fixture();
        words[4].id = 999;
        let old = snap(&u, &words);
        restore_area_units(&mut u, &mut words);
        assert_eq!(snap(&u, &words), old);
    }
}
