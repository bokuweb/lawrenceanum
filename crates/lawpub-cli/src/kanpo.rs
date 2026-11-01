use anyhow::{Context, Result};
use chrono::Utc;
use kanpo_client::{page_pdf_url, pdf, HttpProvider, KanpoDate, KanpoProvider, MockKanpoProvider};
use kanpo_linker::{match_event, AUTO_LINK_THRESHOLD};
use serde::Serialize;
use serde_json::json;
use std::path::{Path, PathBuf};

/// `lawpub kanpo-fetch` — 指定日の官報を取得し、全項目の本文（改正・廃止・制定の項目は改め文）を
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
            let stats = fill_amend_texts(&http, &mut kd, false, limit, pdf_dir.as_deref());
            tracing::info!(
                "extracted item bodies: items={} unmatched={} downloaded={} pdf={:.1}MB formats={:?}{}",
                stats.amend_items,
                stats.unmatched,
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
pub(crate) struct FillStats {
    pub amend_items: usize,
    /// 標題に合う記事を特定できず、項目のページの本文をまとめて入れた項目数。
    pub unmatched: usize,
    pub downloaded: usize,
    pub total_pdf_bytes: usize,
    pub by_format: std::collections::BTreeMap<String, usize>,
}

/// ページ PDF の URL から解析の入力 (座標付きテキストと罫線) を取る手段。デジタル官報から PDF を取って
/// pdftotext にかける (`kanpo-fetch`) か、R2 に蓄積した原本の座標付きテキストを読む (`kanpo-reextract`)。
/// `None` は取得の上限に達した (この項目の残りのページは取らない)、`Some(None)` は取得・抽出の失敗。
pub(crate) type PageLoader<'a> = dyn FnMut(&str) -> Option<Option<pdf::PageInput>> + 'a;

/// ページ PDF の URL から、そのページの画像を OCR した結果を取る手段（`kanpo-reextract --ocr-dir`、opt-in）。無ければ `None`。
pub(crate) type OcrLoader<'a> = dyn FnMut(&str) -> Option<pdf::OcrPage> + 'a;

/// デジタル官報から項目別 PDF を取得して改め文を詰める。`limit` は 1 日あたりの PDF ダウンロード上限。
/// `pdf_dir` を渡すと取得した生 PDF をそこへ保存する（`{pdf_dir}/{PDFファイル名}`）。
fn fill_amend_texts(
    provider: &HttpProvider,
    kd: &mut KanpoDate,
    amend_only: bool,
    limit: usize,
    pdf_dir: Option<&Path>,
) -> FillStats {
    if let Some(dir) = pdf_dir {
        if let Err(e) = std::fs::create_dir_all(dir) {
            tracing::warn!("create pdf dir {} failed: {e}", dir.display());
        }
    }
    let (mut downloaded, mut total_pdf_bytes) = (0usize, 0usize);
    let mut load = |url: &str| -> Option<Option<pdf::PageInput>> {
        if downloaded >= limit {
            return None;
        }
        let pdf_bytes = match provider.get_bytes(url) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("download failed {url}: {e}");
                return Some(None);
            }
        };
        total_pdf_bytes += pdf_bytes.len();
        downloaded += 1;
        if let Some(dir) = pdf_dir {
            if let Some(name) = url.rsplit('/').next() {
                let _ = std::fs::write(dir.join(name), &pdf_bytes);
            }
        }
        Some(match pdf::bbox_xhtml(&pdf_bytes) {
            Ok(xhtml) => Some(pdf::PageInput {
                xhtml,
                // 罫線は表（新旧対照表の中の別表など）の復元に使う。取れなくても本文は組める。
                rules: pdf::page_rules(&pdf_bytes).ok(),
            }),
            Err(e) => {
                tracing::warn!("extract failed {url}: {e}");
                None
            }
        })
    };
    let mut stats = fill_amend_texts_with(kd, amend_only, &mut load, None);
    stats.downloaded = downloaded;
    stats.total_pdf_bytes = total_pdf_bytes;
    stats
}

