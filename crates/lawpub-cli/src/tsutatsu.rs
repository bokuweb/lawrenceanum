use anyhow::{Context, Result};
use std::path::Path;
use tsutatsu_client::{known_sets, HttpProvider, MockProvider, TsutatsuProvider, TsutatsuSet, TsutatsuSource};

fn make_provider(provider: &str) -> Box<dyn TsutatsuProvider> {
    match provider {
        "mock" => Box::new(MockProvider),
        _ => Box::new(HttpProvider::new()),
    }
}

/// `lawpub tsutatsu-fetch` — 国税庁 法令解釈通達を取得し
/// `.cache/tsutatsu/{tax}.json` (TsutatsuSet) に保存する。
pub fn run_fetch(cache: &Path, provider: &str, max_pages: usize) -> Result<()> {
    let p = make_provider(provider);
    let dir = cache.join("tsutatsu");
    std::fs::create_dir_all(&dir)?;
    let fetched_at = chrono::Utc::now().to_rfc3339();

    for (tax, name, index_url) in known_sets() {
        let pages = match p.list_pages(&index_url) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("tsutatsu list_pages {tax} failed: {e:#}");
                continue;
            }
        };
        let take = if max_pages == 0 { pages.len() } else { max_pages.min(pages.len()) };
        let mut items = Vec::new();
        for page in pages.iter().take(take) {
            match p.fetch_page(page, &tax) {
                Ok(mut its) => items.append(&mut its),
                Err(e) => tracing::warn!("skip {page}: {e:#}"),
            }
        }
        tracing::info!("tsutatsu-fetch: {name} → {} items ({}/{} pages)", items.len(), take, pages.len());
        let set = TsutatsuSet {
            schema_version: 1,
            name,
            tax: tax.clone(),
            items,
            source: TsutatsuSource {
                provider: "nta".to_string(),
                fetched_at: fetched_at.clone(),
                index_url,
            },
        };
        std::fs::write(dir.join(format!("{tax}.json")), serde_json::to_string_pretty(&set)?)
            .with_context(|| format!("write tsutatsu {tax}"))?;
    }
    Ok(())
}

/// `lawpub tsutatsu-build-json` — `.cache/tsutatsu/*.json` →
/// `public/tsutatsu/{tax}.json` + index.json。
pub fn run_build_json(cache: &Path, public: &Path) -> Result<()> {
    let src = cache.join("tsutatsu");
    if !src.exists() {
        anyhow::bail!("no tsutatsu cache at {}; run tsutatsu-fetch first", src.display());
    }
    let out = public.join("tsutatsu");
    std::fs::create_dir_all(&out)?;

    let mut index: Vec<serde_json::Value> = Vec::new();
    for entry in std::fs::read_dir(&src)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let set: serde_json::Value = serde_json::from_slice(&std::fs::read(&path)?)?;
        let tax = set["tax"].as_str().unwrap_or("").to_string();
        if tax.is_empty() {
            continue;
        }
        std::fs::write(out.join(format!("{tax}.json")), serde_json::to_string_pretty(&set)?)?;
        index.push(serde_json::json!({
            "tax": tax,
            "name": set["name"],
            "count": set["items"].as_array().map(|a| a.len()).unwrap_or(0),
        }));
    }
    index.sort_by(|a, b| a["tax"].as_str().unwrap_or("").cmp(b["tax"].as_str().unwrap_or("")));
    std::fs::write(
        out.join("index.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": 1,
            "count": index.len(),
            "sets": index,
        }))?,
    )?;
    tracing::info!("tsutatsu-build-json: {} sets written", index.len());
    Ok(())
}
