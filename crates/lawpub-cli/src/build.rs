use anyhow::{Context, Result};
use chrono::Utc;
use egov_client::{EgovProvider, FetchedLaw, LawRevisionList, MockProvider, RevisionMeta};
use law_normalizer::{parse_law_xml, sha256_hex, LawDocument};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::state;

const SCHEMA_VERSION: u32 = 1;

fn provider_by_name(name: &str) -> Result<Box<dyn EgovProvider>> {
    match name {
        "mock" => Ok(Box::new(MockProvider)),
        "http" => {
            // 既定は v1 API。v2 (`/api/2/`) は path 構造が異なり 404 になるため、
            // 既知の動作する v1 (`/api/1/lawlists/{cat}`, `/api/1/lawdata/{id}`,
            // `/api/1/updatelawlists/{yyyymmdd}`) を採用する。
            let base = std::env::var("LAWPUB_EGOV_BASE_URL")
                .unwrap_or_else(|_| "https://laws.e-gov.go.jp/api/1".to_string());
            Ok(Box::new(egov_client::HttpProvider::new(base)))
        }
        other => anyhow::bail!("unknown provider: {other}"),
    }
}

/// v2 メタ取得用の HttpProvider を作る。`LAWPUB_EGOV_V2_BASE_URL` が未設定なら
/// HttpProvider 内部で公開エンドポイントにフォールバックする。
fn http_provider_v2() -> egov_client::HttpProvider {
    // base_url は v1 ベース。HttpProvider 内で v2_base_url を別途解決する。
    let base = std::env::var("LAWPUB_EGOV_BASE_URL")
        .unwrap_or_else(|_| "https://laws.e-gov.go.jp/api/1".to_string());
    egov_client::HttpProvider::new(base)
}

/// `.cache/revisions_meta/{law_id}.json` に v2 改正履歴を保存する。
/// 全件 backfill / 単一 law の両モードを使い分け、resume するため既に
/// ファイルがあれば既定で skip する。
pub fn run_fetch_revisions(
    law_id: Option<&str>,
    all: bool,
    from_public: Option<&Path>,
    concurrency: usize,
    limit: Option<usize>,
    force: bool,
    cache: &Path,
) -> Result<()> {
    let meta_dir = cache.join("revisions_meta");
    std::fs::create_dir_all(&meta_dir).context("create revisions_meta dir")?;

    let mut targets: Vec<String> = if let Some(id) = law_id {
        vec![id.to_string()]
    } else if all {
        // 既に bulk で revisions/{id}/ が出来ているはずなので、それを対象に。
        let revisions_dir = cache.join("revisions");
        if !revisions_dir.exists() {
            anyhow::bail!(
                "{} not found — run `lawpub fetch-bulk` first (or use --from-public)",
                revisions_dir.display()
            );
        }
        std::fs::read_dir(&revisions_dir)?
            .flatten()
            .filter_map(|e| {
                e.file_type().ok().and_then(|t| {
                    if t.is_dir() {
                        e.file_name().to_str().map(String::from)
                    } else {
                        None
                    }
                })
            })
            .collect()
    } else if let Some(public_dir) = from_public {
        // public/laws/index.json の laws[].law_id を ID 源として使う。
        // git checkout だけある別端末でも .cache 無しで backfill 出来る。
        let idx_path = public_dir.join("laws").join("index.json");
        let bytes =
            std::fs::read(&idx_path).with_context(|| format!("read {}", idx_path.display()))?;
        let v: serde_json::Value = serde_json::from_slice(&bytes)
            .with_context(|| format!("parse {}", idx_path.display()))?;
        v["laws"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|e| e["law_id"].as_str().map(String::from))
            .collect()
    } else {
        anyhow::bail!("specify --law-id <ID> | --all | --from-public <DIR>");
    };
    if let Some(n) = limit {
        targets.truncate(n);
    }
    tracing::info!(
        "fetch-revisions: {} target(s), concurrency={}, force={}",
        targets.len(),
        concurrency,
        force
    );

    let provider = http_provider_v2();
    let counter = std::sync::atomic::AtomicUsize::new(0);
    let errors = std::sync::Mutex::new(Vec::<(String, String)>::new());
    let total = targets.len();
    let concurrency = concurrency.clamp(1, 16);

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(concurrency)
        .build()
        .context("rayon pool")?;
    pool.install(|| {
        use rayon::prelude::*;
        targets.par_iter().for_each(|id| {
            let out = meta_dir.join(format!("{}.json", id));
            if out.exists() && !force {
                let n = counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                if n % 500 == 0 {
                    tracing::info!("fetch-revisions: {}/{} (skipped existing)", n, total);
                }
                return;
            }
            match provider.fetch_law_revisions(id) {
                Ok(list) => {
                    let body = match serde_json::to_vec_pretty(&list) {
                        Ok(b) => b,
                        Err(e) => {
                            errors
                                .lock()
                                .unwrap()
                                .push((id.clone(), format!("serialize: {e}")));
                            return;
                        }
                    };
                    if let Err(e) = std::fs::write(&out, body) {
                        errors
                            .lock()
                            .unwrap()
                            .push((id.clone(), format!("write: {e}")));
                    }
                }
                Err(e) => {
                    errors.lock().unwrap().push((id.clone(), format!("{e:#}")));
                }
            }
            let n = counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            if n % 100 == 0 {
                tracing::info!("fetch-revisions: {}/{}", n, total);
            }
        });
    });

    let errs = errors.into_inner().unwrap();
    if !errs.is_empty() {
        tracing::warn!("fetch-revisions: {} errors", errs.len());
        for (id, msg) in errs.iter().take(10) {
            tracing::warn!("  {id}: {msg}");
        }
        if errs.len() > 10 {
            tracing::warn!("  ... and {} more", errs.len() - 10);
        }
    }
    tracing::info!("fetch-revisions done: written to {}", meta_dir.display());
    Ok(())
}

/// `.cache/revisions_meta/{law_id}.json` の各 revision について、e-Gov v2
/// `/law_data/{revision_id}` を叩いて本文 XML を取得し
/// `.cache/revisions/{law_id}/{revision_id}.xml` に保存する。
///
/// build-diffs が「実データ」を吐けるようにするための backfill 入口。
/// 既存ファイルがあれば skip (resume 友好)。
///
/// `--law-id` で単一、`--all` で全 meta、`--limit` で件数キャップ。
pub fn run_fetch_revision_bodies(
    law_id: Option<&str>,
    all: bool,
    concurrency: usize,
    limit_laws: Option<usize>,
    limit_revs_per_law: Option<usize>,
    force: bool,
    cache: &Path,
) -> Result<()> {
    let meta_dir = cache.join("revisions_meta");
    if !meta_dir.exists() {
        anyhow::bail!(
            "{} not found — run `lawpub fetch-revisions` first",
            meta_dir.display()
        );
    }
    let revisions_root = cache.join("revisions");
    std::fs::create_dir_all(&revisions_root)?;

    // 対象法令の選定
    let mut law_ids: Vec<String> = if let Some(id) = law_id {
        vec![id.to_string()]
    } else if all {
        std::fs::read_dir(&meta_dir)?
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                if p.extension().and_then(|s| s.to_str()) == Some("json") {
                    p.file_stem().and_then(|s| s.to_str()).map(String::from)
                } else {
                    None
                }
            })
            .collect()
    } else {
        anyhow::bail!("specify --law-id <ID> or --all");
    };
    law_ids.sort();
    if let Some(n) = limit_laws {
        law_ids.truncate(n);
    }

    // 全 (law_id, revision_id) ペアを集めて並列ダウンロード
    #[derive(Clone)]
    struct Job {
        law_id: String,
        revision_id: String,
    }
    let mut jobs: Vec<Job> = Vec::new();
    for lid in &law_ids {
        let p = meta_dir.join(format!("{}.json", lid));
        let bytes = match std::fs::read(&p) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("skip {}: read meta: {e}", lid);
                continue;
            }
        };
        let parsed: LawRevisionList = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("skip {}: parse meta: {e}", lid);
                continue;
            }
        };
        let mut revs: Vec<&RevisionMeta> = parsed.revisions.iter().collect();
        if let Some(n) = limit_revs_per_law {
            revs.truncate(n);
        }
        for r in revs {
            if r.law_revision_id.is_empty() {
                continue;
            }
            let out = revisions_root
                .join(lid)
                .join(format!("{}.xml", r.law_revision_id));
            if out.exists() && !force {
                continue;
            }
            jobs.push(Job {
                law_id: lid.clone(),
                revision_id: r.law_revision_id.clone(),
            });
        }
    }

    let total = jobs.len();
    tracing::info!(
        "fetch-revision-bodies: {} body(ies) to fetch across {} law(s), concurrency={}",
        total,
        law_ids.len(),
        concurrency
    );
    if total == 0 {
        return Ok(());
    }

    let provider = http_provider_v2();
    let counter = std::sync::atomic::AtomicUsize::new(0);
    let errors = std::sync::Mutex::new(Vec::<(String, String, String)>::new());
    let concurrency = concurrency.clamp(1, 8);

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(concurrency)
        .build()
        .context("rayon pool")?;
    pool.install(|| {
        use rayon::prelude::*;
        jobs.par_iter().for_each(|job| {
            let dir = revisions_root.join(&job.law_id);
            if let Err(e) = std::fs::create_dir_all(&dir) {
                errors.lock().unwrap().push((
                    job.law_id.clone(),
                    job.revision_id.clone(),
                    format!("mkdir: {e}"),
                ));
                return;
            }
            let xml_path = dir.join(format!("{}.xml", job.revision_id));
            let meta_path = dir.join(format!("{}.meta.json", job.revision_id));

            match provider.fetch_revision_body(&job.revision_id) {
                Ok(bytes) => {
                    let sha = sha256_hex(&bytes);
                    // 軽くパース検証してから書く (HTML 等の混入を弾く)
                    if parse_law_xml(&bytes, &job.law_id).is_err() {
                        errors.lock().unwrap().push((
                            job.law_id.clone(),
                            job.revision_id.clone(),
                            "parse_law_xml failed".to_string(),
                        ));
                        return;
                    }
                    if let Err(e) = std::fs::write(&xml_path, &bytes) {
                        errors.lock().unwrap().push((
                            job.law_id.clone(),
                            job.revision_id.clone(),
                            format!("write xml: {e}"),
                        ));
                        return;
                    }
                    let meta = json!({
                        "law_id": job.law_id,
                        "revision_id": job.revision_id,
                        "first_seen_date": "",
                        "sha256": sha,
                        "source_url": format!(
                            "https://laws.e-gov.go.jp/api/2/law_data/{}?response_format=xml",
                            job.revision_id
                        ),
                        "bytes": bytes.len(),
                        "fetched_via": "fetch_revision_bodies",
                    });
                    if let Err(e) = std::fs::write(
                        &meta_path,
                        serde_json::to_vec_pretty(&meta).unwrap_or_default(),
                    ) {
                        tracing::warn!(
                            "write meta failed for {}/{}: {e}",
                            job.law_id,
                            job.revision_id
                        );
                    }
                }
                Err(e) => {
                    errors.lock().unwrap().push((
                        job.law_id.clone(),
                        job.revision_id.clone(),
                        format!("{e:#}"),
                    ));
                }
            }
            let n = counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            if n % 50 == 0 {
                tracing::info!("fetch-revision-bodies: {}/{}", n, total);
            }
        });
    });

    let errs = errors.into_inner().unwrap();
    if !errs.is_empty() {
        tracing::warn!("fetch-revision-bodies: {} errors", errs.len());
        for (lid, rid, msg) in errs.iter().take(10) {
            tracing::warn!("  {lid}/{rid}: {msg}");
        }
        if errs.len() > 10 {
            tracing::warn!("  ... and {} more", errs.len() - 10);
        }
    }
    tracing::info!("fetch-revision-bodies done");
    Ok(())
}