/// 項目の頁（`item.page` から `end` まで）と、欄の文脈にする前の頁を集めて解析し、項目に当たる記事を選ぶ。
/// 頁が取れないか、本文が無ければ `None`。
fn analyze_window(
    item: &kanpo_client::KanpoItem,
    end: u32,
    starts_of: &[(u32, String)],
    page_input: &mut std::collections::HashMap<String, Option<pdf::PageInput>>,
    load: &mut PageLoader,
) -> Option<(pdf::Analysis, Vec<usize>)> {
    // 欄の文脈として見る項目の前の頁数（頁ごとに取得するので上限を置く）。
    const CONTEXT_SPAN: u32 = 10;
    let mut pages: Vec<pdf::PageInput> = Vec::new();
    for n in item.page..=end {
        let url = if n == item.page {
            item.pdf_url.clone()
        } else {
            match page_pdf_url(&item.pdf_url, item.page, n) {
                Some(u) => u,
                None => continue,
            }
        };
        if let Some(cached) = page_input.get(&url) {
            pages.extend(cached.clone());
            continue;
        }
        let Some(input) = load(&url) else { break };
        pages.extend(input.clone());
        page_input.insert(url, input);
    }
    if pages.is_empty() {
        return None;
    }

    // 項目の前の頁（官庁報告の欄見出しがあれば、その欄の文脈になる）。
    let mut context: Vec<pdf::PageInput> = Vec::new();
    for n in item.page.saturating_sub(CONTEXT_SPAN).max(1)..item.page {
        let Some(url) = page_pdf_url(&item.pdf_url, item.page, n) else { continue };
        if let Some(cached) = page_input.get(&url) {
            context.extend(cached.clone());
            continue;
        }
        let Some(input) = load(&url) else { continue };
        context.extend(input.clone());
        page_input.insert(url, input);
    }

    // 項目のページをまとめて解析する（ページで切れた新旧対照表は 1 つの表につなぎ、
    // 罫線の表は本文の流れの中の表として構造化する）。記事ごとに分け、標題に合う記事を採る。
    let mut analysis = pdf::analyze_item_in_context(&context, &pages);
    if analysis.text.is_empty() {
        return None;
    }
    // 号数を並べた項目（「保安林の指定をする件（農林水産一〇一九～一〇三四）」）は告示ごとの記事を、欄見出しの項目
    // （「会社その他」）は欄の公告を、すべてつなぐ。
    let titles: Vec<&str> = std::iter::once(item.title.as_str())
        .chain(starts_of.iter().filter(|(p, t)| (item.page..=end).contains(p) && *t != item.title).map(|(_, t)| t.as_str()))
        .collect();
    pdf::split_listed_sections(&titles, &mut analysis.articles);
    let picks = pdf::pick_items(&titles, &analysis.articles);
    let mut picked = picks[0].clone();
    // 項目の開始頁が画像だけ（字が無い）なら、項目の公告は画像で、次の頁の頭に文の途中から続く（号外第225号の「日本弁護士連合会
    // 裁決取消訴訟の判決確定・懲戒処分・公示送達関係」の「審査請求をした場合には、処分の取消しの訴え…」）。頁の範囲の最初の
    // 記事が、ほかの項目の選ばない、見出しの無い（最初の行が `〇` でなく読点を持つ）記事なら、その続きを項目の本文にする。
    let image_start = page_input.get(&item.pdf_url).cloned().flatten().is_some_and(|p| pdf::analyze_item(&[p]).text.trim().is_empty());
    if picked.is_empty() && image_start && !picks.iter().flatten().any(|&i| i == 0) {
        let head = analysis.articles.first().and_then(|a| a.text.lines().map(str::trim).find(|l| !l.is_empty())).unwrap_or("");
        if !head.starts_with('〇') && head.contains('、') {
            picked = vec![0];
        }
    }
    Some((analysis, picked))
}

