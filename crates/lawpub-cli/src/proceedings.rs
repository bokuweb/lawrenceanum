use anyhow::{Context, Result};
use kokkai_client::{HttpProvider, KokkaiProvider, MockProvider, normalize_meeting};
use std::path::Path;

fn make_provider(provider: &str) -> Box<dyn KokkaiProvider> {
    match provider {
        "mock" => Box::new(MockProvider),
        _ => Box::new(HttpProvider::new()),
    }
}

/// `lawpub proceedings-fetch --session N` の実装。
/// `.cache/proceedings/{session}/{meeting_id}.json` に生 JSON を保存する。
pub fn run_fetch(session: u32, cache: &Path, provider: &str) -> Result<()> {
    let p = make_provider(provider);
    let batch = p.fetch_session(session)?;
    let dir = cache.join("proceedings").join(session.to_string());
    std::fs::create_dir_all(&dir)?;
    for fetched in &batch.meetings {
        let path = dir.join(format!("{}.json", fetched.meeting_id));
        let json = serde_json::to_string_pretty(&fetched.raw_json)?;
        std::fs::write(&path, json)
            .with_context(|| format!("write {}", path.display()))?;
    }
    tracing::info!("proceedings-fetch: session={session} → {} meetings saved", batch.meetings.len());
    Ok(())
}

/// `lawpub proceedings-build-json` の実装。
/// `.cache/proceedings/{session}/*.json` を読み、正規化した JSON を
/// `public/proceedings/{meeting_id}.json` と `public/proceedings/index.json` に書く。
pub fn run_build_json(cache: &Path, public: &Path) -> Result<()> {
    let proc_cache = cache.join("proceedings");
    if !proc_cache.exists() {
        anyhow::bail!("no proceedings cache at {}; run proceedings-fetch first", proc_cache.display());
    }

    let out_dir = public.join("proceedings");
    std::fs::create_dir_all(&out_dir)?;

    let fetched_at = chrono::Utc::now().to_rfc3339();
    let mut index_entries: Vec<serde_json::Value> = Vec::new();
    let mut total = 0usize;

    for session_entry in std::fs::read_dir(&proc_cache)? {
        let session_dir = session_entry?.path();
        if !session_dir.is_dir() {
            continue;
        }
        for file_entry in std::fs::read_dir(&session_dir)? {
            let file_path = file_entry?.path();
            if file_path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let meeting_id = file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let raw_bytes = std::fs::read(&file_path)
                .with_context(|| format!("read {}", file_path.display()))?;
            let raw_json: serde_json::Value = serde_json::from_slice(&raw_bytes)?;

            let fetched = kokkai_client::FetchedMeeting {
                meeting_id: meeting_id.clone(),
                raw_json,
                source_url: format!("cache://{}", file_path.display()),
            };

            let meeting = match normalize_meeting(&fetched, &fetched_at) {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!("skip {meeting_id}: {e:#}");
                    continue;
                }
            };

            // 個別ファイル
            let dest = out_dir.join(format!("{meeting_id}.json"));
            std::fs::write(&dest, serde_json::to_string_pretty(&meeting)?)
                .with_context(|| format!("write {}", dest.display()))?;

            // index エントリ
            index_entries.push(serde_json::json!({
                "meeting_id": meeting.meeting_id,
                "session": meeting.session,
                "house": meeting.house,
                "committee": meeting.committee,
                "date": meeting.date,
                "issue": meeting.issue,
                "speech_count": meeting.speeches.len(),
            }));
            total += 1;
        }
    }

    // index.json（日付降順）
    index_entries.sort_by(|a, b| {
        let da = a["date"].as_str().unwrap_or("");
        let db = b["date"].as_str().unwrap_or("");
        db.cmp(da)
    });
    let index = serde_json::json!({
        "schema_version": 1,
        "count": index_entries.len(),
        "meetings": index_entries,
    });
    std::fs::write(out_dir.join("index.json"), serde_json::to_string_pretty(&index)?)?;

    tracing::info!("proceedings-build-json: {total} meetings written to {}", out_dir.display());
    Ok(())
}
