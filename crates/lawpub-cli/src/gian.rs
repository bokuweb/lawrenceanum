use anyhow::{Context, Result};
use gian_client::{GianProvider, HttpProvider, MockProvider};
use sha2::{Digest, Sha256};
use std::path::Path;

fn make_provider(provider: &str) -> Box<dyn GianProvider> {
    match provider {
        "mock" => Box::new(MockProvider),
        _ => Box::new(HttpProvider::new()),
    }
}

/// `lawpub gian-fetch` — 指定回次 (0=最新) の議案審議経過を取得し
/// `.cache/gian/{session}/{bill_id}.json` に保存する。
pub fn run_fetch(cache: &Path, provider: &str, session: u32) -> Result<()> {
    let p = make_provider(provider);
    let bills = p.list_bills(session)?;
    tracing::info!(
        "gian-fetch: {} bills listed (session={session})",
        bills.len()
    );

    let mut total = 0usize;
    for meta in &bills {
        let mut bill = match p.fetch_bill(meta) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("skip {}: {e:#}", meta.bill_id);
                continue;
            }
        };
        archive_documents(cache, &mut bill)?;
        let dir = cache.join("gian").join(bill.session.to_string());
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.json", bill.bill_id));
        std::fs::write(&path, serde_json::to_string_pretty(&bill)?)
            .with_context(|| format!("write {}", path.display()))?;
        archive_bill_version(cache, &bill)?;
        total += 1;
    }
    tracing::info!("gian-fetch: {total} bills saved");
    Ok(())
}

/// 原 HTML は SHA-256 をファイル名にして内容アドレス保存する。同じ版を毎日取得しても
/// 1 ファイルのまま、原文が改訂された場合だけ新しい版が残る。
fn archive_documents(cache: &Path, bill: &mut gian_client::Bill) -> Result<()> {
    for document in &mut bill.documents {
        let Some(raw_html) = document.raw_html.take() else {
            continue;
        };
        let relative = format!(
            "gian-assets/{}/{}/{}.html",
            bill.session, bill.bill_id, document.sha256
        );
        let path = cache.join(&relative);
        if !path.exists() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, raw_html.as_bytes())
                .with_context(|| format!("write {}", path.display()))?;
        }
        document.raw_path = Some(relative);
    }
    Ok(())
}

/// fetched_at だけの差を除いた議案状態をハッシュし、意味のある変更時だけ履歴を追加する。
fn archive_bill_version(cache: &Path, bill: &gian_client::Bill) -> Result<()> {
    let mut stable = serde_json::to_value(bill)?;
    if let Some(source) = stable.get_mut("source").and_then(|v| v.as_object_mut()) {
        source.remove("fetched_at");
    }
    if let Some(documents) = stable.get_mut("documents").and_then(|v| v.as_array_mut()) {
        for document in documents {
            if let Some(object) = document.as_object_mut() {
                object.remove("fetched_at");
            }
        }
    }
    let fingerprint = format!("{:x}", Sha256::digest(serde_json::to_vec(&stable)?));
    let dir = cache
        .join("gian-history")
        .join(bill.session.to_string())
        .join(&bill.bill_id);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{fingerprint}.json"));
    if !path.exists() {
        std::fs::write(&path, serde_json::to_string_pretty(bill)?)
            .with_context(|| format!("write {}", path.display()))?;
    }
    Ok(())
}

/// `lawpub gian-build-json` — `.cache/gian/{session}/*.json` →
/// `public/gian/{session}/{bill_id}.json` + 回次別/全体 index.json。
pub fn run_build_json(cache: &Path, public: &Path) -> Result<()> {
    let src = cache.join("gian");
    if !src.exists() {
        anyhow::bail!("no gian cache at {}; run gian-fetch first", src.display());
    }
    let out = public.join("gian");
    std::fs::create_dir_all(&out)?;

    let mut global: Vec<serde_json::Value> = Vec::new();
    for sess_entry in std::fs::read_dir(&src)? {
        let sess_path = sess_entry?.path();
        if !sess_path.is_dir() {
            continue;
        }
        let session = sess_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let out_sess = out.join(&session);
        std::fs::create_dir_all(&out_sess)?;

        let mut entries: Vec<serde_json::Value> = Vec::new();
        for f in std::fs::read_dir(&sess_path)? {
            let path = f?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let bill: serde_json::Value = serde_json::from_slice(&std::fs::read(&path)?)?;
            let bill_id = bill["bill_id"].as_str().unwrap_or("").to_string();
            if bill_id.is_empty() {
                continue;
            }
            std::fs::write(
                out_sess.join(format!("{bill_id}.json")),
                serde_json::to_string_pretty(&bill)?,
            )?;
            let entry = serde_json::json!({
                "bill_id": bill_id,
                "session": bill["session"],
                "bill_type": bill["bill_type"],
                "number": bill["number"],
                "title": bill["title"],
                "committee": bill["committee"],
                "result": bill["result"],
                "status": bill["status"],
                "promulgation_date": bill["promulgation_date"],
                "latest_date": bill["latest_date"],
                "latest_event": bill["latest_event"],
                "detail_url": bill["source"]["detail_url"],
            });
            entries.push(entry.clone());
            global.push(entry);
        }
        entries.sort_by(|a, b| {
            a["number"]
                .as_str()
                .unwrap_or("")
                .cmp(b["number"].as_str().unwrap_or(""))
        });
        let idx = serde_json::json!({
            "schema_version": 1,
            "session": session,
            "count": entries.len(),
            "bills": entries,
        });
        std::fs::write(
            out_sess.join("index.json"),
            serde_json::to_string_pretty(&idx)?,
        )?;
    }

    // 全体 index: 回次降順。
    global.sort_by(|a, b| {
        let sa = a["session"].as_u64().unwrap_or(0);
        let sb = b["session"].as_u64().unwrap_or(0);
        sb.cmp(&sa)
    });
    let idx = serde_json::json!({
        "schema_version": 1,
        "count": global.len(),
        "bills": global,
    });
    std::fs::write(out.join("index.json"), serde_json::to_string_pretty(&idx)?)?;
    tracing::info!("gian-build-json: {} bills written", global.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_archives_document_and_deduplicates_bill_history() {
        let root = std::env::temp_dir().join(format!(
            "lawpub-gian-documents-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        run_fetch(&root, "mock", 221).unwrap();
        run_fetch(&root, "mock", 221).unwrap();

        let bill_path = root.join("gian/221/1DE153E.json");
        let bill: gian_client::Bill =
            serde_json::from_slice(&std::fs::read(bill_path).unwrap()).unwrap();
        assert_eq!(bill.documents.len(), 1);
        let raw_path = bill.documents[0].raw_path.as_deref().unwrap();
        assert!(root.join(raw_path).exists());
        assert_eq!(
            std::fs::read_dir(root.join("gian-history/221/1DE153E"))
                .unwrap()
                .count(),
            1
        );

        std::fs::remove_dir_all(root).unwrap();
    }
}
