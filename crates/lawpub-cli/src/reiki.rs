//! 自治体例規の収集・配信。
//!
//! ## キャッシュ（R2 `reiki-cache/` と同期）
//!
//! ```text
//! .cache/reiki/
//! ├── state.json                  # 自治体ごとの巡回状態（内容現在日・周回・公開済み時刻）
//! └── tenants/{code}.ndjson.zst   # その自治体の全例規（1 行 1 ReikiDocument）
//! ```
//!
//! 自治体単位の 1 ファイルにすることで、日次の R2 同期は巡回した自治体のファイルだけになる。
//!
//! ## 巡回（`reiki-fetch`）
//!
//! 1 実行の時間予算内で、未取得 → 中断中 → 内容現在日が変わった → 古い順に自治体を回る。
//! 例規集トップの「内容現在」が前回と同じ自治体は 1 リクエストで済ませる。
//! 同じホストの自治体は 1 ワーカーが順に処理し（ホストごとに 1 req/sec）、別ホストは並列。
//! 予算切れで中断した自治体は、次回その周回で確認済みの例規を飛ばして再開する。
//!
//! ## 配信（`reiki-build-json`）
//!
//! `reiki/index.json`（全国一覧）・`reiki/{code}/index.json`・`reiki/{code}/{reiki_id}.json`。
//! `--pending-only` は前回公開以降に内容が変わった例規だけを書き、`_publish.json` に
//! 対象自治体と削除すべきパスを出す。R2 へ上げたら `reiki-mark-published` で確定する。

use anyhow::{Context, Result};
use reiki_client::http::PoliteClient;
use reiki_client::registry::{self, Registry};
use reiki_client::{
    HttpProvider, IncompleteListing, MockProvider, Municipality, ReikiDocument, ReikiProvider,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// ── キャッシュ ────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TenantState {
    /// 前回完了時の例規集「内容現在」日。
    #[serde(default)]
    pub current_as_of: Option<String>,
    #[serde(default)]
    pub last_completed_at: Option<String>,
    /// 進行中の周回の開始時刻（完了で None）。
    #[serde(default)]
    pub cycle_started_at: Option<String>,
    #[serde(default)]
    pub last_checked_at: Option<String>,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub doc_count: usize,
    /// 配信先（R2）へ反映済みの時刻。これより後に内容が変わった例規が未公開。
    #[serde(default)]
    pub published_at: Option<String>,
    /// 一覧から消えた（廃止された）例規のうち、配信先からまだ消していないもの。
    #[serde(default)]
    pub removed_pending: Vec<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CacheState {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub tenants: BTreeMap<String, TenantState>,
}

pub struct Cache {
    root: PathBuf,
}

impl Cache {
    pub fn new(cache: &Path) -> Self {
        Self {
            root: cache.join("reiki"),
        }
    }

    fn state_path(&self) -> PathBuf {
        self.root.join("state.json")
    }

    fn tenant_path(&self, code: &str) -> PathBuf {
        self.root.join("tenants").join(format!("{code}.ndjson.zst"))
    }

    pub fn load_state(&self) -> Result<CacheState> {
        match std::fs::read(self.state_path()) {
            Ok(b) => serde_json::from_slice(&b).context("parse reiki state.json"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(CacheState {
                version: 1,
                ..Default::default()
            }),
            Err(e) => Err(e).context("read reiki state.json"),
        }
    }

    pub fn save_state(&self, st: &CacheState) -> Result<()> {
        write_atomic(
            &self.state_path(),
            serde_json::to_string_pretty(st)?.as_bytes(),
        )
    }

    /// 自治体の例規を reiki_id 順で読む。
    pub fn load_docs(&self, code: &str) -> Result<BTreeMap<String, ReikiDocument>> {
        let path = self.tenant_path(code);
        let file = match std::fs::File::open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
            Err(e) => return Err(e).with_context(|| format!("open {}", path.display())),
        };
        let reader = BufReader::new(zstd::Decoder::new(file)?);
        let mut out = BTreeMap::new();
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let doc: ReikiDocument = serde_json::from_str(&line)
                .with_context(|| format!("parse doc in {}", path.display()))?;
            out.insert(doc.reiki_id.clone(), doc);
        }
        Ok(out)
    }

    pub fn save_docs(&self, code: &str, docs: &BTreeMap<String, ReikiDocument>) -> Result<()> {
        let mut enc = zstd::Encoder::new(Vec::new(), 9)?;
        for d in docs.values() {
            serde_json::to_writer(&mut enc, d)?;
            enc.write_all(b"\n")?;
        }
        write_atomic(&self.tenant_path(code), &enc.finish()?)
    }

    pub fn tenant_codes(&self) -> Result<Vec<String>> {
        let dir = self.root.join("tenants");
        let mut codes: Vec<String> = match std::fs::read_dir(&dir) {
            Ok(rd) => rd
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    e.file_name()
                        .to_str()
                        .and_then(|n| n.strip_suffix(".ndjson.zst"))
                        .map(str::to_string)
                })
                .collect(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e.into()),
        };
        codes.sort();
        Ok(codes)
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("rename {}", path.display()))?;
    Ok(())
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn parse_ts(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&chrono::Utc))
}

