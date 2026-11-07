//! Source gaps in structural prose, certified by original Word rectangles.
use kanpo_parser::{
    trace::{Word, WordStatus},
    Analysis,
};

#[derive(Default)]
pub(crate) struct Spacing {
    gaps: Vec<Gap>,
    spans: Vec<Span>,
}
struct Gap {
    prefix: String,
    label: String,
    offset: usize,
    prose_only: bool,
    spaces: Option<usize>,
}
struct Span {
    prefix: String,
    start: usize,
    end: usize,
    prose_only: bool,
}
const N: &str = "〇零一二三四五六七八九十百千万億0123456789０１２３４５６７８９";
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn label(text: &str) -> Option<&str> {
    let rest = text.strip_prefix('第')?;
    let n = rest
        .chars()
        .take_while(|c| N.contains(*c))
        .map(char::len_utf8)
        .sum::<usize>();
    if n == 0 || !rest[n..].starts_with('条') {
        return None;
    }
    let mut end = '第'.len_utf8() + n + '条'.len_utf8();
    while let Some(tail) = text[end..].strip_prefix('の') {
        let n = tail
            .chars()
            .take_while(|c| N.contains(*c))
            .map(char::len_utf8)
            .sum::<usize>();
        if n == 0 {
            break;
        }
        end += 'の'.len_utf8() + n;
    }
    Some(&text[..end])
}
fn prose_label(text: &str) -> Option<&str> {
    let (start, rest) = text
        .strip_prefix('第')
        .map_or((0, text), |s| ('第'.len_utf8(), s));
    let n = rest
        .chars()
        .take_while(|c| N.contains(*c))
        .map(char::len_utf8)
        .sum::<usize>();
    if n == 0 {
        return None;
    }
    let mut end = start + n;
    if start > 0 {
        if let Some(c) = text[end..]
            .chars()
            .next()
            .filter(|c| "編章節款目部項号".contains(*c))
        {
            end += c.len_utf8();
        }
        while let Some(tail) = text[end..].strip_prefix('の') {
            let n = tail
                .chars()
                .take_while(|c| N.contains(*c))
                .map(char::len_utf8)
                .sum::<usize>();
            if n == 0 {
                break;
            }
            end += 'の'.len_utf8() + n;
        }
    }
    Some(&text[..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn one_character_names_can_follow_a_directional_office_word() {
        for horizontal in [false,true] {
            let mut w=words(1.,0.,0.,8.,horizontal);
            w[1].text="任".into();w[1].raw="任".into();
            let axis=if horizontal {0}else{1};
            w[1].source_box[axis+2]=w[1].source_box[axis]+8.;
            assert_eq!(measured(&w[0],&w[1]),Some(1));
            w[0].source_box[axis+2]=w[0].source_box[axis]+8.;
            assert_eq!(measured(&w[0],&w[1]),None,"two square glyphs do not prove the writing axis");
        }
    }
    fn words(scale: f32, dx: f32, dy: f32, gap: f32, horizontal: bool) -> Vec<Word> {
        [
            ("第一条", 0., 24.),
            ("本文を定める。", 24. + gap, 80. + gap),
        ]
        .into_iter()
        .enumerate()
        .map(|(id, (text, a, b))| {
            let rect = if horizontal {
                [dx + a * scale, dy, dx + b * scale, dy + 8. * scale]
            } else {
                [dx, dy + a * scale, dx + 8. * scale, dy + b * scale]
            };
            Word {
                id,
                x0: rect[0],
                y0: rect[1],
                x1: rect[2],
                y1: rect[3],
                source_box: rect,
                raw: text.into(),
                text: text.into(),
                status: WordStatus::Used,
            }
        })
        .collect()
    }
    #[test]
    fn article_space_uses_measured_em_in_both_axes_and_at_arbitrary_scale() {
        for horizontal in [false, true] {
            for (scale, dx, dy) in [(1., 0., 0.), (0.7, 42., 73.), (1.8, 93., 23.)] {
                let mut s = Spacing::default();
                s.observe(&words(scale, dx, dy, 8., horizontal), &[0, 1], false);
                assert_eq!(s.gap("第一条本文を定める。"), Some(("第一条", 1)));
                assert_eq!(s.gap("第一条　本文を定める。"), None);
                assert_eq!(s.gap("第一条第一項中の参照"), None);
            }
        }
    }
    #[test]
    fn absent_or_conflicting_original_gaps_never_authorize_insertions() {
        let mut s = Spacing::default();
        s.observe(&words(1., 0., 0., 0., false), &[0, 1], false);
        assert_eq!(s.gap("第一条本文を定める。"), None);
        s.observe(&words(1., 0., 0., 8., false), &[0, 1], false);
        assert_eq!(s.gap("第一条本文を定める。"), None);
        let mut bad = words(1., 0., 0., 8., false);
        bad[1].source_box[0] += 50.;
        bad[1].source_box[2] += 50.;
        let mut s = Spacing::default();
        s.observe(&bad, &[0, 1], false);
        assert_eq!(s.gap("第一条本文を定める。"), None);
        assert_eq!(label("第１２条の３の二本文"), Some("第１２条の３の二"));
    }
    #[test]
    fn numbered_prose_keeps_each_measured_label_and_definition_gap() {
        for horizontal in [false, true] {
            for (scale, dx, dy) in [(1., 0., 0.), (0.7, 42., 73.), (1.8, 93., 23.)] {
                let source = [
                    ("一", 0., 8.),
                    ("定義語", 16., 40.),
                    ("定義の本文。", 48., 96.),
                ]
                .into_iter()
                .enumerate()
                .map(|(id, (text, a, b))| {
                    let rect = if horizontal {
                        [dx + a * scale, dy, dx + b * scale, dy + 8. * scale]
                    } else {
                        [dx, dy + a * scale, dx + 8. * scale, dy + b * scale]
                    };
                    Word {
                        id,
                        x0: rect[0],
                        y0: rect[1],
                        x1: rect[2],
                        y1: rect[3],
                        source_box: rect,
                        raw: text.into(),
                        text: text.into(),
                        status: WordStatus::Used,
                    }
                })
                .collect::<Vec<_>>();
                let mut s = Spacing::default();
                s.observe(&source, &[0, 1, 2], true);
                assert_eq!(s.gaps("一定義語定義の本文。", true), vec![(1, 1), (4, 1)]);
                assert!(s.gaps("一定義語定義の本文。", false).is_empty());
                let mut table = Spacing::default();
                table.observe(&source, &[0, 1, 2], false);
                assert!(table.gaps("一定義語定義の本文。", true).is_empty());
                let mut uninterrupted = source[0].clone();
                uninterrupted.text = "一定義語定義の本文。".into();
                uninterrupted.raw = uninterrupted.text.clone();
                s.observe(&[uninterrupted], &[0], true);
                assert!(s.gaps("一定義語定義の本文。", true).is_empty());
            }
        }
    }
}

pub(crate) fn measured(a: &Word, b: &Word) -> Option<usize> {
    let [ax, ay, ar, ab] = a.source_box;
    let [bx, by, br, bb] = b.source_box;
    if !a
        .source_box
        .iter()
        .chain(&b.source_box)
        .all(|v| v.is_finite())
        || ar <= ax
        || ab <= ay
        || br <= bx
        || bb <= by
    {
        return None;
    }
    let (aw, ah, bw, bh) = (ar - ax, ab - ay, br - bx, bb - by);
    let (em, gap) = if ah >= aw * 0.5 && bh >= bw * 0.5
        && (ah > aw * 1.5 || bh > bw * 1.5)
        && ((ax + ar - bx - br) / 2.).abs() <= aw.min(bw) * 0.3
    {
        (bw, by - ab)
    } else if aw >= ah * 0.5 && bw >= bh * 0.5
        && (aw > ah * 1.5 || bw > bh * 1.5)
        && ((ay + ab - by - bb) / 2.).abs() <= ah.min(bh) * 0.3
    {
        (bh, bx - ar)
    } else {
        return None;
    };
    if em <= 0. || gap < 0. || gap > em * 4.5 {
        return None;
    }
    Some(if gap < em * 0.5 {
        0
    } else {
        (gap / em).round().clamp(1., 4.) as usize
    })
}

impl Spacing {
    pub fn new(analysis: &Analysis) -> Self {
        let mut s = Self::default();
        for p in &analysis.pages {
            for r in &p.regions {
                for l in &r.lines {
                    s.observe(&p.words, &l.words, l.table.is_none());
                    if let Some(t) = &l.table {
                        for c in &t.cells {
                            s.observe(&p.words, &c.words, false);
                        }
                    }
                }
            }
        }
        s
    }
    fn observe(&mut self, words: &[Word], ids: &[usize], prose: bool) {
        for (i, &id) in ids.iter().enumerate() {
            let Some(a) = words.get(id).filter(|w| w.status == WordStatus::Used) else {
                continue;
            };
            let raw = compact(&a.text);
            let article = label(&raw);
            let Some(l) =
                article.or_else(|| (prose && i == 0).then(|| prose_label(&raw)).flatten())
            else {
                continue;
            };
            let prose_only = article.is_none();
            if l != raw {
                self.spans.push(Span {
                    prefix: raw.clone(),
                    start: 0,
                    end: raw.chars().count(),
                    prose_only,
                });
                self.gaps.push(Gap {
                    prefix: raw.clone(),
                    label: l.into(),
                    offset: l.chars().count(),
                    prose_only,
                    spaces: None,
                });
                continue;
            }
            let mut prefix = String::new();
            let mut end = 0;
            for j in i..ids.len() {
                let Some(a) = words
                    .get(ids[j])
                    .filter(|w| w.status == WordStatus::Used && !w.text.is_empty())
                else {
                    break;
                };
                let start = end;
                let own = compact(&a.text);
                end += own.chars().count();
                prefix.push_str(&own);
                self.spans.push(Span {
                    prefix: prefix.clone(),
                    start,
                    end,
                    prose_only,
                });
                let Some(b) = ids
                    .get(j + 1)
                    .and_then(|&id| words.get(id))
                    .filter(|w| w.status == WordStatus::Used && !w.text.is_empty())
                else {
                    break;
                };
                self.gaps.push(Gap {
                    prefix: format!("{}{}", prefix, compact(&b.text)),
                    label: l.into(),
                    offset: end,
                    prose_only,
                    spaces: measured(a, b),
                });
            }
        }
    }
    pub fn gaps(&self, text: &str, prose: bool) -> Vec<(usize, usize)> {
        let text = compact(text);
        let mut at = std::collections::BTreeMap::<usize, Vec<&Gap>>::new();
        for gap in self
            .gaps
            .iter()
            .filter(|g| (prose || !g.prose_only) && text.starts_with(&g.prefix))
        {
            at.entry(gap.offset).or_default().push(gap);
        }
        at.into_iter()
            .filter_map(|(offset, matches)| {
                if self.spans.iter().any(|s| {
                    (prose || !s.prose_only)
                        && s.start < offset
                        && offset < s.end
                        && text.starts_with(&s.prefix)
                }) {
                    return None;
                }
                let max = matches.iter().map(|g| g.prefix.len()).max()?;
                let selected = matches
                    .into_iter()
                    .filter(|g| g.prefix.len() == max)
                    .collect::<Vec<_>>();
                let first = selected.first()?;
                let n = first.spaces.filter(|&n| n > 0)?;
                selected
                    .iter()
                    .all(|g| g.spaces == Some(n) && g.label == first.label)
                    .then_some((offset, n))
            })
            .collect()
    }
    #[cfg(test)]
    fn gap<'a>(&'a self, text: &str) -> Option<(&'a str, usize)> {
        let l = label(text)?;
        if text[l.len()..].starts_with(char::is_whitespace) {
            return None;
        }
        let n = self
            .gaps(text, false)
            .into_iter()
            .find(|&(i, _)| i == l.chars().count())?
            .1;
        let own = self.gaps.iter().find(|g| g.label == l)?;
        Some((own.label.as_str(), n))
    }
}
