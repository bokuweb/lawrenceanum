//! Keep explicitly referenced financial notices and balance sheets together
//! inside their actual native article enclosure. Never synthesize text/IDs.
use crate::trace::{Line, Unit, Word};
use std::collections::HashMap;
type Box4 = [f32; 4];
fn compact(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            '０'..='９' | 'Ａ'..='Ｚ' | 'ａ'..='ｚ' => {
                char::from_u32(c as u32 - 0xfee0).unwrap_or(c)
            }
            _ => c,
        })
        .collect()
}
fn contains(a: Box4, b: Box4) -> bool {
    b[0] >= a[0] - 1. && b[1] >= a[1] - 1. && b[2] <= a[2] + 1. && b[3] <= a[3] + 1.
}
fn bounds(l: &Line, words: &HashMap<usize, &Word>) -> Option<Box4> {
    let mut bs: Vec<Box4> = l
        .words
        .iter()
        .map(|i| words.get(i).map(|w| [w.x0, w.y0, w.x1, w.y1]))
        .collect::<Option<_>>()?;
    if let Some(t) = &l.table {
        bs.push([t.x0, t.y0, t.x1, t.y1]);
    }
    if bs.is_empty() {
        return None;
    }
    Some(bs.into_iter().fold(
        [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ],
        |a, b| {
            [
                a[0].min(b[0]),
                a[1].min(b[1]),
                a[2].max(b[2]),
                a[3].max(b[3]),
            ]
        },
    ))
}
fn upright(l: &Line, words: &HashMap<usize, &Word>) -> bool {
    !l.words.is_empty()
        && l.table.is_none()
        && bounds(l, words).is_some_and(|b| b[3] - b[1] > (b[2] - b[0]) * 1.5)
}
fn notice_title(t: &str) -> bool {
    let t = compact(t);
    t.ends_with("公告")
        && !t.contains(['、', '。'])
        && !t.contains("本公告")
        && t.chars().count() <= 30
        && ["合併", "分割", "資本金", "準備金"]
            .iter()
            .any(|s| t.contains(s))
}
fn bs_title(t: &str) -> bool {
    let t = compact(t);
    t.starts_with('第') && t.contains("期決算公告")
}
fn company(t: &str) -> Option<String> {
    let t = compact(t);
    let t = t.trim_start_matches(|c: char| "（）()甲乙丙丁".contains(c));
    (!t.contains("公告")
        && (5..=60).contains(&t.chars().count())
        && ["株式会社", "有限会社", "合同会社"]
            .iter()
            .any(|s| t.contains(s)))
    .then(|| t.to_string())
}
#[derive(Debug)]
struct Notice {
    ids: Vec<usize>,
    text: String,
    rect: Box4,
}

fn reorder(lines: &mut Vec<Line>, words: &HashMap<usize, &Word>, frame: Box4) {
    let rects: Vec<_> = lines.iter().map(|l| bounds(l, words)).collect();
    let selected: Vec<usize> = rects
        .iter()
        .enumerate()
        .filter_map(|(i, r)| r.filter(|r| contains(frame, *r)).map(|_| i))
        .collect();
    if selected.len() < 6 {
        return;
    }
    // A line spanning the enclosure boundary cannot be split safely here.
    if lines.iter().zip(&rects).any(|(l, r)| {
        r.is_some_and(|r| !contains(frame, r))
            && (l.words.iter().any(|i| {
                words
                    .get(i)
                    .is_some_and(|w| contains(frame, [w.x0, w.y0, w.x1, w.y1]))
            }) || (l.table.is_some()
                && r.is_some_and(|r| {
                    r[0].max(frame[0]) + 1. < r[2].min(frame[2])
                        && r[1].max(frame[1]) + 1. < r[3].min(frame[3])
                })))
    }) {
        return;
    }
    let mut notices = Vec::new();
    let mut k = 0;
    while k < selected.len() {
        let at = selected[k];
        if !notice_title(&lines[at].text) || !upright(&lines[at], words) {
            k += 1;
            continue;
        }
        let start = k;
        k += 1;
        while k < selected.len()
            && selected[k] == selected[k - 1] + 1
            && upright(&lines[selected[k]], words)
            && !notice_title(&lines[selected[k]].text)
        {
            k += 1;
        }
        let ids = selected[start..k].to_vec();
        let text = ids
            .iter()
            .map(|i| compact(&lines[*i].text))
            .collect::<String>();
        // Only notices explicitly referring to the supplied balance sheets.
        if !text.contains("貸借対照表") || !text.contains("要旨") || ids.len() < 4 {
            return;
        }
        let rect = ids.iter().map(|i| rects[*i].unwrap()).fold(
            [
                f32::INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
            ],
            |a, b| {
                [
                    a[0].min(b[0]),
                    a[1].min(b[1]),
                    a[2].max(b[2]),
                    a[3].max(b[3]),
                ]
            },
        );
        notices.push(Notice { ids, text, rect });
    }
    if notices.is_empty() {
        return;
    }
    let notice_ids: Vec<usize> = notices.iter().flat_map(|n| n.ids.iter().copied()).collect();
    let rest: Vec<usize> = selected
        .iter()
        .copied()
        .filter(|i| !notice_ids.contains(i))
        .collect();
    let titles: Vec<usize> = rest
        .iter()
        .enumerate()
        .filter_map(|(j, i)| bs_title(&lines[*i].text).then_some(j))
        .collect();
    // The remaining material must consist entirely of complete balance-sheet
    // blocks. A generic enclosing page frame or unrelated prose is insufficient.
    if titles.is_empty() || titles[0] != 0 {
        return;
    }
    let mut matched = vec![false; notices.len()];
    for (j, start) in titles.iter().enumerate() {
        let end = titles.get(j + 1).copied().unwrap_or(rest.len());
        let block = &rest[*start..end];
        if !block.iter().any(|i| lines[*i].text.contains("貸借対照表"))
            || !block.iter().any(|i| lines[*i].table.is_some())
        {
            return;
        }
        let names: Vec<_> = block
            .iter()
            .take_while(|i| lines[**i].table.is_none())
            .filter_map(|i| company(&lines[*i].text))
            .collect();
        if names.len() != 1 {
            return;
        }
        let mut found = false;
        for (i, n) in notices.iter().enumerate() {
            if n.text.contains(&names[0]) {
                matched[i] = true;
                found = true;
            }
        }
        if !found {
            return;
        }
    }
    if matched.iter().any(|m| !*m) {
        return;
    }
    // Same top band: native vertical text reads from the right. Different bands
    // read from the top. Ambiguous staggered notices are left unchanged.
    notices.sort_by(|a, b| a.rect[1].total_cmp(&b.rect[1]));
    let mut start = 0;
    while start < notices.len() {
        let mut end = start + 1;
        while end < notices.len() && notices[end].rect[1] - notices[start].rect[1] <= 3. {
            end += 1;
        }
        if end < notices.len()
            && notices[start..end]
                .iter()
                .any(|n| n.rect[3] > notices[end].rect[1] + 2.)
        {
            return;
        }
        notices[start..end].sort_by(|a, b| b.rect[2].total_cmp(&a.rect[2]));
        start = end;
    }
    let desired: Vec<usize> = notices
        .iter()
        .flat_map(|n| n.ids.iter().copied())
        .chain(rest)
        .collect();
    if desired == selected && selected.windows(2).all(|w| w[1] == w[0] + 1) {
        return;
    }
    let first = selected[0];
    let mut old: Vec<Option<Line>> = std::mem::take(lines).into_iter().map(Some).collect();
    let mut group: Vec<Line> = desired.iter().map(|i| old[*i].take().unwrap()).collect();
    for (i, l) in old.into_iter().enumerate() {
        if i == first {
            lines.append(&mut group);
        }
        if let Some(l) = l {
            lines.push(l);
        }
    }
}

