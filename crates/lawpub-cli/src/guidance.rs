//! `lawpub guidance-fetch` / `guidance-build-json` — 所管省庁の政府解釈
//! (ノーアクションレター・Q&A・ガイドライン等)。取得とパースは `guidance-client` の省庁アダプタ。
//!
//! 添付原本は gian の議決 PDF と同じく SHA-256 で content-addressed に
//! `.cache/guidance-assets/{ministry}/{sha256}.{ext}` へ置き、本文抽出は pubcomment と共通の
//! [`crate::pubcomment::extract_attachment_text`] (pdftotext / DOCX / XLSX) を使う。
//! HTML は省庁サイトの文字コード (古い回答書は Shift_JIS) を見て本文枠だけ取る。

use anyhow::{Context, Result};
use guidance_client::{
    compose, decode_html, html_main_text, Attachment, DocLink, GuidanceDoc, GuidanceKind,
    GuidanceProvider, Listing, MockProvider,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn make_provider(
    provider: &str,
    ministry: &str,
    interval: Duration,
) -> Result<Box<dyn GuidanceProvider>> {
    if provider == "mock" {
        return Ok(Box::new(MockProvider));
    }
    match ministry {
        "fsa" => Ok(Box::new(guidance_client::fsa::HttpProvider::new(interval)?)),
        other => anyhow::bail!("unknown ministry {other:?} (fsa)"),
    }
}

fn parse_kinds(spec: Option<&str>) -> Result<Option<HashSet<GuidanceKind>>> {
    let Some(spec) = spec.filter(|s| !s.trim().is_empty()) else {
        return Ok(None);
    };
    spec.split(',')
        .map(|k| {
            GuidanceKind::parse(k.trim()).with_context(|| {
                format!("unknown kind {k:?} (no_action_letter|written_inquiry|consultation_cases|qa|guideline|interpretation_cases)")
            })
        })
        .collect::<Result<HashSet<_>>>()
        .map(Some)
}

fn cache_path(cache: &Path, ministry: &str, id: &str) -> PathBuf {
    cache
        .join("guidance")
        .join(ministry)
        .join(format!("{id}.json"))
}

fn extension_of(media_type: Option<&str>, url: &str, bytes: &[u8]) -> &'static str {
    let path = url
        .split(['?', '#'])
        .next()
        .unwrap_or(url)
        .to_ascii_lowercase();
    if bytes.starts_with(b"%PDF-") || media_type == Some("application/pdf") {
        "pdf"
    } else if media_type == Some("text/html") || path.ends_with(".html") || path.ends_with(".htm") {
        "html"
    } else if path.ends_with(".xlsx") {
        "xlsx"
    } else if path.ends_with(".docx") {
        "docx"
    } else {
        "bin"
    }
}

/// 古い回答書の「照会日・回答日」は変わらないので、ノーアクションレター等は一度取れば取り直さない。
/// Q&A・ガイドラインは同じ URL のまま改訂されるため `refresh_days` ごとに取り直す。
fn is_stale(prev: &GuidanceDoc, listing: &Listing, refresh_days: i64) -> bool {
    if prev.needs_refetch(listing) {
        return true;
    }
    if matches!(
        listing.kind,
        GuidanceKind::NoActionLetter | GuidanceKind::WrittenInquiry
    ) {
        return false;
    }
    chrono::DateTime::parse_from_rfc3339(&prev.fetched_at)
        .map(|t| chrono::Utc::now().signed_duration_since(t).num_days() >= refresh_days)
        .unwrap_or(true)
}

