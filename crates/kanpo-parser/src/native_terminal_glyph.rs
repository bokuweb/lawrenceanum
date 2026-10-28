//! Optional original last-glyph evidence, independently bound to its own Word.
use crate::{
    native_word_flow::{NativeFlow, NativeMode},
    trace::Word,
};
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Terminal {
    raw: String,
    source_box: [f32; 4],
    bounds: [f32; 4],
    flow: NativeFlow,
    first: usize,
    after: usize,
    total: usize,
    start: f64,
}
impl Terminal {
    fn valid(&self) -> bool {
        self.first < self.after
            && self.after <= self.total
            && self.flow.mode == Some(NativeMode::Horizontal)
            && self.flow.belongs(self.bounds)
            && self.start.is_finite()
            && self.flow.start <= self.start
            && self.start < self.flow.end
            && self.source_box.iter().all(|x| x.is_finite())
            && self.source_box[0] < self.source_box[2]
            && self.source_box[1] < self.source_box[3]
            && (self.after - self.first != 1
                || (self.start == self.flow.start
                    && self.flow.leading == self.flow.end - self.flow.start))
    }
    pub(crate) fn advance_for(&self, w: &Word) -> Option<[f64; 2]> {
        (self.valid()
            && self.raw == w.raw
            && self.source_box == w.source_box
            && w.x0 == self.bounds[0]
            && w.x1 == self.bounds[2]
            && [w.x0, w.y0, w.x1, w.y1].iter().all(|x| x.is_finite())
            && w.y0 < w.y1
            && w.y0 <= self.flow.baseline as f32
            && self.flow.baseline as f32 <= w.y1)
            .then_some([self.start, self.flow.end])
    }
    pub(crate) fn range_for(&self, w: &Word) -> Option<std::ops::Range<usize>> {
        self.advance_for(w)?;
        Some(self.first..self.after)
    }
    pub(crate) fn origins_for(&self, w: &Word) -> Option<[f64; 3]> {
        self.advance_for(w)?;
        Some([self.flow.start, self.flow.end, self.flow.baseline])
    }
}
fn legacy_box(word: scraper::ElementRef<'_>) -> Option<[f32; 4]> {
    let a = word.value();
    let mut b = [0.; 4];
    for (i, k) in ["xmin", "ymin", "xmax", "ymax"].into_iter().enumerate() {
        b[i] = a.attr(k)?.parse::<f32>().ok()?;
    }
    (b.iter().all(|x| x.is_finite()) && b[0] < b[2] && b[1] < b[3]).then_some(b)
}
fn from_word(word: scraper::ElementRef<'_>, total: Option<usize>) -> Option<Terminal> {
    let total = total?;
    let a = word.value();
    let source_box = legacy_box(word)?;
    let crate::trace::NativeBounds::Glyphs(bounds) = crate::native_word_bounds::from_word(word)
    else {
        return None;
    };
    let flow = crate::native_word_flow::from_word(word)?;
    let values = a
        .attr("data-native-terminal-glyph")?
        .split_whitespace()
        .collect::<Vec<_>>();
    if values.len() != 6 || values[0] != "1" {
        return None;
    }
    let first = values[1].parse::<usize>().ok()?;
    let after = values[2].parse::<usize>().ok()?;
    if a.attr("data-native-flow-first-glyph")?
        .parse::<usize>()
        .ok()?
        != first
        || a.attr("data-native-flow-after-glyph")?
            .parse::<usize>()
            .ok()?
            != after
    {
        return None;
    }
    let start = values[3].parse::<f64>().ok()?;
    let end = values[4].parse::<f64>().ok()?;
    let baseline = values[5].parse::<f64>().ok()?;
    if end != flow.end || baseline != flow.baseline {
        return None;
    }
    let terminal = Terminal {
        raw: word.text().collect(),
        source_box,
        bounds,
        flow,
        first,
        after,
        total,
        start,
    };
    terminal.valid().then_some(terminal)
}
/// Only this module can construct a page-local set of nonconflicting source tails.
#[derive(Clone, Debug)]
pub(crate) struct Terminals {
    values: Vec<Option<Terminal>>,
}
impl Terminals {
    pub(crate) fn get(&self, word: &Word) -> Option<&Terminal> {
        let tail = self.values.get(word.id)?.as_ref()?;
        tail.advance_for(word)?;
        Some(tail)
    }
}
pub(crate) fn for_words(page: scraper::ElementRef<'_>, words: &[Word]) -> Terminals {
    let none = || Terminals {
        values: vec![None; words.len()],
    };
    if !words.iter().enumerate().all(|(i, w)| w.id == i) {
        return none();
    }
    let Some(total) = page
        .value()
        .attr("data-native-glyph-count")
        .and_then(|x| x.parse::<usize>().ok())
    else {
        return none();
    };
    let mut by =
        std::collections::HashMap::<(String, [u32; 4]), Vec<(usize, Option<Terminal>)>>::new();
    let mut claims = vec![];
    for (index, source) in page
        .select(&scraper::Selector::parse("word").unwrap())
        .enumerate()
    {
        let Some(b) = legacy_box(source) else {
            continue;
        };
        let attrs = source.value();
        let range = attrs
            .attr("data-native-flow-first-glyph")
            .and_then(|x| x.parse::<usize>().ok())
            .zip(
                attrs
                    .attr("data-native-flow-after-glyph")
                    .and_then(|x| x.parse::<usize>().ok()),
            );
        if let Some((first, after)) = range.filter(|&(a, b)| a < b && b <= total) {
            claims.push((first, after, index));
        }
        by.entry((source.text().collect(), b.map(f32::to_bits)))
            .or_default()
            .push((index, from_word(source, Some(total))));
    }
    // Original glyph ownership is half-open. Touching ranges are distinct words;
    // every member of an overlapping component is ambiguous, never just the later one.
    claims.sort_unstable();
    let mut bad = std::collections::HashSet::new();
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
    Terminals {
        values: words
            .iter()
            .map(|word| {
                let candidates = by.get(&(word.raw.clone(), word.source_box.map(f32::to_bits)))?;
                let [(index, Some(terminal))] = candidates.as_slice() else {
                    return None;
                };
                if bad.contains(index) {
                    return None;
                }
                terminal.advance_for(word)?;
                Some(terminal.clone())
            })
            .collect(),
    }
}
#[cfg(test)]
#[path = "native_terminal_glyph_tests156.rs"]
mod tests;