/// `.cache/revisions_meta/{id}.json` を 1 つの JSONL に pack / unpack する。
/// pack: ディレクトリの全 JSON を 1 行 1 法令の JSONL にまとめる
///       (`{"law_id":"...","law_info":{...},"revisions":[...]}`)。
/// unpack: 同形式の JSONL を法令毎の JSON ファイルに展開する。
pub fn run_bundle_revisions_meta(mode: &str, dir: &Path, file: &Path) -> Result<()> {
    match mode {
        "pack" => {
            anyhow::ensure!(dir.exists(), "{} not found", dir.display());
            if let Some(parent) = file.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut writer = std::io::BufWriter::new(std::fs::File::create(file)?);
            use std::io::Write;
            let mut count = 0usize;
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let p = entry.path();
                if p.extension().and_then(|s| s.to_str()) != Some("json") {
                    continue;
                }
                let law_id = match p.file_stem().and_then(|s| s.to_str()) {
                    Some(s) => s.to_string(),
                    None => continue,
                };
                let bytes = std::fs::read(&p)?;
                let mut v: serde_json::Value = match serde_json::from_slice(&bytes) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!("skip bad meta {}: {e:#}", p.display());
                        continue;
                    }
                };
                if let Some(obj) = v.as_object_mut() {
                    obj.insert("law_id".to_string(), serde_json::Value::String(law_id));
                }
                let line = serde_json::to_vec(&v)?;
                writer.write_all(&line)?;
                writer.write_all(b"\n")?;
                count += 1;
            }
            writer.flush()?;
            tracing::info!("bundle pack: wrote {count} laws to {}", file.display());
            Ok(())
        }
        "unpack" => {
            anyhow::ensure!(file.exists(), "{} not found", file.display());
            std::fs::create_dir_all(dir)?;
            let reader = std::io::BufReader::new(std::fs::File::open(file)?);
            use std::io::BufRead;
            let mut count = 0usize;
            for (i, line) in reader.lines().enumerate() {
                let line = line?;
                if line.trim().is_empty() {
                    continue;
                }
                let v: serde_json::Value = serde_json::from_str(&line)
                    .with_context(|| format!("parse line {} of {}", i + 1, file.display()))?;
                let law_id = match v.get("law_id").and_then(|s| s.as_str()) {
                    Some(s) => s.to_string(),
                    None => {
                        tracing::warn!("line {} has no law_id, skipping", i + 1);
                        continue;
                    }
                };
                let mut out = v.clone();
                if let Some(obj) = out.as_object_mut() {
                    obj.remove("law_id");
                }
                let body = serde_json::to_vec_pretty(&out)?;
                std::fs::write(dir.join(format!("{}.json", law_id)), body)?;
                count += 1;
            }
            tracing::info!("bundle unpack: wrote {count} files to {}", dir.display());
            Ok(())
        }
        other => anyhow::bail!("unknown mode: {other} (expected pack|unpack)"),
    }
}

pub fn run_fetch_update(date: &str, cache: &Path, provider: &str) -> Result<usize> {
    let p = provider_by_name(provider)?;
    let batch = p.fetch_update(date)?;
    let new_xmls = write_cache_batch(cache, &batch.date, &batch.laws)?;
    tracing::info!(
        "date={date}: provider returned {} laws, {new_xmls} new XML(s) cached",
        batch.laws.len()
    );
    // 差分対象の各 law の改正履歴メタも同時に refresh する。
    // - provider が "mock" の場合は v2 API を叩かないので skip。
    // - 失敗しても warn にとどめ、本体 (XML 取得) 成功は壊さない。
    // - 件数は通常 0〜数件なので並列化せず逐次 (rate-limit 安全側)。
    if provider == "http" && !batch.laws.is_empty() {
        let v2 = http_provider_v2();
        let meta_dir = cache.join("revisions_meta");
        if let Err(e) = std::fs::create_dir_all(&meta_dir) {
            tracing::warn!("create revisions_meta dir failed: {e}");
        } else {
            for l in &batch.laws {
                match v2.fetch_law_revisions(&l.law_id) {
                    Ok(list) => match serde_json::to_vec_pretty(&list) {
                        Ok(body) => {
                            if let Err(e) =
                                std::fs::write(meta_dir.join(format!("{}.json", l.law_id)), body)
                            {
                                tracing::warn!(
                                    "refresh revisions_meta for {}: write failed: {e}",
                                    l.law_id
                                );
                            }
                        }
                        Err(e) => tracing::warn!(
                            "refresh revisions_meta for {}: serialize failed: {e}",
                            l.law_id
                        ),
                    },
                    Err(e) => tracing::warn!(
                        "refresh revisions_meta for {}: fetch failed: {e:#}",
                        l.law_id
                    ),
                }
            }
        }
    }
    Ok(new_xmls)
}

