//! Native source boundaries must be applied before a comparison marker parser
//! can let its final cell extend into following independent announcements.
//! The existing paragraph adapter remains preferred for exact Run/LF/style
//! preservation. Only structured documents need this additional source path.
use crate::{
    notice_boundaries::Part,
    trace::{PageTrace, WordStatus},
    Block, Document,
};
pub(crate) fn authorized(
    original: &Document,
    segment: &str,
    parts: &[Part],
    pages: &[PageTrace],
) -> bool {
    if !original
        .blocks
        .iter()
        .any(|b| matches!(b, Block::Shinkyu { .. }))
        || parts.len() < 2
    {
        return false;
    }
    // These Parts are produced solely by notice_boundaries::groups from native
    // source line offsets and root certificates. No prefix/text search supplies
    // their boundaries. Require the entire original byte stream, in order.
    if parts.iter().any(|p| p.text.is_empty())
        || parts
            .iter()
            .map(|p| p.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
            != segment
    {
        return false;
    }
    let mut claimed = std::collections::BTreeSet::new();
    for (i, p) in parts.iter().enumerate() {
        // The original segment start is established already; each additional
        // article boundary must carry positive native source ownership.
        if i > 0 && p.sources.is_empty() {
            return false;
        }
        for &(pi, id) in &p.sources {
            let Some(w) = pages.get(pi).and_then(|page| page.words.get(id)) else {
                return false;
            };
            if w.id != id || w.status != WordStatus::Used || !claimed.insert((pi, id)) {
                return false;
            }
        }
    }
    true
}
#[cfg(test)]
#[path = "source_article_parts_tests147.rs"]
mod tests;

/// First partition a literal paragraph projection through the existing strict
/// Run/table adapter. Its original LF, spaces, styles and opaque tables remain
/// authoritative for articles without comparison blocks. Model only actual
/// comparisons inside their already-proven native article bounds.
pub(crate) fn documents(
    original: &Document,
    segment: &str,
    parts: &[Part],
    pages: &[PageTrace],
    desired: &[Document],
    tables: &[crate::trace::Table],
) -> Option<Vec<Document>> {
    if !authorized(original, segment, parts, pages) || desired.len() != parts.len() {
        return None;
    }
    let source = Document::from_paragraph_text_with_tables(segment, tables);
    let wanted: Vec<_> = parts
        .iter()
        .map(|p| Document::from_paragraph_text_with_tables(&p.text, tables))
        .collect();
    let preserved = crate::company_document_parts::repartition(&source, &wanted)?;
    Some(
        preserved
            .into_iter()
            .zip(desired)
            .map(|(mut original, bounded)| {
                if bounded
                    .blocks
                    .iter()
                    .any(|b| matches!(b, Block::Shinkyu { .. }))
                {
                    bounded.clone()
                } else {
                    original.format = bounded.format.clone();
                    original
                }
            })
            .collect(),
    )
}
