//! [`kanpo_parser::analyze`] をブラウザから呼ぶための WASM ラッパー。
//!
//! playground（figma/ の `/kanpo-playground`）が座標付きテキスト（`pdftotext -bbox-layout` と同じ形の
//! XHTML）か PDF そのものを渡し、解析結果を JSON 文字列で受け取る。PDF の読み取りも pure Rust
//! （kanpo-pdf）なので poppler は要らない。ビルドは `figma/scripts/build-kanpo-wasm.sh`。

use wasm_bindgen::prelude::*;

/// `pdftotext -bbox-layout` の XHTML を解析し、[`kanpo_parser::Analysis`] を JSON で返す。
#[wasm_bindgen]
pub fn analyze(xhtml: &str) -> String {
    serde_json::to_string(&kanpo_parser::analyze(xhtml)).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
}

/// 複数ページの項目をまとめて解析する。`pages_json` は `[{"xhtml": "...", "rules": {...}|null}, ...]`
/// （rules は `.rules.json` の中身）。ページで切れた新旧対照表は 1 つの表につなぎ、罫線があれば
/// 表の中の表も復元する。[`kanpo_parser::Analysis`] を JSON で返す。
#[wasm_bindgen]
pub fn analyze_item(pages_json: &str) -> String {
    match serde_json::from_str::<Vec<kanpo_parser::PageInput>>(pages_json) {
        Ok(pages) => serde_json::to_string(&kanpo_parser::analyze_item(&pages)).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}")),
        Err(e) => format!("{{\"error\":\"invalid input: {e}\"}}"),
    }
}

/// PDF の各ページを、[`analyze_item`] に渡す入力（`[{"xhtml": "...", "rules": {...}}, ...]`）の
/// JSON にする。項目のページを別々の PDF で持っているときは、ページごとに呼んで並べて渡す。
#[wasm_bindgen]
pub fn page_inputs(pdf: &[u8]) -> String {
    match kanpo_parser::page_inputs(pdf) {
        Ok(pages) => serde_json::to_string(&pages).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}")),
        Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
    }
}

/// 1 始まりの PDF ページ番号と XHTML の語の枠から、実際の表示枠を返す。
#[wasm_bindgen]
pub fn display_boxes(pdf: &[u8], page_number: usize, boxes_json: &str) -> String {
    let result = (|| -> Result<_, Box<dyn std::error::Error>> {
        let boxes: Vec<[f64; 4]> = serde_json::from_str(boxes_json)?;
        let doc = kanpo_pdf::Document::load(pdf.to_vec())?;
        let pages = doc.pages();
        let page = pages.get(page_number.wrapping_sub(1)).ok_or("invalid PDF page number")?;
        let page = kanpo_pdf::extract_page(&doc, page);
        Ok(kanpo_pdf::bbox::display_boxes(&page, &boxes))
    })();
    match result {
        Ok(boxes) => serde_json::to_string(&boxes).unwrap(),
        Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
    }
}

/// PDF（項目の全ページを 1 つにしたもの、または 1 ページ）をそのまま解析し、
/// [`kanpo_parser::Analysis`] を JSON で返す。
#[wasm_bindgen]
pub fn analyze_pdf(pdf: &[u8]) -> String {
    match kanpo_parser::page_inputs(pdf) {
        Ok(pages) => serde_json::to_string(&kanpo_parser::analyze_item(&pages)).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}")),
        Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
    }
}

/// パーサの版（crate のバージョン + ビルド時の git 情報があればそれ）。
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn corpus_display_boxes_match_pdf_glyphs() {
        let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../kanpo-parser/tests/corpus"));
        let mut count = 0;
        for dir in std::fs::read_dir(root).unwrap() {
            let dir = dir.unwrap().path();
            if !dir.is_dir() { continue; }
            for file in std::fs::read_dir(&dir).unwrap() {
                let pdf_path = file.unwrap().path();
                if pdf_path.extension().is_none_or(|e| e != "pdf") { continue; }
                let xhtml_path = pdf_path.with_extension("bbox.html");
                let xhtml = std::fs::read_to_string(&xhtml_path).unwrap();
                let analysis = kanpo_parser::analyze(&xhtml);
                let words = &analysis.pages[0].words;
                let boxes: Vec<_> = words.iter().map(|w| w.source_box.map(f64::from)).collect();
                let pdf = std::fs::read(&pdf_path).unwrap();
                let result: Vec<Option<[f64; 4]>> = serde_json::from_str(&super::display_boxes(&pdf, 1, &serde_json::to_string(&boxes).unwrap())).unwrap();
                let missing: Vec<_> = words.iter().zip(&result).filter(|(_, b)| b.is_none()).collect();
                assert!(missing.is_empty(), "{}: unmatched words {:?}", pdf_path.strip_prefix(root).unwrap().display(), missing.iter().map(|(w, _)| (&w.raw, w.source_box)).collect::<Vec<_>>());
                assert_eq!(result.len(), words.len());
                assert!(result.iter().flatten().all(|b| b.iter().all(|v| v.is_finite()) && b[0] < b[2] && b[1] < b[3]));
                count += 1;
            }
        }
        assert!(count >= 24);
    }

    #[test]
    fn analyze_returns_json() {
        let xhtml = r#"<page width="595" height="842"><word xMin="100" yMin="100" xMax="108" yMax="124">本文</word></page>"#;
        let v: serde_json::Value = serde_json::from_str(&super::analyze(xhtml)).unwrap();
        assert_eq!(v["text"], "本文");
        assert_eq!(v["pages"][0]["words"][0]["status"], "used");
    }

    #[test]
    fn analyze_pdf_reads_kanpo_pdf() {
        let pdf = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../kanpo-parser/tests/corpus/2026-10-13-gougai224-p69/page.pdf")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&super::analyze_pdf(&pdf)).unwrap();
        assert!(v["text"].as_str().unwrap().contains("前払式特定取引業者営業保証金取戻し公告"));
        let pages: serde_json::Value = serde_json::from_str(&super::page_inputs(&pdf)).unwrap();
        assert_eq!(pages.as_array().unwrap().len(), 1);
        assert!(pages[0]["xhtml"].as_str().unwrap().contains("<word"));
    }

    #[test]
    fn analyze_pdf_reports_broken_input() {
        let v: serde_json::Value = serde_json::from_str(&super::analyze_pdf(b"not a pdf")).unwrap();
        assert!(v["error"].is_string());
    }

    #[test]
    fn analyze_item_accepts_pages_with_rules() {
        let page = r#"<page width="595" height="842"><word xMin="100" yMin="100" xMax="108" yMax="124">本文</word></page>"#;
        let input = serde_json::json!([{ "xhtml": page, "rules": null }, { "xhtml": page }]).to_string();
        let v: serde_json::Value = serde_json::from_str(&super::analyze_item(&input)).unwrap();
        assert_eq!(v["pages"].as_array().unwrap().len(), 2);
        assert_eq!(v["text"], "本文\n本文");
    }
}
