//! Analyze complete PDFs without truncating an item at a page boundary.
//! Usage: cargo run -p kanpo-parser --example analyze_pdf -- input.pdf [next.pdf ...] output-dir
use std::path::Path;

fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    anyhow::ensure!(
        args.len() >= 2,
        "usage: analyze_pdf input.pdf [next.pdf ...] output-dir"
    );
    let mut pages = Vec::new();
    for input in &args[..args.len() - 1] {
        pages.extend(kanpo_parser::page_inputs(&std::fs::read(input)?)?);
    }
    let analysis = kanpo_parser::analyze_item(&pages);
    let output = Path::new(args.last().unwrap());
    std::fs::create_dir_all(output)?;
    std::fs::write(output.join("analysis.json"), serde_json::to_vec(&analysis)?)?;
    std::fs::write(output.join("text.txt"), &analysis.text)?;
    for (index, page) in pages.iter().enumerate() {
        std::fs::write(
            output.join(format!("page-{}.bbox.html", index + 1)),
            &page.xhtml,
        )?;
        std::fs::write(
            output.join(format!("page-{}.rules.json", index + 1)),
            serde_json::to_vec(&page.rules)?,
        )?;
    }
    eprintln!(
        "{} pages, {} articles",
        analysis.pages.len(),
        analysis.articles.len()
    );
    Ok(())
}
