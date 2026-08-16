use anyhow::{Context, Result};
use pubcomment_client::{
    Attachment, CaseDetail, CaseMeta, HttpProvider, MockProvider, PubcommentProvider,
};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;

fn make_provider(provider: &str) -> Box<dyn PubcommentProvider> {
    match provider {
        "mock" => Box::new(MockProvider),
        _ => Box::new(HttpProvider::new()),
    }
}

/// status 文字列 → 取得する Mode 群。open=意見募集中(0), closed=結果公示(1)。
fn modes_for(status: &str) -> Vec<u8> {
    match status {
        "open" => vec![0],
        "both" => vec![0, 1],
        _ => vec![1], // closed (既定)
    }
}

/// e-Gov がページ番号を無視して同じ一覧を返すことがあるため、既出案件を除外する。
/// mode ごとに別の集合を使い、募集中から結果公示へ移った案件は結果側で更新できるようにする。
fn retain_unseen_cases(cases: Vec<CaseMeta>, seen: &mut HashSet<String>) -> Vec<CaseMeta> {
    cases
        .into_iter()
        .filter(|case| seen.insert(case.case_id.clone()))
        .collect()
}

/// `lawpub pubcomment-fetch` の実装。
/// `status`(open/closed/both) の各 Mode を全ページ取得し
/// `.cache/pubcomment/{case_id}.json` に保存する。
pub fn run_fetch(
    cache: &Path,
    provider: &str,
    max_pages: u32,
    status: &str,
    fetch_attachments: bool,
) -> Result<()> {
    let p = make_provider(provider);
    let dir = cache.join("pubcomment");
    std::fs::create_dir_all(&dir)?;

    let mut total = 0usize;
    for mode in modes_for(status) {
        let mut seen_case_ids = HashSet::new();
        for page in 1..=max_pages {
            let cases = p.fetch_case_list(mode, page)?;
            if cases.is_empty() {
                break;
            }
            let cases = retain_unseen_cases(cases, &mut seen_case_ids);
            if cases.is_empty() {
                tracing::info!(
                    "pubcomment-fetch: mode={mode} page={page} returned no new case IDs; pagination exhausted"
                );
                break;
            }
            for meta in &cases {
                let path = dir.join(format!("{}.json", meta.case_id));
                // 募集中(mode=0)は結果未公開＆詳細ページが別系統で空のため、
                // 一覧メタから組む（締切・所管・案件名は一覧に揃っている）。
                let mut detail = if mode == 0 {
                    pubcomment_client::CaseDetail::from_meta(meta, &chrono::Utc::now().to_rfc3339())
                } else {
                    match p.fetch_case_detail(&meta.case_id, mode) {
                        Ok(d) => d,
                        Err(e) => {
                            // HTML 詳細も WAF で拒否される場合がある。既存の充実済み cache を
                            // 優先し、初回でも RSS/一覧メタだけは保存して案件を欠落させない。
                            tracing::warn!(
                                "pubcomment {} detail unavailable; preserving metadata: {e:#}",
                                meta.case_id
                            );
                            read_cached_case(&path).unwrap_or_else(|| {
                                pubcomment_client::CaseDetail::from_meta(
                                    meta,
                                    &chrono::Utc::now().to_rfc3339(),
                                )
                            })
                        }
                    }
                };
                // 一覧側のメタで詳細の欠損を補完する。
                if detail.ministry.is_none() {
                    detail.ministry = meta.ministry.clone();
                }
                if detail.result_published.is_none() {
                    detail.result_published = meta.result_published.clone();
                }
                if detail.reception_end.is_none() {
                    detail.reception_end = meta.reception_end.clone();
                }
                if detail.category.is_none() {
                    detail.category = meta.category.clone();
                }
                if detail.responsible_office.is_none() {
                    detail.responsible_office = meta.responsible_office.clone();
                }
                if detail.opinion_count.is_none() {
                    detail.opinion_count = meta.opinion_count;
                }
                if detail.title.is_empty() {
                    detail.title = meta.title.clone();
                }
                if detail.status.is_empty() {
                    detail.status = meta.status.clone();
                }
                if fetch_attachments && !detail.attachments.is_empty() {
                    let previous = read_cached_case(&path);
                    hydrate_attachments(p.as_ref(), cache, &mut detail, previous.as_ref());
                }
                std::fs::write(&path, serde_json::to_string_pretty(&detail)?)
                    .with_context(|| format!("write {}", path.display()))?;
                total += 1;
            }
            tracing::info!("pubcomment-fetch: mode={mode} page={page} ({total} total)");
        }
        if mode == 0 && max_pages > 0 {
            let removed = prune_stale_open_cases(&dir, &seen_case_ids)?;
            if removed > 0 {
                tracing::info!(
                    "pubcomment-fetch: removed {removed} stale open cases not present in current RSS"
                );
            }
        }
    }
    tracing::info!(
        "pubcomment-fetch: {total} cases saved (status={status}, attachments={fetch_attachments})"
    );
    Ok(())
}

