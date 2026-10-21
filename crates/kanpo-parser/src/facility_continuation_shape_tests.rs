use super::*;
use crate::lines::HRule;
struct Case {
    units: Vec<Unit>,
    words: Vec<Word>,
    rules: PageRules,
}
fn add(words: &mut Vec<Word>, text: &str, x: f32, y: f32, end: f32) -> usize {
    let id = words.len();
    words.push(Word {
        id,
        x0: x,
        y0: y,
        x1: x + 8.0,
        y1: end,
        raw: text.into(),
        text: text.into(),
        status: WordStatus::Used,
    });
    id
}
fn line(words: &[Word], ids: Vec<usize>) -> Line {
    Line::text(ids.iter().map(|id| words[*id].text.as_str()).collect(), ids)
}
fn case() -> Case {
    let mut words = Vec::new();
    let mut headers = Vec::new();
    for (t, y, z) in [
        ("施設番号", 60., 92.),
        ("施設名", 108., 156.),
        ("所在地名", 180., 236.),
        ("所有関係", 252., 284.),
        ("摘要", 308., 404.),
    ] {
        headers.push(add(&mut words, t, 120., y, z));
    }
    let h = line(&words, headers);
    let prev = add(&mut words, "一〇七六", 108., 60., 92.);
    let mut bands: Vec<Vec<Line>> = (0..4).map(|_| Vec::new()).collect();
    for (r, (number, name, city)) in [
        ("三〇八三", "第一施設", "第一市"),
        ("五一二七", "第二施設", "第二市"),
    ]
    .into_iter()
    .enumerate()
    {
        let x = 544. - r as f32 * 32.;
        let n = add(&mut words, number, x, 428., 460.);
        let a = add(&mut words, name, x, 468., 524.);
        bands[0].push(line(&words, vec![n, a]));
        let city = add(&mut words, city, x, 540., 564.);
        bands[1].push(line(&words, vec![city]));
        let owner = add(&mut words, "国有", x, 620., 636.);
        bands[2].push(line(&words, vec![owner]));
    }
    for (text, x, y, end) in [
        ("工作物：門", 544., 660., 708.),
        ("同じ施設の備考", 536., 660., 788.),
        ("建物：一式", 512., 660., 708.),
        ("次の施設の備考", 504., 660., 788.),
    ] {
        let id = add(&mut words, text, x, y, end);
        bands[3].push(line(&words, vec![id]));
    }
    let mut units = vec![Unit::Plain(vec![h, line(&words, vec![prev])])];
    units.extend(bands.into_iter().map(Unit::Plain));
    Case {
        units,
        words,
        rules: PageRules {
            horizontal: vec![
                HRule {
                    y: 420.,
                    x0: 40.,
                    x1: 560.,
                },
                HRule {
                    y: 800.,
                    x0: 40.,
                    x1: 560.,
                },
            ],
            ..Default::default()
        },
    }
}
fn view(us: &[Unit]) -> Vec<String> {
    us.iter().map(Unit::text).collect()
}
fn no_change(mut c: Case) {
    let was = view(&c.units);
    restore(&mut c.units, &c.words, Some(&c.rules), 600., 840.);
    assert_eq!(view(&c.units), was);
}
#[test]
fn pair_restores_five_fields_without_touching_source_words_or_ids() {
    let mut c = case();
    let ids = ownership(&c.units);
    let ws = format!("{:?}", c.words);
    restore(&mut c.units, &c.words, Some(&c.rules), 600., 840.);
    assert_eq!(c.units.len(), 2);
    let Unit::Plain(ls) = &c.units[1] else {
        panic!()
    };
    assert_eq!(ls.len(), 2);
    assert_eq!(
        ls[0].text,
        "三〇八三　第一施設　第一市　国有　工作物：門同じ施設の備考"
    );
    assert_eq!(
        ls[1].text,
        "五一二七　第二施設　第二市　国有　建物：一式次の施設の備考"
    );
    assert_eq!(ownership(&c.units), ids);
    assert_eq!(format!("{:?}", c.words), ws);
}
#[test]
fn physical_wrap_beyond_record_midpoint_stays_with_prior_record() {
    let mut c = case();
    let id = match &c.units[4] {
        Unit::Plain(ls) => ls[1].words[0],
        _ => unreachable!(),
    };
    c.words[id].x0 = 524.;
    c.words[id].x1 = 532.;
    restore(&mut c.units, &c.words, Some(&c.rules), 600., 840.);
    assert_eq!(c.units.len(), 2);
    let Unit::Plain(ls) = &c.units[1] else {
        panic!()
    };
    assert!(ls[0].text.ends_with("同じ施設の備考"));
    assert!(!ls[1].text.contains("同じ施設"));
}
#[test]
fn wrong_or_missing_heading_is_not_numeric_table_evidence() {
    let mut c = case();
    c.words[0].text = "価格番号".into();
    if let Unit::Plain(ls) = &mut c.units[0] {
        ls[0].text = ls[0].text.replace("施設番号", "価格番号");
    }
    no_change(c);
}
#[test]
fn repeated_numeric_identifier_is_ambiguous() {
    let mut c = case();
    c.words[10].text = "三〇八三".into();
    if let Unit::Plain(ls) = &mut c.units[1] {
        ls[1].text = "三〇八三第二施設".into();
    }
    no_change(c);
}
#[test]
fn mismatched_city_lane_is_rejected() {
    let mut c = case();
    c.words[8].x0 -= 10.;
    c.words[8].x1 -= 10.;
    no_change(c);
}
#[test]
fn crossing_summary_lane_is_rejected() {
    let mut c = case();
    let id = match &c.units[4] {
        Unit::Plain(ls) => ls[3].words[0],
        _ => unreachable!(),
    };
    c.words[id].x0 = 532.;
    c.words[id].x1 = 540.;
    no_change(c);
}
#[test]
fn margin_or_empty_numeric_anchor_is_not_evidence() {
    for status in [WordStatus::Margin, WordStatus::Empty, WordStatus::Noise] {
        let mut c = case();
        c.words[6].status = status;
        no_change(c);
    }
}
#[test]
fn foreign_used_word_inside_band_is_rejected() {
    let mut c = case();
    let id = add(&mut c.words, "別記事", 528., 540., 564.);
    c.units.push(Unit::Plain(vec![line(&c.words, vec![id])]));
    no_change(c);
}
#[test]
fn duplicate_owner_is_rejected() {
    let mut c = case();
    c.units.push(Unit::Plain(vec![line(&c.words, vec![6])]));
    no_change(c);
}
#[test]
fn missing_real_divider_is_rejected() {
    let mut c = case();
    c.rules.horizontal.remove(0);
    no_change(c);
}
#[test]
fn intervening_article_or_second_header_is_rejected() {
    let mut c = case();
    let id = add(&mut c.words, "〇別の記事", 80., 60., 92.);
    if let Unit::Plain(ls) = &mut c.units[0] {
        ls.push(line(&c.words, vec![id]));
    }
    no_change(c);
    let mut c = case();
    let ids = vec![0, 1, 2, 3, 4];
    let l = line(&c.words, ids);
    if let Unit::Plain(ls) = &mut c.units[0] {
        ls.push(l);
    }
    no_change(c);
}
#[test]
fn internal_vertical_separator_is_a_different_table() {
    let mut c = case();
    c.rules.vertical.push(crate::lines::VRule {
        x: 530.,
        y0: 420.,
        y1: 800.,
    });
    no_change(c);
}
#[test]
fn field_start_offset_must_agree_with_source_header() {
    let mut c = case();
    c.words[8].y0 += 32.;
    c.words[8].y1 += 32.;
    c.words[12].y0 += 32.;
    c.words[12].y1 += 32.;
    no_change(c);
}
#[test]
fn sideways_or_unruled_page_is_not_an_upright_continuation() {
    let mut c = case();
    let was = view(&c.units);
    restore(&mut c.units, &c.words, Some(&c.rules), 840., 600.);
    assert_eq!(view(&c.units), was);
    restore(&mut c.units, &c.words, None, 600., 840.);
    assert_eq!(view(&c.units), was);
}

