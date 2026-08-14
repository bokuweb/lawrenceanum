use anyhow::{Context, Result};
use scraper::{Html, Selector};
use sha2::{Digest, Sha256};
use shingikai_client::{
    CaoAdapter, MhlwAdapter, MinistryAdapter, MinutesAttachment, MinutesDocument, MlitAdapter,
    MockAdapter, MojAdapter,
};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

fn make_adapter(ministry: &str, provider: &str) -> Result<Box<dyn MinistryAdapter>> {
    if provider == "mock" {
        return Ok(Box::new(MockAdapter));
    }
    match ministry {
        "moj" => Ok(Box::new(MojAdapter::new())),
        "cao" => Ok(Box::new(CaoAdapter::new())),
        "mlit" => Ok(Box::new(MlitAdapter::new())),
        "mhlw" => Ok(Box::new(MhlwAdapter::new())),
        _ => anyhow::bail!("unsupported shingikai ministry: {ministry}"),
    }
}

pub fn run_fetch(ministry: &str, cache: &Path, provider: &str, max_meetings: usize) -> Result<()> {
    let adapter = make_adapter(ministry, provider)?;
    let dir = cache.join("shingikai").join(ministry);
    std::fs::create_dir_all(&dir)?;

    let committees = adapter.list_committees()?;
    tracing::info!(
        "shingikai-fetch: {} → {} active councils",
        ministry,
        committees.len()
    );

    let mut total = 0usize;
    let mut attachment_total = 0usize;
    for committee in &committees {
        let mut metas = match adapter.list_minutes(committee) {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!("skip {}: {error:#}", committee.title);
                continue;
            }
        };
        if max_meetings > 0 {
            metas.truncate(max_meetings);
        }
        for meta in &metas {
            let path = dir.join(format!("{}.json", meta.minutes_id));
            let previous: Option<MinutesDocument> = std::fs::read(&path)
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok());
            let mut document = match adapter.fetch_minutes(meta) {
                Ok(value) => value,
                Err(error) => {
                    tracing::warn!("skip {}: {error:#}", meta.minutes_id);
                    continue;
                }
            };
            archive_html(cache, &mut document)?;
            attachment_total +=
                enrich_attachments(cache, adapter.as_ref(), &mut document, previous.as_ref())?;
            document.minutes_text = document
                .attachments
                .iter()
                .find(|attachment| attachment.kind == "minutes_text")
                .and_then(|attachment| attachment.extracted_text.clone())
                .or_else(|| {
                    document
                        .attachments
                        .iter()
                        .find(|attachment| attachment.kind == "minutes_pdf")
                        .and_then(|attachment| attachment.extracted_text.clone())
                });
            std::fs::write(&path, serde_json::to_string_pretty(&document)?)
                .with_context(|| format!("write {}", path.display()))?;
            archive_version(cache, &document)?;
            total += 1;
        }
    }
    tracing::info!(
        "shingikai-fetch: {total} meetings saved / {attachment_total} new attachments fetched"
    );
    Ok(())
}

fn archive_html(cache: &Path, document: &mut MinutesDocument) -> Result<()> {
    let Some(raw_html) = document.raw_html.take() else {
        return Ok(());
    };
    let sha256 = format!("{:x}", Sha256::digest(raw_html.as_bytes()));
    let relative = format!(
        "shingikai-assets/{}/{}/{}.html",
        document.ministry, document.minutes_id, sha256
    );
    let path = cache.join(&relative);
    if !path.exists() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, raw_html.as_bytes())
            .with_context(|| format!("write {}", path.display()))?;
    }
    document.source.raw_html_sha256 = Some(sha256);
    document.source.raw_html_path = Some(relative);
    Ok(())
}

