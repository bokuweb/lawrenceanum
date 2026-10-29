//! A typographically independent instrument with its own prose, date and office.
use super::*;

struct Candidate {
    first: usize,
    end: usize,
    bounds: Rect,
    styles: NativeStyles,
    invalid_announcement: bool,
}

fn candidates(page: &PageTrace, lines: &[SourceLine<'_>]) -> Vec<Candidate> {
    let mut out = vec![];
    let mut first = 0;
    while first < lines.len() {
        let Some((end, styles, bounds, text)) = run(page, lines, first) else {
            first += 1;
            continue;
        };
        let independent = first == 0
            || lines[first - 1].scope != lines[first].scope
            || lines[first - 1].line.table.is_some()
            || super::styles(page, &lines[first - 1].ids)
                .is_some_and(|prior| prior.is_disjoint(&styles));
        // A paragraph's indentation or an inline definition can stop a run
        // without ending its source typography. Only a real style transition
        // after the complete run can delimit a heading from its own prose.
        let distinct_body = lines
            .get(end)
            .and_then(|line| super::styles(page, &line.ids))
            .is_some_and(|body| body.is_disjoint(&styles));
        if independent && distinct_body && lines[first].quote_state.strict() && nominal(&text) {
            out.push(Candidate {
                first,
                end,
                bounds,
                styles,
                // A signed root cannot repair incomplete announcement grammar.
                // A complete existing announcement title can use the same own
                // prose/date/office certificate as any other nominal instrument.
                invalid_announcement: !announcement_ends(&text).is_empty()
                    && title_end(&text).is_none(),
            });
        }
        first = end;
    }
    out
}

// Prose may contain definitions in balanced quotes. It is not a heading or a
// signature certificate: the complete preexisting quote state must be outside
// and valid again at this instrument's own date and signature.
fn prose_content(page: &PageTrace, line: &SourceLine<'_>) -> Option<NativeStyles> {
    native(page, line)?;
    if line.line.table.is_some()
        || !line.quote_state.valid
        || line.ids.iter().any(|&id| {
            !matches!(
                page.native_content_styles.get(id),
                Some(ContentStyles::Glyphs(_))
            )
        })
    {
        return None;
    }
    let styles = heading_styles(page, line, line.tokens.as_ref()?.len())?;
    (!styles.is_empty()).then_some(styles)
}

fn region_bounds(page: &PageTrace, lines: &[SourceLine<'_>], scope: usize) -> Option<Rect> {
    let mut out: Option<Rect> = None;
    for l in lines.iter().filter(|l| l.scope == scope) {
        let b = native(page, l)?;
        out = Some(out.map_or(b, |a| union(a, b)));
    }
    out
}

// The source page's own region order puts a column's last line directly before
// the next column's first line. Prose continues there only when the two regions
// are separated by one original vertical column rule that both lines touch, and
// reading moves up to the top of the following column. No distance, line count
// or wording decides this.
fn column_continues(
    page: &PageTrace,
    rules: &PageRules,
    lines: &[SourceLine<'_>],
    at: usize,
) -> bool {
    let (Some(p), Some(n)) = (at.checked_sub(1).map(|i| &lines[i]), lines.get(at)) else {
        return false;
    };
    let (Some(pb), Some(nb)) = (native(page, p), native(page, n)) else {
        return false;
    };
    let (Some(pr), Some(nr)) = (page.regions.get(p.scope), page.regions.get(n.scope)) else {
        return false;
    };
    if n.scope != p.scope + 1 || pr.unit == nr.unit || nb[1] >= pb[1] {
        return false;
    }
    let (Some(pc), Some(nc)) = (
        region_bounds(page, lines, p.scope),
        region_bounds(page, lines, n.scope),
    ) else {
        return false;
    };
    if pc[3] != pb[3] || nc[1] != nb[1] || pc[2] >= nc[0] {
        return false;
    }
    let touches = |r: &kanpo_pdf::rules::VRule, b: Rect| r.y0 < b[3] && b[1] < r.y1;
    rules
        .vertical
        .iter()
        .any(|r| pc[2] <= r.x && r.x <= nc[0] && touches(r, pb) && touches(r, nb))
}

fn signed(
    page: &PageTrace,
    rules: &PageRules,
    lines: &[SourceLine<'_>],
    head: &Candidate,
    end: usize,
) -> bool {
    let mut previous = lines[head.end - 1].bbox.unwrap();
    let mut body_bounds: Option<Rect> = None;
    let mut body_styles: Option<NativeStyles> = None;
    let mut complete_sentence = false;
    let mut scope = lines[head.first].scope;
    // The horizontal extent this instrument occupies in its current column.
    let mut column = head.bounds;
    // Body lines in the current column; the date and office are placed there.
    let mut column_body: Option<Rect> = None;
    let mut at = head.end;
    while at < end {
        let line = &lines[at];
        let Some(b) = native(page, line) else {
            return false;
        };
        let Some(s) = prose_content(page, line) else {
            return false;
        };
        // Unfinished prose may continue at the top of the next source column.
        if line.scope != scope
            && body_bounds.is_some()
            && column_continues(page, rules, lines, at)
        {
            let Some(next) = region_bounds(page, lines, line.scope) else {
                return false;
            };
            scope = line.scope;
            column = next;
            column_body = None;
            previous = [b[0], f32::NEG_INFINITY, b[2], f32::NEG_INFINITY];
        }
        if line.scope != scope
            || b[1] <= previous[1]
            || b[0] >= column[2]
            || b[2] <= column[0]
            || closed_by_frame(rules, b)
            || !s.is_disjoint(&head.styles)
        {
            return false;
        }
        let text = line.tokens.as_ref().unwrap().concat();
        if date(&text) {
            let (Some(whole), Some(body), Some(bs)) =
                (body_bounds, column_body, body_styles.as_ref())
            else {
                return false;
            };
            if !complete_sentence
                || !line.quote_state.strict()
                || source_baseline(page, line).is_none()
                || *bs != s
                || head.bounds[0] < whole[0]
                || head.bounds[2] > whole[2]
                || b[0] < body[0]
                || b[2] > body[2]
            {
                return false;
            }
            let Some((next, signature)) = native_row(page, lines, at + 1, end) else {
                return false;
            };
            let signature_lines = &lines[at + 1..next];
            if !signature_lines.iter().all(|l| {
                l.scope == line.scope
                    && l.quote_state.strict()
                    && source_baseline(page, l).is_some()
                    && !closed_by_frame(rules, native(page, l).unwrap())
                    && content(page, l).as_ref() == Some(bs)
            }) {
                return false;
            }
            let tokens = signature_lines
                .iter()
                .flat_map(|l| l.tokens.as_ref().unwrap())
                .collect::<Vec<_>>();
            // The office and name are distinct original Word occurrences. No
            // issuer/name list or fixed title wording supplies this evidence.
            let office = tokens.first().copied().map(String::as_str).unwrap_or("");
            let official = office
                .chars()
                .all(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
                && (office.ends_with('長') || office.ends_with("大臣"));
            return official
                && tokens.len() > 1
                && tokens.iter().all(|t| nominal(t))
                && signature[1] >= b[3]
                && signature[0] > b[0]
                && signature[0] >= body[0]
                && signature[2] <= body[2]
                && signature[2] >= head.bounds[2];
        }
        // Prose keeps one shared original body face. A line may add another
        // face only at that same type size, e.g. Latin words within a sentence.
        let shared = match body_styles {
            None => s,
            Some(prior) => {
                let shared: NativeStyles = prior.intersection(&s).copied().collect();
                if shared.is_empty()
                    || !s.iter().all(|(_, size)| prior.iter().any(|(_, p)| p == size))
                {
                    return false;
                }
                shared
            }
        };
        complete_sentence |= text.ends_with('。');
        body_styles = Some(shared);
        body_bounds = Some(body_bounds.map_or(b, |old| union(old, b)));
        column_body = Some(column_body.map_or(b, |old| union(old, b)));
        previous = b;
        at += 1;
    }
    false
}

pub(super) fn starts(page: &PageTrace, rules: &PageRules, lines: &[SourceLine<'_>]) -> Vec<usize> {
    let heads = candidates(page, lines);
    heads
        .iter()
        .enumerate()
        .filter_map(|(i, h)| {
            let end = heads.get(i + 1).map_or(lines.len(), |next| next.first);
            if h.invalid_announcement
                || closed_by_frame(rules, h.bounds)
                || !signed(page, rules, lines, h, end)
            {
                return None;
            }
            // Reuse the existing native hierarchy proof: a larger nominal
            // prefix in this source column belongs to the following root.
            let first = super::super::legal_summaries::larger_prefix(
                page,
                NativeAxis::Right,
                rules,
                lines,
                h.first,
                h.bounds,
                &h.styles,
            )?;
            lines[first].quote_state.strict().then_some(first)
        })
        .collect()
}

// One original row: a complete date, then an office and a name, as distinct
// Words. A date may span several Words where its digits change font. No
// issuer list or title wording supplies this evidence.
fn signature_row(page: &PageTrace, rules: &PageRules, line: &SourceLine<'_>) -> Option<NativeStyles> {
    let b = native(page, line)?;
    let tokens = line.tokens.as_ref()?;
    let s = content(page, line)?;
    if line.line.table.is_some()
        || !line.quote_state.strict()
        || source_baseline(page, line).is_none()
        || closed_by_frame(rules, b)
    {
        return None;
    }
    let n = (1..tokens.len()).find(|&n| date(&tokens[..n].concat()))?;
    let rest = &tokens[n..];
    let office = rest.first()?.as_str();
    let official = office.chars().all(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
        && (office.ends_with('長') || office.ends_with("大臣"));
    (official && rest.len() > 1 && rest.iter().all(|t| nominal(t))).then_some(s)
}

/// A statement without a printed title begins right after the closed table of
/// a preceding statement that was already signed before that table. It needs
/// its own complete prose sentence, immediately followed by its own original
/// date and office row in the same source region and body typography.
pub(super) fn untitled_statements(
    page: &PageTrace,
    rules: &PageRules,
    lines: &[SourceLine<'_>],
) -> Vec<usize> {
    let mut out = vec![];
    for first in 1..lines.len() {
        let scope = lines[first].scope;
        // The preceding statement: its signature row, then only its tables.
        let mut k = first;
        while k > 0 && lines[k - 1].line.table.is_some() && lines[k - 1].scope == scope {
            k -= 1;
        }
        if k == first || k == 0 || lines[k - 1].scope != scope {
            continue;
        }
        let Some(previous) = signature_row(page, rules, &lines[k - 1]) else {
            continue;
        };
        // Its own prose: complete only on the line right before its signature.
        let mut at = first;
        let mut body: Option<NativeStyles> = None;
        let mut ok = false;
        while let Some(line) = lines.get(at) {
            let Some(s) = prose_content(page, line) else { break };
            if line.scope != scope || closed_by_frame(rules, native(page, line).unwrap()) {
                break;
            }
            if body.as_ref().is_some_and(|b| *b != s) {
                break;
            }
            body = Some(s);
            let text = line.tokens.as_ref().unwrap().concat();
            at += 1;
            if text.ends_with('。') {
                ok = lines.get(at).is_some_and(|l| l.scope == scope)
                    && signature_row(page, rules, &lines[at]).is_some_and(|sig| sig == previous);
                break;
            }
        }
        if ok && body.is_some() && lines[first].quote_state.strict() {
            out.push(first);
        }
    }
    out
}