pub fn load_registry(path: Option<&Path>) -> Result<Registry> {
    match path {
        Some(p) => serde_json::from_slice(&std::fs::read(p)?)
            .with_context(|| format!("parse {}", p.display())),
        None => Ok(registry::embedded()),
    }
}

// ── discover ──────────────────────────────────────────────────────

/// `lawpub reiki-discover`: RILG リンク集からテナント一覧を再生成する。
pub fn run_discover(out: &Path, rilg_html: Option<&Path>) -> Result<()> {
    let reg = match rilg_html {
        Some(p) => registry::parse_rilg(&std::fs::read_to_string(p)?, &now_rfc3339())?,
        None => {
            let client = PoliteClient::new(Duration::from_secs(1))?;
            let mut reg = registry::fetch_rilg(&client)?;
            reg.generated_at = now_rfc3339();
            reg
        }
    };
    let mut by_vendor: BTreeMap<&str, usize> = BTreeMap::new();
    for t in &reg.tenants {
        *by_vendor.entry(t.vendor.as_str()).or_default() += 1;
    }
    write_atomic(out, (serde_json::to_string_pretty(&reg)? + "\n").as_bytes())?;
    tracing::info!(
        "reiki-discover: {} supported tenants {:?}, {} unsupported → {}",
        reg.tenants.len(),
        by_vendor,
        reg.unsupported.len(),
        out.display()
    );
    Ok(())
}

// ── fetch ─────────────────────────────────────────────────────────

pub struct FetchOptions {
    pub municipalities: Vec<String>,
    pub provider: String,
    pub registry: Option<PathBuf>,
    /// この実行で使ってよい時間。超えたら区切りの良いところで中断して状態を保存する。
    pub time_budget: Duration,
    /// 並列に処理するホスト数。
    pub concurrency: usize,
    /// 内容現在日が変わらなくても、この日数を超えたら全件を再確認する。
    pub max_age_days: i64,
    /// 内容現在日が取れない自治体を再確認する間隔。
    pub recheck_days: i64,
    pub min_interval: Duration,
}

#[derive(Debug, Default)]
struct RunStats {
    checked: usize,
    crawled: usize,
    completed: usize,
    docs_changed: usize,
    docs_unchanged: usize,
    docs_failed: usize,
    removed: usize,
    errors: usize,
}

fn origin_of(url: &str) -> String {
    url::Url::parse(url)
        .map(|u| format!("{}://{}", u.scheme(), u.host_str().unwrap_or("")))
        .unwrap_or_else(|_| url.to_string())
}

