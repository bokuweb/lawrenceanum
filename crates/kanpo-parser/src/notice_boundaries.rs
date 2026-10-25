//! Additional article boundaries supported by native source typography and scope.
//! Issuer identity is deliberately not inferred here. Source frames may restore atomic reading blocks.
use crate::lines::PageRules;
use crate::trace::{Line, NativeStyles, PageTrace, Role, WordStatus};
use std::collections::{BTreeSet, HashMap, HashSet};
type Rect = [f32; 4];
pub(crate) struct Part {
    pub text: String,
    pub sources: Vec<(usize, usize)>,
}
fn compact(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            '０'..='９' => char::from_u32(c as u32 - 0xfee0).unwrap(),
            _ => c,
        })
        .collect()
}
fn valid(b: Rect) -> bool {
    b.iter().all(|v| v.is_finite()) && b[0] < b[2] && b[1] < b[3]
}
fn contains(a: Rect, b: Rect) -> bool {
    a[0] <= b[0] && a[1] <= b[1] && a[2] >= b[2] && a[3] >= b[3]
}
fn intersects(a: Rect, b: Rect) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}
pub(crate) fn date(t: &str) -> bool {
    fn digit(c: char) -> Option<u32> {
        match c {
            '0'..='9' => Some(c as u32 - '0' as u32),
            '〇' | '零' => Some(0),
            '一' => Some(1),
            '二' => Some(2),
            '三' => Some(3),
            '四' => Some(4),
            '五' => Some(5),
            '六' => Some(6),
            '七' => Some(7),
            '八' => Some(8),
            '九' => Some(9),
            _ => None,
        }
    }
    fn number(s: &str) -> Option<u32> {
        if s.is_empty() {
            return None;
        }
        if s.chars().all(|c| digit(c).is_some()) {
            return s
                .chars()
                .try_fold(0u32, |n, c| n.checked_mul(10)?.checked_add(digit(c)?));
        }
        let (mut total, mut coefficient, mut previous) = (0u32, None, 10000u32);
        for c in s.chars() {
            let unit = match c {
                '千' => 1000,
                '百' => 100,
                '十' => 10,
                _ => 0,
            };
            if unit != 0 {
                if unit >= previous {
                    return None;
                }
                total = total.checked_add(coefficient.take().unwrap_or(1u32).checked_mul(unit)?)?;
                previous = unit;
            } else {
                if coefficient.is_some() || !"一二三四五六七八九".contains(c) {
                    return None;
                }
                coefficient = Some(digit(c)?);
            }
        }
        total.checked_add(coefficient.unwrap_or(0))
    }
    let Some((y, r)) = t.split_once('年') else {
        return false;
    };
    let Some((m, d)) = r.split_once('月') else {
        return false;
    };
    let Some(d) = d.strip_suffix('日') else {
        return false;
    };
    let y = ["令和", "平成", "昭和"]
        .iter()
        .find_map(|e| y.strip_prefix(e))
        .unwrap_or(y);
    let y = if y == "元" { Some(1) } else { number(y) };
    let (Some(y), Some(m), Some(d)) = (y, number(m), number(d)) else {
        return false;
    };
    let max = match m {
        2 => 29,
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => return false,
    };
    y > 0 && d > 0 && d <= max
}
struct SourceLine<'a> {
    line: &'a Line,
    scope: usize,
    role: Role,
    ids: Vec<usize>,
    tokens: Option<Vec<String>>,
    bbox: Option<Rect>,
    unquoted: bool,
}
fn word_bounds(page: &PageTrace, id: usize) -> Option<Rect> {
    let w = page.words.get(id)?;
    let compatibility = [w.x0, w.y0, w.x1, w.y1];
    if !valid(compatibility) {
        return None;
    }
    if page.native_bounds.is_empty() {
        return Some(compatibility);
    }
    match page.native_bounds.get(id)? {
        crate::trace::NativeBounds::Compatibility => Some(compatibility),
        crate::trace::NativeBounds::Glyphs(b) => valid(*b).then_some(*b),
        crate::trace::NativeBounds::Unknown => None,
    }
}
fn source_lines(page: &PageTrace) -> Option<Vec<SourceLine<'_>>> {
    let mut seen = HashSet::new();
    let mut lines = vec![];
    let quote_lines=crate::amendment_quotes::source_lines(page);
    let quote_states=crate::amendment_quotes::states(&quote_lines);
    let mut occurrences = HashMap::new();
    for w in &page.words {
        if !matches!(w.status, WordStatus::Used) {
            continue;
        }
        let key = (
            [
                w.x0.to_bits(),
                w.y0.to_bits(),
                w.x1.to_bits(),
                w.y1.to_bits(),
            ],
            w.raw.as_str(),
        );
        let style = (
            page.native_styles.get(w.id).and_then(|s| s.as_ref()),
            page.native_bounds.get(w.id),
            page.native_content_styles.get(w.id),
        );
        if let Some(previous) = occurrences.insert(key, style) {
            if previous != style {
                return None;
            }
        }
    }
    for (scope, region) in page.regions.iter().enumerate() {
        for l in &region.lines {
            let ids = if let Some(t) = &l.table {
                t.cells
                    .iter()
                    .flat_map(|c| c.words.iter().copied())
                    .collect()
            } else {
                l.words.clone()
            };
            let mut bbox: Option<Rect> = None;
            let mut tokens = vec![];
            for &id in &ids {
                let w = page.words.get(id)?;
                if w.id != id || !matches!(w.status, WordStatus::Used) || !seen.insert(id) {
                    return None;
                }
                let b = word_bounds(page, id)?;
                if !valid(b) {
                    return None;
                }
                bbox = Some(if let Some(a) = bbox {
                    [
                        a[0].min(b[0]),
                        a[1].min(b[1]),
                        a[2].max(b[2]),
                        a[3].max(b[3]),
                    ]
                } else {
                    b
                });
                tokens.push(compact(&w.text));
            }
            let tokens =
                (l.table.is_none() && tokens.concat() == compact(&l.text)).then_some(tokens);
            let unquoted = quote_states[lines.len()].empty && !l.text.contains(['「','」','『','』']);
            lines.push(SourceLine {
                line: l,
                scope,
                role: region.role,
                ids,
                tokens,
                bbox,
                unquoted,
            });
        }
    }
    if page
        .words
        .iter()
        .any(|w| matches!(w.status, WordStatus::Used) && !seen.contains(&w.id))
    {
        return None;
    }
    Some(lines)
}
fn styles(page: &PageTrace, ids: &[usize]) -> Option<NativeStyles> {
    let mut out = NativeStyles::new();
    for &id in ids {
        out.extend(page.native_styles.get(id)?.as_ref()?.iter().copied());
    }
    (!out.is_empty()).then_some(out)
}
#[derive(Clone)]
struct Heading {
    line: usize,
    financial: bool,
    styles: NativeStyles,
    after: usize,
    authorized: bool,
}
// A title may have balanced parenthetical qualifiers followed by its date.
// No qualifier vocabulary, ordinal, character count or source Word count is used.
fn qualifier_bytes(tail: &str) -> Option<usize> {
    let mut offset = 0;
    loop {
        let Some(open) = tail[offset..].chars().next() else {
            return Some(offset);
        };
        let close = match open {
            '(' => ')',
            '（' => '）',
            _ => break,
        };
        let mut stack = vec![close];
        let mut content = false;
        let mut closed = None;
        for (i, c) in tail[offset + open.len_utf8()..].char_indices() {
            match c {
                '(' => stack.push(')'),
                '（' => stack.push('）'),
                ')' | '）' => {
                    if stack.pop() != Some(c) {
                        return None;
                    }
                    if stack.is_empty() {
                        closed = Some(offset + open.len_utf8() + i + c.len_utf8());
                        break;
                    }
                }
                '「' | '」' | '『' | '』' => return None,
                _ => content = true,
            }
        }
        if !content {
            return None;
        }
        offset = closed?;
    }
    date(&tail[offset..]).then_some(offset)
}
// Only a suffix at the outer title level can end the main announcement noun.
// An unclosed/mismatched prefix cannot be repaired by a later marker inside it.
fn announcement_ends(text: &str) -> Vec<usize> {
    let (mut out, mut stack, mut valid_prefix) = (Vec::new(), Vec::new(), true);
    for (offset, c) in text.char_indices() {
        if valid_prefix && stack.is_empty() && text[offset..].starts_with("公告") {
            out.push(offset + "公告".len());
        }
        match c {
            '(' => stack.push(')'),
            '（' => stack.push('）'),
            ')' | '）' => {
                if stack.pop() != Some(c) {
                    valid_prefix = false;
                }
            }
            '。' | '、' | '「' | '」' | '『' | '』' => valid_prefix = false,
            _ => {}
        }
    }
    out
}
fn heading_styles(page: &PageTrace, line: &SourceLine<'_>, end: usize) -> Option<NativeStyles> {
    // All glyphs, including punctuation, need complete original font evidence.
    styles(page, &line.ids[..end])?;
    let mut out=NativeStyles::new();
    for (&id, token) in line.ids[..end].iter().zip(line.tokens.as_ref()?) {
        let content=token.chars().any(char::is_alphanumeric);
        match page.native_content_styles.get(id) {
            None if page.native_content_styles.is_empty() => { if content { out.extend(page.native_styles.get(id)?.as_ref()?.iter().copied()); } },
            Some(crate::trace::ContentStyles::Compatibility) => { if content { out.extend(page.native_styles.get(id)?.as_ref()?.iter().copied()); } },
            Some(crate::trace::ContentStyles::Glyphs(s)) => {
                if content == s.is_empty() { return None; }
                out.extend(s.iter().copied());
            },
            _ => return None,
        }
    }
    Some(out)
}
fn headings(page: &PageTrace, lines: &[SourceLine<'_>], frames: &[OwnedFrame]) -> Vec<Heading> {
    headings_at_source_starts(page, lines, frames, &[])
}
fn headings_at_source_starts(
    page: &PageTrace,
    lines: &[SourceLine<'_>],
    frames: &[OwnedFrame],
    geometric_starts: &[usize],
) -> Vec<Heading> {
    let mut out=vec![];
    let source=page.regions.iter().flat_map(|r|&r.lines).collect::<Vec<_>>();
    if source.len()!=lines.len() || source.iter().zip(lines).any(|(a,b)|!std::ptr::eq(*a,b.line)) {return vec![];}
    let quote_lines=crate::amendment_quotes::source_lines(page);
    let quote_states=crate::amendment_quotes::states(&quote_lines);
    for (i, l) in lines.iter().enumerate() {
        if quote_states[i].empty && l.role == Role::Body {
            if let Some(tokens) = &l.tokens {
                let text = tokens.concat();
                let bases = announcement_ends(&text);
                let parsed = bases.iter().find_map(|&base| {
                    let title_end = base + qualifier_bytes(&text[base..])?;
                    let mut bytes = 0;
                    let after = tokens.iter().position(|t| {
                        bytes += t.len();
                        bytes == title_end
                    })? + 1;
                    Some((base, after, true))
                });
                // Unparsed titles still fence a structurally independent scope.
                // Source Word segmentation must not let their witnesses leak.
                // The topology filter below rejects announcement mentions in prose.
                let candidate = parsed.or_else(|| {
                    bases.iter().find_map(|&base| {
                        // A bare announcement noun is accepted by the complete
                        // grammar above. At the start of an unparsed compound
                        // it is a prefix, not evidence of a title suffix. An
                        // opening qualifier still marks a possible title end.
                        if base == "公告".len() && !text[base..].starts_with(['(', '（']) {
                            return None;
                        }
                        let mut bytes = 0;
                        let after = tokens.iter().position(|t| {
                            bytes += t.len();
                            bytes >= base
                        })? + 1;
                        Some((base, after, false))
                    })
                });
                if let Some((base, after, valid_tail)) = candidate {
                    let s = heading_styles(page, l, after).unwrap_or_default();
                    // A complete owned enclosure starts a separate source scope.
                    // Typography on a line outside it cannot prove continuation.
                    let continued = i > 0
                        && !frames.iter().any(|f| f.first == i)
                        && !geometric_starts.contains(&i)
                        && lines[i - 1].line.table.is_none()
                        && styles(page, &lines[i - 1].ids).is_some_and(|p| !p.is_disjoint(&s));
                    // A malformed suffix cannot turn an ordinary prose
                    // continuation into a new semantic scope. Fully parsed
                    // title syntax still fences even with ambiguous styles.
                    if valid_tail || !continued {
                        out.push(Heading {
                            line: i,
                            financial: text[..base].ends_with("決算公告"),
                            authorized: valid_tail && !continued && !s.is_empty(),
                            styles: s,
                            after,
                        });
                    }
                }
            }
        }

    }
    // Lexical announcement syntax inside prose does not start a new scope.
    // A title must independently lead a complete frame or fit the vertical
    // heading/body topology. Parsed titles with missing/equal styles still fence their scope.
    out.retain(|h| {
        // In a geometrically separated local band, retain every candidate
        // caption as a witness fence. Its glyph-box aspect is not authority;
        // the caller still requires an operative root at its exact source
        // start and its own signed date. Other captions cannot supply that date.
        !geometric_starts.is_empty()
            || frames.iter().any(|f| f.first == h.line)
            || vertical_heading(lines, h, lines.len())
            || horizontal_scope_heading(page, lines, h)
    });
    out
}
// A role mention in a sentence is not a nominal signature. Dots used in
// personal initials and middle dots remain valid; sentence terminators do not.
pub(crate) fn signature_office(tail: &str) -> bool {
    !tail.contains(['。', '！', '？', '!', '?']) && [
        "代表取締役", "取締役", "代表清算人", "清算人", "代表社員", "代表理事", "理事長", "破産管財人",
    ].iter().any(|role| tail.strip_prefix(role).is_some_and(|s| !s.is_empty()))
}
// Date and signature must be source-token prefixes in this heading's own range.
// A date joined to an address may end only at an original Word boundary.
fn witness(
    page: &PageTrace,
    lines: &[SourceLine<'_>],
    head: &Heading,
    end: usize,
    enclosed_table: bool,
) -> bool {
    if !head.authorized {
        return false;
    }
    let mut date_styles = NativeStyles::new();
    for (i, l) in lines.iter().enumerate().take(end).skip(head.line) {
        if l.role != Role::Body || !l.unquoted {
            continue;
        }
        // An independently owned financial frame may publish a statement
        // without a representative signature. The complete table is the terminal
        // witness only in this already-validated frame, after its own dated header.
        if enclosed_table && head.financial && !date_styles.is_empty() && l.line.table.is_some() {
            return true;
        }
        let Some(tokens) = &l.tokens else {
            continue;
        };
        let from = if i == head.line { head.after } else { 0 };
        let mut text = String::new();
        for n in from..tokens.len() {
            text.push_str(&tokens[n]);
            if date(&text) {
                if let Some(s) =
                    styles(page, &l.ids[from..=n]).filter(|s| s.is_disjoint(&head.styles))
                {
                    date_styles.extend(s);
                    break;
                }
            }
        }
        if !date_styles.is_empty() && i > head.line {
            for start in 0..tokens.len() {
                let tail = tokens[start..].concat();
                if styles(page, &l.ids[start..])
                    .is_some_and(|s| !s.is_disjoint(&date_styles) && s.is_disjoint(&head.styles))
                    && signature_office(&tail)
                {
                    return true;
                }
            }
        }
    }
    false
}
fn vertical_heading(lines: &[SourceLine<'_>], h: &Heading, end: usize) -> bool {
    let Some(a) = lines[h.line].bbox else {
        return false;
    };
    let Some(b) = lines
        .get(h.line + 1)
        .filter(|_| h.line + 1 < end)
        .and_then(|l| l.bbox)
    else {
        return false;
    };
    a[3] - a[1] > a[2] - a[0]
        && b[3] - b[1] > b[2] - b[0]
        && b[2] < a[0]
        && b[1] >= a[1]
        && b[1] <= a[3]
        && b[3] > a[3]
}
// Horizontal subheadings may belong to their parent announcement while still
// fencing its date/signature search. Typography or a writing-direction change
// distinguishes the start of a local block from ordinary prose continuation.
fn horizontal_scope_heading(page: &PageTrace, lines: &[SourceLine<'_>], h: &Heading) -> bool {
    let Some(a) = lines[h.line].bbox else {
        return false;
    };
    let Some(next) = lines.get(h.line + 1) else {
        return false;
    };
    let Some(b) = next.bbox else {
        return false;
    };
    let contrast =
        !h.styles.is_empty() && styles(page, &next.ids).is_some_and(|s| h.styles.is_disjoint(&s));
    let changed_axis = h.line > 0
        && lines[h.line - 1]
            .bbox
            .is_some_and(|p| p[3] - p[1] > p[2] - p[0]);
    a[2] - a[0] > a[3] - a[1]
        && b[2] - b[0] > b[3] - b[1]
        && a[3] < b[1]
        && a[0] < b[2]
        && b[0] < a[2]
        && (contrast || changed_axis)
}
struct OwnedFrame {
    rect: Rect,
    first: usize,
    end: usize,
}
fn owned_frames(page: &PageTrace, rules: &PageRules, lines: &[SourceLine<'_>]) -> Vec<OwnedFrame> {
    let mut out = Vec::new();
    for &frame in &rules.enclosures {
        let mut members = vec![];
        let mut ambiguous = false;
        for (li, l) in lines.iter().enumerate() {
            let mut inside = 0;
            for &id in &l.ids {
                let Some(b) = word_bounds(page, id) else {
                    ambiguous = true;
                    break;
                };
                if contains(frame, b) {
                    inside += 1;
                } else if intersects(frame, b) {
                    ambiguous = true;
                }
            }
            if inside > 0 {
                if inside != l.ids.len() || l.role != Role::Body {
                    ambiguous = true;
                }
                members.push(li);
            }
        }
        if ambiguous || members.is_empty() {
            continue;
        }
        let first = members[0];
        let end = members[members.len() - 1] + 1;
        if (first..end).any(|i| !members.contains(&i) && !lines[i].line.text.trim().is_empty()) {
            continue;
        }
        out.push(OwnedFrame {
            rect: frame,
            first,
            end,
        });
    }
    out
}
// A separately enclosed publication can identify its source with a typeset
// attribution rather than a representative's signature. Recognize the header
// fields by their original typography and columns, never by an issuer name.
// This narrow grammar needs a dated left field, a separate right address
// field, an emphasized byline in that right column, then full-width prose and
// a whole table. Address and byline wrapping are unrestricted.
fn attributed_table_header(
    page: &PageTrace,
    lines: &[SourceLine<'_>],
    head: &Heading,
    end: usize,
    frame: Rect,
    frame_end: usize,
) -> Option<()> {
    let horizontal = |b: Rect| b[2] - b[0] > b[3] - b[1];
    // Nominal glyph boxes can overlap adjacent baselines (ascent/descent).
    // Both vertical endpoints must advance; no fixed gap or overlap allowance.
    let below = |a: Rect, b: Rect| a[1] < b[1] && a[3] < b[3];
    let title = &lines[head.line];
    if !head.authorized || !title.unquoted || head.after != title.ids.len() {
        return None;
    }
    let title_box = title.bbox.filter(|&b| horizontal(b))?;
    let header = lines.get(head.line + 1).filter(|_| head.line + 1 < end)?;
    if !header.unquoted {
        return None;
    }
    let tokens = header.tokens.as_ref()?;
    let mut date_text = String::new();
    let after_date = tokens.iter().position(|t| {
        date_text.push_str(t);
        date(&date_text)
    })? + 1;
    if after_date == tokens.len() {
        return None;
    }
    let union = |ids: &[usize]| -> Option<Rect> {
        ids.iter().try_fold(None, |bounds: Option<Rect>, &id| {
            let b = word_bounds(page, id)?;
            Some(Some(bounds.map_or(b, |a| {
                [
                    a[0].min(b[0]),
                    a[1].min(b[1]),
                    a[2].max(b[2]),
                    a[3].max(b[3]),
                ]
            })))
        })?
    };
    let date_box = union(&header.ids[..after_date])?;
    let address_box = union(&header.ids[after_date..])?;
    let body_styles = heading_styles(page, header, after_date)?;
    // Address punctuation may use another font and need not provide content
    // typography. It still needs complete original style evidence.
    let address_styles = styles(page, &header.ids[after_date..])?;
    if body_styles.is_empty()
        || !body_styles.is_disjoint(&head.styles)
        || !address_styles.is_disjoint(&head.styles)
        || !horizontal(date_box)
        || !horizontal(address_box)
        || !below(title_box, date_box)
        || !below(title_box, address_box)
        || date_box[2] >= address_box[0]
        || date_box[1] >= address_box[3]
        || address_box[1] >= date_box[3]
    {
        return None;
    }
    // All frame source fields must have complete metadata, including table
    // members; absent evidence must not become an apparently blank field.
    for line in &lines[head.line..frame_end] {
        styles(page, &line.ids)?;
        if let Some(table) = &line.line.table {
            let b = [table.x0, table.y0, table.x1, table.y1];
            if !valid(b) || !contains(frame, b) {
                return None;
            }
        }
    }
    let (mut previous, mut attributed) = (
        [
            date_box[0],
            date_box[1].max(address_box[1]),
            address_box[2],
            date_box[3].max(address_box[3]),
        ],
        false,
    );
    for (i, line) in lines.iter().enumerate().take(end).skip(head.line + 2) {
        if line.line.table.is_some() || !line.unquoted {
            return None;
        }
        let b = line.bbox.filter(|&b| horizontal(b))?;
        if !below(previous, b) {
            return None;
        }
        let in_right_column = address_box[0] <= b[0] && b[2] <= address_box[2];
        if !attributed && in_right_column && styles(page, &line.ids)?.is_disjoint(&head.styles) {
            // Address rows may contain punctuation in another source font.
        } else if in_right_column && heading_styles(page, line, line.ids.len())? == head.styles {
            if line.line.text.contains(['。', '！', '？', '!', '?']) {
                return None;
            }
            attributed = true;
        } else if attributed
            && heading_styles(page, line, line.ids.len())? == body_styles
            && b[0] <= date_box[0]
            && b[2] >= address_box[2]
        {
            // The first full-width body row ends the header. A table must be
            // wholly represented later in this same already-owned scope.
            return lines[i + 1..end]
                .iter()
                .any(|l| {
                    l.line.table.as_ref().is_some_and(|t| {
                        let table = [t.x0, t.y0, t.x1, t.y1];
                        valid(table) && contains(frame, table) && below(b, table)
                    })
                })
                .then_some(());
        } else {
            return None;
        }
        previous = b;
    }
    None
}
// Unrendered/undecoded source words are still original geometry. They cannot
// silently straddle two newly certified publications. Unknown native geometry
// is not replaced by a compatibility bbox or by a character interpretation.
fn raw_source_respects_divider(page: &PageTrace, frame: Rect, y: f32) -> bool {
    page.words.iter().enumerate().all(|(id, w)| {
        if w.status == WordStatus::Used {
            return true;
        }
        if w.id != id {
            return false;
        }
        let Some(crate::trace::NativeBounds::Glyphs(b)) = page.native_bounds.get(id) else {
            return false;
        };
        valid(*b) && (!intersects(frame, *b) || contains(frame, *b) && (b[3] <= y || y <= b[1]))
    })
}
// A source horizontal divider can refine a common physical frame only when
// both complete source bands begin with their own dated, signed operative
// notices. A financial caption alone remains an annex, not an independent
// notice. Opaque existing body rows/tables are preserved, never interpreted as
// title/date/signature evidence or normalized to match their Word text.
fn divided_notice_starts(
    page: &PageTrace,
    rules: &PageRules,
    lines: &[SourceLine<'_>],
    frames: &[OwnedFrame],
    frame: &OwnedFrame,
) -> Vec<usize> {
    let mut cuts = BTreeSet::new();
    for h in &rules.horizontal {
        if ![h.x0, h.x1, h.y].iter().all(|v| v.is_finite())
            || h.x0 >= h.x1
            || !(frame.rect[1] < h.y && h.y < frame.rect[3])
        {
            continue;
        }
        let mut split = None;
        let mut valid = true;
        for i in frame.first..frame.end {
            let line = &lines[i];
            let mut side = None;
            for &id in &line.ids {
                let Some(crate::trace::NativeBounds::Glyphs(b)) = page.native_bounds.get(id) else {
                    valid = false;
                    break;
                };
                // A transversal must meet every original column/field, not
                // merely the union bbox of separated words or one table cell.
                if !(h.x0 < b[2] && b[0] < h.x1) {
                    valid = false;
                    break;
                }
                let next = if b[3] <= h.y {
                    false
                } else if h.y <= b[1] {
                    true
                } else {
                    valid = false;
                    break;
                };
                if side.is_some_and(|old| old != next) {
                    valid = false;
                    break;
                }
                side = Some(next);
            }
            if let Some(t) = &line.line.table {
                let b = [t.x0, t.y0, t.x1, t.y1];
                if !super::notice_boundaries::valid(b)
                    || !contains(frame.rect, b)
                    || side.is_none_or(|below| if below { b[1] < h.y } else { h.y < b[3] })
                {
                    valid = false;
                }
            }
            if !valid {
                break;
            }
            match side {
                Some(true) => {
                    split.get_or_insert(i);
                }
                Some(false) if split.is_some() => {
                    valid = false;
                    break;
                }
                None if !line.line.text.trim().is_empty() => {
                    valid = false;
                    break;
                }
                _ => {}
            }
        }
        let Some(split) = split.filter(|&i| valid && frame.first < i && i < frame.end) else {
            continue;
        };
        let local_heads = headings_at_source_starts(page, lines, frames, &[split]);
        let own_notice = |start, end| {
            let Some(root) = local_heads
                .iter()
                .find(|h| h.line == start && h.authorized && !h.financial)
            else {
                return false;
            };
            let fence = local_heads
                .iter()
                .find(|h| h.line > start)
                .map_or(end, |h| h.line.min(end));
            witness(page, lines, root, fence, false)
        };
        if own_notice(frame.first, split)
            && own_notice(split, frame.end)
            && raw_source_respects_divider(page, frame.rect, h.y)
        {
            cuts.insert(split);
        }
    }
    cuts.into_iter().collect()
}
// A complete physical source frame can prove its publication start without
// proving any relationship between the publications inside it. These starts only
// refine existing partitions; they never join or reorder an internal block.
fn frame_fences(
    page: &PageTrace,
    rules: &PageRules,
    lines: &[SourceLine<'_>],
    frames: &[OwnedFrame],
    heads: &[Heading],
) -> BTreeSet<usize> {
    let mut cuts = BTreeSet::new();
    for frame in frames {
        // A nested panel cannot establish independence from its enclosing
        // publication. Crossing enclosures also leave ownership ambiguous.
        if rules.enclosures.iter().any(|&other| {
            other != frame.rect && intersects(other, frame.rect) && !contains(frame.rect, other)
        }) {
            continue;
        }
        let Some(root) = heads.iter().find(|h| h.line == frame.first && h.authorized) else {
            continue;
        };
        let end = heads
            .iter()
            .find(|h| h.line > root.line)
            .map_or(frame.end, |h| h.line.min(frame.end));
        // Witnesses stop at the next local title, even if its styles or syntax
        // are unknown. The later notice cannot supply the first one's date.
        if witness(page, lines, root, end, true)
            || attributed_table_header(page, lines, root, end, frame.rect, frame.end).is_some()
        {
            cuts.insert(frame.first);
            cuts.extend(divided_notice_starts(page,rules,lines,frames,frame));
            // The end of a frame does not prove that following text is a new
            // publication. That text must supply its own independent evidence.
        }
    }
    cuts
}
fn decisions(page: &PageTrace, rules: &PageRules, lines: &[SourceLine<'_>]) -> Vec<usize> {
    if rules.enclosures.iter().any(|&b| !valid(b)) {
        return vec![];
    }
    let frames = owned_frames(page, rules, lines);
    let heads = headings(page, lines, &frames);
    let fences = frame_fences(page, rules, lines, &frames, &heads);
    let mut accepted = vec![];
    let mut scope = vec![];
    for OwnedFrame {
        rect: frame,
        first,
        end,
    } in frames
    {
        let owned: Vec<_> = heads
            .iter()
            .filter(|h| h.line >= first && h.line < end)
            .collect();
        // A fully owned, separately closed statement may use an arbitrary title.
        // Its own distinct typography, publication date and signature remain
        // necessary. A mere table enclosure or undated caption is insufficient.
        let original_root = owned.first().copied().filter(|h| h.line == first);
        // A complete frame can prove an independent publication even when
        // the title's formal suffix is unparsed. Keep its own whole-line
        // typography, date and signature requirements, and every child fence.
        let generic = if (owned.is_empty() || original_root.is_some_and(|h| !h.authorized))
            && lines[first].unquoted
        {
            lines[first].tokens.as_ref().and_then(|tokens| {
                heading_styles(page, &lines[first], tokens.len()).and_then(|content| {
                    if content.is_empty() { styles(page, &lines[first].ids) } else { Some(content) }
                }).map(|styles| Heading {
                    line: first,
                    financial: false,
                    styles,
                    after: tokens.len(),
                    authorized: true,
                })
            })
        } else {
            None
        };
        let Some(root) = original_root
            .filter(|h| h.authorized)
            .or(generic.as_ref())
            .or(original_root)
        else {
            continue;
        };
        if owned.iter().skip(1).any(|h| !h.financial) || root.financial && owned.len() > 1 {
            continue;
        }
        if !root.financial {
            scope.push((first, end));
        }
        let fence = heads
            .iter()
            .find(|h| h.line > root.line)
            .map_or(end, |h| h.line.min(end));
        if witness(page, lines, root, fence, true)
            || attributed_table_header(page, lines, root, fence, frame, end).is_some()
        {
            accepted.push((frame, first, end));
        }
    }
    // A common enclosing announcement owns its related financial subheadings.
    // Overlapping, non-nested frame claims are ambiguous.
    let mut outer = vec![];
    for (i, &(a, first, end)) in accepted.iter().enumerate() {
        if scope
            .iter()
            .any(|&(start, stop)| start < first && end <= stop)
        {
            continue;
        }
        if accepted
            .iter()
            .enumerate()
            .any(|(j, (b, _, _))| j != i && contains(*b, a) && (*b != a || j < i))
        {
            continue;
        }
        if accepted
            .iter()
            .any(|(b, _, _)| intersects(a, *b) && !contains(a, *b) && !contains(*b, a))
        {
            continue;
        }
        outer.push((first, end));
    }
    let mut cuts: BTreeSet<_> = outer.iter().map(|&(first, _)| first).collect();
    for (n, h) in heads.iter().enumerate() {
        if outer
            .iter()
            .chain(scope.iter())
            .any(|&(a, b)| h.line >= a && h.line < b)
        {
            continue;
        }
        let b = lines[h.line].bbox.unwrap();
        if rules.enclosures.iter().any(|&frame| intersects(frame, b)) {
            continue;
        }
        let end = heads.get(n + 1).map_or(lines.len(), |h| h.line);
        if vertical_heading(lines, h, end) && witness(page, lines, h, end, false) {
            cuts.insert(h.line);
        }
    }
    cuts.extend(fences);
    cuts.extend(section_roots::starts(page, rules, lines));
    cuts.extend(legal_summaries::starts(page, rules, lines));
    cuts.extend(court_records::starts(page, rules, lines));
    cuts.extend(horizontal_publications::starts(page, rules, lines));
    cuts.extend(ruled_corrections::section_starts(page, rules, lines));
    let scopes=crate::financial_enclosures::certified_ranges(page,rules);
    cuts.retain(|&cut|!scopes.iter().any(|&(start,end)|start<cut&&cut<end));
    for(start,end)in scopes{cuts.insert(start);if end<lines.len(){cuts.insert(end);}}
    cuts.into_iter().collect()
}
pub(crate) fn cuts(page: &PageTrace, rules: &PageRules) -> Vec<usize> {
    source_lines(page).map_or_else(Vec::new, |lines| decisions(page, rules, &lines))
}
/// Source line offsets must reproduce the actual document stream, including whole tables.
/// No string search is used to assign source ownership to repeated announcements.
pub(crate) fn groups(
    original: &[String],
    text: &str,
    pages: &[PageTrace],
    rules: &[Option<PageRules>],
) -> Vec<Vec<Part>> {
    let unchanged = || {
        original
            .iter()
            .map(|s| {
                vec![Part {
                    text: s.clone(),
                    sources: vec![],
                }]
            })
            .collect()
    };
    if original.join("\n") != text {
        return unchanged();
    }
    let (mut offset, mut table, mut nonempty) = (0usize, 0usize, false);
    let (mut breaks, mut ownership) = (BTreeSet::new(), vec![]);
    let quote_lines=pages.iter().flat_map(crate::amendment_quotes::source_lines).collect::<Vec<_>>();
    let quote_states=crate::amendment_quotes::states(&quote_lines);
    let mut quote_offset=0;
    for (pi, p) in pages.iter().enumerate() {
        let continuing_quote=quote_states.get(quote_offset).is_some_and(|s|!s.empty);
        quote_offset+=p.regions.iter().map(|r|r.lines.len()).sum::<usize>();
        let table_start = table;
        table += p.tables.len();
        if p.doc_text.is_empty() {
            continue;
        }
        if nonempty {
            offset += 1;
        }
        nonempty = true;
        if let Some(lines) = source_lines(p) {
            let mut ti = table_start;
            let mut start = 0;
            let mut rendered = vec![];
            let mut spans = vec![];
            for l in &lines {
                if !rendered.is_empty() {
                    start += 1;
                }
                let s = if l.line.table.is_some() {
                    let s = crate::trace::table_marker(ti);
                    ti += 1;
                    s
                } else {
                    l.line.text.clone()
                };
                spans.push(start);
                start += s.len();
                rendered.push(s);
            }
            let flat = rendered.join("\n");
            let leading = flat.len() - flat.trim_start().len();
            if flat.trim() == p.doc_text && ti == table {
                let default = PageRules::default();
                let r = rules.get(pi).and_then(|r| r.as_ref()).unwrap_or(&default);
                for c in if continuing_quote {
                    Vec::new()
                } else {
                    decisions(p, r, &lines)
                } {
                    if spans[c] >= leading && spans[c] - leading < p.doc_text.len() {
                        breaks.insert(offset + spans[c] - leading);
                    }
                }
                for (l, s) in lines.iter().zip(spans) {
                    if s >= leading {
                        for &id in &l.ids {
                            ownership.push((offset + s - leading, (pi, id)));
                        }
                    }
                }
            }
        }
        offset += p.doc_text.len();
    }
    if offset != text.len() {
        return unchanged();
    }
    let mut result = vec![];
    let mut offset = 0;
    for s in original {
        let end = offset + s.len();
        let mut bounds = vec![offset];
        for &point in breaks.range((
            std::ops::Bound::Excluded(offset),
            std::ops::Bound::Excluded(end),
        )) {
            // A category heading supplies context for its first publication;
            // it has no independent operative body to repartition into an article.
            // Keep every original byte with that first publication. Later roots
            // still split once the preceding part contains real body or a table.
            let start = *bounds.last().unwrap();
            if text[start..point].lines().any(|line| {
                !line.trim().is_empty() && !crate::segment::is_section_label(line)
            }) {
                bounds.push(point);
            }
        }
        bounds.push(end);
        let parts: Vec<_> = bounds
            .windows(2)
            .filter_map(|b| {
                let raw = &text[b[0]..b[1]];
                let t = raw.trim();
                (!t.is_empty()).then(|| Part {
                    text: t.into(),
                    sources: ownership
                        .iter()
                        .filter(|(pos, _)| *pos >= b[0] && *pos < b[1])
                        .map(|(_, id)| *id)
                        .collect(),
                })
            })
            .collect();
        if parts
            .iter()
            .map(|p| p.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
            == *s
        {
            result.push(parts);
        } else {
            result.push(vec![Part {
                text: s.clone(),
                sources: vec![],
            }]);
        }
        offset = end + 1;
    }
    result
}
#[cfg(test)]
#[path = "notice_boundaries_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "notice_attributed_header_tests.rs"]
mod attributed_header_tests;

#[path = "court_records.rs"]
mod court_records;

#[path = "horizontal_publications.rs"]
mod horizontal_publications;

#[cfg(test)]
#[path = "court_record_tests.rs"]
mod court_record_tests;

#[path = "framed_publications.rs"]
mod framed_publications;
pub(crate) use framed_publications::restore_framed_reading_order;

#[path="ruled_corrections.rs"]mod ruled_corrections;
pub(crate) use ruled_corrections::restore_ruled_corrections;

#[cfg(test)]
#[path="horizontal_publication_tests.rs"]
mod horizontal_publication_tests;

#[path="legal_summaries.rs"]mod legal_summaries;
#[cfg(test)]#[path="legal_summary_tests.rs"]mod legal_summary_tests;

#[path = "native_section_roots.rs"]
mod section_roots;
