//! Source capacity and indentation bound literal edits without resetting quotes.
use crate::{
    lines::PageRules,
    native_word_axes::NativeAxis,
    trace::{NativeBounds, NativeStyles, PageTrace, WordStatus},
};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
type Rect = [f32; 4];
/// Only original page geometry can construct production scope endpoints.
/// Their source row material remains bound when used by the quote interpreter.
pub(crate) struct Scopes<'a> {
    source: Vec<(&'a str, bool)>,
    ends: BTreeMap<usize, usize>,
}
impl<'a> Scopes<'a> {
    pub(crate) fn for_pages(
        xhtml: &str,
        pages: &'a [PageTrace],
        rules: &[Option<PageRules>],
    ) -> Self {
        let mut out = Self {
            source: vec![],
            ends: BTreeMap::new(),
        };
        // Only later page starts use the document-wide continuation guard.
        if pages.len() < 2 {
            return out;
        }
        let mut selected = vec![];
        let mut offset = 0;
        for (pi, p) in pages.iter().enumerate() {
            let mut rows = crate::amendment_quotes::source_lines(p);
            let count = rows.len();
            if pi + 1 < pages.len()
                && rows
                    .iter()
                    .any(|&(text, eligible)| eligible && text.contains('「'))
                && p.native_axes.contains(&NativeAxis::Down)
            {
                rows.push(("", true));
                if crate::amendment_quotes::states(&rows)
                    .last()
                    .is_some_and(|s| !s.strict())
                {
                    selected.push((pi, offset));
                }
                rows.pop();
            }
            out.source.extend(rows);
            offset += count;
        }
        if selected.is_empty() {
            return out;
        }
        let dom = scraper::Html::parse_document(xhtml);
        let nodes = dom
            .select(&scraper::Selector::parse("page").unwrap())
            .collect::<Vec<_>>();
        if nodes.len() != pages.len() {
            return out;
        }
        for (pi, offset) in selected {
            let Some(r) = rules.get(pi).and_then(Option::as_ref) else {
                continue;
            };
            for range in for_page(&pages[pi], nodes[pi], r) {
                out.ends.insert(offset + range.start, offset + range.end);
            }
        }
        out
    }
    pub(crate) fn matches(&self, rows: &[(&str, bool)]) -> bool {
        self.source == rows
    }
    pub(crate) fn end(&self, row: usize) -> Option<usize> {
        self.ends.get(&row).copied()
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.ends.is_empty()
    }
    #[cfg(test)]
    pub(crate) fn for_test(rows: &[(&'a str, bool)], ranges: &[Range<usize>]) -> Self {
        Self {
            source: rows.to_vec(),
            ends: ranges.iter().map(|r| (r.start, r.end)).collect(),
        }
    }
}
fn valid(b: Rect) -> bool {
    b.iter().all(|v| v.is_finite()) && b[0] < b[2] && b[1] < b[3]
}
fn overlaps(a: Rect, b: Rect) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}
fn union(a: Rect, b: Rect) -> Rect {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}
fn styles(value: &str) -> Option<NativeStyles> {
    let mut out = NativeStyles::new();
    for t in value.split_whitespace() {
        let (font, bits) = t.split_once(':')?;
        let v = (font.parse().ok()?, u64::from_str_radix(bits, 16).ok()?);
        if !f64::from_bits(v.1).is_finite() || f64::from_bits(v.1) <= 0. || !out.insert(v) {
            return None;
        }
    }
    (!out.is_empty()).then_some(out)
}
struct Column {
    ids: Vec<usize>,
    scope: usize,
    bounds: Rect,
    path: [f64; 3],
    steps: [f64; 3],
    range: (usize, usize),
    styles: NativeStyles,
}
fn join(a: &mut Column, b: Column) -> bool {
    let start = a.path[2] + a.steps[1] + b.steps[2];
    if a.scope != b.scope
        || a.styles != b.styles
        || a.range.1 != b.range.0
        || a.path[0] != b.path[0]
        || !start.is_finite()
        || start != b.path[1]
        || a.bounds[3] > b.bounds[1]
    {
        return false;
    }
    a.ids.extend(b.ids);
    a.bounds = union(a.bounds, b.bounds);
    a.path[2] = b.path[2];
    a.steps[1] = b.steps[1];
    a.range.1 = b.range.1;
    true
}
fn next(a: &Column, b: &Column) -> bool {
    a.scope == b.scope
        && a.styles == b.styles
        && a.range.1 == b.range.0
        && b.bounds[2] < a.bounds[0]
        && b.path[0] < a.path[0]
}
fn candidates(columns: &[Option<Column>]) -> Vec<Range<usize>> {
    let mut out = vec![];
    for i in 0..columns.len().saturating_sub(2) {
        let (Some(a), Some(b)) = (&columns[i], &columns[i + 1]) else {
            continue;
        };
        if !next(a, b) || a.path[1] <= b.path[1] {
            continue;
        }
        let (indent, margin) = (a.path[1], b.path[1]);
        let mut j = i + 1;
        while j < columns.len() {
            let (Some(prev), Some(c)) = (&columns[j - 1], &columns[j]) else {
                break;
            };
            if !next(prev, c) || c.path[1] != margin {
                break;
            }
            if let Some(Some(witness)) = columns.get(j + 1) {
                if next(c, witness) && witness.path[1] == indent {
                    out.push(i..j + 1);
                    break;
                }
            }
            j += 1;
        }
    }
    out
}
fn capacity(cs: &[&Column], bottom: f64) -> bool {
    if cs.len() < 3 || !bottom.is_finite() {
        return false;
    }
    for (i, pair) in cs.windows(2).enumerate() {
        let (a, b) = (pair[0], pair[1]);
        let Some(em) = b
            .styles
            .iter()
            .map(|p| f64::from_bits(p.1))
            .max_by(f64::total_cmp)
        else {
            return false;
        };
        if !em.is_finite() || em <= 0. {
            return false;
        }
        // Both the complete first glyph envelope and its original advance
        // must fit. A narrow advance is not a smaller painted source glyph.
        let step = a.path[2] + a.steps[1] + b.steps[2] + b.steps[0].max(em);
        if !step.is_finite()
            || if i + 2 == cs.len() {
                step > bottom
            } else {
                step <= bottom
            }
        {
            return false;
        }
    }
    true
}
pub(crate) fn for_page(
    page: &PageTrace,
    source: scraper::ElementRef<'_>,
    rules: &PageRules,
) -> Vec<Range<usize>> {
    let Some(scene) =
        crate::native_source_scene::for_page(source, rules).filter(|s| s.matches(&page.words))
    else {
        return vec![];
    };
    let Some(total) = source
        .value()
        .attr("data-native-glyph-count")
        .and_then(|v| v.parse::<usize>().ok())
    else {
        return vec![];
    };
    let mut nodes = BTreeMap::<(String, [u32; 4]), Vec<_>>::new();
    for node in source.select(&scraper::Selector::parse("word").unwrap()) {
        let b = ["xmin", "ymin", "xmax", "ymax"]
            .iter()
            .map(|k| node.value().attr(k)?.parse::<f32>().ok())
            .collect::<Option<Vec<_>>>();
        let Some(b) = b
            .and_then(|b| <[f32; 4]>::try_from(b).ok())
            .filter(|b| valid(*b))
        else {
            return vec![];
        };
        nodes
            .entry((node.text().collect(), b.map(f32::to_bits)))
            .or_default()
            .push(node);
    }
    let eligible = crate::amendment_quotes::source_lines(page);
    let mut columns = vec![];
    for (scope, region) in page.regions.iter().enumerate() {
        for line in &region.lines {
            let i = columns.len();
            let c = (|| {
                if !eligible.get(i)?.1 || line.words.is_empty() || line.table.is_some() {
                    return None;
                }
                let mut aggregate: Option<Column> = None;
                for &id in &line.words {
                    let w = page.words.get(id)?;
                    if w.id != id
                        || w.status != WordStatus::Used
                        || w.raw.is_empty()
                        || page.native_axes.get(id) != Some(&NativeAxis::Down)
                    {
                        return None;
                    }
                    let [node] = nodes
                        .get(&(w.raw.clone(), w.source_box.map(f32::to_bits)))?
                        .as_slice()
                    else {
                        return None;
                    };
                    let NativeBounds::Glyphs(bounds) = crate::native_word_bounds::from_word(*node)
                    else {
                        return None;
                    };
                    if !matches!(page.native_bounds.get(id),Some(NativeBounds::Glyphs(b))if *b==bounds)
                    {
                        return None;
                    }
                    let own_styles = styles(node.value().attr("data-native-styles")?)?;
                    if page.native_styles.get(id)?.as_ref() != Some(&own_styles) {
                        return None;
                    }
                    let flow = crate::native_column_flow::from_word(*node, Some(total))?;
                    let path = flow.vertical_path(bounds)?;
                    flow.vertical_center_envelope(bounds, &own_styles)?;
                    let sizes = own_styles.iter().map(|p| p.1).collect::<BTreeSet<_>>();
                    if sizes.len() != 1 {
                        return None;
                    }
                    let size = f64::from_bits(*sizes.first()?);
                    // Reproduce the existing downwards Poppler square projection.
                    // Its original raw box and native physical glyph frame differ.
                    let encoded = [path[0], path[1] - size, path[0] + size, path[2] - size]
                        .map(|v| format!("{v:.6}").parse::<f32>().ok());
                    let encoded = encoded.into_iter().collect::<Option<Vec<_>>>()?;
                    if encoded.as_slice() != w.source_box {
                        return None;
                    }
                    let primitive = Column {
                        ids: vec![id],
                        scope,
                        bounds,
                        path,
                        steps: flow.vertical_steps(bounds)?,
                        range: flow.source_range(),
                        styles: own_styles,
                    };
                    if let Some(ref mut a) = aggregate {
                        if !join(a, primitive) {
                            return None;
                        }
                    } else {
                        aggregate = Some(primitive);
                    }
                }
                aggregate
            })();
            columns.push(c);
        }
    }
    let all_bounds = page
        .native_bounds
        .iter()
        .map(|b| match b {
            NativeBounds::Glyphs(b) if valid(*b) => Some(*b),
            _ => None,
        })
        .collect::<Option<Vec<_>>>();
    let Some(all_bounds) = all_bounds.filter(|b| b.len() == page.words.len()) else {
        return vec![];
    };
    candidates(&columns)
        .into_iter()
        .filter(|range| {
            let Some(cs) = columns[range.start..=range.end]
                .iter()
                .map(Option::as_ref)
                .collect::<Option<Vec<_>>>()
            else {
                return false;
            };
            let rect = cs.iter().map(|c| c.bounds).reduce(union).unwrap();
            let Some([_, bottom]) = scene.paragraph_frame(rect) else {
                return false;
            };
            let ids = cs
                .iter()
                .flat_map(|c| c.ids.iter().copied())
                .collect::<BTreeSet<_>>();
            if all_bounds
                .iter()
                .enumerate()
                .any(|(id, &b)| !ids.contains(&id) && overlaps(rect, b))
            {
                return false;
            }
            capacity(&cs, bottom)
        })
        .collect()
}

#[cfg(test)]
#[path = "native_quote_scope_tests.rs"]
mod tests;