/// 巡回の優先順位。中断中 → 未取得 → 古い順。
fn priority(st: Option<&TenantState>) -> (u8, String) {
    match st {
        Some(s) if s.cycle_started_at.is_some() => {
            (0, s.cycle_started_at.clone().unwrap_or_default())
        }
        None => (1, String::new()),
        Some(s) if s.last_completed_at.is_none() => (1, String::new()),
        Some(s) => (
            2,
            s.last_checked_at
                .clone()
                .or_else(|| s.last_completed_at.clone())
                .unwrap_or_default(),
        ),
    }
}

/// 内容現在日などから、この自治体を全件巡回すべきか決める。
fn needs_crawl(
    st: &TenantState,
    as_of: Option<&str>,
    now: chrono::DateTime<chrono::Utc>,
    opts: &FetchOptions,
) -> bool {
    if st.cycle_started_at.is_some() {
        return true;
    }
    let Some(done) = st.last_completed_at.as_deref().and_then(parse_ts) else {
        return true;
    };
    let age = now - done;
    if age > chrono::Duration::days(opts.max_age_days) {
        return true;
    }
    match as_of {
        Some(a) => st.current_as_of.as_deref() != Some(a),
        None => age > chrono::Duration::days(opts.recheck_days),
    }
}

struct Shared<'a> {
    cache: &'a Cache,
    state: Mutex<CacheState>,
    stats: Mutex<RunStats>,
    deadline: Instant,
    opts: &'a FetchOptions,
}

impl Shared<'_> {
    fn update_state(&self, code: &str, f: impl FnOnce(&mut TenantState)) -> Result<()> {
        let mut st = self.state.lock().unwrap();
        f(st.tenants.entry(code.to_string()).or_default());
        self.cache.save_state(&st)
    }

    fn tenant_state(&self, code: &str) -> TenantState {
        self.state
            .lock()
            .unwrap()
            .tenants
            .get(code)
            .cloned()
            .unwrap_or_default()
    }

    fn out_of_time(&self) -> bool {
        Instant::now() >= self.deadline
    }
}

const CHECKPOINT_EVERY: usize = 200;

