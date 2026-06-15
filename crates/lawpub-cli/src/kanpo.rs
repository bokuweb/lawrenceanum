use anyhow::{Context, Result};
use chrono::Utc;
use kanpo_client::{pdf, HttpProvider, KanpoIssue, KanpoProvider, MockKanpoProvider};
use kanpo_linker::{match_event, AUTO_LINK_THRESHOLD};
use serde::Serialize;
use serde_json::json;
use std::path::{Path, PathBuf};

/// `lawpub kanpo-fetch` — 指定日の官報を取得して `.cache/kanpo/{date}.json` に保存。
pub fn run_fetch(date: &str, cache: &Path) -> Result<()> {
    let provider = MockKanpoProvider; // Phase 3 初期はモックのみ。
    let kd = provider.fetch_date(date)?;
    let dir = cache.join("kanpo");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.json", date));
    let bytes = serde_json::to_vec_pretty(&kd)?;
    std::fs::write(&path, bytes).with_context(|| format!("write {}", path.display()))?;
    tracing::info!("wrote kanpo cache: {}", path.display());
    Ok(())
}

/// `lawpub kanpo-link` — `.cache/kanpo/*.json` を読み、`public/laws/*/timeline.json`
/// の各イベントに官報マッチングを書き戻し、`public/kanpo/{date}/index.json` を生成する。
pub fn run_link(public: &Path) -> Result<()> {
    let cache_dir = PathBuf::from(".cache/kanpo");
    if !cache_dir.exists() {
        tracing::info!("no kanpo cache; skipping kanpo-link");
        return Ok(());
    }

    // 1) public/kanpo/{date}/index.json を出力。
    let mut by_date: Vec<(String, kanpo_client::KanpoDate)> = Vec::new();
    for f in std::fs::read_dir(&cache_dir)? {
        let f = f?;
        if f.path().extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let kd: kanpo_client::KanpoDate = serde_json::from_slice(&std::fs::read(f.path())?)?;
        by_date.push((kd.date.clone(), kd));
    }
    by_date.sort_by(|a, b| a.0.cmp(&b.0));

    for (date, kd) in &by_date {
        let dir = public.join("kanpo").join(date);
        std::fs::create_dir_all(&dir)?;
        write_json_pretty(
            &dir.join("index.json"),
            &json!({
                "date": date,
                "generated_at": Utc::now().to_rfc3339(),
                "issues": kd.issues,
            }),
        )?;
    }

    // 2) timeline.json に kanpo マッチングを反映。
    let laws_dir = public.join("laws");
    if !laws_dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(&laws_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let timeline_path = entry.path().join("timeline.json");
        if !timeline_path.exists() {
            continue;
        }
        let mut tl: serde_json::Value = serde_json::from_slice(&std::fs::read(&timeline_path)?)?;
        let events = match tl.get_mut("events").and_then(|e| e.as_array_mut()) {
            Some(e) => e,
            None => continue,
        };

        for ev in events.iter_mut() {
            let promulgation = ev
                .get("promulgation_date")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let law_num = ev
                .get("amending_law_num")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let mut best: Option<(f64, &str, &KanpoIssue, Vec<&'static str>)> = None;
            for (date, kd) in &by_date {
                for issue in &kd.issues {
                    let r = match_event(
                        promulgation.as_deref(),
                        law_num.as_deref(),
                        None, // event title は現状 timeline に持たないため未使用。
                        None,
                        issue,
                    );
                    if r.confidence > 0.0
                        && best.as_ref().map(|b| r.confidence > b.0).unwrap_or(true)
                    {
                        best = Some((r.confidence, date.as_str(), issue, r.match_reasons));
                    }
                }
            }
            if let Some((conf, date, _issue, reasons)) = best {
                let linked = conf >= AUTO_LINK_THRESHOLD;
                ev["kanpo"] = json!({
                    "linked": linked,
                    "path": format!("kanpo/{}/index.json", date),
                    "confidence": conf,
                    "match_reasons": reasons,
                });
            }
        }

        let bytes = serde_json::to_vec_pretty(&tl)?;
        std::fs::write(&timeline_path, bytes)?;
    }

    // timeline と kanpo/index.json を書き換えたので manifest を再計算。
    crate::build::rebuild_manifest(public)?;
    Ok(())
}

/// `lawpub kanpo-poc` — デジタル官報の項目別 PDF から改め文を抽出する検証用 PoC。
///
/// `.cache/kanpo-poc/{date}/` に
///   - `toc.json`      : 目次（号・項目・PDF URL）
///   - `NNNN-<type>.txt`: 各項目の整形済み改め文テキスト（目視検証用）
/// を書き出す。timeline.json には一切触れない（取得・抽出の精度/負荷の検証専用）。
pub fn run_poc(date: &str, amend_only: bool, limit: usize, cache: &Path) -> Result<()> {
    let provider = HttpProvider::new()?;
    tracing::info!("fetch kanpo TOC for {date} from {}", provider.base_url());
    let kd = provider.fetch_date(date)?;

    let out_dir = cache.join("kanpo-poc").join(date);
    std::fs::create_dir_all(&out_dir)?;

    let mut total_items = 0usize;
    let mut downloaded = 0usize;
    let mut by_format: std::collections::BTreeMap<String, usize> = Default::default();
    let mut total_pdf_bytes = 0usize;
    // 同一ページ(=同一 PDF)を複数項目が共有するため、ページ単位でキャッシュする。
    // pdf_url -> (記事セグメント一覧, ページ全体の形式)
    let mut page_cache: std::collections::HashMap<String, (Vec<String>, String)> = Default::default();

    let mut kd = kd;
    for issue in &mut kd.issues {
        for item in &mut issue.items {
            total_items += 1;
            if amend_only
                && !(item.title.contains("改正") || item.title.contains("廃止"))
            {
                continue;
            }

            // ページ PDF を取得・抽出・記事分割（ページ単位で1回だけ）。
            if !page_cache.contains_key(&item.pdf_url) {
                if downloaded >= limit {
                    continue;
                }
                let pdf_bytes = match provider.get_bytes(&item.pdf_url) {
                    Ok(b) => b,
                    Err(e) => {
                        tracing::warn!("download failed {}: {e}", item.pdf_url);
                        continue;
                    }
                };
                total_pdf_bytes += pdf_bytes.len();
                downloaded += 1;
                match pdf::extract(&pdf_bytes) {
                    Ok(ex) => {
                        let segs = pdf::segment_articles(&ex.text);
                        page_cache.insert(item.pdf_url.clone(), (segs, ex.format));
                    }
                    Err(e) => {
                        tracing::warn!("extract failed {}: {e}", item.pdf_url);
                        continue;
                    }
                }
            }
            let (segments, page_format) = match page_cache.get(&item.pdf_url) {
                Some(v) => v,
                None => continue,
            };

            // 標題に最も合致する記事セグメントを選ぶ。無ければページ全体。
            let body = best_segment(&item.title, segments)
                .map(|s| s.to_string())
                .unwrap_or_else(|| segments.join("\n\n"));
            let format = pdf::detect_format_of(&body).unwrap_or_else(|| page_format.clone());

            *by_format.entry(format.clone()).or_default() += 1;
            item.amend_format = Some(format.clone());
            item.amend_text = Some(truncate_chars(&body, 400));

            let fname = format!("{:04}-{}.txt", item.page, sanitize(&item.title));
            let header = format!(
                "# {}\n# {}  page={}  format={}\n# {}\n\n",
                item.title, issue.issue_no, item.page, format, item.pdf_url
            );
            std::fs::write(out_dir.join(&fname), format!("{header}{body}"))?;
        }
    }

    write_json_pretty(&out_dir.join("toc.json"), &kd)?;

    tracing::info!(
        "kanpo-poc done: issues={} items={} downloaded={} pdf_total={:.1}MB formats={:?}",
        kd.issues.len(),
        total_items,
        downloaded,
        total_pdf_bytes as f64 / 1_048_576.0,
        by_format,
    );
    println!(
        "wrote {} (issues={}, items={}, downloaded={}, formats={:?})",
        out_dir.display(),
        kd.issues.len(),
        total_items,
        downloaded,
        by_format,
    );
    Ok(())
}

/// 標題に最も合致する記事セグメントを選ぶ。
///
/// 本文ヘッダには対象法令名のあとに法令番号「（…）」が挿入されるため、標題そのものの
/// 完全一致では拾えない。対象法令名（「の一部を」「を廃止」より前）を鍵に、まず完全一致、
/// 次に前方一致を縮めながら探す。
fn best_segment<'a>(title: &str, segments: &'a [String]) -> Option<&'a str> {
    let core = title_core(title);
    if core.chars().count() < 4 {
        return None;
    }
    // core（または前方一致の鍵）を含む候補を集める。
    let mut keys: Vec<String> = vec![core.clone()];
    let chars: Vec<char> = core.chars().collect();
    let mut len = chars.len().saturating_sub(2);
    while len >= 6 {
        keys.push(chars[..len].iter().collect());
        len = len.saturating_sub(4);
    }
    // 制定文本文らしさ（「の規定に基づき」「次のように」「改める」「定める」）を優先し、
    // 目次エントリ（短く制定文マーカを欠く）を避ける。同点は長い方。
    segments
        .iter()
        .filter(|s| keys.iter().any(|k| s.contains(k.as_str())))
        .max_by_key(|s| body_score(s))
        .map(|s| s.as_str())
}

