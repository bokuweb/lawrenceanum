//! `lawpub treaty-fetch` / `treaty-build-json` — 外務省の条約データ。
//!
//! 取得 (`treaty-fetch`) は出所ごとに分けて `.cache/treaty/` に置き、統合は build で行う:
//!
//! - `search/{bilateral,multilateral}.json` — 条約データ検索の一覧 (全ページ取れたら置き換え)
//! - `diet/{key}.json` — 国会提出条約ページ (経過・条約番号・PDF)、`diet/sessions/{n}.json` — 回次ページのリンク
//! - `text/{id}.json` — 本文 (HTML 版、`--with-pdf` なら HTML 版の無いものを PDF から)
//! - `../treaty-assets/{sha256}.pdf` — 取得した PDF 原本 (内容アドレス)
//!
//! build は題名で国会提出条約ページと一覧の行を突き合わせ、`public/treaties/{id}.json` と
//! `index.json` を書く。

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use treaty_client::{
    match_diet_to_items, merge, parse_diet_detail, parse_diet_index, parse_diet_session,
    parse_search_page, parse_treaty_text, search_page_url, Attachment, DietLink, DietTreaty,
    HttpProvider, Kind, MockProvider, SearchItem, TextRecord, Treaty, TreatyProvider,
    DIET_INDEX_URL, SEARCH_TOP_URL,
};

/// 回次ページを毎回取り直す直近の回次数 (提出条約の追加・差し替えがあり得る)。
const RECENT_SESSIONS: usize = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SearchCache {
    fetched_at: String,
    /// 全ページを取り切ったか。途中で打ち切った一覧は前回分に足し込む。
    complete: bool,
    total: Option<u32>,
    items: Vec<SearchItem>,
}

pub struct FetchOptions {
    pub interval_ms: u64,
    /// 一覧の最大ページ数 (0 = 全ページ)。
    pub max_pages: u32,
    /// 全ページ取得済みの一覧がこの日数より新しければ一覧を取り直さない。
    pub list_max_age_days: i64,
    /// 1 回の実行で取得する条約ページ・本文の最大数。
    pub limit: usize,
    pub with_pdf: bool,
    pub pdf_limit: usize,
    pub skip_diet: bool,
}

fn make_provider(provider: &str, interval_ms: u64) -> Result<Box<dyn TreatyProvider>> {
    Ok(match provider {
        "mock" => Box::new(MockProvider),
        _ => Box::new(HttpProvider::new(Duration::from_millis(interval_ms))?),
    })
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

fn write_json<T: Serialize>(path: &Path, v: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(v)?)
        .with_context(|| format!("write {}", path.display()))
}

/// ディレクトリ直下の `*.json` (exFAT/macOS の AppleDouble `._*` は除く)。
fn json_files(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            p.is_file() && name.ends_with(".json") && !name.starts_with("._")
        })
        .collect();
    out.sort();
    out
}

fn search_cache_path(cache: &Path, kind: Kind) -> PathBuf {
    cache
        .join("treaty")
        .join("search")
        .join(format!("{}.json", kind.as_str()))
}

/// URL の最後のパス要素から拡張子を除いたもの (国会提出条約ページの key)。
fn page_key(url: &str) -> String {
    let name = url.rsplit('/').next().unwrap_or(url);
    name.rsplit_once('.')
        .map(|(n, _)| n)
        .unwrap_or(name)
        .to_string()
}

fn is_fresh(fetched_at: &str, max_age_days: i64) -> bool {
    chrono::DateTime::parse_from_rfc3339(fetched_at)
        .map(|t| chrono::Utc::now().signed_duration_since(t) < chrono::Duration::days(max_age_days))
        .unwrap_or(false)
}