#[test]
fn another_article_title_inside_remarks_is_not_part_of_a_facility() {
    let mut c = case();
    let id = match &c.units[4] {
        Unit::Plain(ls) => ls[3].words[0],
        _ => unreachable!(),
    };
    c.words[id].text = "〇別記事の題名".into();
    if let Unit::Plain(ls) = &mut c.units[4] {
        ls[3].text = "〇別記事の題名".into();
    }
    no_change(c);
}
// Source50 rejected duplicate names. Native identifiers and axes, not names,
// distinguish these records. The original negative is retained in history.
#[test]
fn same_name_with_distinct_source_number_and_axis_is_two_records() {
    let mut c = case();
    c.words[11].text = "第一施設".into();
    c.words[11].raw = "第一施設".into();
    if let Unit::Plain(ls) = &mut c.units[1] {
        ls[1] = line(&c.words, ls[1].words.clone());
    }
    let ids = ownership(&c.units);
    let old = format!("{:?}", c.words);
    restore(&mut c.units, &c.words, Some(&c.rules), 600., 840.);
    assert_eq!(c.units.len(), 2);
    let Unit::Plain(ls) = &c.units[1] else {
        panic!()
    };
    assert_eq!(
        ls[0].text,
        "三〇八三　第一施設　第一市　国有　工作物：門同じ施設の備考"
    );
    assert_eq!(
        ls[1].text,
        "五一二七　第一施設　第二市　国有　建物：一式次の施設の備考"
    );
    assert_eq!(ls[0].words, vec![6, 7, 8, 9, 14, 15]);
    assert_eq!(ls[1].words, vec![10, 11, 12, 13, 16, 17]);
    assert_eq!(ownership(&c.units), ids);
    assert_eq!(format!("{:?}", c.words), old);
}
#[test]
fn same_name_with_duplicate_source_number_remains_ambiguous() {
    let mut c = case();
    c.words[11].text = "第一施設".into();
    c.words[11].raw = "第一施設".into();
    c.words[10].text = "三〇八三".into();
    c.words[10].raw = "三〇八三".into();
    if let Unit::Plain(ls) = &mut c.units[1] {
        ls[1] = line(&c.words, ls[1].words.clone());
    }
    no_change(c);
}
#[test]
fn same_name_and_overlaid_source_axis_remains_ambiguous() {
    let mut c = case();
    c.words[11].text = "第一施設".into();
    c.words[11].raw = "第一施設".into();
    for id in [10, 11, 12, 13, 16, 17] {
        c.words[id].x0 += 32.;
        c.words[id].x1 += 32.;
    }
    if let Unit::Plain(ls) = &mut c.units[1] {
        ls[1] = line(&c.words, ls[1].words.clone());
    }
    no_change(c);
}

