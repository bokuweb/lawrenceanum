//! `lawpub build-feeds` — 法令・法案・パブコメ・官報・会議録・調達の新着を 1 本の
//! 逆時系列ストリームに集約し `public/feeds/recent.json` と RSS `recent.xml` を書く。
//!
//! 規制変化アラート (T0: 認証ゼロ) の心臓部。RSS リーダーで購読でき、アプリ内
//! 「新着」ビューも同じ JSON を読む。各ソースは既に public/ に生成済みの成果物
//! (updates/*.json, pubcomment/index.json, kanpo/{date}/index.json) を読むだけ。

use anyhow::{Context, Result};
use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

const MAX_ITEMS_PER_KIND: usize = 50;

#[derive(Debug, Clone, Serialize)]
pub struct FeedItem {
    /// law / hanrei / bill / pubcomment / kanpo / proceedings / shingikai / procurement
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
    #[serde(skip_serializing_if = "Option::is_none")]
    topic: Option<WatchTopic>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct WatchTopic {
    id: String,
    title: String,
    description: String,
    keywords: Vec<String>,
}

fn watch_topics() -> Vec<WatchTopic> {
    serde_json::from_str(include_str!("../../../config/watch-topics.json"))
        .expect("valid bundled watch topics")
}

fn matches_topic(item: &FeedItem, topic: &WatchTopic) -> bool {
    [
        item.title.as_str(),
        item.summary.as_deref().unwrap_or(""),
        item.law_title.as_deref().unwrap_or(""),
    ]
    .iter()
    .any(|text| {
        topic
            .keywords
            .iter()
            .any(|word| text.contains(word.as_str()))
    })
}

/// 裁判所の一覧に収録された判例。日付は公表日・取得日ではなく裁判日。
fn collect_hanrei_items(public: &Path) -> Vec<FeedItem> {
    let Some(v) = read_json(&public.join("hanrei/index.json")) else {
        return Vec::new();
    };
    v.get("cases")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter_map(|case| {
            let id = case.get("case_id")?.as_u64().filter(|id| *id > 0)?;
            let name = case.get("case_name")?.as_str()?.trim();
            if name.is_empty() {
                return None;
            }
            let mut item = internal_item(
                "hanrei",
                case.get("judged_on")?.as_str()?,
                name.into(),
                format!("/hanrei/{id}"),
            );
            // holding は裁判所公表の判示事項。LLM の生成文と混ぜない。
            item.summary = case
                .get("holding")
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty())
                .map(String::from);
            Some(item)
        })
        .collect()
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

/// パブコメ: 募集開始または結果公示を新着イベントとして扱う。
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
        let status = c.get("status").and_then(|x| x.as_str()).unwrap_or("");
        let open = status == "open";
        // 締切は将来の予定であり、新着日には使わない。
        let date = c
            .get(if open {
                "reception_start"
            } else {
                "result_published"
            })
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let ministry = c.get("ministry").and_then(|x| x.as_str()).map(String::from);
        let related = c.get("related_law_name").and_then(|x| x.as_str());
        let summary = if open {
            let deadline = c
                .get("reception_end")
                .and_then(|x| x.as_str())
                .filter(|s| !s.is_empty());
            let base = deadline
                .map(|d| format!("意見募集中 · 締切: {d}"))
                .unwrap_or_else(|| "意見募集中".into());
            Some(
                related
                    .map(|r| format!("{base} · 関連: {r}"))
                    .unwrap_or(base),
            )
        } else {
            Some(
                related
                    .map(|r| format!("結果公示 · 関連: {r}"))
                    .unwrap_or_else(|| "結果公示".into()),
            )
        };
        items.push(FeedItem {
            kind: "pubcomment".into(),
            date,
            title: title.to_string(),
            href: format!("/pubcomment/{case_id}"),
            internal: true,
            law_id: None,
            law_title: None,
            ministry,
            summary,
        });
    }
    items
}

