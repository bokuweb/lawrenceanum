//! A separate source certificate for several original baseline coordinates.
//! It never creates or rounds an exact NativeFlow. Private TDD stage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Band {
    pub(crate) start: f64,
    pub(crate) min_baseline: f64,
    pub(crate) max_baseline: f64,
    pub(crate) end: f64,
    pub(crate) leading: f64,
    pub(crate) trailing: f64,
    pub(crate) prefix: f64,
    pub(crate) projection: [f32; 4],
    pub(crate) glyph_bounds: [f64; 4],
    pub(crate) first: usize,
    pub(crate) after: usize,
    pub(crate) glyph_count: usize,
}
fn canonical(value: f64) -> Option<f32> {
    value
        .is_finite()
        .then(|| format!("{value:.6}").parse::<f32>().ok())
        .flatten()
        .filter(|v| v.is_finite())
}
impl Band {
    pub(crate) fn belongs(self, b: [f32; 4]) -> bool {
        let r = self.glyph_bounds;
        self.projection == b
            && b.iter().all(|v| v.is_finite())
            && b[0] < b[2]
            && b[1] < b[3]
            && self.first < self.after
            && self.after <= self.glyph_count
            && [
                self.start,
                self.min_baseline,
                self.max_baseline,
                self.end,
                self.leading,
                self.trailing,
                self.prefix,
                self.end + self.trailing,
                self.leading + self.prefix,
            ]
            .iter()
            .all(|v| v.is_finite())
            && self.start < self.end
            && self.leading > 0.
            && self.leading <= self.end - self.start
            && r.iter().all(|v| v.is_finite())
            && r[0] < r[2]
            && r[1] < r[3]
            && self.min_baseline <= self.max_baseline
            && r[1] <= self.min_baseline
            && self.max_baseline <= r[3]
            && canonical(r[0]) == Some(b[0])
            && canonical(r[2]) == Some(b[2])
            && canonical(r[1]).is_some_and(|y| y <= b[1])
            && canonical(r[3]).is_some_and(|y| y >= b[3])
            // One source glyph has one origin, one complete advance and one
            // envelope. Unicode length does not determine the glyph count.
            && (self.after-self.first!=1 || (self.min_baseline==self.max_baseline
                &&self.leading==self.end-self.start
                &&canonical(r[1])==Some(b[1])&&canonical(r[3])==Some(b[3])))
    }
}
pub(crate) fn from_word(word: scraper::ElementRef<'_>, glyph_count: Option<usize>) -> Option<Band> {
    let a = word.value();
    if a.attr("data-wmode") != Some("0")
        || crate::native_word_axes::from_word(word) != crate::native_word_axes::NativeAxis::Right
    {
        return None;
    }
    let crate::trace::NativeBounds::Glyphs(bounds) = crate::native_word_bounds::from_word(word)
    else {
        return None;
    };
    for field in [
        "start",
        "baseline",
        "end",
        "leading",
        "trailing",
        "prefix",
        "first-glyph",
        "after-glyph",
    ] {
        if a.attr(&format!("data-native-flow-{field}")).is_some() {
            return None;
        }
    }
    let styles = a.attr("data-native-styles")?;
    let mut count = 0;
    for style in styles.split_whitespace() {
        let (font, size) = style.split_once(':')?;
        font.parse::<usize>().ok()?;
        let size = f64::from_bits(u64::from_str_radix(size, 16).ok()?);
        if !size.is_finite() || size <= 0. {
            return None;
        }
        count += 1;
    }
    if count == 0 {
        return None;
    }
    let fields = a
        .attr("data-native-advance-band")?
        .split_whitespace()
        .collect::<Vec<_>>();
    if fields.len() != 18 || fields[0] != "1" {
        return None;
    }
    let first = fields[1].parse::<usize>().ok()?;
    let after = fields[2].parse::<usize>().ok()?;
    let n = fields[3..]
        .iter()
        .map(|v| v.parse::<f64>().ok().filter(|v| v.is_finite()))
        .collect::<Option<Vec<_>>>()?;
    let projection = [n[7] as f32, n[8] as f32, n[9] as f32, n[10] as f32];
    let band = Band {
        first,
        after,
        glyph_count: glyph_count?,
        start: n[0],
        min_baseline: n[1],
        max_baseline: n[2],
        end: n[3],
        leading: n[4],
        trailing: n[5],
        prefix: n[6],
        projection,
        glyph_bounds: [n[11], n[12], n[13], n[14]],
    };
    band.belongs(bounds).then_some(band)
}
#[cfg(test)]
#[path = "native_advance_band_tests.rs"]
mod tests;
