//! Typography of alphanumeric source glyphs, kept separate from all glyphs.
use crate::trace::{ContentStyles, NativeStyles};

pub(crate) fn from_word(
    word: scraper::ElementRef<'_>,
    all: Option<&NativeStyles>,
) -> ContentStyles {
    let Some(value) = word.value().attr("data-native-content-styles") else {
        return ContentStyles::Compatibility;
    };
    let Some(all) = all else {
        return ContentStyles::Unknown;
    };
    let mut styles = NativeStyles::new();
    for token in value.split_whitespace() {
        let Some((font, bits)) = token.split_once(':') else {
            return ContentStyles::Unknown;
        };
        let (Ok(font), Ok(bits)) = (font.parse::<usize>(), u64::from_str_radix(bits, 16)) else {
            return ContentStyles::Unknown;
        };
        let size = f64::from_bits(bits);
        if !size.is_finite() || size <= 0. || !all.contains(&(font, bits)) {
            return ContentStyles::Unknown;
        }
        styles.insert((font, bits));
    }
    ContentStyles::Glyphs(styles)
}
