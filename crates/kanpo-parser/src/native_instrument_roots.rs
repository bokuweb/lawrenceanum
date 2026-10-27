//! A typographically independent instrument with its own prose, date and office.
use super::*;

struct Candidate {
    first: usize,
    end: usize,
    bounds: Rect,
    styles: NativeStyles,
    announcement: bool,
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
                // Existing announcement grammar keeps its original authorization.
                announcement: !announcement_ends(&text).is_empty(),
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
    let mut at = head.end;
    while at < end {
        let line = &lines[at];
        let Some(b) = native(page, line) else {
            return false;
        };
        let Some(s) = prose_content(page, line) else {
            return false;
        };
        if line.scope != lines[head.first].scope
            || b[1] <= previous[1]
            || b[0] >= head.bounds[2]
            || b[2] <= head.bounds[0]
            || closed_by_frame(rules, b)
            || !s.is_disjoint(&head.styles)
        {
            return false;
        }
        let text = line.tokens.as_ref().unwrap().concat();
        if date(&text) {
            let (Some(body), Some(bs)) = (body_bounds, body_styles.as_ref()) else {
                return false;
            };
            if !complete_sentence
                || !line.quote_state.strict()
                || source_baseline(page, line).is_none()
                || *bs != s
                || head.bounds[0] < body[0]
                || head.bounds[2] > body[2]
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
        if body_styles.as_ref().is_some_and(|prior| prior != &s) {
            return false;
        }
        complete_sentence |= text.ends_with('。');
        body_styles = Some(s);
        body_bounds = Some(body_bounds.map_or(b, |old| union(old, b)));
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
            if h.announcement
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
