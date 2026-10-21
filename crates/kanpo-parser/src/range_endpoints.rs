//! Return verified overlaid ordinal halves only at an already-owned range end.
use crate::trace::{Line, Unit, Word, WordStatus};
use std::collections::{BTreeMap, BTreeSet};
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn pair(left: char, right: &str) -> Option<(&'static str, &'static str, u32)> {
    let (l, t) = match left {
        '\u{eac1}' => ("（１", 10),
        '\u{eac2}' => ("（２", 20),
        _ => return None,
    };
    let (r, u) = match right {
        "\u{eacb}" => ("１）", 1),
        "\u{ead4}" => ("０）", 0),
        "\u{ead0}" => ("６）", 6),
        "\u{ead1}" => ("７）", 7),
        _ => return None,
    };
    Some((l, r, t + u))
}
fn start_number(s: &str) -> Option<u32> {
    let s = s.strip_prefix('［').unwrap_or(s);
    let s = s.strip_suffix(['〜', '～'])?;
    let s = s.strip_prefix('（').or_else(|| s.strip_prefix('('))?;
    let s = s.strip_suffix('）').or_else(|| s.strip_suffix(')'))?;
    if !(1..=2).contains(&s.chars().count()) {
        return None;
    }
    s.chars()
        .try_fold(0, |n, c| {
            let d = c
                .to_digit(10)
                .or_else(|| (c as u32).checked_sub('０' as u32).filter(|&n| n < 10))?;
            Some(n * 10 + d)
        })
        .filter(|&n| n > 0)
}
fn lines(units: &[Unit]) -> Vec<&Line> {
    let mut out = vec![];
    for u in units {
        match u {
            Unit::Plain(ls) => out.extend(ls),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            } => {
                out.extend(preamble);
                for ls in after.iter().chain(before).chain(outside) {
                    out.extend(ls);
                }
            }
        }
    }
    out
}
fn lines_mut(units: &mut [Unit]) -> Vec<&mut Line> {
    let mut out = vec![];
    for u in units {
        match u {
            Unit::Plain(ls) => out.extend(ls),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            } => {
                out.extend(preamble);
                for ls in after.iter_mut().chain(before).chain(outside) {
                    out.extend(ls);
                }
            }
        }
    }
    out
}
struct Owner {
    line: usize,
    cell: Option<usize>,
    text: String,
    ids: Vec<usize>,
}
struct Edit {
    owner: usize,
    left: usize,
    right: usize,
    at: usize,
    position: usize,
    l: &'static str,
    r: &'static str,
}
pub(crate) fn restore(units: &mut [Unit], words: &mut [Word]) {
    if !words.iter().any(|w| {
        w.status == WordStatus::Empty
            && matches!(
                w.raw.as_str(),
                "\u{eacb}" | "\u{ead4}" | "\u{ead0}" | "\u{ead1}"
            )
    }) {
        return;
    }
    let ls = lines(units);
    let mut owners = vec![];
    let mut attached = vec![vec![]; words.len()];
    for (li, l) in ls.iter().enumerate() {
        let mut add = |cell, text: &String, ids: &Vec<usize>| {
            let oi = owners.len();
            for &id in ids {
                if let Some(a) = attached.get_mut(id) {
                    a.push(oi);
                }
            }
            owners.push(Owner {
                line: li,
                cell,
                text: text.clone(),
                ids: ids.clone(),
            });
        };
        if l.table.is_none() {
            add(None, &l.text, &l.words);
        } else if l.words.is_empty() {
            for (ci, c) in l.table.as_ref().unwrap().cells.iter().enumerate() {
                add(Some(ci), &c.text, &c.words);
            }
        }
    }
    let mut edits = vec![];
    for (i, a) in words.iter().enumerate() {
        if a.id != i || a.status != WordStatus::Used || attached[i].len() != 1 {
            continue;
        }
        let Some((last, left)) = a.raw.char_indices().next_back() else {
            continue;
        };
        if !matches!(left, '\u{eac1}' | '\u{eac2}') {
            continue;
        }
        let raw_prefix = &a.raw[..last];
        let prefix = crate::normalize::word_text(raw_prefix);
        if prefix != a.text
            || raw_prefix.chars().any(|c| {
                crate::normalize::is_private_use(c)
                    && crate::normalize::word_text(&c.to_string()).is_empty()
            })
        {
            continue;
        }
        let Some(start) = start_number(&compact(&prefix)) else {
            continue;
        };
        let bs: Vec<_> = words
            .iter()
            .enumerate()
            .filter_map(|(j, b)| {
                let (l, r, end) = pair(left, &b.raw)?;
                let em = b.x1 - b.x0;
                (i != j
                    && b.id == j
                    && b.status == WordStatus::Empty
                    && b.text.is_empty()
                    && attached[j].is_empty()
                    && end >= start
                    && (3.0..=20.0).contains(&em)
                    && (em * 0.9..=em * 1.2).contains(&(b.y1 - b.y0))
                    && a.x1 - a.x0 >= em * 2.5
                    && (a.x1 - b.x1).abs() <= em * 0.05
                    && (a.y0 - b.y0).abs() <= em * 0.05
                    && (a.y1 - b.y1).abs() <= em * 0.05)
                    .then_some((j, l, r))
            })
            .collect();
        let [(j, l, r)] = bs.as_slice() else {
            continue;
        };
        let oi = attached[i][0];
        let o = &owners[oi];
        if o.ids.iter().any(|&id| {
            words
                .get(id)
                .is_none_or(|w| w.id != id || w.status != WordStatus::Used || w.text.is_empty())
                || attached[id].len() != 1
        }) {
            continue;
        }
        let joined = o
            .ids
            .iter()
            .map(|&id| compact(&words[id].text))
            .collect::<String>();
        if joined != compact(&o.text) {
            continue;
        }
        let Some(pos) = o.ids.iter().position(|&id| id == i) else {
            continue;
        };
        let chars = o.ids[..=pos]
            .iter()
            .map(|&id| compact(&words[id].text).chars().count())
            .sum::<usize>();
        let Some(at) = chars
            .checked_sub(1)
            .and_then(|n| {
                o.text
                    .char_indices()
                    .filter(|(_, c)| !c.is_whitespace())
                    .nth(n)
            })
            .map(|(at, c)| at + c.len_utf8())
        else {
            continue;
        };
        edits.push(Edit {
            owner: oi,
            left: i,
            right: *j,
            at,
            position: pos,
            l,
            r,
        });
    }
    let mut bad = BTreeSet::new();
    for a in &edits {
        if edits
            .iter()
            .filter(|b| a.right == b.right || a.left == b.left)
            .count()
            != 1
        {
            bad.insert(a.owner);
        }
    }
    let mut groups: BTreeMap<usize, Vec<Edit>> = BTreeMap::new();
    for e in edits {
        if !bad.contains(&e.owner) {
            groups.entry(e.owner).or_default().push(e);
        }
    }
    drop(ls);
    let mut ls = lines_mut(units);
    let mut changed_tables = BTreeSet::new();
    for (oi, mut es) in groups {
        es.sort_by_key(|e| std::cmp::Reverse(e.at));
        let o = &owners[oi];
        let mut text = o.text.clone();
        let mut ids = o.ids.clone();
        for e in es {
            text.insert_str(e.at, &format!("{}{}", e.l, e.r));
            ids.insert(e.position + 1, e.right);
            words[e.left].text.push_str(e.l);
            words[e.right].text = e.r.into();
            words[e.right].status = WordStatus::Used;
        }
        let line = &mut ls[o.line];
        if let Some(ci) = o.cell {
            let c = &mut line.table.as_mut().unwrap().cells[ci];
            c.text = text;
            c.words = ids;
            changed_tables.insert(o.line);
        } else {
            line.text = text;
            line.words = ids;
        }
    }
    for li in changed_tables {
        ls[li].text = ls[li].table.as_ref().unwrap().render();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::{Table, TableCell};
    fn fixture(left: char, right: char) -> (Vec<Unit>, Vec<Word>) {
        let words = vec![
            Word {
                id: 0,
                x0: 20.,
                y0: 30.,
                x1: 44.,
                y1: 38.184,
                raw: format!("\u{e7ba}〜{left}"),
                text: "（９）〜".into(),
                status: WordStatus::Used,
            },
            Word {
                id: 1,
                x0: 36.,
                y0: 30.,
                x1: 44.,
                y1: 38.184,
                raw: right.to_string(),
                text: "".into(),
                status: WordStatus::Empty,
            },
            Word {
                id: 2,
                x0: 52.,
                y0: 30.,
                x1: 76.,
                y1: 38.184,
                raw: "［略］".into(),
                text: "［略］".into(),
                status: WordStatus::Used,
            },
        ];
        (
            vec![Unit::Plain(vec![Line::text(
                "（９）〜　［略］".into(),
                vec![0, 2],
            )])],
            words,
        )
    }
    fn state(u: &[Unit], w: &[Word]) -> String {
        let ls = lines(u);
        serde_json::to_string(&(ls, w)).unwrap()
    }
    fn unchanged(mut u: Vec<Unit>, mut w: Vec<Word>) {
        let before = state(&u, &w);
        restore(&mut u, &mut w);
        assert_eq!(state(&u, &w), before);
    }
    #[test]
    fn four_original_range_pairs_restore_only_source_halves() {
        for (l, r, expected) in [
            ('\u{eac2}', '\u{eacb}', "（２１）"),
            ('\u{eac2}', '\u{ead4}', "（２０）"),
            ('\u{eac1}', '\u{ead1}', "（１７）"),
            ('\u{eac1}', '\u{ead0}', "（１６）"),
        ] {
            let (mut u, mut w) = fixture(l, r);
            restore(&mut u, &mut w);
            assert_eq!(lines(&u)[0].text, format!("（９）〜{expected}　［略］"));
            assert_eq!(lines(&u)[0].words, vec![0, 1, 2]);
            assert_eq!(w[1].status, WordStatus::Used);
            let before = state(&u, &w);
            restore(&mut u, &mut w);
            assert_eq!(state(&u, &w), before);
        }
    }
    #[test]
    fn source_halves_compose_independently_of_range_start_value() {
        for l in ['\u{eac1}', '\u{eac2}'] {
            for r in ['\u{eacb}', '\u{ead4}', '\u{ead0}', '\u{ead1}'] {
                let (mut u, mut w) = fixture(l, r);
                restore(&mut u, &mut w);
                let (p, q, _) = pair(l, &r.to_string()).unwrap();
                assert_eq!(lines(&u)[0].text, format!("（９）〜{p}{q}　［略］"));
            }
        }
    }
    #[test]
    fn the_same_original_cell_keeps_all_row_column_geometry() {
        let (mut u, mut w) = fixture('\u{eac2}', '\u{eacb}');
        let t = Table {
            x0: 12.,
            y0: 20.,
            x1: 80.,
            y1: 50.,
            rows: 1,
            cols: 1,
            cells: vec![TableCell {
                row: 0,
                col: 0,
                rowspan: 1,
                colspan: 1,
                x0: 12.,
                y0: 20.,
                x1: 80.,
                y1: 50.,
                text: "（９）〜　［略］".into(),
                words: vec![0, 2],
            }],
        };
        u = vec![Unit::Plain(vec![Line {
            text: t.render(),
            table: Some(t.clone()),
            words: vec![],
        }])];
        restore(&mut u, &mut w);
        let l = lines(&u)[0];
        let c = &l.table.as_ref().unwrap().cells[0];
        assert_eq!(c.words, vec![0, 1, 2]);
        assert_eq!(c.text, "（９）〜（２１）　［略］");
        let mut expected = t;
        expected.cells[0].text = c.text.clone();
        expected.cells[0].words = c.words.clone();
        assert_eq!(l.table.as_ref(), Some(&expected));
        assert_eq!(l.text, expected.render());
    }
    #[test]
    fn actual_shinkyu_unit_keeps_its_role_and_part() {
        let (u, mut w) = fixture('\u{eac1}', '\u{ead0}');
        let Unit::Plain(ls) = u.into_iter().next().unwrap() else {
            panic!()
        };
        let mut u = vec![Unit::Shinkyu {
            preamble: vec![],
            after: vec![ls],
            before: vec![],
            outside: vec![],
        }];
        restore(&mut u, &mut w);
        let Unit::Shinkyu { after, before, .. } = &u[0] else {
            panic!()
        };
        assert_eq!(after[0][0].words, vec![0, 1, 2]);
        assert!(before.is_empty());
    }
    #[test]
    fn standalone_prefix_component_never_creates_a_line() {
        let (u, mut w) = fixture('\u{eac2}', '\u{eacb}');
        w[0].raw = "\u{eac2}".into();
        w[0].text = "".into();
        w[0].status = WordStatus::Empty;
        unchanged(u, w);
    }
    #[test]
    fn ordinary_prose_before_the_component_is_not_a_range() {
        let (u, mut w) = fixture('\u{eac2}', '\u{eacb}');
        w[0].raw = "次の\u{eac2}".into();
        w[0].text = "次の".into();
        unchanged(u, w);
    }
    #[test]
    fn missing_range_separator_is_rejected() {
        let (u, mut w) = fixture('\u{eac2}', '\u{eacb}');
        w[0].raw = "\u{e7ba}\u{eac2}".into();
        w[0].text = "（９）".into();
        unchanged(u, w);
    }
    #[test]
    fn reversed_range_is_not_guessed() {
        let (u, mut w) = fixture('\u{eac1}', '\u{ead0}');
        w[0].raw = "（２３）〜\u{eac1}".into();
        w[0].text = "（２３）〜".into();
        unchanged(u, w);
    }
    #[test]
    fn unknown_component_stays_empty() {
        for r in ['\u{eace}', '\u{ead2}', '\u{ead3}'] {
            let (u, w) = fixture('\u{eac1}', r);
            unchanged(u, w);
        }
    }
    #[test]
    fn unknown_left_component_stays_unmapped() {
        let (u, w) = fixture('\u{eac3}', '\u{eacb}');
        unchanged(u, w);
    }
    #[test]
    fn unknown_glyph_hidden_in_prefix_is_not_ignored() {
        let (u, mut w) = fixture('\u{eac2}', '\u{eacb}');
        w[0].raw = "\u{e7ba}〜\u{eaf0}\u{eac2}".into();
        unchanged(u, w);
    }
    #[test]
    fn displaced_or_rotated_components_are_not_paired() {
        for mode in 0..3 {
            let (u, mut w) = fixture('\u{eac2}', '\u{eacb}');
            match mode {
                0 => {
                    w[1].x0 += 1.;
                    w[1].x1 += 1.;
                }
                1 => {
                    w[1].y0 += 1.;
                    w[1].y1 += 1.;
                }
                _ => w[1].y1 += 8.,
            }
            unchanged(u, w);
        }
    }
    #[test]
    fn different_em_size_is_not_paired() {
        let (u, mut w) = fixture('\u{eac2}', '\u{eacb}');
        w[1].x0 += 2.;
        unchanged(u, w);
    }
    #[test]
    fn empty_word_must_be_unowned() {
        let (mut u, w) = fixture('\u{eac2}', '\u{eacb}');
        if let Unit::Plain(ls) = &mut u[0] {
            ls[0].words.push(1);
        }
        unchanged(u, w);
    }
    #[test]
    fn duplicate_suffix_sources_are_ambiguous() {
        let (u, mut w) = fixture('\u{eac2}', '\u{eacb}');
        let mut d = w[1].clone();
        d.id = w.len();
        w.push(d);
        unchanged(u, w);
    }
    #[test]
    fn duplicate_left_owner_is_rejected() {
        let (mut u, w) = fixture('\u{eac2}', '\u{eacb}');
        if let Unit::Plain(ls) = &mut u[0] {
            ls.push(ls[0].clone());
        }
        unchanged(u, w);
    }
    #[test]
    fn existing_owner_text_must_be_source_exact() {
        let (mut u, w) = fixture('\u{eac2}', '\u{eacb}');
        if let Unit::Plain(ls) = &mut u[0] {
            ls[0].text.push_str("注");
        }
        unchanged(u, w);
    }
    #[test]
    fn non_used_anchor_is_rejected() {
        for status in [WordStatus::Noise, WordStatus::Margin, WordStatus::Empty] {
            let (u, mut w) = fixture('\u{eac2}', '\u{eacb}');
            w[0].status = status;
            unchanged(u, w);
        }
    }
    #[test]
    fn non_empty_suffix_is_not_overwritten() {
        for status in [WordStatus::Noise, WordStatus::Margin, WordStatus::Used] {
            let (u, mut w) = fixture('\u{eac2}', '\u{eacb}');
            w[1].status = status;
            unchanged(u, w);
        }
    }
    #[test]
    fn unrelated_owner_word_must_be_used() {
        let (u, mut w) = fixture('\u{eac2}', '\u{eacb}');
        w[2].status = WordStatus::Margin;
        unchanged(u, w);
    }
}