fn enrich_attachments(
    cache: &Path,
    adapter: &dyn MinistryAdapter,
    document: &mut MinutesDocument,
    previous: Option<&MinutesDocument>,
) -> Result<usize> {
    let previous_by_url: HashMap<&str, &MinutesAttachment> = previous
        .map(|old| {
            old.attachments
                .iter()
                .map(|attachment| (attachment.source_url.as_str(), attachment))
                .collect()
        })
        .unwrap_or_default();
    let mut fetched_count = 0usize;

    for attachment in &mut document.attachments {
        if let Some(old) = previous_by_url.get(attachment.source_url.as_str()) {
            let raw_exists = old
                .raw_path
                .as_deref()
                .is_some_and(|path| cache.join(path).exists());
            if raw_exists {
                copy_enrichment(attachment, old);
                continue;
            }
        }

        let fetched = match adapter.fetch_attachment(attachment) {
            Ok(value) => value,
            Err(error) => {
                attachment.extraction_error = Some(format!("fetch failed: {error:#}"));
                continue;
            }
        };
        let sha256 = format!("{:x}", Sha256::digest(&fetched.bytes));
        let extension = source_extension(&attachment.source_url);
        let relative = format!(
            "shingikai-assets/{}/{}/{}/{}.{}",
            document.ministry, document.minutes_id, attachment.attachment_id, sha256, extension
        );
        let path = cache.join(&relative);
        if !path.exists() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, &fetched.bytes)
                .with_context(|| format!("write {}", path.display()))?;
        }
        let extraction = extract_attachment_text(
            &path,
            fetched.media_type.as_deref(),
            &fetched.bytes,
            &extension,
        );
        attachment.media_type = fetched.media_type;
        attachment.bytes = Some(fetched.bytes.len() as u64);
        attachment.sha256 = Some(sha256);
        attachment.fetched_at = Some(fetched.fetched_at);
        attachment.raw_path = Some(relative);
        match extraction {
            Ok((text, method)) => {
                attachment.extracted_text = Some(text);
                attachment.extraction_method = Some(method);
                attachment.extraction_error = None;
            }
            Err(error) => {
                attachment.extraction_error = Some(error.to_string());
            }
        }
        fetched_count += 1;
    }
    Ok(fetched_count)
}

fn copy_enrichment(target: &mut MinutesAttachment, source: &MinutesAttachment) {
    target.media_type = source.media_type.clone();
    target.bytes = source.bytes;
    target.sha256 = source.sha256.clone();
    target.fetched_at = source.fetched_at.clone();
    target.raw_path = source.raw_path.clone();
    target.extracted_text = source.extracted_text.clone();
    target.extraction_method = source.extraction_method.clone();
    target.extraction_error = source.extraction_error.clone();
}

fn source_extension(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|url| {
            url.path_segments()?
                .next_back()?
                .rsplit_once('.')
                .map(|(_, extension)| extension.to_ascii_lowercase())
        })
        .filter(|extension| {
            !extension.is_empty()
                && extension
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric())
        })
        .unwrap_or_else(|| "bin".to_string())
}

