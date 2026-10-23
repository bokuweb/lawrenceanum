use crate::native_word_axes::NativeAxis;
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum NativeRuling {
    Unknown,
    Horizontal([f64; 4]),
    Vertical([f64; 4]),
}
impl NativeRuling {
    pub(crate) fn rect(self) -> Option<[f64; 4]> {
        let b = match self {
            Self::Unknown => return None,
            Self::Horizontal(b) | Self::Vertical(b) => b,
        };
        (b.iter().all(|n| n.is_finite()) && b[0] < b[2] && b[1] < b[3]).then_some(b)
    }
    pub(crate) fn matches_axis(self, axis: NativeAxis) -> bool {
        matches!(
            (self, axis),
            (Self::Horizontal(_), NativeAxis::Right | NativeAxis::Left)
                | (Self::Vertical(_), NativeAxis::Down | NativeAxis::Up)
        )
    }
}
pub(crate) fn from_word(word: scraper::ElementRef<'_>) -> NativeRuling {
    let attrs = word.value();
    let values = [
        "data-native-ink-xmin",
        "data-native-ink-ymin",
        "data-native-ink-xmax",
        "data-native-ink-ymax",
    ]
    .map(|key| attrs.attr(key).and_then(|v| v.parse::<f64>().ok()));
    let [Some(x0), Some(y0), Some(x1), Some(y1)] = values else {
        return NativeRuling::Unknown;
    };
    let ruling = match attrs.attr("data-native-ruling") {
        Some("horizontal") => NativeRuling::Horizontal([x0, y0, x1, y1]),
        Some("vertical") => NativeRuling::Vertical([x0, y0, x1, y1]),
        _ => return NativeRuling::Unknown,
    };
    if ruling.rect().is_some() && ruling.matches_axis(crate::native_word_axes::from_word(word)) {
        ruling
    } else {
        NativeRuling::Unknown
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn parse(attrs: &str) -> NativeRuling {
        let h = scraper::Html::parse_fragment(&format!("<word {attrs}>任意</word>"));
        from_word(
            h.select(&scraper::Selector::parse("word").unwrap())
                .next()
                .unwrap(),
        )
    }
    #[test]
    fn complete_directional_source_ink_bounds_keep_f64_precision() {
        let tag="data-native-ruling='horizontal' data-native-axis='right' data-native-ink-xmin='10.00000000001' data-native-ink-ymin='20' data-native-ink-xmax='30' data-native-ink-ymax='21'";
        assert_eq!(
            parse(tag),
            NativeRuling::Horizontal([10.00000000001, 20., 30., 21.])
        );
        let tag = tag.replace("horizontal", "vertical").replace("right", "up");
        assert_eq!(
            parse(&tag),
            NativeRuling::Vertical([10.00000000001, 20., 30., 21.])
        );
    }
    #[test]
    fn absent_partial_invalid_or_inconsistent_metadata_never_uses_a_plausible_legacy_box() {
        let valid="data-native-ruling='horizontal' data-native-axis='right' data-native-ink-xmin='10' data-native-ink-ymin='20' data-native-ink-xmax='30' data-native-ink-ymax='21'";
        for tag in [
            String::new(),
            valid.replace("data-native-axis='right'", ""),
            valid.replace("right", "down"),
            valid.replace("horizontal", "unknown"),
            valid.replace("data-native-ink-ymax='21'", ""),
            valid.replace("='10'", "='NaN'"),
            valid.replace("='30'", "='inf'"),
            valid.replace("='30'", "='9'"),
        ] {
            assert_eq!(
                parse(&format!("xmin=10 ymin=20 xmax=30 ymax=21 {tag}")),
                NativeRuling::Unknown
            );
        }
    }
}
