//! Original embedded program/CID equality, never a font-name alias.
use super::*;
use std::sync::Arc;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    PrivateUnicode,
    UnresolvedIdentity,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FontGlyph {
    program: Arc<[u8]>,
    cid: u16,
    role: Role,
    unicode: Option<char>,
}
#[derive(Debug, Clone)]
pub(crate) struct Program {
    bytes: Arc<[u8]>,
    cids: Vec<u16>,
}
impl FontGlyph {
    pub(crate) fn matches_identity_source(&self, other: &Self) -> bool {
        self.role == Role::PrivateUnicode
            && other.role == Role::UnresolvedIdentity
            && self.cid == other.cid
            && self.program == other.program
    }
}
impl Font {
    pub(crate) fn source_alias_key(&self, doc: &Document, c: &CharCode) -> Option<FontGlyph> {
        if c.len != 2
            || c.code != c.cid
            || !self.composite
            || !self.source_cid0
            || !self.identity_encoding
        {
            return None;
        }
        let cid = u16::try_from(c.cid).ok()?;
        let role = if self.declared_to_unicode && self.has_to_unicode {
            let raw = self.to_unicode.as_ref()?.unicode(c.code, c.len)?;
            let mut chars = raw.chars();
            let scalar = chars.next()?;
            if chars.next().is_some() || !('\u{e000}'..='\u{f8ff}').contains(&scalar) {
                return None;
            }
            Role::PrivateUnicode
        } else if !self.declared_to_unicode
            && !self.has_to_unicode
            && self.to_unicode.is_none()
            && self.ordering.as_deref() == Some("Identity")
        {
            Role::UnresolvedIdentity
        } else {
            return None;
        };
        let program = self
            .source_alias_program
            .get_or_init(|| {
                let stream = self.source_cff.as_ref()?;
                if doc
                    .dict_get(&stream.dict, b"Subtype")
                    .and_then(|o| o.as_name().map(Vec::from))
                    .as_deref()
                    != Some(b"CIDFontType0C")
                {
                    return None;
                }
                let bytes = doc.decode_stream(stream).ok()?;
                let cids = crate::cff_aj1::program_cids(&bytes)?;
                Some(Program {
                    bytes: Arc::from(bytes),
                    cids,
                })
            })
            .as_ref()?;
        program.cids.binary_search(&cid).ok()?;
        let unicode = if role == Role::PrivateUnicode {
            self.to_unicode
                .as_ref()?
                .unicode(c.code, c.len)?
                .chars()
                .next()
        } else {
            None
        };
        Some(FontGlyph {
            program: program.bytes.clone(),
            cid,
            role,
            unicode,
        })
    }
}
#[cfg(test)]
#[path = "source_font_alias157_tests.rs"]
mod tests;

/// An occurrence relation captured alongside the original native Glyphs.
/// It cannot be paired with an externally supplied Page through the public API.
#[derive(Debug, Clone, Default)]
pub(crate) struct Aliases {
    links: Vec<Option<(usize, u16)>>,
}
impl Aliases {
    pub(crate) fn capture(glyphs: &[crate::page::Glyph], keys: &[Option<FontGlyph>]) -> Self {
        let mut out = Self {
            links: vec![None; glyphs.len()],
        };
        if glyphs.len() != keys.len() {
            return out;
        }
        let valid = |i: usize| -> Option<&FontGlyph> {
            let g = glyphs.get(i)?;
            let k = keys.get(i)?.as_ref()?;
            if g.invisible
                || g.cid != u32::from(k.cid)
                || g.code != g.cid
                || g.size <= 0.
                || ![
                    g.x, g.y, g.end_x, g.end_y, g.size, g.bbox[0], g.bbox[1], g.bbox[2], g.bbox[3],
                ]
                .iter()
                .all(|x| x.is_finite())
                || g.bbox[0] >= g.bbox[2]
                || g.bbox[1] >= g.bbox[3]
            {
                return None;
            }
            let mut cs = g.text.chars();
            let c = cs.next()?;
            if cs.next().is_some() {
                return None;
            }
            match k.role {
                Role::PrivateUnicode
                    if k.unicode == Some(c) && ('\u{e000}'..='\u{f8ff}').contains(&c) =>
                {
                    Some(k)
                }
                Role::UnresolvedIdentity if c as u32 == g.cid => Some(k),
                _ => None,
            }
        };
        let mut sources = std::collections::BTreeMap::<u16, Vec<usize>>::new();
        for i in 0..glyphs.len() {
            if let Some(k) = valid(i).filter(|k| k.role == Role::UnresolvedIdentity) {
                sources.entry(k.cid).or_default().push(i);
            }
        }
        for i in 0..glyphs.len() {
            let Some(k) = valid(i).filter(|k| k.role == Role::PrivateUnicode) else {
                continue;
            };
            if let Some(source) = sources.get(&k.cid).and_then(|ids| {
                ids.iter()
                    .copied()
                    .find(|&j| valid(j).is_some_and(|v| k.matches_identity_source(v)))
            }) {
                out.links[i] = Some((source, k.cid));
            }
        }
        out
    }
    /// Version 1 binds the own original range, the serialized source box and
    /// the actual private scalar to an original sibling Glyph and its CID.
    /// The attribute contains no proposed decoded text.
    pub(crate) fn word_attribute(
        &self,
        page: &crate::Page,
        word: &crate::bbox::Word,
    ) -> Option<String> {
        if self.links.len() != page.glyphs.len()
            || word.glyphs.is_empty()
            || word
                .glyphs
                .windows(2)
                .any(|a| a[0].checked_add(1) != Some(a[1]))
            || word.glyphs.len() != word.text.chars().count()
        {
            return None;
        }
        let (first, after) = crate::bbox::native_glyph_range(page, word)?;
        let mut raw = String::new();
        let mut pairs = vec![];
        for (&id, c) in word.glyphs.iter().zip(word.text.chars()) {
            let g = page.glyphs.get(id)?;
            if g.invisible || g.text.chars().count() != 1 {
                return None;
            }
            // glyph_text applies only the source Identity tag here; reproduce it exactly.
            let font = page.fonts.get(g.font)?;
            let actual = if font.ordering.as_deref() == Some("Identity") && !font.has_to_unicode {
                char::from_u32(crate::bbox::IDENTITY_CID_BASE.checked_add(g.cid)?)?
            } else {
                g.text.chars().next()?
            };
            if c != actual {
                return None;
            }
            raw.push(c);
            if let Some((source, cid)) = self.links.get(id).copied().flatten() {
                pairs.push(format!("{}:{}:{source}:{cid}", id - first, c as u32));
            }
        }
        (raw == word.text && !pairs.is_empty()).then(|| {
            format!(
                "1 {first} {after} {:.6} {:.6} {:.6} {:.6} {}",
                word.x_min,
                word.y_min,
                word.x_max,
                word.y_max,
                pairs.join(" ")
            )
        })
    }
}