/// 条約データ検索の一覧を取る。全ページ取れたら置き換え、打ち切ったら前回分に足し込む。
fn fetch_listing(
    p: &dyn TreatyProvider,
    cache: &Path,
    kind: Kind,
    opts: &FetchOptions,
) -> Result<()> {
    let path = search_cache_path(cache, kind);
    let prev: Option<SearchCache> = read_json(&path);
    if let Some(prev) = &prev {
        if prev.complete && is_fresh(&prev.fetched_at, opts.list_max_age_days) {
            tracing::info!(
                "treaty-fetch: {} listing is fresh ({} items, {}), skip",
                kind.as_str(),
                prev.items.len(),
                prev.fetched_at
            );
            return Ok(());
        }
    }
    let mut items: Vec<SearchItem> = Vec::new();
    let mut total = None;
    let mut complete = false;
    let mut page = 0u32;
    loop {
        let html = match p.search_page(kind, page) {
            Ok(h) => h,
            Err(e) if page == 0 => return Err(e),
            Err(e) => {
                tracing::warn!("treaty-fetch: {} page {page}: {e:#}", kind.as_str());
                break;
            }
        };
        let parsed = parse_search_page(&html, kind, &search_page_url(page));
        if page == 0 {
            total = parsed.total;
            tracing::info!(
                "treaty-fetch: {} listing: {:?} treaties in {:?} pages",
                kind.as_str(),
                parsed.total,
                parsed.pages
            );
        }
        if parsed.items.is_empty() {
            complete = true;
            break;
        }
        for it in parsed.items {
            if !items.iter().any(|x| x.id == it.id) {
                items.push(it);
            }
        }
        page += 1;
        if parsed.pages.is_some_and(|n| page >= n) {
            complete = true;
            break;
        }
        if opts.max_pages > 0 && page >= opts.max_pages {
            break;
        }
    }
    if !complete {
        // 前回の一覧の行で、今回のページに無かったものは残す。
        for old in prev.map(|p| p.items).unwrap_or_default() {
            if !items.iter().any(|x| x.id == old.id) {
                items.push(old);
            }
        }
    }
    tracing::info!(
        "treaty-fetch: {} listing saved: {} items (complete={complete})",
        kind.as_str(),
        items.len()
    );
    write_json(
        &path,
        &SearchCache {
            fetched_at: chrono::Utc::now().to_rfc3339(),
            complete,
            total,
            items,
        },
    )
}

/// 国会提出条約: 目次 → 回次ページ → 条約ページ。公布・効力発生まで済んだ保存分は取り直さない。
/// 戻り値は取得した条約ページ数。
fn fetch_diet(p: &dyn TreatyProvider, cache: &Path, budget: usize) -> Result<usize> {
    let dir = cache.join("treaty").join("diet");
    let Some(index) = p.get_html(DIET_INDEX_URL)? else {
        anyhow::bail!("{DIET_INDEX_URL} not found");
    };
    let sessions = parse_diet_index(&index, DIET_INDEX_URL);
    tracing::info!("treaty-fetch: diet index: {} sessions", sessions.len());
    // key → (最初に提出された回次, URL)。継続審議の条約は複数の回次ページに載る。
    let mut pages: BTreeMap<String, (u32, String)> = BTreeMap::new();
    for (i, s) in sessions.iter().enumerate() {
        let spath = dir.join("sessions").join(format!("{}.json", s.session));
        let cached: Option<Vec<DietLink>> = read_json(&spath);
        let links = match cached {
            Some(l) if i >= RECENT_SESSIONS => l,
            _ => match p.get_html(&s.url) {
                Ok(Some(html)) => {
                    let l = parse_diet_session(&html, &s.url);
                    write_json(&spath, &l)?;
                    l
                }
                Ok(None) => {
                    tracing::warn!("treaty-fetch: session {} page not found", s.session);
                    continue;
                }
                Err(e) => {
                    tracing::warn!("treaty-fetch: session {}: {e:#}", s.session);
                    continue;
                }
            },
        };
        for l in links {
            let key = page_key(&l.url);
            let e = pages.entry(key).or_insert((s.session, l.url.clone()));
            e.0 = e.0.min(s.session);
        }
    }
    let fetched_at = chrono::Utc::now().to_rfc3339();
    let (mut fetched, mut skipped, mut failed) = (0usize, 0usize, 0usize);
    for (key, (session, url)) in &pages {
        let path = dir.join(format!("{key}.json"));
        if read_json::<DietTreaty>(&path).is_some_and(|d| d.is_settled()) {
            skipped += 1;
            continue;
        }
        if fetched >= budget {
            tracing::info!("treaty-fetch: reached --limit in diet pages");
            break;
        }
        fetched += 1;
        match p.get_html(url) {
            Ok(Some(html)) => match parse_diet_detail(&html, url, Some(*session), &fetched_at) {
                Some(d) => write_json(&path, &d)?,
                None => {
                    tracing::warn!("treaty-fetch: no title/events in {url}");
                    failed += 1;
                }
            },
            Ok(None) => {
                tracing::warn!("treaty-fetch: {url} not found");
                failed += 1;
            }
            Err(e) => {
                tracing::warn!("treaty-fetch: {url}: {e:#}");
                failed += 1;
            }
        }
    }
    tracing::info!(
        "treaty-fetch: diet pages: {} known, fetched={fetched} settled={skipped} failed={failed}",
        pages.len()
    );
    Ok(fetched)
}

