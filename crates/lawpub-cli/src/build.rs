use anyhow::{Context, Result};
use chrono::Utc;
use egov_client::{EgovProvider, FetchedLaw, MockProvider};
use law_normalizer::{parse_law_xml, sha256_hex, LawDocument, LawSummary};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
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

pub fn run_fetch_update(date: &str, cache: &Path, provider: &str) -> Result<usize> {
    let p = provider_by_name(provider)?;
    let batch = p.fetch_update(date)?;
    let new_xmls = write_cache_batch(cache, &batch.date, &batch.laws)?;
    tracing::info!(
        "date={date}: provider returned {} laws, {new_xmls} new XML(s) cached",
        batch.laws.len()
    );
    Ok(new_xmls)
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
    revisions: Vec<Revision>,
    /// このビルドで「新しく取得された」日 → 直近の change_type 推定に使う。
    fetched_dates: BTreeMap<String, String>, // date -> revision_id
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
        if idx == 0 { None } else { self.revisions.get(idx - 1) }
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
    let prev_map: BTreeMap<&str, &law_normalizer::Article> =
        prev.articles.iter().map(|a| (a.article_id.as_str(), a)).collect();
    let cur_map: BTreeMap<&str, &law_normalizer::Article> =
        cur.articles.iter().map(|a| (a.article_id.as_str(), a)).collect();
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
    ArticleDiff { added, removed, modified }
}

pub fn run_build_json(input: &Path, output: &Path) -> Result<()> {
    let laws = collect_laws_with_history(input)?;
    if laws.is_empty() {
        anyhow::bail!(
            "no law XML found under {} — run `lawpub fetch-update` first",
            input.display()
        );
    }
    build_into(output, &laws)
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
    let changed = force || new_xmls > 0 || !public_complete;
    if changed {
        run_build_json(cache, public)?;
    } else {
        tracing::info!(
            "no new revisions and public/ is intact — skipping rebuild ({} dates checked)",
            dates.len()
        );
    }

    if let Some(last) = dates.last() {
        st.latest_successful_update_date = Some(last.clone());
    }
    st.last_run_at = Some(state::now_iso());
    st.last_run_status = Some(if errors.is_empty() { "ok".into() } else { "partial".into() });
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
                    let v: serde_json::Value =
                        serde_json::from_slice(&std::fs::read(&meta_path)?)?;
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
                let entry = by_law.entry(law_id.clone()).or_insert_with(|| LawWithHistory {
                    law_id: law_id.clone(),
                    revisions: Vec::new(),
                    fetched_dates: BTreeMap::new(),
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
        let doc: LawDocument = serde_json::from_slice(&bytes)
            .with_context(|| format!("parse {}", cur.display()))?;
        out.push(doc);
    }
    Ok(out)
}

fn build_into(public: &Path, laws: &[LawWithHistory]) -> Result<()> {
    let tmp = public.with_file_name(format!(
        "{}.tmp",
        public.file_name().and_then(|s| s.to_str()).unwrap_or("public")
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
        public.file_name().and_then(|s| s.to_str()).unwrap_or("public")
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
        let dir = public.join("laws").join(&law.law_id);
        std::fs::create_dir_all(&dir)?;

        // current.json は最新版に revision_id を埋める。
        let cur_rev = law.current_rev();
        let mut current_doc = cur_rev.doc.clone();
        current_doc.revision_id = Some(cur_rev.revision_id.clone());
        write_json_pretty(&dir.join("current.json"), &current_doc)?;

        let articles_dir = dir.join("articles");
        std::fs::create_dir_all(&articles_dir)?;
        for a in &current_doc.articles {
            write_json_pretty(&articles_dir.join(format!("{}.json", a.article_id)), a)?;
        }

        // 過去 revision を全部書き出す (Phase 2 §7.6)。
        let revisions_dir = dir.join("revisions");
        std::fs::create_dir_all(&revisions_dir)?;
        for r in &law.revisions {
            let mut doc = r.doc.clone();
            doc.revision_id = Some(r.revision_id.clone());
            doc.status = if r.revision_id == cur_rev.revision_id {
                "current".to_string()
            } else {
                "historical".to_string()
            };
            write_json_pretty(
                &revisions_dir.join(format!("{}.json", r.revision_id)),
                &doc,
            )?;
        }

        let versions: Vec<_> = law
            .revisions
            .iter()
            .map(|r| {
                json!({
                    "revision_id": r.revision_id,
                    "effective_date": r.doc.effective_date,
                    "promulgation_date": r.doc.promulgation_date,
                    "path": format!("laws/{}/revisions/{}.json", law.law_id, r.revision_id),
                    "source_update_date": if r.first_seen_date.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(r.first_seen_date.clone()) },
                })
            })
            .collect();
        write_json_pretty(
            &dir.join("versions.json"),
            &json!({
                "law_id": law.law_id,
                "current_revision_id": cur_rev.revision_id,
                "versions": versions,
            }),
        )?;

        // timeline は revision 遷移 1件 = イベント 1件として並べる。
        let mut events = Vec::new();
        for (i, r) in law.revisions.iter().enumerate() {
            let event_type = if i == 0 { "initial" } else { "snapshot" };
            events.push(json!({
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
            }));
        }
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

    let summaries: Vec<LawSummary> = laws
        .iter()
        .map(|l| {
            let d = l.current();
            LawSummary {
                law_id: l.law_id.clone(),
                law_num: d.law_num.clone(),
                title: d.title.clone(),
                current: format!("laws/{}/current.json", l.law_id),
                timeline: format!("laws/{}/timeline.json", l.law_id),
                versions: format!("laws/{}/versions.json", l.law_id),
            }
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
    };
    let stub = vec![dummy; law_count];
    write_manifest_and_health(public, &stub)
}

fn write_manifest_and_health(public: &Path, laws: &[LawWithHistory]) -> Result<()> {
    let generated_at = Utc::now().to_rfc3339();

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
    let path = public.join("search.db");
    search_index::build_search_db(&path, &docs)?;
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
