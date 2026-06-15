use anyhow::{Context, Result};
use estat_client::{EstatProvider, HttpProvider, MockProvider, FISCAL_STATS};
use std::path::Path;

fn make_provider(provider: &str) -> Box<dyn EstatProvider> {
    match provider {
        "mock" => Box::new(MockProvider),
        _ => Box::new(HttpProvider::new().expect("LAWPUB_ESTAT_APP_ID must be set")),
    }
}

pub fn run_fetch(cache: &Path, provider: &str) -> Result<()> {
    let p = make_provider(provider);
    let dir = cache.join("budget");
    std::fs::create_dir_all(&dir)?;

    for (stats_id, title) in FISCAL_STATS {
        let dataset = match p.fetch_stats(stats_id, title) {
            Ok(d) => d,
            Err(e) => { tracing::warn!("skip {stats_id}: {e:#}"); continue; }
        };
        let path = dir.join(format!("{stats_id}.json"));
        std::fs::write(&path, serde_json::to_string_pretty(&dataset)?)
            .with_context(|| format!("write {}", path.display()))?;
        tracing::info!("budget-fetch: {stats_id} ({}) → {} values", title, dataset.values.len());
    }
    Ok(())
}

pub fn run_build_json(cache: &Path, public: &Path) -> Result<()> {
    let src = cache.join("budget");
    if !src.exists() {
        anyhow::bail!("no budget cache; run budget-fetch first");
    }
    let out = public.join("budget");
    std::fs::create_dir_all(&out)?;

    let mut index_entries: Vec<serde_json::Value> = Vec::new();
    for entry in std::fs::read_dir(&src)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") { continue; }
        let bytes = std::fs::read(&path)?;
        let dataset: serde_json::Value = serde_json::from_slice(&bytes)?;
        let stats_id = dataset["stats_data_id"].as_str().unwrap_or("").to_string();
        if stats_id.is_empty() { continue; }
        std::fs::write(
            out.join(format!("{stats_id}.json")),
            serde_json::to_string_pretty(&dataset)?,
        )?;
        index_entries.push(serde_json::json!({
            "stats_data_id": stats_id,
            "title": dataset["title"],
            "value_count": dataset["values"].as_array().map(|a| a.len()).unwrap_or(0),
        }));
    }

    std::fs::write(
        out.join("index.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": 1,
            "count": index_entries.len(),
            "datasets": index_entries,
        }))?,
    )?;
    tracing::info!("budget-build-json: {} datasets written", index_entries.len());
    Ok(())
}
