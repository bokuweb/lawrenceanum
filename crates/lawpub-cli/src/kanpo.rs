use anyhow::{Context, Result};
use chrono::Utc;
use kanpo_client::{page_pdf_url, pdf, HttpProvider, KanpoDate, KanpoProvider, MockKanpoProvider};
use kanpo_linker::{match_event, AUTO_LINK_THRESHOLD};
use serde::Serialize;
use serde_json::json;
use std::path::{Path, PathBuf};

/// `lawpub kanpo-fetch` — 指定日の官報を取得し、改正・廃止・制定系の各項目の改め文を
/// 抽出して `.cache/kanpo/{date}.json` に保存する。`provider` は "http"(デジタル官報) /
/// "mock"。`limit` は1日あたりの PDF ダウンロード上限（負荷ガード）。
pub fn run_fetch(
    date: &str,
    provider: &str,
    limit: usize,
    save_pdf: bool,
    cache: &Path,
) -> Result<()> {
    let mut kd = match provider {
        "mock" => MockKanpoProvider.fetch_date(date)?,
        _ => {
            let http = HttpProvider::new()?;
            tracing::info!("fetch kanpo for {date} from {}", http.base_url());
            let mut kd = http.fetch_date(date)?;
            // --save-pdf 指定時のみ生 PDF を `{cache}/kanpo-pdf/{date}/` に保持
            // （抽出精度を上げた際の再抽出用。git 管理外、永続化は R2 等へ別途同期）。
            let pdf_dir = save_pdf.then(|| cache.join("kanpo-pdf").join(date));
            let stats = fill_amend_texts(&http, &mut kd, true, limit, pdf_dir.as_deref());
            tracing::info!(
                "extracted amend texts: items={} downloaded={} pdf={:.1}MB formats={:?}{}",
                stats.amend_items,
                stats.downloaded,
                stats.total_pdf_bytes as f64 / 1_048_576.0,
                stats.by_format,
                pdf_dir
                    .map(|d| format!(" (raw pdf -> {})", d.display()))
                    .unwrap_or_default(),
            );
            kd
        }
    };
    // mock のときも安定した形で保存。
    kd.date = date.to_string();
    let dir = cache.join("kanpo");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.json", date));
    let bytes = serde_json::to_vec_pretty(&kd)?;
    std::fs::write(&path, bytes).with_context(|| format!("write {}", path.display()))?;
    tracing::info!("wrote kanpo cache: {}", path.display());
    Ok(())
}

/// 改め文抽出の集計。
#[derive(Default)]
struct FillStats {
    amend_items: usize,
    downloaded: usize,
    total_pdf_bytes: usize,
    by_format: std::collections::BTreeMap<String, usize>,
}