fn load_items(cache: &Path) -> Vec<SearchItem> {
    Kind::ALL
        .iter()
        .filter_map(|k| read_json::<SearchCache>(&search_cache_path(cache, *k)))
        .flat_map(|c| c.items)
        .collect()
}

fn load_diets(cache: &Path) -> Vec<DietTreaty> {
    json_files(&cache.join("treaty").join("diet"))
        .iter()
        .filter_map(|p| read_json(p))
        .collect()
}

fn text_path(cache: &Path, id: &str) -> PathBuf {
    cache.join("treaty").join("text").join(format!("{id}.json"))
}

/// 本文 HTML を取る (保存済みで URL が同じものは飛ばす)。新しく効力を生じたものから。
fn fetch_texts(p: &dyn TreatyProvider, cache: &Path, budget: usize) -> Result<usize> {
    let mut items: Vec<SearchItem> = load_items(cache)
        .into_iter()
        .filter(|it| it.html_url.is_some())
        .filter(|it| {
            read_json::<TextRecord>(&text_path(cache, &it.id))
                .is_none_or(|t| Some(&t.url) != it.html_url.as_ref())
        })
        .collect();
    items.sort_by(|a, b| b.effective_on.cmp(&a.effective_on).then(a.id.cmp(&b.id)));
    let pending = items.len();
    let mut fetched = 0usize;
    for it in items.into_iter().take(budget) {
        let url = it.html_url.clone().unwrap_or_default();
        let fetched_at = chrono::Utc::now().to_rfc3339();
        let rec = match p.get_html(&url) {
            Ok(Some(html)) => {
                let t = parse_treaty_text(&html);
                TextRecord {
                    id: it.id.clone(),
                    url: url.clone(),
                    lang: t.as_ref().map(|t| t.lang.clone()),
                    error: t.is_none().then(|| "no text blocks".to_string()),
                    text: t.map(|t| t.text),
                    source: "html".into(),
                    attachments: vec![],
                    fetched_at,
                }
            }
            Ok(None) => TextRecord {
                id: it.id.clone(),
                url: url.clone(),
                lang: None,
                text: None,
                source: "html".into(),
                attachments: vec![],
                error: Some("not found".into()),
                fetched_at,
            },
            Err(e) => {
                // 一時的な失敗は保存しない (次回取り直す)。
                tracing::warn!("treaty-fetch: text {}: {e:#}", it.id);
                continue;
            }
        };
        write_json(&text_path(cache, &it.id), &rec)?;
        fetched += 1;
    }
    tracing::info!("treaty-fetch: texts fetched={fetched} (pending before run={pending})");
    Ok(fetched)
}