fn crawl_tenant(sh: &Shared, p: &dyn ReikiProvider, m: &Municipality) -> Result<()> {
    let now = chrono::Utc::now();
    let as_of = match p.current_as_of(m) {
        Ok(a) => a,
        Err(e) => {
            sh.stats.lock().unwrap().errors += 1;
            let msg = format!("{e:#}");
            tracing::warn!("reiki {} {}: entry page: {msg}", m.code, m.name);
            return sh.update_state(&m.code, |s| {
                s.last_error = Some(msg);
                s.last_checked_at = Some(now_rfc3339());
            });
        }
    };
    sh.stats.lock().unwrap().checked += 1;
    let st = sh.tenant_state(&m.code);
    if !needs_crawl(&st, as_of.as_deref(), now, sh.opts) {
        return sh.update_state(&m.code, |s| {
            s.last_checked_at = Some(now_rfc3339());
            s.last_error = None;
        });
    }
    sh.stats.lock().unwrap().crawled += 1;
    let cycle_start = st.cycle_started_at.clone().unwrap_or_else(now_rfc3339);
    let cycle_ts = parse_ts(&cycle_start).unwrap_or(now);
    sh.update_state(&m.code, |s| s.cycle_started_at = Some(cycle_start.clone()))?;

    let (metas, complete_listing) = match p.list_reiki(m) {
        Ok(v) => (v, true),
        Err(e) if e.downcast_ref::<IncompleteListing>().is_some() => {
            tracing::warn!("reiki {} {}: {e:#} — 削除判定は行わない", m.code, m.name);
            (Vec::new(), false)
        }
        Err(e) => {
            sh.stats.lock().unwrap().errors += 1;
            let msg = format!("{e:#}");
            tracing::warn!("reiki {} {}: listing: {msg}", m.code, m.name);
            return sh.update_state(&m.code, |s| s.last_error = Some(msg));
        }
    };
    if !complete_listing {
        // 一部ページが取れない一覧で進めると周回が完了扱いにならないので、次回やり直す。
        sh.stats.lock().unwrap().errors += 1;
        return sh.update_state(&m.code, |s| {
            s.last_error = Some("incomplete listing".into())
        });
    }

    let mut docs = sh.cache.load_docs(&m.code)?;
    let mut since_checkpoint = 0usize;
    let mut failed = 0usize;
    let mut interrupted = false;
    for meta in &metas {
        if sh.out_of_time() {
            interrupted = true;
            break;
        }
        if let Some(old) = docs.get(&meta.reiki_id) {
            let checked =
                parse_ts(&old.source.checked_at).or_else(|| parse_ts(&old.source.fetched_at));
            if checked.is_some_and(|c| c >= cycle_ts) {
                continue; // この周回で確認済み（中断からの再開）
            }
        }
        match p.fetch_reiki(meta, m, as_of.clone()) {
            Ok(mut doc) => {
                let mut stats = sh.stats.lock().unwrap();
                match docs.get_mut(&meta.reiki_id) {
                    Some(old) if old.content_sha256 == doc.content_sha256 => {
                        old.source.checked_at = doc.source.checked_at.clone();
                        old.current_as_of = doc.current_as_of.clone();
                        stats.docs_unchanged += 1;
                    }
                    _ => {
                        doc.source.fetched_at = doc.source.checked_at.clone();
                        docs.insert(meta.reiki_id.clone(), doc);
                        stats.docs_changed += 1;
                    }
                }
            }
            Err(e) => {
                failed += 1;
                sh.stats.lock().unwrap().docs_failed += 1;
                tracing::warn!("reiki {}: {e:#}", meta.reiki_id);
            }
        }
        since_checkpoint += 1;
        if since_checkpoint >= CHECKPOINT_EVERY {
            sh.cache.save_docs(&m.code, &docs)?;
            since_checkpoint = 0;
        }
    }

    // 本文の取得失敗が多い周回は不完全とみなし、次回に持ち越す（一時障害で削除しない）。
    let too_many_failures = failed > 5 && failed * 20 > metas.len();
    let completed = !interrupted && !too_many_failures && !metas.is_empty();
    let mut removed = Vec::new();
    if completed {
        let live: HashSet<&str> = metas.iter().map(|m| m.reiki_id.as_str()).collect();
        removed = docs
            .keys()
            .filter(|k| !live.contains(k.as_str()))
            .cloned()
            .collect();
        for k in &removed {
            docs.remove(k);
        }
    }
    sh.cache.save_docs(&m.code, &docs)?;
    {
        let mut stats = sh.stats.lock().unwrap();
        stats.removed += removed.len();
        if completed {
            stats.completed += 1;
        }
    }
    let count = docs.len();
    tracing::info!(
        "reiki {} {}: {} listed, {} cached{}{}",
        m.code,
        m.name,
        metas.len(),
        count,
        if interrupted {
            " (time budget reached, will resume)"
        } else {
            ""
        },
        if too_many_failures {
            " (too many failures, will retry)"
        } else {
            ""
        },
    );
    sh.update_state(&m.code, |s| {
        s.doc_count = count;
        s.last_checked_at = Some(now_rfc3339());
        s.removed_pending.extend(removed);
        if completed {
            s.cycle_started_at = None;
            s.last_completed_at = Some(now_rfc3339());
            s.current_as_of = as_of.clone();
            s.last_error = None;
        } else if too_many_failures {
            s.last_error = Some(format!("{failed} of {} documents failed", metas.len()));
        } else if metas.is_empty() {
            s.last_error = Some("empty listing".into());
        }
    })
}

