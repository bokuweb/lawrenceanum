//! Optional source evidence; legacy public Page/Glyph/FontInfo are untouched.
#[cfg(test)]
use crate::page::Glyph;
use crate::page::Page;
/// An owned native page and its opaque original-font evidence.
/// Rendering consumes only this bundle, never an externally paired `Page`.
#[derive(Debug, Clone)]
pub struct Aj1PageSources {
    pub(crate) original: Page,
    pub(crate) resolved: Vec<Option<char>>,
}
#[cfg(test)]
fn same(a: &Glyph, b: &Glyph) -> bool {
    a.text == b.text
        && a.code == b.code
        && a.cid == b.cid
        && a.font == b.font
        && a.x == b.x
        && a.y == b.y
        && a.end_x == b.end_x
        && a.end_y == b.end_y
        && a.size == b.size
        && a.vertical == b.vertical
        && a.bbox == b.bbox
        && a.invisible == b.invisible
        && a.spacing == b.spacing
        && a.kern == b.kern
        && a.show == b.show
}
impl Aj1PageSources {
    /// Read-only native values. Cloning or modifying this view cannot change
    /// the original page used by the source-aware renderer.
    pub fn page(&self) -> &Page {
        &self.original
    }
    pub(crate) fn resolved(&self) -> &[Option<char>] {
        &self.resolved
    }
    // Historical value-compatibility tests remain useful, but this is not
    // document identity and is deliberately absent from production routing.
    #[cfg(test)]
    pub(crate) fn matched(&self, page: &Page) -> Option<&[Option<char>]> {
        let a = &self.original;
        if a.width != page.width
            || a.height != page.height
            || a.rotate != page.rotate
            || a.fonts.len() != page.fonts.len()
            || a.glyphs.len() != page.glyphs.len()
            || self.resolved.len() != page.glyphs.len()
        {
            return None;
        }
        if !a.fonts.iter().zip(&page.fonts).all(|(a, b)| {
            a.name == b.name
                && a.vertical == b.vertical
                && a.ordering == b.ordering
                && a.has_to_unicode == b.has_to_unicode
        }) || !a.glyphs.iter().zip(&page.glyphs).all(|(a, b)| same(a, b))
        {
            return None;
        }
        Some(&self.resolved)
    }
}
pub(crate) fn word_candidate(
    page: &Page,
    word: &crate::bbox::Word,
    resolved: &[Option<char>],
) -> Option<String> {
    if word.glyphs.is_empty()
        || word.glyphs.windows(2).any(|w| w[0] >= w[1])
        || word.glyphs.len() != word.text.chars().count()
        || word
            .glyphs
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != word.glyphs.len()
    {
        return None;
    }
    let mut out = String::new();
    for (id, raw) in word.glyphs.iter().zip(word.text.chars()) {
        let g = page.glyphs.get(*id)?;
        let font = page.fonts.get(g.font)?;
        if font.ordering.as_deref() != Some("Identity")
            || font.has_to_unicode
            || g.invisible
            || g.code != g.cid
            || char::from_u32(crate::bbox::IDENTITY_CID_BASE.checked_add(g.cid)?)? != raw
            || g.text.chars().ne(std::iter::once(char::from_u32(g.cid)?))
        {
            return None;
        }
        let c = resolved.get(*id).copied().flatten()?;
        if crate::font::aj1(g.cid) != Some(c) || c.is_whitespace() || c.is_control() {
            return None;
        }
        out.push(c);
    }
    Some(out)
}

#[cfg(test)]
#[path = "source_aj1_word_tests.rs"]
mod source_aj1_word_tests;