/// 頁の範囲（`item.page` から `end` まで）を、頁ごとの OCR の結果（`ocr`、無い頁は `None`）と合わせて解析し、項目に当たる
/// 記事を選ぶ（`pdf::ocr_articles::pick_items_with_ocr`）。頁の範囲に OCR の結果が 1 つも無いか、何も選べなければ `None`。
fn analyze_window_with_ocr(
    item: &kanpo_client::KanpoItem,
    end: u32,
    starts_of: &[(u32, String)],
    page_input: &mut std::collections::HashMap<String, Option<pdf::PageInput>>,
    load: &mut PageLoader,
    ocr: &mut OcrLoader,
) -> Option<(pdf::Analysis, Vec<usize>)> {
    let (mut pages, mut ocrs, mut have) = (Vec::new(), Vec::new(), false);
    for n in item.page..=end {
        let url = if n == item.page {
            item.pdf_url.clone()
        } else {
            match page_pdf_url(&item.pdf_url, item.page, n) {
                Some(u) => u,
                None => continue,
            }
        };
        let input = match page_input.get(&url) {
            Some(cached) => cached.clone(),
            None => {
                let Some(input) = load(&url) else { break };
                page_input.insert(url.clone(), input.clone());
                input
            }
        };
        let page_ocr = ocr(&url);
        have |= page_ocr.is_some();
        for p in input {
            pages.push(p);
            ocrs.push(page_ocr.clone());
        }
    }
    if !have {
        return None;
    }
    let mut analysis = pdf::analyze_item_with_ocr(&pages, &ocrs);
    let titles: Vec<&str> = std::iter::once(item.title.as_str())
        .chain(starts_of.iter().filter(|(p, t)| (item.page..=end).contains(p) && *t != item.title).map(|(_, t)| t.as_str()))
        .collect();
    pdf::split_listed_sections(&titles, &mut analysis.articles);
    let picked = pdf::ocr_articles::pick_items_with_ocr(&titles, &mut analysis.articles).swap_remove(0);
    (!picked.is_empty()).then_some((analysis, picked))
}