fn read_cached_case(path: &Path) -> Option<CaseDetail> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// 募集中案件は履歴として残すと、締切後も UI 上で open のままになる。
/// 現在の公式 RSS にない open キャッシュだけを除去し、closed の結果公示は保持する。
fn prune_stale_open_cases(dir: &Path, active_ids: &HashSet<String>) -> Result<usize> {
    let mut removed = 0;
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let Some(detail) = read_cached_case(&path) else {
            continue;
        };
        if detail.status == "open" && !active_ids.contains(&detail.case_id) {
            std::fs::remove_file(&path)
                .with_context(|| format!("remove stale open case {}", path.display()))?;
            removed += 1;
        }
    }
    Ok(removed)
}

/// URL から安定した短いキャッシュキーを作る。e-Gov の seqNo URL は実質不変だが、
/// URL 文字列をファイル名に直接使わず、case_id 配下で衝突しないようにする。
fn attachment_cache_key(url: &str) -> String {
    let digest = Sha256::digest(url.as_bytes());
    hex::encode(digest)[..24].to_string()
}

fn hydrate_attachments(
    provider: &dyn PubcommentProvider,
    cache: &Path,
    detail: &mut CaseDetail,
    previous: Option<&CaseDetail>,
) {
    let previous_by_url: HashMap<&str, &Attachment> = previous
        .map(|p| p.attachments.iter().map(|a| (a.url.as_str(), a)).collect())
        .unwrap_or_default();
    let asset_dir = cache.join("pubcomment-assets").join(&detail.case_id);
    if let Err(e) = std::fs::create_dir_all(&asset_dir) {
        tracing::warn!(
            "pubcomment {}: create asset dir failed: {e:#}",
            detail.case_id
        );
        return;
    }

    for attachment in &mut detail.attachments {
        let raw_path = asset_dir.join(attachment_cache_key(&attachment.url));

        // Reader 由来の本文は原本ファイルを保存しないため raw_path が存在しない。
        // 前回の抽出本文が durable cache にあれば、日次実行で同じ PDF を再取得しない。
        if let Some(old) = previous_by_url.get(attachment.url.as_str()) {
            if old.extraction_method.as_deref() == Some("jina-reader")
                && old
                    .extracted_text
                    .as_ref()
                    .is_some_and(|text| !text.is_empty())
            {
                let name = attachment.name.clone();
                let url = attachment.url.clone();
                *attachment = (*old).clone();
                attachment.name = name;
                attachment.url = url;
                continue;
            }
        }

        // R2/GH cache から原本と抽出済みメタが両方戻っていれば再取得しない。
        if raw_path.exists() {
            if let Some(old) = previous_by_url.get(attachment.url.as_str()) {
                if old.sha256.is_some() {
                    let name = attachment.name.clone();
                    let url = attachment.url.clone();
                    *attachment = (*old).clone();
                    attachment.name = name;
                    attachment.url = url;
                    continue;
                }
            }
        }

        let fetched = match provider.fetch_attachment(&attachment.url) {
            Ok(v) => v,
            Err(e) => {
                attachment.extraction_error = Some(format!("fetch failed: {e:#}"));
                tracing::warn!(
                    "pubcomment {} attachment {}: {e:#}",
                    detail.case_id,
                    attachment.url
                );
                continue;
            }
        };
        // Reader fallback は e-Gov 原本ではなく、公開 PDF から抽出された派生テキスト。
        // 原本 SHA/bytes や raw archive と混同せず、抽出本文としてだけ保存する。
        if let Some(method) = fetched.extraction_method.as_deref() {
            let text = String::from_utf8_lossy(&fetched.bytes).trim().to_string();
            if text.is_empty() {
                attachment.extraction_error = Some(format!("{method} returned empty text"));
                continue;
            }
            attachment.media_type = fetched.media_type;
            attachment.filename = fetched.filename;
            attachment.sha256 = None;
            attachment.bytes = None;
            attachment.extracted_text = Some(text);
            attachment.extraction_method = Some(method.to_string());
            attachment.extraction_error = None;
            attachment.fetched_at = Some(fetched.fetched_at);
            continue;
        }
        let sha256 = hex::encode(Sha256::digest(&fetched.bytes));
        if let Err(e) = std::fs::write(&raw_path, &fetched.bytes) {
            attachment.extraction_error = Some(format!("cache write failed: {e:#}"));
            continue;
        }

        attachment.media_type = fetched.media_type;
        attachment.filename = fetched.filename;
        attachment.sha256 = Some(sha256);
        attachment.bytes = Some(fetched.bytes.len() as u64);
        attachment.fetched_at = Some(fetched.fetched_at);

        match extract_attachment_text(&raw_path, attachment.media_type.as_deref(), &fetched.bytes) {
            Ok((text, method)) => {
                attachment.extracted_text = Some(text);
                attachment.extraction_method = Some(method);
                attachment.extraction_error = None;
            }
            Err(e) => {
                attachment.extraction_error = Some(e.to_string());
            }
        }
    }
}

