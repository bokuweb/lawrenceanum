//! [`kanpo_parser::analyze`] をブラウザから呼ぶための WASM ラッパー。
//!
//! playground（figma/ の `/kanpo-playground`）が `pdftotext -bbox-layout` の XHTML を渡し、
//! 解析結果を JSON 文字列で受け取る。ビルドは `figma/scripts/build-kanpo-wasm.sh`。

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

/// パーサの版（crate のバージョン + ビルド時の git 情報があればそれ）。
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn analyze_returns_json() {
        let xhtml = r#"<page width="595" height="842"><word xMin="100" yMin="100" xMax="108" yMax="124">本文</word></page>"#;
        let v: serde_json::Value = serde_json::from_str(&super::analyze(xhtml)).unwrap();
        assert_eq!(v["text"], "本文");
        assert_eq!(v["pages"][0]["words"][0]["status"], "used");
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
