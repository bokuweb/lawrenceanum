use anyhow::Result;
use serde_json::json;
use std::path::Path;
use walkdir::WalkDir;

use crate::state;

fn count_files(dir: &Path) -> (u64, u64) {
    let mut files = 0u64;
    let mut bytes = 0u64;
    if dir.exists() {
        for e in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
            if e.file_type().is_file() {
                files += 1;
                bytes += e.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    (files, bytes)
}

pub fn run(public: &Path, cache: &Path) -> Result<()> {
    let (public_files, public_bytes) = count_files(public);
    let (cache_files, cache_bytes) = count_files(cache);
    let revisions_dir = cache.join("revisions");
    let mut law_count = 0u64;
    let mut rev_count = 0u64;
    if revisions_dir.exists() {
        for d in std::fs::read_dir(&revisions_dir)? {
            let d = d?;
            if d.file_type()?.is_dir() {
                law_count += 1;
                for f in std::fs::read_dir(d.path())? {
                    let f = f?;
                    if f.path().extension().and_then(|s| s.to_str()) == Some("xml") {
                        rev_count += 1;
                    }
                }
            }
        }
    }

    let latest = state::load(&Path::new("state/latest.json").to_path_buf())?;
    let last_run_path = std::path::PathBuf::from("state/last_run.json");
    let last_run: Option<serde_json::Value> = if last_run_path.exists() {
        serde_json::from_slice(&std::fs::read(&last_run_path)?).ok()
    } else {
        None
    };

    let report = json!({
        "public": {
            "path": public.display().to_string(),
            "exists": public.exists(),
            "files": public_files,
            "bytes": public_bytes,
            "manifest": public.join("manifest.json").exists(),
            "index": public.join("index.json").exists(),
            "sitemap": public.join("sitemap.xml").exists(),
        },
        "cache": {
            "path": cache.display().to_string(),
            "files": cache_files,
            "bytes": cache_bytes,
            "law_count_in_revisions": law_count,
            "revision_count": rev_count,
        },
        "state": {
            "latest_successful_update_date": latest.latest_successful_update_date,
            "last_run_at": latest.last_run_at,
            "last_run_status": latest.last_run_status,
            "last_run": last_run,
        },
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
