//! 現行版の本文と版 ID の整合を保つためのコマンド群。
//!
//! - `sync-current-bodies`: e-Gov v2 `/laws` 一覧の現行版 ID と手元の改正履歴メタ・本文 XML を
//!   突き合わせ、足りないものだけを取得する。施行日を迎えて版が切り替わった法令
//!   (日次の更新一覧に載らないことがある) や、全件取得で漏れた法令の救済に使う。
//! - `check-current-bodies`: 生成済み `public/` について、配信している本文が e-Gov の現行版か、
//!   版 ID と本文 XML が食い違っていないかを検出する。

use anyhow::{Context, Result};
use chrono::Utc;
use egov_client::{LawListEntry, LawRevisionList};
use law_normalizer::{parse_law_xml, sha256_hex};
use serde::Serialize;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use crate::build::{
    expected_current_meta, sort_meta_revisions, v2_revision_date, write_json_pretty,
};

/// e-Gov への 1 リクエストごとの待ち。並列度 (最大 8) と合わせて秒間リクエスト数を抑える。
const REQUEST_INTERVAL: Duration = Duration::from_millis(250);

fn today_jst() -> String {
    (Utc::now() + chrono::Duration::hours(9))
        .date_naive()
        .format("%Y-%m-%d")
        .to_string()
}

/// e-Gov v2 `/law_data/{revision_id}` の本文を `{cache}/revisions/{law_id}/{revision_id}.xml` に保存する。
/// パースできない応答 (HTML のエラーページ等) は保存しない。
pub(crate) fn store_revision_body(
    cache: &Path,
    law_id: &str,
    revision_id: &str,
    bytes: &[u8],
    fetched_via: &str,
) -> Result<()> {
    parse_law_xml(bytes, law_id).with_context(|| format!("parse law_data {revision_id}"))?;
    let dir = cache.join("revisions").join(law_id);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(format!("{revision_id}.xml")), bytes)?;
    let meta = json!({
        "law_id": law_id,
        "revision_id": revision_id,
        "first_seen_date": today_jst(),
        "sha256": sha256_hex(bytes),
        "source_url": format!(
            "https://laws.e-gov.go.jp/api/2/law_data/{revision_id}?response_format=xml"
        ),
        "bytes": bytes.len(),
        "fetched_via": fetched_via,
    });
    std::fs::write(
        dir.join(format!("{revision_id}.meta.json")),
        serde_json::to_vec_pretty(&meta)?,
    )?;
    Ok(())
}

/// 改正履歴メタ上の現行版の本文が `cache` に無ければ取得する。取得したら `true`。
/// 日次更新 (`fetch-update`) でメタを更新した法令について、版 ID と本文を揃えるために使う。
pub(crate) fn fetch_current_body_if_missing(
    provider: &egov_client::HttpProvider,
    cache: &Path,
    law_id: &str,
    list: &LawRevisionList,
) -> Result<bool> {
    let mut revisions = list.revisions.clone();
    sort_meta_revisions(&mut revisions);
    let Some(current) = expected_current_meta(&revisions) else {
        return Ok(false);
    };
    let rev_id = &current.law_revision_id;
    if rev_id.is_empty()
        || cache
            .join("revisions")
            .join(law_id)
            .join(format!("{rev_id}.xml"))
            .exists()
    {
        return Ok(false);
    }
    let bytes = provider.fetch_revision_body(rev_id)?;
    store_revision_body(cache, law_id, rev_id, &bytes, "fetch_update")?;
    Ok(true)
}

/// 手元 (書き込み先 cache + 参照用 cache + 既存ファイル一覧) の状態。
struct LocalCorpus<'a> {
    cache: &'a Path,
    lookup: &'a [PathBuf],
    /// `{law_id}/{file}` 形式の既存ファイル (R2 の base corpus を tar -t した一覧など)。
    listed_files: HashSet<String>,
    listed_laws: HashSet<String>,
}

