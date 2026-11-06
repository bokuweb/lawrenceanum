fn main() -> anyhow::Result<()> {
    let mut input=std::env::args().skip(1);
    let mut args=Vec::new();let mut raster_layouts=Vec::new();
    while let Some(arg)=input.next() {
        if arg=="--raster-layout" {
            let path=input.next().ok_or_else(||anyhow::anyhow!("--raster-layout requires a JSON file"))?;
            raster_layouts.push(serde_json::from_slice::<kanpo_parser::media::raster::Layout>(&std::fs::read(path)?)?);
        } else {args.push(arg);}
    }
    anyhow::ensure!(
        args.len() == 2 || args.len() == 3,
        "usage: analyze_media_pdf [--raster-layout report.json ...] original.pdf [paint-observations.json] new-output-directory"
    );
    let pdf = std::fs::read(&args[0])?;
    let mut analysis = if args.len() == 3 {
        kanpo_parser::media::analyze_pdf_with_observations(
            &pdf,
            serde_json::from_slice(&std::fs::read(&args[1])?)?,
        )?
    } else {
        kanpo_parser::media::analyze_pdf(&pdf)?
    };
    kanpo_parser::media::raster::attach(&mut analysis,raster_layouts)?;
    let output = std::path::Path::new(args.last().unwrap());
    std::fs::create_dir(output)?;
    for asset in &analysis.assets {
        std::fs::write(
            output.join(format!("asset-{}.encoded", asset.id)),
            &asset.encoded_bytes,
        )?;
    }
    std::fs::write(output.join("analysis.json"), serde_json::to_vec(&analysis)?)?;
    eprintln!(
        "{} image sources, {} body placements, {} page placements, {} placement issues, {} editable raster layouts, {} raster issues; recognition and final visibility unverified",
        analysis.assets.len(),
        analysis.placements.len(),
        analysis.page_placements.len(),
        analysis.unresolved.len(),
        analysis.assets.iter().filter(|a|a.raster_layout.is_some()).count(),
        analysis.assets.iter().filter_map(|a|a.raster_layout.as_ref()).map(|r|r.unresolved.len()).sum::<usize>()
    );
    Ok(())
}