fn extract_pdf_text(path: &Path) -> Result<String> {
    let out = Command::new("pdftotext")
        .args(["-layout", "-enc", "UTF-8"])
        .arg(path)
        .arg("-")
        .output()
        .context("run pdftotext (install poppler-utils)")?;
    if !out.status.success() {
        anyhow::bail!(
            "pdftotext failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if text.is_empty() {
        anyhow::bail!("pdftotext returned empty text (possibly scanned PDF)");
    }
    Ok(text)
}

/// HTML 版の無い条約の本文を PDF から抜く。原本は `.cache/treaty-assets/{sha256}.pdf`。
fn fetch_pdf_texts(p: &dyn TreatyProvider, cache: &Path, budget: usize) -> Result<usize> {
    let items = load_items(cache);
    let diets = load_diets(cache);
    let matched = match_diet_to_items(&items, &diets);
    // (text id, PDF の URL 群)。一覧の行は本文 PDF、一覧に無い国会提出条約は「和文」PDF。
    let mut targets: Vec<(String, Vec<Attachment>, Option<String>)> = items
        .iter()
        .filter(|it| it.html_url.is_none() && !it.pdf_urls.is_empty())
        .map(|it| {
            let n = it.pdf_urls.len();
            let atts = it
                .pdf_urls
                .iter()
                .enumerate()
                .map(|(i, u)| {
                    Attachment::new(
                        if n > 1 {
                            format!("本文 {}", i + 1)
                        } else {
                            "本文".into()
                        },
                        u.clone(),
                    )
                })
                .collect();
            (it.id.clone(), atts, it.effective_on.clone())
        })
        .collect();
    for d in &diets {
        if matched.contains_key(&d.key) {
            continue;
        }
        if let Some(a) = d.attachments.iter().find(|a| a.label.starts_with("和文")) {
            targets.push((
                format!("diet-{}", d.key),
                vec![a.clone()],
                d.dates.effective_on.clone(),
            ));
        }
    }
    targets.retain(|(id, _, _)| !text_path(cache, id).exists());
    targets.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
    let assets = cache.join("treaty-assets");
    std::fs::create_dir_all(&assets)?;
    let mut done = 0usize;
    for (id, atts, _) in targets.into_iter().take(budget) {
        let fetched_at = chrono::Utc::now().to_rfc3339();
        let mut texts = Vec::new();
        let mut out_atts = Vec::new();
        let mut transient = false;
        for mut a in atts {
            match p.get_bytes(&a.url) {
                Ok(Some(f)) => {
                    let sha = hex::encode(Sha256::digest(&f.bytes));
                    let raw = assets.join(format!("{sha}.pdf"));
                    if !raw.exists() {
                        std::fs::write(&raw, &f.bytes)
                            .with_context(|| format!("write {}", raw.display()))?;
                    }
                    a.sha256 = Some(sha);
                    a.bytes = Some(f.bytes.len() as u64);
                    a.media_type = f.media_type;
                    a.fetched_at = Some(fetched_at.clone());
                    match extract_pdf_text(&raw) {
                        Ok(t) => texts.push(t),
                        Err(e) => a.extraction_error = Some(e.to_string()),
                    }
                }
                Ok(None) => a.extraction_error = Some("not found".into()),
                Err(e) => {
                    tracing::warn!("treaty-fetch: pdf {}: {e:#}", a.url);
                    transient = true;
                }
            }
            out_atts.push(a);
        }
        if transient {
            continue;
        }
        let text = (!texts.is_empty()).then(|| texts.join("\n"));
        let rec = TextRecord {
            id: id.clone(),
            url: out_atts.first().map(|a| a.url.clone()).unwrap_or_default(),
            lang: text.as_ref().map(|_| "ja".to_string()),
            error: text
                .is_none()
                .then(|| "no text extracted from PDF".to_string()),
            text,
            source: "pdftotext".into(),
            attachments: out_atts,
            fetched_at,
        };
        write_json(&text_path(cache, &id), &rec)?;
        done += 1;
    }
    tracing::info!("treaty-fetch: pdf texts processed={done}");
    Ok(done)
}

/// `lawpub treaty-fetch`。
pub fn run_fetch(cache: &Path, provider: &str, opts: &FetchOptions) -> Result<()> {
    let p = make_provider(provider, opts.interval_ms)?;
    // 一覧も国会提出条約も取れなかった (サイトに届かない) ときは失敗として返す。
    let mut errors: Vec<String> = Vec::new();
    for kind in Kind::ALL {
        if let Err(e) = fetch_listing(p.as_ref(), cache, kind, opts) {
            tracing::warn!("treaty-fetch: {} listing failed: {e:#}", kind.as_str());
            errors.push(format!("{} listing: {e:#}", kind.as_str()));
        }
    }
    let mut budget = opts.limit;
    if !opts.skip_diet {
        match fetch_diet(p.as_ref(), cache, budget) {
            Ok(n) => budget = budget.saturating_sub(n),
            Err(e) => {
                tracing::warn!("treaty-fetch: diet pages failed: {e:#}");
                errors.push(format!("diet pages: {e:#}"));
            }
        }
    }
    let stages = Kind::ALL.len() + usize::from(!opts.skip_diet);
    if errors.len() == stages {
        anyhow::bail!("treaty-fetch: no source reachable: {}", errors.join("; "));
    }
    fetch_texts(p.as_ref(), cache, budget)?;
    if opts.with_pdf {
        fetch_pdf_texts(p.as_ref(), cache, opts.pdf_limit)?;
    }
    Ok(())
}

#[derive(Serialize)]
struct IndexEntry<'a> {
    id: &'a str,
    title: &'a str,
    short_title: Option<&'a str>,
    kind: Option<Kind>,
    parties: &'a [String],
    signed_on: Option<&'a str>,
    diet_approved_on: Option<&'a str>,
    promulgated_on: Option<&'a str>,
    effective_on: Option<&'a str>,
    treaty_number: Option<u32>,
    treaty_number_label: Option<&'a str>,
    in_database: bool,
    has_text: bool,
    source_url: &'a str,
}

