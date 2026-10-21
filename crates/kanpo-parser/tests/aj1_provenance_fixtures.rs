use serde_json::Value;
fn analyze(pdf: &[u8]) -> Value {
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap()
}
const PDF: &[u8] = include_bytes!("archive/pages/20260831g00194/20260831g001940091.pdf");
#[test]
fn Japan1_unicode_rescue_is_not_reinterpreted_as_untagged_identity_cid() {
    let a = analyze(PDF);
    for id in [159, 469] {
        assert_eq!(a["pages"][0]["words"][id]["raw"], "救");
        assert_eq!(a["pages"][0]["words"][id]["text"], "救");
        assert_eq!(a["pages"][0]["words"][id]["status"], "used");
    }
    assert!(a["text"].as_str().unwrap().contains("災害救護事業費"));
}
#[test]
fn all_other_original_unicode_word_attributes_stay_exact() {
    let a = analyze(PDF);
    let b: Value = serde_json::from_str(include_str!(
        "expected/aj1_provenance/194p91-before48.analysis.json"
    ))
    .unwrap();
    let ws = a["pages"][0]["words"].as_array().unwrap();
    let old = b["pages"][0]["words"].as_array().unwrap();
    assert_eq!(ws.len(), old.len());
    for (i, (x, y)) in ws.iter().zip(old).enumerate() {
        if [159, 469].contains(&i) {
            for k in ["id", "raw", "status", "x0", "y0", "x1", "y1"] {
                assert_eq!(x[k], y[k]);
            }
        } else {
            assert_eq!(x, y)
        }
    }
}
#[test]
fn actual_verified_identity_CID25937_ToUnicode_E9F5_is_restored_page163p2() {
    let a = analyze(include_bytes!(
        "archive/pages/20260723g00163/20260723g001630002.pdf"
    ));
    let b: Value = serde_json::from_str(include_str!(
        "expected/paren_chi_e9f5/20260723g001630002-restored.analysis.json"
    ))
    .unwrap();
    assert_eq!(a, b);
    assert_eq!(a["pages"][0]["words"][51]["raw"], "\u{e9f5}");
    assert_eq!(a["pages"][0]["words"][51]["status"], "used");
    assert_eq!(a["pages"][0]["words"][51]["text"], "（チ）");
}
#[test]
fn actual_verified_identity_CID25937_ToUnicode_E9F5_is_restored_page172p8() {
    let a = analyze(include_bytes!(
        "archive/pages/20260731g00172/20260731g001720008.pdf"
    ));
    let b: Value = serde_json::from_str(include_str!(
        "expected/paren_chi_e9f5/20260731g001720008-restored.analysis.json"
    ))
    .unwrap();
    assert_eq!(a, b);
    assert!(
        a["pages"][0]["words"][50]["raw"]
            .as_str()
            .unwrap()
            .contains('\u{e9f5}')
    );
    assert_eq!(a["pages"][0]["words"][59]["status"], "used");
    assert_eq!(a["pages"][0]["words"][59]["text"], "（チ）");
}
#[test]
fn both_complete_original_cells_and_amount_rows_are_literal() {
    let a = analyze(PDF);
    let p = &a["pages"][0];
    let cells: Vec<_> = p["regions"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| r["lines"].as_array().unwrap())
        .filter_map(|l| l.get("table"))
        .flat_map(|t| t["cells"].as_array().unwrap())
        .filter(|c| {
            c["words"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i == 159 || i == 469)
        })
        .collect();
    assert_eq!(cells.len(), 2);
    for c in cells {
        assert_eq!(c["text"], "災害救護事業費");
        let ids = c["words"].as_array().unwrap();
        let ids: Vec<_> = ids.iter().map(|v| v.as_u64().unwrap()).collect();
        assert!(
            [
                vec![483, 481, 469, 458, 456, 447, 436],
                vec![194, 186, 159, 138, 122, 103, 81]
            ]
            .contains(&ids)
        );
        let raw = ids
            .iter()
            .map(|&i| p["words"][i as usize]["raw"].as_str().unwrap())
            .collect::<String>();
        assert_eq!(raw, "災害救護事業費");
    }
    let lines: Vec<_> = a["text"].as_str().unwrap().lines().collect();
    assert_eq!(lines[29], "社資収入　9,802　災害救護事業費　603");
    assert_eq!(lines[42], "　社資収入　18,655　　災害救護事業費　6,165");
}
#[test]
fn whole_Analysis_only_changes_the_two_source_Words_and_their_rendered_labels() {
    let a = analyze(PDF);
    let mut expected: Value = serde_json::from_str(include_str!(
        "expected/aj1_provenance/194p91-before48.analysis.json"
    ))
    .unwrap();
    for id in [159, 469] {
        expected["pages"][0]["words"][id]["text"] = Value::String("救".into());
    }
    fn replace(v: &mut Value) {
        match v {
            Value::Object(o) => {
                for (k, x) in o {
                    if k == "text" {
                        if let Value::String(s) = x {
                            *s = s.replace("災害　（チ）　護事業費", "災害救護事業費");
                        }
                    } else {
                        replace(x)
                    }
                }
            }
            Value::Array(a) => {
                for x in a {
                    replace(x)
                }
            }
            _ => {}
        }
    }
    replace(&mut expected);
    assert_eq!(a, expected);
}
#[test]
fn source_14_original_Glyph_advances_and_spacing_are_unchanged() {
    let n = kanpo_pdf::extract_pages(PDF).unwrap();
    let p = &n[0];
    let literal: Value = serde_json::from_str(include_str!(
        "expected/aj1_provenance/194p91-14glyph-literal.json"
    ))
    .unwrap();
    for (id, expected) in literal["glyphIDs"]
        .as_array()
        .unwrap()
        .iter()
        .zip(literal["glyphs"].as_array().unwrap())
    {
        let actual: Value = serde_json::from_str(
            &serde_json::to_string(&p.glyphs[id.as_u64().unwrap() as usize]).unwrap(),
        )
        .unwrap();
        assert_eq!(&actual, expected);
    }
    let font: Value = serde_json::from_str(&serde_json::to_string(&p.fonts[2]).unwrap()).unwrap();
    assert_eq!(font, literal["font"]);
    let rows = [(629.0104, 78.2294), (260.9984, 403.6631)];
    for (x, y) in rows {
        let mut gs: Vec<_> = p
            .glyphs
            .iter()
            .filter(|g| (g.y - y).abs() < 0.0002 && g.x >= x - 0.0002 && g.x < x + 104.1)
            .collect();
        gs.sort_by(|a, b| a.x.total_cmp(&b.x));
        assert_eq!(gs.len(), 7);
        assert_eq!(
            gs.iter().map(|g| g.text.as_str()).collect::<String>(),
            "災害救護事業費"
        );
        for (i, g) in gs.iter().enumerate() {
            assert!((g.x - (x + 16. * i as f64)).abs() < 0.0002);
            assert!((g.end_x - g.x - 8.).abs() < 0.0002);
            assert_eq!(g.spacing, 8.);
            assert_eq!(g.kern, if i == 0 { 7.7504 } else { 0. });
            assert_eq!(p.fonts[g.font].ordering.as_deref(), Some("Japan1"));
            assert!(!p.fonts[g.font].has_to_unicode);
        }
        assert_eq!(gs[2].cid, 1657);
        assert_eq!(gs[2].code, 1657);
    }
}
