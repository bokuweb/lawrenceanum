//! PDF から、解析の入力になる座標付きテキスト（`.bbox.html`）と罫線（`.rules.json`）を作る。
//! コーパスへの追加や、playground に手元の PDF を読ませるときに使う（kanpo-pdf で読むので poppler は要らない）。
//!
//! ```text
//! cargo run -p kanpo-parser --example prepare -- page.pdf
//! # => page.bbox.html, page.rules.json
//! ```

fn main() -> anyhow::Result<()> {
    for path in std::env::args().skip(1) {
        let pdf = std::fs::read(&path)?;
        let stem = path.strip_suffix(".pdf").unwrap_or(&path);
        std::fs::write(format!("{stem}.bbox.html"), kanpo_parser::bbox_xhtml(&pdf)?)?;
        let rules = kanpo_parser::page_rules(&pdf)?;
        std::fs::write(format!("{stem}.rules.json"), serde_json::to_string(&rules)?)?;
        eprintln!("{stem}: {} vertical / {} horizontal rules", rules.vertical.len(), rules.horizontal.len());
    }
    Ok(())
}
