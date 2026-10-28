//! Optional exact painted rectangles, bound to complete original Word ranges.
use crate::{native_terminal_glyph::Terminals, trace::Word};
use std::collections::BTreeMap;
pub(crate) fn for_words(
    page: scraper::ElementRef<'_>,
    words: &[Word],
    terminals: &Terminals,
) -> Vec<Option<[f64; 4]>> {
    let mut records = BTreeMap::new();
    for node in page.select(&scraper::Selector::parse("word").unwrap()) {
        let Some(raw) = node.value().attr("data-native-outline-rectangle") else {
            continue;
        };
        let values = raw.split_whitespace().collect::<Vec<_>>();
        let parsed = (|| {
            if values.len() != 12 || values[0] != "1" {
                return None;
            }
            let first = values[1].parse::<usize>().ok()?;
            let after = values[2].parse::<usize>().ok()?;
            let attrs = node.value();
            if attrs
                .attr("data-native-flow-first-glyph")?
                .parse::<usize>()
                .ok()?
                != first
                || attrs
                    .attr("data-native-flow-after-glyph")?
                    .parse::<usize>()
                    .ok()?
                    != after
            {
                return None;
            }
            let hex = values[3];
            if first >= after || hex.len() % 2 != 0 || !hex.is_ascii() {
                return None;
            }
            let raw = String::from_utf8(
                (0..hex.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
                    .collect::<Option<Vec<_>>>()?,
            )
            .ok()?;
            if raw != node.text().collect::<String>() {
                return None;
            }
            let numbers = values[4..]
                .iter()
                .map(|s| s.parse::<f64>().ok())
                .collect::<Option<Vec<_>>>()?;
            if numbers.iter().any(|n| !n.is_finite()) {
                return None;
            }
            let legacy: [f64; 4] = numbers[..4].try_into().ok()?;
            let ink: [f64; 4] = numbers[4..].try_into().ok()?;
            for (i, key) in ["xmin", "ymin", "xmax", "ymax"].iter().enumerate() {
                if attrs.attr(key)?.parse::<f32>().ok()? != legacy[i] as f32 {
                    return None;
                }
            }
            if legacy[0] >= legacy[2]
                || legacy[1] >= legacy[3]
                || ink[0] >= ink[2]
                || ink[1] >= ink[3]
                || ink[0] < legacy[0]
                || ink[1] < legacy[1]
                || ink[2] > legacy[2]
                || ink[3] > legacy[3]
            {
                return None;
            }
            Some(((first, after), (raw, legacy.map(|n| n as f32), ink)))
        })();
        if let Some((key, value)) = parsed {
            // Duplicated original occurrences have no unique certificate.
            records
                .entry(key)
                .and_modify(|v| *v = None)
                .or_insert(Some(value));
        }
    }
    words
        .iter()
        .map(|word| {
            let range = terminals.get(word)?.range_for(word)?;
            let (raw, legacy, ink) = records.get(&(range.start, range.end))?.as_ref()?;
            (raw == &word.raw && legacy == &word.source_box).then_some(*ink)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    const ORIGINAL: &[u8] =
        include_bytes!("../tests/data/native-amount-marks/20260902g001960127.pdf");
    #[test]
    fn an_outline_cannot_move_to_another_occurrence_of_the_same_raw_mark() {
        let mut inputs = crate::page_inputs(ORIGINAL).unwrap();
        let original = crate::analyze_item(&inputs);
        let words = &original.pages[0].words;
        let html = scraper::Html::parse_document(&inputs[0].xhtml);
        let nodes = html
            .select(&scraper::Selector::parse("word[data-native-outline-rectangle]").unwrap())
            .collect::<Vec<_>>();
        let first = nodes
            .iter()
            .find(|n| n.text().collect::<String>() == "\u{e61c}")
            .unwrap();
        let other = nodes
            .iter()
            .find(|n| {
                n.text().collect::<String>() == "\u{e61c}"
                    && n.value().attr("ymin") != first.value().attr("ymin")
            })
            .unwrap();
        let a = first.value().attr("data-native-outline-rectangle").unwrap();
        let b = other.value().attr("data-native-outline-rectangle").unwrap();
        let before = b.split_whitespace().collect::<Vec<_>>();
        let range = before[1].parse::<usize>().unwrap()..before[2].parse::<usize>().unwrap();
        let aa = format!(" data-native-outline-rectangle=\"{a}\"");
        let bb = format!(" data-native-outline-rectangle=\"{b}\"");
        inputs[0].xhtml = inputs[0].xhtml.replacen(&bb, "", 1).replacen(&aa, &bb, 1);
        let html = scraper::Html::parse_document(&inputs[0].xhtml);
        let page = html
            .select(&scraper::Selector::parse("page").unwrap())
            .next()
            .unwrap();
        let terminals = crate::native_terminal_glyph::for_words(page, words);
        let id = words
            .iter()
            .find(|w| terminals.get(w).and_then(|t| t.range_for(w)) == Some(range.clone()))
            .unwrap()
            .id;
        assert!(
            for_words(page, words, &terminals)[id].is_none(),
            "a transferred certificate is not its new node's original occurrence"
        );
    }
}
