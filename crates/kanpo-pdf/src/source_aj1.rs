//! Optional source evidence; legacy public Page/Glyph/FontInfo are untouched.
#[cfg(test)]
use crate::page::Glyph;
use crate::page::Page;
/// Strictly nested rectangular contours of opposite orientation. This keeps
/// the empty interior distinct from the actual painted border.
#[derive(Debug,Clone,Copy,PartialEq,serde::Serialize)]
pub struct RectangleFrame {
    pub outer:[f64;4],
    pub inner:[f64;4],
}
impl RectangleFrame {
    pub fn painted_rectangles(self)->[[f64;4];4] {
        let [a,b,c,d]=self.outer;let [e,f,g,h]=self.inner;
        [[a,b,c,f],[a,f,e,h],[g,f,c,h],[a,h,c,d]]
    }
}
/// Cardinal font-space axes from the original text rendering matrix, including
/// reflection and page/form transforms. Absence is unknown, not an inferred
/// orientation from a font name, bounding box or glyph advance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceTextAxes {
    pub x: [i8; 2],
    pub y: [i8; 2],
}
impl SourceTextAxes {
    pub(crate) fn from_matrix(matrix: [f64; 4]) -> Option<Self> {
        fn axis(x: f64, y: f64) -> Option<[i8; 2]> {
            if !x.is_finite() || !y.is_finite() {
                return None;
            }
            match (x, y) {
                (x, 0.) if x > 0. => Some([1, 0]),
                (x, 0.) if x < 0. => Some([-1, 0]),
                (0., y) if y > 0. => Some([0, 1]),
                (0., y) if y < 0. => Some([0, -1]),
                _ => None,
            }
        }
        let [a, b, c, d] = matrix;
        let x = axis(a, b)?;
        let y = axis(c, d)?;
        (x[0] * y[0] + x[1] * y[1] == 0).then_some(Self { x, y })
    }
}
/// An owned native page and its opaque original-font evidence.
/// Rendering consumes only this bundle, never an externally paired `Page`.
#[derive(Debug, Clone)]
pub struct Aj1PageSources {
    pub(crate) lineage: crate::source_lineage::Tape,
    pub(crate) original: Page,
    pub(crate) resolved: Vec<Option<char>>,
    pub(crate) ink_rects: Vec<Option<[f64; 4]>>,
    pub(crate) aliases: crate::font::source_alias::Aliases,
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
    /// Original glyph's font-space axes from this same owned source snapshot.
    /// Skew, non-cardinal, degenerate or missing transforms return None.
    pub fn glyph_text_axes(&self, glyph: usize) -> Option<SourceTextAxes> {
        if glyph >= self.original.glyphs.len() {
            return None;
        }
        self.lineage.text_axes(glyph)
    }
    /// Actual opaque CFF rectangle painted by an original glyph, independent
    /// of whether bbox extraction grouped it into a Word. This is geometry,
    /// not a declaration that the rectangle is an underline.
    pub fn glyph_painted_rectangle(&self, glyph:usize) -> Option<[f64;4]> {
        if glyph >= self.original.glyphs.len() { return None; }
        self.lineage.glyph_painted_rectangle(glyph)
    }
    /// Two or more positively separated rectangular contours of one original
    /// CFF glyph. Holes, overlaps, touching contours and partial geometry are
    /// unknown. The contours do not by themselves establish decoration style.
    pub fn glyph_separate_painted_rectangles(&self,glyph:usize)->Option<&[[f64;4]]> {
        if glyph>=self.original.glyphs.len(){return None;}
        self.lineage.glyph_separate_rectangles(glyph)
    }
    pub fn glyph_rectangle_frame(&self,glyph:usize)->Option<RectangleFrame> {
        if glyph>=self.original.glyphs.len(){return None;}
        self.lineage.glyph_rectangle_frame(glyph)
    }
    /// The original bbox token, including namespace tags for unresolved CIDs.
    /// This is not a Unicode interpretation of an unknown Identity glyph.
    pub fn glyph_bbox_text(&self,glyph:usize)->Option<String> {
        Some(crate::bbox::glyph_text(self.original.glyphs.get(glyph)?,&self.original.fonts))
    }
    pub(crate) fn ink_rects(&self) -> &[Option<[f64; 4]>] {
        &self.ink_rects
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
