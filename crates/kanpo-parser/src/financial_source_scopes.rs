//! Native publication certificates repair lexical partitions in either
//! direction. Every byte and complete table marker must remain unchanged.
use crate::lines::PageRules;
use crate::trace::{Line, PageTrace, Role};
use std::collections::BTreeSet;
pub(crate) fn ranges(page: &PageTrace, rules: &PageRules) -> Vec<(usize, usize)> {
    if page.native_axes.is_empty() {
        return vec![];
    }
    let refs = page
        .regions
        .iter()
        .flat_map(|r| r.lines.iter().map(move |l| (r.role, l)))
        .collect::<Vec<_>>();
    let lines = refs
        .iter()
        .map(|(_, l)| (*l).clone())
        .collect::<Vec<Line>>();
    let source = super::source_native::Source {
        words: &page.words,
        styles: &page.native_styles,
        content: &page.native_content_styles,
        bounds: &page.native_bounds,
        axes: &page.native_axes,
        rulings: &page.native_rulings,
        previous: &[],
    };
    let contiguous = |members: &[usize]| {
        let first = *members.first()?;
        let end = members.last()? + 1;
        if members.iter().any(|&i| refs[i].0 != Role::Body)
            || (first..end).any(|i| !members.contains(&i) && !lines[i].text.trim().is_empty())
        {
            return None;
        }
        Some((first, end))
    };
    super::source_native::plans(&lines, &source, &rules.enclosures)
        .into_iter()
        .flat_map(|plan| {
            let Some(parent) = contiguous(&plan.selected) else {
                return vec![];
            };
            // Source certificates can refine the parent only when every
            // publication is already a complete contiguous source stream.
            // This stage never reorders bytes or cuts a Line/Table in half.
            plan.publications
                .iter()
                .map(|p| contiguous(p))
                .collect::<Option<Vec<_>>>()
                .unwrap_or_else(|| vec![parent])
        })
        .collect()
}
pub(crate) fn repair_segments(
    segments: &mut Vec<String>,
    pages: &[PageTrace],
    rules: &[Option<PageRules>],
) {
    let text = crate::trace::join_page_texts(pages, &|p| p.doc_text.as_str());
    if segments.join("\n") != text {
        return;
    }
    let mut breaks = BTreeSet::from([0, text.len()]);
    let mut offset = 0;
    for s in segments.iter() {
        breaks.insert(offset);
        offset += s.len() + 1;
    }
    let (mut page_offset, mut table, mut nonempty) = (0, 0, false);
    let mut quotes = vec![];
    let mut certificates = vec![];
    for (pi, page) in pages.iter().enumerate() {
        let continuing = !quotes.is_empty();
        for c in page.text.chars() {
            match c {
                '「' => quotes.push('」'),
                '『' => quotes.push('』'),
                '」' | '』' if quotes.last() == Some(&c) => {
                    quotes.pop();
                }
                _ => {}
            }
        }
        let table_start = table;
        table += page.tables.len();
        if page.doc_text.is_empty() {
            continue;
        }
        if nonempty {
            page_offset += 1;
        }
        nonempty = true;
        if !continuing {
            if let Some(rule) = rules.get(pi).and_then(Option::as_ref) {
                let mut rendered = vec![];
                let mut spans = vec![];
                let mut local_table = table_start;
                let mut start = 0;
                for line in page.regions.iter().flat_map(|r| &r.lines) {
                    if !rendered.is_empty() {
                        start += 1;
                    }
                    spans.push(start);
                    let value = if line.table.is_some() {
                        let s = crate::trace::table_marker(local_table);
                        local_table += 1;
                        s
                    } else {
                        line.text.clone()
                    };
                    start += value.len();
                    rendered.push(value);
                }
                let flat = rendered.join("\n");
                let leading = flat.len() - flat.trim_start().len();
                if flat.trim() == page.doc_text && local_table == table {
                    for (first, end) in ranges(page, rule) {
                        let a = spans[first];
                        let b = spans[end - 1] + rendered[end - 1].len();
                        if a < leading || b < leading || b - leading > page.doc_text.len() {
                            continue;
                        }
                        certificates.push((page_offset + a - leading, page_offset + b - leading));
                    }
                }
            }
        }
        page_offset += page.doc_text.len();
    }
    if page_offset != text.len() || certificates.is_empty() {
        return;
    }
    for (a, b) in certificates {
        breaks.retain(|&point| point <= a || point >= b);
        breaks.insert(a);
        let end = b + usize::from(text[b..].starts_with('\n'));
        if end <= text.len() {
            breaks.insert(end);
        }
    }
    let positions = breaks.into_iter().collect::<Vec<_>>();
    let desired = positions
        .windows(2)
        .filter_map(|pair| {
            let s = text[pair[0]..pair[1]].trim();
            (!s.is_empty()).then(|| s.to_string())
        })
        .collect::<Vec<_>>();
    if desired.join("\n") == text {
        *segments = desired;
    }
}

/// Preserve original line indices: certificates never own text by searching
/// repeated values, and references never duplicate a native Table.
pub(crate) fn notice_scopes(
    page_index: usize,
    page: &PageTrace,
    rules: &PageRules,
    previous: &[PageTrace],
) -> Vec<crate::notice_sources::NoticeScope> {
    use crate::notice_sources::{NoticeScope, SourceSpan, StatementReference};
    if page.native_axes.is_empty() {
        return vec![];
    }
    let refs = page
        .regions
        .iter()
        .enumerate()
        .flat_map(|(ri, r)| {
            r.lines
                .iter()
                .enumerate()
                .map(move |(li, l)| (ri, li, r.role, l))
        })
        .collect::<Vec<_>>();
    let lines = refs
        .iter()
        .map(|(_, _, _, l)| (*l).clone())
        .collect::<Vec<_>>();
    let source = super::source_native::Source {
        words: &page.words,
        styles: &page.native_styles,
        content: &page.native_content_styles,
        bounds: &page.native_bounds,
        axes: &page.native_axes,
        rulings: &page.native_rulings,
        previous,
    };
    let spans = |members: &[usize]| -> Option<Vec<SourceSpan>> {
        let mut members = members.to_vec();
        members.sort_unstable();
        if members.is_empty() || members.windows(2).any(|w| w[0] == w[1]) {
            return None;
        }
        let mut out: Vec<SourceSpan> = vec![];
        for i in members {
            let &(ri, li, role, _) = refs.get(i)?;
            if role != Role::Body {
                return None;
            }
            if let Some(last) = out
                .last_mut()
                .filter(|last| last.region == ri && last.end_line == li)
            {
                last.end_line += 1;
            } else {
                out.push(SourceSpan {
                    page: page_index,
                    region: ri,
                    start_line: li,
                    end_line: li + 1,
                });
            }
        }
        Some(out)
    };
    let mut out = vec![];
    for plan in super::source_native::plans(&lines, &source, &rules.enclosures) {
        let graph = plan
            .relations
            .iter()
            .map(|(body, annexes)| {
                Some(NoticeScope {
                    source: spans(body)?,
                    financial_statements: annexes
                        .iter()
                        .map(|annex| {
                            Some(StatementReference {
                                source: spans(annex)?,
                            })
                        })
                        .collect::<Option<Vec<_>>>()?,
                })
            })
            .collect::<Option<Vec<_>>>();
        if let Some(graph) = graph {
            out.extend(graph);
        }
    }
    out.sort_by_key(|n| {
        let s = &n.source[0];
        (s.page, s.region, s.start_line)
    });
    out
}
