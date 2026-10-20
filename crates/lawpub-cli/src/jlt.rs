//! 日本法令外国語訳データベース (JLT) の英訳を取得し、e-Gov 法令 ID に対応付けて
//! `public/laws/{law_id}/en.json` と対応表 `public/jlt/index.json` を書き出す。
//!
//! 利用条件は PDL1.0 (出典記載・加工の旨の記載が条件)。詳細は `jlt-client` の crate doc と
//! `docs/jlt-translations.md`。

use anyhow::{Context, Result};
use jlt_client::{
    normalize_law_num, HttpProvider, JltLaw, JltProvider, ListEntry, MockProvider, ATTRIBUTION_JA,
    BASE_URL, CATEGORIES, DISCLAIMER_EN, DISCLAIMER_JA, PDL_URL, SOURCE_NAME_EN, SOURCE_NAME_JA,
    TERMS_URL,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn make_provider(provider: &str, interval_ms: u64) -> Result<Box<dyn JltProvider>> {
    Ok(match provider {
        "mock" => Box::new(MockProvider),
        _ => Box::new(HttpProvider::new(Duration::from_millis(interval_ms))?),
    })
}

#[derive(Debug, Serialize, Deserialize)]
struct ListCache {
    fetched_at: String,
    entries: Vec<ListEntry>,
}

/// 同じ翻訳版が複数分野に載るので、分野を和集合にしてまとめる。
fn merge(e: ListEntry, by_id: &mut BTreeMap<u32, ListEntry>) {
    let slot = by_id.entry(e.jlt_id).or_insert_with(|| ListEntry {
        categories: vec![],
        ..e.clone()
    });
    for c in e.categories {
        if !slot.categories.contains(&c) {
            slot.categories.push(c);
        }
    }
    slot.categories.sort_unstable();
}

fn list_path(cache: &Path) -> PathBuf {
    cache.join("jlt").join("list.json")
}

fn law_path(cache: &Path, jlt_id: u32) -> PathBuf {
    cache
        .join("jlt")
        .join("laws")
        .join(format!("{jlt_id}.json"))
}

fn parse_categories(spec: Option<&str>) -> Result<Vec<u32>> {
    let Some(spec) = spec.filter(|s| !s.trim().is_empty()) else {
        return Ok(CATEGORIES.to_vec());
    };
    spec.split(',')
        .map(|s| {
            let n: u32 = s
                .trim()
                .parse()
                .with_context(|| format!("invalid category {s:?}"))?;
            anyhow::ensure!(CATEGORIES.contains(&n), "unknown category {n} (2,4,…,4096)");
            Ok(n)
        })
        .collect()
}

/// `lawpub jlt-fetch` — 分野ごとに一覧を引いて `.cache/jlt/list.json` に保存し、
/// 未取得の翻訳版だけ本文を `.cache/jlt/laws/{jlt_id}.json` に保存する。
///
/// 翻訳版 ID は翻訳更新で新しくなるので、保存済み ID は取り直さない (`refresh` で強制)。
/// `limit` で打ち切っても同じ引数で再実行すれば続きから取れる。
/// 一覧取得に失敗した分野は前回の一覧の行を引き継ぎ、一時的な失敗で翻訳が配信から消えないようにする。
pub fn run_fetch(
    cache: &Path,
    provider: &str,
    categories: Option<&str>,
    interval_ms: u64,
    limit: usize,
    refresh: bool,
) -> Result<()> {
    let p = make_provider(provider, interval_ms)?;
    let cats = parse_categories(categories)?;
    let prev: Option<ListCache> = std::fs::read(list_path(cache))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());
    let mut by_id: BTreeMap<u32, ListEntry> = BTreeMap::new();
    for &c in &cats {
        match p.list(c) {
            Ok(entries) => {
                tracing::info!("jlt-fetch: category {c}: {} translations", entries.len());
                for e in entries {
                    merge(e, &mut by_id);
                }
            }
            Err(e) => {
                tracing::warn!("jlt-fetch: category {c} list failed: {e:#}");
                for old in prev
                    .iter()
                    .flat_map(|l| &l.entries)
                    .filter(|o| o.categories.contains(&c))
                {
                    merge(
                        ListEntry {
                            categories: vec![c],
                            ..old.clone()
                        },
                        &mut by_id,
                    );
                }
            }
        }
    }
    // 今回引かなかった分野 (`--categories` で絞ったとき) の行は前回の一覧から残す。
    for old in prev.iter().flat_map(|l| &l.entries) {
        for &c in old.categories.iter().filter(|c| !cats.contains(c)) {
            merge(
                ListEntry {
                    categories: vec![c],
                    ..old.clone()
                },
                &mut by_id,
            );
        }
    }
    let entries: Vec<ListEntry> = by_id.into_values().collect();
    std::fs::create_dir_all(cache.join("jlt").join("laws"))?;
    std::fs::write(
        list_path(cache),
        serde_json::to_string_pretty(&ListCache {
            fetched_at: chrono::Utc::now().to_rfc3339(),
            entries: entries.clone(),
        })?,
    )?;

    let (mut saved, mut skipped, mut failed) = (0usize, 0usize, 0usize);
    for e in &entries {
        let path = law_path(cache, e.jlt_id);
        if !refresh && path.exists() {
            skipped += 1;
            continue;
        }
        if saved >= limit {
            tracing::info!("jlt-fetch: reached --limit {limit}");
            break;
        }
        match p.fetch(e) {
            Ok(law) => {
                std::fs::write(&path, serde_json::to_string_pretty(&law)?)
                    .with_context(|| format!("write {}", path.display()))?;
                saved += 1;
            }
            Err(err) => {
                tracing::warn!("jlt-fetch: skip {} ({}): {err:#}", e.jlt_id, e.title);
                failed += 1;
            }
        }
    }
    tracing::info!(
        "jlt-fetch: {} translations listed, saved={saved} cached={skipped} failed={failed}",
        entries.len()
    );
    Ok(())
}

