//! `lawpub kanpo-archive` — デジタル官報の全ページ PDF を保存する。
//!
//! デジタル官報は直近約 90 日しか公開されないため、パーサの検証・再抽出用に原本を手元
//! （CI では R2）へ蓄積する。項目単位の本文抽出 (`kanpo-fetch`) と違い、目次に無いページも含めて号の
//! 全ページを対象にし、`pdftotext -bbox-layout` の座標付きテキストも並べて保存する
//! （playground がブラウザで解析し直せるように）。
//!
//! 出力レイアウト:
//!   {out}/{date}/manifest.json
//!   {out}/{date}/{号dir}/{号dir}{PPPP}.pdf
//!   {out}/{date}/{号dir}/{号dir}{PPPP}.bbox.html
//!   {out}/{date}/{号dir}/{号dir}{PPPP}.rules.json   罫線（表の復元用）
//!
//! `lawpub kanpo-reextract` (`run_reextract`) はこの原本から改め文を抽出し直し、`kanpo-fetch` と同じ形の
//! `.cache/kanpo/{date}.json` を書く。

use anyhow::{Context, Result};
use kanpo_client::{issue_dir, pdf, HttpProvider, KanpoProvider};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::Duration;

#[derive(Serialize, Deserialize)]
struct Manifest {
    date: String,
    issues: Vec<IssueEntry>,
}

