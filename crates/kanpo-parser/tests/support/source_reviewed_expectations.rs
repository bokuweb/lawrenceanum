//! Source-reviewed corrections to frozen historical golds.
//! Only declared Word-ID lists and original PDF device dimensions can change.
pub fn current(mut value: serde_json::Value) -> serde_json::Value {
    let reviews: serde_json::Value = serde_json::from_str(include_str!(
        "../data/workbook/historical-source-owners.json"
    ))
    .unwrap();
    let Some(pages) = value["pages"].as_array_mut() else {
        return value;
    };
    let sizes: serde_json::Value = serde_json::from_str(include_str!(
        "../data/workbook/historical-device-sizes.json"
    ))
    .unwrap();
    for page in pages {
        for review in sizes.as_array().unwrap() {
            let anchor = &review["anchor"];
            let id = anchor["id"].as_u64().unwrap() as usize;
            let matches = page["words"].as_array().unwrap().get(id).is_some_and(|w| {
                ["id", "raw", "x0", "y0", "x1", "y1"]
                    .iter()
                    .all(|k| w[*k] == anchor[*k])
            });
            if matches {
                let size = serde_json::json!([page["width"], page["height"]]);
                assert!(size == review["old_size"] || size == review["device_size"]);
                page["width"] = review["device_size"][0].clone();
                page["height"] = review["device_size"][1].clone();
            }
        }
        for review in reviews.as_array().unwrap() {
            let words = page["words"].as_array().unwrap();
            let anchor = review["anchor"]["id"].as_u64().unwrap() as usize;
            if words.get(anchor).is_none_or(|w| {
                w["raw"] != review["anchor"]["raw"]
                    || if let Some(b) = review["anchor"].get("source_box") {
                        &w["source_box"] != b
                    } else {
                        serde_json::json!([w["x0"], w["y0"], w["x1"], w["y1"]])
                            != review["anchor"]["display_box"]
                    }
            }) {
                continue;
            }
            let ids = review["current_owners"].as_array().unwrap();
            for (id, raw) in ids.iter().zip(review["raw_sequence"].as_array().unwrap()) {
                let id = id.as_u64().unwrap() as usize;
                assert_eq!(words[id]["id"], id);
                assert_eq!(&words[id]["raw"], raw);
                assert_eq!(words[id]["status"], "used");
            }
            let mut count = 0;
            for region in page["regions"].as_array_mut().unwrap() {
                for line in region["lines"].as_array_mut().unwrap() {
                    if line["words"] == review["old_owners"] {
                        line["words"] = review["current_owners"].clone();
                        count += 1;
                    } else if line["words"] == review["current_owners"] {
                        count += 1;
                    }
                }
            }
            assert_eq!(count, 1, "{}", review["source_pdf"]);
        }
    }
    value
}