/// 官報の改め文記事: `public/kanpo/{date}/index.json` の改正・廃止・制定の標題で amend_text を持つ項目。
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
        let date = v
            .get("date")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let Some(issues) = v.get("issues").and_then(|x| x.as_array()) else {
            continue;
        };
        for issue in issues {
            let Some(its) = issue.get("items").and_then(|x| x.as_array()) else {
                continue;
            };
            for it in its {
                // 改め文 (法令改正) を持つ項目だけ。本文は全項目から取るので、標題で公告・公示などを除く。
                let title = it.get("title").and_then(|x| x.as_str()).unwrap_or("");
                let has_amend = it
                    .get("amend_text")
                    .and_then(|x| x.as_str())
                    .is_some_and(|s| !s.is_empty());
                if !has_amend || !crate::kanpo::is_amend_title(title) {
                    continue;
                }
                let pdf = it.get("pdf_url").and_then(|x| x.as_str()).unwrap_or("");
                if title.is_empty() || pdf.is_empty() {
                    continue;
                }
                let agency = it
                    .get("agency_hint")
                    .and_then(|x| x.as_str())
                    .map(String::from);
                // 逆引き: linked_laws の先頭 (改正対象法令)。
                let first_law = it
                    .get("linked_laws")
                    .and_then(|x| x.as_array())
                    .and_then(|a| a.first());
                let law_id = first_law
                    .and_then(|l| l.get("law_id"))
                    .and_then(|x| x.as_str())
                    .map(String::from);
                let law_title = first_law
                    .and_then(|l| l.get("title"))
                    .and_then(|x| x.as_str())
                    .map(String::from);
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

/// ISO 日時・「YYYY年M月D日…」→「YYYY-MM-DD」。パース不能は元のまま返す。
/// パブコメは和式日付 (例「2026年6月19日」「2026年3月27日18時0分」)、法令/官報は ISO。
fn normalize_date(s: &str) -> String {
    let s = s.trim();
    let digits: String = s
        .chars()
        .map(|c| match c {
            '０'..='９' => char::from_u32(c as u32 - '０' as u32 + '0' as u32).unwrap(),
            _ => c,
        })
        .collect();
    if let Some(date) = jlt_client::wareki_date_to_iso(&digits) {
        return date;
    }
    let s = digits.as_str();
    if let Some(date) = s
        .get(..10)
        .filter(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").is_ok())
    {
        return date.to_string();
    }
    if let (Some(yi), Some(mi), Some(di)) = (s.find('年'), s.find('月'), s.find('日')) {
        if yi >= mi || mi >= di {
            return s.to_string();
        }
        let y: i32 = s[..yi].trim().parse().unwrap_or(0);
        let m: u32 = s[yi + '年'.len_utf8()..mi].trim().parse().unwrap_or(0);
        let d: u32 = s[mi + '月'.len_utf8()..di].trim().parse().unwrap_or(0);
        if let Some(date) = NaiveDate::from_ymd_opt(y, m, d) {
            return date.format("%Y-%m-%d").to_string();
        }
    }
    s.to_string()
}

fn kind_label(k: &str) -> &str {
    match k {
        "law" => "法令",
        "hanrei" => "判例",
        "pubcomment" => "パブコメ",
        "kanpo" => "官報",
        "bill" => "法案",
        "proceedings" => "国会会議録",
        "shingikai" => "審議会",
        "procurement" => "政府調達",
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
        let bill_id = b.get("bill_id").and_then(|x| x.as_str()).unwrap_or("");
        let session = b
            .get("session")
            .and_then(|x| {
                x.as_str()
                    .map(String::from)
                    .or_else(|| x.as_u64().map(|n| n.to_string()))
            })
            .unwrap_or_default();
        if bill_id.is_empty() || session.is_empty() {
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
            href: format!("/gian/{session}/{bill_id}"),
            internal: true,
            law_id: None,
            law_title: None,
            ministry: None,
            summary: (!summary.is_empty()).then_some(summary),
        });
    }
    items
}

fn index_entries(public: &Path, source: &str, key: &str) -> Vec<serde_json::Value> {
    read_json(&public.join(source).join("index.json"))
        .and_then(|v| v.get(key).and_then(|v| v.as_array()).cloned())
        .unwrap_or_default()
}

fn text_field<'a>(value: &'a serde_json::Value, key: &str) -> &'a str {
    value.get(key).and_then(|v| v.as_str()).unwrap_or("")
}