impl LocalCorpus<'_> {
    fn roots(&self) -> impl Iterator<Item = &Path> {
        std::iter::once(self.cache).chain(self.lookup.iter().map(PathBuf::as_path))
    }

    /// 書き込み先 cache を優先して改正履歴メタを読む (delta が base より新しい)。
    fn meta(&self, law_id: &str) -> Option<LawRevisionList> {
        self.roots().find_map(|root| {
            let p = root.join("revisions_meta").join(format!("{law_id}.json"));
            let bytes = std::fs::read(p).ok()?;
            serde_json::from_slice::<LawRevisionList>(&bytes).ok()
        })
    }

    fn has_body(&self, law_id: &str, revision_id: &str) -> bool {
        let file = format!("{revision_id}.xml");
        self.listed_files.contains(&format!("{law_id}/{file}"))
            || self
                .roots()
                .any(|root| root.join("revisions").join(law_id).join(&file).is_file())
    }

    fn knows_law(&self, law_id: &str) -> bool {
        self.listed_laws.contains(law_id)
            || self.roots().any(|root| {
                root.join("revisions").join(law_id).is_dir()
                    || root
                        .join("revisions_meta")
                        .join(format!("{law_id}.json"))
                        .is_file()
            })
    }
}

/// `tar -t` 等の出力 (`./LAW/REV.xml`, `revisions/LAW/REV.xml`, `LAW/REV.xml`) を
/// `LAW/REV.xml` に正規化して読む。
fn read_file_list(path: &Path) -> Result<(HashSet<String>, HashSet<String>)> {
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut files = HashSet::new();
    let mut laws = HashSet::new();
    for line in text.lines() {
        let line = line.trim().trim_start_matches("./");
        let line = line.strip_prefix("revisions/").unwrap_or(line);
        let mut parts = line.splitn(2, '/');
        let (Some(law), Some(file)) = (parts.next(), parts.next()) else {
            continue;
        };
        if law.is_empty() || law.starts_with('.') {
            continue;
        }
        laws.insert(law.to_string());
        if file.ends_with(".xml") && !file.contains('/') {
            files.insert(format!("{law}/{file}"));
        }
    }
    Ok((files, laws))
}

/// 1 法令分の取得計画。
#[derive(Debug, Clone, PartialEq, Serialize)]
struct SyncJob {
    law_id: String,
    revision_id: String,
    fetch_meta: bool,
    fetch_body: bool,
    new_law: bool,
}

/// e-Gov 一覧の 1 法令と手元の状態から、取るべきもの (メタ / 本文) を決める。
/// 手元に無い法令は、現行 (repeal_status = None) のものだけを新規に加える。
fn plan_sync(
    entry: &LawListEntry,
    meta: Option<&LawRevisionList>,
    known: bool,
    has_body: bool,
    include_repealed: bool,
) -> Option<SyncJob> {
    let new_law = !known;
    if new_law && !include_repealed && entry.repeal_status.as_deref() != Some("None") {
        return None;
    }
    // メタを取り直すのは、一覧の現行版が手元のメタに無いか、その版の状態が一覧と違う
    // (施行日を迎えて UnEnforced → CurrentEnforced に切り替わった等) ときだけ。
    // e-Gov 側で現行版が PreviousEnforced のまま残る法令でも毎回取り直さない。
    let fetch_meta = match meta {
        None => true,
        Some(list) => !list.revisions.iter().any(|m| {
            m.law_revision_id == entry.current_revision_id
                && m.current_revision_status == entry.current_revision_status
        }),
    };
    let fetch_body = !has_body;
    (fetch_meta || fetch_body).then(|| SyncJob {
        law_id: entry.law_id.clone(),
        revision_id: entry.current_revision_id.clone(),
        fetch_meta,
        fetch_body,
        new_law,
    })
}

pub struct SyncOptions<'a> {
    /// 改正履歴メタと本文 XML の書き込み先。
    pub cache: &'a Path,
    /// 既存のメタ・本文を探す追加の cache (書き込みはしない)。
    pub lookup: &'a [PathBuf],
    /// 既存本文のファイル一覧 (`tar -t` の出力など)。
    pub existing_list: Option<&'a Path>,
    /// 対象を絞る法令 ID (空なら一覧の全法令)。
    pub law_ids: &'a [String],
    /// 手元に無い廃止・失効法令も新規に加える。
    pub include_repealed: bool,
    pub concurrency: usize,
    /// 取得する法令数の上限 (スモークテスト用)。
    pub limit: Option<usize>,
    /// 取得せず、何が足りないかだけを報告する。
    pub dry_run: bool,
    pub report: Option<&'a Path>,
}

