use anyhow::{Context, Result};
use reiki_client::{GyoseiHttpProvider, MockProvider, Municipality, ReikiProvider, known_municipalities};
use std::path::Path;

fn make_provider(provider: &str) -> Box<dyn ReikiProvider> {
    match provider {
        "mock" => Box::new(MockProvider),
        _ => Box::new(GyoseiHttpProvider::new()),
    }
}

/// `lawpub reiki-fetch` の実装。
/// `.cache/reiki/{municipality_code}/{reiki_id}.json` に保存する。
pub fn run_fetch(
    municipality_codes: &[String],
    cache: &Path,
    provider: &str,
) -> Result<()> {
    let p = make_provider(provider);
    let all = known_municipalities();
    let targets: Vec<&Municipality> = if municipality_codes.is_empty() {
        all.iter().collect()
    } else {
        all.iter()
            .filter(|m| municipality_codes.contains(&m.code))
            .collect()
    };

    for m in &targets {
        tracing::info!("reiki-fetch: {}", m.name);
        let dir = cache.join("reiki").join(&m.code);
        std::fs::create_dir_all(&dir)?;

        let metas = match p.list_reiki(m) {
            Ok(v) => v,
            Err(e) => { tracing::warn!("skip {}: {e:#}", m.code); continue; }
        };

        for meta in &metas {
            let doc = match p.fetch_reiki(meta, m) {
                Ok(d) => d,
                Err(e) => { tracing::warn!("skip {}: {e:#}", meta.reiki_id); continue; }
            };
            let path = dir.join(format!("{}.json", meta.reiki_id));
            std::fs::write(&path, serde_json::to_string_pretty(&doc)?)
                .with_context(|| format!("write {}", path.display()))?;
        }
        tracing::info!("reiki-fetch: {} → {} reiki saved", m.code, metas.len());
    }
    Ok(())
}

/// `lawpub reiki-build-json` の実装。
pub fn run_build_json(cache: &Path, public: &Path) -> Result<()> {
    let src = cache.join("reiki");
    if !src.exists() {
        anyhow::bail!("no reiki cache; run reiki-fetch first");
    }
    let out = public.join("reiki");
    std::fs::create_dir_all(&out)?;

    let mut global_index: Vec<serde_json::Value> = Vec::new();

    for muni_entry in std::fs::read_dir(&src)? {
        let muni_dir = muni_entry?.path();
        if !muni_dir.is_dir() { continue; }
        let muni_code = muni_dir.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();
        let muni_out = out.join(&muni_code);
        std::fs::create_dir_all(&muni_out)?;

        let mut muni_index: Vec<serde_json::Value> = Vec::new();
        for entry in std::fs::read_dir(&muni_dir)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") { continue; }
            let bytes = std::fs::read(&path)?;
            let doc: serde_json::Value = serde_json::from_slice(&bytes)?;
            let reiki_id = doc["reiki_id"].as_str().unwrap_or("").to_string();
            if reiki_id.is_empty() { continue; }
            std::fs::write(
                muni_out.join(format!("{reiki_id}.json")),
                serde_json::to_string_pretty(&doc)?,
            )?;
            muni_index.push(serde_json::json!({
                "reiki_id": reiki_id,
                "title": doc["title"],
                "reiki_number": doc["reiki_number"],
                "enforced_date": doc["enforced_date"],
            }));
        }
        std::fs::write(
            muni_out.join("index.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "schema_version": 1,
                "municipality_code": muni_code,
                "count": muni_index.len(),
                "reiki": muni_index,
            }))?,
        )?;
        global_index.push(serde_json::json!({ "municipality_code": muni_code }));
    }

    std::fs::write(
        out.join("index.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": 1,
            "count": global_index.len(),
            "municipalities": global_index,
        }))?,
    )?;
    tracing::info!("reiki-build-json: {} municipalities written", global_index.len());
    Ok(())
}
