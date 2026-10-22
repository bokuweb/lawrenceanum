#[test]
fn all_twelve_original_bank_page_enclosures_own_every_source_word_once() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/archive/pages/20260803g00174/20260803g001740136.pdf");
    let inputs = crate::page_inputs(&std::fs::read(path).unwrap()).unwrap();
    let a = crate::analyze_item(&inputs);
    let parts = crate::notice_title_scope_tests::parts(&a, &inputs);
    let g: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/expected-company-article-sources/20260803g001740136.json"
    ))
    .unwrap();
    let expected: Vec<std::collections::HashSet<usize>> = g["expected_source_groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            v["source_word_ids"]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| id.as_u64().unwrap() as usize)
                .collect()
        })
        .collect();
    let got: Vec<std::collections::HashSet<usize>> =
        parts.iter().map(|p| p.iter().copied().collect()).collect();
    assert_eq!(
        got.len(),
        expected.len(),
        "two independently printed bank frames must not merge"
    );
    for group in expected {
        assert_eq!(
            got.iter().filter(|p| **p == group).count(),
            1,
            "an original enclosure's complete source membership changed"
        );
    }
    let all: std::collections::HashSet<_> = parts.iter().flatten().copied().collect();
    assert_eq!(all.len(), parts.iter().map(Vec::len).sum::<usize>());
    assert_eq!(
        all,
        a.pages[0]
            .words
            .iter()
            .filter(|w| matches!(w.status, crate::trace::WordStatus::Used))
            .map(|w| w.id)
            .collect()
    );
}

#[test]
fn review48_bank_font_bijection_and_emission_reversal_preserve_every_analysis_field() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/archive/pages/20260803g00174/20260803g001740136.pdf");
    let inputs = crate::page_inputs(&std::fs::read(path).unwrap()).unwrap();
    let a = crate::analyze_item(&inputs);
    assert_eq!(a.articles.len(), 12);
    let baseline = serde_json::to_value(a).unwrap();
    for (fonts, reverse) in [(true, false), (false, true), (true, true)] {
        let mut changed = inputs.clone();
        for p in &mut changed {
            if fonts {
                for attr in ["data-native-styles", "data-native-content-styles"] {
                    let needle = format!(" {attr}=\"");
                    let mut output = String::new();
                    let mut rest = p.xhtml.as_str();
                    while let Some(start) = rest.find(&needle) {
                        let from = start + needle.len();
                        let end = from + rest[from..].find('"').unwrap();
                        output.push_str(&rest[..from]);
                        let values: Vec<_> = rest[from..end]
                            .split_whitespace()
                            .map(|t| {
                                if t == "unknown" {
                                    t.to_string()
                                } else {
                                    let (f, b) = t.split_once(':').unwrap();
                                    format!("{}:{b}", usize::MAX - f.parse::<usize>().unwrap())
                                }
                            })
                            .collect();
                        output.push_str(&values.join(" "));
                        rest = &rest[end..];
                    }
                    output.push_str(rest);
                    p.xhtml = output;
                }
            }
            if reverse {
                let mut words = vec![];
                let mut rest = p.xhtml.as_str();
                while let Some(start) = rest.find("<word ") {
                    let end = start + rest[start..].find("</word>").unwrap() + "</word>".len();
                    words.push(rest[start..end].to_string());
                    rest = &rest[end..];
                }
                words.reverse();
                let doc = scraper::Html::parse_document(&p.xhtml);
                let sel = scraper::Selector::parse("page").unwrap();
                let page = doc.select(&sel).next().unwrap();
                p.xhtml = format!(
                    "<doc><page width=\"{}\" height=\"{}\">{}</page></doc>",
                    page.value().attr("width").unwrap(),
                    page.value().attr("height").unwrap(),
                    words.join("")
                );
            }
        }
        assert!(
            baseline == serde_json::to_value(crate::analyze_item(&changed)).unwrap(),
            "fonts={fonts} reverse={reverse}"
        );
    }
}
