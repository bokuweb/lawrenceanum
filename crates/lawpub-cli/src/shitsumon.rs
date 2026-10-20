use anyhow::{Context, Result};
use serde::Serialize;
use shitsumon_client::{House, HttpProvider, Shitsumon};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// `--session` 指定 (`222` / `219,220` / `210-219`) を回次の列にする。新しい回次から返す。
fn parse_sessions(spec: &str) -> Result<Vec<u32>> {
    let mut out = Vec::new();
    for part in spec.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        match part.split_once('-') {
            Some((a, b)) => {
                let (a, b): (u32, u32) = (
                    a.trim()
                        .parse()
                        .with_context(|| format!("invalid session {part:?}"))?,
                    b.trim()
                        .parse()
                        .with_context(|| format!("invalid session {part:?}"))?,
                );
                anyhow::ensure!(a <= b, "invalid session range {part:?}");
                out.extend(a..=b);
            }
            None => out.push(
                part.parse()
                    .with_context(|| format!("invalid session {part:?}"))?,
            ),
        }
    }
    out.sort_unstable_by(|a, b| b.cmp(a));
    out.dedup();
    Ok(out)
}

fn parse_houses(spec: &str) -> Result<Vec<House>> {
    if spec == "both" || spec.is_empty() {
        return Ok(vec![House::Shugiin, House::Sangiin]);
    }
    spec.split(',')
        .map(|h| {
            House::parse(h.trim())
                .with_context(|| format!("unknown house {h:?} (shugiin|sangiin|both)"))
        })
        .collect()
}

fn cache_path(cache: &Path, house: House, session: u32, number: u32) -> PathBuf {
    cache
        .join("shitsumon")
        .join(house.as_str())
        .join(session.to_string())
        .join(format!("{number}.json"))
}

/// `lawpub shitsumon-fetch` — 衆参の質問主意書・答弁書を回次ごとに取得し
/// `.cache/shitsumon/{house}/{session}/{number}.json` に保存する。
///
/// 答弁済み・撤回済みの保存分は飛ばし、未答弁の保存分は一覧に新しい本文が出たときだけ
/// 取り直す。`limit` で打ち切っても同じ引数で再実行すれば続きから取れる。
/// `sessions` を省略すると最新の 2 回次 (閉会後に答弁が届く前回次を含む)。
pub fn run_fetch(
    cache: &Path,
    sessions: Option<&str>,
    houses: &str,
    interval_ms: u64,
    limit: usize,
) -> Result<()> {
    let p = HttpProvider::new(Duration::from_millis(interval_ms))?;
    let sessions = match sessions {
        Some(spec) => parse_sessions(spec)?,
        None => {
            let latest = p.latest_session()?;
            vec![latest, latest.saturating_sub(1)]
        }
    };
    let houses = parse_houses(houses)?;
    let (mut saved, mut skipped, mut failed) = (0usize, 0usize, 0usize);
    'outer: for &session in &sessions {
        for &house in &houses {
            let Some(items) = p.list(house, session)? else {
                tracing::warn!(
                    "shitsumon-fetch: {} {session}: no list page",
                    house.as_str()
                );
                continue;
            };
            tracing::info!(
                "shitsumon-fetch: {} {session}: {} questions",
                house.as_str(),
                items.len()
            );
            for it in &items {
                let path = cache_path(cache, house, session, it.number);
                if let Ok(bytes) = std::fs::read(&path) {
                    if let Ok(prev) = serde_json::from_slice::<Shitsumon>(&bytes) {
                        if !prev.is_stale(it) {
                            skipped += 1;
                            continue;
                        }
                    }
                }
                if saved >= limit {
                    tracing::info!("shitsumon-fetch: reached --limit {limit}");
                    break 'outer;
                }
                match p.fetch(it) {
                    Ok(q) => {
                        std::fs::create_dir_all(path.parent().unwrap())?;
                        std::fs::write(&path, serde_json::to_string_pretty(&q)?)
                            .with_context(|| format!("write {}", path.display()))?;
                        saved += 1;
                    }
                    Err(e) => {
                        tracing::warn!("skip {}: {e:#}", it.id());
                        failed += 1;
                    }
                }
            }
        }
    }
    tracing::info!("shitsumon-fetch: saved={saved} unchanged={skipped} failed={failed}");
    Ok(())
}

#[derive(Serialize)]
struct IndexEntry<'a> {
    id: &'a str,
    house: House,
    session: u32,
    number: u32,
    title: &'a str,
    submitter: &'a str,
    submitted_on: Option<&'a str>,
    answered_on: Option<&'a str>,
    status: Option<&'a str>,
}

/// `lawpub shitsumon-build-json` — `.cache/shitsumon/` → `public/shitsumon/{id}.json` + `index.json`。
pub fn run_build_json(cache: &Path, public: &Path) -> Result<()> {
    let src = cache.join("shitsumon");
    let out = public.join("shitsumon");
    std::fs::create_dir_all(&out)?;
    let mut qs: Vec<Shitsumon> = Vec::new();
    if src.is_dir() {
        for entry in walkdir::WalkDir::new(&src) {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy();
            // exFAT/macOS の AppleDouble (._*.json) は飛ばす。
            if !entry.file_type().is_file() || !name.ends_with(".json") || name.starts_with("._") {
                continue;
            }
            let q: Shitsumon = serde_json::from_slice(&std::fs::read(path)?)
                .with_context(|| format!("parse {}", path.display()))?;
            qs.push(q);
        }
    }
    // 新しい順。同日は回次・番号の降順。
    qs.sort_by(|a, b| {
        (&b.submitted_on, b.session, b.number).cmp(&(&a.submitted_on, a.session, a.number))
    });
    for q in &qs {
        std::fs::write(
            out.join(format!("{}.json", q.id)),
            serde_json::to_string_pretty(q)?,
        )?;
    }
    let entries: Vec<IndexEntry> = qs
        .iter()
        .map(|q| IndexEntry {
            id: &q.id,
            house: q.house,
            session: q.session,
            number: q.number,
            title: &q.title,
            submitter: &q.submitter,
            submitted_on: q.submitted_on.as_deref(),
            answered_on: q.answered_on.as_deref(),
            status: q.status.as_deref(),
        })
        .collect();
    std::fs::write(
        out.join("index.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "sources": [
                "https://www.shugiin.go.jp/internet/itdb_shitsumon.nsf/html/shitsumon/menu_m.htm",
                "https://www.sangiin.go.jp/japanese/joho1/kousei/syuisyo/current/syuisyo.htm",
            ],
            "count": entries.len(),
            "questions": entries,
        }))?,
    )?;
    tracing::info!("shitsumon-build-json: {} questions", entries.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sessions_spec() {
        assert_eq!(parse_sessions("222").unwrap(), vec![222]);
        assert_eq!(parse_sessions("219,221").unwrap(), vec![221, 219]);
        assert_eq!(parse_sessions("217-219,218").unwrap(), vec![219, 218, 217]);
        assert!(parse_sessions("219-217").is_err());
        assert!(parse_sessions("abc").is_err());
    }

    #[test]
    fn houses_spec() {
        assert_eq!(
            parse_houses("both").unwrap(),
            vec![House::Shugiin, House::Sangiin]
        );
        assert_eq!(parse_houses("sangiin").unwrap(), vec![House::Sangiin]);
        assert!(parse_houses("x").is_err());
    }
}
