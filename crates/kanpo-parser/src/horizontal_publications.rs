//! Source-backed horizontal publication roots. No issuer identity is inferred.
use super::*;
use crate::{
    native_word_axes::NativeAxis,
    trace::{ContentStyles, NativeBounds},
};

fn union(a: Rect, b: Rect) -> Rect {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}
fn native(page: &PageTrace, l: &SourceLine<'_>) -> Option<Rect> {
    if l.role != Role::Body || l.ids.is_empty() {
        return None;
    }
    for &id in &l.ids {
        if page.native_axes.get(id) != Some(&NativeAxis::Right)
            || !matches!(page.native_bounds.get(id),Some(NativeBounds::Glyphs(b))if valid(*b))
        {
            return None;
        }
    }
    l.bbox
}
fn content(page: &PageTrace, l: &SourceLine<'_>) -> Option<NativeStyles> {
    native(page, l)?;
    if !l.unquoted
        || l.line.table.is_some()
        || l.ids.iter().any(|&id| {
            !matches!(
                page.native_content_styles.get(id),
                Some(ContentStyles::Glyphs(_))
            )
        })
    {
        return None;
    }
    let s = heading_styles(page, l, l.tokens.as_ref()?.len())?;
    (!s.is_empty()).then_some(s)
}
fn title_end(t: &str) -> Option<usize> {
    announcement_ends(t)
        .into_iter()
        .find(|&b| qualifier_bytes(&t[b..]).is_some_and(|n| b + n == t.len()))
}
struct Head {
    first: usize,
    end: usize,
    rect: Rect,
    styles: NativeStyles,
    text: String,
    base: usize,
}
// Complete source typography and the exact shared left margin delimit a run.
// Native baseline order allows tight leading, including overlapping glyph boxes.
// There is no source-line count or guessed proximity threshold.
fn run(
    page: &PageTrace,
    lines: &[SourceLine<'_>],
    first: usize,
) -> Option<(usize, NativeStyles, Rect, String)> {
    let a = lines.get(first)?;
    let s = content(page, a)?;
    let mut b = native(page, a)?;
    let mut text = a.tokens.as_ref()?.concat();
    let mut end = first + 1;
    while let Some(l) = lines.get(end) {
        let Some(c) = native(page, l) else {
            break;
        };
        if l.scope != a.scope
            || c[0] != b[0]
            || c[1] <= lines[end - 1].bbox?[1]
            || content(page, l).as_ref() != Some(&s)
        {
            break;
        }
        b = union(b, c);
        text.push_str(&l.tokens.as_ref()?.concat());
        end += 1;
    }
    Some((end, s, b, text))
}
fn heads(page: &PageTrace, rules: &PageRules, lines: &[SourceLine<'_>]) -> Vec<Head> {
    let mut out = vec![];
    let mut i = 0;
    while i < lines.len() {
        let Some((end, s, b, text)) = run(page, lines, i) else {
            i += 1;
            continue;
        };
        if let Some(base) = title_end(&text) {
            let independent = i == 0
                || lines[i - 1].scope != lines[i].scope
                || lines[i - 1].line.table.is_some()
                || styles(page, &lines[i - 1].ids).is_some_and(|p| p.is_disjoint(&s));
            if independent && !rules.enclosures.iter().any(|&r| intersects(r, b)) {
                out.push(Head {
                    first: i,
                    end,
                    rect: b,
                    styles: s,
                    text,
                    base,
                });
            }
        }
        i = end;
    }
    out
}
// A smaller numbered instrument must share its own title prefix and source
// font family. Other or malformed captions still fence witness searches.
fn child_instrument(page: &PageTrace, lines: &[SourceLine<'_>], h: &Head) -> Option<usize> {
    let (end, s, b, t) = run(page, lines, h.end)?;
    let (prefix, number) = t.split_once("公告第")?;
    let number = number.strip_suffix('号')?;
    let root = h.text[..h.base].strip_suffix("公告")?;
    if root.is_empty()
        || !prefix.starts_with(root)
        || !super::court_records::positive_integer(number)
        || s.iter().map(|&(f, _)| f).collect::<BTreeSet<_>>()
            != h.styles.iter().map(|&(f, _)| f).collect()
        || b[0] > h.rect[0]
        || b[2] < h.rect[2]
        || lines[h.end].bbox?[1] <= lines[h.end - 1].bbox?[1]
    {
        return None;
    }
    Some(end)
}
fn fence(page: &PageTrace, lines: &[SourceLine<'_>], h: &Head) -> usize {
    let child = child_instrument(page, lines, h);
    for i in h.end..lines.len() {
        let l = &lines[i];
        if l.scope != lines[h.first].scope {
            return i;
        }
        if child.is_some_and(|end| i < end) {
            continue;
        }
        if !l.unquoted || l.line.table.is_some() {
            continue;
        }
        let Some(tokens) = &l.tokens else {
            continue;
        };
        let t = tokens.concat();
        if title_end(&t).is_some() {
            return i;
        }
        // A complete sentence is not an unparsed nominal caption. Its first
        // prose line may immediately follow the differently styled instrument.
        if t.contains(['。', '！', '？', '!', '?']) {
            continue;
        }
        if !announcement_ends(&t).is_empty() {
            let continued = i > 0
                && styles(page, &lines[i - 1].ids)
                    .zip(styles(page, &l.ids))
                    .is_some_and(|(a, b)| !a.is_disjoint(&b));
            if !continued {
                return i;
            }
        }
    }
    lines.len()
}
// The date ends at an original Word boundary. An opaque address later on
// the same source line supplies geometry, never evidence about the date font.
fn dated(page: &PageTrace, l: &SourceLine<'_>) -> Option<(Rect, NativeStyles)> {
    native(page, l)?;
    if !l.unquoted || l.line.table.is_some() {
        return None;
    }
    let mut t = String::new();
    let mut b = None;
    for (n, token) in l.tokens.as_ref()?.iter().enumerate() {
        if !matches!(
            page.native_content_styles.get(l.ids[n]),
            Some(ContentStyles::Glyphs(_))
        ) {
            return None;
        }
        t.push_str(token);
        let w = word_bounds(page, l.ids[n])?;
        b = Some(b.map_or(w, |a| union(a, w)));
        if date(&t) {
            return Some((b?, heading_styles(page, l, n + 1)?));
        }
    }
    None
}
fn nominal(t: &str) -> bool {
    !t.is_empty() && !t.contains(['。', '、', '「', '」', '『', '』', '！', '？', '!', '?'])
}
fn closed_by_frame(rules: &PageRules, b: Rect) -> bool {
    rules.enclosures.iter().any(|&r| intersects(r, b))
}
// A dated, right-aligned source header precedes a whole existing table, below
// it or in the next column. Address text stays opaque; only its geometry is used.
fn financial_title(h: &Head) -> bool {
    h.text[..h.base].ends_with("決算公告") || h.text[..h.base].ends_with("決算に関する公告")
}
fn financial(
    page: &PageTrace,
    rules: &PageRules,
    lines: &[SourceLine<'_>],
    h: &Head,
    end: usize,
) -> bool {
    if !financial_title(h) {
        return false;
    }
    let Some(date_line) = lines.get(h.end).filter(|_| h.end < end) else {
        return false;
    };
    let Some((d, ds)) = dated(page, date_line) else {
        return false;
    };
    if !ds.is_disjoint(&h.styles) || closed_by_frame(rules, native(page, date_line).unwrap()) {
        return false;
    }
    let mut prior = vec![];
    let mut block = h.rect;
    let mut previous_y = lines[h.end - 1].bbox.unwrap()[1];
    let mut signed = None;
    let mut i = h.end;
    while i < end {
        let Some((next, b)) = native_row(page, lines, i, end) else {
            break;
        };
        if lines[i..next].iter().any(|l| {
            l.line.table.is_some()
                || !l.unquoted
                || l.scope != lines[h.first].scope
                || !nominal(&l.line.text)
                || closed_by_frame(rules, native(page, l).unwrap())
        }) || b[0] < d[0]
            || b[1] <= previous_y
            || b[0] >= block[2]
            || b[2] <= block[0]
        {
            break;
        }
        if i > h.end
            && b[0] > d[0]
            && b[1] >= d[3]
            && prior.iter().any(|p: &Rect| p[2] == b[2])
            && lines[i..next]
                .iter()
                .all(|l| content(page, l).as_ref() == Some(&ds))
        {
            signed = Some((next - 1, b));
        }
        prior.push(b);
        block = union(block, b);
        previous_y = b[1];
        i = next;
    }
    let Some((signed, s)) = signed else {
        return false;
    };
    if h.rect[0] < d[0] || h.rect[2] > block[2] {
        return false;
    }
    let Some(ti) = (signed + 1..end).find(|&i| lines[i].line.table.is_some()) else {
        return false;
    };
    let table = lines[ti].line.table.as_ref().unwrap();
    let t = [table.x0, table.y0, table.x1, table.y1];
    if !valid(t) || native(page, &lines[ti]).is_none() || lines[ti].scope != lines[h.first].scope {
        return false;
    }
    let below = t[1] >= s[3]
        && ((t[0] >= d[0] && t[2] <= block[2] && t[0] <= h.rect[0] && h.rect[2] <= t[2])
            || header_order::below_own_source_table(page, rules, lines, h, ti));
    let next = t[0] > block[2];
    if !below && !next {
        return false;
    }
    (signed + 1..ti).all(|i| {
        let l = &lines[i];
        native(page, l).is_some_and(|b| {
            l.scope == lines[h.first].scope
                && b[0] < t[2]
                && t[0] < b[2]
                && b[1] < t[1]
                && (next || b[1] >= s[3])
        })
    })
}

fn office(t: &str) -> bool {
    t.ends_with('長') && t.chars().all(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
}
// The organization may precede the office on its line (「法人名　学長　氏名」).
fn officer(l: &SourceLine<'_>) -> bool {
    let Some(tokens) = &l.tokens else {
        return false;
    };
    nominal(&tokens.concat())
        && tokens
            .iter()
            .position(|t| office(t))
            .is_some_and(|n| tokens[n + 1..].iter().any(|t| nominal(t)))
}
// An office set alone on its row; the name follows on the next row.
fn office_row(l: &SourceLine<'_>) -> bool {
    l.tokens.as_ref().is_some_and(|t| t.len() == 1 && office(&t[0]))
}
// The signer is set in the date's typography. A name glyph may come from
// another font of the same size, never from the title's.
fn signer_styles(s: &NativeStyles, date: &NativeStyles, title: &NativeStyles) -> bool {
    s.is_superset(date)
        && s.is_disjoint(title)
        && s.iter().all(|&(_, z)| date.iter().any(|&(_, y)| y == z))
}
// Distinct logical Lines can be adjacent fields on one printed source row.
// Require every original horizontal Word to certify the same finite baseline;
// glyph-box top/bottom equality alone is not source baseline evidence.
fn source_baseline(page: &PageTrace, line: &SourceLine<'_>) -> Option<f64> {
    let mut baseline = None;
    for &id in &line.ids {
        let n = page.native_horizontal_baselines.get(id).copied().flatten()?;
        let NativeBounds::Glyphs(b) = page.native_bounds.get(id)? else { return None };
        if !n.is_finite() || (n as f32) < b[1] || (n as f32) > b[3]
            || baseline.is_some_and(|prior| prior != n)
        {
            return None;
        }
        baseline = Some(n);
    }
    baseline
}
fn rightward_source_row(
    page: &PageTrace,
    previous: &SourceLine<'_>,
    current: &SourceLine<'_>,
    a: Rect,
    b: Rect,
) -> bool {
    a[2] <= b[0]
        && source_baseline(page, previous)
            .zip(source_baseline(page, current))
            .is_some_and(|(a, b)| a == b)
}
fn signed_notice(
    page: &PageTrace,
    rules: &PageRules,
    lines: &[SourceLine<'_>],
    h: &Head,
    end: usize,
) -> bool {
    let mut date_evidence = None;
    let mut previous = lines[h.end - 1].bbox.unwrap();
    let mut previous_line = &lines[h.end - 1];
    let mut block = h.rect;
    let mut right_edges = vec![];
    let mut office_edge = None;
    for l in &lines[h.end..end] {
        let Some(b) = native(page, l) else {
            return false;
        };
        if l.line.table.is_some()
            || l.scope != lines[h.first].scope
            || (b[1] <= previous[1]
                && !rightward_source_row(page, previous_line, l, previous, b))
            || b[0] >= block[2]
            || b[2] <= block[0]
            || closed_by_frame(rules, b)
        {
            return false;
        }
        if let Some((d, ds)) = dated(page, l).filter(|(_, s)| s.is_disjoint(&h.styles)) {
            date_evidence = Some((d, ds));
        }
        let mut office_here = None;
        if let Some((d, ds)) = &date_evidence {
            let tail = l.tokens.as_ref().map(|t| t.concat()).unwrap_or_default();
            if b[1] >= d[3]
                && d[0] < b[0]
                && h.rect[0] >= d[0]
                && h.rect[2] <= b[2]
                && right_edges.contains(&b[2])
                && content(page, l).is_some_and(|s| signer_styles(&s, ds, &h.styles))
            {
                if signature_office(&tail) || officer(l) || office_edge == Some(b[2]) && nominal(&tail) {
                    return true;
                }
                office_here = office_row(l).then_some(b[2]);
            }
        }
        office_edge = office_here;
        right_edges.push(b[2]);
        previous = b;
        previous_line = l;
        block = union(block, b);
    }
    false
}
pub(super) fn starts(page: &PageTrace, rules: &PageRules, lines: &[SourceLine<'_>]) -> Vec<usize> {
    let mut starts = heads(page, rules, lines)
        .into_iter()
        .filter_map(|h| {
            let end = fence(page, lines, &h);
            (financial(page, rules, lines, &h, end) || signed_notice(page, rules, lines, &h, end))
                .then_some(h.first)
        })
        .collect::<Vec<_>>();
    starts.extend(instrument_roots::starts(page, rules, lines));
    starts
}

#[path = "native_instrument_roots.rs"]
mod instrument_roots;

// Geometry of one printed source row, possibly represented by several Lines.
// Unknown source baselines retain the existing one-Line behavior.
fn native_row(
    page: &PageTrace,
    lines: &[SourceLine<'_>],
    first: usize,
    end: usize,
) -> Option<(usize, Rect)> {
    let mut rect = native(page, lines.get(first)?)?;
    let mut next = first + 1;
    while next < end {
        let previous = &lines[next - 1];
        let current = &lines[next];
        let Some(b) = native(page, current) else {
            break;
        };
        if current.scope != lines[first].scope
            || current.line.table.is_some()
            || previous.line.table.is_some()
            || !rightward_source_row(page, previous, current, native(page, previous)?, b)
        {
            break;
        }
        rect = union(rect, b);
        next += 1;
    }
    Some((next, rect))
}

#[path = "horizontal_publication_headers.rs"]
mod header_order;
pub(super) use header_order::restore_header_order;
