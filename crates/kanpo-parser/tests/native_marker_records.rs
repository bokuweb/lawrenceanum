//! Complete original Analysis contracts, including source paint and stale/partial evidence controls.
use serde_json::{json, Value};
fn fixture(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/native-marker-records")
        .join(name)
}
fn inputs(name: &str) -> Vec<kanpo_parser::PageInput> {
    let file = if name == "original" {
        "source.pdf".into()
    } else {
        format!("{name}.pdf")
    };
    kanpo_parser::page_inputs(&std::fs::read(fixture(&file)).unwrap()).unwrap()
}
fn actual(inputs: &[kanpo_parser::PageInput]) -> Value {
    serde_json::from_str(&serde_json::to_string(&kanpo_parser::analyze_item(inputs)).unwrap())
        .unwrap()
}
fn gold(changed: bool) -> Value {
    serde_json::from_slice(
        &std::fs::read(fixture(if changed {
            "expected.json"
        } else {
            "baseline.json"
        }))
        .unwrap(),
    )
    .unwrap()
}
fn certificate(inputs: &[kanpo_parser::PageInput]) -> Value {
    let h = scraper::Html::parse_document(&inputs[0].xhtml);
    let p = h
        .select(&scraper::Selector::parse("page").unwrap())
        .next()
        .unwrap();
    serde_json::from_str(
        p.value()
            .attr("data-native-scene")
            .expect("original scene certificate"),
    )
    .unwrap()
}
fn rewrite(inputs: &mut [kanpo_parser::PageInput], value: Option<&Value>) {
    let s = &mut inputs[0].xhtml;
    let start = s.find(" data-native-scene=\"").unwrap();
    let from = start + " data-native-scene=\"".len();
    let end = from + s[from..].find('"').unwrap() + 1;
    let new = value
        .map(|v| {
            format!(
                " data-native-scene=\"{}\"",
                v.to_string()
                    .replace('&', "&amp;")
                    .replace('"', "&quot;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;")
            )
        })
        .unwrap_or_default();
    s.replace_range(start..end, &new);
}
fn mutation(change: impl FnOnce(&mut Value)) {
    let mut i = inputs("original");
    let mut c = certificate(&i);
    change(&mut c);
    rewrite(&mut i, Some(&c));
    assert_eq!(actual(&i), gold(false));
}
#[test]
fn original_source_keeps_only_the_three_independently_verified_cuts() {
    assert_eq!(actual(&inputs("original")), gold(true));
}
#[test]
fn an_outside_image_preserves_complete_original_analysis() {
    assert_eq!(actual(&inputs("image-outside")), gold(true));
}
#[test]
fn an_inside_image_refuses_both_counterpart_transfers() {
    assert_eq!(actual(&inputs("image-inside")), gold(false));
}
#[test]
fn an_inside_fill_refuses_both_counterpart_transfers() {
    assert_eq!(actual(&inputs("fill-inside")), gold(false));
}
#[test]
fn a_curved_outline_is_never_an_empty_native_scene() {
    assert_eq!(actual(&inputs("curve-inside")), gold(false));
}
#[test]
fn a_form_with_an_image_cannot_borrow_outer_native_words() {
    assert_eq!(actual(&inputs("form-image-inside")), gold(false));
}
#[test]
fn absence_is_not_empty() {
    let mut i = inputs("original");
    rewrite(&mut i, None);
    assert_eq!(actual(&i), gold(false));
}
#[test]
fn future_version_refuses() {
    mutation(|c| c["version"] = json!(2));
}
#[test]
fn foreign_glyph_count_refuses() {
    mutation(|c| c["glyphs"] = json!(c["glyphs"].as_u64().unwrap() + 1));
}
#[test]
fn missing_image_inventory_refuses() {
    mutation(|c| {
        c.as_object_mut().unwrap().remove("images");
    });
}
#[test]
fn stale_original_word_refuses() {
    mutation(|c| c["words"][0][2] = json!("stale original word"));
}
#[test]
fn foreign_original_word_geometry_refuses() {
    mutation(|c| c["words"][0][3][0] = c["words"][0][3][2].clone());
}
#[test]
fn partial_omitted_glyph_inventory_refuses() {
    mutation(|c| {
        c["rectangle_ids"].as_array_mut().unwrap().remove(0);
        c["rectangles"].as_array_mut().unwrap().remove(0);
    });
}
#[test]
fn duplicate_omitted_glyph_inventory_refuses() {
    mutation(|c| c["rectangle_ids"][1] = c["rectangle_ids"][0].clone());
}
#[test]
fn unknown_original_glyph_refuses() {
    mutation(|c| c["unknown"] = json!([0]));
}
#[test]
fn invalid_numeric_obstacle_refuses() {
    mutation(|c| c["images"] = json!([["NaN", 1, 2, 3]]));
}

#[test]
fn a_straight_interior_stroke_is_not_a_table_ruling() {
    assert_eq!(actual(&inputs("stroke-inside")), gold(false));
}

#[test]
fn an_interior_stroke_long_enough_for_native_rules_is_not_a_field_boundary() {
    assert_eq!(actual(&inputs("stroke-inside-long")), gold(false));
}

fn reordered_input(interleaved: bool) -> Vec<kanpo_parser::PageInput> {
    let mut inputs = inputs("original");
    let html = &inputs[0].xhtml;
    let start = html.find("<page ").unwrap();
    let open = start + html[start..].find('>').unwrap() + 1;
    let close = html.find("</page>").unwrap();
    let mut rest = &html[open..close];
    let mut words = vec![];
    while let Some(begin) = rest.find("<word ") {
        let end = begin + rest[begin..].find("</word>").unwrap() + "</word>".len();
        words.push(&rest[begin..end]);
        rest = &rest[end..];
    }
    if interleaved {
        let (even, odd): (Vec<_>, Vec<_>) =
            words.into_iter().enumerate().partition(|(i, _)| i % 2 == 0);
        words = even.into_iter().chain(odd).map(|(_, w)| w).collect();
    } else {
        words.reverse();
    }
    let mut output = html[..open].to_owned();
    for word in words {
        output.push_str("<line>");
        output.push_str(word);
        output.push_str("</line>\n");
    }
    output.push_str(&html[close..]);
    inputs[0].xhtml = output;
    inputs
}
#[test]
fn original_scene_keeps_complete_analysis_when_source_nodes_are_reversed() {
    assert_eq!(actual(&reordered_input(false)), gold(true));
}
#[test]
fn original_scene_keeps_complete_analysis_when_source_nodes_are_interleaved() {
    assert_eq!(actual(&reordered_input(true)), gold(true));
}