/// 各項目について項目別 PDF を取得・抽出・記事分割し、標題に合致する改め文本文を
/// `item.amend_text` / `item.amend_format` に詰める。
///
/// 同一ページ(=同一 PDF)を複数項目が共有するためページ単位でキャッシュする。
/// `amend_only` なら改正/廃止/制定系の標題だけを対象にする。
/// `pdf_dir` を渡すと取得した生 PDF をそこへ保存する（後で抽出精度を上げた際の
/// 再抽出用。`{pdf_dir}/{PDFファイル名}`）。
fn fill_amend_texts(
    provider: &HttpProvider,
    kd: &mut KanpoDate,
    amend_only: bool,
    limit: usize,
    pdf_dir: Option<&Path>,
) -> FillStats {
    // 1項目が複数ページに跨る（新旧対照表等）ため、項目の開始ページから次項目の
    // 開始ページ手前（最終項目は号の総ページ）までを連結して抽出する。
    const MAX_SPAN: u32 = 25;
    let mut stats = FillStats::default();
    if let Some(dir) = pdf_dir {
        if let Err(e) = std::fs::create_dir_all(dir) {
            tracing::warn!("create pdf dir {} failed: {e}", dir.display());
        }
    }
    // page PDF URL -> 抽出済みページ本文（空文字は取得/抽出失敗のメモ化）。
    let mut page_text: std::collections::HashMap<String, String> = Default::default();

    for issue in &mut kd.issues {
        // 号内の全項目の開始ページ（昇順・重複排除）。終端ページ算定に使う。
        let mut starts: Vec<u32> = issue.items.iter().map(|i| i.page).collect();
        starts.sort_unstable();
        starts.dedup();
        let last_page = issue
            .page_count
            .unwrap_or_else(|| starts.last().copied().unwrap_or(0));

        for item in &mut issue.items {
            if amend_only && !is_amend_title(&item.title) {
                continue;
            }
            stats.amend_items += 1;

            // 終端ページ = 次の開始ページ「込み」（無ければ号の最終ページ）。上限で頭打ち。
            // 短い記事は次記事の開始ページへ本文が溢れることが多いため、境界ページも
            // 取り込み、混入分は記事分割＋標題突合で振り分ける。
            let end = starts
                .iter()
                .copied()
                .find(|&p| p > item.page)
                .unwrap_or(last_page)
                .max(item.page)
                .min(item.page + MAX_SPAN - 1);

            let mut parts: Vec<String> = Vec::new();
            for n in item.page..=end {
                let url = if n == item.page {
                    item.pdf_url.clone()
                } else {
                    match page_pdf_url(&item.pdf_url, item.page, n) {
                        Some(u) => u,
                        None => continue,
                    }
                };
                if let Some(t) = page_text.get(&url) {
                    if !t.is_empty() {
                        parts.push(t.clone());
                    }
                    continue;
                }
                if stats.downloaded >= limit {
                    break;
                }
                let pdf_bytes = match provider.get_bytes(&url) {
                    Ok(b) => b,
                    Err(e) => {
                        tracing::warn!("download failed {url}: {e}");
                        page_text.insert(url, String::new());
                        continue;
                    }
                };
                stats.total_pdf_bytes += pdf_bytes.len();
                stats.downloaded += 1;
                if let Some(dir) = pdf_dir {
                    if let Some(name) = url.rsplit('/').next() {
                        let _ = std::fs::write(dir.join(name), &pdf_bytes);
                    }
                }
                let text = match pdf::extract(&pdf_bytes) {
                    Ok(ex) => ex.text,
                    Err(e) => {
                        tracing::warn!("extract failed {url}: {e}");
                        String::new()
                    }
                };
                page_text.insert(url, text.clone());
                if !text.is_empty() {
                    parts.push(text);
                }
            }

            let full = parts.join("\n");
            if full.is_empty() {
                continue;
            }
            let segments = pdf::segment_articles(&full);
            let body = best_segment(&item.title, &segments)
                .map(|s| s.to_string())
                .unwrap_or_else(|| segments.join("\n\n"));
            let format = pdf::detect_format_of(&body)
                .or_else(|| pdf::detect_format_of(&full))
                .unwrap_or_else(|| "unknown".to_string());
            *stats.by_format.entry(format.clone()).or_default() += 1;
            item.amend_format = Some(format);
            item.amend_text = Some(body);
        }
    }
    stats
}

/// 改正・廃止・制定系の標題か（改め文抽出の対象判定）。
fn is_amend_title(title: &str) -> bool {
    title.contains("改正") || title.contains("廃止") || title.contains("制定")
}

/// 改正イベントに対応する官報項目を、公布日の厳密一致 + 標題一致で探す。
/// 戻り値は (confidence, 官報日付, 項目)。
fn match_item<'a>(
    promulgation: Option<&str>,
    title: Option<&str>,
    by_date: &'a [(String, KanpoDate)],
) -> Option<(f64, &'a str, &'a kanpo_client::KanpoItem)> {
    let pd = promulgation?;
    let ev_core = title_core(title?);
    if ev_core.chars().count() < 4 {
        return None;
    }
    let mut best: Option<(f64, &str, &kanpo_client::KanpoItem)> = None;
    for (date, kd) in by_date {
        if date != pd {
            continue;
        }
        for issue in &kd.issues {
            for item in &issue.items {
                let score = title_match_score(&ev_core, &item.title);
                if score <= 0.0 {
                    continue;
                }
                // 公布日一致(0.5) + 標題一致(最大0.45)。
                let conf = 0.5 + 0.45 * score;
                if best.map(|b| conf > b.0).unwrap_or(true) {
                    best = Some((conf, date.as_str(), item));
                }
            }
        }
    }
    best
}