// ── build ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct LawsIndex {
    #[serde(default)]
    laws: Vec<LawsIndexEntry>,
}

#[derive(Deserialize)]
struct LawsIndexEntry {
    law_id: String,
    #[serde(default)]
    law_num: Option<String>,
    #[serde(default)]
    title: String,
}

#[derive(Deserialize)]
struct CurrentDoc {
    #[serde(default)]
    articles: Vec<CurrentArticle>,
}

#[derive(Deserialize)]
struct CurrentArticle {
    article_id: String,
    #[serde(default)]
    article_no: String,
}

/// e-Gov 法令番号 → (law_id, 題名) の索引。
struct EgovIndex {
    by_num: HashMap<String, Vec<(String, String)>>,
    /// 題名 → (law_id, 法令番号)。法令番号の誤記救済用。
    by_title: HashMap<String, Vec<(String, String)>>,
    by_id: HashMap<String, (Option<String>, String)>,
}

impl EgovIndex {
    fn load(public: &Path) -> Self {
        let laws: Vec<LawsIndexEntry> = std::fs::read(public.join("laws").join("index.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<LawsIndex>(&b).ok())
            .map(|i| i.laws)
            .unwrap_or_default();
        let mut by_num: HashMap<String, Vec<(String, String)>> = HashMap::new();
        let mut by_title: HashMap<String, Vec<(String, String)>> = HashMap::new();
        let mut by_id = HashMap::new();
        for l in laws {
            if let Some(n) = l.law_num.as_deref().filter(|n| !n.is_empty()) {
                by_num
                    .entry(normalize_law_num(n))
                    .or_default()
                    .push((l.law_id.clone(), l.title.clone()));
                by_title
                    .entry(base_title(&l.title))
                    .or_default()
                    .push((l.law_id.clone(), normalize_law_num(n)));
            }
            by_id.insert(l.law_id, (l.law_num, l.title));
        }
        Self {
            by_num,
            by_title,
            by_id,
        }
    }
}

/// JLT の題名から範囲書き (「（抄）」「 (第一編第二編第三編)」等) と空白を除く。
fn base_title(t: &str) -> String {
    let t = t.trim();
    let cut = t
        .find(" (")
        .or_else(|| t.find('（'))
        .or_else(|| t.find('('))
        .unwrap_or(t.len());
    t[..cut].chars().filter(|c| !c.is_whitespace()).collect()
}

/// 法令番号の末尾 `第…号` (発令者・年の誤記に左右されない部分)。
fn num_tail(n: &str) -> &str {
    n.rfind('第').map_or(n, |i| &n[i..])
}

/// 1 翻訳を e-Gov law_id に対応付ける。Ok は (law_id, 対応付けの根拠)、Err は未解決理由。
///
/// 1. 手動対応表 2. 法令番号の一致 (同番号が複数なら題名で絞る)
/// 3. 題名が一意に一致し、法令番号の `第…号` も一致する (JLT 側の発令者・年の誤記を救う。
///    全部改正で番号が変わった旧法の訳を新法に誤って付けないよう、番号の末尾までは一致を求める)。
fn resolve(
    index: &EgovIndex,
    overrides: &HashMap<u32, String>,
    e: &ListEntry,
) -> Result<(String, &'static str), String> {
    if let Some(id) = overrides.get(&e.jlt_id) {
        return Ok((id.clone(), "override"));
    }
    let num = e
        .law_num
        .as_deref()
        .ok_or_else(|| "no_law_num".to_string())?;
    let num = normalize_law_num(num);
    let want = base_title(&e.title);
    if let Some(cands) = index.by_num.get(&num) {
        if cands.len() == 1 {
            return Ok((cands[0].0.clone(), "law_num"));
        }
        let hits: Vec<_> = cands
            .iter()
            .filter(|(_, t)| base_title(t) == want)
            .collect();
        return match hits.as_slice() {
            [one] => Ok((one.0.clone(), "law_num")),
            _ => Err(format!("ambiguous_law_num ({} candidates)", cands.len())),
        };
    }
    match index.by_title.get(&want).map(Vec::as_slice) {
        Some([(id, egov_num)]) if num_tail(egov_num) == num_tail(&num) => Ok((id.clone(), "title")),
        _ => Err("law_num_not_in_egov".to_string()),
    }
}

/// jlt_id → law_id の手動対応 (法令番号で引けない翻訳用)。
fn load_overrides() -> HashMap<u32, String> {
    let raw: BTreeMap<String, serde_json::Value> =
        serde_json::from_str(jlt_client::LAW_ID_OVERRIDES).unwrap_or_default();
    raw.into_iter()
        .filter_map(|(k, v)| Some((k.parse().ok()?, v.get("law_id")?.as_str()?.to_string())))
        .collect()
}

#[derive(Serialize)]
struct Source {
    name_ja: &'static str,
    name_en: &'static str,
    url: &'static str,
    terms_url: &'static str,
    license: &'static str,
    license_url: &'static str,
    attribution: &'static str,
    disclaimer_ja: &'static str,
    disclaimer_en: &'static str,
}

const SOURCE: Source = Source {
    name_ja: SOURCE_NAME_JA,
    name_en: SOURCE_NAME_EN,
    url: BASE_URL,
    terms_url: TERMS_URL,
    license: "PDL1.0",
    license_url: PDL_URL,
    attribution: ATTRIBUTION_JA,
    disclaimer_ja: DISCLAIMER_JA,
    disclaimer_en: DISCLAIMER_EN,
};

#[derive(Serialize)]
struct TranslationMeta<'a> {
    jlt_id: u32,
    /// 一覧上の題名 (範囲書きを含む)。
    title_ja: &'a str,
    title_en: Option<&'a str>,
    law_num_en: Option<&'a str>,
    /// 翻訳が反映している最終改正。
    last_version_ja: Option<&'a str>,
    last_version_en: Option<&'a str>,
    translated_on: Option<&'a str>,
    dictionary_version: Option<&'a str>,
    tentative: bool,
    source_url: &'a str,
    article_count: usize,
    /// e-Gov 現行版の条 ID に一致した条数。
    matched_article_count: usize,
}

#[derive(Serialize)]
struct EnArticle<'a> {
    article_id: String,
    article_no: Option<&'a str>,
    title: Option<&'a str>,
    caption: Option<&'a str>,
    paragraphs: &'a [jlt_client::JltParagraph],
    jlt_id: u32,
    url: String,
}

