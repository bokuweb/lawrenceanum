//! Device axes supplied by the PDF interpreter, independent of text occupancy.
pub(crate) fn device(page: scraper::ElementRef<'_>) -> Option<[f64; 2]> {
    let attr = page.value();
    let parts = attr
        .attr("data-native-device-size")?
        .split_whitespace()
        .collect::<Vec<_>>();
    if parts.len() != 3 || parts[0] != "1" {
        return None;
    }
    let size = [parts[1].parse::<f64>().ok()?, parts[2].parse::<f64>().ok()?];
    let paper = [
        attr.attr("width")?.parse::<f64>().ok()?,
        attr.attr("height")?.parse::<f64>().ok()?,
    ];
    if !size
        .iter()
        .chain(&paper)
        .all(|v| v.is_finite() && *v > 0. && (*v as f32).is_finite())
    {
        return None;
    }
    [paper, [paper[1], paper[0]]]
        .iter()
        .any(|p| p.map(|v| v as f32) == size.map(|v| v as f32))
        .then_some(size)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(attrs: &str, words: &str) -> Option<[f64; 2]> {
        let dom = scraper::Html::parse_document(&format!("<page {attrs}>{words}</page>"));
        device(
            dom.select(&scraper::Selector::parse("page").unwrap())
                .next()
                .unwrap(),
        )
    }
    #[test]
    fn native_axes_do_not_depend_on_empty_dense_or_asymmetric_word_occupancy() {
        for words in [
            "",
            "<word xmin='10' xmax='20'>A</word>",
            "<word xmin='-100' xmax='2000'>arbitrary crop</word>",
        ] {
            assert_eq!(
                parse(
                    "width='595' height='842' data-native-device-size='1 842 595'",
                    words
                ),
                Some([842., 595.])
            );
            assert_eq!(
                parse(
                    "width='595' height='842' data-native-device-size='1 595 842'",
                    words
                ),
                Some([595., 842.])
            );
        }
    }
    #[test]
    fn unknown_partial_nonfinite_or_incompatible_device_metadata_is_not_a_guess() {
        for size in [
            "",
            "1",
            "2 842 595",
            "1 842",
            "1 NaN 595",
            "1 inf 595",
            "1 0 595",
            "1 -842 595",
            "1 600 900",
            "1 842 595 extra",
        ] {
            assert!(parse(
                &format!("width='595' height='842' data-native-device-size='{size}'"),
                ""
            )
            .is_none());
        }
        assert!(parse("data-native-device-size='1 842 595'", "").is_none());
        assert!(parse("width='595' height='842'", "").is_none());
    }
}
