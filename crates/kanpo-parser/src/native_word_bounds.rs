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
        return if attrs.attr("data-wmode").is_none_or(|mode| mode == "0") {
            NativeBounds::Compatibility
        } else {
            NativeBounds::Unknown
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
