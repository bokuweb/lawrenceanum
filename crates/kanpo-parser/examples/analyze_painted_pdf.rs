//! Explicitly include unverified paint observations with their native analysis.
fn main() -> anyhow::Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    anyhow::ensure!(
        args.len() == 3,
        "usage: analyze_painted_pdf original.pdf observations.json new-output-directory"
    );
    let pdf = std::fs::read(&args[0])?;
    let report = serde_json::from_slice(&std::fs::read(&args[1])?)?;
    let analysis = kanpo_parser::painted_text::analyze_pdf(&pdf, report)?;
    let output = std::path::Path::new(&args[2]);
    std::fs::create_dir(output)?;
    std::fs::write(output.join("analysis.json"), serde_json::to_vec(&analysis)?)?;
    std::fs::write(output.join("native.txt"), &analysis.native.text)?;
    std::fs::write(output.join("observed.txt"), &analysis.observed.text)?;
    eprintln!(
        "{} source-bound readings, {} unresolved; recognition remains unverified",
        analysis.bindings.len(),
        analysis.unresolved.len()
    );
    Ok(())
}
