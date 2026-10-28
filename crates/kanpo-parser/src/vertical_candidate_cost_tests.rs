//! Keep the pre-optimization grammar as an independent equivalence oracle.
use super::warichu_spaced_part;
fn previous(text: &str, extended: bool) -> bool {
    let name = |text: &str| {
        let mut chars = text.chars();
        matches!((chars.next(), chars.next()), (Some(c), None)
            if ('\u{4E00}'..='\u{9FFF}').contains(&c) || ('\u{3041}'..='\u{30FF}').contains(&c))
    };
    let comma = |c| matches!(c, '、' | '・' | '，' | '︑' | '︐');
    let chars: Vec<_> = text.chars().collect();
    name(text)
        || (chars.len() == 2 && name(&chars[0].to_string())
            && (matches!(chars[1], '、' | '・' | '，') || (extended && comma(chars[1]))))
        || (extended && chars.len() == 3 && name(&chars[0].to_string())
            && comma(chars[1]) && name(&chars[2].to_string()))
}
#[test]
fn grammar_matches_previous_for_unicode_classes_and_all_short_sequences() {
    let alphabet = ['一', 'ぁ', 'ヿ', '、', '・', '，', '︑', '︐', 'a', ' ', '\u{3040}', '\u{3100}', '\u{4DFF}', '\u{A000}', '🦀'];
    let mut strings = vec![String::new()];
    for _ in 0..4 {
        let previous_layer = strings.clone();
        strings = previous_layer.iter().flat_map(|s| alphabet.iter().map(move |c| format!("{s}{c}"))).collect();
        for text in &strings {
            for extended in [false, true] {
                assert_eq!(warichu_spaced_part(text, extended), previous(text, extended), "{text:?}, extended={extended}");
            }
        }
    }
    for text in [String::new(), "一".repeat(100_000), format!("一、{}", "あ".repeat(100_000)), "🦀".repeat(100_000)] {
        for extended in [false, true] {
            assert_eq!(warichu_spaced_part(&text, extended), previous(&text, extended));
        }
    }
}
