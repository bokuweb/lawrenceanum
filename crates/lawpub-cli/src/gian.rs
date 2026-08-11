use anyhow::{Context, Result};
use gian_client::{GianProvider, HttpProvider, MockProvider, SupplementaryResolution};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

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
    let resolutions = fetch_resolutions(cache, p.as_ref(), session)?;
    tracing::info!("gian-fetch: {resolutions} supplementary resolutions saved");
    Ok(())
}

fn fetch_resolutions(cache: &Path, provider: &dyn GianProvider, session: u32) -> Result<usize> {
    let metas = provider.list_resolutions(session)?;
    let mut total = 0usize;
    for meta in metas {
        let fetched = match provider.fetch_resolution(&meta) {
            Ok(value) => value,
            Err(e) => {
                tracing::warn!("skip resolution {}: {e:#}", meta.resolution_id);
                continue;
            }
        };
        let sha256 = format!("{:x}", Sha256::digest(&fetched.bytes));
        let extension = if fetched.bytes.starts_with(b"%PDF-") {
            "pdf"
        } else {
            "bin"
        };
        let relative = format!(
            "gian-resolution-assets/{}/{}/{}.{}",
            meta.session, meta.resolution_id, sha256, extension
        );
        let asset_path = cache.join(&relative);
        if !asset_path.exists() {
            if let Some(parent) = asset_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&asset_path, &fetched.bytes)
                .with_context(|| format!("write {}", asset_path.display()))?;
        }

        let dir = cache
            .join("gian-resolutions")
            .join(meta.session.to_string());
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.json", meta.resolution_id));
        let previous: Option<SupplementaryResolution> = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok());
        let same_content = previous.as_ref().is_some_and(|old| old.sha256 == sha256);
        let (extracted_text, extraction_method, extraction_error, fetched_at) = if same_content {
            let old = previous.as_ref().expect("checked above");
            (
                old.extracted_text.clone(),
                old.extraction_method.clone(),
                old.extraction_error.clone(),
                old.fetched_at.clone(),
            )
        } else {
            match extract_resolution_text(
                &asset_path,
                fetched.media_type.as_deref(),
                &fetched.bytes,
            ) {
                Ok((text, method)) => (Some(text), Some(method), None, fetched.fetched_at.clone()),
                Err(e) => (None, None, Some(e.to_string()), fetched.fetched_at.clone()),
            }
        };
        let resolution = SupplementaryResolution {
            schema_version: 1,
            resolution_id: meta.resolution_id,
            session: meta.session,
            chamber: meta.chamber,
            committee: meta.committee,
            title: meta.title,
            subject: meta.subject,
            resolution_date: meta.resolution_date,
            source_url: meta.source_url,
            media_type: fetched.media_type,
            sha256,
            bytes: fetched.bytes.len() as u64,
            fetched_at,
            raw_path: relative,
            extracted_text,
            extraction_method,
            extraction_error,
        };
        std::fs::write(&path, serde_json::to_string_pretty(&resolution)?)
            .with_context(|| format!("write {}", path.display()))?;
        total += 1;
    }
    Ok(total)
}