pub(crate) fn order_financial_enclosures(
    units: &mut [Unit],
    words: &[Word],
    frames: &[Box4],
    width: f32,
    height: f32,
) {
    let words: HashMap<_, _> = words.iter().map(|w| (w.id, w)).collect();
    let mut frames = frames.to_vec();
    frames.retain(|b| !(b[2] - b[0] > width * 0.95 && b[3] - b[1] > height * 0.8));
    frames.sort_by(|a, b| {
        ((a[2] - a[0]) * (a[3] - a[1])).total_cmp(&((b[2] - b[0]) * (b[3] - b[1])))
    });
    let mut unique: Vec<Box4> = Vec::new();
    for f in frames {
        if !unique
            .iter()
            .any(|g| (0..4).all(|i| (f[i] - g[i]).abs() < 3.))
        {
            unique.push(f);
        }
    }
    for f in &unique {
        // Prefer the smallest native article enclosure. A parent page frame
        // enclosing several separate financial articles is not their identity.
        let nested = unique.iter().any(|g| {
            g != f
                && contains(*f, *g)
                && units.iter().any(|u| {
                    let Unit::Plain(ls) = u else {
                        return false;
                    };
                    let local: Vec<_> = ls
                        .iter()
                        .filter(|l| bounds(l, &words).is_some_and(|b| contains(*g, b)))
                        .collect();
                    local
                        .iter()
                        .any(|l| notice_title(&l.text) && upright(l, &words))
                        && local.iter().any(|l| bs_title(&l.text))
                })
        });
        if nested {
            continue;
        }
        for u in units.iter_mut() {
            if let Unit::Plain(ls) = u {
                reorder(ls, &words, *f);
            }
        }
    }
}

#[cfg(test)]
#[path = "financial_enclosure_tests.rs"]
mod tests;

pub(crate) fn order_with_source(
 units:&mut [Unit], words:&[Word], frames:&[Box4], width:f32,height:f32,
 _styles:&[Option<crate::trace::NativeStyles>],_content:&[crate::trace::ContentStyles],
 _bounds:&[crate::trace::NativeBounds],_axes:&[crate::native_word_axes::NativeAxis],_rulings:&[crate::native_word_rulings::NativeRuling], previous:&[crate::trace::PageTrace],
) {
 if _axes.is_empty() || _axes.iter().all(|a|*a==crate::native_word_axes::NativeAxis::Compatibility) {order_financial_enclosures(units,words,frames,width,height);return;}
 let source=source_native::Source{words,styles:_styles,content:_content,bounds:_bounds,axes:_axes,rulings:_rulings,previous};source_native::restore(units,&source,frames);
}
#[path="financial_native_source.rs"]
mod source_native;
#[cfg(test)]
#[path="financial_source_contract_tests.rs"]
mod source_contract_tests;

#[path="financial_source_scopes.rs"]
mod source_scopes;
pub(crate) use source_scopes::{ranges as certified_ranges,repair_segments};
