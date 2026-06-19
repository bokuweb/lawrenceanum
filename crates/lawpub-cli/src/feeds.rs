//! `lawpub build-feeds` — 法令改正・パブコメ・官報(改め文)の新着を 1 本の
//! 逆時系列ストリームに集約し `public/feeds/recent.json` と RSS `recent.xml` を書く。
//!
//! 規制変化アラート (T0: 認証ゼロ) の心臓部。RSS リーダーで購読でき、アプリ内
//! 「新着」ビューも同じ JSON を読む。各ソースは既に public/ に生成済みの成果物
//! (updates/*.json, pubcomment/index.json, kanpo/{date}/index.json) を読むだけ。

use anyhow::{Context, Result};
use chrono::{NaiveDate, Utc};
use serde::Serialize;
use std::path::Path;

const MAX_ITEMS: usize = 200;

#[derive(Debug, Clone, Serialize)]
pub struct FeedItem {
    /// "law" | "pubcomment" | "kanpo"
    pub kind: String,
    pub date: String,
    pub title: String,
    /// アプリ内ルート ("/laws/..", "/pubcomment/..") か外部 URL (官報PDF)。
    pub href: String,
    /// href がアプリ内ルートなら true (HashRouter)、外部 URL なら false。
    pub internal: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_id: Option<String>,
    /// 逆引き: 官報項目が改正する対象法令名 (kanpo の linked_laws 先頭)。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ministry: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Debug, Serialize)]
struct Feed {
    schema_version: u32,
    generated_at: String,
    count: usize,
    items: Vec<FeedItem>,
}

fn read_json(path: &Path) -> Option<serde_json::Value> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// 法令改正イベント: `public/updates/{date}.json` を新しい順に最大 `max_dates` 日分。
fn collect_law_items(public: &Path, max_dates: usize) -> Vec<FeedItem> {
    let dir = public.join("updates");
    let mut date_files: Vec<(String, std::path::PathBuf)> = match std::fs::read_dir(&dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter_map(|p| {
                let stem = p.file_stem()?.to_str()?.to_string();
                // `YYYY-MM-DD.json` のみ (latest.json は除外)。
                if NaiveDate::parse_from_str(&stem, "%Y-%m-%d").is_ok() {
                    Some((stem, p))
                } else {
                    None
                }
            })
            .collect(),
        Err(_) => return Vec::new(),
    };
    date_files.sort_by(|a, b| b.0.cmp(&a.0)); // 日付降順
    date_files.truncate(max_dates);

    let mut items = Vec::new();
    for (date, path) in &date_files {
        let Some(v) = read_json(path) else { continue };
        let Some(laws) = v.get("updated_laws").and_then(|x| x.as_array()) else {
            continue;
        };
        for l in laws {
            let law_id = l.get("law_id").and_then(|x| x.as_str()).unwrap_or("");
            let title = l.get("title").and_then(|x| x.as_str()).unwrap_or("");
            if law_id.is_empty() || title.is_empty() {
                continue;
            }
            let change = l.get("change_type").and_then(|x| x.as_str()).unwrap_or("");
            let summary = match change {
                "added" => "新規",
                "amended" => "改正",
                "repealed" => "廃止",
                other => other,
            };
            items.push(FeedItem {
                kind: "law".into(),
                date: date.clone(),
                title: title.to_string(),
                href: format!("/laws/{law_id}"),
                internal: true,
                law_id: Some(law_id.to_string()),
                law_title: None,
                ministry: None,
                summary: Some(summary.to_string()),
            });
        }
    }
    items
}

/// パブコメ: `public/pubcomment/index.json` の結果公示済み案件。
fn collect_pubcomment_items(public: &Path) -> Vec<FeedItem> {
    let Some(v) = read_json(&public.join("pubcomment").join("index.json")) else {
        return Vec::new();
    };
    let Some(cases) = v.get("cases").and_then(|x| x.as_array()) else {
        return Vec::new();
    };
    let mut items = Vec::new();
    for c in cases {
        let case_id = c.get("case_id").and_then(|x| x.as_str()).unwrap_or("");
        let title = c.get("title").and_then(|x| x.as_str()).unwrap_or("");
        if case_id.is_empty() || title.is_empty() {
            continue;
        }
        let date = c
            .get("result_published")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let ministry = c.get("ministry").and_then(|x| x.as_str()).map(String::from);
        let related = c.get("related_law_name").and_then(|x| x.as_str());
        items.push(FeedItem {
            kind: "pubcomment".into(),
            date,
            title: title.to_string(),
            href: format!("/pubcomment/{case_id}"),
            internal: true,
            law_id: None,
            law_title: None,
            ministry,
            summary: related.map(|r| format!("関連: {r}")),
        });
    }
    items
}

/// 官報の改め文記事: `public/kanpo/{date}/index.json` の amend_text を持つ項目。
fn collect_kanpo_items(public: &Path, max_dates: usize) -> Vec<FeedItem> {
    let dir = public.join("kanpo");
    let mut date_dirs: Vec<std::path::PathBuf> = match std::fs::read_dir(&dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect(),
        Err(_) => return Vec::new(),
    };
    date_dirs.sort();
    date_dirs.reverse(); // 新しい日付から
    date_dirs.truncate(max_dates);

    let mut items = Vec::new();
    for ddir in &date_dirs {
        let Some(v) = read_json(&ddir.join("index.json")) else {
            continue;
        };
        let date = v.get("date").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let Some(issues) = v.get("issues").and_then(|x| x.as_array()) else {
            continue;
        };
        for issue in issues {
            let Some(its) = issue.get("items").and_then(|x| x.as_array()) else {
                continue;
            };
            for it in its {
                // 改め文 (法令改正) を持つ項目だけ。公告・公示は対象外。
                let has_amend = it.get("amend_text").and_then(|x| x.as_str()).is_some_and(|s| !s.is_empty());
                if !has_amend {
                    continue;
                }
                let title = it.get("title").and_then(|x| x.as_str()).unwrap_or("");
                let pdf = it.get("pdf_url").and_then(|x| x.as_str()).unwrap_or("");
                if title.is_empty() || pdf.is_empty() {
                    continue;
                }
                let agency = it.get("agency_hint").and_then(|x| x.as_str()).map(String::from);
                // 逆引き: linked_laws の先頭 (改正対象法令)。
                let first_law = it.get("linked_laws").and_then(|x| x.as_array()).and_then(|a| a.first());
                let law_id = first_law.and_then(|l| l.get("law_id")).and_then(|x| x.as_str()).map(String::from);
                let law_title = first_law.and_then(|l| l.get("title")).and_then(|x| x.as_str()).map(String::from);
                items.push(FeedItem {
                    kind: "kanpo".into(),
                    date: date.clone(),
                    title: title.to_string(),
                    href: pdf.to_string(),
                    internal: false,
                    law_id,
                    law_title,
                    ministry: agency,
                    summary: Some("官報".into()),
                });
            }
        }
    }
    items
}

/// 「YYYY年M月D日…」→「YYYY-MM-DD」。既に ISO ならそのまま。パース不能は元のまま返す。
/// パブコメは和式日付 (例「2026年6月19日」「2026年3月27日18時0分」)、法令/官報は ISO。
fn normalize_date(s: &str) -> String {
    let s = s.trim();
    if NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok() {
        return s.to_string();
    }
    if let (Some(yi), Some(mi), Some(di)) = (s.find('年'), s.find('月'), s.find('日')) {
        let y: i32 = s[..yi].trim().parse().unwrap_or(0);
        let m: u32 = s[yi + '年'.len_utf8()..mi].trim().parse().unwrap_or(0);
        let d: u32 = s[mi + '月'.len_utf8()..di].trim().parse().unwrap_or(0);
        if y > 0 && m > 0 && d > 0 {
            return format!("{y:04}-{m:02}-{d:02}");
        }
    }
    s.to_string()
}

fn kind_label(k: &str) -> &str {
    match k {
        "law" => "法令",
        "pubcomment" => "パブコメ",
        "kanpo" => "官報",
        "bill" => "法案",
        _ => k,
    }
}

/// 法案審議の動き: `public/gian/index.json` の各法案を最新動向日で。
fn collect_gian_items(public: &Path) -> Vec<FeedItem> {
    let Some(v) = read_json(&public.join("gian").join("index.json")) else {
        return Vec::new();
    };
    let Some(bills) = v.get("bills").and_then(|x| x.as_array()) else {
        return Vec::new();
    };
    let mut items = Vec::new();
    for b in bills {
        let title = b.get("title").and_then(|x| x.as_str()).unwrap_or("");
        let date = b.get("latest_date").and_then(|x| x.as_str()).unwrap_or("");
        // 最新動向日が無い法案 (未審議等) はフィードに出さない。
        if title.is_empty() || date.is_empty() {
            continue;
        }
        let detail = b.get("detail_url").and_then(|x| x.as_str()).unwrap_or("");
        if detail.is_empty() {
            continue;
        }
        let bill_type = b.get("bill_type").and_then(|x| x.as_str());
        let committee = b.get("committee").and_then(|x| x.as_str());
        let event = b.get("latest_event").and_then(|x| x.as_str());
        // summary 例: 「衆法 · 委員会付託(衆) · 政治改革に関する特別」
        let summary = [bill_type, event, committee]
            .into_iter()
            .flatten()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        items.push(FeedItem {
            kind: "bill".into(),
            date: date.to_string(),
            title: title.to_string(),
            href: detail.to_string(),
            internal: false,
            law_id: None,
            law_title: None,
            ministry: None,
            summary: (!summary.is_empty()).then_some(summary),
        });
    }
    items
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// RFC822 風の pubDate。パース不能なら None。
fn rfc822(date: &str) -> Option<String> {
    let d = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    let dt = d.and_hms_opt(0, 0, 0)?;
    Some(dt.format("%a, %d %b %Y %H:%M:%S +0000").to_string())
}

fn write_rss(public: &Path, items: &[FeedItem]) -> Result<()> {
    let base = std::env::var("LAWPUB_BASE_URL").unwrap_or_else(|_| "/".to_string());
    let base_norm = if base.ends_with('/') { base.clone() } else { format!("{base}/") };

    let mut rss = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
<channel>
<title>Lawrenceanum 規制変化フィード</title>
<description>法令改正・パブリックコメント・官報(改め文)の新着</description>
"#,
    );
    rss.push_str(&format!("<link>{}</link>\n", xml_escape(&base_norm)));
    for it in items {
        let link = if it.internal {
            format!("{base_norm}#{}", it.href)
        } else {
            it.href.clone()
        };
        let title = format!("[{}] {}", kind_label(&it.kind), it.title);
        rss.push_str("<item>\n");
        rss.push_str(&format!("<title>{}</title>\n", xml_escape(&title)));
        rss.push_str(&format!("<link>{}</link>\n", xml_escape(&link)));
        rss.push_str(&format!("<guid isPermaLink=\"false\">{}</guid>\n", xml_escape(&format!("{}:{}:{}", it.kind, it.date, it.href))));
        if let Some(p) = rfc822(&it.date) {
            rss.push_str(&format!("<pubDate>{p}</pubDate>\n"));
        }
        if let Some(s) = &it.summary {
            rss.push_str(&format!("<description>{}</description>\n", xml_escape(s)));
        }
        rss.push_str("</item>\n");
    }
    rss.push_str("</channel>\n</rss>\n");
    std::fs::write(public.join("feeds").join("recent.xml"), rss.as_bytes())
        .context("write recent.xml")?;
    Ok(())
}

/// `lawpub build-feeds` の実装。public/ の既存成果物から横断フィードを生成する。
pub fn run_build_feeds(public: &Path) -> Result<()> {
    let out_dir = public.join("feeds");
    std::fs::create_dir_all(&out_dir)?;

    let mut items = Vec::new();
    items.extend(collect_law_items(public, 14));
    items.extend(collect_pubcomment_items(public));
    items.extend(collect_kanpo_items(public, 30));
    items.extend(collect_gian_items(public));

    // 日付を ISO に正規化 (パブコメの和式日付を揃え、横断ソートを正しくする)。
    for it in items.iter_mut() {
        it.date = normalize_date(&it.date);
    }

    // 日付降順。日付が同じなら kind 安定。
    items.sort_by(|a, b| b.date.cmp(&a.date).then(a.kind.cmp(&b.kind)));
    items.truncate(MAX_ITEMS);

    let feed = Feed {
        schema_version: 1,
        generated_at: Utc::now().to_rfc3339(),
        count: items.len(),
        items: items.clone(),
    };
    std::fs::write(
        out_dir.join("recent.json"),
        serde_json::to_string_pretty(&feed)?,
    )
    .context("write recent.json")?;

    write_rss(public, &items)?;

    tracing::info!("build-feeds: {} items (feeds/recent.json + recent.xml)", items.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_feed_from_public_artifacts() {
        let root = std::env::temp_dir().join("lawpub_feeds_test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("updates")).unwrap();
        std::fs::create_dir_all(root.join("pubcomment")).unwrap();
        std::fs::create_dir_all(root.join("kanpo").join("2026-04-02")).unwrap();

        std::fs::write(
            root.join("updates").join("2026-05-22.json"),
            serde_json::to_string(&serde_json::json!({
                "date": "2026-05-22",
                "updated_laws": [{"law_id": "X1", "title": "テスト法", "change_type": "amended"}]
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            root.join("pubcomment").join("index.json"),
            serde_json::to_string(&serde_json::json!({
                "schema_version": 1, "count": 1,
                "cases": [{"case_id": "c1", "title": "テストパブコメ", "ministry": "法務省", "result_published": "2026-06-01", "related_law_name": "民法"}]
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            root.join("kanpo").join("2026-04-02").join("index.json"),
            serde_json::to_string(&serde_json::json!({
                "date": "2026-04-02",
                "issues": [{"issue_no": "第1号", "items": [
                    {"title": "ある省令の一部改正", "pdf_url": "https://x/y.pdf", "agency_hint": "総務一", "amend_text": "改める。"},
                    {"title": "ただの公告", "pdf_url": "https://x/z.pdf"}
                ]}]
            }))
            .unwrap(),
        )
        .unwrap();

        run_build_feeds(&root).unwrap();

        let feed: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join("feeds").join("recent.json")).unwrap())
                .unwrap();
        let items = feed["items"].as_array().unwrap();
        // law + pubcomment + kanpo(改め文1件、公告は除外) = 3。
        assert_eq!(items.len(), 3);
        let kinds: Vec<&str> = items.iter().map(|i| i["kind"].as_str().unwrap()).collect();
        assert!(kinds.contains(&"law"));
        assert!(kinds.contains(&"pubcomment"));
        assert!(kinds.contains(&"kanpo"));
        // 日付降順: 先頭は 2026-06-01 (pubcomment)。
        assert_eq!(items[0]["date"].as_str().unwrap(), "2026-06-01");

        // RSS も生成される。
        let rss = std::fs::read_to_string(root.join("feeds").join("recent.xml")).unwrap();
        assert!(rss.contains("<rss version=\"2.0\">"));
        assert!(rss.contains("[パブコメ] テストパブコメ"));

        let _ = std::fs::remove_dir_all(&root);
    }
}
