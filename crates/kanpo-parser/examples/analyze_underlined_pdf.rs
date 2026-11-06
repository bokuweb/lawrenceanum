fn main() -> anyhow::Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    anyhow::ensure!(
        args.len() == 2,
        "usage: analyze_underlined_pdf original.pdf new-output-directory"
    );
    let analysis = kanpo_parser::source_underlines::analyze_pdf(&std::fs::read(&args[0])?)?;
    let output = std::path::Path::new(&args[1]);
    std::fs::create_dir(output)?;
    std::fs::write(output.join("analysis.json"), serde_json::to_vec(&analysis)?)?;
    eprintln!(
        "{} applied fields, {} unresolved fields, {} source glyph bindings",
        analysis.applied_fields.len(),
        analysis.unresolved_fields.len(),
        analysis.glyph_bindings.len()
    );
    Ok(())
}