/// 1 ファイルを取得して原本保存・本文抽出する。前回と同じ内容 (SHA-256 一致) なら抽出結果を引き継ぐ。
/// HTML なら、本文として一緒に取るべきリンク (Q&A の PDF 版など) を返す。
fn hydrate(
    p: &dyn GuidanceProvider,
    cache: &Path,
    link: &DocLink,
    previous: Option<&Attachment>,
) -> (Attachment, Vec<DocLink>) {
    let ministry = p.ministry().slug;
    let mut att = Attachment::from_link(link);
    let fetched = match p.fetch(&link.url) {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!("guidance {}: {e:#}", link.url);
            att.extraction_error = Some(format!("fetch failed: {e:#}"));
            return (att, Vec::new());
        }
    };
    let media_type = fetched.media_type();
    let sha256 = hex::encode(Sha256::digest(&fetched.bytes));
    let ext = extension_of(media_type.as_deref(), &link.url, &fetched.bytes);
    let relative = format!("guidance-assets/{ministry}/{sha256}.{ext}");
    let raw = cache.join(&relative);
    if !raw.exists() {
        if let Err(e) = std::fs::create_dir_all(raw.parent().unwrap())
            .and_then(|_| std::fs::write(&raw, &fetched.bytes))
        {
            att.extraction_error = Some(format!("cache write failed: {e:#}"));
            return (att, Vec::new());
        }
    }
    att.media_type = media_type.clone();
    att.sha256 = Some(sha256.clone());
    att.bytes = Some(fetched.bytes.len() as u64);
    att.raw_path = Some(relative);
    att.fetched_at = Some(fetched.fetched_at.clone());

    let mut follow = Vec::new();
    if ext == "html" {
        let html = decode_html(&fetched.bytes, fetched.content_type.as_deref());
        follow = p.follow_links(&link.url, &html);
    }
    if let Some(old) = previous.filter(|o| o.sha256.as_deref() == Some(sha256.as_str())) {
        att.text = old.text.clone();
        att.extraction_method = old.extraction_method.clone();
        att.extraction_error = old.extraction_error.clone();
        return (att, follow);
    }
    let extracted = if ext == "html" {
        let html = decode_html(&fetched.bytes, fetched.content_type.as_deref());
        let text = html_main_text(&html);
        if text.is_empty() {
            Err(anyhow::anyhow!("HTML contains no text"))
        } else {
            Ok((text, "html-main-text".to_string()))
        }
    } else {
        let filename = link.url.rsplit('/').next();
        crate::pubcomment::extract_attachment_text(
            &raw,
            media_type.as_deref(),
            filename,
            &fetched.bytes,
        )
    };
    match extracted {
        Ok((text, method)) => {
            att.text = Some(text);
            att.extraction_method = Some(method);
        }
        Err(e) => att.extraction_error = Some(e.to_string()),
    }
    (att, follow)
}

/// `lawpub guidance-fetch` — 省庁の政府解釈一覧を巡回し、文書ごとに照会書・回答書・本文を取得して
/// `.cache/guidance/{ministry}/{id}.json` に保存する。取得済みで変化の無いものは飛ばすので、
/// `limit` で打ち切っても同じ引数で再実行すれば続きから取れる。
pub fn run_fetch(
    cache: &Path,
    provider: &str,
    ministry: &str,
    kinds: Option<&str>,
    interval_ms: u64,
    limit: usize,
    refresh_days: i64,
) -> Result<()> {
    let p = make_provider(provider, ministry, Duration::from_millis(interval_ms))?;
    let kinds = parse_kinds(kinds)?;
    let listings = p.list()?;
    let slug = p.ministry().slug;
    tracing::info!(
        "guidance-fetch: {slug}: {} documents listed",
        listings.len()
    );
    let (mut saved, mut skipped, mut failed) = (0usize, 0usize, 0usize);
    for listing in &listings {
        if kinds.as_ref().is_some_and(|k| !k.contains(&listing.kind)) {
            continue;
        }
        let path = cache_path(cache, slug, &listing.id);
        let prev: Option<GuidanceDoc> = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok());
        if let Some(prev) = prev
            .as_ref()
            .filter(|p| !is_stale(p, listing, refresh_days))
        {
            // 一覧側のメタ (表題・備考など) だけ変わった場合は取得せずに書き直す。
            let doc = compose(
                p.ministry(),
                listing,
                prev.attachments.clone(),
                &prev.fetched_at,
            );
            if &doc != prev {
                std::fs::write(&path, serde_json::to_string_pretty(&doc)?)?;
            }
            skipped += 1;
            continue;
        }
        if saved >= limit {
            tracing::info!("guidance-fetch: reached --limit {limit}");
            break;
        }
        let previous: HashMap<&str, &Attachment> = prev
            .as_ref()
            .map(|d| d.attachments.iter().map(|a| (a.url.as_str(), a)).collect())
            .unwrap_or_default();
        let mut queue: Vec<DocLink> = listing.links.clone();
        let mut seen: HashSet<String> = queue.iter().map(|l| l.url.clone()).collect();
        let mut attachments = Vec::new();
        let mut i = 0;
        while i < queue.len() {
            let link = queue[i].clone();
            i += 1;
            let (att, follow) = hydrate(
                p.as_ref(),
                cache,
                &link,
                previous.get(link.url.as_str()).copied(),
            );
            attachments.push(att);
            // 本文ページから辿るのは 1 段だけ。
            if i <= listing.links.len() {
                for f in follow {
                    if seen.insert(f.url.clone()) {
                        queue.push(f);
                    }
                }
            }
        }
        if attachments.iter().all(|a| a.sha256.is_none()) {
            failed += 1;
            continue;
        }
        let fetched_at = chrono::Utc::now().to_rfc3339();
        let doc = compose(p.ministry(), listing, attachments, &fetched_at);
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(&path, serde_json::to_string_pretty(&doc)?)
            .with_context(|| format!("write {}", path.display()))?;
        saved += 1;
    }
    tracing::info!("guidance-fetch: saved={saved} unchanged={skipped} failed={failed}");
    Ok(())
}