/// `.cache/treaty/` の取得結果を統合する (build-json とテストで共用)。
fn build_treaties(cache: &Path) -> Vec<Treaty> {
    let items = load_items(cache);
    let diets = load_diets(cache);
    let matched = match_diet_to_items(&items, &diets);
    let mut diet_by_item: HashMap<&str, &DietTreaty> = HashMap::new();
    for d in &diets {
        if let Some(id) = matched.get(&d.key) {
            diet_by_item.entry(id.as_str()).or_insert(d);
        }
    }
    let text = |id: &str| read_json::<TextRecord>(&text_path(cache, id));
    let mut out: Vec<Treaty> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for it in &items {
        if !seen.insert(it.id.clone()) {
            continue;
        }
        let t = text(&it.id);
        if let Some(tr) = merge(
            Some(it),
            diet_by_item.get(it.id.as_str()).copied(),
            t.as_ref(),
        ) {
            out.push(tr);
        }
    }
    for d in &diets {
        if matched.contains_key(&d.key) {
            continue;
        }
        let t = text(&format!("diet-{}", d.key));
        if let Some(tr) = merge(None, Some(d), t.as_ref()) {
            if seen.insert(tr.id.clone()) {
                out.push(tr);
            }
        }
    }
    // 新しく効力を生じた順 (未発効・日付不明は後ろ)。
    out.sort_by(|a, b| {
        b.effective_on
            .is_some()
            .cmp(&a.effective_on.is_some())
            .then(b.effective_on.cmp(&a.effective_on))
            .then(a.id.cmp(&b.id))
    });
    out
}