fn cache_has_revisions(cache: &Path) -> bool {
    let dir = cache.join("revisions");
    if !dir.exists() {
        return false;
    }
    std::fs::read_dir(&dir)
        .map(|it| {
            it.filter_map(|e| e.ok())
                .any(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        })
        .unwrap_or(false)
}

pub fn run_fetch_bulk(
    category: u32,
    limit: Option<usize>,
    cache: &Path,
    provider: &str,
) -> Result<()> {
    let p = provider_by_name(provider)?;
    let batch = p.fetch_bulk(category, limit)?;
    // bulk 取得を JST 今日付の更新として記録する。non-date label
    // ("bulk-catN") を first_seen_date に使うと updates/{date}.json や
    // updates/latest.json が壊れるので、必ず YYYY-MM-DD で揃える。
    let today = (Utc::now() + chrono::Duration::hours(9))
        .date_naive()
        .format("%Y-%m-%d")
        .to_string();
    let n = write_cache_batch(cache, &today, &batch.laws)?;
    tracing::info!(
        "bulk: category={category} fetched={} new={n} stamped_as={today}",
        batch.laws.len()
    );
    Ok(())
}

pub fn run_fetch_range(from: &str, to: &str, cache: &Path, provider: &str) -> Result<()> {
    use chrono::NaiveDate;
    let from = NaiveDate::parse_from_str(from, "%Y-%m-%d")?;
    let to = NaiveDate::parse_from_str(to, "%Y-%m-%d")?;
    anyhow::ensure!(from <= to, "from must be <= to");
    let p = provider_by_name(provider)?;
    let mut cur = from;
    while cur <= to {
        let d = cur.format("%Y-%m-%d").to_string();
        let batch = p.fetch_update(&d)?;
        let n = write_cache_batch(cache, &batch.date, &batch.laws)?;
        tracing::info!("date={d}: {n} new XML(s)");
        cur += chrono::Duration::days(1);
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedMeta {
    source: String,
    kind: String,
    date: String,
    law_id: String,
    url: String,
    sha256: String,
    fetched_at: String,
    bytes: usize,
}

fn revision_id_from_sha(sha: &str) -> String {
    sha.chars().take(12).collect()
}

/// Write today's snapshot to `.cache/egov/{date}/{id}.xml` AND, if the sha is new,
/// archive a copy at `.cache/revisions/{id}/{revision_id}.xml`. The revisions dir
/// is the source of truth for historical versions; the per-date dir is just an
/// audit trail of what was visible on each fetch day.
/// その日に新しく書き込んだ XML ファイル数を返す (sha256 重複は除外)。
fn write_cache_batch(cache: &Path, date: &str, laws: &[FetchedLaw]) -> Result<usize> {
    let dir = cache.join("egov").join(date);
    std::fs::create_dir_all(&dir)?;
    let mut new_count = 0usize;
    for law in laws {
        let xml_path = dir.join(format!("{}.xml", law.law_id));
        let meta_path = dir.join(format!("{}.meta.json", law.law_id));
        let new_sha = sha256_hex(&law.xml);

        if xml_path.exists() {
            let existing = std::fs::read(&xml_path)?;
            if sha256_hex(&existing) == new_sha {
                continue;
            }
        }
        std::fs::write(&xml_path, &law.xml)?;
        new_count += 1;
        let meta = CachedMeta {
            source: "egov".to_string(),
            kind: "daily_update_xml".to_string(),
            date: date.to_string(),
            law_id: law.law_id.clone(),
            url: law.source_url.clone(),
            sha256: new_sha.clone(),
            fetched_at: Utc::now().to_rfc3339(),
            bytes: law.xml.len(),
        };
        std::fs::write(&meta_path, serde_json::to_vec_pretty(&meta)?)?;

        let rev_dir = cache.join("revisions").join(&law.law_id);
        std::fs::create_dir_all(&rev_dir)?;
        let rev_id = revision_id_from_sha(&new_sha);
        let rev_xml = rev_dir.join(format!("{}.xml", rev_id));
        if !rev_xml.exists() {
            std::fs::write(&rev_xml, &law.xml)?;
            let rev_meta = json!({
                "law_id": law.law_id,
                "revision_id": rev_id,
                "first_seen_date": date,
                "sha256": new_sha,
                "source_url": law.source_url,
                "bytes": law.xml.len(),
            });
            std::fs::write(
                rev_dir.join(format!("{}.meta.json", rev_id)),
                serde_json::to_vec_pretty(&rev_meta)?,
            )?;
        }
    }
    Ok(new_count)
}

#[derive(Debug, Clone)]
struct Revision {
    revision_id: String,
    #[allow(dead_code)]
    sha256: String,
    first_seen_date: String,
    doc: LawDocument,
}

#[derive(Debug, Clone)]
struct LawWithHistory {
    law_id: String,
    /// 古い順 (first_seen_date 昇順)。最後の要素が現行版。
    /// これは「本文が手元にある revision」だけを表す — メタだけの履歴は別。
    revisions: Vec<Revision>,
    /// このビルドで「新しく取得された」日 → 直近の change_type 推定に使う。
    fetched_dates: BTreeMap<String, String>, // date -> revision_id
    /// e-Gov v2 `/law_revisions/{id}` で取れた改正履歴メタ。
    /// 本文を持っているとは限らない (殆どは meta-only)。古い順。
    meta_revisions: Vec<RevisionMeta>,
    /// 同 v2 から得た正規 `LawInfo` (公布日・法令番号・元号 など)。
    meta_law_info: Option<egov_client::LawInfoV2>,
}

impl LawWithHistory {
    fn current(&self) -> &LawDocument {
        &self.revisions.last().unwrap().doc
    }
    fn current_rev(&self) -> &Revision {
        self.revisions.last().unwrap()
    }
    /// 指定 rev_id の直前 (first_seen_date が一つ前) の rev を返す。
    fn prev_of(&self, rev_id: &str) -> Option<&Revision> {
        let idx = self
            .revisions
            .iter()
            .position(|r| r.revision_id == rev_id)?;
        if idx == 0 {
            None
        } else {
            self.revisions.get(idx - 1)
        }
    }
    fn rev(&self, rev_id: &str) -> Option<&Revision> {
        self.revisions.iter().find(|r| r.revision_id == rev_id)
    }
}

#[derive(Debug, Clone, Serialize)]
struct ArticleDiff {
    added: Vec<String>,    // article_id
    removed: Vec<String>,  // article_id
    modified: Vec<String>, // article_id
}

fn diff_articles(prev: &LawDocument, cur: &LawDocument) -> ArticleDiff {
    use std::collections::BTreeMap;
    let prev_map: BTreeMap<&str, &law_normalizer::Article> = prev
        .articles
        .iter()
        .map(|a| (a.article_id.as_str(), a))
        .collect();
    let cur_map: BTreeMap<&str, &law_normalizer::Article> = cur
        .articles
        .iter()
        .map(|a| (a.article_id.as_str(), a))
        .collect();
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut modified = Vec::new();
    for (id, a) in &cur_map {
        match prev_map.get(id) {
            None => added.push(id.to_string()),
            Some(p) => {
                if serde_json::to_string(p).ok() != serde_json::to_string(a).ok() {
                    modified.push(id.to_string());
                }
            }
        }
    }
    for id in prev_map.keys() {
        if !cur_map.contains_key(id) {
            removed.push(id.to_string());
        }
    }
    ArticleDiff {
        added,
        removed,
        modified,
    }
}

pub fn run_build_json(input: &Path, output: &Path) -> Result<()> {
    // メモリ有界化: 法令を 1 件ずつ load → 本文ファイルを書き出し → 履歴 doc を解放する。
    // (旧実装は全 revision の LawDocument を一括で RAM に載せ、全件履歴では 16GB を
    //  超えて OOM していた。build-diffs は public のファイルを 1 法令ずつ読むので、
    //  ここで履歴本文を書き出しさえすれば後段は元々有界。)
    //
    // グローバル索引 / 検索 / SEO は「現行 doc + meta だけ持つ軽量 Vec」から既存
    // writer で生成する。updates の差分計算 (compute_diff) は fetched_dates を持つ
    // 法令にだけ必要なので、その法令だけ履歴 doc を残す (日次 update は少数、
    // backfill は 0 件)。出力は旧実装とバイト等価 (generated_at を除く)。
    let tmp = output.with_file_name(format!(
        "{}.tmp",
        output
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("public")
    ));
    if tmp.exists() {
        std::fs::remove_dir_all(&tmp)?;
    }
    std::fs::create_dir_all(&tmp)?;
    write_schema(&tmp)?;

    let egov = build_egov_index(input)?;
    let law_ids = collect_law_ids(input, &egov)?;
    let mut light: Vec<LawWithHistory> = Vec::new();
    let mut skipped = 0usize;
    for law_id in law_ids {
        let mut law = build_one_law(input, &law_id, egov.get(&law_id))?;
        if law.revisions.is_empty() {
            // 本文を持たない (meta-only) 法令は配信対象外 (旧 build_into と同じ)。
            skipped += 1;
            continue;
        }
        // この法令の本文ファイル (current/revisions/articles/versions/timeline) を書き出す。
        write_law_documents(&tmp, std::slice::from_ref(&law))?;
        if law.fetched_dates.is_empty() {
            // 履歴 doc を解放しピーク RAM を抑える。現行版だけ残す。
            let cur = law.current_rev().clone();
            law.revisions = vec![cur];
        }
        light.push(law);
    }
    if light.is_empty() {
        anyhow::bail!(
            "no law XML found under {} — run `lawpub fetch-update` first",
            input.display()
        );
    }
    if skipped > 0 {
        tracing::info!(
            "build-json: skipped {} meta-only laws (no body cached)",
            skipped
        );
    }

    write_indices(&tmp, &light)?;
    write_per_date_updates(&tmp, &light)?;
    write_seo(&tmp, &light)?;
    write_search_db(&tmp, &light)?;
    write_manifest_and_health(&tmp, &light)?;

    swap_into(output, &tmp)
}

pub fn run_build_index(output: &Path) -> Result<()> {
    let docs = read_existing_law_documents(output)?;
    let laws: Vec<LawWithHistory> = docs
        .into_iter()
        .map(|doc| {
            let sha = doc.source.raw_xml_sha256.clone().unwrap_or_default();
            let rev_id = revision_id_from_sha(&sha);
            LawWithHistory {
                law_id: doc.law_id.clone(),
                revisions: vec![Revision {
                    revision_id: rev_id,
                    sha256: sha,
                    first_seen_date: String::new(),
                    doc,
                }],
                fetched_dates: BTreeMap::new(),
                meta_revisions: Vec::new(),
                meta_law_info: None,
            }
        })
        .collect();
    write_indices(output, &laws)?;
    write_manifest_and_health(output, &laws)?;
    Ok(())
}

pub fn run_update(
    public: &Path,
    cache: &Path,
    provider: &str,
    date: Option<&str>,
    force: bool,
) -> Result<()> {
    let state_path = PathBuf::from("state/latest.json");
    let last_run_path = PathBuf::from("state/last_run.json");
    let mut st = state::load(&state_path)?;

    let today_jst = (Utc::now() + chrono::Duration::hours(9)).date_naive();
    let dates = match date {
        Some(d) => vec![d.to_string()],
        None => state::pick_dates(&st, today_jst),
    };

    tracing::info!("update target dates: {:?}", dates);
    let mut new_xmls = 0usize;
    let mut errors: Vec<String> = Vec::new();
    for d in &dates {
        match run_fetch_update(d, cache, provider) {
            Ok(n) => new_xmls += n,
            Err(e) => {
                tracing::warn!("fetch {} failed: {e:#}", d);
                errors.push(format!("{d}: {e:#}"));
            }
        }
    }

    // public/ が存在しない/壊れている場合は強制再生成。
    let public_complete = public.join("manifest.json").exists();
    let mut changed = force || new_xmls > 0 || !public_complete;
    if changed {
        // cache が空のときに build_json は bail するが、`--force` の場合は
        // 「キャッシュが復元できていないだけ」のケースが多いので、warn にし
        // 既存 public/ をそのまま残す方針に変える。
        if cache_has_revisions(cache) {
            run_build_json(cache, public)?;
        } else if public_complete {
            tracing::warn!(
                "cache is empty but public/ already exists — keeping existing public/ as-is"
            );
            changed = false;
        } else {
            anyhow::bail!(
                "cache is empty and public/ does not exist — run `lawpub fetch-bulk --category 1` first"
            );
        }
    } else {
        tracing::info!(
            "no new revisions and public/ is intact — skipping rebuild ({} dates checked)",
            dates.len()
        );
    }

    // Phase 1: 差分 / スナップショット生成。
    // build-json が走った直後は public/ が新しくなっているので、毎回再計算する。
    // 失敗しても update 全体は壊さない方針 (差分は補助情報)。
    let mut wrote_post_artifacts = false;
    if changed && public.join("laws").join("index.json").exists() {
        match crate::diffs::run_build_diffs(public) {
            Ok(()) => wrote_post_artifacts = true,
            Err(e) => tracing::warn!("build-diffs failed (continuing): {e:#}"),
        }
        // 任意日付スナップショットはコストが大 (法令数 × 日付数 のファイルが出る)
        // ため、`LAWPUB_SNAPSHOT_DATES` 環境変数が設定されたときだけ生成する。
        // 例: `LAWPUB_SNAPSHOT_DATES=2018-04-01,2020-04-01,2024-04-01`
        if let Ok(raw) = std::env::var("LAWPUB_SNAPSHOT_DATES") {
            let dates: Vec<String> = raw
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if !dates.is_empty() {
                match crate::snapshots::run_build_snapshots(public, &dates, false) {
                    Ok(()) => wrote_post_artifacts = true,
                    Err(e) => tracing::warn!("build-snapshots failed (continuing): {e:#}"),
                }
            }
        }
        // diff/snapshot は manifest.json の後に書かれるため、validate を通すには
        // manifest を再生成する必要がある。
        if wrote_post_artifacts {
            if let Err(e) = rebuild_manifest(public) {
                tracing::warn!("rebuild_manifest after diffs/snapshots failed: {e:#}");
            }
        }
    }

    if let Some(last) = dates.last() {
        st.latest_successful_update_date = Some(last.clone());
    }
    st.last_run_at = Some(state::now_iso());
    st.last_run_status = Some(if errors.is_empty() {
        "ok".into()
    } else {
        "partial".into()
    });
    state::save(&state_path, &st)?;

    let report = state::RunReport {
        version: 1,
        ran_at: state::now_iso(),
        provider: provider.to_string(),
        dates: dates.clone(),
        new_xmls,
        errors,
        changed,
    };
    state::save_run_report(&last_run_path, &report)?;
    Ok(())
}

/// Walk `.cache/revisions/{law_id}/{rev_id}.xml` to build the historical version
/// list, then walk `.cache/egov/{date}/{law_id}.xml` to learn which revisions
/// became visible on which dates. The latter informs `updates/{date}.json`.
#[allow(dead_code)] // 旧一括実装。run_build_json はストリーム版に移行済み (参照用に保持)。
fn collect_laws_with_history(cache: &Path) -> Result<Vec<LawWithHistory>> {
    let revisions_dir = cache.join("revisions");
    let egov_dir = cache.join("egov");

    let mut by_law: BTreeMap<String, LawWithHistory> = BTreeMap::new();

    if revisions_dir.exists() {
        for law_dir in std::fs::read_dir(&revisions_dir)? {
            let law_dir = law_dir?;
            if !law_dir.file_type()?.is_dir() {
                continue;
            }
            let law_id = law_dir.file_name().to_string_lossy().to_string();
            let mut revs: Vec<Revision> = Vec::new();
            for f in std::fs::read_dir(law_dir.path())? {
                let f = f?;
                let path = f.path();
                if path.extension().and_then(|s| s.to_str()) != Some("xml") {
                    continue;
                }
                let rev_id = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                let bytes = std::fs::read(&path)?;
                let sha = sha256_hex(&bytes);

                let meta_path = path.with_extension("meta.json");
                let first_seen = if meta_path.exists() {
                    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&meta_path)?)?;
                    v.get("first_seen_date")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string()
                } else {
                    String::new()
                };

                // 不正な XML (HTML エラーページが混じった等) でビルド全体が
                // 死なないように warn + skip + cache の不正ファイル削除。
                let doc = match parse_law_xml(&bytes, &law_id) {
                    Ok(d) => d,
                    Err(e) => {
                        tracing::warn!("skip bad XML cache {}: {e:#}", path.display());
                        let _ = std::fs::remove_file(&path);
                        let _ = std::fs::remove_file(meta_path);
                        continue;
                    }
                };
                revs.push(Revision {
                    revision_id: rev_id,
                    sha256: sha,
                    first_seen_date: first_seen,
                    doc,
                });
            }
            revs.sort_by(|a, b| a.first_seen_date.cmp(&b.first_seen_date));
            if !revs.is_empty() {
                by_law.insert(
                    law_id.clone(),
                    LawWithHistory {
                        law_id,
                        revisions: revs,
                        fetched_dates: BTreeMap::new(),
                        meta_revisions: Vec::new(),
                        meta_law_info: None,
                    },
                );
            }
        }
    }

    // egov/{date}/{id}.xml で「どの日にどのrevが見えたか」を埋める。
    if egov_dir.exists() {
        for date_dir in std::fs::read_dir(&egov_dir)? {
            let date_dir = date_dir?;
            if !date_dir.file_type()?.is_dir() {
                continue;
            }
            let date = date_dir.file_name().to_string_lossy().to_string();
            for f in std::fs::read_dir(date_dir.path())? {
                let f = f?;
                let path = f.path();
                if path.extension().and_then(|s| s.to_str()) != Some("xml") {
                    continue;
                }
                let law_id = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                let bytes = std::fs::read(&path)?;
                let sha = sha256_hex(&bytes);
                let rev_id = revision_id_from_sha(&sha);

                // revisions が無い (履歴が消失した) ときの保険として、その場で1件作る。
                let entry = by_law
                    .entry(law_id.clone())
                    .or_insert_with(|| LawWithHistory {
                        law_id: law_id.clone(),
                        revisions: Vec::new(),
                        fetched_dates: BTreeMap::new(),
                        meta_revisions: Vec::new(),
                        meta_law_info: None,
                    });
                if !entry.revisions.iter().any(|r| r.revision_id == rev_id) {
                    let doc = match parse_law_xml(&bytes, &law_id) {
                        Ok(d) => d,
                        Err(e) => {
                            tracing::warn!("skip bad XML cache {}: {e:#}", path.display());
                            let _ = std::fs::remove_file(&path);
                            continue;
                        }
                    };
                    entry.revisions.push(Revision {
                        revision_id: rev_id.clone(),
                        sha256: sha,
                        first_seen_date: date.clone(),
                        doc,
                    });
                    entry
                        .revisions
                        .sort_by(|a, b| a.first_seen_date.cmp(&b.first_seen_date));
                }
                entry.fetched_dates.insert(date.clone(), rev_id);
            }
        }
    }

    // v2 `.cache/revisions_meta/{law_id}.json` を読み込み、各 LawWithHistory に
    // meta_revisions / meta_law_info を流し込む。timeline.json / versions.json の
    // 「改正履歴フル一覧」の駆動源になる。本文 (.cache/revisions/) が無い meta は
    // メタだけ持ち、本文閲覧は出来ない (= UI で "本文 future revision" 表示)。
    let meta_dir = cache.join("revisions_meta");
    if meta_dir.exists() {
        for f in std::fs::read_dir(&meta_dir)? {
            let f = f?;
            let p = f.path();
            if p.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let law_id = match p.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };
            let bytes = std::fs::read(&p)?;
            let list: LawRevisionList = match serde_json::from_slice(&bytes) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!("skip bad revisions_meta {}: {e:#}", p.display());
                    continue;
                }
            };
            // 改正履歴は通常新しい順 (e-Gov) で返るので、古い順に並び替えてから格納。
            let mut revs = list.revisions;
            revs.sort_by(|a, b| {
                a.amendment_promulgate_date
                    .as_deref()
                    .unwrap_or("")
                    .cmp(b.amendment_promulgate_date.as_deref().unwrap_or(""))
            });
            let entry = by_law
                .entry(law_id.clone())
                .or_insert_with(|| LawWithHistory {
                    law_id: law_id.clone(),
                    revisions: Vec::new(),
                    fetched_dates: BTreeMap::new(),
                    meta_revisions: Vec::new(),
                    meta_law_info: None,
                });
            entry.meta_revisions = revs;
            entry.meta_law_info = Some(list.law_info);
        }
    }

    Ok(by_law.into_values().collect())
}

