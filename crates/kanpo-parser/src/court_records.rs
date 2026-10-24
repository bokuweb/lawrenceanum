//! A nominal court signature closes one record; a separately styled, complete
//! case identifier below it opens the next. Evidence belongs to those exact
//! original lines, never to a nearby name, date, or another page's signature.
use super::*;
use crate::{
    native_word_axes::NativeAxis,
    trace::{ContentStyles, NativeBounds},
};

fn positive_integer(s: &str) -> bool {
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
        let (footer, head, body) = (&lines[i - 1], &lines[i], &lines[i + 1]);
        if footer.scope != head.scope || body.scope != head.scope {
            continue;
        }
        let Some(tokens) = &head.tokens else {
            continue;
        };
        if !case_identifier(&tokens.concat())
            || !footer
                .tokens
                .as_ref()
                .is_some_and(|t| court_signature(&t.concat()))
        {
            continue;
        }
        let (Some((f, fs)), Some((h, hs)), Some((b, bs))) = (
            evidence(page, footer),
            evidence(page, head),
            evidence(page, body),
        ) else {
            continue;
        };
        // The closing institution is right aligned in the same source column.
        // The new header returns to its left margin, followed by indented body.
        // Exact original bounds define this relation; there is no point tolerance.
        if f[3] > h[1]
            || h[3] > b[1]
            || h[0] >= f[0]
            || h[2] > f[2]
            || b[0] <= h[0]
            || b[2] > f[2]
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
        cuts.push(i);
    }
    cuts
}