#[derive(Serialize)]
struct IndexEntry<'a> {
    id: &'a str,
    ministry: &'a str,
    ministry_name: &'a str,
    kind: GuidanceKind,
    title: &'a str,
    issued_on: Option<&'a str>,
    issuer: &'a str,
    related_laws: Vec<&'a str>,
    has_text: bool,
    source_url: &'a str,
}

/// `lawpub guidance-build-json` — `.cache/guidance/` → `public/guidance/{id}.json` + `index.json`。
/// `public/laws/index.json` があれば関連法令を法令 ID・条番号に解決して埋める。
pub fn run_build_json(cache: &Path, public: &Path) -> Result<()> {
    let src = cache.join("guidance");
    let out = public.join("guidance");
    std::fs::create_dir_all(&out)?;
    let resolver = match linking::LawRefResolver::from_index(public) {
        Ok(r) => Some(r),
        Err(e) => {
            tracing::warn!(
                "guidance-build-json: no law index, related laws left unresolved: {e:#}"
            );
            None
        }
    };
    let mut docs: Vec<GuidanceDoc> = Vec::new();
    if src.is_dir() {
        for entry in walkdir::WalkDir::new(&src) {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy();
            // exFAT/macOS の AppleDouble (._*.json) は飛ばす。
            if !entry.file_type().is_file() || !name.ends_with(".json") || name.starts_with("._") {
                continue;
            }
            match serde_json::from_slice::<GuidanceDoc>(&std::fs::read(entry.path())?) {
                Ok(d) => docs.push(d),
                Err(e) => {
                    tracing::warn!("guidance-build-json: skip {}: {e}", entry.path().display())
                }
            }
        }
    }
    docs.sort_by(|a, b| b.issued_on.cmp(&a.issued_on).then_with(|| a.id.cmp(&b.id)));
    for d in &mut docs {
        if let Some(r) = &resolver {
            for law in &mut d.related_laws {
                if let Some((id, arts)) = r.resolve(&law.name, law.provisions.as_deref()) {
                    law.law_id = Some(id);
                    law.articles = arts;
                }
            }
        }
        std::fs::write(
            out.join(format!("{}.json", d.id)),
            serde_json::to_string_pretty(&d.for_public())?,
        )?;
    }
    let entries: Vec<IndexEntry> = docs
        .iter()
        .map(|d| IndexEntry {
            id: &d.id,
            ministry: &d.ministry,
            ministry_name: &d.ministry_name,
            kind: d.kind,
            title: &d.title,
            issued_on: d.issued_on.as_deref(),
            issuer: &d.issuer,
            related_laws: d.related_laws.iter().map(|l| l.name.as_str()).collect(),
            has_text: d.body_text.is_some(),
            source_url: &d.source_url,
        })
        .collect();
    std::fs::write(
        out.join("index.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": 1,
            "sources": [
                "https://www.fsa.go.jp/common/noact/index.html",
                "https://www.fsa.go.jp/common/law/kokuji.html",
            ],
            "license": "金融庁ウェブサイト: 公共データ利用規約（第1.0版）(PDL1.0) https://www.fsa.go.jp/rules/index.html",
            "note": "本文は公表 PDF/HTML から機械的に抽出したもの（加工）。金融庁のパブリックコメント回答は pubcomment/ (e-Gov 結果公示) を参照。",
            "count": entries.len(),
            "documents": entries,
        }))?,
    )?;
    tracing::info!("guidance-build-json: {} documents", entries.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_spec() {
        assert!(parse_kinds(None).unwrap().is_none());
        let k = parse_kinds(Some("nal,qa")).unwrap().unwrap();
        assert!(k.contains(&GuidanceKind::NoActionLetter) && k.contains(&GuidanceKind::Qa));
        assert!(parse_kinds(Some("bogus")).is_err());
    }

    #[test]
    fn xlsx_consultation_cases_text() {
        let bytes = include_bytes!("../../guidance-client/tests/fixtures/fsa/soudan_ginkou.xlsx");
        let dir = std::env::temp_dir().join(format!("lawpub_guidance_xlsx_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ginkou.xlsx");
        std::fs::write(&path, bytes).unwrap();
        let (text, method) =
            crate::pubcomment::extract_attachment_text(&path, None, Some("ginkou.xlsx"), bytes)
                .unwrap();
        assert_eq!(method, "xlsx-sharedstrings");
        assert!(text.starts_with("照会内容\n回答内容"), "{text}");
        assert!(text.contains("・銀行法第10条第２項"));
        // ふりがな (rPh) は入れない。
        assert!(!text.contains("ショウカイ"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn fetch_build_and_link_with_mock() {
        let root = std::env::temp_dir().join(format!("lawpub_guidance_e2e_{}", std::process::id()));
        let (cache, public) = (root.join(".cache"), root.join("public"));
        std::fs::create_dir_all(public.join("laws")).unwrap();
        std::fs::write(
            public.join("laws").join("index.json"),
            serde_json::to_vec(&serde_json::json!({"laws": [
                {"law_id": "GINKO", "title": "銀行法"},
            ]}))
            .unwrap(),
        )
        .unwrap();

        run_fetch(&cache, "mock", "fsa", None, 0, 10, 7).unwrap();
        let cached: GuidanceDoc = serde_json::from_slice(
            &std::fs::read(cache_path(&cache, "fsa", "fsa-nal-ginkou-10")).unwrap(),
        )
        .unwrap();
        assert_eq!(cached.attachments.len(), 2);
        let raw = cached.attachments[1].raw_path.as_deref().unwrap();
        assert!(raw.starts_with("guidance-assets/fsa/") && raw.ends_with(".html"));
        assert!(cache.join(raw).exists());
        assert_eq!(cached.issuer, "金融庁監督局 銀行第一課長");

        // 2 回目は取り直さない (fetched_at が変わらない)。
        run_fetch(&cache, "mock", "fsa", None, 0, 10, 7).unwrap();
        let again: GuidanceDoc = serde_json::from_slice(
            &std::fs::read(cache_path(&cache, "fsa", "fsa-nal-ginkou-10")).unwrap(),
        )
        .unwrap();
        assert_eq!(again.fetched_at, cached.fetched_at);

        run_build_json(&cache, &public).unwrap();
        let built: serde_json::Value = serde_json::from_slice(
            &std::fs::read(public.join("guidance").join("fsa-nal-ginkou-10.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(built["related_laws"][0]["law_id"], "GINKO");
        assert_eq!(
            built["related_laws"][0]["articles"],
            serde_json::json!(["10", "12"])
        );
        assert!(built["attachments"][0].get("text").is_none());
        assert!(built["attachments"][0].get("raw_path").is_none());
        assert!(built["body_text"].as_str().unwrap().contains("【回答書】"));
        let index: serde_json::Value = serde_json::from_slice(
            &std::fs::read(public.join("guidance").join("index.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(index["count"], 1);

        linking::run_link_guidance(&public).unwrap();
        let link: serde_json::Value = serde_json::from_slice(
            &std::fs::read(
                public
                    .join("links")
                    .join("law-to-guidance")
                    .join("GINKO.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(link["linked_guidance"][0]["id"], "fsa-nal-ginkou-10");
        assert_eq!(link["linked_guidance"][0]["relation"], "subject");
        std::fs::remove_dir_all(&root).ok();
    }
}