fn extract_resolution_text(
    path: &Path,
    media_type: Option<&str>,
    bytes: &[u8],
) -> Result<(String, String)> {
    let is_pdf =
        media_type.is_some_and(|m| m.starts_with("application/pdf")) || bytes.starts_with(b"%PDF-");
    if is_pdf {
        let bbox = Command::new("pdftotext")
            .args(["-bbox-layout", "-enc", "UTF-8"])
            .arg(path)
            .arg("-")
            .output()
            .context("run pdftotext (install poppler-utils)")?;
        if bbox.status.success() {
            let xhtml = String::from_utf8_lossy(&bbox.stdout);
            let text = gian_client::reconstruct_vertical_glyph_text(&xhtml);
            if !text.is_empty() {
                return Ok((text, "pdftotext-bbox-vertical".to_string()));
            }
        }

        let output = Command::new("pdftotext")
            .args(["-layout", "-enc", "UTF-8"])
            .arg(path)
            .arg("-")
            .output()
            .context("run pdftotext (install poppler-utils)")?;
        if !output.status.success() {
            anyhow::bail!(
                "pdftotext failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if text.is_empty() {
            anyhow::bail!("pdftotext returned empty text (possibly scanned PDF)");
        }
        return Ok((text, "pdftotext-layout".to_string()));
    }

    if media_type.is_some_and(|m| m.starts_with("text/")) {
        let text = String::from_utf8_lossy(bytes).trim().to_string();
        if !text.is_empty() {
            return Ok((text, "utf8-lossy".to_string()));
        }
    }
    anyhow::bail!("resolution text extraction unsupported; raw source archived")
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
    let resolution_count = build_resolution_json(cache, public, &global)?;
    let idx = serde_json::json!({
        "schema_version": 1,
        "count": global.len(),
        "supplementary_resolution_count": resolution_count,
        "bills": global,
    });
    std::fs::write(out.join("index.json"), serde_json::to_string_pretty(&idx)?)?;
    tracing::info!(
        "gian-build-json: {} bills / {} supplementary resolutions written",
        global.len(),
        resolution_count
    );
    Ok(())
}

fn normalize_title(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '・' | '、' | '，' | ','))
        .collect()
}

/// 決議回次以前に提出された同名議案のうち、最新回次のものを結び付ける。
/// 継続審議された議案は決議回次と提出回次が異なるため、同一回次だけに限定しない。
fn related_bills_for_resolution(
    resolution_session: u64,
    subject: &str,
    bills: &[serde_json::Value],
) -> Vec<serde_json::Value> {
    let subject = normalize_title(subject);
    let candidates: Vec<(&serde_json::Value, String)> = bills
        .iter()
        .filter(|bill| {
            bill["session"]
                .as_u64()
                .is_some_and(|s| s <= resolution_session)
        })
        .filter_map(|bill| {
            let title = normalize_title(bill["title"].as_str().unwrap_or(""));
            (!title.is_empty() && subject.contains(&title)).then_some((bill, title))
        })
        .collect();
    let mut latest_session_by_title: HashMap<String, u64> = HashMap::new();
    for (bill, title) in &candidates {
        let session = bill["session"].as_u64().unwrap_or(0);
        latest_session_by_title
            .entry(title.clone())
            .and_modify(|latest| *latest = (*latest).max(session))
            .or_insert(session);
    }
    candidates
        .into_iter()
        .filter(|(bill, title)| {
            latest_session_by_title.get(title).copied() == bill["session"].as_u64()
        })
        .map(|(bill, _)| {
            serde_json::json!({
                "bill_id": bill["bill_id"],
                "session": bill["session"],
                "title": bill["title"],
            })
        })
        .collect()
}