/// 各項目について項目のページを `load` で集めて解析・記事分割し、標題に合致する改め文本文を
/// `item.amend_text` / `item.amend_format` に詰める。
///
/// 同一ページ(=同一 PDF)を複数項目が共有するためページ単位でキャッシュする。
/// `amend_only` なら改正/廃止/制定系の標題だけを対象にする（`kanpo-poc --amend-only` 用。取得・再抽出は
/// 標題を問わず全項目を対象にし、告示・公告なども検索できるようにする）。
///
/// `ocr` を渡したときだけ（`kanpo-reextract --ocr-dir`）、PDF の字から記事を選べない項目（本文が画像の告示など）を、頁の画像を
/// OCR した結果と合わせて選び直し、選べたら `amend_source: "ocr"` の本文にする。
pub(crate) fn fill_amend_texts_with(kd: &mut KanpoDate, amend_only: bool, load: &mut PageLoader, mut ocr: Option<&mut OcrLoader>) -> FillStats {
    // 1項目が複数ページに跨る（新旧対照表等）ため、項目の開始ページから次項目の
    // 開始ページ手前（最終項目は号の総ページ）までを連結して抽出する。
    const MAX_SPAN: u32 = 25;
    let mut stats = FillStats::default();
    // page PDF URL -> 解析の入力（座標付きテキストと罫線）。None は取得/抽出失敗のメモ化。
    let mut page_input: std::collections::HashMap<String, Option<pdf::PageInput>> = Default::default();

    for issue in &mut kd.issues {
        // 号内の全項目の開始ページ（昇順・重複排除）。終端ページ算定に使う。
        let mut starts: Vec<u32> = issue.items.iter().map(|i| i.page).collect();
        starts.sort_unstable();
        starts.dedup();
        let last_page = issue
            .page_count
            .unwrap_or_else(|| starts.last().copied().unwrap_or(0));

        // 同じ頁の範囲から始まる項目の標題（欄見出しだけの記事を選んだ項目が、ほかの項目の記事の手前で止まるため）。
        let starts_of: Vec<(u32, String)> = issue.items.iter().map(|i| (i.page, i.title.clone())).collect();
        // 頁の範囲を次の項目の開始頁まで広げた項目（下で、後ろの項目の本文の手前で切る）。
        let mut widened: Vec<usize> = Vec::new();
        for (n, item) in issue.items.iter_mut().enumerate() {
            if amend_only && !is_amend_title(&item.title) {
                continue;
            }
            stats.amend_items += 1;

            // 終端ページ = 次の開始ページ「込み」（無ければ号の最終ページ）。上限で頭打ち。
            // 短い記事は次記事の開始ページへ本文が溢れることが多いため、境界ページも
            // 取り込み、混入分は記事分割＋標題突合で振り分ける。
            let window_end = |from: u32| {
                starts.iter().copied().find(|&p| p > from).unwrap_or(last_page).max(from).min(from + MAX_SPAN - 1)
            };
            let mut end = window_end(item.page);
            let Some((mut analysis, mut picked)) = analyze_window(item, end, &starts_of, &mut page_input, load) else { continue };
            // 欄見出しの項目（「会社その他」）の公告が頁の範囲の最後まで続いたら、次の項目（「会社決算公告」）の頁の範囲まで
            // 広げて選び直す（会社その他の公告は決算公告の欄の頁にも組まれる）。
            let titles: Vec<&str> = std::iter::once(item.title.as_str())
                .chain(starts_of.iter().filter(|(p, t)| (item.page..=end).contains(p) && *t != item.title).map(|(_, t)| t.as_str()))
                .collect();
            if end > item.page && pdf::field_runs_to_end(&titles, &analysis.articles) {
                // 次の項目の頁の範囲の終わりまで、上限で切らずに広げる。上限で切ると、範囲の最後の記事が決算公告になり、下の
                // 広げ直しも効かず、残りの頁の会社その他の公告（本紙号外第159号の p80 以降の合併公告）はどの項目にも入らない。
                let field_end = starts.iter().copied().find(|&p| p > end).unwrap_or(last_page).max(end);
                if let Some(wide) = analyze_window(item, field_end, &starts_of, &mut page_input, load) {
                    (analysis, picked) = wide;
                    end = field_end;
                }
            }
            // 選んだ記事が頁の範囲の最後の記事まで続き、項目の範囲（次の項目の開始頁まで）が上限で切った頁の範囲より長いなら、
            // 項目の範囲の終わりまで広げて選び直す（頁の多い省令の別表、号の最後の「会社決算公告」の欄）。上限で切ると、残りの
            // 頁の本文はどの項目にも入らない。小見出しだけの記事（本文が画像の「破産、免責、再生関係」の、文も数字も無い記事）は、
            // 残りの頁に本文が続くとは限らないので広げない（決算公告の貸借対照表は文が無くても数字がある）。
            // 号数を並べた項目の告示は、間に別の項目の告示を挟んで続く（本紙第1750号の「砂防法第二条の土地を指定する件（国土交通
            // 八四三、八四五～八四七、八五〇～八五二）」の八五〇～八五二は、八四八・八四九の後ろの頁）。広げた範囲でも最後の記事まで
            // 続くか、標題の号数の告示をまだ選び終えない（本紙第1788号の「…政府短期証券の発行条件等を告示（財務二五七～二六〇、
            // 二六二、二六三）」の二六二・二六三は二六一の後ろの頁）なら、選ぶ記事が増える間、次の項目の開始頁までさらに広げる。
            let has_body = |t: &str| t.chars().any(|c| c == '。' || c.is_ascii_digit() || ('０'..='９').contains(&c));
            loop {
                let span_end = starts.iter().copied().find(|&p| p > end).unwrap_or(last_page).max(end);
                let runs_on = picked.last().is_some_and(|&l| l + 1 == analysis.articles.len() && has_body(&analysis.articles[l].text));
                let unfound = pdf::lists_unfound_numbers(&item.title, &analysis.articles, &picked);
                // 号数の告示を探して広げるのは、項目の頁から上限の頁数まで。
                let unfound = unfound && span_end < item.page + MAX_SPAN;
                // 上限で切った範囲（終わりが項目の開始頁でも号の最終頁でもない）の最後の記事が、どの項目も選ばない記事のことがある
                // （号外第174号の「会社決算公告」は p151 の「資本金及び準備金の額の減少公告」で切れ、p152〜p160 の決算公告が
                // どの項目にも入らない）。選んだ記事に本文があれば広げてみて、選ぶ記事が増えるときだけ採る。
                let capped = end < last_page && !starts.contains(&end) && picked.last().is_some_and(|&l| has_body(&analysis.articles[l].text));
                if span_end <= end || !(runs_on || unfound || capped) {
                    break;
                }
                let Some(wide) = analyze_window(item, span_end, &starts_of, &mut page_input, load) else { break };
                let grew = wide.1.len() > picked.len();
                // 最初の 1 回は、最後の記事まで続くなら選ぶ記事が増えなくても広げる（#318）。
                if !grew && !(runs_on && !widened.contains(&n)) {
                    break;
                }
                (analysis, picked) = wide;
                end = span_end;
                if !widened.contains(&n) {
                    widened.push(n);
                }
                if !grew {
                    break;
                }
            }
            let mut from_ocr = false;
            if picked.is_empty() {
                if let Some(ocr) = ocr.as_mut() {
                    if let Some(found) = analyze_window_with_ocr(item, end, &starts_of, &mut page_input, load, &mut **ocr) {
                        (analysis, picked) = found;
                        from_ocr = true;
                    }
                }
            }
            let segments: Vec<String> = analysis.articles.iter().map(|a| a.text.clone()).collect();
            let (body, doc) = match picked.as_slice() {
                [] => {
                    // 記事を選べない項目（本文が画像の告示など）は本文を持たない。頁の範囲の記事は、ほかの項目の公告
                    // （「北海道開発局公示」「会社その他」の合併公告）か、どの項目にも選ばれていない別の公告の続きで、この項目の
                    // 本文ではない（選べない項目の頁の範囲の記事に、その項目の標題の字が見つかるのは 157 項目中 1 項目）。
                    stats.unmatched += 1;
                    item.amend_text = None;
                    item.amend_format = None;
                    item.amend_document = None;
                    item.amend_source = None;
                    continue;
                }
                [i] => (segments[*i].clone(), analysis.articles[*i].document.clone()),
                many => {
                    let body = many.iter().map(|&i| segments[i].as_str()).collect::<Vec<_>>().join("\n\n");
                    let docs: Vec<&pdf::Document> = many.iter().map(|&i| &analysis.articles[i].document).collect();
                    // 告示ごとの書式がそろわなければ unknown にし、下でつないだ本文から判じる。
                    let format = if docs.iter().all(|d| d.format == docs[0].format) { docs[0].format.clone() } else { "unknown".to_string() };
                    let doc = pdf::Document { format, blocks: docs.iter().flat_map(|d| d.blocks.iter().cloned()).collect() };
                    (body, doc)
                }
            };
            // 形式は選んだ記事だけで決める。項目のページ全体（analysis.text）で判じると、同じページの別の記事の
            // 新旧対照表につられて、告示の一覧などまで「新旧対照表」になる。
            let format = Some(doc.format.clone())
                .filter(|f| f != "unknown")
                .or_else(|| pdf::detect_format_of(&body))
                .unwrap_or_else(|| "unknown".to_string());
            *stats.by_format.entry(format.clone()).or_default() += 1;
            item.amend_document = serde_json::to_value(&doc).ok();
            item.amend_format = Some(format);
            item.amend_text = Some(body);
            item.amend_source = from_ocr.then(|| "ocr".to_string());
        }
        // 広げた頁の範囲の終わりの頁から始まる項目の本文は、見出しが無いと前の記事に続いて 1 つの記事になる（本紙号外
        // 第196号の「令和七年度における予算使用の状況」が、続く「令和八年度第一・四半期における予算使用の状況」の本文まで
        // 持つ）。後ろの項目の本文の最初の行が、広げた項目の本文の 2 行目以降の行にあれば、その行の手前で切る。
        for &n in &widened {
            let page = issue.items[n].page;
            let compact = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
            let others: String =
                issue.items.iter().enumerate().filter(|&(m, _)| m != n).filter_map(|(_, o)| o.amend_text.as_deref()).map(compact).collect();
            let heads: Vec<String> = issue
                .items
                .iter()
                .filter(|o| o.page > page)
                .filter_map(|o| o.amend_text.as_deref()?.lines().map(str::trim).find(|l| !l.is_empty()).map(str::to_string))
                .filter(|h| h.chars().count() >= 10)
                .collect();
            let item = &mut issue.items[n];
            if item.amend_format.as_deref() == Some("shinkyu") {
                continue;
            }
            let Some(body) = item.amend_text.as_deref() else { continue };
            let lines: Vec<&str> = body.lines().collect();
            let Some(cut) = (1..lines.len()).find(|&i| heads.iter().any(|h| lines[i].trim() == h)) else { continue };
            // 切る残りは、ほかの項目の本文にある字（30 字ずつの 8 割以上）でなければ切らない（後ろの項目の最初の行と
            // 同じ行が、この項目の本文にもある）。
            let rest: Vec<char> = compact(&lines[cut..].join("")).chars().collect();
            let chunks: Vec<String> = rest.chunks(30).filter(|c| c.len() == 30).map(|c| c.iter().collect()).collect();
            if chunks.is_empty() || chunks.iter().filter(|c| others.contains(c.as_str())).count() * 5 < chunks.len() * 4 {
                continue;
            }
            let body = lines[..cut].join("\n").trim_end().to_string();
            let doc = pdf::Document::from_text(&body);
            item.amend_format = Some(Some(doc.format.clone()).filter(|f| f != "unknown").or_else(|| pdf::detect_format_of(&body)).unwrap_or_else(|| "unknown".to_string()));
            item.amend_document = serde_json::to_value(&doc).ok();
            item.amend_text = Some(body);
        }
    }
    stats
}