fn internal_item(kind: &str, date: &str, title: String, href: String) -> FeedItem {
    FeedItem {
        kind: kind.into(),
        date: date.into(),
        title,
        href,
        internal: true,
        law_id: None,
        law_title: None,
        ministry: None,
        summary: None,
    }
}

fn collect_proceedings_items(public: &Path) -> Vec<FeedItem> {
    index_entries(public, "proceedings", "meetings")
        .iter()
        .filter_map(|meeting| {
            let id = text_field(meeting, "meeting_id");
            let house = match text_field(meeting, "house") {
                "shugiin" => "衆議院",
                "sangiin" => "参議院",
                other => other,
            };
            let committee = text_field(meeting, "committee");
            if id.is_empty() || house.is_empty() {
                return None;
            }
            let title = [house, committee, text_field(meeting, "issue")]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" · ");
            let mut item = internal_item(
                "proceedings",
                text_field(meeting, "date"),
                title,
                format!("/proceedings/{id}"),
            );
            item.summary = meeting["speech_count"]
                .as_u64()
                .map(|n| format!("{n}件の発言"));
            Some(item)
        })
        .collect()
}

fn collect_shingikai_items(public: &Path) -> Vec<FeedItem> {
    index_entries(public, "shingikai", "minutes")
        .iter()
        .filter_map(|meeting| {
            let id = text_field(meeting, "minutes_id");
            let ministry = text_field(meeting, "ministry");
            let title = text_field(meeting, "title");
            // 開催予定は新着とは別。開催済みの会議・資料を表示する。
            if id.is_empty()
                || ministry.is_empty()
                || title.is_empty()
                || text_field(meeting, "status") == "scheduled"
            {
                return None;
            }
            let mut item = internal_item(
                "shingikai",
                text_field(meeting, "date"),
                title.into(),
                format!("/shingikai/{ministry}/{id}"),
            );
            item.ministry = Some(
                match ministry {
                    "moj" => "法務省",
                    "cao" => "内閣府",
                    "mlit" => "国土交通省",
                    "mhlw" => "厚生労働省",
                    other => other,
                }
                .into(),
            );
            let content = if meeting["has_minutes"].as_bool() == Some(true) {
                "議事録あり"
            } else {
                "会議資料"
            };
            item.summary = Some(
                [text_field(meeting, "committee"), content]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" · "),
            );
            Some(item)
        })
        .collect()
}

