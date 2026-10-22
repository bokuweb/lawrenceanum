use crate::trace::NativeBounds;

pub(crate) fn from_word(word: scraper::ElementRef<'_>) -> NativeBounds {
    let attrs = word.value();
    let keys = [
        "data-native-xmin",
        "data-native-ymin",
        "data-native-xmax",
        "data-native-ymax",
    ];
    if !keys.iter().any(|key| attrs.attr(key).is_some()) {
        return match attrs.attr("data-wmode") {
            None => NativeBounds::Compatibility,
            Some("0") => {
                let values: Option<Vec<f32>> = ["xmin", "ymin", "xmax", "ymax"]
                    .iter()
                    .map(|key| attrs.attr(key)?.parse().ok())
                    .collect();
                match values {
                    Some(v) if v.iter().all(|n| n.is_finite()) && v[0] < v[2] && v[1] < v[3] => {
                        NativeBounds::Glyphs([v[0], v[1], v[2], v[3]])
                    }
                    _ => NativeBounds::Unknown,
                }
            }
            _ => NativeBounds::Unknown,
        };
    }
    if !matches!(attrs.attr("data-wmode"), Some("0" | "1")) {
        return NativeBounds::Unknown;
    }
    let values: Option<Vec<f32>> = keys
        .iter()
        .map(|key| attrs.attr(key)?.parse().ok())
        .collect();
    let Some(v) = values else {
        return NativeBounds::Unknown;
    };
    if v.iter().all(|v| v.is_finite()) && v[0] < v[2] && v[1] < v[3] {
        NativeBounds::Glyphs([v[0], v[1], v[2], v[3]])
    } else {
        NativeBounds::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(attrs: &str) -> NativeBounds {
        let html = scraper::Html::parse_fragment(&format!("<word {attrs}>任意</word>"));
        from_word(
            html.select(&scraper::Selector::parse("word").unwrap())
                .next()
                .unwrap(),
        )
    }
    #[test]
    fn horizontal_source_boxes_are_retained_before_display_normalization() {
        assert_eq!(
            parse("xmin='12' ymin='23' xmax='45' ymax='31' data-wmode='0'"),
            NativeBounds::Glyphs([12., 23., 45., 31.])
        );
    }
    #[test]
    fn only_a_known_horizontal_mode_can_supply_implicit_native_geometry() {
        assert_eq!(
            parse("xmin='12' ymin='23' xmax='45' ymax='31'"),
            NativeBounds::Compatibility
        );
        for mode in ["1", "2", "unknown"] {
            assert_eq!(
                parse(&format!(
                    "xmin='12' ymin='23' xmax='45' ymax='31' data-wmode='{mode}'"
                )),
                NativeBounds::Unknown
            );
        }
    }
    #[test]
    fn invalid_horizontal_source_geometry_is_never_a_compatibility_fallback() {
        for attrs in [
            "xmin='NaN' ymin='23' xmax='45' ymax='31'",
            "xmin='12' ymin='23' xmax='inf' ymax='31'",
            "xmin='12' ymin='23' xmax='12' ymax='31'",
            "xmin='12' ymin='23' xmax='45' ymax='23'",
            "xmin='45' ymin='23' xmax='12' ymax='31'",
            "xmin='12' ymin='23' ymax='31'",
        ] {
            assert_eq!(
                parse(&format!("{attrs} data-wmode='0'")),
                NativeBounds::Unknown
            );
        }
    }
    #[test]
    fn partial_explicit_geometry_does_not_fall_back_to_horizontal_attributes() {
        assert_eq!(
            parse("xmin='12' ymin='23' xmax='45' ymax='31' data-wmode='0' data-native-xmin='12'"),
            NativeBounds::Unknown
        );
    }
    #[test]
    fn complete_explicit_geometry_keeps_priority_for_both_writing_modes() {
        for mode in ["0", "1"] {
            assert_eq!(parse(&format!("xmin='12' ymin='23' xmax='45' ymax='31' data-wmode='{mode}' data-native-xmin='50' data-native-ymin='60' data-native-xmax='70' data-native-ymax='80'")),NativeBounds::Glyphs([50.,60.,70.,80.]));
        }
    }
}