pub fn run_sync_current_bodies(opts: &SyncOptions) -> Result<()> {
    let (listed_files, listed_laws) = match opts.existing_list {
        Some(p) => read_file_list(p)?,
        None => Default::default(),
    };
    let local = LocalCorpus {
        cache: opts.cache,
        lookup: opts.lookup,
        listed_files,
        listed_laws,
    };
    let provider = crate::build::http_provider_v2();
    let mut listing = provider.list_laws().context("list e-Gov laws (v2 /laws)")?;
    tracing::info!("sync-current-bodies: e-Gov lists {} laws", listing.len());
    if !opts.law_ids.is_empty() {
        let wanted: BTreeSet<&str> = opts.law_ids.iter().map(String::as_str).collect();
        listing.retain(|e| wanted.contains(e.law_id.as_str()));
        for id in &wanted {
            if !listing.iter().any(|e| e.law_id == *id) {
                tracing::warn!("sync-current-bodies: {id} is not in the e-Gov law list");
            }
        }
    }

    let mut jobs: Vec<SyncJob> = listing
        .iter()
        .filter_map(|e| {
            let meta = local.meta(&e.law_id);
            plan_sync(
                e,
                meta.as_ref(),
                meta.is_some() || local.knows_law(&e.law_id),
                local.has_body(&e.law_id, &e.current_revision_id),
                opts.include_repealed,
            )
        })
        .collect();
    let planned = jobs.len();
    if let Some(n) = opts.limit {
        jobs.truncate(n);
    }
    tracing::info!(
        "sync-current-bodies: {} law(s) need work (meta stale/missing: {}, body missing: {}, new: {}){}",
        planned,
        jobs.iter().filter(|j| j.fetch_meta).count(),
        jobs.iter().filter(|j| j.fetch_body).count(),
        jobs.iter().filter(|j| j.new_law).count(),
        if opts.dry_run { " [dry-run]" } else { "" }
    );

    let errors: Mutex<Vec<serde_json::Value>> = Mutex::new(Vec::new());
    let done: Mutex<Vec<&SyncJob>> = Mutex::new(Vec::new());
    if !opts.dry_run && !jobs.is_empty() {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(opts.concurrency.clamp(1, 8))
            .build()
            .context("rayon pool")?;
        let counter = std::sync::atomic::AtomicUsize::new(0);
        let total = jobs.len();
        pool.install(|| {
            use rayon::prelude::*;
            jobs.par_iter().for_each(|job| {
                match sync_one(&provider, opts.cache, job) {
                    Ok(()) => done.lock().unwrap().push(job),
                    Err(e) => errors.lock().unwrap().push(json!({
                        "law_id": job.law_id,
                        "revision_id": job.revision_id,
                        "error": format!("{e:#}"),
                    })),
                }
                let n = counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                if n.is_multiple_of(100) || n == total {
                    tracing::info!("sync-current-bodies: {n}/{total}");
                }
            });
        });
    }

    let errors = errors.into_inner().unwrap();
    let mut done = done.into_inner().unwrap();
    done.sort_by(|a, b| a.law_id.cmp(&b.law_id));
    let ids = |pred: &dyn Fn(&SyncJob) -> bool| -> Vec<&str> {
        let src: Vec<&SyncJob> = if opts.dry_run {
            jobs.iter().collect()
        } else {
            done.clone()
        };
        src.into_iter()
            .filter(|j| pred(j))
            .map(|j| j.law_id.as_str())
            .collect()
    };
    let report = json!({
        "generated_at": Utc::now().to_rfc3339(),
        "dry_run": opts.dry_run,
        "listed_laws": listing.len(),
        "planned": planned,
        "processed": if opts.dry_run { 0 } else { jobs.len() },
        "meta_refreshed": ids(&|j| j.fetch_meta),
        "bodies_fetched": ids(&|j| j.fetch_body),
        "new_laws": ids(&|j| j.new_law),
        "errors": errors,
    });
    if let Some(path) = opts.report {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        write_json_pretty(path, &report)?;
    }
    if !errors.is_empty() {
        tracing::warn!("sync-current-bodies: {} error(s)", errors.len());
        for e in errors.iter().take(10) {
            tracing::warn!("  {e}");
        }
    }
    tracing::info!(
        "sync-current-bodies done: meta {} / bodies {} / new laws {} / errors {}",
        report["meta_refreshed"].as_array().map_or(0, Vec::len),
        report["bodies_fetched"].as_array().map_or(0, Vec::len),
        report["new_laws"].as_array().map_or(0, Vec::len),
        errors.len()
    );
    Ok(())
}

