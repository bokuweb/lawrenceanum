#[path = "support/legacy_analysis.rs"]
mod legacy_analysis;
use serde_json::Value;
fn analyze(p: &[u8]) -> Value {
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(p).unwrap());
    legacy_analysis::value(&a)
}
const P163: &[u8] = include_bytes!("archive/pages/20260723g00163/20260723g001630002.pdf");
#[path = "support/legal_summary_expectation.rs"]
mod legal_summary_expectation;
const P172: &[u8] = include_bytes!("archive/pages/20260731g00172/20260731g001720008.pdf");
const P194: &[u8] = include_bytes!("archive/pages/20260831g00194/20260831g001940091.pdf");
const P160: &[u8] = include_bytes!("archive/pages/20260717g00160/20260717g001600048.pdf");
fn owner_count(a: &Value, id: u64) -> usize {
    a["pages"][0]["regions"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| r["lines"].as_array().unwrap())
        .map(|l| {
            let mut n = l["words"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|v| v.as_u64() == Some(id))
                .count();
            if let Some(t) = l.get("table") {
                n += t["cells"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .flat_map(|c| c["words"].as_array().unwrap())
                    .filter(|v| v.as_u64() == Some(id))
                    .count()
            }
            n
        })
        .sum()
}
#[test]
fn actual163_verified_chi_has_its_original_Word_and_unique_line_owner() {
    let a = analyze(P163);
    let w = &a["pages"][0]["words"][51];
    assert_eq!(w["raw"], "\u{e9f5}");
    assert_eq!(w["text"], "（チ）");
    assert_eq!(w["status"], "used");
    assert_eq!(owner_count(&a, 51), 1);
    let s = a["text"].as_str().unwrap();
    let pos = s.find("（チ）").unwrap();
    assert!(
        s[pos + "（チ）".len()..]
            .trim_start()
            .starts_with("暗号資産取引業を行う者の財務")
    );
}
#[test]
fn actual172_standalone_chi_precedes_its_original_designated_city_paragraph() {
    let a = analyze(P172);
    let w = &a["pages"][0]["words"][59];
    assert_eq!(w["text"], "（チ）");
    assert_eq!(w["status"], "used");
    assert_eq!(owner_count(&a, 59), 1);
    let s = a["text"].as_str().unwrap();
    let pos = s.find("（チ）").unwrap();
    assert!(
        s[pos + "（チ）".len()..]
            .trim_start()
            .starts_with("４の（１）イにより指定された副首都ごと")
    );
}
#[test]
fn actual172_mixed_range_restores_only_original_chi_suffix() {
    let a = analyze(P172);
    let w = &a["pages"][0]["words"][50];
    assert_eq!(w["raw"], "\u{e9e6}〜\u{e9f5}のほか、副首都の整備に係る施");
    assert_eq!(w["text"], "（イ）〜（チ）のほか、副首都の整備に係る施");
    assert_eq!(w["status"], "used");
    assert_eq!(owner_count(&a, 50), 1);
    assert!(
        a["text"]
            .as_str()
            .unwrap()
            .contains("（イ）〜（チ）のほか、副首都の整備に係る施")
    );
}
#[test]
fn source_three_original_PUA_CID_font_and_ToUnicode_identity_are_preserved() {
    let mut count = 0;
    for pdf in [P163, P172] {
        let ps = kanpo_pdf::extract_pages(pdf).unwrap();
        let p = &ps[0];
        for g in &p.glyphs {
            if g.text == "\u{e9f5}" {
                count += 1;
                assert_eq!(g.cid, 25937);
                assert_eq!(p.fonts[g.font].ordering.as_deref(), Some("Identity"));
                assert!(p.fonts[g.font].has_to_unicode);
                assert_eq!(g.size, 8.0);
                assert!(!g.invisible);
            }
        }
    }
    assert_eq!(count, 3);
}
#[test]
fn all_other_Word_attributes_and_the_remaining_fourteen_labels_stay_exact() {
    for (pdf, old, ids) in [
        (
            P163,
            include_str!("expected/paren_chi_e9f5/20260723g001630002-before50.analysis.json"),
            vec![51usize],
        ),
        (
            P172,
            include_str!("expected/paren_chi_e9f5/20260731g001720008-before50.analysis.json"),
            vec![50, 59],
        ),
    ] {
        let a = analyze(pdf);
        let b: Value = serde_json::from_str(old).unwrap();
        let ws = a["pages"][0]["words"].as_array().unwrap();
        let bs = b["pages"][0]["words"].as_array().unwrap();
        assert_eq!(ws.len(), bs.len());
        for (i, (w, o)) in ws.iter().zip(bs).enumerate() {
            if ids.contains(&i) {
                for k in ["id", "raw", "x0", "y0", "x1", "y1"] {
                    assert_eq!(w[k], o[k]);
                }
            } else {
                assert_eq!(w, o, "Word{i}")
            }
        }
    }
}
#[test]
fn ordinary_rescue_AJ1_metadata_and_both_original_603_6165_cells_are_byte_preserved() {
    let a = analyze(P194);
    let b: Value = serde_json::from_str(include_str!(
        "expected/paren_chi_e9f5/20260831g001940091-before50.analysis.json"
    ))
    .unwrap();
    assert_eq!(a, b);
    for i in [159, 469] {
        assert_eq!(a["pages"][0]["words"][i]["text"], "救");
    }
}
#[test]
fn quoted_inline_old_new_Table_Run_repairs_are_byte_preserved() {
    let a = analyze(P160);
    let b: Value = serde_json::from_str(include_str!(
        "expected/paren_chi_e9f5/20260717g001600048-before50.analysis.json"
    ))
    .unwrap();
    assert_eq!(a, b);
}
#[test]
fn whole_Analysis_changes_only_three_source_labels_two_statuses_and_two_unique_owners() {
    fn replace_texts(v: &mut Value, from: &str, to: &str) {
        match v {
            Value::Object(o) => {
                for (k, x) in o {
                    if k == "text" {
                        if let Value::String(s) = x {
                            *s = s.replace(from, to);
                        }
                    } else {
                        replace_texts(x, from, to)
                    }
                }
            }
            Value::Array(a) => {
                for x in a {
                    replace_texts(x, from, to)
                }
            }
            _ => {}
        }
    }
    for (pdf, old, word, line, body) in [
        (
            P163,
            include_str!("expected/paren_chi_e9f5/20260723g001630002-before50.analysis.json"),
            51,
            167,
            "暗号資産取引業を行う者の財務の健",
        ),
        (
            P172,
            include_str!("expected/paren_chi_e9f5/20260731g001720008-before50.analysis.json"),
            59,
            82,
            "４の（１）イにより指定された副首都ごと",
        ),
    ] {
        let a = analyze(pdf);
        let mut b: Value = serde_json::from_str(old).unwrap();
        // Page/Article/Document text derives from the same exact original paragraph.
        // Original Word.text for this paragraph must remain unprefixed.
        let bodyword = if word == 51 { 7 } else { 49 };
        let original_word = b["pages"][0]["words"][bodyword]["text"].clone();
        replace_texts(&mut b, body, &format!("（チ）　{body}"));
        b["pages"][0]["words"][bodyword]["text"] = original_word;
        b["pages"][0]["words"][word]["text"] = Value::String("（チ）".into());
        b["pages"][0]["words"][word]["status"] = Value::String("used".into());
        b["pages"][0]["regions"][0]["lines"][line]["words"] = serde_json::json!([word, bodyword]);
        if word == 51 {
            let d = &mut b["articles"][0]["document"]["blocks"][0]["runs"][0]["text"];
            *d = Value::String(d.as_str().unwrap().replace(
                "（第四十六条の五関係）（チ）",
                "（第四十六条の五関係）\n（チ）",
            ));
        }
        if word == 59 {
            replace_texts(
                &mut b,
                "（イ）〜のほか、副首都の整備に係る施",
                "（イ）〜（チ）のほか、副首都の整備に係る施",
            );
        }
        if word == 51 {
            legal_summary_expectation::separate_original_laws_63_and_64(&mut b);
        }
        assert_eq!(a, b);
    }
}
