//! Read-only original CFF rectangle evidence, independent of bbox Word grouping.
use sha2::{Digest, Sha256};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .expect("usage: painted_rectangles original.pdf");
    let pdf = std::fs::read(path)?;
    let sources = kanpo_pdf::extract_pages_with_aj1_sources(&pdf)?;
    let mut rows = Vec::new();
    let mut frames = Vec::new();
    for (page, source) in sources.iter().enumerate() {
        for (id, glyph) in source.page().glyphs.iter().enumerate() {
            if let Some(frame) = source.glyph_rectangle_frame(id) {
                frames.push(serde_json::json!({"page":page,"glyph":id,"font":glyph.font,"code":glyph.code,"cid":glyph.cid,"frame":frame}));
            }
            let rectangles = if let Some(b) = source.glyph_painted_rectangle(id) {
                vec![b]
            } else if let Some(boxes) = source.glyph_separate_painted_rectangles(id) {
                boxes.to_vec()
            } else {
                continue;
            };
            rows.push(serde_json::json!({"page":page,"glyph":id,"font":glyph.font,
                "code":glyph.code,"cid":glyph.cid,"rectangles":rectangles}));
        }
    }
    serde_json::to_writer(
        std::io::stdout().lock(),
        &serde_json::json!({
            "schema":1,"kind":"source_cff_rectangle_paint","document_sha256":format!("{:x}",Sha256::digest(&pdf)),
            "document_bytes":pdf.len(),"rows":rows,"frames":frames,"coverage_verified":false,
        }),
    )?;
    Ok(())
}
