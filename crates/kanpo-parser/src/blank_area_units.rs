//! Attach an original blank area-unit glyph only to a proven paired form label.
use crate::{
    grid::{find_grids, Grid},
    lines::PageRules,
    trace::{Line, Unit, Word, WordStatus},
};
pub(crate) fn restore(
    units: &mut [Unit],
    words: &mut [Word],
    rules: Option<&PageRules>,
    extent: f32,
) {
    if !words
        .iter()
        .any(|w| w.raw == "\u{E68D}" && w.text.is_empty() && w.status == WordStatus::Empty)
    {
        return;
    }
    let Some(rules) = rules else {
        return;
    };
    let grids = find_grids(rules, extent, extent);
    restore_in_grids(units, words, &grids);
}
fn each_lines(units: &[Unit], f: &mut impl FnMut(&[Line])) {
    for u in units {
        match u {
            Unit::Plain(p) => f(p),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            ..} => {
                f(preamble);
                for p in after.iter().chain(before).chain(outside) {
                    f(p);
                }
            }
        }
    }
}
fn area_label(w: &Word) -> bool {
    if w.status != WordStatus::Used || crate::normalize::word_text(&w.raw) != w.text {
        return false;
    }
    let Some(body) = w
        .text
        .strip_prefix('【')
        .and_then(|s| s.strip_suffix("面積】"))
    else {
        return false;
    };
    !body.is_empty() && body.chars().count() <= 8 && body.chars().all(char::is_alphabetic)
}
fn cell_key(w: &Word, grids: &[Grid]) -> Option<(usize, usize)> {
    if ![w.x0, w.x1, w.y0, w.y1].iter().all(|v| v.is_finite()) || w.x0 >= w.x1 || w.y0 >= w.y1 {
        return None;
    }
    let mut found = None;
    for (gi, g) in grids.iter().enumerate() {
        for (ci, c) in g.cells.iter().enumerate() {
            if g.xs[c.x0] <= w.x0 && w.x1 <= g.xs[c.x1] && g.ys[c.y0] <= w.y0 && w.y1 <= g.ys[c.y1]
            {
                if found.is_some() {
                    return None;
                }
                found = Some((gi, ci));
            }
        }
    }
    found
}
fn aligned(label: &Word, glyph: &Word) -> bool {
    let em = label.y1 - label.y0;
    em.is_finite()
        && em > 0.
        && label.x1 - label.x0 > em * 2.
        && (glyph.y0 - label.y0).abs() <= em * 0.05
        && ((glyph.y1 - glyph.y0) - em).abs() <= em * 0.05
        && (em * 0.8..=em * 1.2).contains(&(glyph.x1 - glyph.x0))
        && (em * 0.5..=em * 12.).contains(&(glyph.x0 - label.x1))
}
fn restore_lines(lines: &mut [Line], words: &mut [Word], grids: &[Grid], owners: &[usize]) {
    struct Anchor {
        line: usize,
        label: usize,
        glyph: usize,
        key: (usize, usize),
    }
    let mut anchors = Vec::new();
    for (li, line) in lines.iter().enumerate() {
        if line.table.is_some() || line.words.len() != 1 {
            continue;
        }
        let id = line.words[0];
        let Some(label) = words.get(id).filter(|w| w.id == id && area_label(w)) else {
            continue;
        };
        if owners.get(id) != Some(&1) || line.text.trim() != label.text {
            continue;
        }
        let Some(key) = cell_key(label, grids) else {
            continue;
        };
        let glyphs: Vec<_> = words
            .iter()
            .filter(|w| {
                w.id < words.len()
                    && std::ptr::eq(*w, &words[w.id])
                    && w.raw == "\u{E68D}"
                    && w.text.is_empty()
                    && w.status == WordStatus::Empty
                    && owners[w.id] == 0
                    && aligned(label, w)
                    && cell_key(w, grids) == Some(key)
            })
            .collect();
        if glyphs.len() != 1 {
            continue;
        }
        let glyph = glyphs[0];
        // One physical suffix must not be shared by overlapping labels or by a hidden value.
        if words
            .iter()
            .filter(|w| area_label(w) && aligned(w, glyph) && cell_key(w, grids) == Some(key))
            .count()
            != 1
        {
            continue;
        }
        let em = label.y1 - label.y0;
        if words.iter().any(|w| {
            w.id != label.id
                && w.id != glyph.id
                && !w.text.is_empty()
                && (w.y0 - label.y0).abs() <= em * 0.05
                && w.x0 >= label.x1
                && w.x1 <= glyph.x0
        }) {
            continue;
        }
        anchors.push(Anchor {
            line: li,
            label: id,
            glyph: glyph.id,
            key,
        });
    }
    let mut pairs = Vec::new();
    let mut participating = vec![0usize; anchors.len()];
    for (i, a) in anchors.iter().enumerate() {
        for (j, b) in anchors.iter().enumerate().skip(i + 1) {
            let (x, y) = (&words[a.label], &words[b.label]);
            let (g, h) = (&words[a.glyph], &words[b.glyph]);
            let em = x.y1 - x.y0;
            if b.line != a.line + 1
                || a.key != b.key
                || a.glyph == b.glyph
                || x.text == y.text
                || (x.x0 - y.x0).abs() > em * 0.05
                || (g.x0 - h.x0).abs() > em * 0.05
                || ((y.y1 - y.y0) - em).abs() > em * 0.05
                || !(em * 0.8..=em * 2.).contains(&(y.y0 - x.y0))
            {
                continue;
            }
            let grid = &grids[a.key.0];
            let c = &grid.cells[a.key.1];
            let count = words
                .iter()
                .filter(|w| {
                    w.raw.contains('\u{E68D}')
                        && w.x1 > grid.xs[c.x0]
                        && w.x0 < grid.xs[c.x1]
                        && w.y1 > grid.ys[c.y0]
                        && w.y0 < grid.ys[c.y1]
                })
                .map(|w| w.raw.matches('\u{E68D}').count())
                .sum::<usize>();
            if count != 2 {
                continue;
            }
            pairs.push((i, j));
            participating[i] += 1;
            participating[j] += 1;
        }
    }
    for (i, j) in pairs {
        if participating[i] != 1 || participating[j] != 1 {
            continue;
        }
        let (a, b) = (&anchors[i], &anchors[j]);
        if [a.glyph, b.glyph]
            .iter()
            .any(|&id| words[id].status != WordStatus::Empty || !words[id].text.is_empty())
        {
            continue;
        }
        for a in [a, b] {
            let line = &mut lines[a.line];
            line.text.insert(line.text.trim_end().len(), '㎡');
            line.words.push(a.glyph);
            words[a.glyph].text = "㎡".into();
            words[a.glyph].status = WordStatus::Used;
        }
    }
}
fn restore_in_grids(units: &mut [Unit], words: &mut [Word], grids: &[Grid]) {
    let mut owners = vec![0usize; words.len()];
    each_lines(units, &mut |ls| {
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
    });
    for u in units {
        match u {
            Unit::Plain(p) => restore_lines(p, words, grids, &owners),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            ..} => {
                restore_lines(preamble, words, grids, &owners);
                for p in after.iter_mut().chain(before).chain(outside) {
                    restore_lines(p, words, grids, &owners);
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::GridCell;
    fn fixture() -> (Vec<Unit>, Vec<Word>, Vec<Grid>) {
        let word = |id, text: &str, raw: &str, x0, x1, y0| Word {
            source_box: [x0, y0, x1, y0 + 8.],
            id,
            text: text.into(),
            raw: raw.into(),
            x0,
            x1,
            y0,
            y1: y0 + 8.,
            status: if text.is_empty() {
                WordStatus::Empty
            } else {
                WordStatus::Used
            },
        };
        let w = vec![
            word(0, "【延床面積】", "【延床面積】", 16., 64., 10.),
            word(1, "【総専有床面積】", "【総専有床面積】", 16., 80., 22.),
            word(2, "", "\u{E68D}", 120., 128., 10.),
            word(3, "", "\u{E68D}", 120., 128., 22.),
        ];
        let u = vec![Unit::Plain(vec![
            Line::text("【延床面積】".into(), vec![0]),
            Line::text("【総専有床面積】".into(), vec![1]),
        ])];
        let g = vec![Grid {
            xs: vec![0., 220.],
            ys: vec![0., 50.],
            cells: vec![GridCell {
                x0: 0,
                x1: 1,
                y0: 0,
                y1: 1,
            }],
        }];
        (u, w, g)
    }
    fn snap(u: &[Unit], w: &[Word]) -> String {
        format!(
            "{}{}",
            u.iter()
                .map(|u| match u {
                    Unit::Plain(ls) => serde_json::to_string(ls).unwrap(),
                    _ => u.text(),
                })
                .collect::<Vec<_>>()
                .join("|"),
            serde_json::to_string(w).unwrap()
        )
    }
    #[test]
    fn paired_labels_restore_original_ids_once_without_changing_layout() {
        let (mut u, mut w, g) = fixture();
        let old = w.clone();
        restore_in_grids(&mut u, &mut w, &g);
        let Unit::Plain(ls) = &u[0] else { panic!() };
        assert_eq!(ls[0].text, "【延床面積】㎡");
        assert_eq!(ls[1].text, "【総専有床面積】㎡");
        assert_eq!(ls[0].words, vec![0, 2]);
        assert_eq!(ls[1].words, vec![1, 3]);
        for i in 0..2 {
            assert_eq!(
                serde_json::to_string(&w[i]).unwrap(),
                serde_json::to_string(&old[i]).unwrap()
            );
        }
        for i in 2..4 {
            assert_eq!(w[i].text, "㎡");
            assert_eq!(w[i].status, WordStatus::Used);
            assert_eq!(
                (w[i].id, &w[i].raw, w[i].x0, w[i].x1, w[i].y0, w[i].y1),
                (
                    old[i].id,
                    &old[i].raw,
                    old[i].x0,
                    old[i].x1,
                    old[i].y0,
                    old[i].y1
                )
            );
        }
        let once = snap(&u, &w);
        restore_in_grids(&mut u, &mut w, &g);
        assert_eq!(snap(&u, &w), once);
    }
    #[test]
    fn preserves_original_outer_whitespace() {
        let (mut u, mut w, g) = fixture();
        let Unit::Plain(ls) = &mut u[0] else { panic!() };
        ls[0].text = "\t 【延床面積】  ".into();
        restore_in_grids(&mut u, &mut w, &g);
        let Unit::Plain(ls) = &u[0] else { panic!() };
        assert_eq!(ls[0].text, "\t 【延床面積】㎡  ");
    }
    #[test]
    fn rejects_missing_ambiguous_and_split_physical_cells() {
        for mode in 0..3 {
            let (mut u, mut w, mut g) = fixture();
            match mode {
                0 => g.clear(),
                1 => g.push(g[0].clone()),
                _ => {
                    g[0].ys = vec![0., 20., 50.];
                    g[0].cells = vec![
                        GridCell {
                            x0: 0,
                            x1: 1,
                            y0: 0,
                            y1: 1,
                        },
                        GridCell {
                            x0: 0,
                            x1: 1,
                            y0: 1,
                            y1: 2,
                        },
                    ];
                }
            }
            let old = snap(&u, &w);
            restore_in_grids(&mut u, &mut w, &g);
            assert_eq!(snap(&u, &w), old);
        }
    }
    #[test]
    fn rejects_non_area_labels_and_unaligned_or_different_symbols() {
        for mode in 0..8 {
            let (mut u, mut w, g) = fixture();
            match mode {
                0 => {
                    w[0].text = "【評価額】".into();
                    w[0].raw = w[0].text.clone();
                    let Unit::Plain(ls) = &mut u[0] else { panic!() };
                    ls[0].text = w[0].text.clone();
                }
                1 => {
                    w[3].x0 += 8.;
                    w[3].x1 += 8.;
                }
                2 => {
                    w[3].y0 += 4.;
                    w[3].y1 += 4.;
                }
                3 => w[3].raw = "\u{E68E}".into(),
                4 => w[3].status = WordStatus::Margin,
                5 => w[0].id = 999,
                6 => {
                    w[1].text = w[0].text.clone();
                    w[1].raw = w[0].raw.clone();
                    let Unit::Plain(ls) = &mut u[0] else { panic!() };
                    ls[1].text = w[1].text.clone();
                }
                _ => {
                    w[3].x0 = 225.;
                    w[3].x1 = 233.;
                }
            }
            let old = snap(&u, &w);
            restore_in_grids(&mut u, &mut w, &g);
            assert_eq!(snap(&u, &w), old);
        }
    }
    #[test]
    fn rejects_third_glyph_existing_owner_and_intervening_number() {
        for mode in 0..3 {
            let (mut u, mut w, g) = fixture();
            match mode {
                0 => {
                    let mut q = w[3].clone();
                    q.id = 4;
                    q.x0 = 180.;
                    q.x1 = 188.;
                    w.push(q);
                }
                1 => {
                    let Unit::Plain(ls) = &mut u[0] else { panic!() };
                    ls.push(Line::text("".into(), vec![2]));
                }
                _ => {
                    let mut q = w[0].clone();
                    q.id = 4;
                    q.text = "12".into();
                    q.raw = "12".into();
                    q.x0 = 90.;
                    q.x1 = 98.;
                    w.push(q);
                }
            }
            let old = snap(&u, &w);
            restore_in_grids(&mut u, &mut w, &g);
            assert_eq!(snap(&u, &w), old);
        }
    }
    #[test]
    fn rejects_cross_article_pairs() {
        let (mut u, mut w, g) = fixture();
        let Unit::Plain(mut ls) = u.remove(0) else {
            panic!()
        };
        let b = ls.pop().unwrap();
        u = vec![Unit::Plain(ls), Unit::Plain(vec![b])];
        let old = snap(&u, &w);
        restore_in_grids(&mut u, &mut w, &g);
        assert_eq!(snap(&u, &w), old);
    }
}
