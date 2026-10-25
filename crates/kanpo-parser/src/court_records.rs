//! A nominal court signature closes one record; a separately styled, complete
//! case identifier below it opens the next. Evidence belongs to those exact
//! original lines, never to a nearby name, date, or another page's signature.
use super::*;
use crate::{
    native_word_axes::NativeAxis,
    trace::{ContentStyles, NativeBounds},
};

pub(super) fn positive_integer(s: &str) -> bool {
    fn digit(c: char) -> bool {
        c.is_ascii_digit() || "〇零一二三四五六七八九".contains(c)
    }
    if s.is_empty() {
        return false;
    }
    if s.chars().all(digit) {
        return s.chars().any(|c| !"0〇零".contains(c));
    }
    fn group(s: &str) -> bool {
        let (mut previous, mut coefficient, mut any) = (10000, false, false);
        for c in s.chars() {
            let unit = match c {
                '千' => 1000,
                '百' => 100,
                '十' => 10,
                _ => 0,
            };
            if unit > 0 {
                if unit >= previous {
                    return false;
                }
                previous = unit;
                coefficient = false;
            } else {
                if coefficient || !"一二三四五六七八九".contains(c) {
                    return false;
                }
                coefficient = true;
            }
            any = true;
        }
        any
    }
    let (mut previous, mut start) = (4, 0);
    for (i, c) in s.char_indices() {
        let unit = match c {
            '兆' => 3,
            '億' => 2,
            '万' => 1,
            _ => continue,
        };
        if unit >= previous || !group(&s[start..i]) {
            return false;
        }
        previous = unit;
        start = i + c.len_utf8();
    }
    start == s.len() || group(&s[start..])
}
fn case_identifier(s: &str) -> bool {
    let Some(s) = ["令和", "平成", "昭和", "大正", "明治"]
        .iter()
        .find_map(|e| s.strip_prefix(e))
    else {
        return false;
    };
    let Some((year, s)) = s.split_once('年') else {
        return false;
    };
    if year != "元" && !positive_integer(year) {
        return false;
    }
    let (open, close) = match s.chars().next() {
        Some('（') => ('（', '）'),
        Some('(') => ('(', ')'),
        _ => return false,
    };
    let Some((kind, rest)) = s[open.len_utf8()..].split_once(close) else {
        return false;
    };
    if kind.is_empty()
        || !kind.chars().all(|c| {
            ('\u{30a1}'..='\u{30fa}').contains(&c) || ('\u{4e00}'..='\u{9fff}').contains(&c)
        })
    {
        return false;
    }
    rest.strip_prefix('第')
        .and_then(|s| s.strip_suffix('号'))
        .is_some_and(positive_integer)
}
fn court_signature(s: &str) -> bool {
    let Some((name, tail)) = s.split_once("裁判所") else {
        return false;
    };
    let nominal = |c: char| {
        c.is_ascii_digit()
            || ('\u{3041}'..='\u{3096}').contains(&c)
            || ('\u{30a1}'..='\u{30fa}').contains(&c)
            || ('\u{4e00}'..='\u{9fff}').contains(&c)
    };
    !name.is_empty()
        && name.chars().all(nominal)
        && ["最高", "高等", "地方", "家庭", "簡易"]
            .iter()
            .any(|kind| name.ends_with(kind))
        && (tail.is_empty()
            || tail.chars().all(nominal)
                && ["部", "法廷", "出張所"]
                    .iter()
                    .any(|end| tail.ends_with(end)))
}
fn evidence(page: &PageTrace, l: &SourceLine<'_>) -> Option<(Rect, NativeStyles)> {
    if l.role != Role::Body || !l.unquoted || l.line.table.is_some() || l.ids.is_empty() {
        return None;
    }
    for &id in &l.ids {
        if page.native_axes.get(id) != Some(&NativeAxis::Right)
            || !matches!(page.native_bounds.get(id), Some(NativeBounds::Glyphs(_)))
            || !matches!(
                page.native_content_styles.get(id),
                Some(ContentStyles::Glyphs(_))
            )
        {
            return None;
        }
    }
    let styles = heading_styles(page, l, l.tokens.as_ref()?.len())?;
    (!styles.is_empty()).then_some((l.bbox?, styles))
}
pub(super) fn starts(page: &PageTrace, rules: &PageRules, lines: &[SourceLine<'_>]) -> Vec<usize> {
    let mut cuts = vec![];
    for i in 1..lines.len().saturating_sub(1) {
        let (head, body) = (&lines[i], &lines[i + 1]);
        if body.scope != head.scope {
            continue;
        }
        let Some(tokens) = &head.tokens else {
            continue;
        };
        if !case_identifier(&tokens.concat()) {
            continue;
        }
        let (Some((h, hs)), Some((b, bs))) = (evidence(page, head), evidence(page, body)) else {
            continue;
        };
        let Some(reference_size) = max_size(hs.iter().chain(bs.iter())) else {
            continue;
        };
        let mut previous = i - 1;
        let mut captions = vec![];
        let mut valid_prefix = true;
        loop {
            let row = &lines[previous];
            if row
                .tokens
                .as_ref()
                .is_some_and(|t| court_signature(&t.concat()))
            {
                break;
            }
            if row.scope != head.scope {
                valid_prefix = false;
                break;
            }
            let Some((bounds, style)) = evidence(page, row) else {
                valid_prefix = false;
                break;
            };
            // Source typography supplies the hierarchy. Font IDs are opaque;
            // no category word, row count or distance selects this prefix.
            if row.line.text.contains(['。', '.', '．'])
                || row
                    .tokens
                    .as_ref()
                    .is_some_and(|t| case_identifier(&t.concat()))
                || max_size(style.iter()).is_none()
                || fonts(&style) != fonts(&hs)
                || !style
                    .iter()
                    .all(|(_, size)| f64::from_bits(*size) > reference_size)
            {
                valid_prefix = false;
                break;
            }
            captions.push((previous, bounds, style));
            let Some(at) = previous.checked_sub(1) else {
                valid_prefix = false;
                break;
            };
            previous = at;
        }
        if !valid_prefix {
            continue;
        }
        let footer = &lines[previous];
        let (Some((f, fs)), Some((h, hs)), Some((b, bs))) = (
            evidence(page, footer),
            evidence(page, head),
            evidence(page, body),
        ) else {
            continue;
        };
        let transition = if captions.is_empty() {
            None
        } else {
            column_transition(page, rules, lines, footer, head, body, &captions)
        };
        if footer.scope != head.scope && transition.is_none() {
            continue;
        }
        let (lf, lh, lb) = transition.as_ref().map_or((f, h, b), |c| {
            (c.local(f, false), c.local(h, true), c.local(b, true))
        });
        // The closing institution is right aligned in the same source column.
        // The new header returns to its left margin, followed by indented body.
        // Exact original bounds define this relation; there is no point tolerance.
        if transition.is_none() && f[3] > h[1]
            || h[3] > b[1]
            || lh[0] >= lf[0]
            || lh[2] > lf[2]
            || lb[0] <= lh[0]
            || lb[2] > lf[2]
            || !hs.is_disjoint(&fs)
            || !hs.is_disjoint(&bs)
            || fs.is_disjoint(&bs)
            || rules
                .enclosures
                .iter()
                .any(|&r| intersects(r, f) || intersects(r, h) || intersects(r, b))
        {
            continue;
        }
        if !captions.is_empty() {
            let Some(footer_size) = max_size(fs.iter()) else {
                continue;
            };
            captions.reverse();
            let mut edge = if transition.is_some() {
                f32::NEG_INFINITY
            } else {
                f[3]
            };
            let mut certified = true;
            for (at, bounds, style) in &captions {
                let local = transition
                    .as_ref()
                    .map_or(*bounds, |c| c.local(*bounds, true));
                if local[0] < lh[0]
                    || local[2] > lf[2]
                    || bounds[1] < edge
                    || bounds[3] > h[1]
                    || !style
                        .iter()
                        .all(|(_, size)| f64::from_bits(*size) > footer_size)
                    || rules.enclosures.iter().any(|r| intersects(*r, *bounds))
                    || line_baselines(page, &lines[*at]).is_none()
                {
                    certified = false;
                    break;
                }
                edge = bounds[3];
            }
            // A cross-column prefix occupies the next column's top, not the
            // vertical interval following the previous column's footer.
            let (start, left, right) = transition.as_ref().map_or((f[3], h[0], f[2]), |c| {
                (captions[0].1[1], c.next[0], c.next[1])
            });
            // Printed separators can introduce a different source scope.
            if !certified
                || rules
                    .horizontal
                    .iter()
                    .any(|r| start < r.y && r.y < h[1] && r.x0 < right && left < r.x1)
                || rules
                    .vertical
                    .iter()
                    .any(|r| left < r.x && r.x < right && r.y0 < b[3] && start < r.y1)
            {
                continue;
            }
            if line_baselines(page, footer).is_none()
                || line_baselines(page, head).is_none()
                || line_baselines(page, body).is_none()
            {
                continue;
            }
        }
        cuts.push(captions.first().map_or(i, |c| c.0));
    }
    cuts
}
fn line_baselines(page: &PageTrace, line: &SourceLine<'_>) -> Option<Vec<f64>> {
    line.ids
        .iter()
        .map(|&id| {
            let n = page
                .native_horizontal_baselines
                .get(id)
                .copied()
                .flatten()?;
            let NativeBounds::Glyphs(b) = page.native_bounds.get(id)? else {
                return None;
            };
            (n.is_finite() && b[1] <= n as f32 && n as f32 <= b[3]).then_some(n)
        })
        .collect()
}
struct Columns {
    previous: [f32; 2],
    next: [f32; 2],
}
impl Columns {
    fn local(&self, mut b: Rect, next: bool) -> Rect {
        let x = if next { self.next } else { self.previous };
        b[0] = (b[0] - x[0]) / (x[1] - x[0]);
        b[2] = (b[2] - x[0]) / (x[1] - x[0]);
        b
    }
}
fn column_transition(
    page: &PageTrace,
    rules: &PageRules,
    lines: &[SourceLine<'_>],
    footer: &SourceLine<'_>,
    head: &SourceLine<'_>,
    body: &SourceLine<'_>,
    captions: &[(usize, Rect, NativeStyles)],
) -> Option<Columns> {
    let mut native = vec![];
    for w in &page.words {
        if w.status != WordStatus::Used {
            continue;
        }
        let NativeBounds::Glyphs(b) = page.native_bounds.get(w.id)? else {
            return None;
        };
        let n = page
            .native_horizontal_baselines
            .get(w.id)
            .copied()
            .flatten()?;
        if !valid(*b) || !n.is_finite() || page.native_axes.get(w.id) != Some(&NativeAxis::Right) {
            return None;
        }
        native.push((w.id, *b, n));
    }
    let top = native.iter().map(|v| v.2).reduce(f64::min)?;
    let bottom = native.iter().map(|v| v.2).reduce(f64::max)?;
    let mut xs = rules
        .vertical
        .iter()
        .filter(|r| {
            r.x.is_finite()
                && r.y0.is_finite()
                && r.y1.is_finite()
                && r.y0 as f64 <= top
                && bottom <= r.y1 as f64
        })
        .map(|r| r.x)
        .collect::<Vec<_>>();
    xs.sort_by(f32::total_cmp);
    xs.dedup();
    let fb = footer.bbox?;
    let hb = head.bbox?;
    let bb = body.bbox?;
    let last = line_baselines(page, footer)?.into_iter().reduce(f64::max)?;
    let first = line_baselines(page, &lines[captions.last()?.0])?
        .into_iter()
        .reduce(f64::min)?;
    let mut answers = vec![];
    for triple in xs.windows(3) {
        let [a, b, c] = [triple[0], triple[1], triple[2]];
        let in_x = |box_: Rect, left: f32, right: f32| left <= box_[0] && box_[2] <= right;
        if !in_x(fb, a, b)
            || !in_x(hb, b, c)
            || !in_x(bb, b, c)
            || !captions.iter().all(|v| in_x(v.1, b, c))
        {
            continue;
        }
        if native.iter().any(|(_, r, _)| r[0] < b && b < r[2]) {
            continue;
        }
        let previous = native
            .iter()
            .filter(|(_, r, _)| in_x(*r, a, b))
            .map(|v| v.2)
            .reduce(f64::max)?;
        let next = native
            .iter()
            .filter(|(_, r, _)| in_x(*r, b, c))
            .map(|v| v.2)
            .reduce(f64::min)?;
        if previous != last || first != next {
            continue;
        }
        answers.push(Columns {
            previous: [a, b],
            next: [b, c],
        });
    }
    if answers.len() == 1 {
        answers.pop()
    } else {
        None
    }
}
fn fonts(styles: &NativeStyles) -> BTreeSet<usize> {
    styles.iter().map(|(id, _)| *id).collect()
}
fn max_size<'a>(styles: impl Iterator<Item = &'a (usize, u64)>) -> Option<f64> {
    let sizes = styles
        .map(|(_, size)| f64::from_bits(*size))
        .collect::<Vec<_>>();
    if sizes.is_empty() || sizes.iter().any(|s| !s.is_finite() || *s <= 0.) {
        return None;
    }
    sizes.into_iter().reduce(f64::max)
}
