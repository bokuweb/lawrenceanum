use anyhow::{Context, Result};
use procurement_client::{HttpProvider, MockProvider, ProcurementProvider};
use std::path::Path;

fn make_provider(provider: &str) -> Box<dyn ProcurementProvider> {
    match provider {
        "mock" => Box::new(MockProvider),
        _ => Box::new(HttpProvider::new()),
    }
}

/// `lawpub procurement-fetch` の実装。
/// `.cache/procurement/{from}_{to}.json` に保存する。
pub fn run_fetch(from: &str, to: &str, cache: &Path, provider: &str) -> Result<()> {
    let p = make_provider(provider);
    let batch = p.fetch_range(from, to)?;
    let dir = cache.join("procurement");
    std::fs::create_dir_all(&dir)?;
    let fname = format!("{from}_{to}.json");
    let path = dir.join(&fname);
    std::fs::write(&path, serde_json::to_string_pretty(&batch)?)
        .with_context(|| format!("write {}", path.display()))?;
    tracing::info!("procurement-fetch: {} items saved to {fname}", batch.items.len());
    Ok(())
}

/// `lawpub procurement-build-json` の実装。
/// `.cache/procurement/*.json` → `public/procurement/{id}.json` + `index.json`
pub fn run_build_json(cache: &Path, public: &Path) -> Result<()> {
    let src_dir = cache.join("procurement");
    if !src_dir.exists() {
        anyhow::bail!("no procurement cache; run procurement-fetch first");
    }
    let out_dir = public.join("procurement");
    std::fs::create_dir_all(&out_dir)?;

    let mut index_entries: Vec<serde_json::Value> = Vec::new();

    for entry in std::fs::read_dir(&src_dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let bytes = std::fs::read(&path)?;
        let batch: serde_json::Value = serde_json::from_slice(&bytes)?;
        let items = batch["items"].as_array().cloned().unwrap_or_default();
        for item in &items {
            let item_id = item["item_id"].as_str().unwrap_or("").to_string();
            if item_id.is_empty() {
                continue;
            }
            let dest = out_dir.join(format!("{item_id}.json"));
            std::fs::write(&dest, serde_json::to_string_pretty(item)?)?;
            index_entries.push(serde_json::json!({
                "item_id": item_id,
                "title": item["title"],
                "organization": item["organization"],
                "notice_type": item["notice_type"],
                "publish_date": item["publish_date"],
            }));
        }
    }

    index_entries.sort_by(|a, b| {
        let da = a["publish_date"].as_str().unwrap_or("");
        let db = b["publish_date"].as_str().unwrap_or("");
        db.cmp(da)
    });
    let index = serde_json::json!({
        "schema_version": 1,
        "count": index_entries.len(),
        "items": index_entries,
    });
    std::fs::write(out_dir.join("index.json"), serde_json::to_string_pretty(&index)?)?;
    tracing::info!("procurement-build-json: {} items written", index_entries.len());
    Ok(())
}
