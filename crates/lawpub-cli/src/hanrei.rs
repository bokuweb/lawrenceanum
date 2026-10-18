use anyhow::{Context, Result};
use hanrei_client::{CaseDetail, HttpProvider};
use serde::Serialize;
use std::path::Path;
use std::time::Duration;

/// (分野別ページ番号, courtCaseType): 最高裁 / 下級裁。
const SOURCES: &[(u8, u8)] = &[(2, 1), (4, 3)];

/// `lawpub hanrei-fetch` — 「最近の裁判例」一覧から詳細を取得し
/// `.cache/hanrei/{case_id}.json` に保存する。取得済みの事件はスキップする。
pub fn run_fetch(cache: &Path, interval_ms: u64, limit: usize) -> Result<()> {
    let p = HttpProvider::new(Duration::from_millis(interval_ms))?;
    let dir = cache.join("hanrei");
    std::fs::create_dir_all(&dir)?;
    let mut saved = 0usize;
    for &(page, kind) in SOURCES {
        let items = p.fetch_recent(page, kind)?;
        tracing::info!("hanrei-fetch: search{page} listed {} cases", items.len());
        for it in items {
            if saved >= limit {
                break;
            }
            let path = dir.join(format!("{}.json", it.case_id));
            if path.exists() {
                continue;
            }
            match p.fetch_detail(it.case_id) {
                Ok(d) => {
                    std::fs::write(&path, serde_json::to_string_pretty(&d)?)
                        .with_context(|| format!("write {}", path.display()))?;
                    saved += 1;
                }
                Err(e) => tracing::warn!("skip {}: {e:#}", it.case_id),
            }
        }
    }
    tracing::info!("hanrei-fetch: {saved} cases saved");
    Ok(())
}

#[derive(Serialize)]
struct IndexEntry<'a> {
    case_id: u64,
    case_number: &'a str,
    case_name: &'a str,
    judged_on: &'a str,
    court: &'a str,
    result: &'a str,
    holding: &'a str,
}

/// `lawpub hanrei-build-json` — `.cache/hanrei/` → `public/hanrei/{case_id}.json` + `index.json`。
pub fn run_build_json(cache: &Path, public: &Path) -> Result<()> {
    let src = cache.join("hanrei");
    let out = public.join("hanrei");
    std::fs::create_dir_all(&out)?;
    let mut cases: Vec<CaseDetail> = Vec::new();
    if src.is_dir() {
        for entry in std::fs::read_dir(&src)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let d: CaseDetail = serde_json::from_slice(&std::fs::read(&path)?)
                .with_context(|| format!("parse {}", path.display()))?;
            cases.push(d);
        }
    }
    // case_id は採番順 (新しいほど大きい)。新しい順に並べる。
    cases.sort_by(|a, b| b.case_id.cmp(&a.case_id));
    for d in &cases {
        std::fs::write(
            out.join(format!("{}.json", d.case_id)),
            serde_json::to_string_pretty(d)?,
        )?;
    }
    let entries: Vec<IndexEntry> = cases
        .iter()
        .map(|d| IndexEntry {
            case_id: d.case_id,
            case_number: &d.case_number,
            case_name: &d.case_name,
            judged_on: &d.judged_on,
            court: &d.court,
            result: &d.result,
            holding: &d.holding,
        })
        .collect();
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
