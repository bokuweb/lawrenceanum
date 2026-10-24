use serde_json::Value;

const ORIGINAL: &[u8] = include_bytes!("../tests/data/native-unit-dash/20261016g002280433.pdf");

fn value<T: serde::Serialize>(v: &T) -> Value {
    serde_json::from_str(&serde_json::to_string(v).unwrap()).unwrap()
}

// The original source has one no-value glyph before unit Word0. The old complete
// result is retained as a compatibility oracle; this independently applies that
// original-source delta rather than regenerating a golden from the new parser.
fn expected() -> Value {
    let mut a: Value = serde_json::from_str(include_str!(
        "../tests/data/native-unit-dash/20261016g002280433-baseline104.json"
    ))
    .unwrap();
    let unit = a["pages"][0]["words"][0]["text"]
        .as_str()
        .unwrap()
        .to_string();
    let context = a["pages"][0]["regions"][0]["lines"][90]["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        a["pages"][0]["regions"][0]["lines"][91]["words"],
        serde_json::json!([0])
    );
    assert_eq!(a["pages"][0]["words"][12]["raw"], "\u{E61C}");
    a["pages"][0]["words"][12]["text"] = "－".into();
    a["pages"][0]["words"][12]["status"] = "used".into();
    a["pages"][0]["regions"][0]["lines"][91]["text"] = format!("－{unit}").into();
    a["pages"][0]["regions"][0]["lines"][91]["words"] = serde_json::json!([12, 0]);
    let inject = |text: &mut Value| {
        let s = text.as_str().unwrap();
        assert_eq!(s.matches(context.as_str()).count(), 1);
        let start = s.find(&context).unwrap() + context.len();
        let whitespace: usize = s[start..]
            .chars()
            .take_while(|c| c.is_whitespace())
            .map(char::len_utf8)
            .sum();
        let at = start + whitespace;
        assert!(s[at..].starts_with(&unit));
        let mut result = s.to_string();
        result.insert(at, '－');
        *text = result.into();
    };
    inject(&mut a["text"]);
    inject(&mut a["pages"][0]["text"]);
    inject(&mut a["articles"][0]["text"]);
    let mut document_runs = 0;
    for block in a["articles"][0]["document"]["blocks"]
        .as_array_mut()
        .unwrap()
    {
        if let Some(runs) = block.get_mut("runs").and_then(Value::as_array_mut) {
            for run in runs {
                if run["text"].as_str().is_some_and(|s| s.contains(&context)) {
                    inject(&mut run["text"]);
                    document_runs += 1;
                }
            }
        }
    }
    assert_eq!(document_runs, 1);
    a
}

#[test]
fn original_only_adds_its_source_dash_and_ownership_with_four_text_mirrors() {
    let a = crate::analyze_item(&crate::page_inputs(ORIGINAL).unwrap());
    assert!(
        value(&a) == expected(),
        "Only the original dash/ownership and its text mirrors may change"
    );
}

#[test]
fn actual_previous_and_next_pages_keep_their_complete_page_results() {
    let before = crate::page_inputs(include_bytes!(
        "../tests/data/native-unit-dash/20261016g002280432.pdf"
    ))
    .unwrap();
    let current = crate::page_inputs(ORIGINAL).unwrap();
    let after = crate::page_inputs(include_bytes!(
        "../tests/data/native-unit-dash/20261016g002280434.pdf"
    ))
    .unwrap();
    let before_single = crate::analyze_item(&before);
    let after_single = crate::analyze_item(&after);
    let pages: Vec<_> = before.into_iter().chain(current).chain(after).collect();
    let all = crate::analyze_item(&pages);
    assert!(value(&all.pages[0]) == value(&before_single.pages[0]));
    assert!(value(&all.pages[1]) == expected()["pages"][0]);
    assert!(value(&all.pages[2]) == value(&after_single.pages[0]));
}

#[test]
fn previous_global_or_local_comparison_does_not_change_source_quantity_ownership() {
    for prefix in [
        include_bytes!("../tests/data/ruled-marker-bands/20260724g001650782.pdf").as_slice(),
        include_bytes!("../tests/data/ruled-marker-bands/20260731g001720088.pdf").as_slice(),
    ] {
        let mut pages = crate::page_inputs(prefix).unwrap();
        let previous = crate::analyze_item(&pages);
        pages.extend(crate::page_inputs(ORIGINAL).unwrap());
        let a = crate::analyze_item(&pages);
        assert!(value(&a.pages[0]) == value(&previous.pages[0]));
        assert!(value(&a.pages[1]) == expected()["pages"][0]);
    }
}

#[test]
fn each_page_owns_its_original_occurrence_once_when_the_same_input_repeats() {
    let mut pages = crate::page_inputs(ORIGINAL).unwrap();
    pages.extend(crate::page_inputs(ORIGINAL).unwrap());
    let a = crate::analyze_item(&pages);
    for page in &a.pages {
        assert!(value(page) == expected()["pages"][0]);
    }
}
