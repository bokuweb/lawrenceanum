use anyhow::{Context, Result};
use hanrei_client::{CaseDetail, HttpProvider, Ymd};
use serde::Serialize;
use std::path::Path;
use std::time::Duration;

/// (分野別ページ番号, courtCaseType): 最高裁 / 下級裁。
const SOURCES: &[(u8, u8)] = &[(2, 1), (4, 3)];

/// `lawpub hanrei-fetch` — 「最近の裁判例」一覧から詳細を取得し
/// `.cache/hanrei/{case_id}.json` に保存する。取得済みの事件はスキップする。
///
/// `from`/`to` (YYYY-MM-DD) を両方指定すると、その裁判年月日の範囲を月ごとに区切って
/// 取得する (過去分のバックフィル)。保存済みの事件は飛ばすので、`limit` で打ち切っても
/// 同じ引数で再実行すれば続きから取れる。
pub fn run_fetch(
    cache: &Path,
    interval_ms: u64,
    limit: usize,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<()> {
    let p = HttpProvider::new(Duration::from_millis(interval_ms))?;
    let dir = cache.join("hanrei");
    std::fs::create_dir_all(&dir)?;
    let mut saved = 0usize;
    // 保存したら true。保存済み・取得失敗は false。
    let save = |it: &hanrei_client::CaseListItem| -> Result<bool> {
        let path = dir.join(format!("{}.json", it.case_id));
        if path.exists() {
            return Ok(false);
        }
        match p.fetch_detail(it.case_id) {
            Ok(mut d) => {
                if let Err(e) = fill_body(&mut d, |url| p.fetch_body_text(url)) {
                    tracing::warn!("pdf {}: {e:#}", it.case_id);
                }
                std::fs::write(&path, serde_json::to_string_pretty(&d)?)
                    .with_context(|| format!("write {}", path.display()))?;
                Ok(true)
            }
            Err(e) => {
                tracing::warn!("skip {}: {e:#}", it.case_id);
                Ok(false)
            }
        }
    };
    match (from, to) {
        (None, None) => {
            for &(page, kind) in SOURCES {
                let items = p.fetch_recent(page, kind)?;
                tracing::info!("hanrei-fetch: search{page} listed {} cases", items.len());
                for it in items {
                    if saved >= limit {
                        break;
                    }
                    saved += save(&it)? as usize;
                }
            }
        }
        (Some(f), Some(t)) => {
            let (f, t) = (parse_ymd(f)?, parse_ymd(t)?);
            'outer: for (wf, wt) in month_windows(f, t) {
                for &(page, kind) in SOURCES {
                    let mut offset = 0usize;
                    loop {
                        let r = p.fetch_range_page(page, kind, wf, wt, offset)?;
                        if offset == 0 {
                            tracing::info!(
                                "hanrei-fetch: search{page} {}-{:02}: {} cases",
                                wf.y,
                                wf.m,
                                r.total
                            );
                            if r.total > SITE_LIMIT {
                                tracing::warn!(
                                    "{}-{:02} search{page}: {} > {SITE_LIMIT}; excess is not reachable",
                                    wf.y,
                                    wf.m,
                                    r.total
                                );
                            }
                        }
                        for it in &r.items {
                            if saved >= limit {
                                break 'outer;
                            }
                            saved += save(it)? as usize;
                        }
                        offset += hanrei_client::PAGE_SIZE;
                        if r.items.is_empty() || offset >= r.total.min(SITE_LIMIT) {
                            break;
                        }
                    }
                }
            }
        }
        _ => anyhow::bail!("--from and --to must be given together"),
    }
    tracing::info!("hanrei-fetch: {} cases saved", saved);
    Ok(())
}

/// 未取得本文だけ補完する。空の抽出結果は取得済みとして記録し、失敗は再試行可能にする。
fn fill_body(d: &mut CaseDetail, fetch: impl FnOnce(&str) -> Result<String>) -> Result<bool> {
    if d.body_text.is_some() {
        return Ok(false);
    }
    let Some(url) = d.pdf_url.as_deref() else {
        return Ok(false);
    };
    let text = fetch(url)?;
    d.body_citations = hanrei_client::extract_citations(&text);
    d.body_text = Some(text);
    Ok(true)
}