#[derive(Serialize)]
struct EnDoc<'a> {
    schema_version: u32,
    law_id: &'a str,
    law_num: Option<&'a str>,
    title: &'a str,
    title_en: Option<&'a str>,
    source: &'a Source,
    translations: Vec<TranslationMeta<'a>>,
    articles: Vec<EnArticle<'a>>,
}

#[derive(Serialize)]
struct IndexRow<'a> {
    jlt_id: u32,
    title: &'a str,
    title_en: Option<&'a str>,
    law_type: Option<&'a str>,
    law_num: Option<&'a str>,
    translated_on: Option<&'a str>,
    tentative: bool,
    law_id: Option<String>,
    /// `law_num` / `title` / `override`。
    matched_by: Option<&'static str>,
    /// 未解決の理由 (`no_law_num` / `law_num_not_in_egov` / `ambiguous_law_num (...)`)。
    unresolved: Option<String>,
    /// 本文を取得済みか (未取得でも法令番号での対応付けは載せる)。
    fetched: bool,
    article_count: usize,
    matched_article_count: usize,
}

/// `lawpub jlt-build-json` — `.cache/jlt/` → `public/laws/{law_id}/en.json` + `public/jlt/index.json`。
///
/// 対応付けは法令番号 (`public/laws/index.json` の `law_num`) で行い、同じ番号が複数あれば題名で絞る。
/// 1 法令に複数の翻訳 (民法の第一〜三編と第四・五編など) があれば条単位でまとめ、
/// 同じ条は翻訳日の新しいほうを採る。
pub fn run_build_json(cache: &Path, public: &Path) -> Result<()> {
    let list: ListCache = match std::fs::read(list_path(cache)) {
        Ok(b) => serde_json::from_slice(&b).context("parse .cache/jlt/list.json")?,
        Err(_) => {
            tracing::warn!("jlt-build-json: no .cache/jlt/list.json — run jlt-fetch first");
            return Ok(());
        }
    };
    let index = EgovIndex::load(public);
    if index.by_id.is_empty() {
        tracing::warn!(
            "jlt-build-json: public/laws/index.json missing or empty — all translations unresolved"
        );
    }
    let overrides = load_overrides();

    let mut laws: Vec<JltLaw> = Vec::new();
    let mut rows_unfetched: Vec<&ListEntry> = Vec::new();
    for e in &list.entries {
        match std::fs::read(law_path(cache, e.jlt_id)) {
            Ok(b) => match serde_json::from_slice::<JltLaw>(&b) {
                Ok(mut law) => {
                    // 一覧の最新メタ (翻訳日・分野) を優先する。
                    law.entry = e.clone();
                    laws.push(law);
                }
                Err(err) => {
                    tracing::warn!("jlt-build-json: skip {}: {err:#}", e.jlt_id);
                    rows_unfetched.push(e);
                }
            },
            Err(_) => rows_unfetched.push(e),
        }
    }

    let mut by_law: BTreeMap<String, Vec<&JltLaw>> = BTreeMap::new();
    let mut resolved: HashMap<u32, Result<(String, &'static str), String>> = HashMap::new();
    for e in &list.entries {
        resolved.insert(e.jlt_id, resolve(&index, &overrides, e));
    }
    for law in &laws {
        if let Some(Ok((id, _))) = resolved.get(&law.jlt_id) {
            by_law.entry(id.clone()).or_default().push(law);
        }
    }

    let mut matched_counts: HashMap<u32, usize> = HashMap::new();
    let mut written = 0usize;
    for (law_id, mut group) in by_law {
        // 翻訳日の新しい順 (同日は ID の大きい順)。
        group.sort_by(|a, b| {
            (b.entry.translated_on.as_deref(), b.jlt_id)
                .cmp(&(a.entry.translated_on.as_deref(), a.jlt_id))
        });
        let dir = public.join("laws").join(&law_id);
        let current: Option<CurrentDoc> = std::fs::read(dir.join("current.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok());
        let egov_ids: BTreeSet<&str> = current
            .iter()
            .flat_map(|c| &c.articles)
            .map(|a| a.article_id.as_str())
            .collect();
        let egov_by_no: HashMap<&str, &str> = current
            .iter()
            .flat_map(|c| &c.articles)
            .map(|a| (a.article_no.as_str(), a.article_id.as_str()))
            .collect();
        let order: HashMap<&str, usize> = current
            .iter()
            .flat_map(|c| &c.articles)
            .enumerate()
            .map(|(i, a)| (a.article_id.as_str(), i))
            .collect();

        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut articles: Vec<EnArticle> = Vec::new();
        let mut metas = Vec::new();
        for law in &group {
            let mut matched = 0usize;
            for a in &law.articles {
                // 条名の文字列一致を優先し (e-Gov と同じ表記)、無ければ漢数字から導いた ID。
                let id = a
                    .article_no
                    .as_deref()
                    .and_then(|no| egov_by_no.get(no).map(|s| s.to_string()))
                    .or_else(|| a.article_id.clone());
                let Some(id) = id else { continue };
                if egov_ids.contains(id.as_str()) {
                    matched += 1;
                }
                if !seen.insert(id.clone()) {
                    continue;
                }
                articles.push(EnArticle {
                    url: match &a.anchor {
                        Some(anchor) => format!("{}#{anchor}", law.source_url),
                        None => law.source_url.clone(),
                    },
                    article_id: id,
                    article_no: a.article_no.as_deref(),
                    title: a.title.as_deref(),
                    caption: a.caption.as_deref(),
                    paragraphs: &a.paragraphs,
                    jlt_id: law.jlt_id,
                });
            }
            matched_counts.insert(law.jlt_id, matched);
            metas.push(TranslationMeta {
                jlt_id: law.jlt_id,
                title_ja: &law.entry.title,
                title_en: law.title_en.as_deref(),
                law_num_en: law.law_num_en.as_deref(),
                last_version_ja: law.last_version_ja.as_deref(),
                last_version_en: law.last_version_en.as_deref(),
                translated_on: law.entry.translated_on.as_deref(),
                dictionary_version: law.entry.dictionary_version.as_deref(),
                tentative: law.tentative,
                source_url: &law.source_url,
                article_count: law.articles.len(),
                matched_article_count: matched,
            });
        }
        if !order.is_empty() {
            // e-Gov の条順に並べ、現行版に無い条 (翻訳が旧版のまま) は末尾に回す。
            articles.sort_by_key(|a| {
                order
                    .get(a.article_id.as_str())
                    .copied()
                    .unwrap_or(usize::MAX)
            });
        }
        let (law_num, title) = index
            .by_id
            .get(&law_id)
            .map(|(n, t)| (n.as_deref(), t.as_str()))
            .unwrap_or((None, group[0].title_ja.as_str()));
        let doc = EnDoc {
            schema_version: 1,
            law_id: &law_id,
            law_num,
            title,
            title_en: group[0].title_en.as_deref(),
            source: &SOURCE,
            translations: metas,
            articles,
        };
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("en.json"), serde_json::to_string_pretty(&doc)?)
            .with_context(|| format!("write {}/en.json", dir.display()))?;
        written += 1;
    }

    let mut rows: Vec<IndexRow> = laws
        .iter()
        .map(|law| {
            let r = resolved
                .get(&law.jlt_id)
                .cloned()
                .unwrap_or(Err("unknown".into()));
            IndexRow {
                jlt_id: law.jlt_id,
                title: &law.entry.title,
                title_en: law.title_en.as_deref(),
                law_type: law.entry.law_type.as_deref(),
                law_num: law.entry.law_num.as_deref(),
                translated_on: law.entry.translated_on.as_deref(),
                tentative: law.tentative,
                law_id: r.as_ref().ok().map(|(id, _)| id.clone()),
                matched_by: r.as_ref().ok().map(|(_, how)| *how),
                unresolved: r.err(),
                fetched: true,
                article_count: law.articles.len(),
                matched_article_count: matched_counts.get(&law.jlt_id).copied().unwrap_or(0),
            }
        })
        .collect();
    rows.extend(rows_unfetched.iter().map(|e| {
        let r = resolved
            .get(&e.jlt_id)
            .cloned()
            .unwrap_or(Err("unknown".into()));
        IndexRow {
            jlt_id: e.jlt_id,
            title: &e.title,
            title_en: None,
            law_type: e.law_type.as_deref(),
            law_num: e.law_num.as_deref(),
            translated_on: e.translated_on.as_deref(),
            tentative: false,
            law_id: r.as_ref().ok().map(|(id, _)| id.clone()),
            matched_by: r.as_ref().ok().map(|(_, how)| *how),
            unresolved: r.err(),
            fetched: false,
            article_count: 0,
            matched_article_count: 0,
        }
    }));
    rows.sort_by(|a, b| (b.translated_on, b.jlt_id).cmp(&(a.translated_on, a.jlt_id)));
    let unresolved = rows.iter().filter(|r| r.law_id.is_none()).count();
    let total_articles: usize = rows.iter().map(|r| r.article_count).sum();
    let matched_articles: usize = rows.iter().map(|r| r.matched_article_count).sum();
    let out = public.join("jlt");
    std::fs::create_dir_all(&out)?;
    std::fs::write(
        out.join("index.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": 1,
            "generated_at": chrono::Utc::now().to_rfc3339(),
            "source": SOURCE,
            "count": rows.len(),
            "law_count": written,
            "unresolved_count": unresolved,
            "article_count": total_articles,
            "matched_article_count": matched_articles,
            "translations": rows,
        }))?,
    )?;
    tracing::info!(
        "jlt-build-json: {} translations → {written} laws (unresolved {unresolved}); articles matched {matched_articles}/{total_articles}",
        rows.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_titles() {
        assert_eq!(base_title("民法 (第一編第二編第三編)"), "民法");
        assert_eq!(base_title("国税通則法 (抄)"), "国税通則法");
        assert_eq!(base_title("民法（第一編第二編第三編）"), "民法");
        assert_eq!(base_title("関税法"), "関税法");
    }

    fn index_of(laws: &[(&str, &str, &str)]) -> EgovIndex {
        let tmp = std::env::temp_dir().join(format!(
            "lawpub-jlt-idx-{}-{}",
            std::process::id(),
            laws.len()
        ));
        std::fs::create_dir_all(tmp.join("laws")).unwrap();
        let rows: Vec<_> = laws
            .iter()
            .map(|(id, num, title)| serde_json::json!({"law_id": id, "law_num": num, "title": title}))
            .collect();
        std::fs::write(
            tmp.join("laws/index.json"),
            serde_json::json!({"laws": rows}).to_string(),
        )
        .unwrap();
        let idx = EgovIndex::load(&tmp);
        let _ = std::fs::remove_dir_all(&tmp);
        idx
    }

    fn entry(id: u32, num: Option<&str>, title: &str) -> ListEntry {
        ListEntry {
            jlt_id: id,
            title: title.to_string(),
            law_type: None,
            law_num: num.map(str::to_string),
            translated_on: None,
            dictionary_version: None,
            categories: vec![],
        }
    }

    #[test]
    fn resolves_by_law_num_title_and_override() {
        let idx = index_of(&[
            (
                "347M50002000041",
                "昭和四十七年労働省令第四十一号",
                "電離放射線障害防止規則",
            ),
            (
                "415CO0000000372",
                "平成十五年政令第三百七十二号",
                "次世代育成支援対策推進法施行令",
            ),
            ("416AC0000000001", "平成十六年法律第一号", "甲法"),
            ("416AC1000000001", "平成十六年法律第一号", "乙法"),
            ("508AC0000000010", "令和八年法律第十号", "新しい個人情報法"),
        ]);
        let none = HashMap::new();
        // 共同省令の区切りの違いは吸収、同番号は題名で絞る。
        assert_eq!(
            resolve(
                &idx,
                &none,
                &entry(1, Some("平成十六年法律第一号"), "乙法 (抄)")
            )
            .unwrap(),
            ("416AC1000000001".to_string(), "law_num")
        );
        assert!(
            resolve(&idx, &none, &entry(2, Some("平成十六年法律第一号"), "丙法"))
                .unwrap_err()
                .starts_with("ambiguous_law_num")
        );
        // JLT の発令者の誤記 (厚生労働省令 → 労働省令) と年の欠落は題名＋番号末尾で救う。
        assert_eq!(
            resolve(
                &idx,
                &none,
                &entry(
                    3,
                    Some("昭和四十七年厚生労働省令第四十一号"),
                    "電離放射線障害防止規則"
                )
            )
            .unwrap(),
            ("347M50002000041".to_string(), "title")
        );
        assert_eq!(
            resolve(
                &idx,
                &none,
                &entry(
                    4,
                    Some("政令第三百七十二号"),
                    "次世代育成支援対策推進法施行令"
                )
            )
            .unwrap()
            .1,
            "title"
        );
        // 題名が同じでも番号が違えば (全部改正前の旧法など) 付けない。
        assert_eq!(
            resolve(
                &idx,
                &none,
                &entry(5, Some("平成十五年法律第五十八号"), "新しい個人情報法")
            )
            .unwrap_err(),
            "law_num_not_in_egov"
        );
        assert_eq!(
            resolve(&idx, &none, &entry(6, None, "x")).unwrap_err(),
            "no_law_num"
        );
        let ov = HashMap::from([(7u32, "416AC0000000001".to_string())]);
        assert_eq!(
            resolve(&idx, &ov, &entry(7, None, "x")).unwrap().1,
            "override"
        );
    }

    #[test]
    fn categories_spec() {
        assert_eq!(parse_categories(None).unwrap().len(), 12);
        assert_eq!(parse_categories(Some("8,64")).unwrap(), vec![8, 64]);
        assert!(parse_categories(Some("3")).is_err());
    }

    #[test]
    fn mock_fetch_then_build_maps_by_law_num() {
        let tmp = std::env::temp_dir().join(format!("lawpub-jlt-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let cache = tmp.join("cache");
        let public = tmp.join("public");
        let law_dir = public.join("laws").join("129AC0000000089");
        std::fs::create_dir_all(&law_dir).unwrap();
        std::fs::write(
            public.join("laws").join("index.json"),
            r#"{"laws":[
                {"law_id":"129AC0000000089","law_num":"明治二十九年法律第八十九号","title":"民法"},
                {"law_id":"900AC0000000001","law_num":"テスト法律第一号","title":"アルファ法"}
            ]}"#,
        )
        .unwrap();
        std::fs::write(
            law_dir.join("current.json"),
            r#"{"articles":[
                {"article_id":"art_1","article_no":"第一条"},
                {"article_id":"art_3_2","article_no":"第三条の二"},
                {"article_id":"art_38:84","article_no":"第三十八条から第八十四条まで"}
            ]}"#,
        )
        .unwrap();

        run_fetch(&cache, "mock", None, 0, 10, false).unwrap();
        assert!(cache.join("jlt/laws/4848.json").exists());
        run_build_json(&cache, &public).unwrap();

        let en: serde_json::Value =
            serde_json::from_slice(&std::fs::read(law_dir.join("en.json")).unwrap()).unwrap();
        assert_eq!(en["law_id"], "129AC0000000089");
        assert_eq!(en["title_en"], "Civil Code");
        assert_eq!(en["source"]["license"], "PDL1.0");
        assert!(en["source"]["disclaimer_en"]
            .as_str()
            .unwrap()
            .contains("not official"));
        let arts = en["articles"].as_array().unwrap();
        // e-Gov の条順が先頭に来る。
        assert_eq!(arts[0]["article_id"], "art_1");
        assert_eq!(arts[1]["article_id"], "art_3_2");
        assert_eq!(arts[2]["article_id"], "art_38:84");
        assert_eq!(
            arts[0]["paragraphs"][0]["text"],
            "Private rights must be congruent with the public welfare."
        );
        assert!(arts[0]["url"]
            .as_str()
            .unwrap()
            .ends_with("/view/4848/je#je_pt1ch1at1"));
        assert_eq!(en["translations"][0]["matched_article_count"], 3);

        let idx: serde_json::Value =
            serde_json::from_slice(&std::fs::read(public.join("jlt/index.json")).unwrap()).unwrap();
        assert_eq!(idx["count"], 1);
        assert_eq!(idx["law_count"], 1);
        assert_eq!(idx["unresolved_count"], 0);
        assert_eq!(idx["translations"][0]["law_id"], "129AC0000000089");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn unresolved_when_law_num_unknown() {
        let tmp = std::env::temp_dir().join(format!("lawpub-jlt-unres-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let cache = tmp.join("cache");
        let public = tmp.join("public");
        std::fs::create_dir_all(public.join("laws")).unwrap();
        std::fs::write(public.join("laws/index.json"), r#"{"laws":[]}"#).unwrap();
        run_fetch(&cache, "mock", Some("8"), 0, 10, false).unwrap();
        run_build_json(&cache, &public).unwrap();
        let idx: serde_json::Value =
            serde_json::from_slice(&std::fs::read(public.join("jlt/index.json")).unwrap()).unwrap();
        assert_eq!(idx["unresolved_count"], 1);
        assert_eq!(idx["translations"][0]["unresolved"], "law_num_not_in_egov");
        assert!(!public.join("laws/129AC0000000089/en.json").exists());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
