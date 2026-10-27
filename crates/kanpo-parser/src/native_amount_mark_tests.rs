use super::*;
use serde_json::json;
struct Fixture {
    words: Vec<Word>,
    styles: Vec<ContentStyles>,
    units: Vec<Unit>,
    html: String,
}
fn fixture(kern: bool, drift: bool, sign: bool) -> Fixture {
    fixture_inner(kern, drift, sign, false)
}
fn fixture_inner(kern: bool, drift: bool, sign: bool, painted_sign: bool) -> Fixture {
    fixture_at(kern, drift, sign, painted_sign, 1., 0., 0.)
}
fn fixture_at(
    kern: bool,
    drift: bool,
    sign: bool,
    painted_sign: bool,
    k: f64,
    ox: f64,
    oy: f64,
) -> Fixture {
    let mut words = vec![];
    let mut styles = vec![];
    let mut nodes = vec![];
    let mut records = vec![];
    let mut add = |raw: &str,
                   text: &str,
                   x: f64,
                   z: f64,
                   baseline: f64,
                   prefix: f64,
                   label: bool,
                   mark: bool| {
        let (x, z, baseline, prefix) = (x * k + ox, z * k + ox, baseline * k + oy, prefix * k);
        let id = words.len();
        let b = [x, baseline - 7. * k, z, baseline + 2. * k];
        words.push(Word {
            id,
            source_box: b.map(|n| n as f32),
            x0: x as f32,
            y0: b[1] as f32,
            x1: z as f32,
            y1: b[3] as f32,
            raw: raw.into(),
            text: text.into(),
            status: if mark {
                WordStatus::Empty
            } else {
                WordStatus::Used
            },
        });
        styles.push(if mark {
            ContentStyles::Unknown
        } else {
            ContentStyles::Glyphs(NativeStyles::from([(
                if label { 31 } else { 79 },
                (8f64 * k).to_bits(),
            )]))
        });
        records.push(json!([id, id + 1, raw, b]));
        let outline = if mark {
            format!(
                " data-native-outline-rectangle='1 {id} {} {} {} {} {} {} {} {} {} {}'",
                id + 1,
                raw.as_bytes()
                    .iter()
                    .map(|v| format!("{v:02x}"))
                    .collect::<String>(),
                b[0],
                b[1],
                b[2],
                b[3],
                x + 0.5 * k,
                baseline - 3. * k,
                z - 0.5 * k,
                baseline - 2.5 * k
            )
        } else {
            String::new()
        };
        nodes.push(format!("<word xmin='{x}' ymin='{}' xmax='{z}' ymax='{}' data-wmode='0' data-native-axis='right' data-native-flow-start='{x}' data-native-flow-end='{z}' data-native-flow-baseline='{baseline}' data-native-flow-leading='{}' data-native-flow-trailing='0' data-native-flow-prefix='{prefix}' data-native-flow-first-glyph='{id}' data-native-flow-after-glyph='{}' data-native-terminal-glyph='1 {id} {} {x} {z} {baseline}'{outline}>{raw}</word>",b[1],b[3],z-x,id+1,id+1));
        id
    };
    let shift = if drift { 0.125 } else { 0. };
    let label = add("部門甲", "部門甲", 10., 30., 20. + shift, 0., true, false);
    let mut a = vec![label];
    if kern {
        a.push(add("1", "1", 60., 64., 20., 0., false, false));
        a.push(add(",", ",", 61., 69., 20., -3., false, false));
        a.push(add("234", "234", 66., 78., 20., -3., false, false));
    } else {
        a.push(add("11", "11", 68., 78., 20., 0., false, false));
    }
    a.push(add("22", "22", 90., 100., 20., 0., false, false));
    let label = add("部門乙", "部門乙", 10., 30., 40. + shift, 0., true, false);
    let value = add("33", "33", 68., 78., 40., 0., false, false);
    add("\u{e601}", "", 90., 100., 40., 0., false, true);
    let sign_id = painted_sign.then(|| add("△", "△", 50., 58., 40., 0., false, false));
    drop(add);
    if let Some(id) = sign_id {
        words[id].status = WordStatus::Noise;
    }
    let lines = vec![
        Line::text(
            if kern {
                "部門甲　1,234　22"
            } else {
                "部門甲　11　22"
            }
            .into(),
            a,
        ),
        Line::text(
            if sign {
                "部門乙　△33"
            } else {
                "部門乙　33"
            }
            .into(),
            vec![label, value],
        ),
    ];
    let scene = json!({"version":1,"glyphs":words.len(),"ignored":[],"rectangle_ids":[],"words":records,
        "unknown":[],"rectangles":[],"rulings":[],"rectangle_obstacles":[],"images":[],"lines":[],"fills":[]});
    let html = format!(
        "<page data-native-glyph-count='{}' data-native-scene='{}'>{}</page>",
        words.len(),
        scene
            .to_string()
            .replace('&', "&amp;")
            .replace('\'', "&#39;"),
        nodes.join("")
    );
    Fixture {
        words,
        styles,
        units: vec![Unit::Plain(lines)],
        html,
    }
}
fn apply(f: &mut Fixture) {
    let html = scraper::Html::parse_fragment(&f.html);
    let page = html
        .select(&scraper::Selector::parse("page").unwrap())
        .next()
        .unwrap();
    restore(
        &mut f.units,
        &mut f.words,
        &f.styles,
        page,
        &PageRules::default(),
    );
}
fn target(f: &Fixture) -> &str {
    let Unit::Plain(lines) = &f.units[0] else {
        panic!()
    };
    &lines[1].text
}
#[test]
fn a_missing_rectangle_mark_uses_original_rows_and_numeric_columns() {
    let mut f = fixture(false, false, false);
    apply(&mut f);
    assert_eq!(target(&f), "部門乙　33　－");
}
#[test]
fn source_kerning_connects_number_pieces_without_a_distance_tolerance() {
    let mut f = fixture(true, false, false);
    apply(&mut f);
    assert_eq!(target(&f), "部門乙　33　－");
}
#[test]
fn different_source_fonts_may_have_mutually_contained_baselines() {
    let mut f = fixture(false, true, false);
    apply(&mut f);
    assert_eq!(target(&f), "部門乙　33　－");
}
#[test]
fn an_existing_rendered_unary_sign_is_retained_byte_for_byte() {
    let mut f = fixture(false, false, true);
    apply(&mut f);
    assert_eq!(target(&f), "部門乙　△33　－");
}