fn read_existing_law_documents(public: &Path) -> Result<Vec<LawDocument>> {
    let mut out = Vec::new();
    let laws_dir = public.join("laws");
    if !laws_dir.exists() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(&laws_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let cur = entry.path().join("current.json");
        if !cur.exists() {
            continue;
        }
        let bytes = std::fs::read(&cur)?;
        let doc: LawDocument =
            serde_json::from_slice(&bytes).with_context(|| format!("parse {}", cur.display()))?;
        out.push(doc);
    }
    Ok(out)
}

/// tmp ディレクトリを public へ原子的に差し替える (旧 public は .bak 経由でロールバック可)。
fn swap_into(public: &Path, tmp: &Path) -> Result<()> {
    let backup = public.with_file_name(format!(
        "{}.bak",
        public
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("public")
    ));
    if backup.exists() {
        std::fs::remove_dir_all(&backup)?;
    }
    if public.exists() {
        std::fs::rename(public, &backup)?;
    }
    if let Err(e) = std::fs::rename(tmp, public) {
        if backup.exists() {
            let _ = std::fs::rename(&backup, public);
        }
        return Err(e.into());
    }
    if backup.exists() {
        let _ = std::fs::remove_dir_all(&backup);
    }
    Ok(())
}

/// `.cache/egov/{date}/{law_id}.xml` を走査し `law_id -> [(date, path)]` の軽量索引を作る。
/// 本文はここではパースせず、per-law 構築時に取り込む。
fn build_egov_index(cache: &Path) -> Result<BTreeMap<String, Vec<(String, PathBuf)>>> {
    let mut idx: BTreeMap<String, Vec<(String, PathBuf)>> = BTreeMap::new();
    let egov_dir = cache.join("egov");
    if egov_dir.exists() {
        for date_dir in std::fs::read_dir(&egov_dir)? {
            let date_dir = date_dir?;
            if !date_dir.file_type()?.is_dir() {
                continue;
            }
            let date = date_dir.file_name().to_string_lossy().to_string();
            for f in std::fs::read_dir(date_dir.path())? {
                let f = f?;
                let path = f.path();
                if path.extension().and_then(|s| s.to_str()) != Some("xml") {
                    continue;
                }
                let law_id = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                idx.entry(law_id).or_default().push((date.clone(), path));
            }
        }
    }
    for v in idx.values_mut() {
        v.sort_by(|a, b| a.0.cmp(&b.0));
    }
    Ok(idx)
}

/// 配信対象になりうる law_id 集合 (revisions / egov / meta の和)。BTreeSet で law_id 昇順。
fn collect_law_ids(
    cache: &Path,
    egov: &BTreeMap<String, Vec<(String, PathBuf)>>,
) -> Result<std::collections::BTreeSet<String>> {
    let mut ids: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let rev_dir = cache.join("revisions");
    if rev_dir.exists() {
        for e in std::fs::read_dir(&rev_dir)? {
            let e = e?;
            if e.file_type()?.is_dir() {
                ids.insert(e.file_name().to_string_lossy().to_string());
            }
        }
    }
    let meta_dir = cache.join("revisions_meta");
    if meta_dir.exists() {
        for e in std::fs::read_dir(&meta_dir)? {
            let e = e?;
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Some(s) = p.file_stem().and_then(|s| s.to_str()) {
                    ids.insert(s.to_string());
                }
            }
        }
    }
    for k in egov.keys() {
        ids.insert(k.clone());
    }
    Ok(ids)
}