fn sync_one(provider: &egov_client::HttpProvider, cache: &Path, job: &SyncJob) -> Result<()> {
    if job.fetch_meta {
        let list = provider.fetch_law_revisions(&job.law_id)?;
        let dir = cache.join("revisions_meta");
        std::fs::create_dir_all(&dir)?;
        std::fs::write(
            dir.join(format!("{}.json", job.law_id)),
            serde_json::to_vec_pretty(&list)?,
        )?;
        std::thread::sleep(REQUEST_INTERVAL);
    }
    if job.fetch_body {
        let bytes = provider.fetch_revision_body(&job.revision_id)?;
        store_revision_body(
            cache,
            &job.law_id,
            &job.revision_id,
            &bytes,
            "sync_current_bodies",
        )?;
        std::thread::sleep(REQUEST_INTERVAL);
    }
    Ok(())
}

/// `path` か `path.gz` (配信物は gzip 事前圧縮される) を JSON として読む。
fn read_json_maybe_gz(path: &Path) -> Option<serde_json::Value> {
    if let Ok(bytes) = std::fs::read(path) {
        return serde_json::from_slice(&bytes).ok();
    }
    let mut gz = path.as_os_str().to_owned();
    gz.push(".gz");
    let file = std::fs::File::open(PathBuf::from(gz)).ok()?;
    serde_json::from_reader(flate2::read::GzDecoder::new(file)).ok()
}

/// 1 法令の検査結果。
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub(crate) struct LawCheck {
    pub law_id: String,
    pub revision_id: String,
    /// e-Gov の現行版 ID (versions.json の CurrentEnforced / Repeal)。
    pub expected_revision_id: Option<String>,
    /// 配信本文が e-Gov の現行版より古い (現行版の本文 XML が手元に無かった)。
    pub stale: bool,
    /// 条も別表も 0 件。
    pub empty_body: bool,
    /// 版 ID と本文が食い違っている: `{revision_id}.xml` の sha が配信本文と違う、
    /// または配信本文が別の版の XML と同じ sha。
    pub body_mismatch: bool,
    /// 配信本文と同じ sha の XML が手元に無く、版 ID と本文の対応を確かめられない。
    pub body_unverified: bool,
    /// 配信本文と同じ sha を持つ v2 版の XML (= 本文の本当の版) が分かればその ID。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_revision_id: Option<String>,
}

fn expected_from_versions(versions: &serde_json::Value) -> Option<String> {
    let list = versions["versions"].as_array()?;
    let with_status = |s: &str| {
        list.iter()
            .rev()
            .find(|v| v["current_revision_status"].as_str() == Some(s))
            .and_then(|v| v["revision_id"].as_str())
            .map(str::to_string)
    };
    with_status("CurrentEnforced").or_else(|| with_status("Repeal"))
}

fn check_law(law_dir: &Path, law_id: &str, cache: Option<&Path>) -> Option<LawCheck> {
    let current = read_json_maybe_gz(&law_dir.join("current.json"))?;
    let revision_id = current["revision_id"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let sha = current["source"]["raw_xml_sha256"]
        .as_str()
        .unwrap_or_default();
    let is_empty = |k: &str| current[k].as_array().is_none_or(Vec::is_empty);
    let expected = read_json_maybe_gz(&law_dir.join("versions.json"))
        .as_ref()
        .and_then(expected_from_versions);
    let mut check = LawCheck {
        law_id: law_id.to_string(),
        stale: expected.as_ref().is_some_and(|e| *e != revision_id),
        expected_revision_id: expected,
        empty_body: is_empty("articles") && is_empty("appendix_tables"),
        revision_id,
        ..Default::default()
    };
    if let Some(cache) = cache {
        let dir = cache.join("revisions").join(law_id);
        let named = dir.join(format!("{}.xml", check.revision_id));
        match std::fs::read(&named) {
            Ok(bytes) => check.body_mismatch = sha256_hex(&bytes) != sha,
            Err(_) => {
                // `{revision_id}.xml` が無いのは、sha 由来の本文 (v1 取得) を現行版 ID に
                // 付け替えた場合だけ正当。同じ sha の XML が別の v2 版のものなら、
                // その版の本文に別の版 ID が付いている (旧不具合の形)。
                let owner = std::fs::read_dir(&dir)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("xml"))
                    .find(|p| std::fs::read(p).is_ok_and(|b| sha256_hex(&b) == sha))
                    .and_then(|p| p.file_stem().and_then(|s| s.to_str()).map(str::to_string));
                match owner {
                    Some(stem) if v2_revision_date(law_id, &stem).is_some() => {
                        check.body_mismatch = true;
                        check.body_revision_id = Some(stem);
                    }
                    Some(_) => {}
                    None => check.body_unverified = true,
                }
            }
        }
    }
    Some(check)
}