/// イベント標題の対象法令名コアと、官報項目標題のコアの一致度 (0.0–1.0)。
fn title_match_score(ev_core: &str, item_title: &str) -> f64 {
    let item_core = title_core(item_title);
    if item_core.is_empty() {
        return 0.0;
    }
    if ev_core == item_core {
        1.0
    } else if item_core.contains(ev_core) || ev_core.contains(&item_core) {
        0.8
    } else {
        0.0
    }
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
            let title = ev
                .get("amending_law_title")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            // (a) 号レベルの突合（既存。公布日/法令番号/法令名のスコアリング）。
            let mut best: Option<(f64, &str, Vec<&'static str>)> = None;
            for (date, kd) in &by_date {
                for issue in &kd.issues {
                    let r = match_event(
                        promulgation.as_deref(),
                        law_num.as_deref(),
                        title.as_deref(),
                        None,
                        issue,
                    );
                    if r.confidence > 0.0
                        && best.as_ref().map(|b| r.confidence > b.0).unwrap_or(true)
                    {
                        best = Some((r.confidence, date.as_str(), r.match_reasons));
                    }
                }
            }

            // (b) 項目レベルの突合（公布日の厳密一致 + 標題一致）。改め文項目を特定する。
            let item_match = match_item(promulgation.as_deref(), title.as_deref(), &by_date);

            if best.is_none() && item_match.is_none() {
                continue;
            }
            let (mut conf, mut date, mut reasons): (f64, String, Vec<String>) = match &best {
                Some((c, d, r)) => (*c, d.to_string(), r.iter().map(|s| s.to_string()).collect()),
                None => (0.0, String::new(), Vec::new()),
            };
            let mut obj = serde_json::Map::new();
            if let Some((iconf, idate, item)) = item_match {
                // 項目一致は号レベルより強い信号。改め文・官報PDFリンクを付与する。
                if iconf > conf {
                    conf = iconf;
                    date = idate.to_string();
                    reasons = vec!["promulgation_date".into(), "item_title".into()];
                }
                obj.insert("pdf_url".into(), json!(item.pdf_url));
                obj.insert("page".into(), json!(item.page));
                if let Some(t) = &item.amend_text {
                    obj.insert("amend_text".into(), json!(t));
                }
                if let Some(f) = &item.amend_format {
                    obj.insert("amend_format".into(), json!(f));
                }
            }
            obj.insert("linked".into(), json!(conf >= AUTO_LINK_THRESHOLD));
            if !date.is_empty() {
                obj.insert("path".into(), json!(format!("kanpo/{}/index.json", date)));
            }
            obj.insert("confidence".into(), json!(conf));
            obj.insert("match_reasons".into(), json!(reasons));
            ev["kanpo"] = serde_json::Value::Object(obj);
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
    let mut kd = provider.fetch_date(date)?;

    let out_dir = cache.join("kanpo-poc").join(date);
    std::fs::create_dir_all(&out_dir)?;

    let stats = fill_amend_texts(&provider, &mut kd, amend_only, limit, None);

    // 改め文が取れた項目を目視検証用テキストに書き出す。
    for issue in &kd.issues {
        for item in &issue.items {
            let Some(body) = &item.amend_text else { continue };
            let fmt = item.amend_format.as_deref().unwrap_or("unknown");
            let fname = format!("{:04}-{}.txt", item.page, sanitize(&item.title));
            let header = format!(
                "# {}\n# {}  page={}  format={}\n# {}\n\n",
                item.title, issue.issue_no, item.page, fmt, item.pdf_url
            );
            std::fs::write(out_dir.join(&fname), format!("{header}{body}"))?;
        }
    }
    write_json_pretty(&out_dir.join("toc.json"), &kd)?;

    tracing::info!(
        "kanpo-poc done: issues={} downloaded={} pdf_total={:.1}MB formats={:?}",
        kd.issues.len(),
        stats.downloaded,
        stats.total_pdf_bytes as f64 / 1_048_576.0,
        stats.by_format,
    );
    println!(
        "wrote {} (issues={}, amend_items={}, downloaded={}, formats={:?})",
        out_dir.display(),
        kd.issues.len(),
        stats.amend_items,
        stats.downloaded,
        stats.by_format,
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

fn write_json_pretty<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(value)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanpo_client::{KanpoIssue, KanpoItem};

    fn item(title: &str) -> KanpoItem {
        KanpoItem {
            title: title.to_string(),
            page: 1,
            pdf_url: "u".into(),
            sha256: None,
            agency_hint: None,
            amend_text: Some("本文".into()),
            amend_format: Some("prose".into()),
        }
    }

    #[test]
    fn title_core_strips_agency_and_target() {
        assert_eq!(
            title_core("電波法による伝搬障害の防止に関する規則の一部を改正する省令（総務七七）"),
            "電波法による伝搬障害の防止に関する規則"
        );
        assert_eq!(
            title_core("木質ペレット燃料の日本農林規格を廃止する件（農林水産七六四）"),
            "木質ペレット燃料の日本農林規格"
        );
    }

    #[test]
    fn match_item_uses_date_and_title() {
        let kd = KanpoDate {
            date: "2026-06-15".into(),
            issues: vec![KanpoIssue {
                issue_type: "extra".into(),
                issue_no: "第131号".into(),
                pdf_url: String::new(),
                sha256: None,
                promulgation_date: "2026-06-15".into(),
                law_nums: vec![],
                titles: vec![],
                items: vec![item("電波法による伝搬障害の防止に関する規則の一部を改正する省令（総務七七）")],
            }],
        };
        let by_date = vec![("2026-06-15".to_string(), kd)];

        // 公布日 + 標題が一致 → 高 confidence。
        let m = match_item(
            Some("2026-06-15"),
            Some("電波法による伝搬障害の防止に関する規則の一部を改正する省令"),
            &by_date,
        );
        assert!(m.is_some());
        assert!(m.unwrap().0 >= AUTO_LINK_THRESHOLD);

        // 公布日違い → 不一致。
        assert!(match_item(
            Some("2026-06-12"),
            Some("電波法による伝搬障害の防止に関する規則の一部を改正する省令"),
            &by_date
        )
        .is_none());
    }
}