/// `lawpub reiki-fetch`
pub fn run_fetch(cache_dir: &Path, opts: &FetchOptions) -> Result<()> {
    let reg = load_registry(opts.registry.as_deref())?;
    // CI から `--municipalities ""` で渡されることがあるので空要素は無視する。
    let wanted: Vec<&str> = opts
        .municipalities
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let targets: Vec<Municipality> = reg
        .tenants
        .into_iter()
        .filter(|m| wanted.is_empty() || wanted.contains(&m.code.as_str()))
        .collect();
    if targets.is_empty() {
        anyhow::bail!("no reiki tenants matched {:?}", wanted);
    }
    let cache = Cache::new(cache_dir);
    let state = cache.load_state()?;

    // ホストごとにまとめ、各ホスト内は優先順位順。ホスト自体は最優先テナントの順に並べる。
    let mut groups: HashMap<String, Vec<Municipality>> = HashMap::new();
    for m in targets {
        groups
            .entry(origin_of(&m.reiki_base_url))
            .or_default()
            .push(m);
    }
    let mut queue: Vec<Vec<Municipality>> = groups
        .into_values()
        .map(|mut v| {
            v.sort_by_key(|m| priority(state.tenants.get(&m.code)));
            v
        })
        .collect();
    queue.sort_by_key(|v| {
        (
            priority(state.tenants.get(&v[0].code)),
            std::cmp::Reverse(v.len()),
        )
    });
    let total: usize = queue.iter().map(Vec::len).sum();
    tracing::info!(
        "reiki-fetch: {} tenants on {} hosts, budget {:?}",
        total,
        queue.len(),
        opts.time_budget
    );

    let provider: Arc<dyn ReikiProvider> = match opts.provider.as_str() {
        "mock" => Arc::new(MockProvider),
        _ => Arc::new(HttpProvider::new(Arc::new(PoliteClient::new(
            opts.min_interval,
        )?))),
    };
    let shared = Shared {
        cache: &cache,
        state: Mutex::new(state),
        stats: Mutex::new(RunStats::default()),
        deadline: Instant::now() + opts.time_budget,
        opts,
    };
    let queue = Mutex::new(VecDeque::from(queue));
    std::thread::scope(|s| {
        for _ in 0..opts.concurrency.max(1) {
            s.spawn(|| loop {
                if shared.out_of_time() {
                    break;
                }
                let Some(group) = queue.lock().unwrap().pop_front() else {
                    break;
                };
                for m in &group {
                    if shared.out_of_time() {
                        break;
                    }
                    if let Err(e) = crawl_tenant(&shared, provider.as_ref(), m) {
                        shared.stats.lock().unwrap().errors += 1;
                        tracing::error!("reiki {} {}: {e:#}", m.code, m.name);
                    }
                }
            });
        }
    });
    let stats = shared.stats.into_inner().unwrap();
    tracing::info!("reiki-fetch done: {stats:?}");
    Ok(())
}

// ── build-json ────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct PublishManifest {
    pub generated_at: String,
    /// 書き出した（＝配信先へ上げる）自治体。
    pub municipalities: Vec<String>,
    /// 配信先から削除するパス（`reiki/` からの相対）。
    pub removed: Vec<String>,
    pub docs_written: usize,
}

fn index_entry(d: &ReikiDocument) -> serde_json::Value {
    serde_json::json!({
        "reiki_id": d.reiki_id,
        "title": d.title,
        "reiki_number": d.reiki_number,
        "kind": d.kind,
        "promulgated_date": d.promulgated_date,
        "article_count": d.articles.len(),
    })
}

fn write_json(path: &Path, v: &impl Serialize) -> Result<()> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    std::fs::write(path, serde_json::to_vec(v)?)
        .with_context(|| format!("write {}", path.display()))
}

