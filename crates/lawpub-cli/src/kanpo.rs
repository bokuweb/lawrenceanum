use anyhow::{Context, Result};
use chrono::Utc;
use kanpo_client::{KanpoIssue, KanpoProvider, MockKanpoProvider};
use kanpo_linker::{match_event, AUTO_LINK_THRESHOLD};
use serde::Serialize;
use serde_json::json;
use std::path::{Path, PathBuf};

/// `lawpub kanpo-fetch` — 指定日の官報を取得して `.cache/kanpo/{date}.json` に保存。
pub fn run_fetch(date: &str, cache: &Path) -> Result<()> {
    let provider = MockKanpoProvider; // Phase 3 初期はモックのみ。
    let kd = provider.fetch_date(date)?;
    let dir = cache.join("kanpo");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.json", date));
    let bytes = serde_json::to_vec_pretty(&kd)?;
    std::fs::write(&path, bytes).with_context(|| format!("write {}", path.display()))?;
    tracing::info!("wrote kanpo cache: {}", path.display());
    Ok(())
}

/// `lawpub kanpo-link` — `.cache/kanpo/*.json` を読み、`public/laws/*/timeline.json`
/// の各イベントに官報マッチングを書き戻し、`public/kanpo/{date}/index.json` を生成する。
pub fn run_link(public: &Path) -> Result<()> {
    let cache_dir = PathBuf::from(".cache/kanpo");
    if !cache_dir.exists() {
        tracing::info!("no kanpo cache; skipping kanpo-link");
        return Ok(());
    }

    // 1) public/kanpo/{date}/index.json を出力。
    let mut by_date: Vec<(String, kanpo_client::KanpoDate)> = Vec::new();
    for f in std::fs::read_dir(&cache_dir)? {
        let f = f?;
        if f.path().extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let kd: kanpo_client::KanpoDate =
            serde_json::from_slice(&std::fs::read(f.path())?)?;
        by_date.push((kd.date.clone(), kd));
    }
    by_date.sort_by(|a, b| a.0.cmp(&b.0));

    for (date, kd) in &by_date {
        let dir = public.join("kanpo").join(date);
        std::fs::create_dir_all(&dir)?;
        write_json_pretty(
            &dir.join("index.json"),
            &json!({
                "date": date,
                "generated_at": Utc::now().to_rfc3339(),
                "issues": kd.issues,
            }),
        )?;
    }

    // 2) timeline.json に kanpo マッチングを反映。
    let laws_dir = public.join("laws");
    if !laws_dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(&laws_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let timeline_path = entry.path().join("timeline.json");
        if !timeline_path.exists() {
            continue;
        }
        let mut tl: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&timeline_path)?)?;
        let events = match tl.get_mut("events").and_then(|e| e.as_array_mut()) {
            Some(e) => e,
            None => continue,
        };

        for ev in events.iter_mut() {
            let promulgation = ev
                .get("promulgation_date")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let law_num = ev
                .get("amending_law_num")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let mut best: Option<(f64, &str, &KanpoIssue, Vec<&'static str>)> = None;
            for (date, kd) in &by_date {
                for issue in &kd.issues {
                    let r = match_event(
                        promulgation.as_deref(),
                        law_num.as_deref(),
                        None, // event title は現状 timeline に持たないため未使用。
                        None,
                        issue,
                    );
                    if r.confidence > 0.0
                        && best
                            .as_ref()
                            .map(|b| r.confidence > b.0)
                            .unwrap_or(true)
                    {
                        best = Some((r.confidence, date.as_str(), issue, r.match_reasons));
                    }
                }
            }
            if let Some((conf, date, _issue, reasons)) = best {
                let linked = conf >= AUTO_LINK_THRESHOLD;
                ev["kanpo"] = json!({
                    "linked": linked,
                    "path": format!("kanpo/{}/index.json", date),
                    "confidence": conf,
                    "match_reasons": reasons,
                });
            }
        }

        let bytes = serde_json::to_vec_pretty(&tl)?;
        std::fs::write(&timeline_path, bytes)?;
    }

    // timeline と kanpo/index.json を書き換えたので manifest を再計算。
    crate::build::rebuild_manifest(public)?;
    Ok(())
}

fn write_json_pretty<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(value)?;
    std::fs::write(path, bytes)?;
    Ok(())
}