/// 古いキャッシュの PDF 本文を新しい事件から補完する。上限は成功数ではなく試行数。
/// 新規事件の取得とは別に実行でき、--limit で少しずつ移行する。
pub fn run_fetch_bodies(cache: &Path, interval_ms: u64, limit: usize) -> Result<()> {
    let p = HttpProvider::new(Duration::from_millis(interval_ms))?;
    let dir = cache.join("hanrei");
    let mut files = Vec::new();
    if dir.is_dir() {
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.extension().and_then(|x| x.to_str()) == Some("json") {
                if let Some(id) = path
                    .file_stem()
                    .and_then(|x| x.to_str())
                    .and_then(|x| x.parse::<u64>().ok())
                {
                    files.push((id, path));
                }
            }
        }
    }
    files.sort_by_key(|(id, _)| std::cmp::Reverse(*id));
    let (mut attempted, mut saved) = (0, 0);
    for (id, path) in files {
        if attempted >= limit {
            break;
        }
        let mut d: CaseDetail = serde_json::from_slice(&std::fs::read(&path)?)
            .with_context(|| format!("parse {}", path.display()))?;
        if d.body_text.is_some() || d.pdf_url.is_none() {
            continue;
        }
        attempted += 1;
        match fill_body(&mut d, |url| p.fetch_body_text(url)) {
            Ok(true) => {
                // Replace only once extraction has succeeded; a failed fetch leaves the cache intact.
                let tmp = path.with_extension("json.tmp");
                std::fs::write(&tmp, serde_json::to_vec_pretty(&d)?)?;
                std::fs::rename(tmp, &path)?;
                saved += 1;
            }
            Ok(false) => {}
            Err(e) => tracing::warn!("hanrei PDF {id}: {e:#}"),
        }
    }
    tracing::info!("hanrei-fetch-bodies: {saved}/{attempted} cases updated");
    Ok(())
}

/// 1 回の検索で辿れる上限 (サイト側の仕様)。
const SITE_LIMIT: usize = 2000;

fn parse_ymd(s: &str) -> Result<Ymd> {
    let mut it = s.split('-');
    let mut next = |what: &str| -> Result<u32> {
        it.next()
            .and_then(|v| v.parse().ok())
            .with_context(|| format!("invalid {what} in {s:?} (expected YYYY-MM-DD)"))
    };
    let (y, m, d) = (next("year")?, next("month")?, next("day")?);
    anyhow::ensure!(
        (1..=12).contains(&m) && (1..=31).contains(&d),
        "invalid date {s:?}"
    );
    Ok(Ymd { y: y as i32, m, d })
}

fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// `from..=to` を暦月ごとの (開始, 終了) に分ける。新しい月から順に返す
/// (最近の判例を先に集める)。
fn month_windows(from: Ymd, to: Ymd) -> Vec<(Ymd, Ymd)> {
    let mut out = Vec::new();
    let (mut y, mut m) = (to.y, to.m);
    while (y, m) >= (from.y, from.m) {
        let start = if (y, m) == (from.y, from.m) {
            from.d
        } else {
            1
        };
        let end = if (y, m) == (to.y, to.m) {
            to.d
        } else {
            days_in_month(y, m)
        };
        out.push((Ymd { y, m, d: start }, Ymd { y, m, d: end }));
        if m == 1 {
            (y, m) = (y - 1, 12);
        } else {
            m -= 1;
        }
    }
    out
}

#[derive(Serialize)]
struct IndexEntry {
    case_id: u64,
    case_number: String,
    case_name: String,
    judged_on: String,
    court: String,
    result: String,
    holding: String,
}

