use kanpo_parser::{analyze_item, page_inputs};
fn target() -> serde_json::Value {
    serde_json::to_value(analyze_item(
        &page_inputs(include_bytes!(
            "archive/pages/20261015h01808/20261015h018080006.pdf"
        ))
        .unwrap(),
    ))
    .unwrap()
}
#[test]
fn one_native_record_has_all_five_fields_and_twenty_original_ids() {
    let a = target();
    let wanted = "五一三八　志布志港　志布志市　公有　土地：約一〇、〇〇〇平方メートル建物：約八〇平方メートル工作物：水道等訓練施設として新規提供する。使用期間：一令和八年十月十九日から同月二十九日までの間二必要に応じ、訓練の展開及び撤収のための追加期間志布志港の施設の一部を、地位協定第二条四⒝の適用ある施設及び区域として提供する。提供期間中は、地位協定の関連ある条項が適用される。";
    let ls: Vec<_> = a["pages"][0]["regions"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| r["lines"].as_array().unwrap())
        .filter(|l| l["text"] == wanted)
        .collect();
    assert_eq!(ls.len(), 1);
    assert_eq!(
        ls[0]["words"],
        serde_json::json!([
            27, 28, 29, 30, 31, 37, 40, 42, 44, 46, 47, 49, 51, 52, 55, 58, 60, 62, 68, 70
        ])
    );
    assert!(a["articles"][0]["text"].as_str().unwrap().contains(wanted));
    assert_eq!(a["articles"].as_array().unwrap().len(), 3);
}
#[test]
fn one_record_preserves_all_original_words_and_both_later_road_articles() {
    let a = target();
    let b: serde_json::Value =
        serde_json::from_str(include_str!("testdata/facility1808-before51.json")).unwrap();
    for k in ["width", "height", "layout"] {
        assert_eq!(a["pages"][0][k], b["pages"][0][k]);
    }
    let aw = a["pages"][0]["words"].as_array().unwrap();
    let bw = b["pages"][0]["words"].as_array().unwrap();
    assert_eq!(aw.len(), bw.len());
    for (x, y) in aw.iter().zip(bw) {
        for k in ["id", "raw", "text", "status"] {
            assert_eq!(x[k], y[k]);
        }
        for k in ["x0", "x1", "y0", "y1"] {
            assert_eq!(
                x[k].as_f64().unwrap() as f32,
                y[k].as_f64().unwrap() as f32,
                "original f32 coordinate exact after JSON reader"
            );
        }
    }
    for i in [1, 2] {
        assert_eq!(
            a["articles"][i], b["articles"][i],
            "later road article and Document including Table must be exact"
        );
    }
    let tables = |v: &serde_json::Value| {
        v["pages"][0]["regions"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|r| r["lines"].as_array().unwrap())
            .filter_map(|l| l.get("table").cloned())
            .collect::<Vec<_>>()
    };
    fn source_f32(v: &mut serde_json::Value) {
        match v {
            serde_json::Value::Object(m) => {
                for (k, x) in m {
                    if matches!(k.as_str(), "x0" | "x1" | "y0" | "y1") {
                        if let Some(n) = x.as_f64() {
                            *x = serde_json::to_value(n as f32).unwrap();
                        }
                    } else {
                        source_f32(x)
                    }
                }
            }
            serde_json::Value::Array(a) => {
                for x in a {
                    source_f32(x)
                }
            }
            _ => {}
        }
    }
    let (mut at, mut bt) = (serde_json::json!(tables(&a)), serde_json::json!(tables(&b)));
    source_f32(&mut at);
    source_f32(&mut bt);
    assert_eq!(at, bt);
}

#[test]
fn neighboring_20261015h018080005_whole_body_is_unchanged() {
    let a = analyze_item(
        &page_inputs(include_bytes!(
            "archive/pages/20261015h01808/20261015h018080005.pdf"
        ))
        .unwrap(),
    );
    assert_eq!(
        a.text,
        include_str!("testdata/20261015h018080005-before51.txt")
    );
}

#[test]
fn neighboring_20261015h018080007_whole_body_is_unchanged() {
    let a = analyze_item(
        &page_inputs(include_bytes!(
            "archive/pages/20261015h01808/20261015h018080007.pdf"
        ))
        .unwrap(),
    );
    assert_eq!(
        a.text,
        include_str!("testdata/20261015h018080007-before51.txt")
    );
}