fn extract_attachment_text(
    path: &Path,
    media_type: Option<&str>,
    bytes: &[u8],
) -> Result<(String, String)> {
    let is_pdf = media_type == Some("application/pdf") || bytes.starts_with(b"%PDF-");
    if is_pdf {
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

    if media_type.is_some_and(|m| {
        m.starts_with("text/") || m == "application/json" || m == "application/xml"
    }) {
        let text = String::from_utf8_lossy(bytes).trim().to_string();
        if text.is_empty() {
            anyhow::bail!("text attachment is empty");
        }
        return Ok((text, "utf8-lossy".to_string()));
    }

    anyhow::bail!(
        "text extraction unsupported for {} (raw source archived)",
        media_type.unwrap_or("unknown media type")
    )
}

/// `lawpub pubcomment-build-json` の実装。
/// `.cache/pubcomment/*.json` → `public/pubcomment/{case_id}.json` + `index.json`
pub fn run_build_json(cache: &Path, public: &Path) -> Result<()> {
    let src_dir = cache.join("pubcomment");
    if !src_dir.exists() {
        anyhow::bail!(
            "no pubcomment cache at {}; run pubcomment-fetch first",
            src_dir.display()
        );
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
            "reception_start": detail["reception_start"],
            "reception_end": detail["reception_end"],
            "status": detail["status"],
            "related_law_name": detail["related_law_name"],
            "category": detail["category"],
            "responsible_office": detail["responsible_office"],
            "opinion_count": detail["opinion_count"],
        }));
    }

    // 募集中(open)を先頭に（締切が近い順）、その後 結果公示を公示日降順。
    index_entries.sort_by(|a, b| {
        let oa = a["status"].as_str() == Some("open");
        let ob = b["status"].as_str() == Some("open");
        match (oa, ob) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            (true, true) => {
                // 締切が近い順（昇順）。
                let ea = a["reception_end"].as_str().unwrap_or("");
                let eb = b["reception_end"].as_str().unwrap_or("");
                ea.cmp(eb)
            }
            (false, false) => {
                let da = a["result_published"].as_str().unwrap_or("");
                let db = b["result_published"].as_str().unwrap_or("");
                db.cmp(da)
            }
        }
    });
    let index = serde_json::json!({
        "schema_version": 1,
        "count": index_entries.len(),
        "cases": index_entries,
    });
    std::fs::write(
        out_dir.join("index.json"),
        serde_json::to_string_pretty(&index)?,
    )?;
    tracing::info!(
        "pubcomment-build-json: {} cases written",
        index_entries.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case_meta(case_id: &str) -> CaseMeta {
        CaseMeta {
            case_id: case_id.to_string(),
            title: format!("case {case_id}"),
            ministry: None,
            reception_start: None,
            reception_end: None,
            result_published: None,
            category: None,
            responsible_office: None,
            opinion_count: None,
            status: "closed".to_string(),
            detail_url: format!("https://example.com/{case_id}"),
        }
    }

    #[test]
    fn repeated_list_page_is_detected_as_exhausted() {
        let mut seen = HashSet::new();
        let first = retain_unseen_cases(vec![case_meta("a"), case_meta("b")], &mut seen);
        let repeated = retain_unseen_cases(vec![case_meta("a"), case_meta("b")], &mut seen);
        let partly_new = retain_unseen_cases(vec![case_meta("b"), case_meta("c")], &mut seen);

        assert_eq!(first.len(), 2);
        assert!(repeated.is_empty());
        assert_eq!(partly_new[0].case_id, "c");
    }

    #[test]
    fn stale_open_cases_are_pruned_but_closed_cases_are_retained() {
        let root = std::env::temp_dir().join(format!(
            "lawpub-pubcomment-prune-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&root).unwrap();
        for (case_id, status) in [("active", "open"), ("stale", "open"), ("result", "closed")] {
            let mut meta = case_meta(case_id);
            meta.status = status.to_string();
            let detail = CaseDetail::from_meta(&meta, "2026-08-16T00:00:00Z");
            std::fs::write(
                root.join(format!("{case_id}.json")),
                serde_json::to_vec(&detail).unwrap(),
            )
            .unwrap();
        }

        let active_ids = HashSet::from(["active".to_string()]);
        assert_eq!(prune_stale_open_cases(&root, &active_ids).unwrap(), 1);
        assert!(root.join("active.json").exists());
        assert!(!root.join("stale.json").exists());
        assert!(root.join("result.json").exists());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn fetch_archives_and_extracts_mock_attachment() {
        let root = std::env::temp_dir().join(format!(
            "lawpub-pubcomment-attachments-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        run_fetch(&root, "mock", 1, "closed", true).unwrap();

        let case_path = root.join("pubcomment/300110052.json");
        let detail: CaseDetail =
            serde_json::from_slice(&std::fs::read(case_path).unwrap()).unwrap();
        let attachment = &detail.attachments[0];
        assert_eq!(attachment.media_type.as_deref(), Some("text/plain"));
        assert_eq!(attachment.extraction_method.as_deref(), Some("utf8-lossy"));
        assert!(attachment
            .extracted_text
            .as_deref()
            .unwrap()
            .contains("府省の考え方"));
        assert!(attachment.sha256.is_some());
        assert_eq!(
            std::fs::read_dir(root.join("pubcomment-assets/300110052"))
                .unwrap()
                .count(),
            1
        );

        let public = root.join("public");
        run_build_json(&root, &public).unwrap();
        let index: serde_json::Value =
            serde_json::from_slice(&std::fs::read(public.join("pubcomment/index.json")).unwrap())
                .unwrap();
        assert_eq!(index["cases"][0]["category"], "民事");
        assert_eq!(index["cases"][0]["responsible_office"], "法務省民事局");
        assert_eq!(index["cases"][0]["opinion_count"], 1);
        assert_eq!(index["cases"][0]["reception_start"], "2023-06-01");

        std::fs::remove_dir_all(root).unwrap();
    }
}
