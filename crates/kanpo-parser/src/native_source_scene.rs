//! Independently versioned source paint evidence; absence never means empty.
use crate::{lines::PageRules, trace::Word};
type Rect = [f64; 4];
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Certificate {
    version: u8,
    glyphs: usize,
    ignored: Vec<usize>,
    rectangle_ids: Vec<usize>,
    words: Vec<(usize, usize, String, Rect)>,
    unknown: Vec<usize>,
    rectangles: Vec<Rect>,
    rulings: Vec<Rect>,
    rectangle_obstacles: Vec<Rect>,
    images: Vec<Rect>,
    lines: Vec<(Rect, Rect)>,
    fills: Vec<Rect>,
}
pub(crate) struct Scene {
    words: Vec<(String, [f32; 4])>,
    obstacles: Vec<Rect>,
    lines: Vec<(Rect, Rect, bool)>,
}
fn valid(b: Rect) -> bool {
    b.iter().all(|x| x.is_finite()) && b[0] < b[2] && b[1] < b[3]
}
fn contains(a: Rect, b: Rect) -> bool {
    a[0] <= b[0] && b[2] <= a[2] && a[1] <= b[1] && b[3] <= a[3]
}
pub(crate) fn for_page(page: scraper::ElementRef, rules: &PageRules) -> Option<Scene> {
    let c: Certificate = serde_json::from_str(page.value().attr("data-native-scene")?).ok()?;
    if c.version != 1
        || !c.unknown.is_empty()
        || page
            .value()
            .attr("data-native-glyph-count")?
            .parse::<usize>()
            .ok()?
            != c.glyphs
        || c.rectangle_ids.len() != c.rectangles.len()
    {
        return None;
    }
    let mut ranges = vec![];
    for id in c.ignored.iter().chain(&c.rectangle_ids) {
        if *id >= c.glyphs {
            return None;
        }
        ranges.push((*id, id + 1));
    }
    let mut words = vec![];
    let actual = page
        .select(&scraper::Selector::parse("word").ok()?)
        .collect::<Vec<_>>();
    if actual.len() != c.words.len() {
        return None;
    }
    // Original ranges identify occurrences; XHTML node order is not identity.
    let mut records = std::collections::BTreeMap::new();
    for (first, after, raw, b) in &c.words {
        if first >= after
            || *after > c.glyphs
            || !valid(*b)
            || records.insert((*first, *after), (raw, b)).is_some()
        {
            return None;
        }
        ranges.push((*first, *after));
    }
    for w in actual {
        let a = w.value();
        let first = a
            .attr("data-native-flow-first-glyph")?
            .parse::<usize>()
            .ok()?;
        let after = a
            .attr("data-native-flow-after-glyph")?
            .parse::<usize>()
            .ok()?;
        let (raw, b) = records.remove(&(first, after))?;
        let number = |s| a.attr(s)?.parse::<f32>().ok();
        let legacy = [
            number("xmin")?,
            number("ymin")?,
            number("xmax")?,
            number("ymax")?,
        ];
        if b.map(|x| x as f32) != legacy || w.text().collect::<String>() != *raw {
            return None;
        }
        words.push((raw.clone(), legacy));
    }
    if !records.is_empty() {
        return None;
    }
    ranges.sort_unstable();
    let mut at = 0;
    for (first, after) in ranges {
        if first != at {
            return None;
        }
        at = after;
    }
    if at != c.glyphs {
        return None;
    }
    if c.rectangles
        .iter()
        .chain(&c.rulings)
        .chain(&c.rectangle_obstacles)
        .chain(&c.images)
        .chain(&c.fills)
        .any(|&b| !valid(b))
    {
        return None;
    }
    // Every omitted rectangle must retain its source inventory classification.
    for &rect in &c.rectangles {
        if c.rulings.iter().filter(|&&r| contains(r, rect)).count()
            + c.rectangle_obstacles.iter().filter(|&&r| r == rect).count()
            != 1
        {
            return None;
        }
    }
    for &r in &c.rulings {
        if !c.rectangles.iter().any(|&b| contains(r, b)) {
            return None;
        }
    }
    let mut obstacles = c.images;
    obstacles.extend(c.fills);
    obstacles.extend(c.rectangle_obstacles);
    let mut lines = vec![];
    for (center, paint) in c.lines {
        if !valid(paint) || !center.iter().all(|x| x.is_finite()) || !contains(paint, center) {
            return None;
        }
        let [x0, y0, x1, y1] = center.map(|x| x as f32);
        let ruled = if x0 == x1 && y0 < y1 {
            rules.vertical.iter().any(|v| {
                paint[0] <= v.x as f64 && v.x as f64 <= paint[2] && v.y0 <= y0 && y1 <= v.y1
            })
        } else if y0 == y1 && x0 < x1 {
            rules.horizontal.iter().any(|h| {
                paint[1] <= h.y as f64 && h.y as f64 <= paint[3] && h.x0 <= x0 && x1 <= h.x1
            })
        } else {
            return None;
        };
        lines.push((center, paint, ruled));
    }
    Some(Scene {
        words,
        obstacles,
        lines,
    })
}
impl Scene {
    pub(crate) fn unobstructed(&self, b: [f32; 4]) -> bool {
        let b = b.map(f64::from);
        let overlaps = |a: Rect| a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3];
        valid(b) && self.obstacles.iter().all(|&a| !overlaps(a))
            && self.lines.iter().all(|&(_,paint,_)| !overlaps(paint))
    }
    pub(crate) fn matches(&self, words: &[Word]) -> bool {
        if words.len() != self.words.len() || words.iter().enumerate().any(|(id, w)| w.id != id) {
            return false;
        }
        let mut expected = std::collections::BTreeMap::new();
        for (raw, b) in &self.words {
            *expected
                .entry((raw.as_str(), b.map(f32::to_bits)))
                .or_insert(0usize) += 1;
        }
        for w in words {
            let Some(count) = expected.get_mut(&(w.raw.as_str(), w.source_box.map(f32::to_bits)))
            else {
                return false;
            };
            if *count == 0 {
                return false;
            }
            *count -= 1;
        }
        expected.values().all(|n| *n == 0)
    }
    pub(crate) fn clear(&self, b: [f32; 4], grid: &crate::grid::Grid) -> bool {
        let b = b.map(f64::from);
        let overlaps = |a: Rect| a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3];
        valid(b)
            && self.obstacles.iter().all(|&a| !overlaps(a))
            && self.lines.iter().all(|&(center, paint, ruled)| {
                if !overlaps(paint) {
                    return true;
                }
                // Membership in the page's line inventory alone is insufficient:
                // the actual painted stroke must support a boundary of this grid.
                // Its physical width supplies the interval; no numeric tolerance.
                ruled
                    && if center[0] == center[2] {
                        grid.xs
                            .iter()
                            .any(|&x| paint[0] <= x as f64 && x as f64 <= paint[2])
                    } else {
                        grid.ys
                            .iter()
                            .any(|&y| paint[1] <= y as f64 && y as f64 <= paint[3])
                    }
            })
    }
    #[cfg(test)]
    pub(crate) fn for_test(words: &[Word]) -> Self {
        Self {
            words: words
                .iter()
                .map(|w| (w.raw.clone(), w.source_box))
                .collect(),
            obstacles: vec![],
            lines: vec![],
        }
    }
}