// These cases preserve the five source fields and native two-rule band. The
// expected bindings are generated from construction, not a production literal.
struct Scenario {
    c: Case,
    expected: Vec<Line>,
}
fn scenario(count: usize, name_chars: usize) -> Scenario {
    let tall = name_chars > 7;
    let extra = if tall { 260. } else { 0. };
    let top = if tall { 700. } else { 420. };
    let shift = top - 60. + 8.;
    let mut ws = vec![];
    let mut hs = vec![];
    for (t, y, z) in [
        ("施設番号", 60., 92.),
        ("施設名", 108., 156.),
        ("所在地名", 180. + extra, 236. + extra),
        ("所有関係", 252. + extra, 284. + extra),
        ("摘要", 308. + extra, 404. + extra),
    ] {
        hs.push(add(&mut ws, t, 120., y, z));
    }
    let h = line(&ws, hs);
    let prev = add(&mut ws, "一〇七六", 108., 60., 92.);
    let mut bands = vec![vec![], vec![], vec![], vec![]];
    let mut expected = vec![];
    let mut remarks = vec![];
    for r in 0..count {
        let x = 544. - r as f32 * 32.;
        let number = format!("{}", 3000 + r);
        let first = char::from_u32('甲' as u32 + r as u32).unwrap();
        let name = std::iter::repeat_n(first, name_chars).collect::<String>();
        let city = format!("第{}市", r + 1);
        let mut ids = vec![];
        let n = add(&mut ws, &number, x, 60. + shift, 92. + shift);
        ids.push(n);
        let name_y = 100. + shift;
        let a = add(&mut ws, &name, x, name_y, name_y + 8. * name_chars as f32);
        ids.push(a);
        bands[0].push(line(&ws, vec![n, a]));
        let city_id = add(
            &mut ws,
            &city,
            x,
            172. + extra + shift,
            196. + extra + shift,
        );
        ids.push(city_id);
        bands[1].push(line(&ws, vec![city_id]));
        let owner = add(
            &mut ws,
            "国有",
            x,
            252. + extra + shift,
            268. + extra + shift,
        );
        ids.push(owner);
        bands[2].push(line(&ws, vec![owner]));
        let note = format!("設備{}一式", r + 1);
        let note2 = format!("原記録{}の備考", r + 1);
        remarks.push((r, x, note.clone(), note2.clone()));
        expected.push(Line::text(
            [number, name, city, "国有".into(), format!("{note}{note2}")].join("　"),
            ids,
        ));
    }
    for (r, x, a, b) in remarks {
        let y = 300. + extra + shift;
        let id = add(&mut ws, &a, x, y, y + 40.);
        bands[3].push(line(&ws, vec![id]));
        expected[r].words.push(id);
        let id = add(&mut ws, &b, x - 8., y, y + 128.);
        bands[3].push(line(&ws, vec![id]));
        expected[r].words.push(id);
    }
    let mut units = vec![Unit::Plain(vec![h, line(&ws, vec![prev])])];
    units.extend(bands.into_iter().map(Unit::Plain));
    let bottom = if tall { 1348. } else { 800. };
    Scenario {
        c: Case {
            units,
            words: ws,
            rules: PageRules {
                horizontal: vec![
                    HRule {
                        y: top,
                        x0: 40.,
                        x1: 560.,
                    },
                    HRule {
                        y: bottom,
                        x0: 40.,
                        x1: 560.,
                    },
                ],
                ..Default::default()
            },
        },
        expected,
    }
}
fn split_word(s: &mut Scenario, id: usize, at: usize) {
    let w = s.c.words[id].clone();
    let chars: Vec<_> = w.text.chars().collect();
    assert!(at > 0 && at < chars.len());
    let a: String = chars[..at].iter().collect();
    let b: String = chars[at..].iter().collect();
    let y = w.y0 + (w.y1 - w.y0) * at as f32 / chars.len() as f32;
    s.c.words[id].text = a.clone();
    s.c.words[id].raw = a;
    s.c.words[id].y1 = y;
    let other = add(&mut s.c.words, &b, w.x0, y, w.y1);
    for u in &mut s.c.units {
        if let Unit::Plain(ls) = u {
            for l in ls {
                if let Some(i) = l.words.iter().position(|v| *v == id) {
                    l.words.insert(i + 1, other);
                }
            }
        }
    }
    for l in &mut s.expected {
        if let Some(i) = l.words.iter().position(|v| *v == id) {
            l.words.insert(i + 1, other);
        }
    }
}
fn assert_scenario(mut s: Scenario) {
    let old = s.c.words.clone();
    let own = ownership(&s.c.units);
    let height = if s.c.rules.horizontal[0].y > 500. {
        1400.
    } else {
        840.
    };
    restore(&mut s.c.units, &s.c.words, Some(&s.c.rules), 600., height);
    for want in &s.expected {
        let found: Vec<_> =
            s.c.units
                .iter()
                .flat_map(|u| match u {
                    Unit::Plain(ls) => ls.as_slice(),
                    _ => &[],
                })
                .filter(|l| l.text == want.text)
                .collect();
        assert_eq!(
            found.len(),
            1,
            "the five source fields must form one record: {}",
            want.text
        );
        assert_eq!(
            found[0].words, want.words,
            "exact source IDs in field order"
        );
    }
    assert_eq!(format!("{:?}", s.c.words), format!("{:?}", old));
    assert_eq!(ownership(&s.c.units), own);
}
#[test]
fn general_two_record_control() {
    assert_scenario(scenario(2, 4));
}
#[test]
fn general_one_record_same_schema_and_native_band() {
    assert_scenario(scenario(1, 4));
}
#[test]
fn general_three_records_same_schema_and_native_band() {
    assert_scenario(scenario(3, 4));
}
#[test]
fn general_single_character_names() {
    assert_scenario(scenario(2, 1));
}
#[test]
fn general_31_character_names_in_a_sufficient_source_field() {
    assert_scenario(scenario(2, 31));
}
#[test]
fn general_name_word_fragments() {
    let mut s = scenario(2, 4);
    let id = s.expected[0].words[1];
    split_word(&mut s, id, 2);
    assert_scenario(s);
}
#[test]
fn general_header_word_fragments() {
    let mut s = scenario(2, 4);
    split_word(&mut s, 0, 2);
    assert_scenario(s);
}
#[test]
fn general_each_line_in_its_own_unit() {
    let mut s = scenario(2, 4);
    s.c.units =
        s.c.units
            .into_iter()
            .flat_map(|u| match u {
                Unit::Plain(ls) => ls
                    .into_iter()
                    .map(|l| Unit::Plain(vec![l]))
                    .collect::<Vec<_>>(),
                _ => unreachable!(),
            })
            .collect();
    assert_scenario(s);
}
#[test]
fn general_all_plain_lines_in_one_unit() {
    let mut s = scenario(2, 4);
    let ls =
        s.c.units
            .into_iter()
            .flat_map(|u| match u {
                Unit::Plain(ls) => ls,
                _ => unreachable!(),
            })
            .collect();
    s.c.units = vec![Unit::Plain(ls)];
    assert_scenario(s);
}
#[test]
fn general_unknown_schema_is_not_guessed() {
    let mut s = scenario(3, 4);
    s.c.words[2].text = "未詳欄名".into();
    s.c.words[2].raw = "未詳欄名".into();
    if let Unit::Plain(ls) = &mut s.c.units[0] {
        ls[0].text = ls[0].text.replace("所在地名", "未詳欄名");
    }
    no_change(s.c);
}
#[test]
fn general_additional_field_is_not_guessed() {
    let mut s = scenario(3, 4);
    let id = add(&mut s.c.words, "追加欄", 120., 410., 418.);
    if let Unit::Plain(ls) = &mut s.c.units[0] {
        ls[0].words.push(id);
        ls[0].text.push_str("追加欄");
    }
    no_change(s.c);
}
#[test]
fn general_duplicate_code_remains_ambiguous() {
    let mut s = scenario(3, 4);
    let a = s.expected[0].words[0];
    let b = s.expected[1].words[0];
    let text = s.c.words[a].text.clone();
    s.c.words[b].text = text.clone();
    s.c.words[b].raw = text;
    for u in &mut s.c.units {
        if let Unit::Plain(ls) = u {
            for l in ls {
                if l.words.contains(&b) {
                    *l = line(&s.c.words, l.words.clone());
                }
            }
        }
    }
    no_change(s.c);
}
#[test]
fn general_missing_native_rule_is_not_guessed() {
    let mut s = scenario(3, 4);
    s.c.rules.horizontal.pop();
    no_change(s.c);
}
#[test]
fn general_overlaid_number_words_are_not_concatenated() {
    let mut s = scenario(2, 4);
    let id = s.expected[0].words[0];
    let w = s.c.words[id].clone();
    let other = add(&mut s.c.words, "9999", w.x0, w.y0, w.y1);
    for u in &mut s.c.units {
        if let Unit::Plain(ls) = u {
            for l in ls {
                if let Some(i) = l.words.iter().position(|v| *v == id) {
                    l.words.insert(i + 1, other);
                    *l = line(&s.c.words, l.words.clone());
                }
            }
        }
    }
    no_change(s.c);
}

