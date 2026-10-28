use super::*;
#[test]
fn original_omitted_compound_contours_remain_opaque_without_discarding_the_complete_scene() {
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "../../kanpo-pdf/tests/data/native-compound-scene/index.json"
    ))
    .unwrap();
    for (pi, bytes) in [
        include_bytes!("../../kanpo-pdf/tests/data/native-compound-scene/20260828g001920025.pdf")
            .as_slice(),
        include_bytes!("../../kanpo-pdf/tests/data/native-compound-scene/20260828g001920026.pdf")
            .as_slice(),
    ]
    .iter()
    .enumerate()
    {
        let i = crate::page_inputs(bytes).unwrap().remove(0);
        let d = scraper::Html::parse_document(&i.xhtml);
        let p = d
            .select(&scraper::Selector::parse("page").unwrap())
            .next()
            .unwrap();
        let packet:serde_json::Value=serde_json::from_str(p.value().attr("data-native-scene").expect("known compound paint must remain opaque rather than erase the complete source scene")).unwrap();
        assert_eq!(packet["version"], 3);
        assert_eq!(packet["unknown"], serde_json::json!([]));
        let ids = packet["glyph_obstacles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x[0].clone())
            .collect::<Vec<_>>();
        assert_eq!(
            serde_json::json!(ids),
            expected["expected_opaque_original_ids"][pi]["ids"]
        );
        let s = for_page(p, i.rules.as_ref().unwrap()).expect("strict version3 source partition");
        let trace =
            crate::vertical::analyze_pages_with_rules(&i.xhtml, &[i.rules.clone()]).remove(0);
        assert!(s.matches(&trace.words));
        for x in packet["glyph_obstacles"].as_array().unwrap() {
            let b = x[1]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap() as f32)
                .collect::<Vec<_>>();
            let b: [f32; 4] = b.try_into().unwrap();
            assert!(
                !s.unobstructed(b),
                "a compound glyph envelope cannot become empty paint or a table ruling"
            );
        }
    }
}

#[test]
fn source_coordinates_keep_the_exact_original_f64_bits_through_json() {
    let originals = [
        "97.22489999999999",
        "218.96429999999998",
        "-97.22489999999999",
    ]
    .map(|s| s.parse::<f64>().unwrap());
    let mut values = vec![f64::MAX, f64::MIN_POSITIVE, f64::from_bits(1), -0.0];
    for x in originals {
        for bits in [x.to_bits() - 1, x.to_bits(), x.to_bits() + 1] {
            for scale in [0.125, 1., 8.] {
                values.push(f64::from_bits(bits) * scale);
            }
        }
    }
    let encoded = serde_json::to_string(&values).unwrap();
    let decoded: Vec<f64> = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded.into_iter().map(f64::to_bits).collect::<Vec<_>>(),values.into_iter().map(f64::to_bits).collect::<Vec<_>>(),"source evidence requires exact binary coordinates; a pt tolerance or rounding would conflate different original source values");
}

fn original_packet() -> (crate::PageInput, serde_json::Value) {
    let i = crate::page_inputs(include_bytes!(
        "../../kanpo-pdf/tests/data/native-compound-scene/20260828g001920025.pdf"
    ))
    .unwrap()
    .remove(0);
    let d = scraper::Html::parse_document(&i.xhtml);
    let p = d
        .select(&scraper::Selector::parse("page").unwrap())
        .next()
        .unwrap();
    let packet = serde_json::from_str(p.value().attr("data-native-scene").unwrap()).unwrap();
    (i, packet)
}
fn read_packet(i: &crate::PageInput, packet: &serde_json::Value) -> Option<Scene> {
    let mut h = i.xhtml.clone();
    let needle = " data-native-scene=\"";
    let at = h.find(needle).unwrap() + needle.len();
    let end = at + h[at..].find('"').unwrap();
    h.replace_range(
        at..end,
        &packet
            .to_string()
            .replace('&', "&amp;")
            .replace('"', "&quot;")
            .replace('<', "&lt;")
            .replace('>', "&gt;"),
    );
    let d = scraper::Html::parse_document(&h);
    let p = d
        .select(&scraper::Selector::parse("page").unwrap())
        .next()
        .unwrap();
    for_page(p, i.rules.as_ref().unwrap())
}
#[test]
fn compound_packet_versions_and_unknown_paint_are_not_inferred_or_downgraded() {
    let (i, base) = original_packet();
    assert!(read_packet(&i, &base).is_some());
    for version in [0, 1, 2, 4] {
        let mut v = base.clone();
        v["version"] = version.into();
        assert!(read_packet(&i, &v).is_none());
    }
    let mut v = base.clone();
    v["glyph_obstacles"] = serde_json::json!([]);
    assert!(read_packet(&i, &v).is_none());
    let mut v = base;
    v["unknown"] = serde_json::json!([0]);
    assert!(read_packet(&i, &v).is_none());
}
#[test]
fn each_omitted_compound_glyph_requires_one_original_unborrowed_source_id() {
    let (i, base) = original_packet();
    for kind in 0..4 {
        let mut v = base.clone();
        match kind {
            0 => {
                v["glyph_obstacles"].as_array_mut().unwrap().pop();
            }
            1 => {
                let a = v["glyph_obstacles"][0].clone();
                v["glyph_obstacles"].as_array_mut().unwrap().push(a);
            }
            2 => v["glyph_obstacles"][0][0] = v["glyphs"].clone(),
            _ => v["glyph_obstacles"][0][0] = v["words"][0][0].clone(),
        }
        assert!(
            read_packet(&i, &v).is_none(),
            "source partition mismatch {kind}"
        );
    }
}
#[test]
fn malformed_opaque_envelopes_cannot_supply_scene_authority() {
    let (i, base) = original_packet();
    for b in [
        serde_json::json!([1, 2, 1, 4]),
        serde_json::json!([4, 3, 2, 1]),
        serde_json::json!([1, 2, 3]),
        serde_json::json!(["NaN", 2, 3, 4]),
    ] {
        let mut v = base.clone();
        v["glyph_obstacles"][0][1] = b;
        assert!(read_packet(&i, &v).is_none());
    }
}
#[test]
fn a_one_bit_word_envelope_change_cannot_round_back_into_the_original_source() {
    // Page26 contains the nonflow Word whose original JSON coordinate exposed
    // the one-bit parsing mismatch. Page25 has no nonflow Word packet.
    let i = crate::page_inputs(include_bytes!(
        "../../kanpo-pdf/tests/data/native-compound-scene/20260828g001920026.pdf"
    ))
    .unwrap()
    .remove(0);
    let d = scraper::Html::parse_document(&i.xhtml);
    let p = d
        .select(&scraper::Selector::parse("page").unwrap())
        .next()
        .unwrap();
    let mut v: serde_json::Value =
        serde_json::from_str(p.value().attr("data-native-scene").unwrap()).unwrap();
    let original = v["word_obstacles"][0][2][0].as_f64().unwrap();
    v["word_obstacles"][0][2][0] = serde_json::json!(f64::from_bits(original.to_bits() + 1));
    assert!(
        read_packet(&i, &v).is_none(),
        "distinct original envelope bits remain distinct without pt tolerances"
    );
}