fn collect_procurement_items(public: &Path) -> Vec<FeedItem> {
    index_entries(public, "procurement", "items")
        .iter()
        .filter_map(|notice| {
            let id = text_field(notice, "item_id");
            let title = text_field(notice, "title");
            if id.is_empty() || title.is_empty() {
                return None;
            }
            let mut item = internal_item(
                "procurement",
                text_field(notice, "publish_date"),
                title.into(),
                format!("/procurement/{id}"),
            );
            item.ministry = notice["organization"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(String::from);
            item.summary = notice["notice_type"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(String::from);
            Some(item)
        })
        .collect()
}

/// 各種別の最新50件を残す。大量の同種データで他の種別を押し出さない。
fn recent_items(mut items: Vec<FeedItem>, today: NaiveDate) -> Vec<FeedItem> {
    for item in &mut items {
        item.date = normalize_date(&item.date);
    }
    items.retain(|item| {
        NaiveDate::parse_from_str(&item.date, "%Y-%m-%d").is_ok_and(|date| date <= today)
    });
    items.sort_by(|a, b| {
        b.date
            .cmp(&a.date)
            .then(a.kind.cmp(&b.kind))
            .then(a.href.cmp(&b.href))
    });
    let mut seen = HashSet::new();
    let mut counts = HashMap::new();
    items.retain(|item| {
        if !seen.insert((item.kind.clone(), item.date.clone(), item.href.clone())) {
            return false;
        }
        let count = counts.entry(item.kind.clone()).or_insert(0);
        *count += 1;
        *count <= MAX_ITEMS_PER_KIND
    });
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

fn write_rss(path: &Path, items: &[FeedItem], title: &str, description: &str) -> Result<()> {
    let base = std::env::var("LAWPUB_BASE_URL").unwrap_or_else(|_| "/".to_string());
    let base_norm = if base.ends_with('/') {
        base.clone()
    } else {
        format!("{base}/")
    };

    let mut rss = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
<channel>
"#,
    );
    rss.push_str(&format!(
        "<title>{}</title>\n<description>{}</description>\n",
        xml_escape(title),
        xml_escape(description)
    ));
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
        rss.push_str(&format!(
            "<guid isPermaLink=\"false\">{}</guid>\n",
            xml_escape(&format!("{}:{}:{}", it.kind, it.date, it.href))
        ));
        if let Some(p) = rfc822(&it.date) {
            rss.push_str(&format!("<pubDate>{p}</pubDate>\n"));
        }
        if let Some(s) = &it.summary {
            rss.push_str(&format!("<description>{}</description>\n", xml_escape(s)));
        }
        rss.push_str("</item>\n");
    }
    rss.push_str("</channel>\n</rss>\n");
    std::fs::write(path, rss.as_bytes()).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

/// `lawpub build-feeds` の実装。public/ の既存成果物から横断フィードを生成する。
pub fn run_build_feeds(public: &Path) -> Result<()> {
    let out_dir = public.join("feeds");
    std::fs::create_dir_all(&out_dir)?;

    let mut items = Vec::new();
    items.extend(collect_law_items(public, 14));
    items.extend(collect_hanrei_items(public));
    items.extend(collect_pubcomment_items(public));
    items.extend(collect_kanpo_items(public, 30));
    items.extend(collect_gian_items(public));
    items.extend(collect_proceedings_items(public));
    items.extend(collect_shingikai_items(public));
    items.extend(collect_procurement_items(public));

    let today = Utc::now()
        .with_timezone(&chrono::FixedOffset::east_opt(9 * 3600).unwrap())
        .date_naive();
    let generated_at = Utc::now().to_rfc3339();
    let topics_dir = out_dir.join("topics");
    std::fs::create_dir_all(&topics_dir)?;
    for topic in watch_topics() {
        // 全体フィードの種別上限より先に選別する。大量の別テーマで候補を失わない。
        let selected = recent_items(
            items
                .iter()
                .filter(|item| matches_topic(item, &topic))
                .cloned()
                .collect(),
            today,
        );
        let feed = Feed {
            schema_version: 1,
            generated_at: generated_at.clone(),
            count: selected.len(),
            items: selected.clone(),
            topic: Some(topic.clone()),
        };
        std::fs::write(
            topics_dir.join(format!("{}.json", topic.id)),
            serde_json::to_vec_pretty(&feed)?,
        )?;
        write_rss(&topics_dir.join(format!("{}.xml", topic.id)), &selected, &format!("Lawrenceanum — {}", topic.title), &format!("{} キーワード一致による候補です。判例は裁判日順で、公表・取得順ではありません。各種別最新50件まで。", topic.description))?;
    }
    let items = recent_items(items, today);

    let feed = Feed {
        schema_version: 1,
        generated_at,
        count: items.len(),
        items: items.clone(),
        topic: None,
    };
    std::fs::write(
        out_dir.join("recent.json"),
        serde_json::to_string_pretty(&feed)?,
    )
    .context("write recent.json")?;

    write_rss(&out_dir.join("recent.xml"), &items, "Lawrenceanum 規制変化フィード", "法令改正・判例・法案・パブリックコメント・官報・国会会議録・審議会・政府調達。判例は裁判日順です。")?;

    tracing::info!(
        "build-feeds: {} items (feeds/recent.json + recent.xml)",
        items.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_latest_items_for_each_kind_without_future_or_invalid_dates() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
        let mut items: Vec<_> = (0..300)
            .map(|n| {
                internal_item(
                    "pubcomment",
                    "2026-10-20",
                    format!("意見募集{n}"),
                    format!("/pubcomment/{n:03}"),
                )
            })
            .collect();
        items.push(internal_item(
            "law",
            "2026-10-19",
            "改正法".into(),
            "/laws/L1".into(),
        ));
        items.push(internal_item(
            "law",
            "2026-10-19",
            "改正法".into(),
            "/laws/L1".into(),
        ));
        for date in ["", "不明", "2026-02-30", "2026年2月30日", "2026-10-21"] {
            items.push(internal_item(
                "law",
                date,
                "除外".into(),
                format!("/laws/{date}"),
            ));
        }
        let selected = recent_items(items, today);
        assert_eq!(selected.len(), MAX_ITEMS_PER_KIND + 1);
        assert_eq!(selected.last().unwrap().kind, "law");
        assert!(selected.windows(2).all(|pair| pair[0].date >= pair[1].date));
        assert_eq!(selected[0].href, "/pubcomment/000");
        assert_eq!(selected[MAX_ITEMS_PER_KIND - 1].href, "/pubcomment/049");
    }

    #[test]
    fn normalizes_japanese_dates_and_iso_timestamps() {
        assert_eq!(normalize_date("令和８年１０月２３日"), "2026-10-23");
        assert_eq!(normalize_date("平成25年8月1日"), "2013-08-01");
        assert_eq!(normalize_date("2026-10-16 23:59"), "2026-10-16");
        assert_eq!(normalize_date("2026-10-16T12:00:00+09:00"), "2026-10-16");
        assert_eq!(normalize_date("2026年3月7日18時0分"), "2026-03-07");
        assert_eq!(normalize_date("月年日"), "月年日");
    }

    #[test]
    fn topic_rules_are_safe_and_select_before_the_global_limit() {
        let topics = watch_topics();
        let mut ids = std::collections::HashSet::new();
        for topic in &topics {
            assert!(ids.insert(&topic.id));
            assert!(
                !topic.id.is_empty()
                    && topic
                        .id
                        .bytes()
                        .all(|c| c.is_ascii_lowercase() || c == b'-')
            );
            assert!(
                !topic.keywords.is_empty()
                    && topic.keywords.iter().all(|word| !word.trim().is_empty())
            );
        }
        let topic = &topics[0];
        let today = NaiveDate::from_ymd_opt(2026, 10, 24).unwrap();
        let mut items: Vec<_> = (0..60)
            .map(|n| {
                internal_item(
                    "hanrei",
                    "2026-10-23",
                    "別の分野".into(),
                    format!("/hanrei/{n}"),
                )
            })
            .collect();
        let mut matching = internal_item(
            "hanrei",
            "2026-10-22",
            "保護変更決定取消請求事件".into(),
            "/hanrei/97208".into(),
        );
        matching.summary = Some("生活扶助の基準に関する判示事項".into());
        items.push(matching);
        assert!(!recent_items(items.clone(), today)
            .iter()
            .any(|item| matches_topic(item, topic)));
        let selected = recent_items(
            items
                .into_iter()
                .filter(|item| matches_topic(item, topic))
                .collect(),
            today,
        );
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].href, "/hanrei/97208");
    }

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
                "cases": [
                    {"case_id": "c1", "title": "テストパブコメ", "ministry": "法務省", "result_published": "2026-06-01", "related_law_name": "民法"},
                    {"case_id": "c2", "title": "募集中のパブコメ", "status": "open", "reception_start": "2026-05-30 10:00", "reception_end": "2099-01-01 23:59"},
                    {"case_id": "c3", "title": "開始日不明", "status": "open", "reception_end": "2099-01-01"}
                ]
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
                    {"title": "ただの公告", "pdf_url": "https://x/z.pdf", "amend_text": "公告する。"}
                ]}]
            }))
            .unwrap(),
        )
        .unwrap();

        for (source, value) in [
            (
                "hanrei",
                serde_json::json!({"cases": [
                    {"case_id": 97208, "case_name": "保護変更決定取消請求事件", "judged_on": "令和8年5月28日", "holding": "生活保護法の生活扶助基準に関する判示事項"},
                    {"case_id": 97209, "case_name": "日付不明", "judged_on": "不明"},
                    {"case_id": 97210, "case_name": "未来の事件", "judged_on": "令和81年1月1日"},
                    {"case_id": 0, "case_name": "不正なID", "judged_on": "令和8年5月28日"}
                ]}),
            ),
            (
                "gian",
                serde_json::json!({"bills": [{"bill_id": "b1", "session": 221, "title": "テスト法案", "latest_date": "2026-05-29"}]}),
            ),
            (
                "proceedings",
                serde_json::json!({"meetings": [{"meeting_id": "m1", "house": "shugiin", "committee": "法務委員会", "issue": "第1号", "date": "2026-05-28", "speech_count": 12}]}),
            ),
            (
                "shingikai",
                serde_json::json!({"minutes": [
                    {"minutes_id": "s1", "ministry": "moj", "title": "法制審議会", "committee": "民法部会", "date": "2026-05-27", "has_minutes": true},
                    {"minutes_id": "s2", "ministry": "moj", "title": "開催予定", "date": "2026-05-29", "status": "scheduled"},
                    {"minutes_id": "s3", "ministry": "moj", "title": "将来の会議", "date": "2099-05-29"}
                ]}),
            ),
            (
                "procurement",
                serde_json::json!({"items": [{"item_id": "p1", "title": "情報システム調達", "publish_date": "2026-05-26", "organization": "法務省", "notice_type": "役務"}]}),
            ),
        ] {
            std::fs::create_dir_all(root.join(source)).unwrap();
            std::fs::write(
                root.join(source).join("index.json"),
                serde_json::to_vec(&value).unwrap(),
            )
            .unwrap();
        }

        run_build_feeds(&root).unwrap();

        let feed: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join("feeds").join("recent.json")).unwrap())
                .unwrap();
        let items = feed["items"].as_array().unwrap();
        // 8種類 + 募集開始1件。予定・日付不明・本文のある公告も除外。
        assert_eq!(items.len(), 9);
        let kinds: Vec<&str> = items.iter().map(|i| i["kind"].as_str().unwrap()).collect();
        assert!(kinds.contains(&"law"));
        assert!(kinds.contains(&"pubcomment"));
        assert!(kinds.contains(&"kanpo"));
        assert!(kinds.contains(&"bill"));
        assert!(kinds.contains(&"proceedings"));
        assert!(kinds.contains(&"shingikai"));
        assert!(kinds.contains(&"procurement"));
        assert!(kinds.contains(&"hanrei"));
        // 日付降順: 先頭は 2026-06-01 (pubcomment)。
        assert_eq!(items[0]["date"].as_str().unwrap(), "2026-06-01");
        assert_eq!(items[1]["date"], "2026-05-30");
        assert!(items[1]["summary"]
            .as_str()
            .unwrap()
            .contains("締切: 2099-01-01 23:59"));
        for href in [
            "/proceedings/m1",
            "/shingikai/moj/s1",
            "/procurement/p1",
            "/gian/221/b1",
        ] {
            assert!(items
                .iter()
                .any(|item| item["href"] == href && item["internal"] == true));
        }

        // RSS も生成される。
        let rss = std::fs::read_to_string(root.join("feeds").join("recent.xml")).unwrap();
        assert!(rss.contains("<rss version=\"2.0\">"));
        assert!(rss.contains("[パブコメ] テストパブコメ"));
        assert!(rss.contains("[審議会] 法制審議会"));
        assert!(rss.contains("[政府調達] 情報システム調達"));
        assert!(rss.contains("[判例] 保護変更決定取消請求事件"));
        let topic_feed: serde_json::Value = serde_json::from_slice(
            &std::fs::read(root.join("feeds/topics/administrative-welfare.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(topic_feed["topic"]["id"], "administrative-welfare");
        assert_eq!(topic_feed["count"], 1);
        assert_eq!(topic_feed["items"][0]["href"], "/hanrei/97208");
        assert_eq!(topic_feed["items"][0]["date"], "2026-05-28");
        let topic_rss =
            std::fs::read_to_string(root.join("feeds/topics/administrative-welfare.xml")).unwrap();
        assert!(topic_rss.contains("生活保護・行政救済"));
        assert!(topic_rss.contains("#/hanrei/97208"));
        assert!(topic_rss.contains("公表・取得順ではありません"));

        let _ = std::fs::remove_dir_all(&root);
    }
}