// The real pre-restoration source partition is a historical defect input. It
// must not be relabelled as the current baseline output.
#[test]
fn real_native_unit_partition_preserves_all_original_record_ids() {
    let v: serde_json::Value = serde_json::from_str(include_str!(
        "testdata/facility-continuation-pre-restoration.json"
    ))
    .unwrap();
    let p = &v["pages"][0];
    let mut ws = vec![];
    for w in p["words"].as_array().unwrap() {
        let f = |k: &str| w[k].as_f64().unwrap() as f32;
        let status = match w["status"].as_str().unwrap() {
            "used" => WordStatus::Used,
            "empty" => WordStatus::Empty,
            "margin" => WordStatus::Margin,
            "noise" => WordStatus::Noise,
            _ => panic!("unknown source status"),
        };
        ws.push(Word {
            id: w["id"].as_u64().unwrap() as usize,
            x0: f("x0"),
            y0: f("y0"),
            x1: f("x1"),
            y1: f("y1"),
            raw: w["raw"].as_str().unwrap().into(),
            text: w["text"].as_str().unwrap().into(),
            status,
        });
    }
    let mut units = vec![];
    for region in p["regions"].as_array().unwrap() {
        // Table lines are not part of this Plain continuation. They are retained as
        // an uneditable barrier; their content is covered by the actual-PDF tests.
        if region["lines"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l.get("table").is_some())
        {
            units.push(Unit::Shinkyu {
                preamble: vec![],
                after: vec![],
                before: vec![],
                outside: vec![],
            });
            continue;
        }
        units.push(Unit::Plain(
            region["lines"]
                .as_array()
                .unwrap()
                .iter()
                .map(|l| {
                    Line::text(
                        l["text"].as_str().unwrap().into(),
                        l["words"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|i| i.as_u64().unwrap() as usize)
                            .collect(),
                    )
                })
                .collect(),
        ));
    }
    let native = kanpo_pdf::extract_pages(include_bytes!(
        "../tests/archive/pages/20260915h01790/20260915h017900005.pdf"
    ))
    .unwrap();
    let rules = kanpo_pdf::rules::page_rules(&native[0]);
    let old = format!("{:?}", ws);
    let owners = ownership(&units);
    restore(&mut units, &ws, Some(&rules), 595., 842.);
    for ids in [
        &[0, 1, 2, 3, 4, 6, 7][..],
        &[
            10, 11, 12, 13, 14, 15, 17, 18, 20, 21, 23, 24, 25, 27, 29, 31, 33, 35, 36,
        ][..],
    ] {
        let found: Vec<_> = units
            .iter()
            .flat_map(|u| match u {
                Unit::Plain(ls) => ls.as_slice(),
                _ => &[],
            })
            .filter(|l| l.words == ids)
            .collect();
        assert_eq!(found.len(), 1, "whole original record and remarks IDs");
        assert_eq!(found[0].text.split('　').count(), 5);
    }
    assert_eq!(format!("{:?}", ws), old);
    assert_eq!(ownership(&units), owners);
}