/// `lawpub reiki-build-json`
pub fn run_build_json(
    cache_dir: &Path,
    public: &Path,
    pending_only: bool,
    registry: Option<&Path>,
) -> Result<()> {
    let cache = Cache::new(cache_dir);
    let state = cache.load_state()?;
    let reg = load_registry(registry)?;
    let reg_by_code: HashMap<&str, &Municipality> =
        reg.tenants.iter().map(|m| (m.code.as_str(), m)).collect();
    let out = public.join("reiki");
    std::fs::create_dir_all(&out)?;
    let generated_at = now_rfc3339();

    let mut municipalities = Vec::new();
    let mut touched = Vec::new();
    let mut removed = Vec::new();
    let mut total_docs = 0usize;
    let mut written = 0usize;
    for code in cache.tenant_codes()? {
        let docs = cache.load_docs(&code)?;
        let st = state.tenants.get(&code).cloned().unwrap_or_default();
        let published = st.published_at.as_deref().and_then(parse_ts);
        let pending: Vec<&ReikiDocument> = docs
            .values()
            .filter(|d| {
                !pending_only
                    || published.is_none_or(|p| {
                        parse_ts(&d.source.fetched_at).is_none_or(|f| f > p)
                    })
            })
            .collect();
        let is_touched = !pending_only
            || !pending.is_empty()
            || !st.removed_pending.is_empty()
            || published.is_none();
        let first = docs.values().next();
        let name = reg_by_code
            .get(code.as_str())
            .map(|m| m.name.clone())
            .or_else(|| first.map(|d| d.municipality_name.clone()))
            .unwrap_or_default();
        let prefecture = reg_by_code
            .get(code.as_str())
            .map(|m| m.prefecture.clone())
            .or_else(|| first.map(|d| d.prefecture.clone()))
            .unwrap_or_default();
        if !docs.is_empty() {
            municipalities.push(serde_json::json!({
                "municipality_code": code,
                "name": name,
                "prefecture": prefecture,
                "count": docs.len(),
                "current_as_of": st.current_as_of,
                "updated_at": st.last_completed_at,
                "vendor": reg_by_code.get(code.as_str()).map(|m| m.vendor.as_str()),
                "source_url": reg_by_code.get(code.as_str()).map(|m| m.source_url.clone()),
            }));
        }
        total_docs += docs.len();
        if !is_touched {
            continue;
        }
        let dir = out.join(&code);
        for d in &pending {
            write_json(&dir.join(format!("{}.json", d.reiki_id)), d)?;
            written += 1;
        }
        let mut entries: Vec<serde_json::Value> = docs.values().map(index_entry).collect();
        entries.sort_by(|a, b| a["title"].as_str().cmp(&b["title"].as_str()));
        write_json(
            &dir.join("index.json"),
            &serde_json::json!({
                "schema_version": 2,
                "municipality_code": code,
                "name": name,
                "prefecture": prefecture,
                "current_as_of": st.current_as_of,
                "count": entries.len(),
                "reiki": entries,
            }),
        )?;
        removed.extend(
            st.removed_pending
                .iter()
                .map(|id| format!("{code}/{id}.json")),
        );
        touched.push(code);
    }

    let unsupported = reg.unsupported.len();
    write_json(
        &out.join("index.json"),
        &serde_json::json!({
            "schema_version": 2,
            "generated_at": generated_at,
            "count": municipalities.len(),
            "reiki_count": total_docs,
            "registered_count": reg.tenants.len(),
            "unsupported_count": unsupported,
            // ダッシュボードの収集状況（build.rs collect_corpus_health が読む）。
            "collection": collection_summary(&state),
            "municipalities": municipalities,
        }),
    )?;
    if pending_only {
        write_json(
            &out.join("_publish.json"),
            &PublishManifest {
                generated_at,
                municipalities: touched.clone(),
                removed,
                docs_written: written,
            },
        )?;
    }
    tracing::info!(
        "reiki-build-json: {} municipalities / {} reiki ({} touched, {} docs written)",
        municipalities.len(),
        total_docs,
        touched.len(),
        written
    );
    Ok(())
}

/// 自治体ごとの巡回状態から、コーパス全体の収集状況を要約する。
/// 直近 2 日以内に 1 自治体でも巡回を完了していれば success、エラーだけなら failure。
fn collection_summary(state: &CacheState) -> serde_json::Value {
    let last_attempt = state
        .tenants
        .values()
        .filter_map(|t| t.last_checked_at.clone())
        .max();
    let last_success = state
        .tenants
        .values()
        .filter_map(|t| t.last_completed_at.clone())
        .max();
    let recent_success = last_success
        .as_deref()
        .and_then(parse_ts)
        .is_some_and(|t| chrono::Utc::now() - t < chrono::Duration::days(2));
    let any_error = state.tenants.values().any(|t| t.last_error.is_some());
    let status = match (&last_attempt, recent_success, any_error) {
        (None, _, _) => None,
        (_, true, _) | (_, false, false) => Some("success"),
        (_, false, true) => Some("failure"),
    };
    serde_json::json!({
        "status": status,
        "last_attempt_at": last_attempt,
        "last_success_at": last_success,
        "tenants_with_errors": state.tenants.values().filter(|t| t.last_error.is_some()).count(),
    })
}