/// 1 法令分の `LawWithHistory` を構築する (.cache/revisions + egov + revisions_meta)。
/// 旧 `collect_laws_with_history` を per-law に分解したもの。出力互換を保つため
/// revision の収集順 / sort / meta マージは旧実装と同一にしてある。
fn build_one_law(
    cache: &Path,
    law_id: &str,
    egov_entries: Option<&Vec<(String, PathBuf)>>,
) -> Result<LawWithHistory> {
    let mut revs: Vec<Revision> = Vec::new();
    let law_rev_dir = cache.join("revisions").join(law_id);
    if law_rev_dir.is_dir() {
        for f in std::fs::read_dir(&law_rev_dir)? {
            let f = f?;
            let path = f.path();
            if path.extension().and_then(|s| s.to_str()) != Some("xml") {
                continue;
            }
            let rev_id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let bytes = std::fs::read(&path)?;
            let sha = sha256_hex(&bytes);
            let meta_path = path.with_extension("meta.json");
            let first_seen = if meta_path.exists() {
                let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&meta_path)?)?;
                v.get("first_seen_date")
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_string()
            } else {
                String::new()
            };
            let doc = match parse_law_xml(&bytes, law_id) {
                Ok(d) => d,
                Err(e) => {
                    tracing::warn!("skip bad XML cache {}: {e:#}", path.display());
                    let _ = std::fs::remove_file(&path);
                    let _ = std::fs::remove_file(meta_path);
                    continue;
                }
            };
            revs.push(Revision {
                revision_id: rev_id,
                sha256: sha,
                first_seen_date: first_seen,
                doc,
            });
        }
        revs.sort_by(|a, b| a.first_seen_date.cmp(&b.first_seen_date));
    }

    let mut law = LawWithHistory {
        law_id: law_id.to_string(),
        revisions: revs,
        fetched_dates: BTreeMap::new(),
        meta_revisions: Vec::new(),
        meta_law_info: None,
    };

    // egov: どの日にどの rev が見えたか。.cache/revisions に無い新規 rev はここで取り込む。
    if let Some(entries) = egov_entries {
        for (date, path) in entries {
            let bytes = std::fs::read(path)?;
            let sha = sha256_hex(&bytes);
            let rev_id = revision_id_from_sha(&sha);
            if !law.revisions.iter().any(|r| r.revision_id == rev_id) {
                let doc = match parse_law_xml(&bytes, law_id) {
                    Ok(d) => d,
                    Err(e) => {
                        tracing::warn!("skip bad XML cache {}: {e:#}", path.display());
                        let _ = std::fs::remove_file(path);
                        continue;
                    }
                };
                law.revisions.push(Revision {
                    revision_id: rev_id.clone(),
                    sha256: sha,
                    first_seen_date: date.clone(),
                    doc,
                });
                law.revisions
                    .sort_by(|a, b| a.first_seen_date.cmp(&b.first_seen_date));
            }
            law.fetched_dates.insert(date.clone(), rev_id);
        }
    }

    // v2 改正履歴メタ (古い順に整える)。
    let meta_path = cache.join("revisions_meta").join(format!("{law_id}.json"));
    if meta_path.exists() {
        let bytes = std::fs::read(&meta_path)?;
        match serde_json::from_slice::<LawRevisionList>(&bytes) {
            Ok(list) => {
                let mut mrevs = list.revisions;
                mrevs.sort_by(|a, b| {
                    a.amendment_promulgate_date
                        .as_deref()
                        .unwrap_or("")
                        .cmp(b.amendment_promulgate_date.as_deref().unwrap_or(""))
                });
                law.meta_revisions = mrevs;
                law.meta_law_info = Some(list.law_info);
            }
            Err(e) => {
                tracing::warn!("skip bad revisions_meta {}: {e:#}", meta_path.display());
            }
        }
    }

    Ok(law)
}

#[allow(dead_code)] // 旧一括実装。run_build_json はストリーム版に移行済み (参照用に保持)。
fn build_into(public: &Path, laws: &[LawWithHistory]) -> Result<()> {
    // 本文を 1 件も持たない (= meta だけ) の法令は配信対象から外す。
    // fetch_revision_bodies が間に合っていない大量の meta-only 法令でも
    // build がコケないようにするための安全網。
    let laws_owned: Vec<LawWithHistory> = laws
        .iter()
        .filter(|l| !l.revisions.is_empty())
        .cloned()
        .collect();
    let skipped = laws.len() - laws_owned.len();
    if skipped > 0 {
        tracing::info!(
            "build_into: skipped {} meta-only laws (no body cached)",
            skipped
        );
    }
    let laws = laws_owned.as_slice();

    let tmp = public.with_file_name(format!(
        "{}.tmp",
        public
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("public")
    ));
    if tmp.exists() {
        std::fs::remove_dir_all(&tmp)?;
    }
    std::fs::create_dir_all(&tmp)?;

    write_schema(&tmp)?;
    write_law_documents(&tmp, laws)?;
    write_indices(&tmp, laws)?;
    write_per_date_updates(&tmp, laws)?;
    write_seo(&tmp, laws)?;
    write_search_db(&tmp, laws)?;
    write_manifest_and_health(&tmp, laws)?;

    let backup = public.with_file_name(format!(
        "{}.bak",
        public
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("public")
    ));
    if backup.exists() {
        std::fs::remove_dir_all(&backup)?;
    }
    if public.exists() {
        std::fs::rename(public, &backup)?;
    }
    if let Err(e) = std::fs::rename(&tmp, public) {
        if backup.exists() {
            let _ = std::fs::rename(&backup, public);
        }
        return Err(e.into());
    }
    if backup.exists() {
        let _ = std::fs::remove_dir_all(&backup);
    }
    Ok(())
}