/// `lawpub treaty-build-json` — `.cache/treaty/` → `public/treaties/{id}.json` + `index.json`。
pub fn run_build_json(cache: &Path, public: &Path) -> Result<()> {
    let out = public.join("treaties");
    std::fs::create_dir_all(&out)?;
    let treaties = build_treaties(cache);
    for t in &treaties {
        std::fs::write(
            out.join(format!("{}.json", t.id)),
            serde_json::to_string_pretty(t)?,
        )?;
    }
    let entries: Vec<IndexEntry> = treaties
        .iter()
        .map(|t| IndexEntry {
            id: &t.id,
            title: &t.title,
            short_title: t.short_title.as_deref(),
            kind: t.kind,
            parties: &t.parties,
            signed_on: t.signed_on.as_deref(),
            diet_approved_on: t.diet_approved_on.as_deref(),
            promulgated_on: t.promulgated_on.as_deref(),
            effective_on: t.effective_on.as_deref(),
            treaty_number: t.treaty_number,
            treaty_number_label: t.treaty_number_label.as_deref(),
            in_database: t.in_database,
            has_text: t.text.is_some(),
            source_url: &t.source_url,
        })
        .collect();
    let with_text = entries.iter().filter(|e| e.has_text).count();
    let with_diet = treaties.iter().filter(|t| t.diet_url.is_some()).count();
    std::fs::write(
        out.join("index.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "sources": [SEARCH_TOP_URL, DIET_INDEX_URL],
            // PDL1.0 の出典記載。本文は対訳 HTML から和文の段を取り出した (PDF は pdftotext で
            // 抽出した) ものなので、加工した旨も添える。
            "attribution": "出典：外務省ウェブサイト「条約データ検索」「国会提出条約」（各条約の source_url）。本文は同サイトの HTML 版から和文を抽出、または PDF 版からテキストを抽出して加工したもの",
            "count": entries.len(),
            "treaties": entries,
        }))?,
    )?;
    tracing::info!(
        "treaty-build-json: {} treaties (text {with_text}, diet pages {with_diet})",
        entries.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("lawpub_treaty_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn opts() -> FetchOptions {
        FetchOptions {
            interval_ms: 0,
            max_pages: 0,
            list_max_age_days: 7,
            limit: 100,
            with_pdf: false,
            pdf_limit: 0,
            skip_diet: false,
        }
    }

    #[test]
    fn mock_fetch_then_build() {
        let root = tmp("mock");
        let cache = root.join("cache");
        let public = root.join("public");
        run_fetch(&cache, "mock", &opts()).unwrap();
        // 一覧は空ページで止まり、全ページ取得扱いになる。
        let bi: SearchCache = read_json(&search_cache_path(&cache, Kind::Bilateral)).unwrap();
        assert!(bi.complete);
        assert_eq!(bi.items.len(), 3);
        // 国会提出条約: 第217回のウクライナ租税条約・第162回の石綿条約は fixture がある。
        let uk: DietTreaty = read_json(&cache.join("treaty/diet/pagew_000001_00003.json")).unwrap();
        assert_eq!(uk.session, Some(217));
        assert!(cache.join("treaty/diet/treaty162_4.json").exists());

        run_build_json(&cache, &public).unwrap();
        let index: serde_json::Value =
            serde_json::from_slice(&std::fs::read(public.join("treaties/index.json")).unwrap())
                .unwrap();
        // 一覧 3 + 4 行、国会提出条約で一覧に無いもの 2 件。
        assert_eq!(index["count"], 9);
        let t: Treaty = read_json(&public.join("treaties/diet-pagew_000001_00003.json")).unwrap();
        assert_eq!(t.treaty_number_label.as_deref(), Some("令和7年条約第5号"));
        assert!(!t.in_database);
        assert_eq!(
            t.source_url,
            "https://www.mofa.go.jp/mofaj/ila/st/pagew_000001_00003.html"
        );
        let rcep: Treaty = read_json(&public.join("treaties/B-R4-001.json")).unwrap();
        assert_eq!(rcep.kind, Some(Kind::Multilateral));
        assert_eq!(rcep.attachments.len(), 7);
        assert_eq!(rcep.attachments[0].label, "本文 1");

        // 2 回目は一覧が新しいので取り直さず、公布・効力発生済みの条約ページも取り直さない。
        let before = std::fs::read(cache.join("treaty/diet/pagew_000001_00003.json")).unwrap();
        run_fetch(&cache, "mock", &opts()).unwrap();
        let after = std::fs::read(cache.join("treaty/diet/pagew_000001_00003.json")).unwrap();
        assert_eq!(before, after);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn text_records_merge_into_treaties() {
        let root = tmp("text");
        let cache = root.join("cache");
        write_json(
            &search_cache_path(&cache, Kind::Bilateral),
            &SearchCache {
                fetched_at: chrono::Utc::now().to_rfc3339(),
                complete: true,
                total: Some(1),
                items: vec![SearchItem {
                    id: "A-R4-101".into(),
                    kind: Kind::Bilateral,
                    title: "所得に対する租税に関する二重課税の回避のための日本国とスイスとの間の条約を改正する議定書".into(),
                    short_title: Some("スイスとの租税条約改正議定書".into()),
                    effective_on: Some("2022-11-30".into()),
                    parties: vec!["スイス".into()],
                    pdf_urls: vec!["https://www.mofa.go.jp/mofaj/gaiko/treaty/pdfs/A-R4-101.pdf".into()],
                    html_url: Some("https://www.mofa.go.jp/mofaj/gaiko/treaty/htmls/A-R4-101.html".into()),
                }],
            },
        )
        .unwrap();
        let mut o = opts();
        o.skip_diet = true;
        run_fetch(&cache, "mock", &o).unwrap();
        let treaties = build_treaties(&cache);
        let t = treaties.iter().find(|t| t.id == "A-R4-101").unwrap();
        assert_eq!(t.text_source.as_deref(), Some("html"));
        assert_eq!(t.text_lang.as_deref(), Some("ja"));
        assert!(t.text.as_deref().unwrap().contains("第一条"));
        assert_eq!(t.parties, vec!["スイス"]);
        std::fs::remove_dir_all(&root).ok();
    }
}