/// `lawpub reiki-mark-published`: `_publish.json` の自治体を公開済みにする。
pub fn run_mark_published(cache_dir: &Path, manifest: &Path) -> Result<()> {
    let cache = Cache::new(cache_dir);
    let mut state = cache.load_state()?;
    let man: PublishManifest =
        serde_json::from_slice(&std::fs::read(manifest)?).context("parse _publish.json")?;
    for code in &man.municipalities {
        let s = state.tenants.entry(code.clone()).or_default();
        s.published_at = Some(man.generated_at.clone());
        s.removed_pending.clear();
    }
    cache.save_state(&state)?;
    tracing::info!(
        "reiki-mark-published: {} municipalities at {}",
        man.municipalities.len(),
        man.generated_at
    );
    Ok(())
}

// ── search db ─────────────────────────────────────────────────────

/// `lawpub reiki-build-search-db`
pub fn run_build_search_db(cache_dir: &Path, out: &Path) -> Result<()> {
    use search_index::reiki::{build_reiki_search_db, ReikiFtsArticle, ReikiFtsDoc};
    let cache = Cache::new(cache_dir);
    let codes = cache.tenant_codes()?;
    let iter = codes.into_iter().flat_map(|code| {
        let docs: Vec<Result<ReikiFtsDoc>> = match cache.load_docs(&code) {
            Ok(docs) => docs
                .into_values()
                .map(|d| {
                    let articles = d
                        .articles
                        .iter()
                        .map(|a| ReikiFtsArticle {
                            text: a.text(),
                            article_id: a.article_id.clone(),
                            article_no: a.article_no.clone(),
                            caption: a.caption.clone(),
                        })
                        .collect();
                    Ok(ReikiFtsDoc {
                        municipality_code: d.municipality_code,
                        municipality_name: d.municipality_name,
                        prefecture: d.prefecture,
                        reiki_id: d.reiki_id,
                        title: d.title,
                        reiki_number: d.reiki_number,
                        kind: d.kind,
                        promulgated_date: d.promulgated_date,
                        articles,
                    })
                })
                .collect(),
            Err(e) => vec![Err(e)],
        };
        docs
    });
    let stats = build_reiki_search_db(out, iter)?;
    tracing::info!(
        "reiki-build-search-db: {} docs / {} articles → {}",
        stats.docs,
        stats.articles,
        out.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("lawpub_reiki_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    fn registry_file(dir: &Path) -> PathBuf {
        let reg = Registry {
            schema_version: 1,
            generated_at: String::new(),
            source: String::new(),
            unsupported: vec![],
            tenants: vec![
                Municipality::test("121002", "千葉市", "https://a.example/chiba"),
                Municipality::test("012122", "留萌市", "https://b.example/rumoi/d1w_reiki"),
            ],
        };
        let p = dir.join("tenants.json");
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(&p, serde_json::to_vec(&reg).unwrap()).unwrap();
        p
    }

    fn opts(reg: PathBuf) -> FetchOptions {
        FetchOptions {
            municipalities: vec![],
            provider: "mock".into(),
            registry: Some(reg),
            time_budget: Duration::from_secs(60),
            concurrency: 2,
            max_age_days: 60,
            recheck_days: 7,
            min_interval: Duration::from_millis(0),
        }
    }

    #[test]
    fn fetch_build_publish_cycle() {
        let dir = tmp("cycle");
        let reg = registry_file(&dir);
        let cache = dir.join("cache");
        let public = dir.join("public");
        run_fetch(&cache, &opts(reg.clone())).unwrap();

        let c = Cache::new(&cache);
        let st = c.load_state().unwrap();
        let chiba = &st.tenants["121002"];
        assert!(chiba.last_completed_at.is_some());
        assert!(chiba.cycle_started_at.is_none());
        assert_eq!(chiba.current_as_of.as_deref(), Some("2024-04-01"));
        assert_eq!(c.load_docs("121002").unwrap().len(), 1);

        // 初回: 全自治体が未公開
        run_build_json(&cache, &public, true, Some(&reg)).unwrap();
        let man: PublishManifest =
            serde_json::from_slice(&std::fs::read(public.join("reiki/_publish.json")).unwrap())
                .unwrap();
        assert_eq!(man.municipalities.len(), 2);
        assert_eq!(man.docs_written, 2);
        let idx: serde_json::Value =
            serde_json::from_slice(&std::fs::read(public.join("reiki/index.json")).unwrap())
                .unwrap();
        assert_eq!(idx["count"], 2);
        assert_eq!(idx["reiki_count"], 2);
        let muni: serde_json::Value =
            serde_json::from_slice(&std::fs::read(public.join("reiki/121002/index.json")).unwrap())
                .unwrap();
        assert_eq!(muni["name"], "千葉市");
        assert_eq!(muni["reiki"][0]["kind"], "条例");
        assert!(public
            .join("reiki/121002/121002_jourei_sample.json")
            .exists());
        run_mark_published(&cache, &public.join("reiki/_publish.json")).unwrap();

        // 内容現在日が同じなら再巡回しない → 公開対象なし
        run_fetch(&cache, &opts(reg.clone())).unwrap();
        let public2 = dir.join("public2");
        run_build_json(&cache, &public2, true, Some(&reg)).unwrap();
        let man2: PublishManifest =
            serde_json::from_slice(&std::fs::read(public2.join("reiki/_publish.json")).unwrap())
                .unwrap();
        assert!(man2.municipalities.is_empty());
        assert_eq!(man2.docs_written, 0);
        // 全国一覧は常に全自治体を含む
        let idx2: serde_json::Value =
            serde_json::from_slice(&std::fs::read(public2.join("reiki/index.json")).unwrap())
                .unwrap();
        assert_eq!(idx2["count"], 2);

        // 検索 DB
        let db = dir.join("reiki-search.db");
        run_build_search_db(&cache, &db).unwrap();
        assert!(db.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn needs_crawl_rules() {
        let o = opts(PathBuf::new());
        let now = chrono::Utc::now();
        let fresh = TenantState {
            current_as_of: Some("2026-07-01".into()),
            last_completed_at: Some((now - chrono::Duration::days(3)).to_rfc3339()),
            ..Default::default()
        };
        assert!(!needs_crawl(&fresh, Some("2026-07-01"), now, &o));
        assert!(needs_crawl(&fresh, Some("2026-08-01"), now, &o));
        assert!(!needs_crawl(&fresh, None, now, &o));
        let old = TenantState {
            last_completed_at: Some((now - chrono::Duration::days(61)).to_rfc3339()),
            ..fresh.clone()
        };
        assert!(needs_crawl(&old, Some("2026-07-01"), now, &o));
        let resuming = TenantState {
            cycle_started_at: Some(now.to_rfc3339()),
            ..fresh
        };
        assert!(needs_crawl(&resuming, Some("2026-07-01"), now, &o));
        assert!(needs_crawl(
            &TenantState::default(),
            Some("2026-07-01"),
            now,
            &o
        ));
    }

    #[test]
    fn priority_order() {
        let resuming = TenantState {
            cycle_started_at: Some("2026-01-01T00:00:00Z".into()),
            ..Default::default()
        };
        let done = TenantState {
            last_completed_at: Some("2026-01-01T00:00:00Z".into()),
            ..Default::default()
        };
        assert!(priority(Some(&resuming)) < priority(None));
        assert!(priority(None) < priority(Some(&done)));
    }
}
