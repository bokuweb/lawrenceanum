use anyhow::{Context, Result};
use pubcomment_client::{HttpProvider, MockProvider, PubcommentProvider};
use std::path::Path;

fn make_provider(provider: &str) -> Box<dyn PubcommentProvider> {
    match provider {
        "mock" => Box::new(MockProvider),
        _ => Box::new(HttpProvider::new()),
    }
}

/// `lawpub pubcomment-fetch` の実装。
/// 結果公示済み案件を全ページ取得し `.cache/pubcomment/{case_id}.json` に保存する。
pub fn run_fetch(cache: &Path, provider: &str, max_pages: u32) -> Result<()> {
    let p = make_provider(provider);
    let dir = cache.join("pubcomment");
    std::fs::create_dir_all(&dir)?;

    let mut total = 0usize;
    for page in 1..=max_pages {
        let cases = p.fetch_case_list(page)?;
        if cases.is_empty() {
            break;
        }
        for meta in &cases {
            let mut detail = match p.fetch_case_detail(&meta.case_id) {
                Ok(d) => d,
                Err(e) => {
                    tracing::warn!("skip {}: {e:#}", meta.case_id);
                    continue;
                }
            };
            // 一覧側のメタ (所管省庁・結果の公示日・案件名) で詳細の欠損を補完する。
            if detail.ministry.is_none() {
                detail.ministry = meta.ministry.clone();
            }
            if detail.result_published.is_none() {
                detail.result_published = meta.result_published.clone();
            }
            if detail.title.is_empty() {
                detail.title = meta.title.clone();
            }
            let path = dir.join(format!("{}.json", meta.case_id));
            std::fs::write(&path, serde_json::to_string_pretty(&detail)?)
                .with_context(|| format!("write {}", path.display()))?;
            total += 1;
        }
        tracing::info!("pubcomment-fetch: page={page} done ({total} total)");
    }
    tracing::info!("pubcomment-fetch: {total} cases saved");
    Ok(())
}

/// `lawpub pubcomment-build-json` の実装。
/// `.cache/pubcomment/*.json` → `public/pubcomment/{case_id}.json` + `index.json`
pub fn run_build_json(cache: &Path, public: &Path) -> Result<()> {
    let src_dir = cache.join("pubcomment");
    if !src_dir.exists() {
        anyhow::bail!("no pubcomment cache at {}; run pubcomment-fetch first", src_dir.display());
    }
    let out_dir = public.join("pubcomment");
    std::fs::create_dir_all(&out_dir)?;

    let mut index_entries: Vec<serde_json::Value> = Vec::new();

    for entry in std::fs::read_dir(&src_dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let bytes = std::fs::read(&path)?;
        let detail: serde_json::Value = serde_json::from_slice(&bytes)?;
        let case_id = detail["case_id"].as_str().unwrap_or("").to_string();
        if case_id.is_empty() {
            continue;
        }
        let dest = out_dir.join(format!("{case_id}.json"));
        std::fs::write(&dest, serde_json::to_string_pretty(&detail)?)?;
        index_entries.push(serde_json::json!({
            "case_id": case_id,
            "title": detail["title"],
            "ministry": detail["ministry"],
            "result_published": detail["result_published"],
            "related_law_name": detail["related_law_name"],
        }));
    }

    index_entries.sort_by(|a, b| {
        let da = a["result_published"].as_str().unwrap_or("");
        let db = b["result_published"].as_str().unwrap_or("");
        db.cmp(da)
    });
    let index = serde_json::json!({
        "schema_version": 1,
        "count": index_entries.len(),
        "cases": index_entries,
    });
    std::fs::write(out_dir.join("index.json"), serde_json::to_string_pretty(&index)?)?;
    tracing::info!("pubcomment-build-json: {} cases written", index_entries.len());
    Ok(())
}
