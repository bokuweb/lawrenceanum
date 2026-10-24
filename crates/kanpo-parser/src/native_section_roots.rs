//! Native-only printed category roots. Ordinary occurrences of the same label
//! remain ordinary prose unless the complete native masthead proves the role.
use super::*;
pub(super) fn starts(page: &PageTrace, rules: &PageRules, lines: &[SourceLine<'_>]) -> Vec<usize> {
    let mut out = vec![];
    for (start, first) in lines.iter().enumerate() {
        if first.ids.is_empty() || first.line.text.trim().is_empty() {
            continue;
        }
        let mut label = String::new();
        let mut ids = vec![];
        let mut at = start;
        loop {
            let Some(row) = lines.get(at) else {
                break;
            };
            if row.scope != first.scope
                || row.role != Role::Body
                || !row.unquoted
                || row.line.table.is_some()
                || row.tokens.is_none()
            {
                break;
            }
            label.push_str(&row.line.text);
            ids.extend(row.ids.iter().copied());
            at += 1;
            if crate::segment::is_native_section_label(&label) {
                while lines.get(at).is_some_and(|row| {
                    row.scope == first.scope
                        && row.line.text.trim().is_empty()
                        && row.line.table.is_none()
                        && row.ids.is_empty()
                }) {
                    at += 1;
                }
                let Some(body) = lines.get(at) else {
                    break;
                };
                if body.scope != first.scope
                    || body.role != Role::Body
                    || !body.unquoted
                    || body.line.table.is_some()
                    || body.tokens.is_none()
                    || crate::segment::is_section_label(&body.line.text)
                    || crate::segment::is_native_section_label(&body.line.text)
                {
                    break;
                }
                let Some(region) = page.regions.get(first.scope) else {
                    break;
                };
                // Pointer identity ties the original borrowed Line to its source
                // location even when text or whole Table values repeat.
                let Some(line) = region
                    .lines
                    .iter()
                    .position(|row| std::ptr::eq(row, first.line))
                else {
                    break;
                };
                if crate::section_context::printed_root(
                    page,
                    rules,
                    first.scope,
                    line,
                    &ids,
                    &body.ids,
                ) {
                    out.push(start);
                }
                break;
            }
            if !crate::segment::is_native_section_prefix(&label) {
                break;
            }
        }
    }
    out
}