// The whole physical source envelope must be accounted for. A unique owner
// elsewhere cannot certify a Word whose box intersects this record domain.
fn ghost_source_case(status: WordStatus, raw: &str, text: &str, bbox: [f32; 4]) -> Case {
    let mut c = case();
    let id = c.words.len();
    c.words.push(Word {
        id,
        x0: bbox[0],
        y0: bbox[1],
        x1: bbox[2],
        y1: bbox[3],
        raw: raw.into(),
        text: text.into(),
        status,
    });
    c.units.push(Unit::Shinkyu {
        preamble: vec![Line::text(text.into(), vec![id])],
        after: vec![],
        before: vec![],
        outside: vec![],
    });
    assert_eq!(ownership(&c.units)[&id], 1);
    c
}
#[test]
fn coverage_empty_crossing_edge_with_center_outside_rejects() {
    no_change(ghost_source_case(
        WordStatus::Empty,
        "\u{e712}",
        "",
        [496., 500., 508., 508.],
    ));
}
#[test]
fn coverage_used_crossing_edge_with_center_outside_rejects() {
    no_change(ghost_source_case(
        WordStatus::Used,
        "異",
        "異",
        [496., 500., 508., 508.],
    ));
}
#[test]
fn coverage_touching_edge_is_not_disjoint() {
    no_change(ghost_source_case(
        WordStatus::Empty,
        "\u{e712}",
        "",
        [496., 500., 504., 508.],
    ));
}
#[test]
fn coverage_inverted_x_box_normalizes_before_intersection() {
    no_change(ghost_source_case(
        WordStatus::Empty,
        "\u{e712}",
        "",
        [508., 500., 496., 508.],
    ));
}
#[test]
fn coverage_zero_width_edge_box_is_not_ignored() {
    no_change(ghost_source_case(
        WordStatus::Empty,
        "\u{e712}",
        "",
        [504., 500., 504., 508.],
    ));
}
#[test]
fn coverage_zero_dimension_corner_is_not_ignored() {
    no_change(ghost_source_case(
        WordStatus::Empty,
        "\u{e712}",
        "",
        [504., 428., 504., 428.],
    ));
}
#[test]
fn coverage_empty_raw_still_has_a_physical_box() {
    no_change(ghost_source_case(
        WordStatus::Empty,
        "",
        "",
        [496., 500., 508., 508.],
    ));
}
#[test]
fn coverage_inverted_y_box_normalizes_before_intersection() {
    no_change(ghost_source_case(
        WordStatus::Used,
        "異",
        "異",
        [496., 508., 508., 500.],
    ));
}
#[test]
fn coverage_distant_empty_word_is_not_a_record_intrusion() {
    let mut c = ghost_source_case(WordStatus::Empty, "", "", [400., 500., 408., 508.]);
    let owners = ownership(&c.units);
    restore(&mut c.units, &c.words, Some(&c.rules), 600., 840.);
    assert_eq!(c.units.len(), 3);
    assert_eq!(ownership(&c.units), owners);
}