fn extract_attachment_text(
    path: &Path,
    media_type: Option<&str>,
    bytes: &[u8],
    extension: &str,
) -> Result<(String, String)> {
    if media_type.is_some_and(|value| value.starts_with("text/html"))
        || matches!(extension, "html" | "htm")
    {
        let html = String::from_utf8_lossy(bytes);
        let document = Html::parse_document(&html);
        let content_selector = Selector::parse("#content").unwrap();
        let body_selector = Selector::parse("body").unwrap();
        let content = document
            .select(&content_selector)
            .next()
            .or_else(|| document.select(&body_selector).next())
            .context("HTML attachment has no content")?;
        let text = content
            .text()
            .flat_map(str::split_whitespace)
            .collect::<Vec<_>>()
            .join(" ");
        if text.is_empty() {
            anyhow::bail!("HTML attachment has no visible text");
        }
        return Ok((text, "html-visible-text".to_string()));
    }

    if media_type.is_some_and(|value| value.starts_with("text/"))
        || matches!(extension, "txt" | "csv")
    {
        let text = String::from_utf8_lossy(bytes)
            .trim_start_matches('\u{feff}')
            .trim()
            .to_string();
        if text.is_empty() {
            anyhow::bail!("text attachment is empty");
        }
        return Ok((text, "utf8-lossy".to_string()));
    }

    if media_type.is_some_and(|value| value.starts_with("application/pdf")) || extension == "pdf" {
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

    anyhow::bail!("text extraction unsupported; raw attachment archived")
}

fn archive_version(cache: &Path, document: &MinutesDocument) -> Result<()> {
    let mut stable = serde_json::to_value(document)?;
    if let Some(source) = stable
        .get_mut("source")
        .and_then(|value| value.as_object_mut())
    {
        source.remove("fetched_at");
    }
    if let Some(attachments) = stable
        .get_mut("attachments")
        .and_then(|value| value.as_array_mut())
    {
        for attachment in attachments {
            if let Some(object) = attachment.as_object_mut() {
                object.remove("fetched_at");
            }
        }
    }
    let fingerprint = format!("{:x}", Sha256::digest(serde_json::to_vec(&stable)?));
    let dir = cache
        .join("shingikai-history")
        .join(&document.ministry)
        .join(&document.minutes_id);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{fingerprint}.json"));
    if !path.exists() {
        std::fs::write(&path, serde_json::to_string_pretty(document)?)
            .with_context(|| format!("write {}", path.display()))?;
    }
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
        if !ministry_dir.is_dir() {
            continue;
        }
        let ministry_id = ministry_dir
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_string();
        let ministry_out = out.join(&ministry_id);
        std::fs::create_dir_all(&ministry_out)?;
        let mut ministry_entries = Vec::new();

        for entry in std::fs::read_dir(&ministry_dir)? {
            let path = entry?.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let mut document: serde_json::Value = serde_json::from_slice(&std::fs::read(&path)?)?;
            let id = document["minutes_id"].as_str().unwrap_or("").to_string();
            if id.is_empty() {
                continue;
            }
            let status = effective_meeting_status(document["date"].as_str());
            document["status"] = serde_json::Value::String(status.to_string());
            std::fs::write(
                ministry_out.join(format!("{id}.json")),
                serde_json::to_string_pretty(&document)?,
            )?;
            let summary = serde_json::json!({
                "minutes_id": id,
                "ministry": document["ministry"],
                "committee_id": document["committee_id"],
                "committee": document["committee"],
                "date": document["date"],
                "status": status,
                "title": document["title"],
                "attachment_count": document["attachments"].as_array().map(Vec::len).unwrap_or(0),
                "has_minutes": document["minutes_text"].as_str().is_some_and(|value| !value.is_empty()),
                "detail_url": document["source"]["detail_url"],
            });
            ministry_entries.push(summary.clone());
            index_entries.push(summary);
        }
        ministry_entries.sort_by(|a, b| b["date"].as_str().cmp(&a["date"].as_str()));
        std::fs::write(
            ministry_out.join("index.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "schema_version": 3,
                "ministry": ministry_id,
                "count": ministry_entries.len(),
                "minutes": ministry_entries,
            }))?,
        )?;
    }

    index_entries.sort_by(|a, b| b["date"].as_str().cmp(&a["date"].as_str()));
    std::fs::write(
        out.join("index.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": 3,
            "count": index_entries.len(),
            "minutes": index_entries,
        }))?,
    )?;
    tracing::info!(
        "shingikai-build-json: {} meetings written",
        index_entries.len()
    );
    Ok(())
}

