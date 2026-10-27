//! Optional native source aliases. The producer proves exact embedded CFF/CID
//! equality; this consumer binds the directed relation to unique original
//! Glyph occurrences and consults only the existing verified Identity decoder.
//! Version 1 repeats the own raw scalar and canonical source box so a relation
//! cannot transfer to another word by changing just its text or coordinates.
use std::collections::{BTreeMap, HashMap, HashSet};
type Key = (String, [u32; 4]);
#[derive(Default)]
pub(crate) struct Aliases {
    values: HashMap<Key, Vec<Option<String>>>,
}
fn key(w: scraper::ElementRef<'_>) -> Option<Key> {
    let a = w.value();
    let mut b = [0f32; 4];
    for (i, k) in ["xmin", "ymin", "xmax", "ymax"].into_iter().enumerate() {
        b[i] = a.attr(k)?.parse().ok()?;
    }
    (b.iter().all(|v| v.is_finite()) && b[0] < b[2] && b[1] < b[3])
        .then(|| (w.text().collect(), b.map(f32::to_bits)))
}
impl Aliases {
    pub(crate) fn for_word(&self, w: scraper::ElementRef<'_>) -> Option<String> {
        let values = self.values.get(&key(w)?)?;
        let [Some(value)] = values.as_slice() else {
            return None;
        };
        Some(value.clone())
    }
}
struct Occurrence {
    key: Option<Key>,
    chars: Vec<char>,
    range: Option<(usize, usize)>,
    valid: bool,
    relations: Option<Vec<(usize, u32, usize, u32)>>,
    resolved: bool,
}
fn occurrence(w: scraper::ElementRef<'_>, total: usize) -> Occurrence {
    let a = w.value();
    let key = key(w);
    let chars: Vec<_> = w.text().collect::<String>().chars().collect();
    let range = a
        .attr("data-native-flow-first-glyph")
        .and_then(|x| x.parse::<usize>().ok())
        .zip(
            a.attr("data-native-flow-after-glyph")
                .and_then(|x| x.parse::<usize>().ok()),
        );
    let valid = key.is_some()
        && matches!(a.attr("data-wmode"), Some("0" | "1"))
        && matches!(
            a.attr("data-native-axis"),
            Some("right" | "down" | "left" | "up")
        )
        && matches!(
            crate::native_word_bounds::from_word(w),
            crate::trace::NativeBounds::Glyphs(_)
        )
        && range.is_some_and(|(f, e)| f < e && e <= total && e - f == chars.len());
    let relations = (|| {
        let tokens: Vec<_> = a
            .attr("data-native-font-aliases")?
            .split_whitespace()
            .collect();
        if tokens.len() < 8 || tokens[0] != "1" {
            return None;
        }
        let first = tokens[1].parse::<usize>().ok()?;
        let after = tokens[2].parse::<usize>().ok()?;
        if range != Some((first, after)) {
            return None;
        }
        let expected: Option<Vec<f32>> = tokens[3..7].iter().map(|s| s.parse().ok()).collect();
        if expected?
            .as_slice()
            .iter()
            .map(|n| n.to_bits())
            .ne(key.as_ref()?.1)
        {
            return None;
        }
        let mut out = vec![];
        let mut prev = None;
        for token in &tokens[7..] {
            let parts: Vec<_> = token.split(':').collect();
            if parts.len() != 4 {
                return None;
            }
            let offset = parts[0].parse::<usize>().ok()?;
            let private = parts[1].parse::<u32>().ok()?;
            let source = parts[2].parse::<usize>().ok()?;
            let cid = parts[3].parse::<u32>().ok()?;
            if offset >= chars.len()
                || prev.is_some_and(|p| p >= offset)
                || source >= total
                || first.checked_add(offset)? == source
                || cid > 0xFFFD
            {
                return None;
            }
            out.push((offset, private, source, cid));
            prev = Some(offset);
        }
        Some(out)
    })();
    Occurrence {
        key,
        chars,
        range,
        valid,
        relations,
        resolved: a.attr("data-unicode-aj1") == Some("1")
            || a.attr("data-source-aj1-text").is_some(),
    }
}
fn resolved(
    index: usize,
    rows: &[Occurrence],
    bad: &HashSet<usize>,
    glyphs: &BTreeMap<usize, (usize, char)>,
) -> Option<String> {
    let own = &rows[index];
    if !own.valid || bad.contains(&index) || own.resolved {
        return None;
    }
    let links = own.relations.as_ref()?;
    let mut replacements = BTreeMap::new();
    for &(offset, private, source, cid) in links {
        let raw = *own.chars.get(offset)?;
        if raw as u32 != private || !crate::normalize::unresolved_private_scalar(raw) {
            return None;
        }
        let &(source_word, tag) = glyphs.get(&source)?;
        if bad.contains(&source_word)
            || rows[source_word].resolved
            || source_word == index
            || tag as u32 != kanpo_pdf::bbox::IDENTITY_CID_BASE.checked_add(cid)?
        {
            return None;
        }
        // No fallback from an unknown CID to the Unicode codepoint with that number.
        let decoded = crate::normalize::private_cid_glyph(&tag.to_string(), 1., 1.)?;
        let mut chars = decoded.text.chars();
        let c = chars.next()?;
        if chars.next().is_some()
            || c.is_control()
            || c.is_whitespace()
            || matches!(c as u32,0xE000..=0xF8FF|0xF0000..=0xFFFFD|0x100000..=0x10FFFD)
        {
            return None;
        }
        replacements.insert(offset, c);
    }
    let mut out = String::new();
    for (i, &c) in own.chars.iter().enumerate() {
        if crate::normalize::unresolved_private_scalar(c) {
            out.push(*replacements.get(&i)?);
        } else {
            out.push(c);
        }
    }
    Some(out)
}
pub(crate) fn for_page(page: scraper::ElementRef<'_>) -> Aliases {
    let selector = scraper::Selector::parse("word").unwrap();
    if !page
        .select(&selector)
        .any(|w| w.value().attr("data-native-font-aliases").is_some())
    {
        return Aliases::default();
    }
    let Some(total) = page
        .value()
        .attr("data-native-glyph-count")
        .and_then(|x| x.parse().ok())
    else {
        return Aliases::default();
    };
    let rows: Vec<_> = page
        .select(&selector)
        .map(|w| occurrence(w, total))
        .collect();
    // Invalidate every member of an overlapping original ownership component,
    // including a malformed claim extending beyond the page. No total-sized allocation.
    let mut claims: Vec<_> = rows
        .iter()
        .enumerate()
        .filter_map(|(i, r)| r.range.filter(|&(f, e)| f < e).map(|(f, e)| (f, e, i)))
        .collect();
    claims.sort_unstable();
    let mut bad = HashSet::new();
    let mut at = 0;
    while at < claims.len() {
        let mut after = at + 1;
        let mut end = claims[at].1;
        while after < claims.len() && claims[after].0 < end {
            end = end.max(claims[after].1);
            after += 1;
        }
        if after > at + 1 {
            bad.extend(claims[at..after].iter().map(|x| x.2));
        }
        at = after;
    }
    // Equal raw/source boxes are ambiguous even when separately declared ranges differ.
    let mut counts = HashMap::<&Key, Vec<usize>>::new();
    for (i, r) in rows.iter().enumerate() {
        if let Some(k) = &r.key {
            counts.entry(k).or_default().push(i);
        }
    }
    for ids in counts.values().filter(|ids| ids.len() > 1) {
        bad.extend(ids.iter().copied());
    }
    let mut glyphs = BTreeMap::new();
    for (i, r) in rows
        .iter()
        .enumerate()
        .filter(|(i, r)| r.valid && !bad.contains(i))
    {
        let first = r.range.unwrap().0;
        for (offset, &c) in r.chars.iter().enumerate() {
            glyphs.insert(first + offset, (i, c));
        }
    }
    let mut out = Aliases::default();
    for (i, r) in rows.iter().enumerate() {
        if let Some(k) = &r.key {
            out.values
                .entry(k.clone())
                .or_default()
                .push(resolved(i, &rows, &bad, &glyphs));
        }
    }
    out
}
#[cfg(test)]
#[path = "source_font_alias_tests157.rs"]
mod tests;
