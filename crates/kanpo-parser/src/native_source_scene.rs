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
    #[serde(default)]
    word_obstacles: Vec<(usize, usize, Rect)>,
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
    if !matches!(c.version,1|2)
        || (c.version==1)!=c.word_obstacles.is_empty()
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
    let mut opaque = std::collections::BTreeMap::new();
    for &(first,after,b) in &c.word_obstacles {
        if first>=after||after>c.glyphs||!valid(b)||opaque.insert((first,after),b).is_some(){return None}
    }
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
        let (first,after,obstacle)=if let Some(source)=a.attr("data-native-source-occurrence"){
            if c.version!=2||a.attr("data-native-flow-first-glyph").is_some()||a.attr("data-native-flow-after-glyph").is_some(){return None}
            let v=source.split_whitespace().collect::<Vec<_>>();
            if v.len()!=7||v[0]!="1"{return None}
            let first=v[1].parse::<usize>().ok()?;let after=v[2].parse::<usize>().ok()?;
            let b=[v[3].parse().ok()?,v[4].parse().ok()?,v[5].parse().ok()?,v[6].parse().ok()?];
            if !valid(b)||opaque.remove(&(first,after))?!=b{return None}
            (first,after,Some(b))
        }else{
            let first=a.attr("data-native-flow-first-glyph")?.parse::<usize>().ok()?;
            let after=a.attr("data-native-flow-after-glyph")?.parse::<usize>().ok()?;
            if opaque.contains_key(&(first,after)){return None}
            (first,after,None)
        };
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
        if obstacle.is_some_and(|envelope|!contains(envelope,*b)){return None}
        words.push((raw.clone(), legacy));
    }
    if !records.is_empty() || !opaque.is_empty() {
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
    obstacles.extend(c.word_obstacles.into_iter().map(|(_,_,b)|b));
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
    /// Native em boxes stay within the original geometric frame. Its complete
    /// border paint may extend inward; that does not shrink the authored text
    /// capacity. All interior paint and opaque material still prevent proof.
    pub(crate) fn paragraph_frame(&self, b: [f32; 4]) -> Option<[f64; 2]> {
        let b=b.map(f64::from);
        if !valid(b) {return None;}
        let fences=self.lines.iter().filter(|&&(center,_,_)|
            center[1]==center[3] && center[0]<=b[0] && b[2]<=center[2]).collect::<Vec<_>>();
        let top=fences.iter().filter_map(|&&(center,_,_)|(center[1]<=b[1]).then_some(center[1])).max_by(f64::total_cmp)?;
        let bottom=fences.iter().filter_map(|&&(center,_,_)|(b[3]<=center[1]).then_some(center[1])).min_by(f64::total_cmp)?;
        // Never jump an unclassified nearer physical fence to borrow an
        // enclosing page/header fence. Coincident ambiguous paint also refuses.
        if top>=bottom||fences.iter().any(|&&(center,_,ruled)|
            (center[1]==top||center[1]==bottom)&&!ruled) {return None;}
        let frame=[b[0],top,b[2],bottom];
        let overlaps=|a:Rect|a[0]<frame[2]&&frame[0]<a[2]&&a[1]<frame[3]&&frame[1]<a[3];
        if self.obstacles.iter().any(|&a|overlaps(a)) {return None;}
        for &(center,paint,ruled) in &self.lines {
            if overlaps(paint)&&!(ruled&&center[1]==center[3]
                &&center[0]<=b[0]&&b[2]<=center[2]&&(center[1]==top||center[1]==bottom)) {return None;}
        }
        Some([top,bottom])
    }
    /// Exclude only the actual paint of complete original grid boundaries.
    /// Opaque glyphs, images and fills are checked over the whole original strip.
    /// Callers must retain their complete glyph bounds inside the returned area.
    pub(crate) fn interior_strip(&self,b:[f32;4],grid:&crate::grid::Grid)->Option<[f32;4]>{
        if grid.xs.len()<2||grid.ys.len()<2||grid.xs.iter().chain(&grid.ys).any(|v|!v.is_finite())
            ||grid.xs.windows(2).any(|w|w[0]>=w[1])||grid.ys.windows(2).any(|w|w[0]>=w[1]){return None}
        let overlaps=|a:Rect|a[0]<(b[2]as f64)&&(b[0]as f64)<a[2]&&a[1]<(b[3]as f64)&&(b[1]as f64)<a[3];
        if !valid(b.map(f64::from))||self.obstacles.iter().any(|&a|overlaps(a)){return None}
        let upper=|x:f64|{let v=x as f32;if (v as f64)<x{v.next_up()}else{v}};
        let lower=|x:f64|{let v=x as f32;if (v as f64)>x{v.next_down()}else{v}};
        let mut out=b;
        for &(center,paint,_) in &self.lines{
            if center[0]==center[2]&&center[1]==grid.top()as f64&&center[3]==grid.bottom()as f64
                && grid.xs.iter().any(|&x|x as f64==center[0]){
                if center[0]==b[0]as f64{out[0]=out[0].max(upper(paint[2]));}
                if center[0]==b[2]as f64{out[2]=out[2].min(lower(paint[0]));}
            }
            if center[1]==center[3]&&center[0]==grid.left()as f64&&center[2]==grid.right()as f64
                && grid.ys.iter().any(|&y|y as f64==center[1]){
                if center[1]==b[1]as f64{out[1]=out[1].max(upper(paint[3]));}
                if center[1]==b[3]as f64{out[3]=out[3].min(lower(paint[1]));}
            }
        }
        valid(out.map(f64::from)).then_some(out)
    }
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

#[cfg(test)]
#[path="native_scene_interior_tests.rs"]
mod interior_tests;

#[cfg(test)]
#[path="native_scene_occurrence_tests.rs"]
mod occurrence_tests;
