//! Native font identity, independently of the source point-size scope.
//! A line may have one size and several families (e.g. an article label).
use super::Binding;
use crate::media::{Document, Field};
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    Mincho,
    Gothic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Style {
    pub family: Option<Family>,
    pub bold: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StyleBinding {
    pub page: usize,
    pub words: Vec<usize>,
    pub glyphs: Vec<usize>,
    pub document: usize,
    pub field: Field,
    pub offset: usize,
    pub length: usize,
    pub text: String,
    pub style: Style,
}

fn named_style(name: &str) -> Option<Style> {
    // Family names are typographic evidence, never the body spelling or case.
    // Do not turn Gothic or an opaque W-number into a bold assertion.
    let name = name.to_ascii_lowercase();
    let family = match (name.contains("mincho"), name.contains("gothic")) {
        (true, false) => Some(Family::Mincho),
        (false, true) => Some(Family::Gothic),
        _ => None,
    };
    let bold = if name.contains("bold") {
        Some(true)
    } else if name.contains("regular") || name.contains("light") || name.contains("medium") {
        Some(false)
    } else {
        None
    };
    (family.is_some() || bold.is_some()).then_some(Style { family, bold })
}

fn uniform(page: &kanpo_pdf::Page, glyphs: &[usize]) -> Option<Style> {
    let first = *glyphs.first()?;
    let style = |id: usize| {
        let g = page.glyphs.get(id)?;
        if g.invisible {
            return None;
        }
        named_style(&page.fonts.get(g.font)?.name)
    };
    let value = style(first)?;
    glyphs
        .iter()
        .all(|&id| style(id) == Some(value))
        .then_some(value)
}

pub(super) fn bind(
    base: &crate::Analysis,
    documents: &[Document],
    sources: &[kanpo_pdf::XObjectPageSources],
    sizes: &[Binding],
) -> Vec<StyleBinding> {
    let fields = [Field::Paragraph, Field::After, Field::Before];
    let (streams, offsets) = crate::media::ruby::offsets(base);
    let values = fields.map(|f| {
        documents
            .iter()
            .map(|d| d.field_text(f))
            .collect::<Vec<_>>()
    });
    let occurrences = base
        .pages
        .iter()
        .zip(sources)
        .map(|(p, s)| crate::media::ruby::occurrences(p, s.page()))
        .collect::<Vec<_>>();
    let target = |field: Field, start: usize, text: &str| {
        let fi = fields.iter().position(|f| *f == field)?;
        let final_field = values[fi].concat();
        // The supplied size bindings already certified native positions. Keep
        // the same exact native-prefix proof for mixed-family word scopes.
        if streams[fi].is_empty()
            || !final_field.starts_with(&streams[fi])
            || final_field != crate::source_underlines::field_text(base, field)
        {
            return None;
        }

        let mut cursor = 0;
        let len = text.chars().count();
        for (document, value) in values[fi].iter().enumerate() {
            let end = cursor + value.chars().count();
            if cursor <= start
                && start < end
                && start.checked_add(len)? <= end
                && value
                    .chars()
                    .skip(start - cursor)
                    .take(len)
                    .collect::<String>()
                    == text
            {
                return Some((document, start - cursor));
            }
            cursor = end;
        }
        None
    };
    let mut out = Vec::new();
    let mut claimed = BTreeSet::new();
    let mut put = |b: StyleBinding| {
        let fi = fields.iter().position(|f| *f == b.field).unwrap();
        if b.length == 0
            || (b.offset..b.offset + b.length).any(|i| claimed.contains(&(b.document, fi, i)))
        {
            return;
        }
        claimed.extend((b.offset..b.offset + b.length).map(|i| (b.document, fi, i)));
        out.push(b);
    };
    for b in sizes {
        let Some(source) = sources.get(b.page) else {
            continue;
        };
        if let Some(style) = uniform(source.page(), &b.glyphs) {
            put(StyleBinding {
                page: b.page,
                words: b.words.clone(),
                glyphs: b.glyphs.clone(),
                document: b.document,
                field: b.field,
                offset: b.offset,
                length: b.length,
                text: b.text.clone(),
                style,
            });
            continue;
        }
        // The size scope may be mixed-family. Recover each complete original
        // word by its own certified field offset, not by searching final text.
        for &id in &b.words {
            let Some(word) = base
                .pages
                .get(b.page)
                .and_then(|p| p.words.get(id))
                .filter(|w| w.id == id)
            else {
                continue;
            };
            let Some(glyphs) = occurrences[b.page].get(&id) else {
                continue;
            };
            let Some(&(field, start)) = offsets.get(&(b.page, id)) else {
                continue;
            };
            let text = word
                .text
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>();
            let Some((document, offset)) = target(field, start, &text) else {
                continue;
            };
            if let Some(style) = uniform(source.page(), glyphs) {
                put(StyleBinding {
                    page: b.page,
                    words: vec![id],
                    glyphs: glyphs.clone(),
                    document,
                    field,
                    offset,
                    length: text.chars().count(),
                    text,
                    style,
                });
            } else {
                // A bbox Word can itself cross a font boundary. Split only
                // when its complete raw string and normalized scalar sequence
                // are reconstructed exactly from the original glyph order.
                let p = source.page();
                let raw = glyphs
                    .iter()
                    .map(|&gid| p.glyphs[gid].text.as_str())
                    .collect::<String>();
                let parts = glyphs
                    .iter()
                    .map(|&gid| {
                        let g = &p.glyphs[gid];
                        (
                            gid,
                            crate::normalize::word_text(&g.text)
                                .chars()
                                .filter(|c| !c.is_whitespace())
                                .collect::<String>(),
                        )
                    })
                    .collect::<Vec<_>>();
                if raw != word.raw
                    || parts.iter().map(|(_, t)| t.as_str()).collect::<String>() != text
                {
                    continue;
                }
                let mut scalar = offset;
                for (gid, text) in parts {
                    let length = text.chars().count();
                    if let Some(style) = uniform(p, &[gid]) {
                        put(StyleBinding {
                            page: b.page,
                            words: vec![id],
                            glyphs: vec![gid],
                            document,
                            field,
                            offset: scalar,
                            length,
                            text,
                            style,
                        });
                    }
                    scalar += length;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn family_is_not_a_bold_assertion_and_opaque_weights_remain_unknown() {
        assert_eq!(
            named_style("ABCDEF+ExampleGothic-W5-Identity-V"),
            Some(Style {
                family: Some(Family::Gothic),
                bold: None
            })
        );
        assert_eq!(
            named_style("ExampleMincho-Light"),
            Some(Style {
                family: Some(Family::Mincho),
                bold: Some(false)
            })
        );
        assert_eq!(
            named_style("Helvetica-Bold"),
            Some(Style {
                family: None,
                bold: Some(true)
            })
        );
        assert_eq!(named_style("Unknown-W9"), None);
        assert_eq!(named_style("AmbiguousMinchoGothic"), None);
    }
}
