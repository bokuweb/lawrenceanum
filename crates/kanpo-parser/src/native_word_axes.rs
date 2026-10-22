#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeAxis {
    Compatibility,
    Unknown,
    Right,
    Down,
    Left,
    Up,
}
pub(crate) fn from_word(word: scraper::ElementRef<'_>) -> NativeAxis {
    match word.value().attr("data-native-axis") {
        None => NativeAxis::Compatibility,
        Some("right") => NativeAxis::Right,
        Some("down") => NativeAxis::Down,
        Some("left") => NativeAxis::Left,
        Some("up") => NativeAxis::Up,
        _ => NativeAxis::Unknown,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn parse(s: &str) -> NativeAxis {
        let h = scraper::Html::parse_fragment(&format!("<word {s}>任意</word>"));
        from_word(
            h.select(&scraper::Selector::parse("word").unwrap())
                .next()
                .unwrap(),
        )
    }
    #[test]
    fn only_explicit_declared_source_axes_are_known() {
        for (s, a) in [
            ("right", NativeAxis::Right),
            ("down", NativeAxis::Down),
            ("left", NativeAxis::Left),
            ("up", NativeAxis::Up),
        ] {
            assert_eq!(parse(&format!("data-native-axis='{s}'")), a);
        }
    }
    #[test]
    fn absent_and_invalid_axes_are_distinct_and_never_inferred_from_aspect_or_font_mode() {
        assert_eq!(
            parse("xmin=1 ymin=2 xmax=1000 ymax=3 data-wmode=0"),
            NativeAxis::Compatibility
        );
        for s in ["", "unknown", "RIGHT", "right down", "NaN", "0"] {
            assert_eq!(
                parse(&format!(
                    "data-native-axis='{s}' data-wmode=1 data-advance-x=0 data-advance-y=8"
                )),
                NativeAxis::Unknown
            );
        }
    }
}