fn effective_meeting_status(date: Option<&str>) -> &'static str {
    let is_future = date
        .and_then(|value| chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
        .is_some_and(|value| value > chrono::Utc::now().date_naive());
    if is_future {
        "scheduled"
    } else {
        "held"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("lawpub-shingikai-{name}-{}", std::process::id()))
    }

    #[test]
    fn fetch_archives_assets_and_deduplicates_history() {
        let root = temp_root("mock");
        std::fs::create_dir_all(&root).unwrap();
        run_fetch("moj", &root, "mock", 20).unwrap();
        run_fetch("moj", &root, "mock", 20).unwrap();

        let path = root.join("shingikai/moj/mock_moj_0001.json");
        let document: MinutesDocument =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert!(document
            .minutes_text
            .as_deref()
            .unwrap()
            .contains("株主総会"));
        assert!(root.join(document.source.raw_html_path.unwrap()).exists());
        assert!(root
            .join(document.attachments[0].raw_path.as_deref().unwrap())
            .exists());
        assert_eq!(
            std::fs::read_dir(root.join("shingikai-history/mock/mock_moj_0001"))
                .unwrap()
                .count(),
            1
        );

        let public = root.join("public");
        run_build_json(&root, &public).unwrap();
        let index: serde_json::Value =
            serde_json::from_slice(&std::fs::read(public.join("shingikai/index.json")).unwrap())
                .unwrap();
        assert_eq!(index["count"], 1);
        assert_eq!(index["minutes"][0]["has_minutes"], true);
        assert_eq!(index["minutes"][0]["status"], "held");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn extracts_visible_text_from_html_minutes() {
        let bytes = r#"<!doctype html><html><body><nav>menu</nav><main id="content">
            <h1>第35回議事録</h1><p>制度改正について審議した。</p>
            </main></body></html>"#
            .as_bytes();
        let path = temp_root("minutes.html");
        let (text, method) =
            extract_attachment_text(&path, Some("text/html; charset=UTF-8"), bytes, "html")
                .unwrap();
        assert_eq!(method, "html-visible-text");
        assert!(text.contains("制度改正について審議した。"));
        assert!(!text.contains("menu"));
    }

    #[test]
    fn meeting_status_rolls_forward_at_build_time() {
        assert_eq!(effective_meeting_status(Some("2099-09-15")), "scheduled");
        assert_eq!(effective_meeting_status(Some("2000-01-01")), "held");
    }

    #[test]
    #[ignore]
    fn real_moj_minutes_txt_extracts() {
        let adapter = MojAdapter::new();
        let attachment = MinutesAttachment {
            attachment_id: "001467699".to_string(),
            kind: "minutes_text".to_string(),
            label: "ＴＸＴ版".to_string(),
            source_url: "https://www.moj.go.jp/content/001467699.txt".to_string(),
            media_type: None,
            bytes: None,
            sha256: None,
            fetched_at: None,
            raw_path: None,
            extracted_text: None,
            extraction_method: None,
            extraction_error: None,
        };
        let fetched = adapter.fetch_attachment(&attachment).unwrap();
        let path = temp_root("real.txt");
        std::fs::write(&path, &fetched.bytes).unwrap();
        let (text, method) =
            extract_attachment_text(&path, fetched.media_type.as_deref(), &fetched.bytes, "txt")
                .unwrap();
        std::fs::remove_file(path).unwrap();
        println!("{method}: {} chars", text.chars().count());
        assert!(text.contains("法制審議会"));
        assert!(text.chars().count() > 10_000);
    }

    #[test]
    #[ignore]
    fn real_mhlw_html_minutes_extracts() {
        let adapter = MhlwAdapter::new();
        let attachment = MinutesAttachment {
            attachment_id: "newpage_72796".to_string(),
            kind: "minutes_text".to_string(),
            label: "議事録".to_string(),
            source_url: "https://www.mhlw.go.jp/stf/newpage_72796.html".to_string(),
            media_type: None,
            bytes: None,
            sha256: None,
            fetched_at: None,
            raw_path: None,
            extracted_text: None,
            extraction_method: None,
            extraction_error: None,
        };
        let fetched = adapter.fetch_attachment(&attachment).unwrap();
        let path = temp_root("real-mhlw.html");
        let (text, method) =
            extract_attachment_text(&path, fetched.media_type.as_deref(), &fetched.bytes, "html")
                .unwrap();
        println!("{method}: {} chars", text.chars().count());
        assert_eq!(method, "html-visible-text");
        assert!(text.contains("勤労者生活分科会"));
        assert!(text.contains("議題"));
    }
}