fn write_law_documents(public: &Path, laws: &[LawWithHistory]) -> Result<()> {
    for law in laws {
        // 本文を 1 件も持たない (= meta だけ取れている) 法令は current.json を
        // 書けないので skip する。fetch_revision_bodies が走るまでは大量の
        // meta-only 法令が居ても OK にしたい。
        if law.revisions.is_empty() {
            continue;
        }
        let dir = public.join("laws").join(&law.law_id);
        std::fs::create_dir_all(&dir)?;

        // 本文 (revisions[].revision_id) は今 .cache/revisions/{law_id}/{sha-rev-id}.xml
        // から sha 由来の ID で持っている。v2 meta が取れているなら、その「現在
        // 施行中」revision (`current_revision_status == "CurrentEnforced"`) の
        // v2 ID を本文の revision_id として採用する。これで versions.json /
        // timeline.json と current_revision_id が同じ ID 空間で揃う。
        //
        // 値域: CurrentEnforced / PreviousEnforced / UnEnforced / Repealed
        // (将来 Repealed の取扱いは要検討。今は CurrentEnforced を優先し、
        //  無ければ最新の v2 ID にフォールバック)。
        let current_v2_id: Option<String> = law
            .meta_revisions
            .iter()
            .rev()
            .find(|m| m.current_revision_status.as_deref() == Some("CurrentEnforced"))
            .map(|m| m.law_revision_id.clone())
            .or_else(|| law.meta_revisions.last().map(|m| m.law_revision_id.clone()));

        // current.json は最新版に revision_id を埋める。
        let cur_rev = law.current_rev();
        let current_rev_id = current_v2_id
            .clone()
            .unwrap_or_else(|| cur_rev.revision_id.clone());
        let mut current_doc = cur_rev.doc.clone();
        current_doc.revision_id = Some(current_rev_id.clone());
        write_json_pretty(&dir.join("current.json"), &current_doc)?;

        let articles_dir = dir.join("articles");
        std::fs::create_dir_all(&articles_dir)?;
        for a in &current_doc.articles {
            write_json_pretty(&articles_dir.join(format!("{}.json", a.article_id)), a)?;
        }

        // 過去 revision を全部書き出す (Phase 2 §7.6)。
        // 現状は本文を 1 件しか持っていないことが多いので、その 1 件を v2 ID
        // ファイル名で書き出して versions.json と紐付ける。
        let revisions_dir = dir.join("revisions");
        std::fs::create_dir_all(&revisions_dir)?;
        // 履歴束 (history.zst): 全版を NDJSON 1 ファイルにまとめ zstd(--long) で圧縮する。
        // 版間はほぼ同一なので大窓 zstd が重複を dedup し、per-file gzip の ~1/30 になる。
        // SPA はこの束を 1 回取得して履歴閲覧＋任意 2 版 diff をクライアント側で行える。
        let mut history_ndjson: Vec<u8> = Vec::new();
        for r in &law.revisions {
            let mut doc = r.doc.clone();
            let file_rev_id = if r.revision_id == cur_rev.revision_id {
                current_rev_id.clone()
            } else {
                r.revision_id.clone()
            };
            doc.revision_id = Some(file_rev_id.clone());
            doc.status = if file_rev_id == current_rev_id {
                "current".to_string()
            } else {
                "historical".to_string()
            };
            write_json_pretty(&revisions_dir.join(format!("{}.json", file_rev_id)), &doc)?;
            // 束には compact JSON を 1 行として追記 (同じ doc)。
            history_ndjson.extend_from_slice(&serde_json::to_vec(&doc)?);
            history_ndjson.push(b'\n');
        }
        std::fs::write(dir.join("history.ndjson.zst"), zstd_long(&history_ndjson)?)?;

        // versions.json: e-Gov v2 meta_revisions が取れていればそれを骨格にし、
        // 本文 (revisions/{id}/{rev_id}.xml) を持っている revision には path を
        // 埋める。meta が無い場合は従来通り本文ベースで書き出す (= fallback)。
        // 本文を持っている v2 ID 集合 = current_v2_id 一つ (今は) + 仮に
        // .cache/revisions/ に v2 ID 形式で配置されたファイルがあればそれら。
        // body_available は「実際に書き出した revision ファイル」と厳密に一致させる。
        // 現行版はファイル名が current_rev_id になる (上の revisions 書き出しと同じ規則)
        // ため、r.revision_id ではなく書き出し名 (file_rev_id) を登録する。これを誤ると
        // versions.json が存在しないファイルを body_available としてしまい、build-diffs が
        // そのファイルの読み込みに失敗する。
        let mut body_rev_ids: std::collections::BTreeSet<String> =
            std::collections::BTreeSet::new();
        for r in &law.revisions {
            let file_rev_id = if r.revision_id == cur_rev.revision_id {
                current_rev_id.clone()
            } else {
                r.revision_id.clone()
            };
            body_rev_ids.insert(file_rev_id);
        }
        let versions: Vec<_> = if !law.meta_revisions.is_empty() {
            law.meta_revisions
                .iter()
                .map(|m| {
                    let has_body = body_rev_ids.contains(&m.law_revision_id);
                    json!({
                        "revision_id": m.law_revision_id,
                        "effective_date": m.amendment_enforcement_date,
                        "scheduled_enforcement_date": m.amendment_scheduled_enforcement_date,
                        "promulgation_date": m.amendment_promulgate_date,
                        "amendment_law_id": m.amendment_law_id,
                        "amendment_law_num": m.amendment_law_num,
                        "amendment_law_title": m.amendment_law_title,
                        "amendment_type": m.amendment_type,
                        "mission": m.mission,
                        "repeal_status": m.repeal_status,
                        "current_revision_status": m.current_revision_status,
                        "path": if has_body {
                            serde_json::Value::String(format!("laws/{}/revisions/{}.json", law.law_id, m.law_revision_id))
                        } else {
                            serde_json::Value::Null
                        },
                        "body_available": has_body,
                    })
                })
                .collect()
        } else {
            law.revisions
                .iter()
                .map(|r| {
                    json!({
                        "revision_id": r.revision_id,
                        "effective_date": r.doc.effective_date,
                        "promulgation_date": r.doc.promulgation_date,
                        "path": format!("laws/{}/revisions/{}.json", law.law_id, r.revision_id),
                        "body_available": true,
                        "source_update_date": if r.first_seen_date.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(r.first_seen_date.clone()) },
                    })
                })
                .collect()
        };
        write_json_pretty(
            &dir.join("versions.json"),
            &json!({
                "law_id": law.law_id,
                "current_revision_id": current_rev_id,
                "versions": versions,
            }),
        )?;

        // timeline: meta_revisions があれば改正履歴一件=イベント一件、無ければ
        // 従来の「本文 revision 一件=snapshot 一件」フォールバック。
        //
        // 注意: e-Gov v2 `/law_revisions` は「改正」だけを返してくる (= 制定
        // そのものは含まれない)。制定イベントは `law_info.promulgation_date`
        // から合成して先頭に挿入する。これがないと「民法は 2016-04-13 改正
        // から始まったように見える」現象が出る。
        let events: Vec<_> = if !law.meta_revisions.is_empty() {
            let mut events: Vec<serde_json::Value> =
                Vec::with_capacity(law.meta_revisions.len() + 1);
            if let Some(info) = law.meta_law_info.as_ref() {
                if let Some(date) = info.promulgation_date.as_deref() {
                    events.push(json!({
                        "event_id": format!("evt_{}_enactment", law.law_id),
                        "event_type": "enactment",
                        "target_law_id": law.law_id,
                        "amending_law_id": null,
                        "amending_law_num": null,
                        "amending_law_title": null,
                        "promulgation_date": date,
                        "effective_date": null,
                        "scheduled_enforcement_date": null,
                        "enforcement_comment": null,
                        "revision_id": null,
                        "status": "Enacted",
                        "repeal_status": null,
                        "mission": "Enactment",
                        "kanpo": { "linked": false }
                    }));
                }
            }
            events.extend(law.meta_revisions.iter().map(|m| {
                    let event_type = match m.amendment_type.as_deref() {
                        Some("1") => "enactment",   // 制定
                        Some("3") => "amendment",   // 改正
                        Some("8") => "repeal",      // 廃止
                        _ => "snapshot",
                    };
                    json!({
                        "event_id": format!("evt_{}", m.law_revision_id),
                        "event_type": event_type,
                        "target_law_id": law.law_id,
                        "amending_law_id": m.amendment_law_id,
                        "amending_law_num": m.amendment_law_num,
                        "amending_law_title": m.amendment_law_title,
                        "promulgation_date": m.amendment_promulgate_date,
                        "effective_date": m.amendment_enforcement_date,
                        "scheduled_enforcement_date": m.amendment_scheduled_enforcement_date,
                        "enforcement_comment": m.amendment_enforcement_comment,
                        "revision_id": m.law_revision_id,
                        "status": m.current_revision_status.clone().unwrap_or_else(|| "Unknown".to_string()),
                        "repeal_status": m.repeal_status,
                        "mission": m.mission,
                        "kanpo": { "linked": false }
                    })
            }));
            events
        } else {
            law.revisions
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    let event_type = if i == 0 { "initial" } else { "snapshot" };
                    json!({
                        "event_id": format!("evt_{}", r.revision_id),
                        "event_type": event_type,
                        "target_law_id": law.law_id,
                        "amending_law_num": null,
                        "promulgation_date": r.doc.promulgation_date,
                        "effective_date": r.doc.effective_date,
                        "revision_id": r.revision_id,
                        "source_update_date": if r.first_seen_date.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(r.first_seen_date.clone()) },
                        "status": "snapshot",
                        "kanpo": { "linked": false }
                    })
                })
                .collect()
        };
        write_json_pretty(
            &dir.join("timeline.json"),
            &json!({
                "law_id": law.law_id,
                "events": events,
            }),
        )?;
    }
    Ok(())
}

fn write_indices(public: &Path, laws: &[LawWithHistory]) -> Result<()> {
    let generated_at = Utc::now().to_rfc3339();

    // v2 meta があれば category / amendment 件数を index に乗せる。UI 側の
    // mock LAWS による category マッピングを段階的に置き換えるための情報源。
    let summaries: Vec<serde_json::Value> = laws
        .iter()
        .map(|l| {
            let d = l.current();
            let category = l.meta_revisions.last().and_then(|m| m.category.clone());
            let revisions_count = l.meta_revisions.len();
            // 「更新順」ソート用。revisions は first_seen_date 昇順なので現行版の
            // first_seen_date が最新の取込日 = この法令が最後に更新された日。
            let last_updated = {
                let fsd = &l.current_rev().first_seen_date;
                if fsd.is_empty() {
                    serde_json::Value::Null
                } else {
                    serde_json::Value::String(fsd.clone())
                }
            };
            json!({
                "law_id": l.law_id,
                "law_num": d.law_num,
                "title": d.title,
                "category": category,
                "revisions_count": revisions_count,
                "last_updated": last_updated,
                "current": format!("laws/{}/current.json", l.law_id),
                "timeline": format!("laws/{}/timeline.json", l.law_id),
                "versions": format!("laws/{}/versions.json", l.law_id),
            })
        })
        .collect();

    write_json_pretty(
        &public.join("laws").join("index.json"),
        &json!({
            "version": SCHEMA_VERSION,
            "generated_at": generated_at,
            "laws": summaries,
        }),
    )?;

    write_json_pretty(
        &public.join("index.json"),
        &json!({
            "version": SCHEMA_VERSION,
            "generated_at": generated_at,
            "endpoints": {
                "laws": "laws/index.json",
                "updates_latest": "updates/latest.json",
                "manifest": "manifest.json",
                "health": "health.json"
            }
        }),
    )?;

    // updates/latest.json: 直近1日 (=最大の fetched_date) を採用。
    let latest_date = laws
        .iter()
        .flat_map(|l| l.fetched_dates.keys())
        .max()
        .cloned()
        .unwrap_or_default();
    let updated_laws: Vec<_> = if !latest_date.is_empty() {
        laws.iter()
            .filter_map(|l| {
                l.fetched_dates.get(&latest_date).map(|rev_id| {
                    let diff = compute_diff(l, rev_id);
                    json!({
                        "law_id": l.law_id,
                        "title": l.current().title,
                        "change_type": classify(l, &latest_date, rev_id),
                        "revision_id": rev_id,
                        "current": format!("laws/{}/current.json", l.law_id),
                        "article_diff": diff,
                    })
                })
            })
            .collect()
    } else {
        Vec::new()
    };
    let updates_dir = public.join("updates");
    std::fs::create_dir_all(&updates_dir)?;
    write_json_pretty(
        &updates_dir.join("latest.json"),
        &json!({
            "generated_at": generated_at,
            "latest_update_date": if latest_date.is_empty() { Utc::now().date_naive().format("%Y-%m-%d").to_string() } else { latest_date },
            "updated_laws": updated_laws,
        }),
    )?;

    Ok(())
}

/// その日に観測された rev_id がそのlawの最初の rev なら "added"、
/// それ以外なら "modified"。"removed" は別ソースが必要なので未実装。
fn classify(law: &LawWithHistory, _date: &str, rev_id: &str) -> &'static str {
    if law.revisions.first().map(|r| r.revision_id.as_str()) == Some(rev_id) {
        "added"
    } else {
        "modified"
    }
}

fn compute_diff(law: &LawWithHistory, rev_id: &str) -> ArticleDiff {
    let cur = match law.rev(rev_id) {
        Some(r) => r,
        None => {
            return ArticleDiff {
                added: vec![],
                removed: vec![],
                modified: vec![],
            };
        }
    };
    match law.prev_of(rev_id) {
        None => ArticleDiff {
            added: cur
                .doc
                .articles
                .iter()
                .map(|a| a.article_id.clone())
                .collect(),
            removed: vec![],
            modified: vec![],
        },
        Some(prev) => diff_articles(&prev.doc, &cur.doc),
    }
}

