//! [`kanpo_docx`] をブラウザから呼ぶための WASM ラッパー。
//!
//! 1. `pageImages(pdf)` でページの画像（表示の向きに起こした 8 bit グレースケール）を取り出す。
//! 2. ページの画像をブラウザの OCR（ndlocr-lite-rs の WASM 版）にかけ、NDLOCR の trace JSON を得る。
//! 3. `convert(pdf, traces)` に、`pageImages` と同じ並びの trace（OCR しない画像は `null`）を渡して DOCX を得る。
//!
//! ビルドは `figma/scripts/build-kanpo-docx-wasm.sh`。

use js_sys::{Array, Object, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;

fn set(o: &Object, k: &str, v: impl Into<JsValue>) -> Result<(), JsError> {
    Reflect::set(o, &k.into(), &v.into()).map(|_| ()).map_err(|_| JsError::new("set"))
}

/// PDF のページの画像を、`{page, width, height, left, top, scale, pageWidth, pageHeight, pixels}` の配列で返す。
/// `pixels` は 1 画素 1 バイト（0 が黒）。`page` は 0 始まり。位置・大きさの単位は pt（左上原点）。
#[wasm_bindgen(js_name = pageImages)]
pub fn page_images(pdf: &[u8]) -> Result<Array, JsError> {
    let images = kanpo_docx::page_images(pdf).map_err(|e| JsError::new(&format!("{e:#}")))?;
    let out = Array::new();
    for img in images {
        let o = Object::new();
        set(&o, "page", img.page as u32)?;
        set(&o, "width", img.image.width)?;
        set(&o, "height", img.image.height)?;
        set(&o, "left", img.left)?;
        set(&o, "top", img.top)?;
        set(&o, "scale", img.scale)?;
        set(&o, "pageWidth", img.page_width)?;
        set(&o, "pageHeight", img.page_height)?;
        set(&o, "pixels", Uint8Array::from(img.image.pixels.as_slice()))?;
        out.push(&o);
    }
    Ok(out)
}

/// PDF を DOCX にする。`traces_json` は `pageImages` と同じ並びの NDLOCR の trace（`recognize-page --trace-json`
/// と同じ形のオブジェクト。OCR しない画像は `null`）の JSON 配列。空文字列か `[]` なら OCR せずに書く。
#[wasm_bindgen]
pub fn convert(pdf: &[u8], traces_json: &str) -> Result<Vec<u8>, JsError> {
    let err = |e: anyhow::Error| JsError::new(&format!("{e:#}"));
    let traces: Vec<serde_json::Value> = if traces_json.trim().is_empty() { vec![] } else { serde_json::from_str(traces_json).map_err(|e| JsError::new(&format!("traces: {e}")))? };
    let ocr = if traces.iter().all(serde_json::Value::is_null) {
        vec![]
    } else {
        let images = kanpo_docx::page_images(pdf).map_err(err)?;
        if images.len() != traces.len() {
            return Err(JsError::new(&format!("{} traces for {} page images", traces.len(), images.len())));
        }
        let pages = kanpo_docx::page_count(pdf).map_err(err)?;
        let mut per_page: Vec<Vec<(&kanpo_docx::PageImage, Vec<kanpo_docx::OcrRegion>)>> = vec![Vec::new(); pages];
        for (img, trace) in images.iter().zip(&traces) {
            if trace.is_null() {
                continue;
            }
            let page = kanpo_docx::OcrPage::from_ndlocr_trace(&trace.to_string()).map_err(err)?;
            if let Some(slot) = per_page.get_mut(img.page) {
                slot.push((img, page.regions));
            }
        }
        per_page.iter().map(|imgs| kanpo_docx::ocr_page(imgs)).collect()
    };
    kanpo_docx::convert(pdf, &ocr).map_err(err)
}
