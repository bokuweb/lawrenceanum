//! Restore CFF-verified curly quotes only within uniquely owned, unchanged mixed words.
use crate::trace::{Line, Unit, Word, WordStatus};
use std::collections::{BTreeMap, BTreeSet};
fn marker(c: char) -> Option<&'static str> {
    match c {
        '\u{E20A}' => Some("“"),
        '\u{E209}' => Some("”"),
        _ => None,
    }
}
fn is_private_use(c: char) -> bool {
    crate::normalize::is_private_use(c)
        || matches!(c as u32, 0xF0000..=0xFFFFD | 0x100000..=0x10FFFD)
}
fn mapped(w: &Word) -> Option<String> {
    // Unknown glyphs outside this verified pair must not produce a partly restored word.
    if w.raw.chars().any(|c| {
        if !is_private_use(c) || marker(c).is_some() {
            return false;
        }
        // Supplementary Identity CID fallback can look like ordinary text without glyph proof.
        // This module has verified only the two BMP quote glyphs, so leave that mixed word alone.
        if c as u32 >= 0xF0000 {
            return true;
        }
        let normalized = crate::normalize::word_text(&c.to_string());
        normalized.is_empty() || normalized.chars().any(is_private_use)
    }) {
        return None;
    }
    let (mut old, mut new, mut part) = (String::new(), String::new(), String::new());
    for c in w.raw.chars() {
        if let Some(m) = marker(c) {
            let n = crate::normalize::word_text(&part);
            old.push_str(&n);
            new.push_str(&n);
            new.push_str(m);
            part.clear();
        } else {
            part.push(c);
        }
    }
    let n = crate::normalize::word_text(&part);
    old.push_str(&n);
    new.push_str(&n);
    (!w.text.is_empty() && old == w.text && new != w.text).then_some(new)
}
fn all_lines(units: &[Unit]) -> Vec<&Line> {
    let mut out = vec![];
    for u in units {
        match u {
            Unit::Plain(p) => out.extend(p),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            } => {
                out.extend(preamble);
                for p in after.iter().chain(before).chain(outside) {
                    out.extend(p);
                }
            }
        }
    }
    out
}
fn all_lines_mut(units: &mut [Unit]) -> Vec<&mut Line> {
    let mut out = vec![];
    for u in units {
        match u {
            Unit::Plain(p) => out.extend(p),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            } => {
                out.extend(preamble);
                for p in after.iter_mut().chain(before).chain(outside) {
                    out.extend(p);
                }
            }
        }
    }
    out
}
#[derive(Clone)]
struct Owner {
    line: usize,
    cell: Option<usize>,
    text: String,
    editable: bool,
}
struct Edit {
    owner: usize,
    start: usize,
    end: usize,
    text: String,
    id: usize,
}
fn unique(text: &str, part: &str) -> Option<(usize, usize)> {
    let mut matches = text.match_indices(part);
    let (s, _) = matches.next()?;
    matches.next().is_none().then_some((s, s + part.len()))
}
pub(crate) fn restore(units: &mut [Unit], words: &mut [Word]) {
    let lines = all_lines(units);
    let mut owners = vec![];
    let mut attached: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (li, l) in lines.iter().enumerate() {
        if !l.words.is_empty() {
            let oi = owners.len();
            for &id in &l.words {
                attached.entry(id).or_default().push(oi);
            }
            owners.push(Owner {
                line: li,
                cell: None,
                text: l.text.clone(),
                editable: l.table.is_none(),
            });
        }
        if let Some(t) = &l.table {
            for (ci, c) in t.cells.iter().enumerate() {
                let oi = owners.len();
                for &id in &c.words {
                    attached.entry(id).or_default().push(oi);
                }
                owners.push(Owner {
                    line: li,
                    cell: Some(ci),
                    text: c.text.clone(),
                    editable: true,
                });
            }
        }
    }
    let mut edits = vec![];
    for w in words
        .iter()
        .filter(|w| w.status == WordStatus::Used && w.raw.chars().any(|c| marker(c).is_some()))
    {
        let Some(new) = mapped(w) else { continue };
        let Some(os) = attached.get(&w.id) else {
            continue;
        };
        if os.len() != 1 {
            continue;
        };
        let oi = os[0];
        let o = &owners[oi];
        if !o.editable {
            continue;
        };
        if let Some((start, end)) = unique(&o.text, &w.text) {
            edits.push(Edit {
                owner: oi,
                start,
                end,
                text: new,
                id: w.id,
            });
        }
    }
    let mut conflicts = BTreeSet::new();
    for (i, a) in edits.iter().enumerate() {
        for b in &edits[i + 1..] {
            if a.owner == b.owner && a.start < b.end && b.start < a.end {
                conflicts.insert(a.owner);
            }
        }
    }
    drop(lines);
    let mut lines = all_lines_mut(units);
    let mut by_owner: BTreeMap<usize, Vec<Edit>> = BTreeMap::new();
    for e in edits {
        if !conflicts.contains(&e.owner) {
            by_owner.entry(e.owner).or_default().push(e);
        }
    }
    let mut tables = BTreeSet::new();
    for (oi, mut es) in by_owner {
        es.sort_by_key(|e| std::cmp::Reverse(e.start));
        let o = &owners[oi];
        let mut text = o.text.clone();
        for e in es {
            text.replace_range(e.start..e.end, &e.text);
            words[e.id].text = e.text;
        }
        let l = &mut lines[o.line];
        if let Some(ci) = o.cell {
            l.table.as_mut().unwrap().cells[ci].text = text;
            tables.insert(o.line);
        } else {
            l.text = text;
        }
    }
    for li in tables {
        lines[li].text = lines[li].table.as_ref().unwrap().render();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn w(id: usize, raw: &str, text: &str) -> Word {
        Word {
            id,
            raw: raw.into(),
            text: text.into(),
            x0: 0.,
            y0: 0.,
            x1: 8.,
            y1: 8.,
            status: WordStatus::Used,
        }
    }
    fn run(text: &str, words: &mut [Word]) -> Vec<Unit> {
        let mut u = vec![Unit::Plain(vec![Line::text(
            text.into(),
            words.iter().map(|w| w.id).collect(),
        )])];
        restore(&mut u, words);
        u
    }
    fn text(u: &[Unit]) -> String {
        all_lines(u)[0].text.clone()
    }
    #[test]
    fn both_inline_quotes_and_other_text_stay_in_place() {
        let mut ws = vec![
            w(0, "\u{E20A}Due", "Due"),
            w(1, "reasons\u{E209}", "reasons"),
        ];
        let mut u = run("ⅲDue technical reasons 1/2", &mut ws);
        assert_eq!(text(&u), "ⅲ“Due technical reasons” 1/2");
        let before = (text(&u), serde_json::to_string(&ws).unwrap());
        restore(&mut u, &mut ws);
        assert_eq!((text(&u), serde_json::to_string(&ws).unwrap()), before);
    }
    #[test]
    fn repeated_or_overlapping_substrings_are_ambiguous() {
        let mut ws = vec![w(0, "\u{E20A}Due", "Due")];
        assert_eq!(text(&run("Due Due", &mut ws)), "Due Due");
        let mut ws = vec![
            w(0, "\u{E20A}reason", "reason"),
            w(1, "reasons\u{E209}", "reasons"),
        ];
        assert_eq!(text(&run("reasons", &mut ws)), "reasons");
    }
    #[test]
    fn duplicate_owner_and_empty_words_stay_unchanged() {
        let mut ws = vec![w(0, "\u{E20A}Due", "Due")];
        let mut u = vec![Unit::Plain(vec![
            Line::text("Due".into(), vec![0]),
            Line::text("Due".into(), vec![0]),
        ])];
        restore(&mut u, &mut ws);
        assert_eq!(ws[0].text, "Due");
        assert_eq!(mapped(&w(0, "\u{E20A}", "")), None);
    }
    #[test]
    fn repaired_fraction_wrap_and_units_are_not_rebuilt() {
        for (raw, t) in [
            ("\u{E20A}12", "1/2"),
            ("\u{E20A}積立資産", "システム機器更新等経費積立資産"),
            ("\u{E20A}m2", "㎡"),
        ] {
            let mut ws = vec![w(0, raw, t)];
            assert_eq!(text(&run(t, &mut ws)), t);
        }
        let mut ws = vec![w(0, "\u{E20A}a\u{E952}", "a")];
        assert_eq!(text(&run("a", &mut ws)), "a");
    }
    #[test]
    fn cell_values_and_ids_survive_local_insertion() {
        use crate::trace::{Table, TableCell};
        let c = |col, text: &str, words| TableCell {
            row: 0,
            col,
            rowspan: 1,
            colspan: 1,
            x0: 0.,
            y0: 0.,
            x1: 100.,
            y1: 10.,
            text: text.into(),
            words,
        };
        let t = Table {
            x0: 0.,
            y0: 0.,
            x1: 200.,
            y1: 10.,
            rows: 1,
            cols: 2,
            cells: vec![c(0, "Due", vec![0]), c(1, "2,000", vec![1])],
        };
        let mut u = vec![Unit::Plain(vec![Line {
            text: t.render(),
            words: vec![],
            table: Some(t),
        }])];
        let mut ws = vec![w(0, "\u{E20A}Due", "Due"), w(1, "2,000", "2,000")];
        restore(&mut u, &mut ws);
        let l = all_lines(&u)[0];
        assert_eq!(l.table.as_ref().unwrap().cells[0].text, "“Due");
        assert_eq!(l.table.as_ref().unwrap().cells[1].text, "2,000");
        assert_eq!(l.table.as_ref().unwrap().cells[0].words, vec![0]);
    }
    #[test]
    fn shinkyu_parts_preserve_their_separate_owners() {
        let mut units = vec![Unit::Shinkyu {
            preamble: vec![],
            after: vec![vec![Line::text("Due".into(), vec![0])]],
            before: vec![vec![Line::text("reasons".into(), vec![1])]],
            outside: vec![],
        }];
        let mut ws = vec![
            w(0, "\u{E20A}Due", "Due"),
            w(1, "reasons\u{E209}", "reasons"),
        ];
        restore(&mut units, &mut ws);
        let lines = all_lines(&units);
        assert_eq!(
            lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(),
            vec!["“Due", "reasons”"]
        );
        assert_eq!(
            lines.iter().map(|l| l.words.clone()).collect::<Vec<_>>(),
            vec![vec![0], vec![1]]
        );
    }
    #[test]
    fn unknown_supplementary_private_use_and_identity_cids_are_not_partly_restored() {
        for (raw, text) in [
            ("\u{E20A}Due\u{FE952}", "Due"),
            ("\u{E20A}Due\u{100123}", "Due\u{100123}"),
        ] {
            assert_eq!(mapped(&w(0, raw, text)), None);
            let mut ws = vec![w(0, raw, text)];
            assert_eq!(super::tests::text(&run(text, &mut ws)), text);
        }
    }
    #[test]
    fn supplemental_identity_cid_fallback_to_normal_unicode_still_blocks_quotes() {
        // A normal-looking fallback from an unknown Identity CID is not proof of its glyph.
        assert_eq!(crate::normalize::word_text("\u{F4E00}"), "一");
        assert_eq!(mapped(&w(0, "\u{E20A}Due\u{F4E00}", "Due一")), None);
    }
}