#[test]
fn coverage_all_source_statuses_require_complete_physical_coverage() {
    for status in [
        WordStatus::Empty,
        WordStatus::Used,
        WordStatus::Margin,
        WordStatus::Noise,
    ] {
        no_change(ghost_source_case(
            status,
            "異",
            "異",
            [496., 500., 508., 508.],
        ));
    }
}
#[test]
fn coverage_nonfinite_source_box_cannot_prove_disjointness() {
    no_change(ghost_source_case(
        WordStatus::Empty,
        "",
        "",
        [f32::NAN, 500., 508., 508.],
    ));
}
#[test]
fn coverage_positive_gap_is_disjoint_without_inflating_source_edges() {
    let mut c = ghost_source_case(WordStatus::Empty, "", "", [496., 500., 503.99, 508.]);
    let owners = ownership(&c.units);
    restore(&mut c.units, &c.words, Some(&c.rules), 600., 840.);
    assert_eq!(c.units.len(), 3);
    assert_eq!(ownership(&c.units), owners);
}

fn exact_unit_view(
    us: &[Unit],
) -> Vec<(
    u8,
    Vec<Line>,
    Vec<Vec<Line>>,
    Vec<Vec<Line>>,
    Vec<Vec<Line>>,
)> {
    us.iter()
        .map(|u| match u {
            Unit::Plain(ls) => (0, ls.clone(), vec![], vec![], vec![]),
            Unit::Shinkyu {
                preamble,
                after,
                before,
                outside,
            } => (
                1,
                preamble.clone(),
                after.clone(),
                before.clone(),
                outside.clone(),
            ),
        })
        .collect()
}
// All source coordinates and page dimensions undergo the same physical transform.
fn physical_transform(c: &mut Case, scale: f32, dx: f32, dy: f32) {
    for w in &mut c.words {
        w.x0 = w.x0 * scale + dx;
        w.x1 = w.x1 * scale + dx;
        w.y0 = w.y0 * scale + dy;
        w.y1 = w.y1 * scale + dy;
    }
    for r in &mut c.rules.horizontal {
        r.y = r.y * scale + dy;
        r.x0 = r.x0 * scale + dx;
        r.x1 = r.x1 * scale + dx;
    }
    for r in &mut c.rules.vertical {
        r.x = r.x * scale + dx;
        r.y0 = r.y0 * scale + dy;
        r.y1 = r.y1 * scale + dy;
    }
}
#[test]
fn general_facility_source_scale_and_translation_preserve_records() {
    for scale in [0.125, 0.25, 1., 4., 8.] {
        for (dx, dy) in [(0., 0.), (23. * scale, 11. * scale)] {
            for count in 1..=3 {
                for len in [1, 4, 31] {
                    let mut s = scenario(count, len);
                    let height = if len > 20 { 1400. } else { 840. };
                    physical_transform(&mut s.c, scale, dx, dy);
                    let old = format!("{:?}", s.c.words);
                    let own = ownership(&s.c.units);
                    restore(
                        &mut s.c.units,
                        &s.c.words,
                        Some(&s.c.rules),
                        600. * scale,
                        height * scale,
                    );
                    let result: Vec<_> =
                        s.c.units
                            .iter()
                            .flat_map(|u| match u {
                                Unit::Plain(ls) => ls.as_slice(),
                                _ => &[],
                            })
                            .collect();
                    for want in s.expected {
                        let found: Vec<_> = result.iter().filter(|l| l.text == want.text).collect();
                        assert_eq!(
                            found.len(),
                            1,
                            "scale={scale}, dx={dx}, dy={dy}, count={count}, len={len}"
                        );
                        assert_eq!(found[0].words, want.words);
                    }
                    assert_eq!(format!("{:?}", s.c.words), old);
                    assert_eq!(ownership(&s.c.units), own);
                }
            }
        }
    }
}
#[test]
fn general_facility_source_scale_and_translation_preserve_closed_edge_refusal() {
    for scale in [0.125, 0.25, 1., 4., 8.] {
        for (dx, dy) in [(0., 0.), (23. * scale, 11. * scale)] {
            let mut c = ghost_source_case(WordStatus::Empty, "", "", [504., 500., 504., 508.]);
            physical_transform(&mut c, scale, dx, dy);
            let old = exact_unit_view(&c.units);
            restore(
                &mut c.units,
                &c.words,
                Some(&c.rules),
                600. * scale,
                840. * scale,
            );
            let after = exact_unit_view(&c.units);
            assert!(after == old, "scale={scale}, dx={dx}, dy={dy}");
        }
    }
}