#[derive(Serialize, Deserialize)]
struct IssueEntry {
    /// "regular" | "extra" | "special_extra" | "procurement" 等。
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

#[derive(Serialize, Deserialize)]
struct PageEntry {
    page: u32,
    /// `{dir}/{file}` の file（拡張子 `.pdf`）。座標付きテキストは `.bbox.html`。
    file: String,
    sha256: String,
    bytes: usize,
    /// 座標付きテキストを作れたか（poppler が無い・PDF が壊れている場合は false）。
    bbox: bool,
    /// 罫線（`.rules.json`）を作れたか。
    rules: bool,
}

#[derive(Serialize, Deserialize)]
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
            let rules_path = path.with_extension("rules.json");
            let rules = rules_path.exists()
                || match pdf::page_rules(&bytes) {
                    Ok(r) => std::fs::write(&rules_path, serde_json::to_vec(&r)?).is_ok(),
                    Err(e) => {
                        tracing::warn!("pdftocairo failed {}: {e}", path.display());
                        false
                    }
                };
            pages.push(PageEntry {
                page: n,
                file,
                sha256: hex::encode(Sha256::digest(&bytes)),
                bytes: bytes.len(),
                bbox,
                rules,
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

// ── 原本からの再抽出 ────────────────────────────────────────────────

/// `kanpo-archive` の出力の置き場所。R2 の公開 URL (`https://…/kanpo-pdf`) か、手元のディレクトリ。
struct Archive {
    base: String,
    client: Option<reqwest::blocking::Client>,
}

impl Archive {
    fn new(base: &str) -> Result<Self> {
        let base = base.trim_end_matches('/').to_string();
        let client = if base.starts_with("http://") || base.starts_with("https://") {
            Some(reqwest::blocking::Client::builder().timeout(Duration::from_secs(60)).build()?)
        } else {
            None
        };
        Ok(Self { base, client })
    }

    /// 無ければ None。
    fn get(&self, rel: &str) -> Result<Option<Vec<u8>>> {
        match &self.client {
            Some(client) => {
                let url = format!("{}/{rel}", self.base);
                let mut last = None;
                for attempt in 0..3 {
                    match client.get(&url).send() {
                        Ok(r) if r.status() == reqwest::StatusCode::NOT_FOUND => return Ok(None),
                        Ok(r) => match r.error_for_status().and_then(|r| r.bytes()) {
                            Ok(b) => return Ok(Some(b.to_vec())),
                            Err(e) => last = Some(anyhow::anyhow!(e)),
                        },
                        Err(e) => last = Some(anyhow::anyhow!(e)),
                    }
                    std::thread::sleep(Duration::from_millis(500 * (attempt + 1)));
                }
                Err(last.unwrap()).with_context(|| format!("GET {url}"))
            }
            None => {
                let path = Path::new(&self.base).join(rel);
                if !path.exists() {
                    return Ok(None);
                }
                Ok(Some(std::fs::read(&path).with_context(|| format!("read {}", path.display()))?))
            }
        }
    }
}

/// manifest の号の項目のうち、その号の項目。目次は号の項目をページ順に並べるので、ページが前に戻った項目から後ろは
/// 別の号の項目である。目次の「政府調達」の見出しを号として読まなかった頃の manifest は、号外の後ろに政府調達の号の
/// 項目（「入札公告　1」…）を続けて持ち、そのページは政府調達の号のページで、この号のページではない。
fn own_items(issue: &IssueEntry) -> &[ItemEntry] {
    let end = (1..issue.items.len()).find(|&k| issue.items[k].page < issue.items[k - 1].page).unwrap_or(issue.items.len());
    if end < issue.items.len() {
        tracing::warn!("{}: {} item(s) after page {} belong to another issue; skipped", issue.dir, issue.items.len() - end, issue.items[end - 1].page);
    }
    &issue.items[..end]
}

/// manifest から、デジタル官報の目次を取ったときと同じ形の `KanpoDate` を組み立てる。
fn kanpo_date(manifest: &Manifest) -> kanpo_client::KanpoDate {
    let issues = manifest
        .issues
        .iter()
        .map(|i| (i, own_items(i)))
        .map(|(i, items)| kanpo_client::KanpoIssue {
            issue_type: i.issue_type.clone(),
            issue_no: i.issue_no.clone(),
            pdf_url: i.url.clone(),
            sha256: None,
            promulgation_date: manifest.date.clone(),
            law_nums: Vec::new(),
            titles: items.iter().map(|it| it.title.clone()).collect(),
            items: items
                .iter()
                .map(|it| kanpo_client::KanpoItem {
                    title: it.title.clone(),
                    page: it.page,
                    pdf_url: format!("{}pdf/{}{:04}.pdf", i.url, i.dir, it.page),
                    sha256: None,
                    agency_hint: kanpo_client::extract_agency_hint(&it.title),
                    amend_text: None,
                    amend_format: None,
                    amend_document: None,
                })
                .collect(),
            page_count: Some(i.page_count),
        })
        .collect();
    kanpo_client::KanpoDate { date: manifest.date.clone(), issues }
}

/// 1 日分を原本から抽出し直す。戻り値は (項目数, 記事を特定できなかった項目数, 本文が取れた項目数, 読んだページ数)。
fn reextract_date(archive: &Archive, date: &str, cache: &Path) -> Result<Option<(usize, usize, usize, usize)>> {
    let Some(bytes) = archive.get(&format!("{date}/manifest.json"))? else {
        return Ok(None);
    };
    let manifest: Manifest = serde_json::from_slice(&bytes).with_context(|| format!("parse {date}/manifest.json"))?;
    let mut kd = kanpo_date(&manifest);
    // ページ PDF の URL (`…/pdf/{dir}{PPPP}.pdf`) → 原本の `{date}/{dir}/{dir}{PPPP}.pdf`。座標付きテキストと
    // 罫線は今の kanpo-pdf で PDF から作り直す（蓄積した `.bbox.html`・`.rules.json` は取得した日の抽出器の
    // 出力で、その後の kanpo-pdf の改善（ToUnicode の無いフォントの字の判別など）が入っていない。全 10,784
    // ページで比べると 18% のページの本文が変わり、どれも原本に近づく）。PDF が無ければ蓄積したものを読む。
    let mut read = 0usize;
    let mut load = |url: &str| -> Option<Option<pdf::PageInput>> {
        let stem = url.rsplit('/').next()?.strip_suffix(".pdf")?;
        let dir = stem.get(..stem.len().checked_sub(4)?)?;
        let fetch = |ext: &str| archive.get(&format!("{date}/{dir}/{stem}.{ext}"));
        let warn = |e: anyhow::Error| tracing::warn!("{e:#}");
        if let Some(bytes) = fetch("pdf").unwrap_or_else(|e| {
            warn(e);
            None
        }) {
            match pdf::bbox_xhtml(&bytes) {
                Ok(xhtml) => {
                    read += 1;
                    return Some(Some(pdf::PageInput { xhtml, rules: pdf::page_rules(&bytes).ok() }));
                }
                Err(e) => warn(e.context(format!("{stem}.pdf"))),
            }
        }
        let xhtml = match fetch("bbox.html") {
            Ok(Some(b)) => String::from_utf8_lossy(&b).into_owned(),
            Ok(None) => return Some(None),
            Err(e) => {
                warn(e);
                return Some(None);
            }
        };
        read += 1;
        let rules = match fetch("rules.json") {
            Ok(Some(b)) => serde_json::from_slice(&b).ok(),
            _ => None,
        };
        Some(Some(pdf::PageInput { xhtml, rules }))
    };
    let stats = crate::kanpo::fill_amend_texts_with(&mut kd, false, &mut load);
    let extracted = kd.issues.iter().flat_map(|i| &i.items).filter(|i| i.amend_text.is_some()).count();
    let dir = cache.join("kanpo");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(format!("{date}.json")), serde_json::to_vec_pretty(&kd)?)?;
    Ok(Some((stats.amend_items, stats.unmatched, extracted, read)))
}

/// `lawpub kanpo-reextract` — 原本 (`kanpo-archive` の出力) から `.cache/kanpo/{date}.json` を作る。
///
/// `.cache/kanpo/` は GitHub Actions の cache にしか無く、cache の追い出し (リポジトリ上限 10GB) で消える
/// (2026-10-14 時点で配信されていた官報は 10/02 以降の 7 日分だけで、R2 には 07/16 からの 58 日分があった)。
/// 改め文も取得した時点のパーサで抽出したまま残り、パーサを直しても過去の項目には反映されない。
/// 原本の PDF (無ければ蓄積した座標付きテキストと罫線) から抽出し直せば、デジタル官報 (約 90 日で消える) に
/// 取りに行かずにどちらも直せる。
///
/// `dates` が空なら原本の `index.json` の全日付が対象。`all` でなければ cache に既にある日付は飛ばす。
pub fn run_reextract(archive: &str, cache: &Path, dates: &[String], all: bool) -> Result<()> {
    let archive = Archive::new(archive)?;
    let dates: Vec<String> = if dates.is_empty() {
        let Some(index) = archive.get("index.json")? else {
            anyhow::bail!("原本の index.json がありません: {}", archive.base);
        };
        let v: serde_json::Value = serde_json::from_slice(&index)?;
        v["dates"].as_array().into_iter().flatten().filter_map(|d| d.as_str().map(String::from)).collect()
    } else {
        dates.to_vec()
    };
    let (mut done, mut skipped) = (0usize, 0usize);
    for date in &dates {
        if !all && cache.join("kanpo").join(format!("{date}.json")).exists() {
            skipped += 1;
            continue;
        }
        match reextract_date(&archive, date, cache) {
            Ok(Some((items, unmatched, extracted, read))) => {
                done += 1;
                tracing::info!("kanpo-reextract {date}: items={items} unmatched={unmatched} extracted={extracted} pages read={read}");
            }
            Ok(None) => tracing::warn!("kanpo-reextract {date}: 原本に manifest がありません"),
            // 1 日の失敗で他の日を止めない (次回また cache に無い日として拾う)。
            Err(e) => tracing::warn!("kanpo-reextract {date}: {e:#}"),
        }
    }
    println!("kanpo-reextract: {done} day(s) written, {skipped} day(s) already cached, {} day(s) in archive", dates.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_becomes_the_same_shape_as_the_toc() {
        let manifest: Manifest = serde_json::from_value(serde_json::json!({
            "date": "2026-10-09",
            "issues": [{"issue_type": "extra", "issue_no": "第223号", "dir": "20261009g00223",
                        "url": "https://www.kanpo.go.jp/20261009/20261009g00223/", "page_count": 176,
                        "pages": [], "items": [{"title": "輸出貿易管理令の一部を改正する政令（三二五）", "page": 2}]}]
        }))
        .unwrap();
        let kd = kanpo_date(&manifest);
        let item = &kd.issues[0].items[0];
        assert_eq!(item.pdf_url, "https://www.kanpo.go.jp/20261009/20261009g00223/pdf/20261009g002230002.pdf");
        assert_eq!(item.agency_hint.as_deref(), Some("三二五"));
        assert_eq!(kd.issues[0].page_count, Some(176));
        assert_eq!(kd.issues[0].promulgation_date, "2026-10-09");
    }

    #[test]
    fn items_after_the_page_order_falls_back_belong_to_another_issue() {
        // 号外第226号の目次の後ろに、政府調達の号の項目（ページは政府調達の号のもの）が続いた manifest。
        let manifest: Manifest = serde_json::from_value(serde_json::json!({
            "date": "2026-10-15",
            "issues": [{"issue_type": "extra", "issue_no": "第226号", "dir": "20261015g00226",
                        "url": "https://www.kanpo.go.jp/20261015/20261015g00226/", "page_count": 140, "pages": [],
                        "items": [{"title": "環境影響評価法の一部を改正する法律の施行期日を定める政令（三二六）", "page": 2},
                                  {"title": "会社その他", "page": 125},
                                  {"title": "入札公告", "page": 1},
                                  {"title": "落札者等の公示", "page": 39}]}]
        }))
        .unwrap();
        let kd = kanpo_date(&manifest);
        assert_eq!(kd.issues[0].items.iter().map(|i| i.page).collect::<Vec<_>>(), vec![2, 125]);
        assert_eq!(kd.issues[0].titles.len(), 2);
    }

    #[test]
    fn reextract_reads_pages_from_a_local_archive_and_skips_cached_days() {
        let root = crate::wiki::temp_dir("kanpo-reextract");
        let (archive, cache) = (root.join("archive"), root.join("cache"));
        let day = archive.join("2026-10-09");
        std::fs::create_dir_all(day.join("20261009g00223")).unwrap();
        std::fs::write(
            archive.join("index.json"),
            r#"{"dates": ["2026-10-09", "2026-10-08"]}"#,
        )
        .unwrap();
        std::fs::write(
            day.join("manifest.json"),
            serde_json::to_vec(&serde_json::json!({
                "date": "2026-10-09",
                "issues": [{"issue_type": "extra", "issue_no": "第223号", "dir": "20261009g00223",
                            "url": "https://www.kanpo.go.jp/20261009/20261009g00223/", "page_count": 1,
                            "pages": [{"page": 1, "file": "20261009g002230001.pdf", "sha256": "", "bytes": 0, "bbox": true, "rules": false}],
                            "items": [{"title": "輸出貿易管理令の一部を改正する政令（三二五）", "page": 1},
                                      {"title": "日本国に帰化を許可する件", "page": 1}]}]
            }))
            .unwrap(),
        )
        .unwrap();
        // 回帰テスト用コーパスの実官報ページを原本の座標付きテキストとして置く。
        let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../kanpo-parser/tests/corpus");
        let bbox = std::fs::read_dir(&corpus)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path().join("page.bbox.html"))
            .find(|p| p.exists())
            .expect("corpus page");
        std::fs::copy(&bbox, day.join("20261009g00223/20261009g002230001.bbox.html")).unwrap();

        run_reextract(&archive.display().to_string(), &cache, &[], false).unwrap();
        let kd: kanpo_client::KanpoDate =
            serde_json::from_slice(&std::fs::read(cache.join("kanpo/2026-10-09.json")).unwrap()).unwrap();
        let items = &kd.issues[0].items;
        assert!(items[0].amend_text.as_deref().is_some_and(|t| !t.is_empty()), "改正の項目は本文を抽出する");
        assert!(items[1].amend_text.as_deref().is_some_and(|t| !t.is_empty()), "改正・廃止・制定の標題でない項目も抽出する");
        assert!(!cache.join("kanpo/2026-10-08.json").exists(), "原本に manifest が無い日は書かない");

        // cache にある日は既定では作り直さず、--all なら作り直す。
        std::fs::write(cache.join("kanpo/2026-10-09.json"), r#"{"date": "2026-10-09", "issues": []}"#).unwrap();
        run_reextract(&archive.display().to_string(), &cache, &[], false).unwrap();
        assert_eq!(std::fs::read_to_string(cache.join("kanpo/2026-10-09.json")).unwrap(), r#"{"date": "2026-10-09", "issues": []}"#);
        run_reextract(&archive.display().to_string(), &cache, &[], true).unwrap();
        assert!(std::fs::read_to_string(cache.join("kanpo/2026-10-09.json")).unwrap().contains("amend_text"));
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn reextract_rebuilds_pages_from_the_archived_pdf() {
        // 原本の PDF があれば、蓄積した座標付きテキストではなく PDF から読み直す（座標付きテキストが
        // 無くても抽出できる）。
        let root = crate::wiki::temp_dir("kanpo-reextract-pdf");
        let (archive, cache) = (root.join("archive"), root.join("cache"));
        let day = archive.join("2026-10-05");
        std::fs::create_dir_all(day.join("20261005h01801")).unwrap();
        std::fs::write(
            day.join("manifest.json"),
            serde_json::to_vec(&serde_json::json!({
                "date": "2026-10-05",
                "issues": [{"issue_type": "regular", "issue_no": "第1801号", "dir": "20261005h01801",
                            "url": "https://www.kanpo.go.jp/20261005/20261005h01801/", "page_count": 2,
                            "pages": [{"page": 2, "file": "20261005h018010002.pdf", "sha256": "", "bytes": 0, "bbox": false, "rules": false}],
                            "items": [{"title": "国際海事機関海洋環境保護委員会の判定に基づき環境大臣が指定する物質の一部を改正する件（環境四三）", "page": 2}]}]
            }))
            .unwrap(),
        )
        .unwrap();
        let pdf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../kanpo-parser/tests/corpus/2026-10-05-honshi1801-p2/page.pdf");
        std::fs::copy(&pdf, day.join("20261005h01801/20261005h018010002.pdf")).unwrap();
        run_reextract(&archive.display().to_string(), &cache, &["2026-10-05".to_string()], false).unwrap();
        let kd: kanpo_client::KanpoDate =
            serde_json::from_slice(&std::fs::read(cache.join("kanpo/2026-10-05.json")).unwrap()).unwrap();
        assert!(kd.issues[0].items[0].amend_text.as_deref().is_some_and(|t| !t.is_empty()), "PDF から本文を抽出する");
    }
}
