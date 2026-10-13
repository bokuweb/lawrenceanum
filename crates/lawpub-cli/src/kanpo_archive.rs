//! `lawpub kanpo-archive` — デジタル官報の全ページ PDF を保存する。
//!
//! デジタル官報は直近約 90 日しか公開されないため、パーサの検証・再抽出用に原本を手元
//! （CI では R2）へ蓄積する。改め文抽出 (`kanpo-fetch`) と違い、項目の種類を問わず号の
//! 全ページを対象にし、`pdftotext -bbox-layout` の座標付きテキストも並べて保存する
//! （playground がブラウザで解析し直せるように）。
//!
//! 出力レイアウト:
//!   {out}/{date}/manifest.json
//!   {out}/{date}/{号dir}/{号dir}{PPPP}.pdf
//!   {out}/{date}/{号dir}/{号dir}{PPPP}.bbox.html

use anyhow::{Context, Result};
use kanpo_client::{issue_dir, pdf, HttpProvider, KanpoProvider};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::Duration;

#[derive(Serialize)]
struct Manifest {
    date: String,
    issues: Vec<IssueEntry>,
}

#[derive(Serialize)]
struct IssueEntry {
    /// "regular" | "extra" | "special_extra" 等。
    issue_type: String,
    issue_no: String,
    /// 号ディレクトリ名（例: `20261005h01801`）。ページファイルはこの下に置く。
    dir: String,
    url: String,
    page_count: u32,
    pages: Vec<PageEntry>,
    /// 目次の項目（標題と開始ページ）。
    items: Vec<ItemEntry>,
}

#[derive(Serialize)]
struct PageEntry {
    page: u32,
    /// `{dir}/{file}` の file（拡張子 `.pdf`）。座標付きテキストは `.bbox.html`。
    file: String,
    sha256: String,
    bytes: usize,
    /// 座標付きテキストを作れたか（poppler が無い・PDF が壊れている場合は false）。
    bbox: bool,
}

#[derive(Serialize)]
struct ItemEntry {
    title: String,
    page: u32,
}

pub fn run(date: &str, out: &Path, interval_ms: u64) -> Result<()> {
    let http = HttpProvider::new()?;
    tracing::info!("archive kanpo for {date} from {}", http.base_url());
    let kd = http.fetch_date(date)?;
    let day_dir = out.join(date);
    std::fs::create_dir_all(&day_dir)?;

    let (mut fetched, mut reused, mut failed) = (0usize, 0usize, 0usize);
    let mut issues = Vec::new();
    for issue in &kd.issues {
        let Some(dir) = issue_dir(&issue.pdf_url) else {
            tracing::warn!("skip issue without dir: {}", issue.pdf_url);
            continue;
        };
        // 号の総ページ数が取れなければ、目次の最終項目の開始ページまでで妥協する。
        let page_count = issue
            .page_count
            .or_else(|| issue.items.iter().map(|i| i.page).max())
            .unwrap_or(0);
        let issue_dir = day_dir.join(&dir);
        std::fs::create_dir_all(&issue_dir)?;
        let mut pages = Vec::new();
        for n in 1..=page_count {
            let file = format!("{dir}{n:04}.pdf");
            let path = issue_dir.join(&file);
            let bytes = match std::fs::read(&path) {
                Ok(b) if !b.is_empty() => {
                    reused += 1;
                    b
                }
                _ => {
                    let url = format!("{}pdf/{file}", issue.pdf_url);
                    std::thread::sleep(Duration::from_millis(interval_ms));
                    match http.get_bytes(&url) {
                        Ok(b) => {
                            std::fs::write(&path, &b).with_context(|| format!("write {}", path.display()))?;
                            fetched += 1;
                            b
                        }
                        Err(e) => {
                            tracing::warn!("download failed {url}: {e}");
                            failed += 1;
                            continue;
                        }
                    }
                }
            };
            let bbox_path = path.with_extension("bbox.html");
            let bbox = bbox_path.exists()
                || match pdf::bbox_xhtml(&bytes) {
                    Ok(x) => std::fs::write(&bbox_path, x).is_ok(),
                    Err(e) => {
                        tracing::warn!("pdftotext failed {}: {e}", path.display());
                        false
                    }
                };
            pages.push(PageEntry {
                page: n,
                file,
                sha256: hex::encode(Sha256::digest(&bytes)),
                bytes: bytes.len(),
                bbox,
            });
        }
        issues.push(IssueEntry {
            issue_type: issue.issue_type.clone(),
            issue_no: issue.issue_no.clone(),
            dir,
            url: issue.pdf_url.clone(),
            page_count,
            pages,
            items: issue
                .items
                .iter()
                .map(|i| ItemEntry {
                    title: i.title.clone(),
                    page: i.page,
                })
                .collect(),
        });
    }
    tracing::info!("archived {date}: fetched={fetched} reused={reused} failed={failed}");
    // 取りこぼしがある日は manifest を書かない（呼び出し側が未完了として次回やり直す）。
    if failed > 0 {
        anyhow::bail!("{failed} pages failed for {date}; manifest not written");
    }
    let manifest = Manifest {
        date: date.to_string(),
        issues,
    };
    let path = day_dir.join("manifest.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&manifest)?)?;
    tracing::info!("wrote {}", path.display());
    Ok(())
}
