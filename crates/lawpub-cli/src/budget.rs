use anyhow::{Context, Result};
use estat_client::{EstatProvider, HttpProvider, MockProvider, FISCAL_STATS};
use std::collections::HashSet;
use std::path::Path;

fn make_provider(provider: &str) -> Box<dyn EstatProvider> {
    match provider {
        "mock" => Box::new(MockProvider),
        _ => Box::new(HttpProvider::new().expect("LAWPUB_ESTAT_APP_ID must be set")),
    }
}

pub fn run_fetch(cache: &Path, provider: &str) -> Result<()> {
    let p = make_provider(provider);
    run_fetch_with_provider(cache, p.as_ref())
}

fn run_fetch_with_provider(cache: &Path, provider: &dyn EstatProvider) -> Result<()> {
    // 全統計の取得が成功してからキャッシュを書き換える。API障害時に last-good を
    // 壊さず、統計表IDを差し替えた際は旧IDのJSONを確実に除去する。
    let mut datasets = Vec::with_capacity(FISCAL_STATS.len());
    for (stats_id, title) in FISCAL_STATS {
        let dataset = provider
            .fetch_stats(stats_id, title)
            .with_context(|| format!("fetch e-Stat dataset {stats_id} ({title})"))?;
        tracing::info!(
            "budget-fetch: {stats_id} ({}) → {} values",
            title,
            dataset.values.len()
        );
        datasets.push(dataset);
    }

    let dir = cache.join("budget");
    std::fs::create_dir_all(&dir)?;
    let active_ids: HashSet<&str> = FISCAL_STATS.iter().map(|(id, _)| *id).collect();
    for dataset in datasets {
        let path = dir.join(format!("{}.json", dataset.stats_data_id));
        std::fs::write(&path, serde_json::to_string_pretty(&dataset)?)
            .with_context(|| format!("write {}", path.display()))?;
    }
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Some(stats_id) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        if !active_ids.contains(stats_id) {
            std::fs::remove_file(&path)
                .with_context(|| format!("remove stale budget cache {}", path.display()))?;
            tracing::info!("budget-fetch: removed stale dataset {stats_id}");
        }
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
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let bytes = std::fs::read(&path)?;
        let dataset: serde_json::Value = serde_json::from_slice(&bytes)?;
        let stats_id = dataset["stats_data_id"].as_str().unwrap_or("").to_string();
        if stats_id.is_empty() {
            continue;
        }
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
    tracing::info!(
        "budget-build-json: {} datasets written",
        index_entries.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_removes_datasets_no_longer_tracked() {
        let root = std::env::temp_dir().join(format!(
            "lawpub-budget-test-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("unnamed")
        ));
        let budget = root.join("budget");
        std::fs::create_dir_all(&budget).unwrap();
        std::fs::write(budget.join("obsolete.json"), "{}").unwrap();

        run_fetch_with_provider(&root, &MockProvider).unwrap();

        assert!(!budget.join("obsolete.json").exists());
        assert_eq!(
            std::fs::read_dir(&budget)
                .unwrap()
                .filter_map(|entry| entry.ok())
                .filter(
                    |entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("json")
                )
                .count(),
            FISCAL_STATS.len()
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