/// `public/laws/*/` を検査して、版 ID と本文の食い違い・古い本文・空の本文を報告する。
/// `fail_on_mismatch` なら、版 ID と本文が食い違う法令が 1 件でもあればエラーにする。
pub fn run_check_current_bodies(
    public: &Path,
    cache: Option<&Path>,
    report: Option<&Path>,
    fail_on_mismatch: bool,
) -> Result<()> {
    let laws_dir = public.join("laws");
    let mut law_ids: Vec<String> = std::fs::read_dir(&laws_dir)
        .with_context(|| format!("read {}", laws_dir.display()))?
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|id| !id.starts_with('.'))
        .collect();
    law_ids.sort();

    let checks: Vec<LawCheck> = {
        use rayon::prelude::*;
        law_ids
            .par_iter()
            .filter_map(|id| check_law(&laws_dir.join(id), id, cache))
            .collect()
    };
    let pick =
        |f: fn(&LawCheck) -> bool| -> Vec<&LawCheck> { checks.iter().filter(|c| f(c)).collect() };
    let stale = pick(|c| c.stale);
    let empty = pick(|c| c.empty_body);
    let mismatch = pick(|c| c.body_mismatch);
    let unverified = pick(|c| c.body_unverified);
    let summary: BTreeMap<&str, usize> = BTreeMap::from([
        ("laws", checks.len()),
        ("stale", stale.len()),
        ("empty_body", empty.len()),
        ("body_mismatch", mismatch.len()),
        ("body_unverified", unverified.len()),
    ]);
    tracing::info!("check-current-bodies: {summary:?}");
    for c in mismatch.iter().chain(&unverified).take(20) {
        tracing::warn!(
            "  {}: revision_id={} の本文が同じ版の XML と一致しない (mismatch={}, unverified={})",
            c.law_id,
            c.revision_id,
            c.body_mismatch,
            c.body_unverified
        );
    }
    if let Some(path) = report {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        write_json_pretty(
            path,
            &json!({
                "generated_at": Utc::now().to_rfc3339(),
                "summary": summary,
                "stale": stale,
                "empty_body": empty,
                "body_mismatch": mismatch,
                "body_unverified": unverified,
            }),
        )?;
    }
    if fail_on_mismatch && !(mismatch.is_empty() && unverified.is_empty()) {
        anyhow::bail!(
            "check-current-bodies: 版 ID と本文が食い違う法令が {} 件 (mismatch {}, unverified {})",
            mismatch.len() + unverified.len(),
            mismatch.len(),
            unverified.len()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use egov_client::{LawInfoV2, RevisionMeta};

    fn meta_rev(id: &str, promulgate: &str, status: &str) -> RevisionMeta {
        RevisionMeta {
            law_revision_id: id.to_string(),
            law_title: None,
            category: None,
            amendment_promulgate_date: Some(promulgate.to_string()),
            amendment_enforcement_date: None,
            amendment_scheduled_enforcement_date: None,
            amendment_law_id: None,
            amendment_law_title: None,
            amendment_law_num: None,
            amendment_type: None,
            repeal_status: None,
            repeal_date: None,
            current_revision_status: Some(status.to_string()),
            mission: None,
            remain_in_force: None,
            amendment_enforcement_comment: None,
            updated: None,
        }
    }

    fn list(revs: Vec<RevisionMeta>) -> LawRevisionList {
        LawRevisionList {
            law_info: LawInfoV2 {
                law_id: "L".to_string(),
                law_num: None,
                law_num_era: None,
                law_num_year: None,
                law_num_type: None,
                promulgation_date: None,
            },
            revisions: revs,
        }
    }

    fn entry(cur: &str, repeal: &str) -> LawListEntry {
        LawListEntry {
            law_id: "L".to_string(),
            law_title: None,
            current_revision_id: cur.to_string(),
            current_revision_status: Some("CurrentEnforced".to_string()),
            repeal_status: Some(repeal.to_string()),
            updated: None,
        }
    }

    #[test]
    fn plan_fetches_body_and_meta_when_current_switched() {
        // 手元のメタは改正前の版を現行としている (施行日に版が切り替わった)。
        let old = list(vec![
            meta_rev("L_20220617_A", "2022-06-17", "CurrentEnforced"),
            meta_rev("L_20250601_A", "2022-06-17", "UnEnforced"),
        ]);
        let job = plan_sync(
            &entry("L_20250601_A", "None"),
            Some(&old),
            true,
            false,
            false,
        )
        .unwrap();
        assert!(job.fetch_meta && job.fetch_body && !job.new_law);

        // メタも本文も揃っていれば何もしない。
        let fresh = list(vec![
            meta_rev("L_20220617_A", "2022-06-17", "PreviousEnforced"),
            meta_rev("L_20250601_A", "2022-06-17", "CurrentEnforced"),
        ]);
        assert_eq!(
            plan_sync(
                &entry("L_20250601_A", "None"),
                Some(&fresh),
                true,
                true,
                false
            ),
            None
        );
        // メタは新しいが本文だけ無い。
        let job = plan_sync(
            &entry("L_20250601_A", "None"),
            Some(&fresh),
            true,
            false,
            false,
        )
        .unwrap();
        assert!(!job.fetch_meta && job.fetch_body);
    }

    #[test]
    fn current_falls_back_to_latest_enforced_when_egov_lacks_current_enforced() {
        // 学校教育法 (322AC0000000026) の実データの形: 施行済みの最新版が PreviousEnforced の
        // まま CurrentEnforced が無く、同じ公布日の未施行版が並ぶ。e-Gov 一覧は 20260617 を現行とする。
        let mut revs = vec![
            meta_rev("L_20270401_A", "2026-06-17", "UnEnforced"),
            meta_rev("L_20260617_A", "2026-06-17", "PreviousEnforced"),
            meta_rev("L_20261225_B", "2024-06-19", "UnEnforced"),
            meta_rev("L_20260401_C", "2025-06-01", "PreviousEnforced"),
        ];
        sort_meta_revisions(&mut revs);
        assert_eq!(
            expected_current_meta(&revs).unwrap().law_revision_id,
            "L_20260617_A"
        );
        // 一覧と状態が一致していればメタは取り直さない (毎日の再取得を防ぐ)。
        let mut e = entry("L_20260617_A", "None");
        e.current_revision_status = Some("PreviousEnforced".to_string());
        assert_eq!(plan_sync(&e, Some(&list(revs)), true, true, false), None);
    }

    #[test]
    fn plan_adds_only_in_force_laws_that_are_missing_locally() {
        let job = plan_sync(&entry("L_20260521_B", "None"), None, false, false, false).unwrap();
        assert!(job.new_law && job.fetch_meta && job.fetch_body);
        assert_eq!(
            plan_sync(&entry("L_20200101_C", "Repeal"), None, false, false, false),
            None
        );
        assert!(plan_sync(&entry("L_20200101_C", "Repeal"), None, false, false, true).is_some());
    }

    #[test]
    fn read_file_list_normalizes_tar_listing() {
        let dir = std::env::temp_dir().join(format!(
            "lawpub_filelist_{}_{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("list.txt");
        std::fs::write(
            &p,
            "./\n./A/\n./A/A_20200101_X.xml\n./A/A_20200101_X.meta.json\nrevisions/B/abc.xml\nC/C_1_2.xml\n./._junk/x.xml\n",
        )
        .unwrap();
        let (files, laws) = read_file_list(&p).unwrap();
        assert!(files.contains("A/A_20200101_X.xml"));
        assert!(files.contains("B/abc.xml"));
        assert!(files.contains("C/C_1_2.xml"));
        assert!(!files.iter().any(|f| f.ends_with(".meta.json")));
        assert_eq!(
            laws,
            HashSet::from(["A".to_string(), "B".to_string(), "C".to_string()])
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn check_flags_stale_and_mismatched_bodies() {
        let root = std::env::temp_dir().join(format!(
            "lawpub_check_bodies_{}_{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let public = root.join("public");
        let cache = root.join("cache");
        let xml_old = b"<Law>old</Law>".as_slice();
        let xml_new = b"<Law>new</Law>".as_slice();
        let old_sha = sha256_hex(xml_old);
        let new_sha = sha256_hex(xml_new);
        // (法令, 配信 revision_id, 配信本文の sha, CurrentEnforced, 手元の XML)
        type Case<'a> = (&'a str, &'a str, &'a str, &'a str, Vec<(&'a str, &'a [u8])>);
        let cases: Vec<Case> = vec![
            // 正常: 版 ID の XML と配信本文が一致し、e-Gov の現行版。
            (
                "OK",
                "OK_20250601_A",
                &new_sha,
                "OK_20250601_A",
                vec![("OK_20250601_A", xml_new)],
            ),
            // 正常: v1 で取った sha 名の本文を現行版 ID に付け替えたもの。
            (
                "REN",
                "REN_20250601_A",
                &new_sha,
                "REN_20250601_A",
                vec![("0123456789ab", xml_new)],
            ),
            // 旧不具合: 現行版の XML が無く、旧版 (20220617) の本文に現行版 ID が付いている。
            (
                "OLD",
                "OLD_20250601_A",
                &old_sha,
                "OLD_20250601_A",
                vec![("OLD_20220617_A", xml_old)],
            ),
            // 旧不具合: 現行版の XML はあるのに、配信本文は旧版のもの。
            (
                "BAD",
                "BAD_20250601_A",
                &old_sha,
                "BAD_20250601_A",
                vec![("BAD_20220617_A", xml_old), ("BAD_20250601_A", xml_new)],
            ),
            // 修正後の安全側出力: 旧版の ID と旧版の本文 (現行版の本文が未取得)。
            (
                "STALE",
                "STALE_20220617_A",
                &old_sha,
                "STALE_20250601_A",
                vec![("STALE_20220617_A", xml_old)],
            ),
            // 本文 XML が手元に無く確かめられない。
            (
                "GONE",
                "GONE_20250601_A",
                &old_sha,
                "GONE_20250601_A",
                vec![],
            ),
        ];
        for (law, rev, sha, cur, files) in &cases {
            let d = public.join("laws").join(law);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(
                d.join("current.json"),
                json!({
                    "revision_id": rev,
                    "articles": [{"article_id": "art_1"}],
                    "source": {"raw_xml_sha256": sha}
                })
                .to_string(),
            )
            .unwrap();
            std::fs::write(
                d.join("versions.json"),
                json!({"versions": [
                    {"revision_id": format!("{law}_20220617_A"), "current_revision_status": "PreviousEnforced"},
                    {"revision_id": cur, "current_revision_status": "CurrentEnforced"},
                ]})
                .to_string(),
            )
            .unwrap();
            let c = cache.join("revisions").join(law);
            std::fs::create_dir_all(&c).unwrap();
            for (name, xml) in files {
                std::fs::write(c.join(format!("{name}.xml")), xml).unwrap();
            }
        }

        let report = root.join("report.json");
        let err = run_check_current_bodies(&public, Some(&cache), Some(&report), true);
        assert!(err.is_err(), "mismatch must fail the check");
        let r: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&report).unwrap()).unwrap();
        let laws = |k: &str| -> Vec<String> {
            r[k].as_array()
                .unwrap()
                .iter()
                .map(|c| c["law_id"].as_str().unwrap().to_string())
                .collect()
        };
        assert_eq!(r["summary"]["laws"], 6);
        assert_eq!(laws("body_mismatch"), vec!["BAD", "OLD"]);
        assert_eq!(r["body_mismatch"][1]["body_revision_id"], "OLD_20220617_A");
        assert_eq!(laws("stale"), vec!["STALE"]);
        assert_eq!(r["stale"][0]["expected_revision_id"], "STALE_20250601_A");
        assert_eq!(laws("body_unverified"), vec!["GONE"]);

        // cache 無しでも stale は検出でき、mismatch では落ちない。
        run_check_current_bodies(&public, None, None, true).unwrap();
        let _ = std::fs::remove_dir_all(root);
    }
}
