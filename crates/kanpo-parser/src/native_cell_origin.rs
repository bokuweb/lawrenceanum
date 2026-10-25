use crate::native_word_flow::{NativeFlow, NativeMode};
#[derive(Debug, Clone, Copy)]
pub(crate) struct Origin {
    pub legacy: [f32; 4],
    pub bounds: [f32; 4],
    pub flow: NativeFlow,
    pub range: (usize, usize, usize),
    pub sizes: [f64; 2],
}
impl Origin {
    pub(crate) fn horizontal_source(
        self,
        legacy: [f32; 4],
    ) -> Option<(NativeFlow, (usize, usize, usize))> {
        (self.legacy == legacy && self.flow.belongs(self.bounds)).then_some((self.flow, self.range))
    }
}
pub(crate) fn from_word(word: scraper::ElementRef<'_>, total: Option<usize>) -> Option<Origin> {
    let a = word.value();
    let total = total?;
    let get = |s| a.attr(s)?.parse::<f32>().ok();
    let legacy = [get("xmin")?, get("ymin")?, get("xmax")?, get("ymax")?];
    let raw = a.attr("data-native-cell-origin")?;
    let v = raw.split_whitespace().collect::<Vec<_>>();
    if v.len() != 15
        || v[0] != "2"
        || a.attr("data-wmode") != Some("0")
        || crate::native_word_axes::from_word(word) != crate::native_word_axes::NativeAxis::Right
    {
        return None;
    }
    let first = v[1].parse::<usize>().ok()?;
    let after = v[2].parse::<usize>().ok()?;
    if first >= after || after > total {
        return None;
    }
    let n = v[3..]
        .iter()
        .map(|s| s.parse::<f64>().ok())
        .collect::<Option<Vec<_>>>()?;
    if n.iter().any(|n| !n.is_finite()) {
        return None;
    }
    let sizes = [n[10], n[11]];
    if sizes.iter().any(|n| *n <= 0.) {
        return None;
    }
    let bounds = [n[6] as f32, n[7] as f32, n[8] as f32, n[9] as f32];
    let flow = NativeFlow {
        mode: Some(NativeMode::Horizontal),
        start: n[0],
        baseline: n[1],
        end: n[2],
        leading: n[3],
        trailing: n[4],
        prefix: n[5],
    };
    if !legacy.iter().all(|n| n.is_finite())
        || legacy[0] >= legacy[2]
        || legacy[1] >= legacy[3]
        || bounds[0] > legacy[0]
        || bounds[1] > legacy[1]
        || legacy[2] > bounds[2]
        || legacy[3] > bounds[3]
        || !flow.belongs(bounds)
    {
        return None;
    }
    Some(Origin {
        legacy,
        bounds,
        flow,
        range: (first, after, total),
        sizes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(origin: &str, total: Option<usize>) -> Option<Origin> {
        let raw=format!("<word xmin='10' ymin='93' xmax='34' ymax='102' data-wmode='0' data-native-axis='right' data-native-cell-origin='{origin}' data-native-flow-start='10' data-native-flow-baseline='100' data-native-flow-end='34' data-native-flow-leading='8' data-native-flow-trailing='0' data-native-flow-prefix='0' data-native-flow-first-glyph='0' data-native-flow-after-glyph='3'>ab-cd</word>");
        let h = scraper::Html::parse_fragment(&raw);
        from_word(
            h.select(&scraper::Selector::parse("word").unwrap())
                .next()
                .unwrap(),
            total,
        )
    }
    const GOOD: &str = "2 0 3 10 100 34 8 0 0 10 92.84 34 102 8 8";
    #[test]
    fn original_envelope_and_content_baseline_are_separate() {
        let o = parse(GOOD, Some(3)).unwrap();
        assert_eq!(o.legacy, [10., 93., 34., 102.]);
        assert_eq!(o.bounds, [10., 92.84, 34., 102.]);
        assert_eq!(o.flow.baseline, 100.);
        assert_eq!(o.range, (0, 3, 3));
    }
    #[test]
    fn malformed_new_origin_does_not_borrow_an_old_complete_flow() {
        for s in [
            "",
            "3 0 3 10 100 34 8 0 0 10 92.84 34 102 8 8",
            "2 0 99 10 100 34 8 0 0 10 92.84 34 102 8 8",
            "2 0 3 10 NaN 34 8 0 0 10 92.84 34 102 8 8",
            "2 0 3 10 100 34 8 0 0 11 92.84 34 102 8 8",
        ] {
            assert!(parse(s, Some(3)).is_none(), "{s}");
        }
    }
    #[test]
    fn original_page_count_is_required_to_bound_the_origin() {
        assert!(parse(GOOD, None).is_none());
        assert!(parse(GOOD, Some(2)).is_none());
    }
    #[test]
    fn original_source_box_is_bound_before_any_display_projection() {
        let o = parse(GOOD, Some(3)).unwrap();
        assert!(o.horizontal_source([10., 93., 34., 102.]).is_some());
        assert!(o.horizontal_source([11., 93., 35., 102.]).is_none());
    }
}

#[cfg(test)]
mod strict_evidence_tests {
    #[test]
    fn legacy_complete_flow_without_full_original_envelope_is_not_cell_evidence() {
        let raw = "<word xmin='10' ymin='93' xmax='34' ymax='102' data-native-xmin='10' data-native-ymin='93' data-native-xmax='34' data-native-ymax='102' data-wmode='0' data-native-axis='right' data-native-flow-start='10' data-native-flow-baseline='100' data-native-flow-end='34' data-native-flow-leading='8' data-native-flow-trailing='0' data-native-flow-prefix='0' data-native-flow-first-glyph='0' data-native-flow-after-glyph='3'>arbitrary</word>";
        let h = scraper::Html::parse_fragment(raw);
        let selector = scraper::Selector::parse("word").unwrap();
        let word = h.select(&selector).next().unwrap();
        assert!(
            crate::native_column_flow::from_word(word, Some(3)).is_some(),
            "legacy evidence really is otherwise complete"
        );
        assert!(
            super::from_word(word, Some(3)).is_none(),
            "legacy rectangles do not certify the full original glyph envelope"
        );
    }
}

#[cfg(test)]
mod size_evidence_tests {
    #[test]
    fn missing_or_invalid_original_endpoint_sizes_are_unknown() {
        for sizes in ["", "0 8", "8 -1", "NaN 8", "8 inf"] {
            let raw=format!("<word xmin='10' ymin='93' xmax='34' ymax='102' data-wmode='0' data-native-axis='right' data-native-cell-origin='2 0 3 10 100 34 8 0 0 10 92.84 34 102 {sizes}'>arbitrary</word>");
            let h = scraper::Html::parse_fragment(&raw);
            let sel = scraper::Selector::parse("word").unwrap();
            assert!(super::from_word(h.select(&sel).next().unwrap(), Some(3)).is_none());
        }
    }
}
