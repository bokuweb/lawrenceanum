#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct NativeFlow {
    pub(crate) start: f64,
    pub(crate) baseline: f64,
    pub(crate) end: f64,
    pub(crate) leading: f64,
    pub(crate) trailing:f64,
    pub(crate) prefix:f64,
}
impl NativeFlow {
    pub(crate) fn belongs(self, b: [f32; 4]) -> bool {
        [self.start, self.baseline, self.end, self.leading, self.trailing, self.prefix, self.end+self.trailing, self.leading+self.prefix]
            .iter()
            .all(|n| n.is_finite())
            && self.start < self.end
            && self.leading > 0.
            && self.leading <= self.end - self.start
            && b.iter().all(|n| n.is_finite())
            && b[0] < b[2]
            && b[1] < b[3]
            && self.start as f32 == b[0]
            && self.end as f32 == b[2]
            && b[1] <= self.baseline as f32
            && self.baseline as f32 <= b[3]
    }
}
pub(crate) fn from_word(word: scraper::ElementRef<'_>) -> Option<NativeFlow> {
    if crate::native_word_axes::from_word(word) != crate::native_word_axes::NativeAxis::Right {
        return None;
    }
    let crate::trace::NativeBounds::Glyphs(b) = crate::native_word_bounds::from_word(word) else {
        return None;
    };
    let attrs = word.value();
    let num = |key| attrs.attr(key)?.parse::<f64>().ok();
    let flow = NativeFlow{
        start: num("data-native-flow-start")?,
        baseline: num("data-native-flow-baseline")?,
        end: num("data-native-flow-end")?,
        leading: num("data-native-flow-leading")?, trailing:num("data-native-flow-trailing")?, prefix:num("data-native-flow-prefix")? };
    flow.belongs(b).then_some(flow)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn parse(attrs: &str) -> Option<NativeFlow> {
        let s=format!("<word xmin='10' ymin='90' xmax='30' ymax='105' data-wmode='0' data-native-axis='right' {attrs}>任意</word>");
        let h = scraper::Html::parse_fragment(&s);
        from_word(
            h.select(&scraper::Selector::parse("word").unwrap())
                .next()
                .unwrap(),
        )
    }
    const GOOD:&str="data-native-flow-start='10' data-native-flow-baseline='100.00000000001' data-native-flow-end='30' data-native-flow-leading='8' data-native-flow-trailing='0' data-native-flow-prefix='0'";
    #[test]
    fn complete_explicit_source_flow_preserves_f64_baseline() {
        assert_eq!(
            parse(GOOD),
            Some(NativeFlow{
                start: 10.,
                baseline: 100.00000000001,
                end: 30.,
                leading: 8., trailing:0., prefix:0. })
        );
    }
    #[test]
    fn a_plausible_source_box_never_fills_missing_or_partial_origin_metadata() {
        assert_eq!(parse(""), None);
        for key in ["start", "baseline", "end", "leading", "trailing", "prefix"] {
            let bad = GOOD
                .split_whitespace()
                .filter(|s| !s.starts_with(&format!("data-native-flow-{key}=")))
                .collect::<Vec<_>>()
                .join(" ");
            assert_eq!(parse(&bad), None);
        }
    }
    #[test]
    fn malformed_nonfinite_and_nonpositive_flows_are_rejected() {
        for (from, to) in [
            ("='10'", "='NaN'"),
            ("='30'", "='inf'"),
            ("='8'", "='0'"),
            ("='8'", "='-1'"),
            ("='8'", "='21'"),
            ("='30'", "='9'"),
            ("='100.00000000001'", "='106'"),
            ("='100.00000000001'", "='89'"),
        ] {
            assert_eq!(parse(&GOOD.replace(from, to)), None);
        }
    }
    #[test]
    fn explicit_flow_does_not_belong_to_a_changed_original_rectangle() {
        assert_eq!(parse(&GOOD.replace("='10'", "='11'")), None);
        assert_eq!(parse(&GOOD.replace("='30'", "='31'")), None);
    }
    #[test]
    fn source_axis_and_known_native_bounds_are_required() {
        for axis in ["unknown", "left", "up", "down"] {
            let s=format!("<word xmin='10' ymin='90' xmax='30' ymax='105' data-wmode='0' data-native-axis='{axis}' {GOOD}>任意</word>");
            let h = scraper::Html::parse_fragment(&s);
            assert_eq!(
                from_word(
                    h.select(&scraper::Selector::parse("word").unwrap())
                        .next()
                        .unwrap()
                ),
                None
            );
        }
        for attrs in [
            "",
            "data-wmode='1'",
            "data-wmode='2'",
            "data-wmode='0' data-native-xmin='10'",
        ] {
            let s=format!("<word xmin='10' ymin='90' xmax='30' ymax='105' data-native-axis='right' {attrs} {GOOD}>任意</word>");
            let h = scraper::Html::parse_fragment(&s);
            assert_eq!(
                from_word(
                    h.select(&scraper::Selector::parse("word").unwrap())
                        .next()
                        .unwrap()
                ),
                None
            );
        }
    }
    #[test]
    fn native_geometry_is_independent_of_compatibility_display_geometry() {
        let s=format!("<word xmin='100' ymin='200' xmax='300' ymax='400' data-wmode='1' data-native-xmin='10' data-native-ymin='90' data-native-xmax='30' data-native-ymax='105' data-native-axis='right' {GOOD}>任意</word>");
        let h = scraper::Html::parse_fragment(&s);
        assert!(from_word(
            h.select(&scraper::Selector::parse("word").unwrap())
                .next()
                .unwrap()
        )
        .is_some());
    }
    #[test]
    fn arbitrary_body_vocabulary_does_not_classify_source_flow() {
        for text in ["x", "任意の説明", "別の用語12345"] {
            let s=format!("<word xmin='10' ymin='90' xmax='30' ymax='105' data-wmode='0' data-native-axis='right' {GOOD}>{text}</word>");
            let h = scraper::Html::parse_fragment(&s);
            assert!(from_word(
                h.select(&scraper::Selector::parse("word").unwrap())
                    .next()
                    .unwrap()
            )
            .is_some());
        }
    }
    #[test]
    fn original_flow_binding_rejects_foreign_and_invalid_rectangles() {
        let f = NativeFlow{
            start: 10.,
            baseline: 100.,
            end: 30.,
            leading: 8., trailing:0., prefix:0. };
        assert!(f.belongs([10., 90., 30., 105.]));
        for b in [
            [11., 90., 30., 105.],
            [10., 90., 31., 105.],
            [10., 101., 30., 105.],
            [10., 90., 30., 99.],
            [10., 90., 10., 105.],
            [f32::NAN, 90., 30., 105.],
        ] {
            assert!(!f.belongs(b));
        }
    }
#[test]fn tracking_and_TJ_adjustment_are_mandatory_finite_source_evidence(){for key in ["trailing","prefix"]{let needle=format!("data-native-flow-{key}='0'");let missing=GOOD.replace(&needle,"");assert_eq!(parse(&missing),None);for bad in ["NaN","inf","-inf"]{assert_eq!(parse(&GOOD.replace(&needle,&format!("data-native-flow-{key}='{bad}'"))),None);}}let f=parse(&GOOD.replace("data-native-flow-trailing='0'","data-native-flow-trailing='-4'").replace("data-native-flow-prefix='0'","data-native-flow-prefix='3'")).unwrap();assert_eq!((f.trailing,f.prefix),(-4.,3.));}
}
