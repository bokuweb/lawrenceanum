use anyhow::{Context, Result};
use shingikai_client::{MinistryAdapter, MockAdapter, MojAdapter};
use std::path::Path;

fn make_adapter(ministry: &str, provider: &str) -> Box<dyn MinistryAdapter> {
    if provider == "mock" {
        return Box::new(MockAdapter);
    }
    match ministry {
        "moj" => Box::new(MojAdapter::new()),
        _ => Box::new(MockAdapter),
    }
}

pub fn run_fetch(ministry: &str, cache: &Path, provider: &str) -> Result<()> {
    let adapter = make_adapter(ministry, provider);
    let dir = cache.join("shingikai").join(ministry);
    std::fs::create_dir_all(&dir)?;

    let committees = adapter.list_committees()?;
    tracing::info!("shingikai-fetch: {} → {} committees", ministry, committees.len());

    let mut total = 0usize;
    for committee in &committees {
        let metas = match adapter.list_minutes(committee) {
            Ok(v) => v,
            Err(e) => { tracing::warn!("skip {committee}: {e:#}"); continue; }
        };
        for meta in &metas {
            let doc = match adapter.fetch_minutes(meta) {
                Ok(d) => d,
                Err(e) => { tracing::warn!("skip {}: {e:#}", meta.minutes_id); continue; }
            };
            let path = dir.join(format!("{}.json", meta.minutes_id));
            std::fs::write(&path, serde_json::to_string_pretty(&doc)?)
                .with_context(|| format!("write {}", path.display()))?;
            total += 1;
        }
    }
    tracing::info!("shingikai-fetch: {total} minutes saved");
    Ok(())
}

pub fn run_build_json(cache: &Path, public: &Path) -> Result<()> {
    let src = cache.join("shingikai");
    if !src.exists() {
        anyhow::bail!("no shingikai cache; run shingikai-fetch first");
    }
    let out = public.join("shingikai");
    std::fs::create_dir_all(&out)?;

    let mut index_entries: Vec<serde_json::Value> = Vec::new();
    for ministry_entry in std::fs::read_dir(&src)? {
        let ministry_dir = ministry_entry?.path();
        if !ministry_dir.is_dir() { continue; }
        let ministry_id = ministry_dir.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();
        let ministry_out = out.join(&ministry_id);
        std::fs::create_dir_all(&ministry_out)?;

        for entry in std::fs::read_dir(&ministry_dir)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") { continue; }
            let bytes = std::fs::read(&path)?;
            let doc: serde_json::Value = serde_json::from_slice(&bytes)?;
            let id = doc["minutes_id"].as_str().unwrap_or("").to_string();
            if id.is_empty() { continue; }
            std::fs::write(
                ministry_out.join(format!("{id}.json")),
                serde_json::to_string_pretty(&doc)?,
            )?;
            index_entries.push(serde_json::json!({
                "minutes_id": id,
                "ministry": doc["ministry"],
                "committee": doc["committee"],
                "date": doc["date"],
                "title": doc["title"],
            }));
        }
    }

    index_entries.sort_by(|a, b| {
        b["date"].as_str().unwrap_or("").cmp(a["date"].as_str().unwrap_or(""))
    });
    std::fs::write(
        out.join("index.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": 1,
            "count": index_entries.len(),
            "minutes": index_entries,
        }))?,
    )?;
    tracing::info!("shingikai-build-json: {} minutes written", index_entries.len());
    Ok(())
}