fn build_resolution_json(
    cache: &Path,
    public: &Path,
    bills: &[serde_json::Value],
) -> Result<usize> {
    let src = cache.join("gian-resolutions");
    if !src.exists() {
        return Ok(0);
    }
    let out = public.join("gian").join("resolutions");
    let links = public.join("links").join("bill-to-resolutions");
    std::fs::create_dir_all(&out)?;
    std::fs::create_dir_all(&links)?;
    let mut global = Vec::new();
    let mut by_bill: HashMap<(u64, String), Vec<serde_json::Value>> = HashMap::new();

    for session_entry in std::fs::read_dir(&src)? {
        let session_path = session_entry?.path();
        if !session_path.is_dir() {
            continue;
        }
        let session = session_path
            .file_name()
            .and_then(|v| v.to_str())
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        let out_session = out.join(session.to_string());
        std::fs::create_dir_all(&out_session)?;
        let mut session_entries = Vec::new();

        for entry in std::fs::read_dir(&session_path)? {
            let path = entry?.path();
            if path.extension().and_then(|v| v.to_str()) != Some("json") {
                continue;
            }
            let mut resolution: serde_json::Value = serde_json::from_slice(&std::fs::read(&path)?)?;
            let resolution_id = resolution["resolution_id"]
                .as_str()
                .unwrap_or("")
                .to_string();
            if resolution_id.is_empty() {
                continue;
            }
            let related = related_bills_for_resolution(
                session,
                resolution["subject"].as_str().unwrap_or(""),
                bills,
            );
            resolution["related_bills"] = serde_json::Value::Array(related.clone());
            std::fs::write(
                out_session.join(format!("{resolution_id}.json")),
                serde_json::to_string_pretty(&resolution)?,
            )?;
            let summary = serde_json::json!({
                "resolution_id": resolution_id,
                "session": resolution["session"],
                "chamber": resolution["chamber"],
                "committee": resolution["committee"],
                "title": resolution["title"],
                "resolution_date": resolution["resolution_date"],
                "source_url": resolution["source_url"],
                "sha256": resolution["sha256"],
                "related_bills": related,
            });
            if let Some(items) = summary["related_bills"].as_array() {
                for bill in items {
                    let bill_id = bill["bill_id"].as_str().unwrap_or("").to_string();
                    if !bill_id.is_empty() {
                        by_bill
                            .entry((session, bill_id))
                            .or_default()
                            .push(summary.clone());
                    }
                }
            }
            session_entries.push(summary.clone());
            global.push(summary);
        }
        session_entries.sort_by(|a, b| {
            b["resolution_date"]
                .as_str()
                .cmp(&a["resolution_date"].as_str())
        });
        std::fs::write(
            out_session.join("index.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "schema_version": 1,
                "session": session,
                "count": session_entries.len(),
                "resolutions": session_entries,
            }))?,
        )?;
    }

    for ((session, bill_id), resolutions) in by_bill {
        let dir = links.join(session.to_string());
        std::fs::create_dir_all(&dir)?;
        std::fs::write(
            dir.join(format!("{bill_id}.json")),
            serde_json::to_string_pretty(&serde_json::json!({
                "schema_version": 1,
                "session": session,
                "bill_id": bill_id,
                "count": resolutions.len(),
                "resolutions": resolutions,
            }))?,
        )?;
    }
    global.sort_by(|a, b| {
        b["resolution_date"]
            .as_str()
            .cmp(&a["resolution_date"].as_str())
    });
    std::fs::write(
        out.join("index.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": 1,
            "count": global.len(),
            "resolutions": global,
        }))?,
    )?;
    Ok(global.len())
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

        let resolution_path = root.join("gian-resolutions/221/sangiin-221-f065_061601.json");
        let resolution: SupplementaryResolution =
            serde_json::from_slice(&std::fs::read(resolution_path).unwrap()).unwrap();
        assert!(resolution
            .extracted_text
            .as_deref()
            .unwrap()
            .contains("必要な措置"));
        assert!(root.join(&resolution.raw_path).exists());

        let public = root.join("public");
        run_build_json(&root, &public).unwrap();
        let link: serde_json::Value = serde_json::from_slice(
            &std::fs::read(public.join("links/bill-to-resolutions/221/1DE153E.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(link["count"], 1);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolution_links_to_latest_eligible_bill_session() {
        let bills = vec![
            serde_json::json!({"bill_id": "old", "session": 220, "title": "継続審議法案"}),
            serde_json::json!({"bill_id": "future", "session": 222, "title": "継続審議法案"}),
            serde_json::json!({"bill_id": "same", "session": 221, "title": "関連整備法案"}),
        ];
        let related = related_bills_for_resolution(
            221,
            "継続審議法案及び関連整備法案に対する附帯決議",
            &bills,
        );
        let ids: Vec<&str> = related
            .iter()
            .filter_map(|bill| bill["bill_id"].as_str())
            .collect();
        assert_eq!(ids, vec!["old", "same"]);
    }

    #[test]
    #[ignore]
    fn real_resolution_pdf_extracts_vertical_text() {
        let provider = HttpProvider::new();
        let meta = provider.list_resolutions(0).unwrap().remove(0);
        let fetched = provider.fetch_resolution(&meta).unwrap();
        let path = std::env::temp_dir().join("lawpub-resolution-real.pdf");
        std::fs::write(&path, &fetched.bytes).unwrap();
        let (text, method) =
            extract_resolution_text(&path, fetched.media_type.as_deref(), &fetched.bytes).unwrap();
        std::fs::remove_file(path).unwrap();
        println!("{method}: {} chars", text.chars().count());
        assert_eq!(method, "pdftotext-bbox-vertical");
        assert!(text.contains("政府は"));
        assert!(text.chars().count() > 100);
    }
}
