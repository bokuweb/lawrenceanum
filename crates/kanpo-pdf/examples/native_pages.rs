//! Original interpreted glyphs and paths for independent paint-coverage audits.
use sha2::{Digest, Sha256};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = std::env::args()
        .nth(1)
        .expect("usage: native_pages original.pdf");
    let bytes = std::fs::read(input)?;
    let sources = kanpo_pdf::extract_pages_with_xobject_sources(&bytes)?;
    let pages = sources.iter().map(|p| p.page()).collect::<Vec<_>>();
    let inventory = sources
        .iter()
        .map(|p| {
            serde_json::json!({
                "image_draw_count": p.images().len(),
                "capture_gap_count": p.capture_gaps().len(),
            })
        })
        .collect::<Vec<_>>();
    let digest = format!("{:x}", Sha256::digest(&bytes));
    serde_json::to_writer(
        std::io::stdout().lock(),
        &serde_json::json!({
            "schema": 2,
            "kind": "native_page_snapshot",
            "document_sha256": digest,
            "document_bytes": bytes.len(),
            "pages": pages,
            "xobject_inventory": inventory,
        }),
    )?;
    Ok(())
}