/// 改正・廃止・制定系の標題か（`kanpo-poc --amend-only` の対象・更新フィードに載せる項目の判定）。
pub(crate) fn is_amend_title(title: &str) -> bool {
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
/// の各イベントに官報マッチングを書き戻し、`public/kanpo/{date}/index.json` と日付一覧
/// `public/kanpo/index.json` を生成する。
pub fn run_link(public: &Path) -> Result<()> {
    link(&PathBuf::from(".cache/kanpo"), public)
}

fn link(cache_dir: &Path, public: &Path) -> Result<()> {
    if !cache_dir.exists() {
        tracing::info!("no kanpo cache; skipping kanpo-link");
        return Ok(());
    }

    // 1) public/kanpo/{date}/index.json を出力。
    let mut by_date: Vec<(String, kanpo_client::KanpoDate)> = Vec::new();
    for f in std::fs::read_dir(cache_dir)? {
        let f = f?;
        if f.path().extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let kd: kanpo_client::KanpoDate = serde_json::from_slice(&std::fs::read(f.path())?)?;
        by_date.push((kd.date.clone(), kd));
    }
    by_date.sort_by(|a, b| a.0.cmp(&b.0));

    // 法令 id→タイトル (逆引き linked_laws の表示用)。
    let law_titles: std::collections::HashMap<String, String> = {
        let idx_path = public.join("laws").join("index.json");
        std::fs::read(&idx_path)
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
            .and_then(|v| v.get("laws").and_then(|l| l.as_array()).cloned())
            .map(|laws| {
                laws.iter()
                    .filter_map(|l| {
                        Some((
                            l.get("law_id")?.as_str()?.to_string(),
                            l.get("title")?.as_str()?.to_string(),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    // 標題から改正対象・根拠法令を引くための題名索引 (同名の法令は除く)。
    let title_index = kanpo_linker::LawTitles::new(law_titles.iter().map(|(id, t)| (t.clone(), id.clone())));

    // 官報PDF URL → 改正対象法令 id 群 (逆引き)。timeline マッチで埋める。
    let mut reverse: std::collections::HashMap<String, std::collections::BTreeSet<String>> =
        std::collections::HashMap::new();

    // 2) timeline.json に kanpo マッチングを反映しつつ逆引きを集める。
    let laws_dir = public.join("laws");
    let law_entries: Vec<std::fs::DirEntry> = if laws_dir.exists() {
        std::fs::read_dir(&laws_dir)?.filter_map(|e| e.ok()).collect()
    } else {
        Vec::new()
    };
    for entry in law_entries {
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
            // 逆引き用: この timeline が属する改正対象法令。
            let target_law_id = ev
                .get("target_law_id")
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
            // 逆引き用: 実際にマッチした官報記事の標題 (同一PDF/ページ上の無関係記事に
            // 巻き込まれないよう、記事タイトルで一意に紐づける)。
            let mut matched_item_title: Option<String> = None;
            if let Some((iconf, idate, item)) = item_match {
                // 項目一致は号レベルより強い信号。改め文・官報PDFリンクを付与する。
                if iconf > conf {
                    conf = iconf;
                    date = idate.to_string();
                    reasons = vec!["promulgation_date".into(), "item_title".into()];
                }
                matched_item_title = Some(item.title.clone());
                obj.insert("pdf_url".into(), json!(item.pdf_url));
                obj.insert("page".into(), json!(item.page));
                if let Some(t) = &item.amend_text {
                    obj.insert("amend_text".into(), json!(t));
                    // 構造化（条単位の行整列＋別表の2D表）を載せる。取得時に保存した
                    // amend_document（別表含む）があればそれを優先、無ければテキストから再構成。
                    // 配信は gzip なので amend_text との重複は圧縮される。
                    let doc = item
                        .amend_document
                        .clone()
                        .or_else(|| serde_json::to_value(pdf::Document::from_text(t)).ok());
                    if let Some(doc) = doc {
                        obj.insert("amend_document".into(), doc);
                    }
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
            // 逆引き: linked かつマッチ記事が特定できたら (日付, 記事タイトル) → 対象法令。
            if obj.get("linked").and_then(|v| v.as_bool()).unwrap_or(false) {
                if let (Some(t), Some(tlid)) = (matched_item_title.as_deref(), target_law_id.as_deref()) {
                    if !tlid.is_empty() && !date.is_empty() {
                        reverse
                            .entry(format!("{date}\u{1f}{t}"))
                            .or_default()
                            .insert(tlid.to_string());
                    }
                }
            }
            ev["kanpo"] = serde_json::Value::Object(obj);
        }

        let bytes = serde_json::to_vec_pretty(&tl)?;
        std::fs::write(&timeline_path, bytes)?;
    }

    // 3) public/kanpo/{date}/index.json を逆引き (linked_laws / basis_laws) 付きで出力。
    //    官報PDF → 改正対象法令へジャンプできるようにする。
    //    e-Gov の timeline は反映が数か月遅れて直近の官報にほぼ当たらないため、標題からも引く。
    //    linked_laws は先頭を「主たる改正対象」として feeds / 検索索引が使うので、timeline の一致を先に置き、
    //    告示の根拠法令は別の basis_laws に分ける。
    let law_ref = |id: &str, matched: &str| {
        json!({
            "law_id": id,
            "title": law_titles.get(id).cloned().unwrap_or_default(),
            "match": matched,
        })
    };
    let mut dates_index: Vec<serde_json::Value> = Vec::new();
    for (date, kd) in &by_date {
        let dir = public.join("kanpo").join(date);
        std::fs::create_dir_all(&dir)?;
        let mut issues_val = serde_json::to_value(&kd.issues)?;
        let (mut item_count, mut linked_count) = (0usize, 0usize);
        if let Some(issues) = issues_val.as_array_mut() {
            for issue in issues.iter_mut() {
                let Some(items) = issue.get_mut("items").and_then(|x| x.as_array_mut()) else {
                    continue;
                };
                for item in items.iter_mut() {
                    item_count += 1;
                    let item_title = item
                        .get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if item_title.is_empty() {
                        continue;
                    }
                    let mut linked: Vec<serde_json::Value> = Vec::new();
                    let mut seen = std::collections::BTreeSet::new();
                    for id in reverse.get(&format!("{date}\u{1f}{item_title}")).into_iter().flatten() {
                        if seen.insert(id.clone()) {
                            linked.push(law_ref(id, "timeline"));
                        }
                    }
                    let by_title = kanpo_linker::title_links(&item_title, &title_index);
                    for id in &by_title.amends {
                        if seen.insert(id.clone()) {
                            linked.push(law_ref(id, "title"));
                        }
                    }
                    let basis: Vec<serde_json::Value> = if linked.is_empty() {
                        by_title.basis.iter().map(|id| law_ref(id, "title")).collect()
                    } else {
                        Vec::new()
                    };
                    if !linked.is_empty() || !basis.is_empty() {
                        linked_count += 1;
                    }
                    if let Some(obj) = item.as_object_mut() {
                        if !linked.is_empty() {
                            obj.insert("linked_laws".into(), json!(linked));
                        }
                        if !basis.is_empty() {
                            obj.insert("basis_laws".into(), json!(basis));
                        }
                    }
                }
            }
        }
        write_json_pretty(
            &dir.join("index.json"),
            &json!({
                "date": date,
                "generated_at": Utc::now().to_rfc3339(),
                "issues": issues_val,
            }),
        )?;
        dates_index.push(json!({ "date": date, "items": item_count, "linked_items": linked_count }));
    }
    // 4) public/kanpo/index.json: 配信している日付の一覧 (新しい順)。wiki-plan はこれを見て日付ごとの
    //    index.json を取りに行く (休刊日を推測で叩かずに済む)。
    dates_index.reverse();
    write_json_pretty(
        &public.join("kanpo").join("index.json"),
        &json!({ "generated_at": Utc::now().to_rfc3339(), "dates": dates_index }),
    )?;

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
            amend_document: None,
            amend_source: None,
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
    fn link_resolves_targets_from_titles_and_writes_date_index() {
        let root = crate::wiki::temp_dir("kanpo-link");
        let (cache, public) = (root.join("cache"), root.join("public"));
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::create_dir_all(public.join("laws")).unwrap();
        std::fs::write(
            public.join("laws/index.json"),
            serde_json::to_vec(&json!({"laws": [
                {"law_id": "L-yushutsu", "title": "輸出貿易管理令"},
                {"law_id": "L-gaitame-rei", "title": "外国為替令"}
            ]}))
            .unwrap(),
        )
        .unwrap();
        let mut amend = item("輸出貿易管理令の一部を改正する政令（三二五）");
        amend.page = 2;
        let issue = KanpoIssue {
            issue_type: "extra".into(),
            issue_no: "第223号".into(),
            pdf_url: String::new(),
            sha256: None,
            promulgation_date: "2026-10-09".into(),
            law_nums: vec![],
            titles: vec![],
            items: vec![
                amend,
                item("外国為替令第七条の規定に基づき、財務大臣が指定する取引又は行為を指定する件の一部を改正する件（財務二六七）"),
                item("日本国に帰化を許可する件（法務省告示配一四九）"),
            ],
            page_count: None,
        };
        let kd = KanpoDate { date: "2026-10-09".into(), issues: vec![issue] };
        std::fs::write(cache.join("2026-10-09.json"), serde_json::to_vec(&kd).unwrap()).unwrap();

        link(&cache, &public).unwrap();

        let day: serde_json::Value =
            serde_json::from_slice(&std::fs::read(public.join("kanpo/2026-10-09/index.json")).unwrap()).unwrap();
        let items = &day["issues"][0]["items"];
        assert_eq!(items[0]["linked_laws"], json!([{"law_id": "L-yushutsu", "title": "輸出貿易管理令", "match": "title"}]));
        assert!(items[0].get("basis_laws").is_none());
        assert!(items[1].get("linked_laws").is_none(), "告示の改正は改正対象にしない");
        assert_eq!(items[1]["basis_laws"][0]["law_id"], "L-gaitame-rei");
        assert!(items[2].get("linked_laws").is_none() && items[2].get("basis_laws").is_none());

        let index: serde_json::Value =
            serde_json::from_slice(&std::fs::read(public.join("kanpo/index.json")).unwrap()).unwrap();
        assert_eq!(index["dates"], json!([{"date": "2026-10-09", "items": 3, "linked_items": 2}]));
        std::fs::remove_dir_all(root).ok();
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
                page_count: None,
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