fn write_per_date_updates(public: &Path, laws: &[LawWithHistory]) -> Result<()> {
    let mut by_date: BTreeMap<String, Vec<(&LawWithHistory, &String)>> = BTreeMap::new();
    for l in laws {
        for (date, rev_id) in &l.fetched_dates {
            by_date.entry(date.clone()).or_default().push((l, rev_id));
        }
    }
    let updates_dir = public.join("updates");
    std::fs::create_dir_all(&updates_dir)?;
    for (date, entries) in by_date {
        let arr: Vec<_> = entries
            .iter()
            .map(|(l, rev_id)| {
                let diff = compute_diff(l, rev_id);
                json!({
                    "law_id": l.law_id,
                    "title": l.current().title,
                    "change_type": classify(l, &date, rev_id),
                    "revision_id": rev_id,
                    "current": format!("laws/{}/current.json", l.law_id),
                    "article_diff": diff,
                })
            })
            .collect();
        write_json_pretty(
            &updates_dir.join(format!("{}.json", date)),
            &json!({
                "date": date,
                "updated_laws": arr,
            }),
        )?;
    }
    Ok(())
}

/// `public/manifest.json` と `public/health.json` を実ファイルから再構築する公開ヘルパ。
/// `kanpo-link` のように public/ を直接書き換えた後に呼ぶ。
pub fn rebuild_manifest(public: &Path) -> Result<()> {
    // law_count は既存 laws/ ディレクトリ数で代用。
    let mut law_count = 0;
    let laws_dir = public.join("laws");
    if laws_dir.exists() {
        for e in std::fs::read_dir(&laws_dir)? {
            let e = e?;
            if e.file_type()?.is_dir() && e.path().join("current.json").exists() {
                law_count += 1;
            }
        }
    }
    let dummy = LawWithHistory {
        law_id: String::new(),
        revisions: Vec::new(),
        fetched_dates: BTreeMap::new(),
        meta_revisions: Vec::new(),
        meta_law_info: None,
    };
    let stub = vec![dummy; law_count];
    write_manifest_and_health(public, &stub)
}

/// `public/` 配下の全ファイル (manifest.json / health.json を除く) を walk して
/// manifest の files[] エントリ (path / sha256 / bytes / content_type) を作る。
/// パス昇順にソート済み。manifest 生成と manifest 単独再生成の両方で使う。
fn collect_manifest_files(public: &Path) -> Result<Vec<serde_json::Value>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(public).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let rel = path
            .strip_prefix(public)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if rel == "manifest.json" || rel == "health.json" {
            continue;
        }
        let bytes = std::fs::read(path)?;
        let content_type = match path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "json" => "application/json",
            "ndjson" => "application/x-ndjson",
            "xml" => "application/xml",
            "txt" => "text/plain",
            "db" | "sqlite" | "sqlite3" => "application/vnd.sqlite3",
            _ => "application/octet-stream",
        };
        files.push(json!({
            "path": rel,
            "sha256": sha256_hex(&bytes),
            "bytes": bytes.len(),
            "content_type": content_type,
        }));
    }
    files.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    Ok(files)
}

/// `manifest.json` の files[] だけをディスク内容から再計算して書き直す。
/// `health.json` や index/laws 系は触らない。
/// prebuilt 履歴束を上書きした後に validate を通すための軽量再生成。
pub fn run_rebuild_manifest(public: &Path) -> Result<()> {
    let files = collect_manifest_files(public)?;
    let file_count = files.len();
    write_json_pretty(
        &public.join("manifest.json"),
        &json!({
            "version": SCHEMA_VERSION,
            "generated_at": Utc::now().to_rfc3339(),
            "files": files,
        }),
    )?;
    tracing::info!("rebuilt manifest.json ({} files)", file_count);
    Ok(())
}

/// 履歴束 (.zst) を復号して NDJSON の各行 (空行除く) を返す。ファイルが無ければ空。
fn read_history_bundle_lines(path: &Path) -> Result<Vec<String>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let bytes = std::fs::read(path)?;
    let raw =
        zstd::decode_all(&bytes[..]).with_context(|| format!("zstd decode {}", path.display()))?;
    let text = String::from_utf8(raw).with_context(|| format!("utf8 {}", path.display()))?;
    Ok(text
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect())
}

/// NDJSON 1 行 (= 1 版の LawDocument) から revision_id を取り出す。
fn revision_id_of_line(line: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(line)
        .ok()
        .and_then(|v| {
            v.get("revision_id")
                .and_then(|r| r.as_str())
                .map(|s| s.to_string())
        })
}

/// `public` の (CI が cache から作った) 履歴束に `prebuilt` の履歴束をマージする。
///
/// 法令ごとに revision_id で union (dedup) する。過去の版は prebuilt 側が、
/// 新しい版は CI 側が持つので、32GB の revision キャッシュを CI に置かずに
/// 履歴束を最新化できる (差分更新)。同一 revision_id が両方にある場合は
/// **public (CI) 側を採用**する — CI 側は新しい current を踏まえて各版の
/// status (current/historical) を計算し直しているため。
/// 出力は revision_id 昇順 (BTreeMap) なので、同じ入力なら同じ束 = 冪等。
pub fn run_merge_history(public: &Path, prebuilt: &Path) -> Result<()> {
    use std::collections::BTreeSet;
    let pub_laws = public.join("laws");
    let pre_laws = prebuilt.join("laws");

    let mut law_ids: BTreeSet<String> = BTreeSet::new();
    for base in [&pub_laws, &pre_laws] {
        if base.exists() {
            for e in std::fs::read_dir(base)? {
                let e = e?;
                if e.file_type()?.is_dir() {
                    law_ids.insert(e.file_name().to_string_lossy().to_string());
                }
            }
        }
    }

    let mut unioned = 0usize;
    let mut changed = 0usize;
    for id in &law_ids {
        let pub_bundle = pub_laws.join(id).join("history.ndjson.zst");
        let pre_bundle = pre_laws.join(id).join("history.ndjson.zst");
        let pre_lines = read_history_bundle_lines(&pre_bundle)?;
        let pub_lines = read_history_bundle_lines(&pub_bundle)?;
        if pre_lines.is_empty() && pub_lines.is_empty() {
            continue;
        }
        // prebuilt を先に、public を後に入れる。同一 revision_id は後勝ち = public 優先。
        // revision_id が無い行は衝突させないよう一意キーで保持する。
        let mut by_rev: BTreeMap<String, String> = BTreeMap::new();
        let mut anon = 0usize;
        for line in &pre_lines {
            let key = revision_id_of_line(line).unwrap_or_else(|| {
                anon += 1;
                format!("\u{0}pre{anon}")
            });
            by_rev.insert(key, line.clone());
        }
        for line in &pub_lines {
            let key = revision_id_of_line(line).unwrap_or_else(|| {
                anon += 1;
                format!("\u{0}pub{anon}")
            });
            by_rev.insert(key, line.clone());
        }

        let mut ndjson: Vec<u8> = Vec::new();
        for line in by_rev.values() {
            ndjson.extend_from_slice(line.as_bytes());
            ndjson.push(b'\n');
        }
        let compressed = zstd_long(&ndjson)?;

        // 既存と同一バイトなら書かない (冪等・無駄な mtime 更新を避ける)。
        let existing = std::fs::read(&pub_bundle).ok();
        if existing.as_deref() != Some(compressed.as_slice()) {
            if let Some(parent) = pub_bundle.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&pub_bundle, &compressed)?;
            changed += 1;
        }
        unioned += 1;
    }
    tracing::info!(
        "merge-history: {} laws unioned ({} bundles changed)",
        unioned,
        changed
    );
    Ok(())
}

fn write_manifest_and_health(public: &Path, laws: &[LawWithHistory]) -> Result<()> {
    let generated_at = Utc::now().to_rfc3339();

    let files = collect_manifest_files(public)?;

    let file_count = files.len();
    write_json_pretty(
        &public.join("manifest.json"),
        &json!({
            "version": SCHEMA_VERSION,
            "generated_at": generated_at,
            "files": files,
        }),
    )?;

    let latest_date = laws
        .iter()
        .flat_map(|l| l.fetched_dates.keys())
        .max()
        .cloned()
        .unwrap_or_default();
    write_json_pretty(
        &public.join("health.json"),
        &json!({
            "ok": true,
            "generated_at": generated_at,
            "latest_egov_update_date": if latest_date.is_empty() { Utc::now().date_naive().format("%Y-%m-%d").to_string() } else { latest_date },
            "law_count": laws.len(),
            "file_count": file_count,
            "errors": [],
        }),
    )?;

    Ok(())
}

/// Sitemap + robots.txt + 法令一覧の NDJSON を出力する。
/// SSG 配信なので、検索エンジンが各 `/#/laws/:id` にアクセスしても HashRouter が
/// 同じ index.html を返す。それでも sitemap には法令詳細 URL を載せて検索回遊性を
/// 担保する (実体は SPA だが OG 情報は同一の index.html がメタを返す)。
fn write_search_db(public: &Path, laws: &[LawWithHistory]) -> Result<()> {
    // 現行版だけを索引対象にする。履歴 rev は法令本文との突き合わせが必要になったら検討。
    let docs: Vec<LawDocument> = laws.iter().map(|l| l.current().clone()).collect();
    // law_id → e-Gov 法令分類。v2 meta の最新 revision の category を採用する。
    let categories: BTreeMap<String, String> = laws
        .iter()
        .filter_map(|l| {
            l.meta_revisions
                .last()
                .and_then(|m| m.category.clone())
                .map(|c| (l.law_id.clone(), c))
        })
        .collect();
    let categories: std::collections::HashMap<String, String> = categories.into_iter().collect();
    let path = public.join("search.db");
    let proc_dir = public.join("proceedings");
    let proc_dir_opt = if proc_dir.is_dir() { Some(proc_dir.as_path()) } else { None };
    search_index::build_search_db(&path, &docs, &categories, proc_dir_opt)?;
    Ok(())
}