/// セグメントの「制定文本文らしさ」スコア。
fn body_score(s: &str) -> usize {
    let mut score = 0;
    for m in ["の規定に基づき", "次のように", "に改める", "を加える", "定める"] {
        if s.contains(m) {
            score += 1000;
        }
    }
    score + s.chars().count()
}

/// 標題から突合の鍵となる「対象法令名」を取り出す。
fn title_core(title: &str) -> String {
    // 末尾の制定機関略号「（総務七七）」等を除去。
    let mut t = title;
    if let Some(p) = t.rfind('（') {
        t = &t[..p];
    }
    // 「○○の一部を改正する…」「○○を廃止する件」なら対象法令名 ○○ を鍵にする。
    for marker in ["の一部を", "を廃止"] {
        if let Some(p) = t.find(marker) {
            return t[..p].trim().to_string();
        }
    }
    t.trim().to_string()
}

/// 標題をファイル名向けに短縮・サニタイズ。
fn sanitize(s: &str) -> String {
    s.chars()
        .take(24)
        .map(|c| match c {
            '/' | '\\' | ':' | '\u{0}' => '_',
            other => other,
        })
        .collect()
}

fn truncate_chars(s: &str, n: usize) -> String {
    let mut out: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        out.push('…');
    }
    out
}

fn write_json_pretty<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(value)?;
    std::fs::write(path, bytes)?;
    Ok(())
}