/// `lawpub hanrei-build-json` — `.cache/hanrei/` → `public/hanrei/{case_id}.json` + `index.json`。
pub fn run_build_json(cache: &Path, public: &Path) -> Result<()> {
    let src = cache.join("hanrei");
    let out = public.join("hanrei");
    std::fs::create_dir_all(&out)?;
    // 本文全件をメモリに保持せず、一覧用のメタデータだけ集める。
    let mut entries = Vec::new();
    if src.is_dir() {
        for entry in std::fs::read_dir(&src)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json")
                || path
                    .file_stem()
                    .and_then(|e| e.to_str())
                    .and_then(|e| e.parse::<u64>().ok())
                    .is_none()
            {
                continue;
            }
            let d: CaseDetail = serde_json::from_slice(&std::fs::read(&path)?)
                .with_context(|| format!("parse {}", path.display()))?;
            std::fs::write(
                out.join(format!("{}.json", d.case_id)),
                serde_json::to_vec_pretty(&d)?,
            )?;
            entries.push(IndexEntry {
                case_id: d.case_id,
                case_number: d.case_number,
                case_name: d.case_name,
                judged_on: d.judged_on,
                court: d.court,
                result: d.result,
                holding: d.holding,
            });
        }
    }
    entries.sort_by_key(|d| std::cmp::Reverse(d.case_id));
    std::fs::write(
        out.join("index.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "source": "https://www.courts.go.jp/hanrei/search1/index.html",
            "count": entries.len(),
            "cases": entries,
        }))?,
    )?;
    tracing::info!("hanrei-build-json: {} cases", entries.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legacy_case() -> CaseDetail {
        hanrei_client::parse_detail(
            "<dl><dt>参照法条</dt><dd>民法709条</dd></dl><a href='/case.pdf'>全文</a>",
            "https://www.courts.go.jp/hanrei/123/detail2/index.html",
        )
        .unwrap()
    }

    #[test]
    fn fills_body_even_with_referenced_laws_and_keeps_successful_extractions() {
        let mut d = legacy_case();
        assert!(!d.referenced_laws.is_empty());
        assert!(fill_body(&mut d, |_| Ok("主文\n民法７０９条による。".into())).unwrap());
        assert_eq!(d.body_text.as_deref(), Some("主文\n民法７０９条による。"));
        assert!(d.body_citations.iter().any(|s| s.contains("民法７０９条")));
        assert!(!fill_body(&mut d, |_| panic!("must not refetch")).unwrap());
        d.body_text = Some(String::new());
        assert!(!fill_body(&mut d, |_| panic!("image-only PDF must not refetch")).unwrap());
    }

    #[test]
    fn failed_body_fetch_preserves_cache_and_can_retry() {
        let mut d = legacy_case();
        let before = d.clone();
        assert!(fill_body(&mut d, |_| anyhow::bail!("offline")).is_err());
        assert_eq!(d, before);
        assert!(fill_body(&mut d, |_| Ok(String::new())).unwrap());
        assert_eq!(d.body_text.as_deref(), Some(""));
    }

    #[test]
    fn build_json_preserves_body_but_keeps_index_compact() {
        let root = std::env::temp_dir().join(format!("hanrei_build_body_{}", std::process::id()));
        let cache = root.join("cache");
        let public = root.join("public");
        std::fs::create_dir_all(cache.join("hanrei")).unwrap();
        for id in [123, 456] {
            let mut d = legacy_case();
            d.case_id = id;
            d.body_text = Some("主文\n民法７０９条による。".into());
            std::fs::write(
                cache.join(format!("hanrei/{id}.json")),
                serde_json::to_vec(&d).unwrap(),
            )
            .unwrap();
        }
        run_build_json(&cache, &public).unwrap();
        let detail: CaseDetail =
            serde_json::from_slice(&std::fs::read(public.join("hanrei/123.json")).unwrap())
                .unwrap();
        assert_eq!(
            detail.body_text.as_deref(),
            Some("主文\n民法７０９条による。")
        );
        let index: serde_json::Value =
            serde_json::from_slice(&std::fs::read(public.join("hanrei/index.json")).unwrap())
                .unwrap();
        assert_eq!(index["count"], 2);
        assert_eq!(index["cases"][0]["case_id"], 456);
        assert!(index["cases"][0].get("body_text").is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn windows_are_months_newest_first() {
        let w = month_windows(
            parse_ymd("2024-11-15").unwrap(),
            parse_ymd("2025-02-10").unwrap(),
        );
        let got: Vec<_> = w.iter().map(|(a, b)| (a.y, a.m, a.d, b.d)).collect();
        assert_eq!(
            got,
            vec![
                (2025, 2, 1, 10),
                (2025, 1, 1, 31),
                (2024, 12, 1, 31),
                (2024, 11, 15, 30)
            ]
        );
    }

    #[test]
    fn leap_february() {
        let w = month_windows(
            parse_ymd("2024-02-01").unwrap(),
            parse_ymd("2024-02-29").unwrap(),
        );
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].1.d, 29);
    }

    #[test]
    fn rejects_bad_dates() {
        assert!(parse_ymd("2024-13-01").is_err());
        assert!(parse_ymd("2024-01").is_err());
    }
}