#[test]
fn absolute_source_positions_can_connect_overlapping_numeric_punctuation() {
    let mut f = fixture(true, false, false);
    f.html = f.html.replace(
        "data-native-flow-prefix='-3'",
        "data-native-flow-prefix='0'",
    );
    apply(&mut f);
    assert_eq!(target(&f), "部門乙　33　－");
}

#[test]
fn an_already_rendered_original_sign_is_not_foreign_amount_material() {
    let mut f = fixture_inner(false, false, true, true);
    apply(&mut f);
    assert_eq!(target(&f), "部門乙　△33　－");
}

fn state(f: &Fixture) -> (serde_json::Value, serde_json::Value) {
    let units = f
        .units
        .iter()
        .map(|u| match u {
            Unit::Plain(ls) => serde_json::to_value(ls).unwrap(),
            Unit::Shinkyu { .. } => panic!("fixture expects plain units"),
        })
        .collect::<Vec<_>>();
    (
        serde_json::to_value(units).unwrap(),
        serde_json::to_value(&f.words).unwrap(),
    )
}
fn unchanged(mut f: Fixture) {
    let old = state(&f);
    apply(&mut f);
    assert_eq!(state(&f), old);
}
fn outline(f: &mut Fixture, at: usize, value: &str) {
    let prefix = "data-native-outline-rectangle='";
    let first = f.html.find(prefix).unwrap() + prefix.len();
    let end = first + f.html[first..].find('\'').unwrap();
    let mut fields = f.html[first..end]
        .split_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    fields[at] = value.into();
    f.html.replace_range(first..end, &fields.join(" "));
}
#[test]
fn missing_malformed_and_foreign_outline_certificates_do_not_restore_a_mark() {
    let mut f = fixture(false, false, false);
    f.html = f
        .html
        .replace("data-native-outline-rectangle=", "data-untrusted-outline=");
    unchanged(f);
    for (at, value) in [
        (0, "2"),
        (1, "0"),
        (2, "999"),
        (3, "00"),
        (4, "0"),
        (8, "NaN"),
        (9, "0"),
        (10, "1000"),
        (11, "43"),
        (10, "91"),
    ] {
        let mut f = fixture(false, false, false);
        outline(&mut f, at, value);
        unchanged(f);
    }
}
#[test]
fn missing_or_nonfinite_original_baselines_are_not_source_rows() {
    for value in ["unknown", "NaN", "inf"] {
        let mut f = fixture(false, false, false);
        f.html = f.html.replace(
            "data-native-flow-baseline='40'",
            &format!("data-native-flow-baseline='{value}'"),
        );
        unchanged(f);
    }
}
#[test]
fn a_legacy_row_cannot_borrow_numeric_style_from_unknown_originals() {
    let mut f = fixture(false, false, false);
    for (i, w) in f.words.iter().enumerate() {
        if w.text.chars().any(|c| c.is_ascii_digit()) {
            f.styles[i] = ContentStyles::Unknown;
        }
    }
    unchanged(f);
}
#[test]
fn different_numeric_or_label_fonts_do_not_certify_the_same_columns() {
    for i in [0, 1] {
        let mut f = fixture(false, false, false);
        f.styles[i] = ContentStyles::Glyphs(NativeStyles::from([(900, 8f64.to_bits())]));
        unchanged(f);
    }
}
#[test]
fn a_foreign_original_mark_or_sign_inside_a_row_is_not_ignored() {
    unchanged(fixture_inner(false, false, false, true));
}
#[test]
fn source_images_or_fills_inside_the_numeric_row_refuse_restoration() {
    for key in ["images", "fills"] {
        let mut f = fixture(false, false, false);
        f.html = f.html.replace(
            &format!("\"{key}\":[]"),
            &format!("\"{key}\":[[90,37,100,38]]"),
        );
        unchanged(f);
    }
}
#[test]
fn an_intervening_non_numeric_line_fences_the_neighboring_witness() {
    let mut f = fixture(false, false, false);
    let Unit::Plain(lines) = &mut f.units[0] else {
        panic!()
    };
    lines.insert(1, Line::text("別のまとまり。".into(), vec![]));
    unchanged(f);
}
#[test]
fn duplicated_old_ownership_cannot_supply_a_numeric_row() {
    let mut f = fixture(false, false, false);
    let Unit::Plain(lines) = &f.units[0] else {
        panic!()
    };
    let id = lines[1].words[1];
    f.units
        .push(Unit::Plain(vec![Line::text("33".into(), vec![id])]));
    unchanged(f);
}
#[test]
fn missing_source_scene_and_unknown_paint_do_not_mean_empty_space() {
    for (a, b) in [
        ("data-native-scene=", "data-missing-scene="),
        ("\"unknown\":[]", "\"unknown\":[0]"),
    ] {
        let mut f = fixture(false, false, false);
        f.html = f.html.replace(a, b);
        unchanged(f);
    }
}
#[test]
fn scale_translation_and_opaque_font_bijections_preserve_the_rule() {
    for (k, x, y) in [(0.125, -256., 512.), (1., 0., 0.), (8., 1024., -128.)] {
        let mut f = fixture_at(true, true, false, false, k, x, y);
        for style in &mut f.styles {
            if let ContentStyles::Glyphs(s) = style {
                *s = s.iter().map(|&(id, size)| (id + 792, size)).collect();
            }
        }
        apply(&mut f);
        assert_eq!(target(&f), "部門乙　33　－");
    }
}
#[test]
fn a_second_pass_keeps_the_complete_already_restored_state() {
    let mut f = fixture(false, false, false);
    apply(&mut f);
    let old = state(&f);
    apply(&mut f);
    assert_eq!(state(&f), old);
}
#[test]
fn opaque_original_mark_codepoints_are_not_a_dispatch_table() {
    for raw in ["\u{e145}", "\u{f0134}", "\u{f8ff}"] {
        let mut f = fixture(false, false, false);
        let old = "\u{e601}";
        let old_hex = old
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let new_hex = raw
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        f.words.last_mut().unwrap().raw = raw.into();
        f.html = f.html.replace(old, raw).replace(&old_hex, &new_hex);
        apply(&mut f);
        assert_eq!(target(&f), "部門乙　33　－");
    }
}
#[test]
fn an_existing_original_sign_keeps_its_status_and_ownership() {
    for status in [WordStatus::Used, WordStatus::Noise] {
        let mut f = fixture_inner(false, false, true, true);
        let id = f.words.len() - 1;
        f.words[id].status = status;
        let old = serde_json::to_value(&f.words[id]).unwrap();
        apply(&mut f);
        assert_eq!(target(&f), "部門乙　△33　－");
        assert_eq!(serde_json::to_value(&f.words[id]).unwrap(), old);
        let Unit::Plain(lines) = &f.units[0] else {
            panic!()
        };
        assert!(!lines[1].words.contains(&id));
    }
}

#[test]
fn a_valid_landscape_rectangle_has_no_fixed_stroke_thickness() {
    let mut f = fixture(false, false, false);
    outline(&mut f, 11, "40");
    apply(&mut f);
    assert_eq!(target(&f), "部門乙　33　－");
}