fn write_seo(public: &Path, laws: &[LawWithHistory]) -> Result<()> {
    let base = std::env::var("LAWPUB_BASE_URL").unwrap_or_else(|_| "/".to_string());
    let base_norm = if base.ends_with('/') {
        base.clone()
    } else {
        format!("{}/", base)
    };
    let now = Utc::now().date_naive().format("%Y-%m-%d").to_string();

    let mut urls = vec![
        ("".to_string(), now.clone(), "1.0"),
        ("#/search".to_string(), now.clone(), "0.8"),
        ("#/laws".to_string(), now.clone(), "0.8"),
        ("#/updates".to_string(), now.clone(), "0.7"),
        ("#/kanpo".to_string(), now.clone(), "0.6"),
    ];
    for l in laws {
        urls.push((format!("#/laws/{}", l.law_id), now.clone(), "0.6"));
    }

    let mut sitemap = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
"#,
    );
    for (suffix, lastmod, prio) in &urls {
        sitemap.push_str("  <url>\n");
        sitemap.push_str(&format!(
            "    <loc>{}{}</loc>\n",
            xml_escape(&base_norm),
            xml_escape(suffix)
        ));
        sitemap.push_str(&format!("    <lastmod>{}</lastmod>\n", lastmod));
        sitemap.push_str(&format!("    <priority>{}</priority>\n", prio));
        sitemap.push_str("  </url>\n");
    }
    sitemap.push_str("</urlset>\n");
    std::fs::write(public.join("sitemap.xml"), sitemap.as_bytes())?;

    let robots = format!(
        "User-agent: *\nAllow: /\nSitemap: {}sitemap.xml\n",
        base_norm
    );
    std::fs::write(public.join("robots.txt"), robots.as_bytes())?;

    // NDJSON bulk export: 1 行 1 LawSummary、API 利用者がストリーム消費しやすい。
    let ndjson_dir = public.join("laws");
    std::fs::create_dir_all(&ndjson_dir)?;
    let mut ndjson = String::new();
    for l in laws {
        let d = l.current();
        let line = serde_json::json!({
            "law_id": l.law_id,
            "law_num": d.law_num,
            "title": d.title,
            "current": format!("laws/{}/current.json", l.law_id),
            "revision_id": l.current_rev().revision_id,
        });
        ndjson.push_str(&serde_json::to_string(&line)?);
        ndjson.push('\n');
    }
    std::fs::write(ndjson_dir.join("all.ndjson"), ndjson.as_bytes())?;

    Ok(())
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn write_schema(public: &Path) -> Result<()> {
    // Hand-written JSON Schema (draft 2020-12) for the most-consumed endpoints.
    // We keep them static rather than deriving from the Rust types because
    // upstream consumers want stable, human-readable contracts.
    let dir = public.join("schema");
    std::fs::create_dir_all(&dir)?;

    let law_document = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "schema/law-document.json",
        "title": "LawDocument",
        "type": "object",
        "required": ["schema_version", "law_id", "title", "status", "articles", "source"],
        "properties": {
            "schema_version": { "type": "integer", "minimum": 1 },
            "law_id":         { "type": "string" },
            "law_num":        { "type": ["string", "null"] },
            "title":          { "type": "string" },
            "revision_id":    { "type": ["string", "null"] },
            "promulgation_date": { "type": ["string", "null"], "pattern": "^[0-9]{4}-[0-9]{2}-[0-9]{2}$" },
            "effective_date":    { "type": ["string", "null"], "pattern": "^[0-9]{4}-[0-9]{2}-[0-9]{2}$" },
            "status": { "type": "string", "enum": ["current", "historical", "future"] },
            "articles": {
                "type": "array",
                "items": { "$ref": "#/$defs/article" }
            },
            "source": { "$ref": "#/$defs/source" }
        },
        "$defs": {
            "article": {
                "type": "object",
                "required": ["article_id", "article_no", "paragraphs"],
                "properties": {
                    "article_id": { "type": "string" },
                    "article_no": { "type": "string" },
                    "caption":    { "type": ["string", "null"] },
                    "paragraphs": {
                        "type": "array",
                        "items": { "$ref": "#/$defs/paragraph" }
                    }
                }
            },
            "paragraph": {
                "type": "object",
                "required": ["text"],
                "properties": {
                    "paragraph_no": { "type": ["string", "null"] },
                    "text":         { "type": "string" }
                }
            },
            "source": {
                "type": "object",
                "required": ["provider", "fetched_at"],
                "properties": {
                    "provider":        { "type": "string" },
                    "raw_xml_sha256":  { "type": ["string", "null"] },
                    "fetched_at":      { "type": "string", "format": "date-time" }
                }
            }
        }
    });

    let manifest = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "schema/manifest.json",
        "title": "Manifest",
        "type": "object",
        "required": ["version", "generated_at", "files"],
        "properties": {
            "version":      { "type": "integer", "minimum": 1 },
            "generated_at": { "type": "string", "format": "date-time" },
            "files": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["path", "sha256", "bytes", "content_type"],
                    "properties": {
                        "path":         { "type": "string" },
                        "sha256":       { "type": "string", "pattern": "^[0-9a-f]{64}$" },
                        "bytes":        { "type": "integer", "minimum": 0 },
                        "content_type": { "type": "string" }
                    }
                }
            }
        }
    });

    let updates = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "schema/updates.json",
        "title": "UpdatesByDate",
        "type": "object",
        "required": ["date", "updated_laws"],
        "properties": {
            "date": { "type": "string", "pattern": "^[0-9]{4}-[0-9]{2}-[0-9]{2}$" },
            "updated_laws": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["law_id", "title", "change_type", "current"],
                    "properties": {
                        "law_id":      { "type": "string" },
                        "title":       { "type": "string" },
                        "change_type": { "type": "string", "enum": ["added", "modified", "removed"] },
                        "revision_id": { "type": ["string", "null"] },
                        "current":     { "type": "string" },
                        "article_diff": {
                            "type": "object",
                            "required": ["added", "removed", "modified"],
                            "properties": {
                                "added":    { "type": "array", "items": { "type": "string" } },
                                "removed":  { "type": "array", "items": { "type": "string" } },
                                "modified": { "type": "array", "items": { "type": "string" } }
                            }
                        }
                    }
                }
            }
        }
    });

    write_json_pretty(&dir.join("law-document.json"), &law_document)?;
    write_json_pretty(&dir.join("manifest.json"), &manifest)?;
    write_json_pretty(&dir.join("updates.json"), &updates)?;
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

/// NDJSON を zstd(level 19, long-distance matching) で圧縮する。履歴束 (history.ndjson.zst) 用。
/// window は内容サイズに合わせて 23..=27 にクランプし、版間 dedup を効かせつつ
/// 復号側 (ブラウザの fzstd 等) の確保メモリを内容相当に抑える。
fn zstd_long(data: &[u8]) -> Result<Vec<u8>> {
    let bits = usize::BITS - data.len().max(1).leading_zeros();
    let window_log = bits.clamp(23, 27);
    let mut enc = zstd::stream::write::Encoder::new(Vec::new(), 19)?;
    enc.long_distance_matching(true)?;
    enc.window_log(window_log)?;
    enc.write_all(data)?;
    Ok(enc.finish()?)
}

#[cfg(test)]
mod history_bundle_tests {
    use super::zstd_long;
    use std::io::Read;

    #[test]
    fn zstd_long_roundtrips_and_dedups_repeats() {
        // 同一行を多数並べた NDJSON は大窓 zstd で激しく縮む (版間 dedup の代理)。
        let line = format!("{}\n", "{\"a\":\"".to_string() + &"x".repeat(2000) + "\"}");
        let data = line.repeat(50).into_bytes();
        let z = zstd_long(&data).unwrap();
        assert!(
            z.len() * 10 < data.len(),
            "repeated content should compress >10x: {} -> {}",
            data.len(),
            z.len()
        );
        let mut dec = zstd::stream::read::Decoder::new(&z[..]).unwrap();
        let mut out = Vec::new();
        dec.read_to_end(&mut out).unwrap();
        assert_eq!(out, data, "zstd round-trip must be lossless");
    }

    #[test]
    fn merge_history_unions_by_revision_id_public_wins() {
        use std::path::PathBuf;
        let base = std::env::temp_dir().join(format!("lawpub_mh_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let pre = base.join("pre/laws/X");
        let pubd = base.join("pub/laws/X");
        std::fs::create_dir_all(&pre).unwrap();
        std::fs::create_dir_all(&pubd).unwrap();
        let write = |dir: &PathBuf, lines: &[&str]| {
            let nd = lines.join("\n") + "\n";
            std::fs::write(
                dir.join("history.ndjson.zst"),
                super::zstd_long(nd.as_bytes()).unwrap(),
            )
            .unwrap();
        };
        // prebuilt: r1(historical) + r2(当時 current)
        write(
            &pre,
            &[
                r#"{"revision_id":"r1","status":"historical"}"#,
                r#"{"revision_id":"r2","status":"current"}"#,
            ],
        );
        // public(CI): r2(now historical) + r3(新 current)
        write(
            &pubd,
            &[
                r#"{"revision_id":"r2","status":"historical"}"#,
                r#"{"revision_id":"r3","status":"current"}"#,
            ],
        );

        super::run_merge_history(&base.join("pub"), &base.join("pre")).unwrap();

        let lines = super::read_history_bundle_lines(&pubd.join("history.ndjson.zst")).unwrap();
        let ids: Vec<String> = lines
            .iter()
            .map(|l| super::revision_id_of_line(l).unwrap())
            .collect();
        // union = 3 版, revision_id 昇順, 衝突 r2 は public 側 (historical) が勝つ。
        assert_eq!(ids, vec!["r1", "r2", "r3"]);
        assert!(
            lines[1].contains("\"status\":\"historical\""),
            "r2 should take public's status: {}",
            lines[1]
        );

        // 冪等: もう一度マージしてもバイト不変。
        let before = std::fs::read(pubd.join("history.ndjson.zst")).unwrap();
        super::run_merge_history(&base.join("pub"), &base.join("pre")).unwrap();
        let after = std::fs::read(pubd.join("history.ndjson.zst")).unwrap();
        assert_eq!(before, after, "merge-history must be idempotent");

        let _ = std::fs::remove_dir_all(&base);
    }
}
